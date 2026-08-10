//! Локальный журнал переносов (`sys_dataset_transfer_log`).
//!
//! Хранит историю операций ЭТОГО инстанса. Каталогом снапшотов не является —
//! источник истины по тому, что лежит в бакете, только `datasets/catalog.json`.

use anyhow::Result;
use chrono::Utc;
use contracts::system::datasets::TransferLogEntryDto;
use sea_orm::{ConnectionTrait, DatabaseBackend, Statement};

use crate::shared::data::db::get_connection;

pub const OPERATION_SNAPSHOT: &str = "snapshot";
pub const OPERATION_RESTORE: &str = "restore";

pub const STATUS_OK: &str = "ok";
pub const STATUS_FAILED: &str = "failed";
pub const STATUS_ROLLED_BACK: &str = "rolled_back";

#[derive(Debug, Clone)]
pub struct TransferLogEntry {
    pub operation: &'static str,
    pub snapshot_id: String,
    pub instance_id: String,
    pub source_instance_id: Option<String>,
    pub set_ids: Vec<String>,
    pub mode: Option<String>,
    pub status: &'static str,
    pub bytes: i64,
    pub files_written: i64,
    pub files_deleted: i64,
    pub pre_restore_archive: Option<String>,
    pub manifest_json: Option<String>,
    pub error: Option<String>,
    pub actor_user_id: Option<String>,
}

pub async fn insert(entry: &TransferLogEntry) -> Result<()> {
    let conn = get_connection();
    conn.execute(Statement::from_sql_and_values(
        DatabaseBackend::Sqlite,
        "INSERT INTO sys_dataset_transfer_log (
            id, operation, snapshot_id, instance_id, source_instance_id, set_ids, mode,
            status, bytes, files_written, files_deleted, pre_restore_archive,
            manifest_json, error, actor_user_id, created_at
         ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        [
            uuid::Uuid::new_v4().to_string().into(),
            entry.operation.to_string().into(),
            entry.snapshot_id.clone().into(),
            entry.instance_id.clone().into(),
            entry.source_instance_id.clone().into(),
            serde_json::to_string(&entry.set_ids)
                .unwrap_or_else(|_| "[]".to_string())
                .into(),
            entry.mode.clone().into(),
            entry.status.to_string().into(),
            entry.bytes.into(),
            entry.files_written.into(),
            entry.files_deleted.into(),
            entry.pre_restore_archive.clone().into(),
            entry.manifest_json.clone().into(),
            entry.error.clone().into(),
            entry.actor_user_id.clone().into(),
            Utc::now().to_rfc3339().into(),
        ],
    ))
    .await?;
    Ok(())
}

pub async fn list(limit: u64) -> Result<Vec<TransferLogEntryDto>> {
    let conn = get_connection();
    let rows = conn
        .query_all(Statement::from_sql_and_values(
            DatabaseBackend::Sqlite,
            "SELECT id, operation, snapshot_id, instance_id, source_instance_id, set_ids, mode,
                    status, bytes, files_written, files_deleted, pre_restore_archive,
                    error, actor_user_id, created_at
             FROM sys_dataset_transfer_log
             ORDER BY created_at DESC
             LIMIT ?",
            [limit.into()],
        ))
        .await?;

    rows.iter()
        .map(|row| {
            let set_ids: String = row.try_get("", "set_ids")?;
            Ok(TransferLogEntryDto {
                id: row.try_get("", "id")?,
                operation: row.try_get("", "operation")?,
                snapshot_id: row.try_get("", "snapshot_id")?,
                instance_id: row.try_get("", "instance_id")?,
                source_instance_id: row.try_get("", "source_instance_id")?,
                set_ids: serde_json::from_str(&set_ids).unwrap_or_default(),
                mode: row.try_get("", "mode")?,
                status: row.try_get("", "status")?,
                bytes: row.try_get("", "bytes")?,
                files_written: row.try_get("", "files_written")?,
                files_deleted: row.try_get("", "files_deleted")?,
                pre_restore_archive: row.try_get("", "pre_restore_archive")?,
                error: row.try_get("", "error")?,
                actor_user_id: row.try_get("", "actor_user_id")?,
                created_at: row.try_get("", "created_at")?,
            })
        })
        .collect()
}
