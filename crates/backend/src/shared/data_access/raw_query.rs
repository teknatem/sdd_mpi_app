use super::catalog::{DataSourceKind, DataSourceRef};
use super::row_json::{fetch_json_rows, JsonBind};
use super::sql_guard::{inspect_read_query, wrap_limited_sql};
use super::TabularResult;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::{Duration, Instant};

const DEFAULT_LIMIT: usize = 50;
const MAX_LIMIT: usize = 2_000;
/// Потолок для лимита, выведенного из самого SQL (без явного аргумента `limit`).
/// Совпадает с максимумом в схеме инструмента `execute_query`: вывод лимита — это
/// удобство, а не способ протащить в контекст модели тысячи строк.
const DECLARED_LIMIT_CEILING: usize = 200;
const QUERY_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SqlAccessProfile {
    Analytics,
    KnowledgeBase,
    General,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawQueryRequest {
    pub sql: String,
    #[serde(default)]
    pub params: Vec<Value>,
    #[serde(default)]
    pub limit: Option<usize>,
}

fn is_secret_table(table: &str) -> bool {
    // a006_connection_mp намеренно разблокирован для raw SQL (нужен JOIN по marketplace в
    // аналитике/графиках) — ВНИМАНИЕ: таблица содержит credentials (api_key и т.п.), которые
    // тем самым становятся доступны LLM-генерируемому SQL.
    //
    // Остальные носители кредов закрыты целиком. a038_llm_connection попал сюда
    // не сразу: он платформенный (настройки провайдеров LLM), но носит имя вида
    // `aNNN_`, а `is_analytics_table` классифицирует именно по имени — и таблица
    // с `api_key` считалась аналитической. Пофамильный запрет колонки при этом
    // не спасал: `SELECT *` не называет ни одного поля, а запрет звёздочки был
    // заведён только для a006.
    matches!(table, "a001_connection_1c" | "a038_llm_connection")
}

fn is_knowledge_table(table: &str) -> bool {
    table.starts_with("a017_") || table.starts_with("a018_") || table.starts_with("a019_")
}

fn is_analytics_table(table: &str) -> bool {
    if is_secret_table(table) || is_knowledge_table(table) {
        return false;
    }
    (table.starts_with('a')
        && table
            .get(1..4)
            .is_some_and(|part| part.chars().all(|c| c.is_ascii_digit())))
        || table.starts_with('p')
        || table == "sys_general_ledger"
}

fn profile_allows(profile: SqlAccessProfile, table: &str) -> bool {
    if is_secret_table(table) || (table.starts_with("sys_") && table != "sys_general_ledger") {
        return false;
    }
    match profile {
        SqlAccessProfile::Analytics => is_analytics_table(table),
        SqlAccessProfile::KnowledgeBase => is_knowledge_table(table),
        SqlAccessProfile::General => is_analytics_table(table) || is_knowledge_table(table),
    }
}

pub fn enforce_access_profile(profile: SqlAccessProfile, tables: &[String]) -> Result<(), String> {
    let blocked: Vec<&str> = tables
        .iter()
        .map(String::as_str)
        .filter(|table| !profile_allows(profile, table))
        .collect();
    if blocked.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "Raw SQL access is not allowed for table(s): {}. Use a safe data schema or a dedicated tool.",
            blocked.join(", ")
        ))
    }
}

fn json_param_to_bind(value: Value) -> Result<JsonBind, String> {
    match value {
        Value::Null => Ok(JsonBind::Null),
        Value::Bool(value) => Ok(JsonBind::Bool(value)),
        Value::Number(value) => {
            if let Some(value) = value.as_i64() {
                Ok(JsonBind::Int(value))
            } else if let Some(value) = value.as_f64() {
                Ok(JsonBind::Float(value))
            } else {
                Err("Unsupported numeric SQL parameter".to_string())
            }
        }
        Value::String(value) => Ok(JsonBind::Text(value)),
        Value::Array(_) | Value::Object(_) => {
            Err("SQL parameters must be scalar JSON values".to_string())
        }
    }
}

pub async fn execute_raw_query(
    request: RawQueryRequest,
    profile: SqlAccessProfile,
) -> Result<TabularResult, String> {
    let started = Instant::now();
    let query_info = inspect_read_query(&request.sql).map_err(|error| {
        tracing::warn!(blocked_reason = %error, "raw SQL rejected");
        error
    })?;
    enforce_access_profile(profile, &query_info.tables).map_err(|error| {
        tracing::warn!(blocked_reason = %error, "raw SQL rejected");
        error
    })?;

    // Явный `LIMIT n` в самом SQL — это намерение автора запроса. Если вызывающий не задал
    // limit отдельным аргументом, берём лимит из SQL (в пределах MAX_LIMIT), иначе дефолтные 50
    // молча срезали бы результат ниже запрошенного — и по обрезанной выборке делались выводы.
    let limit = match request.limit {
        Some(limit) => limit,
        None => query_info
            .declared_limit
            .filter(|declared| *declared > 0)
            .map(|declared| declared.clamp(DEFAULT_LIMIT, DECLARED_LIMIT_CEILING))
            .unwrap_or(DEFAULT_LIMIT),
    };
    if !(1..=MAX_LIMIT).contains(&limit) {
        return Err(format!("limit must be between 1 and {MAX_LIMIT}"));
    }
    let binds = request
        .params
        .into_iter()
        .map(json_param_to_bind)
        .collect::<Result<Vec<_>, _>>()?;
    let sql = wrap_limited_sql(&request.sql, limit + 1, "llm_limited_result");
    let (mut rows, columns) = tokio::time::timeout(QUERY_TIMEOUT, fetch_json_rows(&sql, binds))
        .await
        .map_err(|_| "Raw SQL query timed out after 10 seconds".to_string())?
        .map_err(|error| format!("SQL execution error: {error}"))?;
    let truncated = rows.len() > limit;
    if truncated {
        rows.truncate(limit);
    }
    let result = TabularResult {
        source: DataSourceRef {
            kind: DataSourceKind::Raw,
            id: "raw_sql".to_string(),
        },
        row_count: rows.len(),
        rows,
        columns,
        truncated,
        generated_sql: None,
    };
    tracing::info!(
        source_kind = "raw",
        source_id = "raw_sql",
        elapsed_ms = started.elapsed().as_millis(),
        row_count = result.row_count,
        truncated = result.truncated,
        "semantic data query completed"
    );
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Носитель кредов закрыт для сырого SQL целиком — во всех профилях.
    ///
    /// Проверяется именно `*`, а не `SELECT api_key`: имя поля ловит гард
    /// `sql_guard`, а звёздочка не называет полей вовсе, и до правки таблица
    /// проходила как аналитическая по одному только префиксу `a038_`.
    #[test]
    fn llm_connection_is_never_reachable_by_raw_sql() {
        for profile in [
            SqlAccessProfile::Analytics,
            SqlAccessProfile::KnowledgeBase,
            SqlAccessProfile::General,
        ] {
            assert!(
                enforce_access_profile(profile, &["a038_llm_connection".to_string()]).is_err(),
                "a038_llm_connection доступна профилю {profile:?}"
            );
        }
    }

    #[test]
    fn analytics_profile_blocks_secret_and_system_tables() {
        // a001_connection_1c остаётся секретной (credentials 1С).
        assert!(enforce_access_profile(
            SqlAccessProfile::Analytics,
            &["a001_connection_1c".to_string()]
        )
        .is_err());
        assert!(enforce_access_profile(
            SqlAccessProfile::Analytics,
            &["sys_task_runs".to_string()]
        )
        .is_err());
        assert!(enforce_access_profile(
            SqlAccessProfile::Analytics,
            &[
                "p904_sales_data".to_string(),
                "sys_general_ledger".to_string()
            ]
        )
        .is_ok());
    }

    #[test]
    fn analytics_profile_allows_connection_mp_after_unblock() {
        // a006_connection_mp намеренно разблокирован для raw SQL (JOIN по marketplace).
        assert!(enforce_access_profile(
            SqlAccessProfile::Analytics,
            &["a006_connection_mp".to_string()]
        )
        .is_ok());
    }

    #[test]
    fn knowledge_profile_is_narrow() {
        assert!(enforce_access_profile(
            SqlAccessProfile::KnowledgeBase,
            &["a018_llm_chat_message".to_string()]
        )
        .is_ok());
        assert!(enforce_access_profile(
            SqlAccessProfile::KnowledgeBase,
            &["p904_sales_data".to_string()]
        )
        .is_err());
    }
}
