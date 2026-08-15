//! Доступ к `sys_metric_snapshot` / `sys_metric_value`.

use contracts::system::metrics::{MetricPoint, MetricSeriesDto, MetricSnapshotDto};
use sea_orm::{ConnectionTrait, DatabaseBackend, FromQueryResult, Statement, Value};
use std::collections::BTreeMap;

/// Сколько снимков держим. Десяток рестартов в день — это полтора месяца
/// истории; больше не нужно, а расти без границы таблица не должна.
pub const KEEP_SNAPSHOTS: i64 = 500;

#[derive(Debug, Clone, FromQueryResult)]
pub struct SnapshotRow {
    pub id: String,
    pub captured_at: String,
    pub trigger: String,
    pub app_version: String,
    pub git_commit: Option<String>,
    pub build_profile: String,
    pub schema_version: i64,
    pub code_generated_at: Option<String>,
    pub collect_ms: i64,
    pub details_json: String,
}

impl SnapshotRow {
    pub fn to_dto(&self) -> MetricSnapshotDto {
        MetricSnapshotDto {
            id: self.id.clone(),
            captured_at: self.captured_at.clone(),
            trigger: self.trigger.clone(),
            app_version: self.app_version.clone(),
            git_commit: self.git_commit.clone(),
            build_profile: self.build_profile.clone(),
            schema_version: self.schema_version,
            code_generated_at: self.code_generated_at.clone(),
            collect_ms: self.collect_ms,
        }
    }
}

#[derive(FromQueryResult)]
struct ValueRow {
    metric_key: String,
    value: f64,
}

#[derive(FromQueryResult)]
struct SeriesRow {
    metric_key: String,
    captured_at: String,
    value: f64,
}

#[derive(FromQueryResult)]
struct CountRow {
    n: i64,
}

const SNAPSHOT_COLUMNS: &str = "id, captured_at, trigger, app_version, git_commit, \
     build_profile, schema_version, code_generated_at, collect_ms, details_json";

/// Записать снимок целиком. Значения пишутся одним многострочным INSERT:
/// сорок отдельных запросов в SQLite стоят на порядок дороже, чем один.
pub async fn insert_snapshot(
    snapshot: &MetricSnapshotDto,
    values: &BTreeMap<String, f64>,
    details_json: &str,
) -> anyhow::Result<()> {
    let db = crate::shared::data::db::get_connection();

    db.execute(Statement::from_sql_and_values(
        DatabaseBackend::Sqlite,
        "INSERT INTO sys_metric_snapshot \
         (id, captured_at, trigger, app_version, git_commit, build_profile, \
          schema_version, code_generated_at, collect_ms, details_json) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        [
            snapshot.id.clone().into(),
            snapshot.captured_at.clone().into(),
            snapshot.trigger.clone().into(),
            snapshot.app_version.clone().into(),
            snapshot.git_commit.clone().into(),
            snapshot.build_profile.clone().into(),
            snapshot.schema_version.into(),
            snapshot.code_generated_at.clone().into(),
            snapshot.collect_ms.into(),
            details_json.into(),
        ],
    ))
    .await?;

    if values.is_empty() {
        return Ok(());
    }

    let mut sql =
        String::from("INSERT INTO sys_metric_value (snapshot_id, metric_key, value) VALUES ");
    let mut params: Vec<Value> = Vec::with_capacity(values.len() * 3);
    for (index, (key, value)) in values.iter().enumerate() {
        if index > 0 {
            sql.push(',');
        }
        sql.push_str("(?, ?, ?)");
        params.push(snapshot.id.clone().into());
        params.push(key.clone().into());
        params.push((*value).into());
    }

    db.execute(Statement::from_sql_and_values(
        DatabaseBackend::Sqlite,
        &sql,
        params,
    ))
    .await?;

    Ok(())
}

pub async fn latest_snapshot() -> anyhow::Result<Option<SnapshotRow>> {
    let db = crate::shared::data::db::get_connection();
    Ok(SnapshotRow::find_by_statement(Statement::from_string(
        DatabaseBackend::Sqlite,
        format!(
            "SELECT {SNAPSHOT_COLUMNS} FROM sys_metric_snapshot \
             ORDER BY captured_at DESC LIMIT 1"
        ),
    ))
    .one(db)
    .await?)
}

/// Снимок, непосредственно предшествующий указанному времени, — база для дельты.
pub async fn previous_snapshot(before: &str) -> anyhow::Result<Option<SnapshotRow>> {
    let db = crate::shared::data::db::get_connection();
    Ok(
        SnapshotRow::find_by_statement(Statement::from_sql_and_values(
            DatabaseBackend::Sqlite,
            format!(
                "SELECT {SNAPSHOT_COLUMNS} FROM sys_metric_snapshot \
                 WHERE captured_at < ?1 ORDER BY captured_at DESC LIMIT 1"
            ),
            [before.into()],
        ))
        .one(db)
        .await?,
    )
}

pub async fn list_snapshots(limit: u64) -> anyhow::Result<Vec<SnapshotRow>> {
    let db = crate::shared::data::db::get_connection();
    Ok(
        SnapshotRow::find_by_statement(Statement::from_sql_and_values(
            DatabaseBackend::Sqlite,
            format!(
                "SELECT {SNAPSHOT_COLUMNS} FROM sys_metric_snapshot \
                 ORDER BY captured_at DESC LIMIT ?1"
            ),
            [(limit as i64).into()],
        ))
        .all(db)
        .await?,
    )
}

pub async fn values_of(snapshot_id: &str) -> anyhow::Result<BTreeMap<String, f64>> {
    let db = crate::shared::data::db::get_connection();
    let rows = ValueRow::find_by_statement(Statement::from_sql_and_values(
        DatabaseBackend::Sqlite,
        "SELECT metric_key, value FROM sys_metric_value WHERE snapshot_id = ?1",
        [snapshot_id.into()],
    ))
    .all(db)
    .await?;
    Ok(rows
        .into_iter()
        .map(|row| (row.metric_key, row.value))
        .collect())
}

/// Ряды по метрикам, от старых к новым, ограниченные последними `limit` снимками.
pub async fn series(keys: &[String], limit: u64) -> anyhow::Result<Vec<MetricSeriesDto>> {
    if keys.is_empty() {
        return Ok(Vec::new());
    }
    let db = crate::shared::data::db::get_connection();

    let placeholders = vec!["?"; keys.len()].join(",");
    let mut params: Vec<Value> = keys.iter().map(|key| key.clone().into()).collect();
    params.push((limit as i64).into());

    let sql = format!(
        "SELECT v.metric_key AS metric_key, s.captured_at AS captured_at, v.value AS value \
         FROM sys_metric_value v \
         JOIN sys_metric_snapshot s ON s.id = v.snapshot_id \
         WHERE v.metric_key IN ({placeholders}) \
           AND s.id IN (SELECT id FROM sys_metric_snapshot ORDER BY captured_at DESC LIMIT ?) \
         ORDER BY s.captured_at"
    );

    let rows = SeriesRow::find_by_statement(Statement::from_sql_and_values(
        DatabaseBackend::Sqlite,
        &sql,
        params,
    ))
    .all(db)
    .await?;

    // Порядок рядов повторяет порядок запрошенных ключей, чтобы фронт мог не
    // искать по имени.
    let mut by_key: BTreeMap<String, Vec<MetricPoint>> = BTreeMap::new();
    for row in rows {
        by_key.entry(row.metric_key).or_default().push(MetricPoint {
            captured_at: row.captured_at,
            value: row.value,
        });
    }

    Ok(keys
        .iter()
        .map(|key| MetricSeriesDto {
            key: key.clone(),
            points: by_key.remove(key).unwrap_or_default(),
        })
        .collect())
}

/// Сколько снимков за последние `days` дней — это и есть число рестартов.
pub async fn snapshots_since(days: i64) -> anyhow::Result<i64> {
    let db = crate::shared::data::db::get_connection();
    let since = (chrono::Utc::now() - chrono::Duration::days(days)).to_rfc3339();
    let row = CountRow::find_by_statement(Statement::from_sql_and_values(
        DatabaseBackend::Sqlite,
        "SELECT COUNT(*) AS n FROM sys_metric_snapshot WHERE captured_at >= ?1",
        [since.into()],
    ))
    .one(db)
    .await?;
    Ok(row.map(|row| row.n).unwrap_or(0))
}

/// Оставить последние `keep` снимков. Значения уезжают следом — внешнего ключа
/// нет намеренно (лишний индекс на горячей вставке), поэтому чистим обе таблицы.
pub async fn prune(keep: i64) -> anyhow::Result<()> {
    let db = crate::shared::data::db::get_connection();
    db.execute(Statement::from_sql_and_values(
        DatabaseBackend::Sqlite,
        "DELETE FROM sys_metric_snapshot WHERE id NOT IN \
         (SELECT id FROM sys_metric_snapshot ORDER BY captured_at DESC LIMIT ?1)",
        [keep.into()],
    ))
    .await?;
    db.execute(Statement::from_string(
        DatabaseBackend::Sqlite,
        "DELETE FROM sys_metric_value WHERE snapshot_id NOT IN \
         (SELECT id FROM sys_metric_snapshot)",
    ))
    .await?;
    Ok(())
}
