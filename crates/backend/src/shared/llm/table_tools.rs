//! Инструменты агента-построителя таблиц (навык `table-builder`).
//!
//! Канонический `build_table` создаёт declarative live/snapshot plugin. Старые
//! template/example helpers остаются только для общего `plugin-authoring` workflow.

use super::types::ToolDefinition;
use contracts::plugins::{
    DataBinding, ParamSpec, ParamType, PluginBundle, PluginDataMode, PluginDataSource,
    PluginManifest, PluginRunContext, PluginRuntime, ViewSpec, Widget, WidgetKind,
};
use serde_json::{json, Value};

/// Имена инструментов построителя таблиц (для guard'а в диспетчере).
pub const TABLE_TOOL_NAMES: &[&str] =
    &["table_template", "table_examples", "get_table_ui_contract"];

/// Клиентский ES-модуль таблицы: грузит данные и рисует через PluginTables.
fn table_client_script(spec_json: &str) -> String {
    format!(
        r##"let _table = null;
export async function mount(root, host) {{
  root.replaceChildren();
  const loading = document.createElement("div");
  loading.className = "status";
  loading.textContent = "Загрузка…";
  root.appendChild(loading);
  try {{
    const rows = await host.invoke("data", {{}});
    const spec = {spec};
    _table = PluginTables.render(root, spec, rows);
  }} catch (e) {{
    root.replaceChildren();
    const box = document.createElement("div");
    box.className = "status status--error";
    box.textContent = e && e.message ? e.message : String(e);
    root.appendChild(box);
  }}
}}
export async function unmount() {{
  if (_table) {{ try {{ _table.destroy(); }} catch (e) {{}} _table = null; }}
}}
"##,
        spec = spec_json
    )
}

fn table_client_script_with_period(spec_json: &str, date_from: &str, date_to: &str) -> String {
    format!(
        r##"let _table = null;
export async function mount(root, host) {{
  root.innerHTML = `<div class="plugin-period">
    <label>С <input type="date" data-role="from" value="{date_from}"></label>
    <label>По <input type="date" data-role="to" value="{date_to}"></label>
    <button type="button" class="btn btn--secondary" data-role="apply">Применить</button>
  </div><div data-role="table"><div class="status">Загрузка…</div></div>`;
  const tableRoot = root.querySelector('[data-role="table"]');
  const from = root.querySelector('[data-role="from"]');
  const to = root.querySelector('[data-role="to"]');
  const apply = root.querySelector('[data-role="apply"]');
  const snapshotMode = host.context && host.context.params && host.context.params._plugin_data_mode === 'snapshot';
  if (snapshotMode) {{ from.disabled = true; to.disabled = true; apply.disabled = true; apply.title = 'Снимок содержит период публикации'; }}
  const spec = {spec};
  async function load() {{
    if (!from.value || !to.value || from.value > to.value) {{
      tableRoot.innerHTML = '<div class="status status--error">Проверьте выбранный период</div>';
      return;
    }}
    if (_table) {{ try {{ _table.destroy(); }} catch (e) {{}} _table = null; }}
    apply.disabled = true;
    tableRoot.innerHTML = '<div class="status">Загрузка…</div>';
    try {{
      const rows = await host.invoke("data", {{ date_from: from.value, date_to: to.value }});
      _table = PluginTables.render(tableRoot, spec, rows);
    }} catch (e) {{
      tableRoot.innerHTML = '<div class="status status--error">' + (e && e.message ? e.message : String(e)) + '</div>';
    }} finally {{ apply.disabled = snapshotMode; }}
  }}
  if (!snapshotMode) apply.addEventListener('click', load);
  await load();
}}
export async function unmount() {{
  if (_table) {{ try {{ _table.destroy(); }} catch (e) {{}} _table = null; }}
}}
"##,
        spec = spec_json,
        date_from = date_from,
        date_to = date_to,
    )
}

/// Серверный метод без параметров (демо-SQL).
const TABLE_SERVER: &str = r##"export async function data(args, host) {
  return await host.db.queryResource("rows", []);
}
"##;

/// Серверный метод с периодом из контекста (для примеров на реальных таблицах).
const TABLE_SERVER_CTX: &str = r##"export async function data(args, host) {
  const c = host.context || {};
  const p = c.params || {};
  const today = new Date();
  const fromDefault = new Date(today);
  fromDefault.setDate(today.getDate() - 30);
  const iso = d => d.toISOString().slice(0, 10);
  const from = c.date_from || p.date_from || args.date_from || iso(fromDefault);
  const to = c.date_to || p.date_to || args.date_to || iso(today);
  return await host.db.queryResource("rows", [from, to]);
}
"##;

/// Собрать bundle-таблицу. `with_ctx` — серверный метод читает период из контекста.
fn build_bundle(
    code: &str,
    title: &str,
    description: &str,
    capabilities: Value,
    spec: Value,
    sql: &str,
    with_ctx: bool,
) -> Value {
    let params = if with_ctx {
        json!([
            {
                "key": "date_from",
                "param_type": "date",
                "label": "Дата с",
                "required": false,
                "global_filter_key": "date_from"
            },
            {
                "key": "date_to",
                "param_type": "date",
                "label": "Дата по",
                "required": false,
                "global_filter_key": "date_to"
            }
        ])
    } else {
        json!([])
    };
    json!({
        "manifest": {
            "code": code,
            "title": title,
            "runtime": "hybrid",
            "api_version": "2",
            "description": description,
            "capabilities": capabilities,
        },
        "params": params,
        "client_script": table_client_script(&spec.to_string()),
        "server_script": if with_ctx { TABLE_SERVER_CTX } else { TABLE_SERVER },
        "sql_resources": { "rows": sql },
        "styles": ".ptables{padding:4px;}",
    })
}

// ─── Демо-SQL без зависимости от схемы (шаблон сразу валиден и запускается) ───

const DEMO_BASIC_SQL: &str = "SELECT 'Куртка' AS name, 5200 AS revenue, 42 AS qty UNION ALL SELECT 'Джинсы', 3100, 88 UNION ALL SELECT 'Кроссовки', 7400, 31 UNION ALL SELECT 'Футболка', 1800, 120";
const DEMO_FIN_SQL: &str = "SELECT 'WB' AS channel, 520000 AS revenue, 0.34 AS margin UNION ALL SELECT 'OZON', 310000, 0.21 UNION ALL SELECT 'YM', 180000, -0.05";
const DEMO_PIVOT_SQL: &str = "SELECT 'Одежда' AS category, 'Май' AS period, 5200 AS revenue UNION ALL SELECT 'Обувь', 'Май', 3100 UNION ALL SELECT 'Одежда', 'Июнь', 6100 UNION ALL SELECT 'Обувь', 'Июнь', 2700";

/// Минимальный валидный bundle-таблица по типу (basic | financial | pivot-lite).
fn table_template(args: &Value) -> Value {
    let kind = args.get("kind").and_then(Value::as_str).unwrap_or("basic");
    let caps = json!(["network:none"]);
    let bundle = match kind {
        "financial" => build_bundle(
            "MY-TABLE-FIN",
            "Моя таблица (финансовая)",
            "Шаблон финансовой таблицы: каналы, выручка, маржа с условным форматированием.",
            caps,
            json!({
                "title": "Финансовый срез",
                "columns": [
                    { "key": "channel", "label": "Канал", "type": "text" },
                    { "key": "revenue", "label": "Выручка", "type": "money" },
                    { "key": "margin", "label": "Маржа", "type": "percent" }
                ],
                "sort": { "key": "revenue", "dir": "desc" },
                "filters": { "global": true, "perColumn": true },
                "conditionalFormat": [
                    { "column": "revenue", "kind": "dataBar", "color": "primary" },
                    { "column": "margin", "kind": "threshold", "rules": [
                        { "op": "<", "value": 0, "color": "error", "target": "text" },
                        { "op": ">=", "value": 0.3, "color": "success", "target": "bg" }
                    ] }
                ],
                "totals": { "enabled": true, "agg": { "revenue": "sum", "margin": "avg" } },
                "pagination": { "enabled": true, "pageSize": 50 },
                "export": { "csv": true, "clipboard": true }
            }),
            DEMO_FIN_SQL,
            false,
        ),
        "pivot-lite" => build_bundle(
            "MY-TABLE-PIVOT",
            "Моя таблица (срез)",
            "Шаблон таблицы-среза: категория × период × мера (плоская форма).",
            caps,
            json!({
                "title": "Срез по категориям",
                "columns": [
                    { "key": "category", "label": "Категория", "type": "text" },
                    { "key": "period", "label": "Период", "type": "text" },
                    { "key": "revenue", "label": "Выручка", "type": "money" }
                ],
                "sort": { "key": "category", "dir": "asc" },
                "filters": { "global": true, "perColumn": true },
                "conditionalFormat": [
                    { "column": "revenue", "kind": "heatmap", "min": "error", "mid": "warning", "max": "success" }
                ],
                "totals": { "enabled": true, "agg": { "revenue": "sum" } },
                "pagination": { "enabled": true, "pageSize": 50 },
                "export": { "csv": true, "clipboard": true }
            }),
            DEMO_PIVOT_SQL,
            false,
        ),
        _ => build_bundle(
            "MY-TABLE-BASIC",
            "Моя таблица",
            "Базовый шаблон: текст + меры, сортировка/фильтры/итоги/экспорт.",
            caps,
            json!({
                "title": "Заголовок",
                "columns": [
                    { "key": "name", "label": "Наименование", "type": "text" },
                    { "key": "revenue", "label": "Выручка", "type": "money" },
                    { "key": "qty", "label": "Кол-во", "type": "int" }
                ],
                "sort": { "key": "revenue", "dir": "desc" },
                "filters": { "global": true, "perColumn": true },
                "conditionalFormat": [
                    { "column": "revenue", "kind": "dataBar", "color": "primary" }
                ],
                "totals": { "enabled": true, "agg": { "revenue": "sum", "qty": "sum" } },
                "pagination": { "enabled": true, "pageSize": 50 },
                "export": { "csv": true, "clipboard": true }
            }),
            DEMO_BASIC_SQL,
            false,
        ),
    };
    json!({
        "bundle": bundle,
        "kind": kind,
        "hint": "Минимальный валидный плагин-таблица. Замени code/title и SQL в sql_resources.rows на реальный \
                 SELECT (проверь через execute_query), синхронизируй column.key с алиасами SELECT, затем \
                 plugin_smoke_test → plugin_upsert(status=active)."
    })
}

/// Готовые рабочие примеры на реальной таблице a012_wb_sales (db:read:wb).
fn table_examples() -> Value {
    let wb_caps = json!(["db:read:wb", "network:none"]);

    let articles = build_bundle(
        "EXAMPLE-TABLE-TOP-ARTICLES",
        "Пример: артикулы WB по выручке",
        "Таблица топ-артикулов: выручка и кол-во продаж за период из контекста.",
        wb_caps.clone(),
        json!({
            "title": "Артикулы WB по выручке",
            "columns": [
                { "key": "article", "label": "Артикул", "type": "text", "width": "200px" },
                { "key": "revenue", "label": "Выручка", "type": "money" },
                { "key": "qty", "label": "Продаж, шт", "type": "int" }
            ],
            "sort": { "key": "revenue", "dir": "desc" },
            "filters": { "global": true, "perColumn": true },
            "conditionalFormat": [
                { "column": "revenue", "kind": "dataBar", "color": "primary" }
            ],
            "totals": { "enabled": true, "agg": { "revenue": "sum", "qty": "sum" } },
            "pagination": { "enabled": true, "pageSize": 50 },
            "export": { "csv": true, "clipboard": true }
        }),
        "SELECT supplier_article AS article, SUM(total_price) AS revenue, COUNT(*) AS qty FROM a012_wb_sales WHERE is_deleted = 0 AND substr(sale_date, 1, 10) BETWEEN ? AND ? GROUP BY supplier_article ORDER BY revenue DESC LIMIT 200",
        true,
    );

    let daily = build_bundle(
        "EXAMPLE-TABLE-DAILY",
        "Пример: продажи WB по дням",
        "Таблица выручки и числа продаж по дням за период из контекста.",
        wb_caps,
        json!({
            "title": "Продажи WB по дням",
            "columns": [
                { "key": "d", "label": "Дата", "type": "date" },
                { "key": "revenue", "label": "Выручка", "type": "money" },
                { "key": "qty", "label": "Продаж, шт", "type": "int" }
            ],
            "sort": { "key": "d", "dir": "asc" },
            "filters": { "global": true, "perColumn": true },
            "conditionalFormat": [
                { "column": "revenue", "kind": "heatmap", "min": "error", "mid": "warning", "max": "success" }
            ],
            "totals": { "enabled": true, "agg": { "revenue": "sum", "qty": "sum" } },
            "pagination": { "enabled": true, "pageSize": 50 },
            "export": { "csv": true, "clipboard": true }
        }),
        "SELECT substr(sale_date, 1, 10) AS d, SUM(total_price) AS revenue, COUNT(*) AS qty FROM a012_wb_sales WHERE is_deleted = 0 AND substr(sale_date, 1, 10) BETWEEN ? AND ? GROUP BY substr(sale_date, 1, 10) ORDER BY d",
        true,
    );

    json!({
        "examples": [
            { "title": "Топ артикулов (dataBar)", "bundle": articles },
            { "title": "По дням (heatmap)", "bundle": daily }
        ],
        "hint": "Скопируй ближайший по форме данных пример, замени таблицу/колонки на свои (проверь SELECT \
                 через execute_query), выровняй table-spec, затем plugin_smoke_test → plugin_upsert."
    })
}

/// Контракт рантайма таблиц: как звать PluginTables.render и какой table-spec.
fn table_ui_contract() -> Value {
    json!({
        "runtime": "В iframe доступна глобаль window.PluginTables. В client_script внутри mount(root, host): \
                    const rows = await host.invoke(\"data\", {}); PluginTables.render(root, spec, rows).",
        "server": "Серверный метод data(args, host) возвращает массив объектов-строк через \
                   host.db.queryResource(\"rows\", params). Период бери из host.context.date_from/date_to \
                   или host.context.params.date_from/date_to; если контекст пустой, шаблон берёт последние 30 дней.",
        "client_side": "Сортировка/фильтры/условное форматирование/итоги/пагинация/экспорт работают \
                        КЛИЕНТ-САЙД над уже загруженным массивом строк — без новых запросов к серверу. \
                        Держи объём в разумных пределах (≈ до нескольких тысяч строк): используй \
                        GROUP BY / LIMIT в SELECT.",
        "column_types": ["text", "number", "int", "money", "percent", "date"],
        "spec": {
            "title": "string",
            "columns": [{
                "key": "имя колонки = алиас в SELECT",
                "label": "подпись",
                "type": "text|number|int|money|percent|date",
                "align": "left|right|center (необяз.; числа по умолчанию right)",
                "width": "напр. '180px' (необяз.)",
                "format": "money|int|percent|number (необяз.; по умолчанию = type)",
                "hidden": "bool (скрыта по умолчанию)"
            }],
            "sort": { "key": "колонка", "dir": "asc|desc" },
            "filters": { "global": "bool — строка поиска по всем колонкам", "perColumn": "bool — фильтр под каждой колонкой" },
            "conditionalFormat": [{
                "column": "ключ колонки",
                "kind": "threshold|dataBar|heatmap",
                "rules": "[для threshold] [{ op:'>|<|>=|<=|=|!=', value:N, color:'success|warning|error|<hex>', target:'text|bg' }]",
                "color": "[для dataBar] success|warning|error|primary|<hex>",
                "min": "[для heatmap] цвет нижней границы (success|warning|error|<hex>)",
                "mid": "[для heatmap] цвет середины",
                "max": "[для heatmap] цвет верхней границы"
            }],
            "totals": { "enabled": "bool", "agg": { "<колонка>": "sum|avg|count|min|max" } },
            "pagination": { "enabled": "bool (по умолчанию true)", "pageSize": "число (по умолчанию 50)" },
            "export": { "csv": "bool (по умолчанию true)", "clipboard": "bool (по умолчанию true)" }
        },
        "rules": [
            "Тема (свет/тёмная) и цвета подхватываются автоматически — НЕ хардкодь палитру; для cond-format \
             используй семантические имена success|warning|error|primary.",
            "column.key обязан совпадать с алиасом колонки в SELECT.",
            "Числовые операции (сортировка/итоги/dataBar/heatmap/числовые фильтры) работают только для \
             числовых типов (number|int|money|percent).",
            "Свой CSS — по минимуму; PluginTables сам рисует шапку/фильтры/итоги/пагинацию/тулбар."
        ]
    })
}

/// Определения инструментов построителя таблиц.
pub fn table_tool_definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "build_table".into(),
            description: "Построить и опубликовать live/snapshot таблицу одним вызовом по source из preview_data. Сервер проверяет реальные колонки и типы, создает снимок и карточку в чате.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "source": super::data_tools::plugin_data_source_schema(),
                    "context": { "type":"object", "description":"Начальные date_from/date_to и params для $context bindings; по умолчанию последние 30 дней.", "properties": { "date_from":{"type":"string"}, "date_to":{"type":"string"}, "connection_mp_refs":{"type":"array","items":{"type":"string"}}, "params":{"type":"object","additionalProperties":{"type":"string"}} } },
                    "title": { "type": "string" },
                    "code": { "type": "string" },
                    "table": {
                        "type": "object",
                        "properties": {
                            "columns": { "type": "array", "items": { "type": "object", "properties": { "field": {"type":"string"}, "label": {"type":"string"}, "type": {"type":"string", "enum":["text","number","int","money","percent","date"]} }, "required":["field"] } },
                            "sort": { "type": "object" },
                            "filters": { "type": "object" },
                            "conditional_format": { "type": "array" },
                            "totals": { "type": "object" },
                            "page_size": { "type": "integer", "minimum": 1, "maximum": 200 }
                        }
                    },
                    "snapshot": {
                        "type": "object",
                        "properties": {
                            "capture": { "type": "boolean", "default": true },
                            "allow_live_only": { "type": "boolean", "default": false }
                        },
                        "description": "По умолчанию снимок обязателен. Для live-only нужны capture=false и явный allow_live_only=true."
                    }
                },
                "required": ["source", "title", "table"]
            }),
        },
        ToolDefinition {
            name: "table_template".into(),
            description: "Минимальный ВАЛИДНЫЙ скелет плагина-таблицы (hybrid) по типу: \
                          basic | financial | pivot-lite. Возвращает { bundle, hint } с готовыми \
                          client/server скриптами и демо-SQL. Начинай новую таблицу с шаблона."
                .into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "kind": { "type": "string", "enum": ["basic", "financial", "pivot-lite"], "description": "Тип шаблона таблицы. По умолчанию basic." }
                }
            }),
        },
        ToolDefinition {
            name: "table_examples".into(),
            description: "Готовые рабочие примеры таблиц (топ-артикулы / по дням) на реальной \
                          таблице a012_wb_sales — образец структуры, SQL и table-spec \
                          (dataBar, heatmap, итоги). Возвращает { examples:[{ title, bundle }] }."
                .into(),
            parameters: json!({ "type": "object", "properties": {} }),
        },
        ToolDefinition {
            name: "get_table_ui_contract".into(),
            description: "Контракт рантайма таблиц: как звать PluginTables.render, полная форма \
                          table-spec (columns/sort/filters/conditionalFormat/totals/pagination/export), \
                          типы колонок и правила темы/условного форматирования."
                .into(),
            parameters: json!({ "type": "object", "properties": {} }),
        },
    ]
}

/// Диспетчер инструментов построителя таблиц (без БД — чистые заготовки).
pub fn execute_table_tool(name: &str, arguments: &str) -> Value {
    let args: Value = serde_json::from_str(arguments).unwrap_or_else(|_| json!({}));
    match name {
        "table_template" => table_template(&args),
        "table_examples" => table_examples(),
        "get_table_ui_contract" => table_ui_contract(),
        _ => json!({ "error": format!("Unknown table tool: '{}'", name) }),
    }
}

pub async fn execute_build_table(arguments: &str, chat_id: &str, agent_id: &str) -> Value {
    let args: Value = serde_json::from_str(arguments).unwrap_or_else(|_| json!({}));
    let source: PluginDataSource = match args
        .get("source")
        .cloned()
        .ok_or_else(|| "source is required")
        .and_then(|value| serde_json::from_value(value).map_err(|_| "invalid source"))
    {
        Ok(source) => source,
        Err(error) => {
            return json!({ "ok": false, "stage": "source", "error_code": "invalid_source", "error": error })
        }
    };
    let title = args
        .get("title")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("Таблица")
        .to_string();
    let requested_context: PluginRunContext = match args.get("context").cloned() {
        Some(value) => match serde_json::from_value(value) {
            Ok(context) => context,
            Err(error) => {
                return json!({ "ok": false, "stage": "source", "error_code": "invalid_context", "error": error.to_string() })
            }
        },
        None => PluginRunContext::default(),
    };
    let effective_context =
        crate::plugins::data::effective_source_context(Some(&requested_context));
    let snapshot = args.get("snapshot").cloned().unwrap_or_else(|| json!({}));
    let capture_snapshot = snapshot
        .get("capture")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let allow_live_only = snapshot
        .get("allow_live_only")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if !capture_snapshot && !allow_live_only {
        return json!({
            "ok": false,
            "stage": "snapshot",
            "error_code": "live_only_requires_confirmation",
            "error": "Set snapshot.allow_live_only=true explicitly to publish without a snapshot"
        });
    }
    let tabular = match crate::plugins::data::execute_source_with_context(
        &source,
        crate::plugins::data::TABLE_ROW_LIMIT,
        Some(&effective_context),
    )
    .await
    {
        Ok(result) => result,
        Err(error) => {
            return json!({ "ok": false, "stage": "source", "error_code": "query_failed", "error": error, "recommended_fix": "Исправьте source и повторите preview_data; затем передайте тот же source в build_table." })
        }
    };
    if tabular.truncated {
        return json!({
            "ok": false, "stage": "snapshot", "error_code": "snapshot_limit_exceeded",
            "error": format!("Table source exceeds {} rows; add filters, aggregation or LIMIT", crate::plugins::data::TABLE_ROW_LIMIT)
        });
    }
    if tabular.rows.is_empty() {
        return json!({ "ok": false, "stage": "source", "error_code": "empty_result", "error": "Source returned no rows", "recommended_fix": "Расширьте период или ослабьте фильтры source." });
    }
    if capture_snapshot {
        if let Err(error) = crate::plugins::data::validate_snapshot_payload(
            &Value::Array(tabular.rows.clone()),
            crate::plugins::data::TABLE_ROW_LIMIT,
        ) {
            return json!({
                "ok": false, "stage": "snapshot", "error_code": "snapshot_limit_exceeded",
                "error": error, "recommended_fix": "Сократите результат source: добавьте фильтр, агрегацию или LIMIT."
            });
        }
    }
    let inferred = crate::plugins::data::infer_columns(&tabular.rows, &tabular.columns);
    let inferred_type = |name: &str| {
        inferred.iter().find_map(|column| {
            (column.get("name").and_then(Value::as_str) == Some(name))
                .then(|| column.get("type").and_then(Value::as_str))
                .flatten()
        })
    };
    let table = args.get("table").cloned().unwrap_or_else(|| json!({}));
    let requested = table
        .get("columns")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let columns = if requested.is_empty() {
        tabular
            .columns
            .iter()
            .map(|field| {
                let kind = inferred_type(field).unwrap_or("text");
                json!({ "key": field, "label": field, "type": if kind == "number" { "number" } else { kind } })
            })
            .collect::<Vec<_>>()
    } else {
        let mut columns = Vec::new();
        for column in requested {
            let field = column
                .get("field")
                .or_else(|| column.get("key"))
                .and_then(Value::as_str)
                .unwrap_or_default();
            let Some(actual_type) = inferred_type(field) else {
                return json!({ "ok": false, "stage": "presentation", "error_code": "column_not_found", "columns": inferred, "error": format!("Column '{field}' was not found"), "recommended_fix": "Используйте field из списка доступных колонок preview_data." });
            };
            let kind = column
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or(actual_type);
            if ["number", "int", "money", "percent"].contains(&kind) && actual_type != "number" {
                return json!({ "ok": false, "stage": "presentation", "error_code": "column_type_mismatch", "columns": inferred, "error": format!("Column '{field}' is not numeric"), "recommended_fix": "Используйте type=text/date либо выберите доступную колонку type=number." });
            }
            columns.push(json!({
                "key": field,
                "label": column.get("label").and_then(Value::as_str).unwrap_or(field),
                "type": kind
            }));
        }
        columns
    };
    let visible_fields: std::collections::HashSet<String> = columns
        .iter()
        .filter_map(|column| {
            column
                .get("key")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .collect();
    if let Some(sort_field) = table
        .get("sort")
        .and_then(Value::as_object)
        .and_then(|sort| sort.get("field").or_else(|| sort.get("key")))
        .and_then(Value::as_str)
    {
        if !visible_fields.contains(sort_field) {
            return json!({
                "ok": false, "stage": "presentation", "error_code": "sort_column_not_found",
                "columns": inferred, "error": format!("Sort column '{sort_field}' is not displayed"),
                "recommended_fix": "Выберите table.sort.field/key из table.columns."
            });
        }
    }
    if let Some(rules) = table.get("conditional_format").and_then(Value::as_array) {
        for rule in rules {
            let field = rule
                .get("column")
                .or_else(|| rule.get("field"))
                .and_then(Value::as_str)
                .unwrap_or_default();
            if !visible_fields.contains(field) {
                return json!({
                    "ok": false, "stage": "presentation", "error_code": "format_column_not_found",
                    "columns": inferred, "error": format!("Conditional format column '{field}' is not displayed"),
                    "recommended_fix": "Ссылайтесь только на показанные table.columns."
                });
            }
            let kind = rule
                .get("kind")
                .and_then(Value::as_str)
                .unwrap_or("threshold");
            if matches!(kind, "threshold" | "dataBar" | "heatmap")
                && inferred_type(field) != Some("number")
            {
                return json!({
                    "ok": false, "stage": "presentation", "error_code": "format_column_type_mismatch",
                    "columns": inferred, "error": format!("Conditional format '{kind}' requires numeric column '{field}'"),
                    "recommended_fix": "Выберите числовую колонку либо удалите правило."
                });
            }
        }
    }
    if let Some(aggregates) = table
        .get("totals")
        .and_then(Value::as_object)
        .and_then(|totals| totals.get("agg"))
        .and_then(Value::as_object)
    {
        for (field, aggregate) in aggregates {
            let operation = aggregate.as_str().unwrap_or_default();
            if !visible_fields.contains(field)
                || !["sum", "avg", "count", "min", "max"].contains(&operation)
                || (operation != "count" && inferred_type(field) != Some("number"))
            {
                return json!({
                    "ok": false, "stage": "presentation", "error_code": "invalid_total",
                    "columns": inferred, "error": format!("Invalid total '{operation}' for column '{field}'"),
                    "recommended_fix": "Totals должны ссылаться на показанную колонку; sum/avg/min/max требуют type=number."
                });
            }
        }
    }
    let spec = json!({
        "title": title,
        "columns": columns,
        "sort": table.get("sort").cloned().unwrap_or(Value::Null),
        "filters": table.get("filters").cloned().unwrap_or_else(|| json!({"global":true,"perColumn":true})),
        "conditionalFormat": table.get("conditional_format").cloned().unwrap_or_else(|| json!([])),
        "totals": table.get("totals").cloned().unwrap_or_else(|| json!({"enabled":false})),
        "pagination": { "enabled": true, "pageSize": table.get("page_size").and_then(Value::as_u64).unwrap_or(50).clamp(1, 200) },
        "export": { "csv": true, "clipboard": true }
    });
    let code = args
        .get("code")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| {
            crate::plugins::data::stable_builder_code("TABLE", chat_id, &title, &source)
        });
    let has_period = crate::plugins::data::source_uses_period_context(&source);
    let params = if has_period {
        vec![
            ParamSpec {
                key: "date_from".into(),
                param_type: ParamType::Date,
                label: "С".into(),
                default_value: effective_context.date_from.clone(),
                required: true,
                global_filter_key: Some("date_from".into()),
            },
            ParamSpec {
                key: "date_to".into(),
                param_type: ParamType::Date,
                label: "По".into(),
                default_value: effective_context.date_to.clone(),
                required: true,
                global_filter_key: Some("date_to".into()),
            },
        ]
    } else {
        vec![]
    };
    let client_script = if has_period {
        table_client_script_with_period(
            &spec.to_string(),
            effective_context.date_from.as_deref().unwrap_or_default(),
            effective_context.date_to.as_deref().unwrap_or_default(),
        )
    } else {
        table_client_script(&spec.to_string())
    };
    let bundle = PluginBundle {
        manifest: PluginManifest {
            code,
            title: title.clone(),
            runtime: PluginRuntime::Client,
            api_version: "2".into(),
            description: Some("Декларативная live/snapshot таблица, созданная из чата.".into()),
            capabilities: vec!["network:none".into()],
            client_kits: Some(vec!["tables".into()]),
            built_for_migration: None,
        },
        params,
        data: DataBinding {
            source: Some(source),
            default_mode: PluginDataMode::Live,
            ..DataBinding::default()
        },
        client_script: Some(client_script),
        server_script: None,
        view_spec: ViewSpec {
            widgets: vec![Widget {
                kind: WidgetKind::Table,
                title: Some(title),
                config: spec,
            }],
            custom_html: None,
        },
        styles: Some(".ptables{padding:4px}.plugin-period{display:flex;gap:10px;align-items:end;flex-wrap:wrap;margin:0 0 12px}.plugin-period label{display:grid;gap:4px;font-size:12px}.plugin-period input{padding:6px;border:1px solid var(--color-border);border-radius:6px;background:var(--color-surface);color:inherit}".into()),
        sql_resources: Default::default(),
        assets: Default::default(),
    };
    let mut result = super::plugin_tools::upsert_bundle_with_snapshot(
        bundle,
        None,
        Some("active".into()),
        Some(true),
        chat_id,
        agent_id,
        capture_snapshot.then(|| Value::Array(tabular.rows.clone())),
        capture_snapshot,
        allow_live_only,
    )
    .await;
    if let Value::Object(map) = &mut result {
        map.insert("columns".into(), Value::Array(inferred));
        map.insert("row_count".into(), json!(tabular.row_count));
        map.insert("data_modes".into(), json!(["live", "snapshot"]));
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use contracts::plugins::PluginBundle;

    #[test]
    fn templates_are_valid_bundles() {
        for kind in ["basic", "financial", "pivot-lite"] {
            let out = table_template(&json!({ "kind": kind }));
            let bundle: PluginBundle = serde_json::from_value(out["bundle"].clone())
                .unwrap_or_else(|e| panic!("template {kind} bundle parse: {e}"));
            bundle
                .validate()
                .unwrap_or_else(|e| panic!("template {kind} invalid: {e}"));
        }
    }

    #[test]
    fn examples_are_valid_bundles() {
        let out = table_examples();
        let arr = out["examples"].as_array().expect("examples array");
        assert_eq!(arr.len(), 2);
        for ex in arr {
            let bundle: PluginBundle =
                serde_json::from_value(ex["bundle"].clone()).expect("example bundle parse");
            bundle.validate().expect("example bundle invalid");
        }
    }

    #[test]
    fn default_kind_is_basic() {
        let out = table_template(&json!({}));
        assert_eq!(out["kind"], json!("basic"));
    }
}
