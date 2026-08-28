//! Действие `import_nomenclature` — подтянуть номенклатуру 1С (a004) и штрихкоды.
//!
//! Узкий вызов u501: только справочник, без организаций/контрагентов/закупок.
//! Исполняется синхронно — журнал эффектов обязан записать, что произошло.

use anyhow::{Context, Result};
use async_trait::async_trait;
use contracts::domain::common::AggregateId;
use contracts::processes::{ActionActor, ActionInfo};
use contracts::usecases::u501_import_from_ut::request::{ImportMode, ImportRequest};
use sea_orm::DatabaseConnection;
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::{Arc, OnceLock};
use uuid::Uuid;

use super::Action;
use crate::usecases::u501_import_from_ut::{ImportExecutor, ProgressTracker};

#[derive(Debug, Deserialize)]
struct Input {
    connection_1c_id: String,
    /// По умолчанию true: штрихкоды нужны для сопоставления YM.
    #[serde(default = "default_true")]
    include_barcodes: bool,
}

fn default_true() -> bool {
    true
}

pub struct ImportNomenclature;

fn info() -> &'static ActionInfo {
    static INFO: OnceLock<ActionInfo> = OnceLock::new();
    INFO.get_or_init(|| ActionInfo {
        name: "import_nomenclature",
        method: "importNomenclature",
        title: "Импортировать номенклатуру 1С",
        description: "Загружает справочник номенклатуры (a004) из подключения 1С через u501. \
                      Опционально — штрихкоды p901. Организации, контрагентов и закупки не трогает.",
        capability: "action:import_nomenclature",
        reversible: true,
        write_tables: &["a004_nomenclature", "p901_nomenclature_barcodes"],
        input_schema: json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "type": "object",
            "required": ["connection_1c_id"],
            "additionalProperties": false,
            "properties": {
                "connection_1c_id": {
                    "type": "string",
                    "minLength": 1,
                    "description": "UUID подключения a001_connection_1c"
                },
                "include_barcodes": {
                    "type": "boolean",
                    "default": true,
                    "description": "Также импортировать p901_barcodes"
                }
            }
        }),
    })
}

#[async_trait]
impl Action for ImportNomenclature {
    fn info(&self) -> &ActionInfo {
        info()
    }

    async fn execute(
        &self,
        _db: &DatabaseConnection,
        input: &Value,
        _actor: &ActionActor,
    ) -> Result<Value> {
        let input: Input = serde_json::from_value(input.clone())?;
        let connection_id = Uuid::parse_str(input.connection_1c_id.trim())
            .context("connection_1c_id: ожидается UUID")?;
        let connection = crate::domain::a001_connection_1c::service::get_by_id(connection_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("подключение 1С не найдено: {connection_id}"))?;

        let mut target_aggregates = vec!["a004_nomenclature".to_string()];
        if input.include_barcodes {
            target_aggregates.push("p901_barcodes".to_string());
        }

        let request = ImportRequest {
            connection_id: connection.base.id.as_string(),
            target_aggregates,
            mode: ImportMode::Background,
            delete_obsolete: false,
            period_from: None,
            period_to: None,
        };

        let session_id = Uuid::new_v4().to_string();
        let executor = ImportExecutor::new(Arc::new(ProgressTracker::new()));
        executor
            .execute_import(&session_id, &request, &connection)
            .await?;

        let progress = executor.get_progress(&session_id);
        Ok(json!({
            "connection_1c_id": input.connection_1c_id,
            "include_barcodes": input.include_barcodes,
            "session_id": session_id,
            "status": progress.as_ref().map(|p| format!("{:?}", p.status)),
            "total_errors": progress.as_ref().map(|p| p.total_errors).unwrap_or(0),
        }))
    }

    async fn plan(
        &self,
        _db: &DatabaseConnection,
        input: &Value,
        _actor: &ActionActor,
    ) -> Result<Value> {
        let input: Input = serde_json::from_value(input.clone())?;
        Ok(json!({
            "effect": "импорт номенклатуры 1С (a004) через u501",
            "connection_1c_id": input.connection_1c_id,
            "include_barcodes": input.include_barcodes,
            "write_tables": ["a004_nomenclature", "p901_nomenclature_barcodes"],
            "reversible": true,
        }))
    }
}
