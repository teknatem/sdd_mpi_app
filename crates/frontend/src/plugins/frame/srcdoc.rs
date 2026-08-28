const IFRAME_BOOTSTRAP: &str = r#"
const root = document.getElementById("plugin-root");
const pending = new Map();
let currentModule = null;
let currentUrl = null;
let hostContext = {};

// Киты грузятся по объявлению плагина, а не безусловно: см. frame/kits.rs.
// Промис на имя, а не флаг, — тогда параллельные вызовы ждут одну загрузку,
// а не запускают вторую.
const kitLoads = new Map();

function loadAsset(tag, attrs) {
  return new Promise((resolve, reject) => {
    const el = document.createElement(tag);
    Object.assign(el, attrs);
    el.onload = () => resolve();
    el.onerror = () => reject(new Error("kit asset failed to load: " + (attrs.src || attrs.href)));
    document.head.append(el);
  });
}

function loadKit(name) {
  let started = kitLoads.get(name);
  if (started) return started;
  started = (async () => {
    const kit = KIT_ASSETS[name];
    if (!kit) throw new Error("unknown client kit: " + name);
    // CSS не блокирует исполнение, JS внутри кита — строго по порядку
    // (chart.umd ставит window.Chart до обёртки, которая её читает).
    const css = (kit.css || []).map((href) => loadAsset("link", { rel: "stylesheet", href }));
    for (const src of kit.js || []) {
      await loadAsset("script", { src, async: false });
    }
    await Promise.all(css);
  })();
  // Провал не кешируем: Restart должен получить честную повторную попытку.
  started.catch(() => kitLoads.delete(name));
  kitLoads.set(name, started);
  return started;
}

function loadKits(names) {
  return Promise.all((names || []).map(loadKit));
}

// plugin_init приходит дважды на холодном старте: хост зовёт его и по on:load
// iframe, и по нашему plugin_ready. Раньше это переживалось молча, но между
// проверкой и присваиванием currentModule стоит await — то есть два прохода
// монтировали модуль наперегонки за #plugin-root. Считаем поколение и даём
// устаревшему проходу бросить работу после каждого await.
let initGeneration = 0;

function emit(level, message) {
  window.parent.postMessage({ type: "plugin_event", instanceId: INSTANCE_ID, secret: BRIDGE_SECRET, level, message }, "*");
}

function makeRequestId() {
  if (typeof crypto.randomUUID === "function") return crypto.randomUUID();
  const bytes = crypto.getRandomValues(new Uint8Array(16));
  bytes[6] = (bytes[6] & 0x0f) | 0x40;
  bytes[8] = (bytes[8] & 0x3f) | 0x80;
  const hex = [...bytes].map((b) => b.toString(16).padStart(2, "0")).join("");
  return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`;
}

// Запрос с ответом: родитель отвечает по requestId. Одна машинерия на invoke
// и на документ — промисы и так лежали в общей карте pending.
function request(type, payload) {
  const requestId = makeRequestId();
  window.parent.postMessage(
    Object.assign({ type, instanceId: INSTANCE_ID, secret: BRIDGE_SECRET, requestId }, payload),
    "*"
  );
  return new Promise((resolve, reject) => {
    pending.set(requestId, { resolve, reject });
  });
}

const host = Object.freeze({
  get context() { return hostContext; },
  invoke(method, args = {}) {
    return request("plugin_invoke", { method, args });
  },
  // Читает поле, адрес которого задал хост. Плагин цель не выбирает — её нет
  // в аргументах: право на чтение и запись держит родительский фрейм.
  loadDocument() {
    return request("plugin_document", { op: "load" });
  },
  // Возвращает { version } — её же надо передать следующим expectedVersion.
  // Ошибка со стороны родителя означает конфликт версий или запрет записи;
  // повторять вызов вслепую нельзя, это затрёт чужие правки.
  saveDocument(content, options = {}) {
    return request("plugin_document", {
      op: "save",
      content,
      expectedVersion: options.expectedVersion === undefined ? null : options.expectedVersion
    });
  },
  // Сообщить хосту о несохранённых правках: он предупредит перед Restart и
  // сменой режима данных. Закрытие вкладки браузера так не перехватывается.
  setDirty(dirty) {
    window.parent.postMessage({
      type: "plugin_dirty",
      instanceId: INSTANCE_ID,
      secret: BRIDGE_SECRET,
      dirty: !!dirty
    }, "*");
  },
  openTab(key, title = key) {
    window.parent.postMessage({
      type: "plugin_open_tab",
      instanceId: INSTANCE_ID,
      secret: BRIDGE_SECRET,
      key: String(key),
      title: String(title)
    }, "*");
  }
});

function showError(error) {
  root.replaceChildren();
  const box = document.createElement("pre");
  box.className = "bootstrap-error";
  box.textContent = error instanceof Error ? `${error.message}\n${error.stack || ""}` : String(error);
  root.append(box);
}

function applyTheme(message) {
  // THEME_INFO инжектится хостом из реестра тем (shared::theme::registry).
  const themeName = THEME_INFO[message.themeName] ? message.themeName : "dark";
  const info = THEME_INFO[themeName];
  for (const el of [document.documentElement, document.body]) {
    el.dataset.theme = themeName;
    el.dataset.themeKind = info.kind;
    el.dataset.themeBase = info.base;
  }
  // Тема приложения и плагина — один источник: подменяем href темы, как делает index.html.
  const link = document.getElementById("plugin-theme");
  if (link) {
    const href = "/static/themes/" + themeName + "/" + themeName + ".css";
    if (link.getAttribute("href") !== href) link.setAttribute("href", href);
  }
  // Графики (PluginCharts) и таблицы (PluginTables) перечитывают цвета темы вслед за приложением.
  if (window.PluginCharts || window.PluginTables) {
    // Дать <link> темы примениться, затем перекрасить живые виджеты.
    setTimeout(() => {
      try { if (window.PluginCharts) window.PluginCharts.applyTheme(); } catch (e) {}
      try { if (window.PluginTables) window.PluginTables.applyTheme(); } catch (e) {}
    }, 60);
  }
}

window.addEventListener("message", async event => {
  const message = event.data || {};
  if (message.instanceId !== INSTANCE_ID || message.secret !== BRIDGE_SECRET) return;

  if (message.type === "plugin_invoke_result" || message.type === "plugin_document_result") {
    const waiter = pending.get(message.requestId);
    if (!waiter) return;
    pending.delete(message.requestId);
    if (message.ok) waiter.resolve(message.result);
    else waiter.reject(new Error(message.error || "Plugin server call failed"));
    return;
  }

  if (message.type === "plugin_theme") {
    applyTheme(message);
    return;
  }

  if (message.type !== "plugin_init") return;
  const generation = ++initGeneration;
  const stale = () => generation !== initGeneration;
  try {
    if (currentModule && typeof currentModule.unmount === "function") {
      await currentModule.unmount();
    }
    if (stale()) return;
    if (currentUrl) URL.revokeObjectURL(currentUrl);
    currentUrl = null;
    currentModule = null;

    hostContext = message.context || {};
    applyTheme(message);

    await loadKits(message.kits);
    if (stale()) return;

    document.getElementById("plugin-styles").textContent = message.styles || "";
    root.replaceChildren();
    emit("info", "init received, mounting");

    // Локальные url/module: устаревший проход не должен затирать состояние,
    // которое уже принадлежит более свежему.
    const blob = new Blob([message.clientScript || ""], { type: "text/javascript" });
    const url = URL.createObjectURL(blob);
    const module = await import(url);
    if (stale()) {
      URL.revokeObjectURL(url);
      return;
    }
    currentUrl = url;
    currentModule = module;

    if (typeof module.mount !== "function") {
      throw new Error("client_script must export async function mount(root, host)");
    }
    await module.mount(root, host);
    if (stale()) return;
    emit("info", "mount() complete");
  } catch (error) {
    if (stale()) return;
    showError(error);
    emit("error", error instanceof Error ? error.message : String(error));
  }
});

window.parent.postMessage({ type: "plugin_ready", instanceId: INSTANCE_ID, secret: BRIDGE_SECRET }, "*");
"#;

pub(super) fn build_srcdoc(
    instance_id: &str,
    bridge_secret: &str,
    theme: &crate::shared::theme::registry::ThemeDef,
) -> String {
    let instance_json = serde_json::to_string(instance_id).unwrap_or_else(|_| "\"plugin\"".into());
    let secret_json = serde_json::to_string(bridge_secret).unwrap_or_else(|_| "\"secret\"".into());
    let theme_attr = theme.id;
    let theme_kind = theme.kind.as_str();
    let theme_base = theme.base.as_str();
    let themes_json = crate::shared::theme::registry::themes_json();
    let kit_assets_json = super::kits::kit_assets_json();
    // Ранний фон до загрузки <link> темы — чтобы не было белой вспышки в тёмной теме.
    let bg_fallback = theme.base.iframe_bg_fallback();
    format!(
        r#"<!doctype html>
<html data-theme="{theme_attr}" data-theme-kind="{theme_kind}" data-theme-base="{theme_base}">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <style>
    html, body, #plugin-root {{ min-height: 100%; }}
    html, body {{ margin: 0; background: {bg_fallback}; }}
    .bootstrap-error {{
      margin: 16px;
      padding: 14px;
      white-space: pre-wrap;
      color: var(--badge-error-text, var(--color-error));
      background: var(--badge-error-bg, color-mix(in srgb, var(--color-error) 16%, transparent));
      border: 1px solid var(--badge-error-border, color-mix(in srgb, var(--color-error) 30%, transparent));
      border-radius: 8px;
    }}
  </style>
  <!-- Единый источник стилей: те же токены/темы, что и у приложения, + снапшот компонентов. -->
  <link rel="stylesheet" href="/static/themes/core/variables.css">
  <link id="plugin-theme" rel="stylesheet" href="/static/themes/{theme_attr}/{theme_attr}.css">
  <!-- Строгий гейт: обязан идти ПОСЛЕ файла темы (см. strict-guard.css) -->
  <link rel="stylesheet" href="/static/themes/core/strict-guard.css">
  <link rel="stylesheet" href="/static/plugin-sdk.css">
  <!-- Киты (Chart.js, таблицы, flow) здесь НЕ перечислены: их грузит loadKits()
       по списку из plugin_init, до вызова mount(). Список приезжает вместе с
       клиентским скриптом, поэтому srcdoc собирается без знания манифеста и не
       требует второй перезагрузки документа. Реестр ассетов — frame/kits.rs. -->
  <style id="plugin-styles"></style>
</head>
<body data-theme="{theme_attr}" data-theme-kind="{theme_kind}" data-theme-base="{theme_base}">
  <div id="plugin-root"></div>
  <script type="module">
    const INSTANCE_ID = {instance_json};
    const BRIDGE_SECRET = {secret_json};
    const THEME_INFO = {themes_json};
    const KIT_ASSETS = {kit_assets_json};
    {IFRAME_BOOTSTRAP}
  </script>
</body>
</html>"#
    )
}
