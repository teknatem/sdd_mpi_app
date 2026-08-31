use super::types::ToolDefinition;
use crate::shared::data_access::{
    execute_raw_query, list_sources, query_schema, run_data_view_drilldown, run_data_view_scalar,
    DataSourceKind, DataViewDrilldownRequest, DataViewScalarRequest, RawQueryRequest,
    SchemaQueryRequest, SqlAccessProfile,
};
use contracts::domain::a017_llm_agent::aggregate::AgentType;
use contracts::plugins::PluginDataSource;
use serde::Deserialize;
use serde_json::{json, Value};

pub const DATA_TOOL_NAMES: &[&str] = &[
    "find_data_sources",
    "preview_data",
    "list_data_sources",
    "query_data_schema",
    "run_data_view_scalar",
    "run_data_view_drilldown",
    "execute_query",
];

/// Exact tagged-union schema shared by preview/build tools. Keeping this in one
/// place prevents providers from treating `source` as an arbitrary object or a
/// JSON-encoded string.
pub(crate) fn plugin_data_source_schema() -> Value {
    let filter = json!({
        "type": "object",
        "properties": {
            "field_id": { "type": "string" },
            "operator": { "type": "string", "enum": ["eq", "not_eq", "lt", "lte", "gt", "gte", "between", "in", "not_in", "contains", "is_null", "is_not_null"] },
            "value": { "description": "Scalar literal or $context.<field> binding." },
            "values": { "type": "array", "items": {} },
            "from": { "description": "Scalar literal or $context.date_from." },
            "to": { "description": "Scalar literal or $context.date_to." }
        },
        "required": ["field_id", "operator"],
        "additionalProperties": false
    });
    json!({
        "oneOf": [
            {
                "type": "object",
                "properties": {
                    "kind": { "type": "string", "enum": ["schema"] },
                    "schema_id": { "type": "string" },
                    "fields": { "type": "array", "items": { "type": "string" } },
                    "group_by": { "type": "array", "items": { "type": "string" } },
                    "metrics": { "type": "array", "items": { "type": "object", "properties": { "field_id": {"type":"string"}, "aggregate": {"type":"string", "enum":["sum","count","avg","min","max"]} }, "required":["field_id","aggregate"], "additionalProperties":false } },
                    "filters": { "type": "array", "items": filter },
                    "sort": { "type": "array", "items": { "type":"object", "properties": { "field_id":{"type":"string"}, "direction":{"type":"string", "enum":["asc","desc"]} }, "required":["field_id","direction"], "additionalProperties":false } }
                },
                "required": ["kind", "schema_id"],
                "additionalProperties": false
            },
            {
                "type": "object",
                "properties": {
                    "kind": { "type": "string", "enum": ["dataview"] },
                    "view_id": { "type": "string" },
                    "metric_ids": { "type": "array", "items": { "type":"string" }, "minItems":1 },
                    "group_by": { "type": "string" },
                    "context": {
                        "type": "object",
                        "properties": {
                            "date_from": {"type":"string"}, "date_to": {"type":"string"},
                            "period2_from": {"type":"string"}, "period2_to": {"type":"string"},
                            "connection_mp_refs": {"type":"array", "items":{"type":"string"}},
                            "params": {"type":"object", "additionalProperties":{"type":"string"}}
                        },
                        "required": ["date_from", "date_to"]
                    }
                },
                "required": ["kind", "view_id", "metric_ids", "group_by", "context"],
                "additionalProperties": false
            },
            {
                "type": "object",
                "properties": {
                    "kind": { "type": "string", "enum": ["sql"] },
                    "sql": { "type": "string", "description": "One SQLite SELECT/WITH. Use ? placeholders, never :named params or PostgreSQL functions." },
                    "params": { "type": "array", "items": { "description": "Scalar literal or $context.date_from/$context.date_to." } }
                },
                "required": ["kind", "sql", "params"],
                "additionalProperties": false
            }
        ]
    })
}

pub fn tool_definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "find_data_sources".into(),
            description: "Найти до нескольких подходящих DataView/base-схем по тексту задачи. Возвращает компактные capabilities; raw_sql всегда доступен как fallback.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Что требуется найти: сущность, метрика, маркетплейс или отчет." },
                    "kind": { "type": "string", "enum": ["base", "dataview", "raw"] },
                    "limit": { "type": "integer", "minimum": 1, "maximum": 10, "default": 5 }
                },
                "required": ["query"]
            }),
        },
        ToolDefinition {
            name: "preview_data".into(),
            description: "Проверить единый источник данных БЕЗ сохранения SQL-артефакта. source.kind: schema (schema_id + fields/group_by/metrics/filters/sort), dataview (view_id + metric_ids + group_by + context) или sql (sql + params). Возвращает реальные строки, колонки и определенные типы. Передай намеченный `chart`, чтобы получить build_ready: тот же презентационный гейт, что и у build_chart, ещё ДО публикации.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "source": plugin_data_source_schema(),
                    "context": {
                        "type": "object",
                        "description": "Значения для $context bindings; если период не задан, используется последние 30 дней.",
                        "properties": { "date_from":{"type":"string"}, "date_to":{"type":"string"}, "connection_mp_refs":{"type":"array","items":{"type":"string"}}, "params":{"type":"object","additionalProperties":{"type":"string"}} }
                    },
                    "limit": { "type": "integer", "minimum": 1, "maximum": 200, "default": 50 },
                    "chart": {
                        "type": "object",
                        "description": "Необязательный presentation-спек для проверки build_ready ДО build_chart: те же поля, что у build_chart.chart, плюс title. Если задан — в ответе появятся build_ready и (при провале) presentation с error_code/columns.",
                        "properties": {
                            "type": { "type": "string", "enum": ["bar", "line", "area", "stacked-bar", "pie", "doughnut"] },
                            "category": { "type": "string" },
                            "series": { "type": "array", "items": { "type": "object", "properties": { "field": {"type":"string"}, "label": {"type":"string"} }, "required": ["field"] } },
                            "format": { "type": "string", "enum": ["money", "int", "percent", "number"] },
                            "horizontal": { "type": "boolean" },
                            "alternatives": { "type": "array", "items": { "type": "string" } },
                            "title": { "type": "string" }
                        }
                    }
                },
                "required": ["source"]
            }),
        },
        ToolDefinition {
            name: "list_data_sources".into(),
            description: "Единый каталог аналитических источников: безопасные base-схемы, \
                          курируемые DataView и Raw SQL fallback. Вызывай первым, затем выбирай: \
                          DataView для официальной метрики/2 периодов, base для ad-hoc среза, \
                          execute_query только если первые два пути не подходят."
                .into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "kind": {
                        "type": "string",
                        "enum": ["base", "dataview", "raw"],
                        "description": "Необязательный фильтр вида источника."
                    }
                }
            }),
        },
        ToolDefinition {
            name: "query_data_schema".into(),
            description: "Безопасно получить строки из base-схемы без написания SQL. \
                          Поля, группировки, метрики и фильтры проверяются по allowlist схемы; \
                          значения передаются bind-параметрами."
                .into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "schema_id": { "type": "string", "description": "schema_id из list_data_sources целиком, например ds03_p904_sales или a006 — НЕ короткий индекс get_entity_schema (a004) и не имя таблицы." },
                    "fields": { "type": "array", "items": { "type": "string" }, "description": "Обычные поля результата без агрегации." },
                    "group_by": { "type": "array", "items": { "type": "string" }, "description": "Измерения группировки." },
                    "metrics": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "field_id": { "type": "string" },
                                "aggregate": { "type": "string", "enum": ["sum", "count", "avg", "min", "max"] }
                            },
                            "required": ["field_id", "aggregate"]
                        }
                    },
                    "filters": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "field_id": { "type": "string" },
                                "operator": { "type": "string", "enum": ["eq", "not_eq", "lt", "lte", "gt", "gte", "between", "in", "not_in", "contains", "is_null", "is_not_null"] },
                                "value": {},
                                "values": { "type": "array", "items": {} },
                                "from": {},
                                "to": {}
                            },
                            "required": ["field_id", "operator"]
                        }
                    },
                    "sort": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "field_id": { "type": "string" },
                                "direction": { "type": "string", "enum": ["asc", "desc"] }
                            },
                            "required": ["field_id", "direction"]
                        }
                    },
                    "limit": { "type": "integer", "minimum": 1, "maximum": 200, "default": 50 }
                },
                "required": ["schema_id"]
            }),
        },
        data_view_definition("run_data_view_scalar", false),
        data_view_definition("run_data_view_drilldown", true),
        ToolDefinition {
            name: "execute_query".into(),
            description: "Raw SQL fallback: один SQLite SELECT/WITH с AST-проверкой, профилем \
                          доступа, bind-параметрами, таймаутом и лимитом. Не читает таблицы \
                          подключений с credentials. Используй только когда DataView и base-схемы \
                          не покрывают запрос. Строк возвращается не больше 50 по умолчанию \
                          (явный LIMIT в SQL поднимает планку до 200) — если в ответе пришло \
                          truncated: true, выборка НЕполная: агрегируй запрос, а не достраивай \
                          выводы по обрезку."
                .into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "sql": { "type": "string", "description": "SELECT с ? placeholders для значений." },
                    "params": { "type": "array", "items": {}, "description": "Скалярные bind-параметры в порядке placeholders." },
                    "description": { "type": "string", "description": "Название сохраняемого SQL-артефакта." },
                    "limit": { "type": "integer", "minimum": 1, "maximum": 200, "default": 50 }
                },
                "required": ["sql", "description"]
            }),
        },
    ]
}

fn data_view_definition(name: &str, drilldown: bool) -> ToolDefinition {
    let mut properties = json!({
        "view_id": { "type": "string", "description": "DataView ID из list_data_sources." },
        "date_from": { "type": "string", "description": "YYYY-MM-DD" },
        "date_to": { "type": "string", "description": "YYYY-MM-DD" },
        "period2_from": { "type": "string", "description": "YYYY-MM-DD; указывать парой с period2_to." },
        "period2_to": { "type": "string", "description": "YYYY-MM-DD; указывать парой с period2_from." },
        "connection_mp_refs": { "type": "array", "items": { "type": "string" } },
        "params": { "type": "object", "additionalProperties": { "type": "string" } }
    });
    let required = if drilldown {
        properties["group_by"] = json!({ "type": "string" });
        properties["metric_ids"] = json!({
            "type": "array",
            "items": { "type": "string" },
            "minItems": 1
        });
        json!(["view_id", "group_by", "metric_ids", "date_from", "date_to"])
    } else {
        properties["metric_id"] = json!({ "type": "string" });
        json!(["view_id", "metric_id", "date_from", "date_to"])
    };
    ToolDefinition {
        name: name.to_string(),
        description: if drilldown {
            "Получить строки курируемого DataView по одному измерению и одной или нескольким метрикам."
        } else {
            "Вычислить курируемую метрику DataView со сравнением периодов."
        }
        .to_string(),
        parameters: json!({
            "type": "object",
            "properties": properties,
            "required": required
        }),
    }
}

#[derive(Deserialize)]
struct ListSourcesArgs {
    #[serde(default)]
    kind: Option<DataSourceKind>,
}

#[derive(Deserialize)]
struct FindSourcesArgs {
    query: String,
    #[serde(default)]
    kind: Option<DataSourceKind>,
    #[serde(default)]
    limit: Option<usize>,
}

#[derive(Deserialize)]
struct PreviewDataArgs {
    source: PluginDataSource,
    #[serde(default)]
    limit: Option<usize>,
    #[serde(default)]
    context: contracts::plugins::PluginRunContext,
    /// Необязательный chart-спек: если задан, preview прогоняет тот же презентационный
    /// гейт, что и build_chart, и возвращает `build_ready` + диагностику до публикации.
    #[serde(default)]
    chart: Option<Value>,
}

#[derive(Deserialize)]
struct ExecuteQueryArgs {
    sql: String,
    description: String,
    #[serde(default)]
    params: Vec<Value>,
    #[serde(default)]
    limit: Option<usize>,
}

fn access_profile(agent_type: &AgentType) -> Result<SqlAccessProfile, String> {
    match agent_type {
        AgentType::BusinessAnalyst
        | AgentType::PluginAdmin
        | AgentType::SalesAnalyst
        | AgentType::Marketer
        | AgentType::Financier
        // Тестировщику read-only SQL нужен, чтобы проверять данные; объём его работы
        // ограничивается матрицей навыков, а не запретом инструмента.
        | AgentType::Tester => Ok(SqlAccessProfile::Analytics),
        AgentType::KbAdmin => Ok(SqlAccessProfile::KnowledgeBase),
        AgentType::CoordinatorAdmin => Ok(SqlAccessProfile::General),
        AgentType::SystemAdmin => Err("execute_query is not available to SystemAdmin".to_string()),
    }
}

fn parse_args<T: for<'de> Deserialize<'de>>(arguments: &str) -> Result<T, String> {
    serde_json::from_str(arguments).map_err(|error| format!("Invalid tool arguments: {error}"))
}

/// Флаг `truncated: true` модели легко пропустить — она видит таблицу с данными и делает по ней
/// выводы, не зная, что часть строк не вернулась. Поэтому усечение проговаривается словами:
/// это единственная часть ответа, которую модель точно прочитает.
fn with_truncation_warning(mut value: Value) -> Value {
    if value.get("truncated").and_then(Value::as_bool) != Some(true) {
        return value;
    }
    let row_count = value
        .get("row_count")
        .and_then(Value::as_u64)
        .unwrap_or_default();
    value["warning"] = Value::String(format!(
        "РЕЗУЛЬТАТ УСЕЧЁН: вернулись первые {row_count} строк, дальше данные есть, но они не показаны. \
         Выводы о полноте/пропусках/крайних датах по этой выборке делать НЕЛЬЗЯ."
    ));
    value["next_step"] = Value::String(
        "Повтори запрос с агрегацией (COUNT/MIN/MAX/SUM/GROUP BY) вместо построчной выборки — \
         либо задай limit явно (максимум 200)."
            .to_string(),
    );
    value
}

/// Сколько строк должно быть в выборке, чтобы сплошные нули считались сигналом,
/// а не случайностью маленького результата.
const ZERO_COLUMN_MIN_ROWS: usize = 3;

/// Колонка, нулевая во ВСЕХ строках, — почти всегда не бизнес-факт, а незаполненная
/// стадия: не загруженный источник или непересобранная проекция. Модель же читает
/// ноль как обычное значение и делает по нему выводы («заказов нет»). Проговариваем
/// это словами — тем же приёмом, что и усечение выборки.
fn with_empty_column_warning(mut value: Value) -> Value {
    let Some(rows) = value.get("rows").and_then(Value::as_array) else {
        return value;
    };
    if rows.len() < ZERO_COLUMN_MIN_ROWS {
        return value;
    }
    let Some(columns) = value.get("columns").and_then(Value::as_array) else {
        return value;
    };

    let empty: Vec<String> = columns
        .iter()
        .filter_map(Value::as_str)
        .filter(|column| {
            let mut saw_number = false;
            let all_zero = rows.iter().all(|row| match row.get(*column) {
                Some(Value::Number(n)) => {
                    saw_number = true;
                    n.as_f64().is_some_and(|v| v == 0.0)
                }
                Some(Value::Null) | None => true,
                _ => false,
            });
            saw_number && all_zero
        })
        .map(str::to_string)
        .collect();

    if empty.is_empty() {
        return value;
    }
    let row_count = rows.len();
    value["empty_columns"] = Value::Array(empty.iter().map(|c| Value::String(c.clone())).collect());
    value["warning"] = Value::String(format!(
        "СПЛОШНЫЕ НУЛИ в колонках: {}. Во всех {} строках значение нулевое или пустое — \
         это чаще признак незаполненных данных, чем реального отсутствия событий. \
         НЕ описывай это как факт («заказов не было»), пока не проверил источник.",
        empty.join(", "),
        row_count
    ));
    value["next_step"] = Value::String(
        "Спустись на слой ниже: если пуста проекция — посмотри документы-источники за тот же \
         период; если пуст и источник — проверь, был ли импорт. В ответе назови, что именно \
         подтвердил: нет источника / источник есть, но не пересобрано / данные есть, ошибка расчёта."
            .to_string(),
    );
    value
}

async fn save_query_artifact(
    args: &ExecuteQueryArgs,
    chat_id: &str,
    agent_id: &str,
    row_count: usize,
) -> Option<String> {
    let dto = crate::domain::a019_llm_artifact::service::LlmArtifactDto {
        id: None,
        code: None,
        description: args.description.clone(),
        comment: Some(format!("Возвращено строк: {row_count}")),
        chat_id: chat_id.to_string(),
        agent_id: agent_id.to_string(),
        artifact_type: Some("sql_query".to_string()),
        sql_query: args.sql.clone(),
        query_params: Some(json!({ "params": args.params, "limit": args.limit }).to_string()),
        visualization_config: None,
    };
    crate::domain::a019_llm_artifact::service::create(dto)
        .await
        .ok()
        .map(|id| id.to_string())
}

fn source_template(source: &crate::shared::data_access::DataSourceCatalogItem) -> Value {
    match source.kind {
        DataSourceKind::Base => json!({
            "kind": "schema", "schema_id": source.id,
            "fields": [], "group_by": [], "metrics": [], "filters": [], "sort": []
        }),
        DataSourceKind::Dataview => json!({
            "kind": "dataview", "view_id": source.id,
            "metric_ids": [], "group_by": "",
            "context": { "date_from": "YYYY-MM-DD", "date_to": "YYYY-MM-DD", "connection_mp_refs": [], "params": {} }
        }),
        DataSourceKind::Raw => json!({
            "kind": "sql", "sql": "SELECT ... WHERE date_column BETWEEN ? AND ?",
            "params": ["$context.date_from", "$context.date_to"]
        }),
    }
}

fn source_search_text(source: &crate::shared::data_access::DataSourceCatalogItem) -> String {
    format!(
        "{} {} {} {}",
        source.id,
        source.name,
        source.description,
        serde_json::to_string(&source.capabilities).unwrap_or_default()
    )
    .to_lowercase()
}

pub async fn execute_data_tool(
    name: &str,
    arguments: &str,
    agent_type: &AgentType,
    chat_id: &str,
    agent_id: &str,
) -> Value {
    let result: Result<Value, String> = match name {
        "find_data_sources" => parse_args::<FindSourcesArgs>(arguments).and_then(|args| {
            let terms: Vec<String> = args
                .query
                .to_lowercase()
                .split_whitespace()
                .map(str::to_string)
                .collect();
            let limit = args.limit.unwrap_or(5).clamp(1, 10);
            let mut ranked = list_sources(args.kind)
                .into_iter()
                .filter_map(|source| {
                    let haystack = source_search_text(&source);
                    let hits = terms.iter().filter(|term| haystack.contains(term.as_str())).count();
                    if hits == 0 && source.kind != DataSourceKind::Raw && !terms.is_empty() {
                        return None;
                    }
                    let kind_bonus = match source.kind {
                        DataSourceKind::Dataview => 3,
                        DataSourceKind::Base => 2,
                        DataSourceKind::Raw => 0,
                    };
                    Some((hits * 10 + kind_bonus, source))
                })
                .collect::<Vec<_>>();
            ranked.sort_by(|(left_score, left), (right_score, right)| {
                right_score.cmp(left_score).then_with(|| left.id.cmp(&right.id))
            });
            let mut sources = ranked
                .into_iter()
                .take(limit)
                .map(|(_, source)| {
                    let template = source_template(&source);
                    let mut value = serde_json::to_value(source).unwrap_or_default();
                    value["source_template"] = template;
                    value
                })
                .collect::<Vec<_>>();
            if args.kind.is_none()
                && !sources.iter().any(|source| source.get("kind").and_then(Value::as_str) == Some("raw"))
            {
                if let Some(raw) = list_sources(Some(DataSourceKind::Raw)).into_iter().next() {
                    let mut value = serde_json::to_value(&raw).unwrap_or_default();
                    value["source_template"] = source_template(&raw);
                    if sources.len() == limit {
                        sources.pop();
                    }
                    sources.push(value);
                }
            }
            Ok(json!({
                "ok": true,
                "sources": sources,
                "selection_order": ["dataview", "base", "raw"],
                "instruction": "Copy source_template as an object (never as a JSON string), fill only IDs listed in capabilities, then call preview_data once."
            }))
        }),
        "preview_data" => match parse_args::<PreviewDataArgs>(arguments) {
            Ok(args) => match crate::plugins::data::execute_source_with_context(
                &args.source,
                args.limit.unwrap_or(50).clamp(1, 200),
                Some(&args.context),
            )
            .await
            {
                Ok(tabular) => {
                    let columns =
                        crate::plugins::data::infer_columns(&tabular.rows, &tabular.columns);
                    // Прогоняем тот же презентационный гейт, что и build_chart: если он
                    // зелёный здесь, build не упадёт на презентации (никакого расхождения
                    // «preview ok → build fail»).
                    let build = args.chart.as_ref().map(|chart| {
                        let title = chart.get("title").and_then(Value::as_str).unwrap_or("График");
                        super::chart_tools::validate_chart_presentation(
                            chart,
                            &tabular.columns,
                            &tabular.rows,
                            title,
                        )
                    });
                    let mut out = json!({
                        "ok": true,
                        "source": tabular.source,
                        "columns": columns,
                        "rows": tabular.rows,
                        "row_count": tabular.row_count,
                        "truncated": tabular.truncated,
                        "effective_context": crate::plugins::data::effective_source_context(Some(&args.context)),
                        "next_step": "Pass this exact source and context to build_chart/build_table; do not call find_data_sources again."
                    });
                    match build {
                        Some(Ok(_)) => {
                            out["build_ready"] = json!(true);
                        }
                        Some(Err(presentation)) => {
                            out["build_ready"] = json!(false);
                            out["presentation"] = presentation;
                            out["next_step"] = json!("build_chart отклонит этот chart: исправьте поле по presentation (выберите category/series.field из columns) и снова вызовите preview_data с тем же source.");
                        }
                        None => {}
                    }
                    Ok(out)
                }
                Err(error) => Err(error),
            },
            Err(error) => Err(error),
        },
        "list_data_sources" => parse_args::<ListSourcesArgs>(arguments)
            .map(|args| list_sources(args.kind))
            .and_then(|sources| {
                serde_json::to_value(json!({
                    "total": sources.len(),
                    "sources": sources,
                    "selection_order": ["dataview", "base", "raw"]
                }))
                .map_err(|error| error.to_string())
            }),
        "query_data_schema" => match parse_args::<SchemaQueryRequest>(arguments) {
            Ok(request) => query_schema(request).await.and_then(|value| {
                serde_json::to_value(value)
                    .map(with_truncation_warning)
                    .map(with_empty_column_warning)
                    .map_err(|error| error.to_string())
            }),
            Err(error) => Err(error),
        },
        "run_data_view_scalar" => match parse_args::<DataViewScalarRequest>(arguments) {
            Ok(request) => run_data_view_scalar(request)
                .await
                .and_then(|value| serde_json::to_value(value).map_err(|error| error.to_string())),
            Err(error) => Err(error),
        },
        "run_data_view_drilldown" => match parse_args::<DataViewDrilldownRequest>(arguments) {
            Ok(request) => run_data_view_drilldown(request)
                .await
                .and_then(|value| serde_json::to_value(value).map_err(|error| error.to_string())),
            Err(error) => Err(error),
        },
        "execute_query" => match parse_args::<ExecuteQueryArgs>(arguments) {
            Ok(args) => match access_profile(agent_type) {
                Ok(profile) => match execute_raw_query(
                    RawQueryRequest {
                        sql: args.sql.clone(),
                        params: args.params.clone(),
                        limit: args.limit,
                    },
                    profile,
                )
                .await
                {
                    Ok(tabular) => {
                        let artifact_id =
                            save_query_artifact(&args, chat_id, agent_id, tabular.row_count).await;
                        serde_json::to_value(tabular)
                            .map(with_truncation_warning)
                            .map(with_empty_column_warning)
                            .map(|mut value| {
                                if let Some(id) = artifact_id {
                                    value["artifact_id"] = Value::String(id);
                                }
                                value
                            })
                            .map_err(|error| error.to_string())
                    }
                    Err(error) => Err(error),
                },
                Err(error) => Err(error),
            },
            Err(error) => Err(error),
        },
        _ => Err(format!("Unknown data tool: {name}")),
    };
    result.unwrap_or_else(|error| json!({ "error": error }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn definitions_are_unique_and_include_typed_queries() {
        let definitions = tool_definitions();
        let names: HashSet<_> = definitions.iter().map(|tool| tool.name.as_str()).collect();
        assert_eq!(names.len(), definitions.len());
        assert!(names.contains("list_data_sources"));
        assert!(names.contains("query_data_schema"));
        assert!(names.contains("run_data_view_scalar"));
        assert!(names.contains("run_data_view_drilldown"));
        let preview = definitions
            .iter()
            .find(|tool| tool.name == "preview_data")
            .expect("preview_data definition");
        assert_eq!(
            preview.parameters["properties"]["source"]["oneOf"]
                .as_array()
                .map(Vec::len),
            Some(3)
        );
    }

    /// Регрессия из разбора чата: агент увидел `orders: 0` за полгода в проекции
    /// p916, назвал это фактом и не заглянул в источник — где заказы были.
    #[test]
    fn all_zero_columns_are_called_out() {
        let value = with_empty_column_warning(json!({
            "columns": ["ym", "opens", "orders"],
            "rows": [
                { "ym": "2025-08", "opens": 784592, "orders": 0 },
                { "ym": "2025-09", "opens": 1172898, "orders": 0 },
                { "ym": "2025-10", "opens": 1442728, "orders": null },
            ],
        }));
        assert_eq!(value["empty_columns"], json!(["orders"]));
        let warning = value["warning"].as_str().unwrap_or_default();
        assert!(warning.contains("orders"));
        assert!(value["next_step"].as_str().is_some());
    }

    #[test]
    fn populated_and_tiny_results_stay_untouched() {
        // Заполненная колонка — не сигнал.
        let full = json!({
            "columns": ["orders"],
            "rows": [{"orders": 1}, {"orders": 0}, {"orders": 5}],
        });
        assert_eq!(with_empty_column_warning(full.clone()), full);

        // На двух строках сплошной ноль — совпадение, а не признак.
        let tiny = json!({
            "columns": ["orders"],
            "rows": [{"orders": 0}, {"orders": 0}],
        });
        assert_eq!(with_empty_column_warning(tiny.clone()), tiny);

        // Текстовые колонки не проверяем: пустая строка — не ноль.
        let text = json!({
            "columns": ["name"],
            "rows": [{"name": ""}, {"name": ""}, {"name": ""}],
        });
        assert_eq!(with_empty_column_warning(text.clone()), text);
    }

    /// Усечение должно быть проговорено словами: голый флаг `truncated` модель пропускает
    /// и делает по неполной выборке выводы о полноте данных.
    #[test]
    fn truncated_results_carry_an_explicit_warning() {
        let warned = with_truncation_warning(json!({ "truncated": true, "row_count": 50 }));
        assert!(warned["warning"].as_str().unwrap().contains("50"));
        assert!(warned["next_step"].is_string());

        let intact = json!({ "truncated": false, "row_count": 3 });
        assert_eq!(with_truncation_warning(intact.clone()), intact);
    }

    #[test]
    fn legacy_catalog_tools_are_gone() {
        // list_data_views / list_data_schemas удалены в пользу единого list_data_sources.
        let names: HashSet<_> = tool_definitions()
            .iter()
            .map(|tool| tool.name.clone())
            .collect();
        assert!(names.contains("list_data_sources"));
        assert!(!names.contains("list_data_views"));
        assert!(!names.contains("list_data_schemas"));
        assert!(!DATA_TOOL_NAMES.contains(&"list_data_views"));
        assert!(!DATA_TOOL_NAMES.contains(&"list_data_schemas"));
    }

    #[test]
    fn wb_catalog_exposes_copyable_source_and_physical_columns() {
        crate::composition::install_all();
        let source = list_sources(Some(DataSourceKind::Base))
            .into_iter()
            .find(|source| source.id == "a012")
            .expect("WB sales source");
        assert_eq!(source_template(&source)["kind"], "schema");
        assert_eq!(source_template(&source)["schema_id"], "a012");
        assert_eq!(source.table.as_deref(), Some("a012_wb_sales"));

        let physical_columns: HashSet<_> = source
            .capabilities
            .dimensions
            .iter()
            .chain(source.capabilities.metrics.iter())
            .chain(source.capabilities.filters.iter())
            .filter_map(|field| field.db_column.as_deref())
            .collect();
        assert!(physical_columns.contains("sale_date"));
        assert!(physical_columns.contains("total_price"));
    }

    #[test]
    fn wb_weekday_source_is_typed_and_uses_live_period() {
        let source: PluginDataSource = serde_json::from_value(json!({
            "kind": "sql",
            "sql": "SELECT ((CAST(strftime('%w', sale_date) AS INTEGER) + 6) % 7) + 1 AS weekday, SUM(COALESCE(amount_line, 0)) AS sales_amount FROM a012_wb_sales WHERE is_deleted = 0 AND substr(sale_date, 1, 10) BETWEEN ? AND ? GROUP BY 1 ORDER BY 1",
            "params": ["$context.date_from", "$context.date_to"]
        }))
        .expect("weekday source must match the shared source contract");
        assert!(crate::plugins::data::source_uses_period_context(&source));
    }

    #[tokio::test]
    async fn wb_sales_query_returns_the_wb_sales_catalog_entry() {
        let result = execute_data_tool(
            "find_data_sources",
            &json!({ "query": "график продаж WB по дням недели", "limit": 5 }).to_string(),
            &AgentType::BusinessAnalyst,
            "test-chat",
            "test-agent",
        )
        .await;
        let sources = result["sources"].as_array().expect("sources array");
        assert!(
            sources
                .iter()
                .any(|source| source["table"] == "a012_wb_sales"),
            "ranked sources must include a012_wb_sales: {sources:?}"
        );
        assert!(sources.iter().any(|source| source["kind"] == "raw"));
    }
}
