//! Профиль данных: сколько строк в таблице, за какой период они есть и
//! насколько заполнены ссылки.
//!
//! Это знание, которое нельзя вывести ни из кода, ни из `metadata.json`: схема
//! говорит, что колонка есть, и молчит о том, что за спрошенный месяц в ней
//! пусто. Именно на этом чаще всего спотыкается агент — пишет корректный SQL к
//! таблице без данных и объясняет пользователю нулевой результат.
//!
//! Считается по требованию (`POST /api/kb/generate`) и один раз при старте, а
//! НЕ планировщиком: планировщик в конфиге выключается целиком, и профиль не
//! должен зависеть от этого решения.
//!
//! Отдаётся в двух видах из одного источника: полем `data_profile` в
//! `get_entity_schema` (там агент уже смотрит) и картой `generated/data-profile.md`.

use once_cell::sync::Lazy;
use sea_orm::{ConnectionTrait, DatabaseBackend, FromQueryResult, Statement};
use serde::Serialize;
use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};

/// Процессный кэш профиля: `get_entity_schema` вызывается внутри цикла чата и
/// должен оставаться синхронным, а строки меняются только при пересчёте.
static SNAPSHOT: Lazy<Mutex<Arc<HashMap<String, ProfileRow>>>> =
    Lazy::new(|| Mutex::new(Arc::new(HashMap::new())));

/// Профиль одной таблицы.
#[derive(Debug, Clone, Serialize, FromQueryResult)]
pub struct ProfileRow {
    pub table_name: String,
    pub entity_index: String,
    pub row_count: i64,
    pub date_column: Option<String>,
    pub date_min: Option<String>,
    pub date_max: Option<String>,
    pub null_stats_json: String,
    pub computed_ms: i64,
    pub computed_at: String,
}

impl ProfileRow {
    /// Доли NULL по ссылочным колонкам, %.
    pub fn null_shares(&self) -> BTreeMap<String, f64> {
        serde_json::from_str(&self.null_stats_json).unwrap_or_default()
    }
}

#[derive(FromQueryResult)]
struct CountRow {
    n: i64,
}

#[derive(FromQueryResult)]
struct RangeRow {
    lo: Option<String>,
    hi: Option<String>,
}

/// Пересчитать профиль по всем таблицам каталога. Возвращает число таблиц.
///
/// Ошибка по одной таблице не останавливает обход: таблица может отсутствовать
/// в этой базе (миграция не применена, набор данных не перенесён), и это не
/// повод остаться без профиля по остальным.
pub async fn refresh_all() -> anyhow::Result<usize> {
    let db = super::db::get_connection();
    let targets = crate::shared::llm::metadata_registry::METADATA_REGISTRY.profile_targets();
    let now = chrono::Utc::now().to_rfc3339();
    let mut written = 0usize;

    for target in targets {
        let started = std::time::Instant::now();

        let row_count =
            match scalar_count(db, &format!("SELECT COUNT(*) AS n FROM {}", target.table)).await {
                Ok(count) => count,
                Err(error) => {
                    tracing::debug!(
                        "[data_profile] таблица '{}' пропущена: {}",
                        target.table,
                        error
                    );
                    continue;
                }
            };

        let (date_min, date_max) = match target.date_column {
            Some(column) if row_count > 0 => date_range(db, target.table, column).await,
            _ => (None, None),
        };

        // Доля NULL считается только там, где есть строки: 0 из 0 — это не 100 %
        // незаполненности, а отсутствие данных, и путать их нельзя.
        let mut null_shares: BTreeMap<String, f64> = BTreeMap::new();
        if row_count > 0 {
            for column in &target.ref_columns {
                if let Ok(nulls) = scalar_count(
                    db,
                    &format!(
                        "SELECT COUNT(*) AS n FROM {} WHERE {} IS NULL OR {} = ''",
                        target.table, column, column
                    ),
                )
                .await
                {
                    let share = (nulls as f64) * 100.0 / (row_count as f64);
                    null_shares.insert((*column).to_string(), (share * 10.0).round() / 10.0);
                }
            }
        }

        let statement = Statement::from_sql_and_values(
            DatabaseBackend::Sqlite,
            "INSERT INTO sys_data_profile \
             (table_name, entity_index, row_count, date_column, date_min, date_max, \
              null_stats_json, computed_ms, computed_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?) \
             ON CONFLICT(table_name) DO UPDATE SET \
               entity_index    = excluded.entity_index, \
               row_count       = excluded.row_count, \
               date_column     = excluded.date_column, \
               date_min        = excluded.date_min, \
               date_max        = excluded.date_max, \
               null_stats_json = excluded.null_stats_json, \
               computed_ms     = excluded.computed_ms, \
               computed_at     = excluded.computed_at",
            [
                target.table.into(),
                target.entity_index.into(),
                row_count.into(),
                target.date_column.into(),
                date_min.into(),
                date_max.into(),
                serde_json::to_string(&null_shares)
                    .unwrap_or_else(|_| "{}".to_string())
                    .into(),
                (started.elapsed().as_millis() as i64).into(),
                now.clone().into(),
            ],
        );

        match db.execute(statement).await {
            Ok(_) => written += 1,
            Err(error) => tracing::warn!(
                "[data_profile] профиль '{}' не записан: {}",
                target.table,
                error
            ),
        }
    }

    tracing::info!("[data_profile] профиль пересчитан по {} таблицам", written);
    refresh_snapshot().await;
    Ok(written)
}

/// Перечитать таблицу в процессный кэш.
pub async fn refresh_snapshot() {
    let map: HashMap<String, ProfileRow> = list_all()
        .await
        .into_iter()
        .map(|row| (row.table_name.clone(), row))
        .collect();
    if let Ok(mut guard) = SNAPSHOT.lock() {
        *guard = Arc::new(map);
    }
    // Профиль уезжает в ответ `get_entity_schema`, а тот кэшируется на весь
    // процесс: без сброса свежие цифры не дошли бы до чата до перезапуска.
    crate::shared::llm::tool_executor::invalidate_metadata_tool_cache();
}

/// Текущий снимок профиля. Дёшево: клон `Arc`.
pub fn snapshot() -> Arc<HashMap<String, ProfileRow>> {
    SNAPSHOT
        .lock()
        .map(|guard| guard.clone())
        .unwrap_or_default()
}

async fn scalar_count(db: &sea_orm::DatabaseConnection, sql: &str) -> anyhow::Result<i64> {
    let row = CountRow::find_by_statement(Statement::from_string(DatabaseBackend::Sqlite, sql))
        .one(db)
        .await?;
    Ok(row.map(|r| r.n).unwrap_or(0))
}

async fn date_range(
    db: &sea_orm::DatabaseConnection,
    table: &str,
    column: &str,
) -> (Option<String>, Option<String>) {
    let sql = format!(
        "SELECT MIN({column}) AS lo, MAX({column}) AS hi FROM {table} \
         WHERE {column} IS NOT NULL AND {column} <> ''"
    );
    match RangeRow::find_by_statement(Statement::from_string(DatabaseBackend::Sqlite, &sql))
        .one(db)
        .await
    {
        Ok(Some(row)) => (row.lo, row.hi),
        _ => (None, None),
    }
}

/// Весь профиль, по убыванию числа строк.
pub async fn list_all() -> Vec<ProfileRow> {
    let db = super::db::get_connection();
    ProfileRow::find_by_statement(Statement::from_string(
        DatabaseBackend::Sqlite,
        "SELECT table_name, entity_index, row_count, date_column, date_min, date_max, \
                null_stats_json, computed_ms, computed_at \
         FROM sys_data_profile ORDER BY row_count DESC, table_name",
    ))
    .all(db)
    .await
    .unwrap_or_default()
}
