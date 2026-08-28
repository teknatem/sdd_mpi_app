//! WB sales-funnel report plugin fixture (`PLG-WB-SALES-FUNNEL`), installed by
//! `/api/plugin/testdata`.
//!
//! A two-level tree report over `a036_wb_sales_funnel_daily`: positions (by `nm_id`)
//! with period totals, each expandable into its per-day rows. All aggregation lives in
//! the plugin's read-only SQL resources (`json_each(lines_json)`); the client renders a
//! sortable/filterable tree in the sandbox iframe. Ships as a Rust fixture for
//! reproducibility, but once seeded it is a normal DB plugin, fully editable from the
//! plugin editor (user mode).

use super::repository;
use chrono::Utc;
use contracts::plugins::{
    DataBinding, PluginBundle, PluginDefinition, PluginManifest, PluginRuntime, PluginStatus,
    ViewSpec,
};

fn db() -> &'static sea_orm::DatabaseConnection {
    crate::shared::data::db::get_connection()
}

// --- SQL resources (read-only; json_each over a036 is whitelisted by the SQL guard) ---

/// Product-level totals over the period. Params: [date_from, date_to, conn, conn].
/// `conn = ""` means "all cabinets" (the `? = '' OR ...` short-circuit).
const FUNNEL_PRODUCTS_SQL: &str = r#"
SELECT
  CAST(json_extract(j.value, '$.nm_id') AS INTEGER)                     AS nm_id,
  MAX(json_extract(j.value, '$.vendor_code'))                          AS vendor_code,
  MAX(json_extract(j.value, '$.title'))                               AS title,
  MAX(json_extract(j.value, '$.brand_name'))                          AS brand_name,
  COALESCE(SUM(json_extract(j.value, '$.metrics.open_count')), 0)      AS open_count,
  COALESCE(SUM(json_extract(j.value, '$.metrics.cart_count')), 0)      AS cart_count,
  COALESCE(SUM(json_extract(j.value, '$.metrics.order_count')), 0)     AS order_count,
  COALESCE(SUM(json_extract(j.value, '$.metrics.order_sum')), 0)       AS order_sum,
  COALESCE(SUM(json_extract(j.value, '$.metrics.buyout_count')), 0)    AS buyout_count,
  COALESCE(SUM(json_extract(j.value, '$.metrics.buyout_sum')), 0)      AS buyout_sum,
  COALESCE(SUM(json_extract(j.value, '$.metrics.add_to_wishlist_count')), 0) AS wishlist_count
FROM a036_wb_sales_funnel_daily d, json_each(d.lines_json) j
WHERE d.is_deleted = 0
  AND d.document_date BETWEEN ? AND ?
  AND (? = '' OR d.connection_id = ?)
GROUP BY nm_id
ORDER BY order_count DESC
"#;

/// Per-day rows for one position. Params: [nm_id, date_from, date_to, conn, conn].
/// Aggregates by day so multiple cabinets/lines on the same date collapse into one row.
const FUNNEL_DAYS_SQL: &str = r#"
SELECT
  d.document_date                                                     AS day,
  COALESCE(SUM(json_extract(j.value, '$.metrics.open_count')), 0)      AS open_count,
  COALESCE(SUM(json_extract(j.value, '$.metrics.cart_count')), 0)      AS cart_count,
  COALESCE(SUM(json_extract(j.value, '$.metrics.order_count')), 0)     AS order_count,
  COALESCE(SUM(json_extract(j.value, '$.metrics.order_sum')), 0)       AS order_sum,
  COALESCE(SUM(json_extract(j.value, '$.metrics.buyout_count')), 0)    AS buyout_count,
  COALESCE(SUM(json_extract(j.value, '$.metrics.buyout_sum')), 0)      AS buyout_sum,
  COALESCE(SUM(json_extract(j.value, '$.metrics.add_to_wishlist_count')), 0) AS wishlist_count
FROM a036_wb_sales_funnel_daily d, json_each(d.lines_json) j
WHERE d.is_deleted = 0
  AND CAST(json_extract(j.value, '$.nm_id') AS INTEGER) = ?
  AND d.document_date BETWEEN ? AND ?
  AND (? = '' OR d.connection_id = ?)
GROUP BY d.document_date
ORDER BY d.document_date
"#;

/// WB cabinets for the filter dropdown — derived from a036 itself (only cabinets that
/// actually have funnel data), since `a006_connection_mp.marketplace` is a ref to
/// a005_marketplace rather than a plain "WB" string.
const FUNNEL_CONNECTIONS_SQL: &str = r#"
SELECT DISTINCT c.id, c.description AS name
FROM a006_connection_mp c
JOIN a036_wb_sales_funnel_daily d ON d.connection_id = c.id
WHERE c.is_deleted = 0 AND d.is_deleted = 0
ORDER BY c.description
"#;

// --- Server ES module: exported async methods called via host.invoke() ---

const FUNNEL_SERVER_SCRIPT: &str = r#"
function num(value) {
  const n = Number(value);
  return Number.isFinite(n) ? n : 0;
}

// Conversion/percent are recomputed from summed counts, never summed per-day.
function pct(part, whole) {
  const w = num(whole);
  if (w <= 0) return 0;
  return Math.round((num(part) / w) * 10000) / 100;
}

function enrich(r) {
  return {
    nm_id: r.nm_id,
    vendor_code: r.vendor_code,
    title: r.title,
    brand_name: r.brand_name,
    day: r.day,
    open_count: num(r.open_count),
    cart_count: num(r.cart_count),
    order_count: num(r.order_count),
    order_sum: num(r.order_sum),
    buyout_count: num(r.buyout_count),
    buyout_sum: num(r.buyout_sum),
    wishlist_count: num(r.wishlist_count),
    cart_conv: pct(r.cart_count, r.open_count),
    order_conv: pct(r.order_count, r.cart_count),
    buyout_pct: pct(r.buyout_count, r.order_count)
  };
}

export async function loadConnections(_args, host) {
  const rows = await host.db.queryResource("funnelConnections", []);
  return { rows };
}

export async function loadProducts(args, host) {
  if (!args || !args.dateFrom || !args.dateTo) {
    throw new Error("Both date boundaries are required");
  }
  const conn = args.connectionId ? String(args.connectionId) : "";
  host.log.info("Funnel products", args.dateFrom, args.dateTo, conn || "(all cabinets)");
  const rows = await host.db.queryResource("funnelProducts", [args.dateFrom, args.dateTo, conn, conn]);
  const out = rows.map(enrich);
  host.log.info("Positions:", out.length);
  return { period: { from: args.dateFrom, to: args.dateTo }, rows: out };
}

export async function loadDays(args, host) {
  if (!args || args.nmId == null || !args.dateFrom || !args.dateTo) {
    throw new Error("nmId and both date boundaries are required");
  }
  const conn = args.connectionId ? String(args.connectionId) : "";
  const rows = await host.db.queryResource("funnelDays", [args.nmId, args.dateFrom, args.dateTo, conn, conn]);
  return { nmId: args.nmId, rows: rows.map(enrich) };
}
"#;

// --- Client ES module: builds the tree UI in the sandbox iframe ---

const FUNNEL_CLIENT_SCRIPT: &str = r##"
const COLUMNS = [
  { key: "nm_id",         label: "nmID",         type: "pos"   },
  { key: "vendor_code",   label: "Артикул",      type: "text"  },
  { key: "title",         label: "Наименование", type: "text"  },
  { key: "brand_name",    label: "Бренд",        type: "text"  },
  { key: "open_count",    label: "Переходы",     type: "int"   },
  { key: "cart_count",    label: "В корзину",    type: "int"   },
  { key: "cart_conv",     label: "Конв.корз,%",  type: "pct"   },
  { key: "order_count",   label: "Заказы",       type: "int"   },
  { key: "order_conv",    label: "Конв.зак,%",   type: "pct"   },
  { key: "order_sum",     label: "Сумма зак.",   type: "money" },
  { key: "buyout_count",  label: "Выкупы",       type: "int"   },
  { key: "buyout_sum",    label: "Сумма вык.",   type: "money" },
  { key: "buyout_pct",    label: "Выкуп,%",      type: "pct"   },
  { key: "wishlist_count",label: "Отложено",     type: "int"   }
];

// Text columns rendered between the toggle/nmID cell and the metric cells — kept as a
// single source of truth so day/total row padding always matches the header.
const TEXT_COLUMN_KEYS = ["vendor_code", "title", "brand_name"];
const NUMERIC = new Set(["int", "money", "pct"]);
const intFmt = new Intl.NumberFormat("ru-RU");
const moneyFmt = new Intl.NumberFormat("ru-RU", { minimumFractionDigits: 2, maximumFractionDigits: 2 });

function isoDate(d) { return d.toISOString().slice(0, 10); }

function fmt(type, value) {
  if (value == null || value === "") return type === "text" || type === "pos" ? "" : "0";
  switch (type) {
    case "int": return intFmt.format(Number(value) || 0);
    case "money": return moneyFmt.format(Number(value) || 0);
    case "pct": return (Number(value) || 0).toFixed(2) + "%";
    default: return String(value);
  }
}

// Conditional highlighting thresholds (%), tune to your category's benchmarks.
// Each metric only lights up once its denominator is non-zero — a position with no
// traffic yet isn't "bad", it's just no data.
const THRESHOLDS = {
  cart_conv:  { bad: 2,  good: 5,  hasSignal: (r) => Number(r.open_count) > 0 },
  order_conv: { bad: 15, good: 30, hasSignal: (r) => Number(r.cart_count) > 0 },
  buyout_pct: { bad: 30, good: 60, hasSignal: (r) => Number(r.order_count) > 0 }
};

function metricClass(key, row) {
  const t = THRESHOLDS[key];
  if (!t || !t.hasSignal(row)) return "";
  const v = Number(row[key]) || 0;
  if (v < t.bad) return "metric-bad";
  if (v >= t.good) return "metric-good";
  return "metric-warn";
}

// Recompute period totals over the currently visible (filtered) positions.
function computeTotals(rows) {
  const t = { open_count: 0, cart_count: 0, order_count: 0, order_sum: 0, buyout_count: 0, buyout_sum: 0, wishlist_count: 0 };
  for (const r of rows) {
    t.open_count += r.open_count; t.cart_count += r.cart_count; t.order_count += r.order_count;
    t.order_sum += r.order_sum; t.buyout_count += r.buyout_count; t.buyout_sum += r.buyout_sum;
    t.wishlist_count += r.wishlist_count;
  }
  const pct = (p, w) => (w > 0 ? Math.round((p / w) * 10000) / 100 : 0);
  t.cart_conv = pct(t.cart_count, t.open_count);
  t.order_conv = pct(t.order_count, t.cart_count);
  t.buyout_pct = pct(t.buyout_count, t.order_count);
  return t;
}

export async function mount(root, host) {
  const state = {
    products: [], sortKey: "order_count", sortDir: "desc", search: "", period: null,
    limitMode: "all", deadOnly: false
  };

  root.innerHTML = `
    <main class="funnel">
      <section class="filters">
        <label class="field">С <input id="f-from" type="date"></label>
        <label class="field">По <input id="f-to" type="date"></label>
        <label class="field">Кабинет
          <select id="f-conn"><option value="">Все кабинеты</option></select></label>
        <label class="field">Поиск
          <input id="f-search" type="search" placeholder="артикул / название / бренд"></label>
        <label class="field">Показать
          <select id="f-limit">
            <option value="all">Все позиции</option>
            <option value="top10">Топ 10</option>
            <option value="top20">Топ 20</option>
            <option value="bottom10">Худшие 10</option>
            <option value="bottom20">Худшие 20</option>
          </select></label>
        <label class="field field--checkbox">
          <input id="f-dead-only" type="checkbox"> Без заказов, есть трафик
        </label>
        <button id="f-refresh" class="btn" type="button">Обновить</button>
      </section>
      <div id="f-status" class="status"></div>
      <div class="table-wrap funnel-scroll">
        <table class="data-table funnel-table">
          <thead><tr id="f-head"></tr></thead>
          <tbody id="f-body"></tbody>
          <tfoot id="f-foot"></tfoot>
        </table>
      </div>
      <p class="funnel-note">Позиции с итогами за период; разверните позицию, чтобы увидеть разбивку по дням.</p>
    </main>`;

  const fromEl = root.querySelector("#f-from");
  const toEl = root.querySelector("#f-to");
  const connEl = root.querySelector("#f-conn");
  const searchEl = root.querySelector("#f-search");
  const limitEl = root.querySelector("#f-limit");
  const deadOnlyEl = root.querySelector("#f-dead-only");
  const refreshEl = root.querySelector("#f-refresh");
  const statusEl = root.querySelector("#f-status");
  const headEl = root.querySelector("#f-head");
  const bodyEl = root.querySelector("#f-body");
  const footEl = root.querySelector("#f-foot");

  const today = new Date();
  const weekAgo = new Date(today); weekAgo.setDate(weekAgo.getDate() - 6);
  fromEl.value = isoDate(weekAgo);
  toEl.value = isoDate(today);

  // --- header (sortable) ---
  for (const col of COLUMNS) {
    const th = document.createElement("th");
    th.textContent = col.label;
    if (NUMERIC.has(col.type)) th.classList.add("num");
    th.addEventListener("click", () => {
      if (state.sortKey === col.key) {
        state.sortDir = state.sortDir === "asc" ? "desc" : "asc";
      } else {
        state.sortKey = col.key;
        state.sortDir = NUMERIC.has(col.type) || col.type === "pos" ? "desc" : "asc";
      }
      render();
    });
    th._col = col;
    headEl.append(th);
  }

  function visibleProducts() {
    const q = state.search.trim().toLowerCase();
    let rows = state.products;
    if (q) {
      rows = rows.filter((r) =>
        String(r.nm_id).includes(q) ||
        (r.vendor_code || "").toLowerCase().includes(q) ||
        (r.title || "").toLowerCase().includes(q) ||
        (r.brand_name || "").toLowerCase().includes(q));
    }
    if (state.deadOnly) {
      rows = rows.filter((r) => Number(r.open_count) > 0 && Number(r.order_count) === 0);
    }
    const key = state.sortKey;
    const col = COLUMNS.find((c) => c.key === key);
    const numeric = col && (NUMERIC.has(col.type) || col.type === "pos");
    const dir = state.sortDir === "asc" ? 1 : -1;
    rows = rows.slice().sort((a, b) => {
      let av = a[key], bv = b[key];
      if (numeric) { av = Number(av) || 0; bv = Number(bv) || 0; return (av - bv) * dir; }
      return String(av || "").localeCompare(String(bv || ""), "ru") * dir;
    });
    const limitMatch = /^(top|bottom)(\d+)$/.exec(state.limitMode);
    if (limitMatch) rows = rows.slice(0, Number(limitMatch[2]));
    return rows;
  }

  function metricCells(tr, row) {
    for (const col of COLUMNS) {
      if (col.type === "pos" || col.type === "text") continue;
      const td = document.createElement("td");
      td.className = "num";
      if (col.type === "pct") {
        const cls = metricClass(col.key, row);
        if (cls) td.classList.add(cls);
      }
      td.textContent = fmt(col.type, row[col.key]);
      tr.append(td);
    }
  }

  function renderDayRow(row) {
    const tr = document.createElement("tr");
    tr.className = "day";
    const first = document.createElement("td");
    first.className = "day-label";
    first.textContent = row.day;
    tr.append(first);
    // Text columns stay empty for day rows.
    for (let i = 0; i < TEXT_COLUMN_KEYS.length; i++) tr.append(document.createElement("td"));
    metricCells(tr, row, "day");
    return tr;
  }

  async function toggle(product) {
    product.expanded = !product.expanded;
    if (product.expanded && !product.daysLoaded) {
      product.loading = true;
      render();
      try {
        const res = await host.invoke("loadDays", {
          nmId: product.nm_id, dateFrom: fromEl.value, dateTo: toEl.value,
          connectionId: connEl.value
        });
        product.days = res.rows || [];
        product.daysLoaded = true;
      } catch (err) {
        product.expanded = false;
        statusEl.className = "status status--error";
        statusEl.textContent = err instanceof Error ? err.message : String(err);
      } finally {
        product.loading = false;
      }
    }
    render();
  }

  function render() {
    // sort arrows
    for (const th of headEl.children) {
      const sorted = th._col.key === state.sortKey;
      th.classList.toggle("sorted", sorted);
      if (sorted) th.dataset.arrow = state.sortDir === "asc" ? "▲" : "▼"; else th.dataset.arrow = "";
    }

    const rows = visibleProducts();
    bodyEl.replaceChildren();
    for (const product of rows) {
      const tr = document.createElement("tr");
      tr.className = "product";
      const first = document.createElement("td");
      const toggleEl = document.createElement("span");
      toggleEl.className = "tree-toggle";
      toggleEl.textContent = product.loading ? "…" : product.expanded ? "▼" : "▶";
      toggleEl.addEventListener("click", () => toggle(product));
      first.append(toggleEl, document.createTextNode(" " + product.nm_id));
      tr.append(first);
      for (const key of TEXT_COLUMN_KEYS) {
        const td = document.createElement("td");
        td.textContent = product[key] == null ? "" : String(product[key]);
        tr.append(td);
      }
      metricCells(tr, product, "product");
      bodyEl.append(tr);

      if (product.expanded) {
        if (product.loading && !product.daysLoaded) {
          const loadingTr = document.createElement("tr");
          loadingTr.className = "day";
          const td = document.createElement("td");
          td.colSpan = COLUMNS.length;
          td.className = "day-label";
          td.textContent = "Загрузка…";
          loadingTr.append(td);
          bodyEl.append(loadingTr);
        } else if ((product.days || []).length === 0) {
          const emptyTr = document.createElement("tr");
          emptyTr.className = "day";
          const td = document.createElement("td");
          td.colSpan = COLUMNS.length;
          td.className = "day-label";
          td.textContent = "Нет данных по дням";
          emptyTr.append(td);
          bodyEl.append(emptyTr);
        } else {
          for (const day of product.days) bodyEl.append(renderDayRow(day));
        }
      }
    }

    // totals footer over visible positions
    footEl.replaceChildren();
    if (rows.length) {
      const totals = computeTotals(rows);
      const tr = document.createElement("tr");
      tr.className = "total";
      const first = document.createElement("td");
      first.textContent = "ИТОГО (" + rows.length + ")";
      tr.append(first);
      for (let i = 0; i < TEXT_COLUMN_KEYS.length; i++) tr.append(document.createElement("td"));
      metricCells(tr, totals, "total");
      footEl.append(tr);
    }
  }

  async function loadProducts() {
    refreshEl.disabled = true;
    statusEl.className = "status";
    statusEl.textContent = "Загрузка…";
    bodyEl.replaceChildren();
    footEl.replaceChildren();
    try {
      const res = await host.invoke("loadProducts", {
        dateFrom: fromEl.value, dateTo: toEl.value, connectionId: connEl.value
      });
      state.period = res.period;
      state.products = (res.rows || []).map((r) => ({
        ...r, expanded: false, daysLoaded: false, loading: false, days: []
      }));
      statusEl.textContent = "Позиций: " + state.products.length +
        " · период " + res.period.from + " — " + res.period.to;
      render();
    } catch (err) {
      statusEl.className = "status status--error";
      statusEl.textContent = err instanceof Error ? err.message : String(err);
    } finally {
      refreshEl.disabled = false;
    }
  }

  async function loadConnections() {
    try {
      const res = await host.invoke("loadConnections", {});
      for (const row of res.rows || []) {
        const opt = document.createElement("option");
        opt.value = row.id;
        opt.textContent = row.name || row.id;
        connEl.append(opt);
      }
    } catch (err) {
      host.log && host.log.warn && host.log.warn("loadConnections failed", String(err));
    }
  }

  let searchTimer = null;
  searchEl.addEventListener("input", () => {
    clearTimeout(searchTimer);
    searchTimer = setTimeout(() => { state.search = searchEl.value; render(); }, 200);
  });
  limitEl.addEventListener("change", () => {
    state.limitMode = limitEl.value;
    // Pin the sort direction to match "top" (best first) / "bottom" (worst first) for
    // whichever column is currently sorted; manual header clicks still override it after.
    if (state.limitMode.startsWith("top")) state.sortDir = "desc";
    else if (state.limitMode.startsWith("bottom")) state.sortDir = "asc";
    render();
  });
  deadOnlyEl.addEventListener("change", () => {
    state.deadOnly = deadOnlyEl.checked;
    render();
  });
  refreshEl.addEventListener("click", loadProducts);
  fromEl.addEventListener("change", loadProducts);
  toEl.addEventListener("change", loadProducts);
  connEl.addEventListener("change", loadProducts);

  await loadConnections();
  await loadProducts();
}
"##;

// Only truly custom (tree-specific) rules live here — everything else (table, inputs,
// buttons, status line, sticky header, hover/alternating rows, colors) comes for free
// from the theme-aware plugin-sdk.css classes (`.data-table`, `.table-wrap`, `.filters`,
// `.field`, `.btn`, `.status`) shared with the WB-unmapped-orders demo plugin. Values
// reference the same --color-*/--table-* tokens the app's own dark/light/forest themes
// define, so this renders correctly in all three without any hardcoded fallback colors.
const FUNNEL_STYLES: &str = r#"
.funnel { padding: 12px 0 16px; display: flex; flex-direction: column; gap: 10px; }
.funnel .filters, .funnel .status, .funnel .funnel-note { padding: 0 20px; }
/* Full-bleed table: no side borders/radius, spans the iframe edge to edge. */
.funnel .table-wrap.funnel-scroll {
  border-left: none;
  border-right: none;
  border-radius: 0;
  width: 100%;
}
.funnel-scroll { max-height: 72vh; }
.funnel-note { color: var(--color-text-secondary); font-size: var(--font-size-xs); }
.funnel-table thead th { cursor: pointer; user-select: none; }
.funnel-table thead th.sorted::after { content: attr(data-arrow); margin-left: 4px; font-size: 0.85em; }
.funnel-table tbody tr.day td { background: var(--table-row-even); color: var(--color-text-secondary); }
.funnel-table tfoot tr.total td {
  font-weight: var(--font-weight-semibold);
  border-top: 2px solid var(--color-border);
  background: var(--table-header-bg);
  position: sticky;
  bottom: 0;
}
.tree-toggle { display: inline-block; width: 1em; cursor: pointer; color: var(--color-primary); }
.day-label { padding-left: 22px; }
/* Checkbox sizing/accent-color now comes from the shared plugin-sdk.css
   input[type="checkbox"] rule — only the row layout is custom here. */
.field--checkbox { flex-direction: row; align-items: center; gap: 6px; }
.metric-good { color: var(--color-success); font-weight: var(--font-weight-semibold); }
.metric-warn { color: var(--color-warning); }
.metric-bad  { color: var(--color-error); font-weight: var(--font-weight-semibold); }
"#;

pub async fn insert_funnel_plugin() -> anyhow::Result<()> {
    use sea_orm::{ConnectionTrait, DbBackend, Statement};

    const FUNNEL_ID: &str = "9f1c0a00-0000-4000-8000-000000000002";
    const FUNNEL_CODE: &str = "PLG-WB-SALES-FUNNEL";

    db().execute(Statement::from_sql_and_values(
        DbBackend::Sqlite,
        "DELETE FROM plugin WHERE code = ? OR id = ?",
        vec![FUNNEL_CODE.into(), FUNNEL_ID.into()],
    ))
    .await?;

    let now = Utc::now();
    let def = PluginDefinition {
        id: FUNNEL_ID.to_string(),
        bundle: PluginBundle {
            manifest: PluginManifest {
                code: FUNNEL_CODE.to_string(),
                title: "Воронка продаж WB".to_string(),
                runtime: PluginRuntime::Hybrid,
                api_version: "2".to_string(),
                description: Some(
                    "Дерево воронки продаж WB (a036): позиции с итогами за период, \
                     разворачиваются по дням; фильтр и сортировки."
                        .to_string(),
                ),
                capabilities: vec!["data:read".to_string()],
                // Своё дерево на plugin-sdk.css, без PluginTables/PluginCharts.
                client_kits: Some(vec![]),
                built_for_migration: None,
            },
            params: vec![],
            data: DataBinding::default(),
            client_script: Some(FUNNEL_CLIENT_SCRIPT.to_string()),
            server_script: Some(FUNNEL_SERVER_SCRIPT.to_string()),
            view_spec: ViewSpec::default(),
            styles: Some(FUNNEL_STYLES.to_string()),
            sql_resources: [
                (
                    "funnelProducts".to_string(),
                    FUNNEL_PRODUCTS_SQL.trim().to_string(),
                ),
                ("funnelDays".to_string(), FUNNEL_DAYS_SQL.trim().to_string()),
                (
                    "funnelConnections".to_string(),
                    FUNNEL_CONNECTIONS_SQL.trim().to_string(),
                ),
            ]
            .into_iter()
            .collect(),
            assets: Default::default(),
        },
        status: PluginStatus::Active,
        is_enabled: true,
        owner_user_id: None,
        created_by_agent_id: None,
        version: 1,
        created_at: now,
        updated_at: now,
        rating: None,
        snapshot: None,
        s3_published_version: None,
        s3_published_at: None,
    };
    repository::insert(db(), &def).await?;
    Ok(())
}
