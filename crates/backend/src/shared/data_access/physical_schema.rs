//! Физическая форма таблицы и разметка «поле метаданных → SQL-выражение».
//!
//! Зачем: `get_entity_schema` описывает агрегат **логически** (a015: `order_dt`, `connection_id`,
//! `nm_id`, …), но физически такие поля лежат внутри JSON-блобов (`header_json`, `line_json`,
//! `state_json`). Модель писала `SELECT connection_id FROM a015_wb_orders` и получала
//! `no such column` — по разбору чата это давало 4 из 10 падений SQL за диалог.
//!
//! Здесь мы сверяем метаданные с `PRAGMA table_info`, а недостающие поля **находим** в JSON-колонках
//! (пробным `json_extract(<col>,'$.<field>')` по выборке строк) и отдаём модели готовое выражение.

use super::row_json::fetch_json_rows;
use once_cell::sync::Lazy;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Mutex;

/// Сколько строк смотрим при поиске поля в JSON-колонке: достаточно, чтобы пережить
/// несколько пустых блобов подряд, и дёшево на любой таблице.
const PROBE_ROWS: usize = 200;
/// Предохранитель на размер пробного SELECT (полей × JSON-колонок).
const MAX_PROBE_EXPRESSIONS: usize = 240;

#[derive(Debug, Clone, Default)]
pub struct TableShape {
    /// Реальные колонки таблицы в порядке объявления.
    pub columns: Vec<String>,
    /// Колонки-блобы, в которых имеет смысл искать логические поля.
    pub json_columns: Vec<String>,
}

impl TableShape {
    pub fn has_column(&self, name: &str) -> bool {
        self.columns.iter().any(|column| column == name)
    }
}

/// Физическая схема статична в пределах запуска процесса — читаем её один раз на таблицу.
static SHAPE_CACHE: Lazy<Mutex<HashMap<String, Option<TableShape>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

/// Идентификаторы подставляются в SQL текстом (PRAGMA и json_extract не принимают bind),
/// поэтому пропускаем только заведомо безопасные имена.
fn is_safe_ident(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        && !name.starts_with(|c: char| c.is_ascii_digit())
}

pub async fn table_shape(table: &str) -> Option<TableShape> {
    if !is_safe_ident(table) {
        return None;
    }
    if let Some(cached) = SHAPE_CACHE
        .lock()
        .ok()
        .and_then(|cache| cache.get(table).cloned())
    {
        return cached;
    }

    let shape = load_table_shape(table).await;
    if let Ok(mut cache) = SHAPE_CACHE.lock() {
        cache.insert(table.to_string(), shape.clone());
    }
    shape
}

async fn load_table_shape(table: &str) -> Option<TableShape> {
    let (rows, _) = fetch_json_rows(&format!("PRAGMA table_info(\"{table}\")"), Vec::new())
        .await
        .ok()?;
    let columns: Vec<String> = rows
        .iter()
        .filter_map(|row| row.get("name").and_then(Value::as_str))
        .map(str::to_string)
        .collect();
    if columns.is_empty() {
        return None;
    }
    let json_columns = columns
        .iter()
        .filter(|column| column.ends_with("_json"))
        .cloned()
        .collect();
    Some(TableShape {
        columns,
        json_columns,
    })
}

/// Найти, в какой JSON-колонке лежит каждое из `fields`. Возвращает `field -> json-колонка`.
/// Одним запросом: `MAX(json_extract(col,'$.field') IS NOT NULL)` по выборке строк.
pub async fn locate_json_fields(
    table: &str,
    shape: &TableShape,
    fields: &[String],
) -> HashMap<String, String> {
    let mut located = HashMap::new();
    if shape.json_columns.is_empty() || fields.is_empty() || !is_safe_ident(table) {
        return located;
    }
    let probes: Vec<(String, String)> = fields
        .iter()
        .filter(|field| is_safe_ident(field))
        .flat_map(|field| {
            shape
                .json_columns
                .iter()
                .map(move |column| (field.clone(), column.clone()))
        })
        .take(MAX_PROBE_EXPRESSIONS)
        .collect();
    if probes.is_empty() {
        return located;
    }

    let select = probes
        .iter()
        .map(|(field, column)| {
            format!("MAX(json_extract({column}, '$.{field}') IS NOT NULL) AS \"{field}|{column}\"")
        })
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!("SELECT {select} FROM (SELECT * FROM \"{table}\" LIMIT {PROBE_ROWS})");

    let Ok((rows, _)) = fetch_json_rows(&sql, Vec::new()).await else {
        return located;
    };
    let Some(row) = rows.first() else {
        return located;
    };
    for (field, column) in probes {
        // Первая колонка, где поле встретилось, и выигрывает: порядок пробы = порядок
        // объявления JSON-колонок в таблице.
        if located.contains_key(&field) {
            continue;
        }
        let hit = row
            .get(format!("{field}|{column}"))
            .and_then(Value::as_i64)
            .unwrap_or(0)
            == 1;
        if hit {
            located.insert(field, column);
        }
    }
    located
}

/// Дополнить результат `get_entity_schema` физической правдой: оставить в `columns_for_sql`
/// только реальные колонки, а логические поля отдать готовыми `json_extract`-выражениями.
/// При любой неудаче (нет таблицы, ошибка запроса) схема возвращается как есть.
pub async fn annotate_with_physical_schema(mut schema: Value) -> Value {
    let Some(table) = schema
        .get("table")
        .and_then(Value::as_str)
        .map(str::to_string)
    else {
        return schema;
    };
    let Some(shape) = table_shape(&table).await else {
        return schema;
    };

    let declared: Vec<String> = schema
        .get("columns_for_sql")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    let (physical, missing): (Vec<String>, Vec<String>) = declared
        .into_iter()
        .partition(|column| shape.has_column(column));
    if missing.is_empty() {
        return schema;
    }

    let located = locate_json_fields(&table, &shape, &missing).await;
    let expression = |field: &str| {
        located
            .get(field)
            .map(|column| format!("json_extract({column}, '$.{field}')"))
    };

    let json_fields: Vec<Value> = missing
        .iter()
        .map(|field| match expression(field) {
            Some(sql) => json!({
                "field": field,
                "sql": format!("{sql} AS {field}"),
                "stored_in": located.get(field),
            }),
            None => json!({
                "field": field,
                "sql": Value::Null,
                "note": "поле не найдено ни в одной JSON-колонке этой таблицы — \
                         не используй его в SQL без проверки",
            }),
        })
        .collect();

    // Внутри `fields[]` тоже проставляем выражение: модель обычно читает именно этот блок.
    if let Some(fields) = schema.get_mut("fields").and_then(Value::as_array_mut) {
        for field in fields.iter_mut() {
            let Some(name) = field
                .get("column")
                .and_then(Value::as_str)
                .map(str::to_string)
            else {
                continue;
            };
            if shape.has_column(&name) {
                continue;
            }
            field["not_a_column"] = Value::Bool(true);
            if let Some(sql) = expression(&name) {
                field["sql"] = Value::String(sql);
            }
        }
    }

    schema["sql_hint"] = Value::String(format!(
        "SELECT {} FROM {} WHERE is_deleted = 0 LIMIT 100",
        physical
            .iter()
            .take(5)
            .cloned()
            .collect::<Vec<_>>()
            .join(", "),
        table
    ));
    schema["columns_for_sql"] = json!(physical);
    schema["json_fields"] = json!(json_fields);
    schema["warning"] = Value::String(format!(
        "{} поле(й) из описания агрегата НЕ являются колонками таблицы {} — они хранятся внутри \
         JSON-блобов. В SQL пиши их только через выражения из json_fields, иначе получишь \
         'no such column'.",
        missing.len(),
        table
    ));
    schema
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_unsafe_identifiers() {
        assert!(is_safe_ident("a015_wb_orders"));
        assert!(!is_safe_ident("a015; DROP TABLE x"));
        assert!(!is_safe_ident("header_json'"));
        assert!(!is_safe_ident(""));
        assert!(!is_safe_ident("2fast"));
    }

    /// Схема без таблицы или без расхождений остаётся неизменной (в т.ч. без похода в БД).
    #[tokio::test]
    async fn schema_without_table_is_untouched() {
        let schema = json!({ "fields": [], "columns_for_sql": ["a"] });
        assert_eq!(annotate_with_physical_schema(schema.clone()).await, schema);
    }
}
