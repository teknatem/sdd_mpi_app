//! Инструменты агента-построителя графиков (навык `chart-builder`).
//!
//! Канонический `build_chart` создаёт declarative live/snapshot plugin. Старые
//! template/example helpers остаются только для общего `plugin-authoring` workflow.

use super::types::ToolDefinition;
use contracts::plugins::{
    DataBinding, ParamSpec, ParamType, PluginBundle, PluginDataMode, PluginDataSource,
    PluginManifest, PluginRunContext, PluginRuntime, ViewSpec, Widget, WidgetKind,
};
use serde_json::{json, Value};

/// Имена инструментов построителя графиков (для guard'а в диспетчере).
/// `build_chart` НЕ входит сюда: ему нужен async + контекст чата, поэтому он
/// диспетчеризуется отдельной веткой (см. `tool_executor`).
pub const CHART_TOOL_NAMES: &[&str] =
    &["chart_template", "chart_examples", "get_chart_ui_contract"];

/// Клиентский ES-модуль графика: грузит данные и рисует через PluginCharts.
fn chart_client_script(spec_json: &str) -> String {
    format!(
        r##"export async function mount(root, host) {{
  root.innerHTML = '<div class="status">Загрузка…</div>';
  try {{
    const rows = await host.invoke("data", {{}});
    const spec = {spec};
    PluginCharts.render(root, spec, rows);
  }} catch (e) {{
    root.innerHTML = '<div class="status status--error">' + (e && e.message ? e.message : String(e)) + '</div>';
  }}
}}
"##,
        spec = spec_json
    )
}

fn chart_client_script_with_period(spec_json: &str, date_from: &str, date_to: &str) -> String {
    format!(
        r##"export async function mount(root, host) {{
  root.innerHTML = `<div class="plugin-period">
    <label>С <input type="date" data-role="from" value="{date_from}"></label>
    <label>По <input type="date" data-role="to" value="{date_to}"></label>
    <button type="button" class="btn btn--secondary" data-role="apply">Применить</button>
  </div><div data-role="chart"><div class="status">Загрузка…</div></div>`;
  const chartRoot = root.querySelector('[data-role="chart"]');
  const from = root.querySelector('[data-role="from"]');
  const to = root.querySelector('[data-role="to"]');
  const apply = root.querySelector('[data-role="apply"]');
  const snapshotMode = host.context && host.context.params && host.context.params._plugin_data_mode === 'snapshot';
  if (snapshotMode) {{ from.disabled = true; to.disabled = true; apply.disabled = true; apply.title = 'Снимок содержит период публикации'; }}
  const spec = {spec};
  async function load() {{
    if (!from.value || !to.value || from.value > to.value) {{
      chartRoot.innerHTML = '<div class="status status--error">Проверьте выбранный период</div>';
      return;
    }}
    apply.disabled = true;
    chartRoot.innerHTML = '<div class="status">Загрузка…</div>';
    try {{
      const rows = await host.invoke("data", {{ date_from: from.value, date_to: to.value }});
      PluginCharts.render(chartRoot, spec, rows);
    }} catch (e) {{
      chartRoot.innerHTML = '<div class="status status--error">' + (e && e.message ? e.message : String(e)) + '</div>';
    }} finally {{
      apply.disabled = snapshotMode;
    }}
  }}
  if (!snapshotMode) apply.addEventListener('click', load);
  await load();
}}
"##,
        spec = spec_json,
        date_from = date_from,
        date_to = date_to,
    )
}

/// Серверный метод без параметров (демо-SQL).
const CHART_SERVER: &str = r##"export async function data(args, host) {
  return await host.db.queryResource("series", []);
}
"##;

/// Серверный метод с периодом из контекста (для примеров на реальных таблицах).
const CHART_SERVER_CTX: &str = r##"export async function data(args, host) {
  const c = host.context || {};
  const from = c.date_from || "1970-01-01";
  const to = c.date_to || "2999-12-31";
  return await host.db.queryResource("series", [from, to]);
}
"##;

/// Собрать bundle-график. `with_ctx` — серверный метод читает период из контекста.
fn build_bundle(
    code: &str,
    title: &str,
    description: &str,
    capabilities: Value,
    spec: Value,
    sql: &str,
    with_ctx: bool,
) -> Value {
    json!({
        "manifest": {
            "code": code,
            "title": title,
            "runtime": "hybrid",
            "api_version": "2",
            "description": description,
            "capabilities": capabilities,
        },
        "client_script": chart_client_script(&spec.to_string()),
        "server_script": if with_ctx { CHART_SERVER_CTX } else { CHART_SERVER },
        "sql_resources": { "series": sql },
        "styles": ".pcharts{padding:8px;}",
    })
}

// ─── Демо-SQL без зависимости от схемы (шаблон сразу валиден и запускается) ───

const DEMO_TS_SQL: &str = "SELECT '2024-05-01' AS d, 1200 AS revenue UNION ALL SELECT '2024-05-02', 1450 UNION ALL SELECT '2024-05-03', 1310 UNION ALL SELECT '2024-05-04', 1680";
const DEMO_BAR_SQL: &str =
    "SELECT 'WB' AS name, 5200 AS value UNION ALL SELECT 'OZON', 3100 UNION ALL SELECT 'YM', 1800";
const DEMO_PIE_SQL: &str = "SELECT 'Одежда' AS name, 5200 AS value UNION ALL SELECT 'Обувь', 3100 UNION ALL SELECT 'Аксессуары', 1800";

/// Минимальный валидный bundle-график по типу (line | bar | pie).
fn chart_template(args: &Value) -> Value {
    let kind = args.get("type").and_then(Value::as_str).unwrap_or("line");
    let caps = json!(["network:none"]);
    let bundle = match kind {
        "bar" => build_bundle(
            "MY-CHART-BAR",
            "Мой график (столбцы)",
            "Шаблон bar-графика: категория + мера.",
            caps,
            json!({
                "type": "bar", "title": "Заголовок", "x": "name",
                "series": [{ "y": "value", "label": "Значение" }],
                "format": "money", "alternatives": ["stacked-bar", "line"]
            }),
            DEMO_BAR_SQL,
            false,
        ),
        "pie" => build_bundle(
            "MY-CHART-PIE",
            "Мой график (доли)",
            "Шаблон круговой/кольцевой диаграммы: доля от целого.",
            caps,
            json!({
                "type": "doughnut", "title": "Заголовок",
                "category": "name", "value": "value", "format": "money"
            }),
            DEMO_PIE_SQL,
            false,
        ),
        _ => build_bundle(
            "MY-CHART-LINE",
            "Мой график (линия)",
            "Шаблон time-series: дата/период + мера(ы).",
            caps,
            json!({
                "type": "line", "title": "Заголовок", "x": "d",
                "series": [{ "y": "revenue", "label": "Выручка" }],
                "format": "money", "alternatives": ["area", "bar"]
            }),
            DEMO_TS_SQL,
            false,
        ),
    };
    json!({
        "bundle": bundle,
        "type": kind,
        "hint": "Минимальный валидный график. Замени code/title и SQL в sql_resources на реальный SELECT \
                 (проверь через execute_query), синхронизируй имена колонок в chart-spec (x/series.y или \
                 category/value), затем plugin_smoke_test → plugin_upsert(status=active)."
    })
}

/// Готовые рабочие примеры на реальной таблице a012_wb_sales (db:read:wb).
fn chart_examples() -> Value {
    let wb_caps = json!(["db:read:wb", "network:none"]);

    let time_series = build_bundle(
        "EXAMPLE-CHART-REVENUE-TS",
        "Пример: выручка WB по дням",
        "Time-series: сумма продаж по дням за период из контекста.",
        wb_caps.clone(),
        json!({
            "type": "line", "title": "Выручка WB по дням", "x": "d",
            "series": [{ "y": "revenue", "label": "Выручка" }],
            "format": "money", "alternatives": ["area", "bar"]
        }),
        "SELECT substr(sale_date, 1, 10) AS d, SUM(total_price) AS revenue FROM a012_wb_sales WHERE is_deleted = 0 AND substr(sale_date, 1, 10) BETWEEN ? AND ? GROUP BY substr(sale_date, 1, 10) ORDER BY d",
        true,
    );

    let bar = build_bundle(
        "EXAMPLE-CHART-TOP-ARTICLES",
        "Пример: топ артикулов WB по выручке",
        "Bar: 10 артикулов с наибольшей выручкой за период.",
        wb_caps.clone(),
        json!({
            "type": "bar", "title": "Топ-10 артикулов по выручке", "x": "name",
            "series": [{ "y": "value", "label": "Выручка" }],
            "format": "money", "horizontal": true, "alternatives": ["stacked-bar"]
        }),
        "SELECT supplier_article AS name, SUM(total_price) AS value FROM a012_wb_sales WHERE is_deleted = 0 AND substr(sale_date, 1, 10) BETWEEN ? AND ? GROUP BY supplier_article ORDER BY value DESC LIMIT 10",
        true,
    );

    let pie = build_bundle(
        "EXAMPLE-CHART-ARTICLE-SHARE",
        "Пример: доля топ-артикулов в выручке",
        "Doughnut: доля топ-6 артикулов в общей выручке за период.",
        wb_caps,
        json!({
            "type": "doughnut", "title": "Доля артикулов в выручке",
            "category": "name", "value": "value", "format": "money"
        }),
        "SELECT supplier_article AS name, SUM(total_price) AS value FROM a012_wb_sales WHERE is_deleted = 0 AND substr(sale_date, 1, 10) BETWEEN ? AND ? GROUP BY supplier_article ORDER BY value DESC LIMIT 6",
        true,
    );

    json!({
        "examples": [
            { "title": "Time-series: выручка по дням", "bundle": time_series },
            { "title": "Bar: топ артикулов", "bundle": bar },
            { "title": "Doughnut: доля артикулов", "bundle": pie }
        ],
        "hint": "Скопируй ближайший по форме данных пример, замени таблицу/колонки на свои (проверь SELECT \
                 через execute_query), выровняй chart-spec, затем plugin_smoke_test → plugin_upsert."
    })
}

/// Контракт рантайма графиков: как звать PluginCharts.render и какой chart-spec.
fn chart_ui_contract() -> Value {
    json!({
        "runtime": "В iframe доступны глобали window.Chart (Chart.js) и window.PluginCharts. \
                    В client_script внутри mount(root, host): const rows = await host.invoke(\"data\", {}); \
                    PluginCharts.render(root, spec, rows).",
        "server": "Серверный метод data(args, host) возвращает массив объектов-строк через \
                   host.db.queryResource(\"series\", params). Период бери из host.context.date_from/date_to.",
        "types": ["line", "area", "bar", "stacked-bar", "pie", "doughnut"],
        "spec_cartesian": {
            "type": "line|area|bar|stacked-bar",
            "title": "string",
            "x": "имя колонки оси категорий/времени",
            "series": [{ "y": "имя колонки меры", "label": "подпись" }],
            "stacked": "bool (для накопления)",
            "horizontal": "bool (горизонтальные столбцы / top-N)",
            "format": "money|int|percent|number",
            "alternatives": ["типы для чипов-переключателя у пользователя"]
        },
        "spec_pie": {
            "type": "pie|doughnut",
            "title": "string",
            "category": "колонка-категория (метки)",
            "value": "колонка-мера (значения)",
            "format": "money|int|percent|number"
        },
        "rules": [
            "Тема (свет/тёмная) и цвета осей подхватываются автоматически — НЕ хардкодь цвета.",
            "Имена колонок в chart-spec (x/series.y или category/value) должны совпадать с алиасами в SELECT.",
            "alternatives рисует чипы для мгновенной смены типа пользователем — перечисли совместимые типы.",
            "Свой CSS — по минимуму; PluginCharts сам форматирует оси/легенду/тултипы."
        ]
    })
}

/// Допустимые типы графика — единый источник правды для guard'а и презентационной валидации.
pub const CHART_TYPES: &[&str] = &["bar", "line", "area", "stacked-bar", "pie", "doughnut"];

/// Чистая презентационная валидация графика поверх фактических колонок/строк источника.
///
/// Один и тот же гейт используют `preview_data` (для `build_ready`) и `build_chart`
/// (перед публикацией), поэтому удачный preview больше не может разойтись с отказом
/// build по презентации. Возвращает готовый chart-spec либо структурированную ошибку
/// (без поля `ok` — его добавляет вызывающий).
pub fn validate_chart_presentation(
    chart: &Value,
    columns: &[String],
    rows: &[Value],
    title: &str,
) -> Result<Value, Value> {
    let chart_type = chart.get("type").and_then(Value::as_str).unwrap_or("bar");
    if !CHART_TYPES.contains(&chart_type) {
        return Err(
            json!({ "stage": "presentation", "error_code": "invalid_chart_type", "error": format!("Unsupported chart type: {chart_type}") }),
        );
    }
    let inferred = crate::plugins::data::infer_columns(rows, columns);
    let is_numeric = |name: &str| {
        inferred.iter().any(|column| {
            column.get("name").and_then(Value::as_str) == Some(name)
                && column.get("type").and_then(Value::as_str) == Some("number")
        })
    };
    let requested_category = chart.get("category").and_then(Value::as_str);
    if let Some(field) = requested_category {
        if !columns.iter().any(|column| column == field) {
            return Err(
                json!({ "stage": "presentation", "error_code": "category_not_found", "columns": inferred, "error": format!("Category '{field}' was not found"), "recommended_fix": "Выберите chart.category из доступных колонок." }),
            );
        }
    }
    let category = requested_category.map(str::to_string).or_else(|| {
        columns
            .iter()
            .find(|name| !is_numeric(name))
            .cloned()
            .or_else(|| columns.first().cloned())
    });
    let Some(category) = category else {
        return Err(
            json!({ "stage": "presentation", "error_code": "category_not_found", "columns": inferred, "error": "Category column was not found", "recommended_fix": "Выберите chart.category из доступных колонок." }),
        );
    };
    let mut series = chart
        .get("series")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if series.is_empty() {
        if let Some(field) = columns
            .iter()
            .find(|name| name.as_str() != category && is_numeric(name))
        {
            series.push(json!({ "field": field, "label": field }));
        }
    }
    let mut normalized_series = Vec::new();
    for item in series {
        let field = item
            .get("field")
            .or_else(|| item.get("y"))
            .and_then(Value::as_str)
            .unwrap_or_default();
        if !is_numeric(field) {
            return Err(json!({
                "stage": "presentation", "error_code": "measure_not_numeric",
                "columns": inferred, "error": format!("Measure '{field}' is absent or not numeric"),
                "recommended_fix": "Выберите series[].field из доступных колонок типа number или исправьте source alias/aggregation."
            }));
        }
        normalized_series.push(json!({
            "y": field,
            "label": item.get("label").and_then(Value::as_str).unwrap_or(field)
        }));
    }
    if normalized_series.is_empty() {
        return Err(
            json!({ "stage": "presentation", "error_code": "measure_not_found", "columns": inferred, "error": "Numeric measure was not found", "recommended_fix": "Добавьте в source числовую меру и снова вызовите preview_data." }),
        );
    }
    let format = chart
        .get("format")
        .and_then(Value::as_str)
        .unwrap_or("number");
    if !["money", "int", "percent", "number"].contains(&format) {
        return Err(json!({
            "stage": "presentation", "error_code": "invalid_format",
            "columns": inferred, "error": format!("Unsupported format '{format}'"),
            "recommended_fix": "Используйте money, int, percent или number."
        }));
    }
    let alternatives = chart
        .get("alternatives")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if alternatives.iter().any(|value| {
        !value
            .as_str()
            .is_some_and(|kind| CHART_TYPES.contains(&kind))
    }) {
        return Err(json!({
            "stage": "presentation", "error_code": "invalid_alternative",
            "columns": inferred, "error": "Unsupported chart alternative",
            "recommended_fix": "Используйте только bar, line, area, stacked-bar, pie, doughnut."
        }));
    }
    let is_pie = matches!(chart_type, "pie" | "doughnut");
    let spec = if is_pie {
        json!({
            "type": chart_type, "title": title, "category": category,
            "value": normalized_series[0]["y"],
            "format": format,
            "alternatives": alternatives
        })
    } else {
        json!({
            "type": chart_type, "title": title, "x": category, "series": normalized_series,
            "format": format,
            "horizontal": chart.get("horizontal").and_then(Value::as_bool).unwrap_or(false),
            "alternatives": alternatives
        })
    };
    Ok(spec)
}

/// Определения инструментов построителя графиков.
pub fn chart_tool_definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "build_chart".into(),
            description: "Построить и опубликовать live/snapshot график по source из preview_data. \
                          Сервер повторно проверяет фактические колонки и типы, presentation contract, \
                          снимок, plugin/revision и единственную карточку чата в одной транзакции."
                .into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "source": super::data_tools::plugin_data_source_schema(),
                    "context": { "type":"object", "description":"Начальные date_from/date_to и params для $context bindings; по умолчанию последние 30 дней.", "properties": { "date_from":{"type":"string"}, "date_to":{"type":"string"}, "connection_mp_refs":{"type":"array","items":{"type":"string"}}, "params":{"type":"object","additionalProperties":{"type":"string"}} } },
                    "title": { "type": "string", "description": "Заголовок графика." },
                    "chart": {
                        "type": "object",
                        "description": "Presentation spec; поля должны совпадать с preview_data.columns.",
                        "properties": {
                            "type": { "type": "string", "enum": ["bar", "line", "area", "stacked-bar", "pie", "doughnut"] },
                            "category": { "type": "string" },
                            "series": { "type": "array", "items": { "type": "object", "properties": { "field": {"type":"string"}, "label": {"type":"string"} }, "required": ["field"] } },
                            "format": { "type": "string", "enum": ["money", "int", "percent", "number"] },
                            "horizontal": { "type": "boolean" },
                            "alternatives": { "type": "array", "items": { "type": "string" } }
                        }
                    },
                    "code": { "type": "string", "description": "Необязательный стабильный код плагина (для обновления существующего графика)." },
                    "snapshot": {
                        "type": "object",
                        "properties": {
                            "capture": { "type": "boolean", "default": true },
                            "allow_live_only": { "type": "boolean", "default": false }
                        },
                        "description": "По умолчанию снимок обязателен. Для live-only нужны capture=false и явный allow_live_only=true."
                    }
                },
                "required": ["source", "title", "chart"]
            }),
        },
        ToolDefinition {
            name: "chart_template".into(),
            description: "Минимальный ВАЛИДНЫЙ скелет графика-плагина (hybrid) по типу: \
                          line | bar | pie. Возвращает { bundle, hint } с готовыми client/server \
                          скриптами и демо-SQL. Начинай новый график с шаблона."
                .into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "type": { "type": "string", "enum": ["line", "bar", "pie"], "description": "Базовый тип графика. По умолчанию line." }
                }
            }),
        },
        ToolDefinition {
            name: "chart_examples".into(),
            description: "Готовые рабочие примеры графиков (time-series / bar / doughnut) на реальной \
                          таблице a012_wb_sales — образец структуры, SQL и chart-spec. \
                          Возвращает { examples:[{ title, bundle }] }."
                .into(),
            parameters: json!({ "type": "object", "properties": {} }),
        },
        ToolDefinition {
            name: "get_chart_ui_contract".into(),
            description: "Контракт рантайма графиков: как звать PluginCharts.render, форма chart-spec \
                          (картезианские типы и pie/doughnut), доступные типы и правила темы/альтернатив."
                .into(),
            parameters: json!({ "type": "object", "properties": {} }),
        },
    ]
}

/// Диспетчер инструментов построителя графиков (без БД — чистые заготовки).
pub fn execute_chart_tool(name: &str, arguments: &str) -> Value {
    let args: Value = serde_json::from_str(arguments).unwrap_or_else(|_| json!({}));
    match name {
        "chart_template" => chart_template(&args),
        "chart_examples" => chart_examples(),
        "get_chart_ui_contract" => chart_ui_contract(),
        _ => json!({ "error": format!("Unknown chart tool: '{}'", name) }),
    }
}

/// Канонический builder принимает только единый declarative source contract.
/// Raw SQL остаётся штатным путём через `source.kind = "sql"`.
pub async fn execute_build_chart(arguments: &str, chat_id: &str, agent_id: &str) -> Value {
    let args: Value = serde_json::from_str(arguments).unwrap_or_else(|_| json!({}));
    execute_declarative_chart(&args, chat_id, agent_id).await
}

async fn execute_declarative_chart(args: &Value, chat_id: &str, agent_id: &str) -> Value {
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
        .unwrap_or("График")
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
    let chart = args.get("chart").cloned().unwrap_or_else(|| json!({}));
    let chart_type = chart.get("type").and_then(Value::as_str).unwrap_or("bar");
    if !CHART_TYPES.contains(&chart_type) {
        return json!({ "ok": false, "stage": "presentation", "error_code": "invalid_chart_type", "error": format!("Unsupported chart type: {chart_type}") });
    }
    let tabular = match crate::plugins::data::execute_source_with_context(
        &source,
        crate::plugins::data::CHART_ROW_LIMIT,
        Some(&effective_context),
    )
    .await
    {
        Ok(result) => result,
        Err(error) => {
            return json!({ "ok": false, "stage": "source", "error_code": "query_failed", "error": error, "recommended_fix": "Исправьте source и повторите preview_data; затем передайте тот же source в build_chart." })
        }
    };
    if tabular.truncated {
        return json!({
            "ok": false, "stage": "snapshot", "error_code": "snapshot_limit_exceeded",
            "error": format!("Chart source exceeds {} rows; aggregate or limit the source", crate::plugins::data::CHART_ROW_LIMIT)
        });
    }
    if tabular.rows.is_empty() {
        return json!({ "ok": false, "stage": "source", "error_code": "empty_result", "error": "Source returned no rows", "recommended_fix": "Расширьте период или ослабьте фильтры source." });
    }
    if capture_snapshot {
        if let Err(error) = crate::plugins::data::validate_snapshot_payload(
            &Value::Array(tabular.rows.clone()),
            crate::plugins::data::CHART_ROW_LIMIT,
        ) {
            return json!({
                "ok": false, "stage": "snapshot", "error_code": "snapshot_limit_exceeded",
                "error": error, "recommended_fix": "Сократите результат source: добавьте агрегацию, фильтр или LIMIT."
            });
        }
    }
    // Единый презентационный гейт: те же проверки выполняет preview_data для build_ready,
    // поэтому удачный preview гарантирует, что build не упадёт на презентации.
    let spec = match validate_chart_presentation(&chart, &tabular.columns, &tabular.rows, &title) {
        Ok(spec) => spec,
        Err(mut error) => {
            if let Value::Object(map) = &mut error {
                map.insert("ok".into(), json!(false));
            }
            return error;
        }
    };
    let inferred = crate::plugins::data::infer_columns(&tabular.rows, &tabular.columns);
    let code = args
        .get("code")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| {
            crate::plugins::data::stable_builder_code("CHART", chat_id, &title, &source)
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
        chart_client_script_with_period(
            &spec.to_string(),
            effective_context.date_from.as_deref().unwrap_or_default(),
            effective_context.date_to.as_deref().unwrap_or_default(),
        )
    } else {
        chart_client_script(&spec.to_string())
    };
    let bundle = PluginBundle {
        manifest: PluginManifest {
            code,
            title: title.clone(),
            runtime: PluginRuntime::Client,
            api_version: "2".into(),
            description: Some("Декларативный live/snapshot график, созданный из чата.".into()),
            capabilities: vec!["network:none".into()],
            client_kits: Some(vec!["charts".into()]),
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
                kind: WidgetKind::Chart,
                title: Some(title),
                config: spec,
            }],
            custom_html: None,
        },
        styles: Some(".pcharts{padding:8px}.plugin-period{display:flex;gap:10px;align-items:end;flex-wrap:wrap;margin:0 0 12px}.plugin-period label{display:grid;gap:4px;font-size:12px}.plugin-period input{padding:6px;border:1px solid var(--color-border);border-radius:6px;background:var(--color-surface);color:inherit}".into()),
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
        for kind in ["line", "bar", "pie"] {
            let out = chart_template(&json!({ "type": kind }));
            let bundle: PluginBundle = serde_json::from_value(out["bundle"].clone())
                .unwrap_or_else(|e| panic!("template {kind} bundle parse: {e}"));
            bundle
                .validate()
                .unwrap_or_else(|e| panic!("template {kind} invalid: {e}"));
        }
    }

    #[test]
    fn examples_are_valid_bundles() {
        let out = chart_examples();
        let arr = out["examples"].as_array().expect("examples array");
        assert_eq!(arr.len(), 3);
        for ex in arr {
            let bundle: PluginBundle =
                serde_json::from_value(ex["bundle"].clone()).expect("example bundle parse");
            bundle.validate().expect("example bundle invalid");
        }
    }

    #[test]
    fn default_type_is_line() {
        let out = chart_template(&json!({}));
        assert_eq!(out["type"], json!("line"));
    }

    /// Регресс на чат ef926556: численная колонка-категория `weekday_num` из raw SQL
    /// должна приниматься презентационным гейтом (а не падать category_not_found).
    #[test]
    fn validate_presentation_accepts_numeric_weekday_category() {
        let rows = vec![
            json!({ "weekday_num": 1, "sales_amount": 2502331.41, "sales_qty": 185.0 }),
            json!({ "weekday_num": 2, "sales_amount": 1975821.96, "sales_qty": 156.0 }),
        ];
        let columns = vec![
            "weekday_num".to_string(),
            "sales_amount".to_string(),
            "sales_qty".to_string(),
        ];
        let chart = json!({
            "type": "bar", "category": "weekday_num",
            "series": [{ "field": "sales_amount", "label": "Сумма продаж" }],
            "format": "money"
        });
        let spec =
            validate_chart_presentation(&chart, &columns, &rows, "Продажи WB по дням недели")
                .expect("weekday_num must be a valid category");
        assert_eq!(spec["type"], json!("bar"));
        assert_eq!(spec["x"], json!("weekday_num"));
        assert_eq!(spec["series"][0]["y"], json!("sales_amount"));
    }

    /// Несуществующая категория отклоняется и возвращает фактический список колонок,
    /// чтобы модель могла самоисправиться (выбрать category из `columns`).
    #[test]
    fn validate_presentation_rejects_unknown_category_with_columns() {
        let rows = vec![json!({ "weekday_num": 1, "sales_amount": 100.0 })];
        let columns = vec!["weekday_num".to_string(), "sales_amount".to_string()];
        let chart = json!({
            "type": "bar", "category": "weekday",
            "series": [{ "field": "sales_amount" }]
        });
        let error = validate_chart_presentation(&chart, &columns, &rows, "t")
            .expect_err("unknown category must fail");
        assert_eq!(error["error_code"], json!("category_not_found"));
        // Гейт без поля ok — его добавляет вызывающий (build_chart).
        assert!(error.get("ok").is_none());
        let names: Vec<&str> = error["columns"]
            .as_array()
            .expect("columns array")
            .iter()
            .filter_map(|column| column["name"].as_str())
            .collect();
        assert!(names.contains(&"weekday_num"));
    }
}
