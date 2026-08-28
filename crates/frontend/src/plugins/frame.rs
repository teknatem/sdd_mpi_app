//! Reusable iframe runtime for client-side plugin UI.

mod bridge;
pub mod kits;
mod srcdoc;
mod theme;

use self::bridge::{event_source_matches_iframe, post_json, string_property, MessageListenerGuard};
use self::srcdoc::build_srcdoc;
use self::theme::current_theme;
use crate::plugins::api;
use contracts::plugins::{
    PluginDataMode, PluginDocumentBinding, PluginDocumentSaveRequest, PluginInvokeRequest,
    PluginRunContext,
};
use leptos::prelude::*;
use leptos::task::spawn_local;
use serde_json::json;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::JsCast;
use web_sys::HtmlIFrameElement;

/// Потолок на записываемый документ. В духе `MAX_SCRIPT_BYTES` у бандла:
/// отсечь очевидную аварию до похода на сервер, а не выражать бизнес-лимит.
const MAX_DOCUMENT_BYTES: usize = 2 * 1024 * 1024;

fn now_hms() -> String {
    let date = js_sys::Date::new_0();
    format!(
        "{:02}:{:02}:{:02}",
        date.get_hours() as u32,
        date.get_minutes() as u32,
        date.get_seconds() as u32
    )
}

#[component]
pub fn PluginFrame(
    plugin_id: String,
    client_src: RwSignal<String>,
    styles_src: RwSignal<String>,
    /// Клиентские киты плагина (`manifest.client_kits`, уже с разрешённым
    /// легаси-набором). Едут в `plugin_init`, а не в srcdoc: документ строится
    /// синхронно, до того как определение плагина приедет с сервера.
    kits: RwSignal<Vec<String>>,
    /// Куда плагин пишет редактируемый документ.
    ///
    /// Держится отдельным сигналом, а не полем [`PluginRunContext`], намеренно:
    /// контекст уходит на сервер при каждом invoke и правится руками на
    /// дев-странице. Цель записи не должна зависеть ни от того, ни от другого.
    #[prop(optional)]
    document: Option<RwSignal<Option<PluginDocumentBinding>>>,
    /// Есть ли у плагина несохранённые правки (плагин сообщает `host.setDirty`).
    #[prop(optional)]
    dirty: Option<RwSignal<bool>>,
    context: RwSignal<PluginRunContext>,
    data_mode: RwSignal<PluginDataMode>,
    restart: RwSignal<u64>,
    console: RwSignal<Vec<String>>,
    events: RwSignal<Vec<String>>,
    #[prop(optional)] dev: bool,
    /// Без внутреннего тулбара (кнопка Restart) — когда управление вынесено в заголовок
    /// страницы (см. [`crate::plugins::PluginView`]). iframe заполняет весь фрейм.
    #[prop(optional)]
    frameless: bool,
) -> impl IntoView {
    let has_client = Signal::derive(move || !client_src.get().trim().is_empty());
    let iframe_element = StoredValue::new_local(None::<HtmlIFrameElement>);
    let instance_id = uuid::Uuid::new_v4().to_string();
    let bridge_secret = uuid::Uuid::new_v4().to_string();
    let tabs_store = use_context::<crate::layout::global_context::AppGlobalContext>()
        .expect("AppGlobalContext must be provided");

    let log = Callback::new(move |message: String| {
        if !dev {
            return;
        }
        events.update(|lines| {
            lines.push(format!("[{}] {}", now_hms(), message));
            if lines.len() > 200 {
                lines.remove(0);
            }
        });
    });

    let do_init = {
        let instance_id = instance_id.clone();
        let bridge_secret = bridge_secret.clone();
        Callback::new(move |_: ()| {
            let Some(iframe) = iframe_element.get_value() else {
                log.run("iframe is not ready yet".to_string());
                return;
            };
            if client_src.get_untracked().trim().is_empty() {
                log.run("client_script is empty; nothing to mount".to_string());
                return;
            }
            let mut context_value =
                serde_json::to_value(context.get_untracked()).unwrap_or_else(|_| json!({}));
            if let Some(object) = context_value.as_object_mut() {
                let params = object.entry("params").or_insert_with(|| json!({}));
                if let Some(params) = params.as_object_mut() {
                    params.insert(
                        "_plugin_data_mode".to_string(),
                        json!(match data_mode.get_untracked() {
                            PluginDataMode::Live => "live",
                            PluginDataMode::Snapshot => "snapshot",
                        }),
                    );
                }
                // Проекция цели — чтобы плагин мог показать, что редактирует.
                // Видимость не есть право: авторитетная копия остаётся в
                // сигнале `document`, и запись идёт по ней.
                if let Some(binding) = document.and_then(|signal| signal.get_untracked()) {
                    object.insert(
                        "_plugin_document".to_string(),
                        serde_json::to_value(&binding).unwrap_or_else(|_| json!(null)),
                    );
                }
            }
            post_json(
                &iframe,
                json!({
                    "type": "plugin_init",
                    "instanceId": instance_id,
                    "secret": bridge_secret,
                    "clientScript": client_src.get_untracked(),
                    "styles": styles_src.get_untracked(),
                    "kits": kits.get_untracked(),
                    "themeName": current_theme().id,
                    "context": context_value,
                }),
            );
            log.run("init sent".to_string());
        })
    };

    let _message_listener = {
        let plugin_id = plugin_id.clone();
        let listener_instance = instance_id.clone();
        let listener_secret = bridge_secret.clone();
        StoredValue::new_local(web_sys::window().map(|window| {
            let handler = Closure::wrap(Box::new(move |event: web_sys::MessageEvent| {
                let data = event.data();
                let Some(message_type) = string_property(&data, "type") else {
                    return;
                };
                let Some(instance) = string_property(&data, "instanceId") else {
                    return;
                };
                let Some(secret) = string_property(&data, "secret") else {
                    return;
                };
                if instance != listener_instance || secret != listener_secret {
                    return;
                }
                if let Some(iframe) = iframe_element.get_value() {
                    if !event_source_matches_iframe(&event, &iframe) {
                        return;
                    }
                }

                match message_type.as_str() {
                    "plugin_event" => {
                        let level = string_property(&data, "level").unwrap_or_default();
                        let message = string_property(&data, "message").unwrap_or_default();
                        let prefix = if level == "error" {
                            "iframe error"
                        } else {
                            "iframe"
                        };
                        log.run(format!("{prefix}: {message}"));
                    }
                    "plugin_ready" => {
                        log.run("iframe ready".to_string());
                        do_init.run(());
                    }
                    "plugin_invoke" => handle_plugin_invoke(
                        &data,
                        &plugin_id,
                        &instance,
                        &secret,
                        iframe_element,
                        context,
                        data_mode,
                        console,
                        log,
                        dev,
                    ),
                    "plugin_document" => handle_plugin_document(
                        &data,
                        &plugin_id,
                        &instance,
                        &secret,
                        iframe_element,
                        document,
                        data_mode,
                        log,
                    ),
                    "plugin_dirty" => {
                        let is_dirty =
                            js_sys::Reflect::get(&data, &wasm_bindgen::JsValue::from_str("dirty"))
                                .ok()
                                .and_then(|value| value.as_bool())
                                .unwrap_or(false);
                        if let Some(signal) = dirty {
                            signal.set(is_dirty);
                        }
                    }
                    "plugin_open_tab" => {
                        let key = string_property(&data, "key").unwrap_or_default();
                        let title = string_property(&data, "title").unwrap_or_else(|| key.clone());
                        if is_navigable_from_plugin(&key) {
                            tabs_store.open_tab(&key, &title);
                        } else {
                            log.run("blocked plugin tab navigation".to_string());
                        }
                    }
                    _ => {}
                }
            }) as Box<dyn FnMut(_)>);
            MessageListenerGuard::new(window, handler)
        }))
    };

    let restart_instance = instance_id.clone();
    let restart_secret = bridge_secret.clone();
    Effect::new(move |prev: Option<u64>| {
        let generation = restart.get();
        if prev.is_some_and(|previous| previous != generation) {
            log.run(format!("restart #{generation}"));
            if let Some(iframe) = iframe_element.get_value() {
                iframe.set_srcdoc(&build_srcdoc(
                    &restart_instance,
                    &restart_secret,
                    current_theme(),
                ));
            }
        }
        generation
    });

    if let Some(theme_ctx) = use_context::<crate::app::ThawThemeContext>() {
        let theme_signal = theme_ctx.0;
        let theme_instance = instance_id.clone();
        let theme_secret = bridge_secret.clone();
        Effect::new(move |prev: Option<bool>| {
            theme_signal.track();
            if prev.is_some() {
                if let Some(iframe) = iframe_element.get_value() {
                    let instance = theme_instance.clone();
                    let secret = theme_secret.clone();
                    gloo_timers::callback::Timeout::new(200, move || {
                        post_json(
                            &iframe,
                            json!({
                                "type": "plugin_theme",
                                "instanceId": instance,
                                "secret": secret,
                                "themeName": current_theme().id,
                            }),
                        );
                    })
                    .forget();
                }
                log.run("theme updated".to_string());
            }
            true
        });
    }

    let restart_click = move |_| {
        console.set(Vec::new());
        events.set(Vec::new());
        restart.update(|value| *value += 1);
    };
    let srcdoc = build_srcdoc(&instance_id, &bridge_secret, current_theme());

    view! {
        <div class="plugin-host__frame">
            {(!frameless).then(|| view! {
                <div class="plugin-host__toolbar">
                    <button class="plugin-host__run plugin-host__run--server" on:click=restart_click>
                        "Restart"
                    </button>
                </div>
            })}
            {move || (!has_client.get()).then(|| view! {
                <div class="plugin-host__state">
                    "Plugin has no client_script; preview is empty."
                </div>
            })}
            <iframe
                class="plugin-host__iframe"
                sandbox="allow-scripts allow-downloads"
                allow="fullscreen"
                allowfullscreen=true
                srcdoc=srcdoc
                on:load=move |event| {
                    let iframe = event
                        .target()
                        .and_then(|target| target.dyn_into::<HtmlIFrameElement>().ok());
                    if let Some(iframe) = iframe {
                        iframe_element.set_value(Some(iframe));
                        do_init.run(());
                        log.run("iframe loaded".to_string());
                    }
                }
            ></iframe>
        </div>
    }
}

/// Куда плагину позволено уводить пользователя из iframe.
///
/// Белый список, а не проверка «ключ выглядит нормально»: `open_tab` открывает
/// произвольную страницу приложения от имени текущего пользователя, и скрипт в
/// iframe не должен выбирать её сам. Расширяется поимённо — под конкретный
/// разобранный сценарий, а не «на всякий случай».
const PLUGIN_NAVIGABLE_PREFIXES: &[&str] = &[
    // Заказы YM: drilldown из плагинов-отчётов.
    "a013_ym_order_details_",
    // Карточка Этапа: двойной клик по узлу в графе Процесса.
    "sys_stage_details_",
];

fn is_navigable_from_plugin(key: &str) -> bool {
    // Потолок длины — против раздувания ключа мусором после легального префикса.
    key.len() <= 128
        && PLUGIN_NAVIGABLE_PREFIXES
            .iter()
            .any(|prefix| key.starts_with(prefix))
}

#[allow(clippy::too_many_arguments)]
fn handle_plugin_invoke(
    data: &wasm_bindgen::JsValue,
    plugin_id: &str,
    instance: &str,
    secret: &str,
    iframe_element: StoredValue<Option<HtmlIFrameElement>, LocalStorage>,
    context: RwSignal<PluginRunContext>,
    data_mode: RwSignal<PluginDataMode>,
    console: RwSignal<Vec<String>>,
    log: Callback<String>,
    dev: bool,
) {
    let Some(request_id) = string_property(data, "requestId") else {
        return;
    };
    let Some(method) = string_property(data, "method") else {
        return;
    };
    let args = js_sys::Reflect::get(data, &wasm_bindgen::JsValue::from_str("args"))
        .ok()
        .and_then(|value| serde_wasm_bindgen::from_value(value).ok())
        .unwrap_or(serde_json::Value::Null);

    let id = plugin_id.to_string();
    let instance = instance.to_string();
    let secret = secret.to_string();
    let method_for_log = method.clone();
    log.run(format!("invoke {method_for_log}"));

    spawn_local(async move {
        let request = PluginInvokeRequest {
            method,
            args,
            context: context.get_untracked(),
            data_mode: data_mode.get_untracked(),
        };
        let response = if dev {
            api::dev_invoke(&id, &request).await
        } else {
            api::invoke(&id, &request).await
        };
        let message = match response {
            Ok(body) => {
                let logs = body
                    .get("logs")
                    .and_then(|value| value.as_array())
                    .map(|items| {
                        items
                            .iter()
                            .filter_map(|value| value.as_str().map(str::to_string))
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                if !logs.is_empty() {
                    console.update(|current| current.extend(logs));
                }
                let result = body
                    .get("result")
                    .cloned()
                    .unwrap_or(serde_json::Value::Null);
                let count = result.as_array().map(|array| array.len());
                log.run(match count {
                    Some(n) => format!("{method_for_log}: ok ({n})"),
                    None => format!("{method_for_log}: ok"),
                });
                json!({
                    "type": "plugin_invoke_result",
                    "instanceId": instance,
                    "secret": secret,
                    "requestId": request_id,
                    "ok": true,
                    "result": result
                })
            }
            Err(message) => {
                log.run(format!("{method_for_log}: error - {message}"));
                json!({
                    "type": "plugin_invoke_result",
                    "instanceId": instance,
                    "secret": secret,
                    "requestId": request_id,
                    "ok": false,
                    "error": message
                })
            }
        };
        if let Some(iframe) = iframe_element.get_value() {
            post_json(&iframe, message);
        }
    });
}

/// Чтение и запись редактируемого поля документа.
///
/// Родитель здесь — граница доверия, а не транспорт. iframe плагина живёт в
/// opaque origin (`sandbox` без `allow-same-origin`): у него нет ни cookie, ни
/// JWT, поэтому сам он до API не дотянется в принципе. Раз посредничество всё
/// равно неизбежно, посредник и держит право: цель записи берётся из сигнала
/// `document`, а не из сообщения, и подменить её плагин не может.
#[allow(clippy::too_many_arguments)]
fn handle_plugin_document(
    data: &wasm_bindgen::JsValue,
    plugin_id: &str,
    instance: &str,
    secret: &str,
    iframe_element: StoredValue<Option<HtmlIFrameElement>, LocalStorage>,
    document: Option<RwSignal<Option<PluginDocumentBinding>>>,
    data_mode: RwSignal<PluginDataMode>,
    log: Callback<String>,
) {
    let Some(request_id) = string_property(data, "requestId") else {
        return;
    };
    let op = string_property(data, "op").unwrap_or_default();

    let instance = instance.to_string();
    let secret = secret.to_string();
    let id = plugin_id.to_string();

    let reply = move |ok: bool, payload: serde_json::Value| {
        let mut message = json!({
            "type": "plugin_document_result",
            "instanceId": instance,
            "secret": secret,
            "requestId": request_id,
            "ok": ok,
        });
        let key = if ok { "result" } else { "error" };
        message[key] = payload;
        if let Some(iframe) = iframe_element.get_value() {
            post_json(&iframe, message);
        }
    };

    // Нет цели — отказываем, не ходя на сервер: значит, плагин открыт не как
    // редактор документа, и писать ему нечего.
    let Some(binding) = document.and_then(|signal| signal.get_untracked()) else {
        log.run("document: no target in host context".to_string());
        reply(
            false,
            json!("Плагин открыт без привязки к документу: редактирование недоступно"),
        );
        return;
    };

    match op.as_str() {
        "load" => {
            spawn_local(async move {
                match api::load_document(&id, &binding.target).await {
                    Ok(response) => {
                        // Версию запоминает хост: следующий save отталкивается
                        // от неё, а не от того, что помнит плагин.
                        if let Some(signal) = document {
                            signal.update(|current| {
                                if let Some(current) = current.as_mut() {
                                    current.version = Some(response.version);
                                }
                            });
                        }
                        log.run(format!("document: loaded v{}", response.version));
                        reply(
                            true,
                            json!({ "content": response.content, "version": response.version }),
                        );
                    }
                    Err(message) => {
                        log.run(format!("document: load error - {message}"));
                        reply(false, json!(message));
                    }
                }
            });
        }
        "save" => {
            // Снимок — замороженные данные. Писать из него нельзя: правки
            // относились бы к состоянию, которого в live уже нет.
            if data_mode.get_untracked() == PluginDataMode::Snapshot {
                log.run("document: save blocked in snapshot mode".to_string());
                reply(
                    false,
                    json!("Сохранение недоступно в режиме снимка: переключитесь на live-данные"),
                );
                return;
            }

            let content = js_sys::Reflect::get(data, &wasm_bindgen::JsValue::from_str("content"))
                .ok()
                .and_then(|value| serde_wasm_bindgen::from_value(value).ok())
                .unwrap_or(serde_json::Value::Null);

            if content.is_null() {
                reply(false, json!("Пустое содержимое документа не сохраняется"));
                return;
            }
            let payload_bytes = serde_json::to_string(&content)
                .map(|s| s.len())
                .unwrap_or(0);
            if payload_bytes > MAX_DOCUMENT_BYTES {
                log.run(format!("document: payload too large ({payload_bytes} B)"));
                reply(false, json!("Документ слишком большой для сохранения"));
                return;
            }

            let request = PluginDocumentSaveRequest {
                target: binding.target.clone(),
                content,
                expected_version: binding.version,
            };
            spawn_local(async move {
                match api::save_document(&id, &request).await {
                    Ok(response) => {
                        if let Some(signal) = document {
                            signal.update(|current| {
                                if let Some(current) = current.as_mut() {
                                    current.version = Some(response.version);
                                }
                            });
                        }
                        log.run(format!("document: saved v{}", response.version));
                        reply(true, json!({ "version": response.version }));
                    }
                    Err(message) => {
                        log.run(format!("document: save error - {message}"));
                        reply(false, json!(message));
                    }
                }
            });
        }
        other => {
            reply(false, json!(format!("Unknown document op '{other}'")));
        }
    }
}
