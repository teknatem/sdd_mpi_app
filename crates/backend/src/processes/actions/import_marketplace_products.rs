//! Действие `import_marketplace_products` — подтянуть товары кабинета (a007).
//!
//! Диспетчер по коду площадки (a005): WB → u504, YM → u503, Ozon → u502,
//! ЛеманаПро → u506. Читает API площадки и пишет только копию в MPI.

use anyhow::{Context, Result};
use async_trait::async_trait;
use chrono::Utc;
use contracts::domain::common::AggregateId;
use contracts::processes::{ActionActor, ActionInfo};
use sea_orm::DatabaseConnection;
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::{Arc, OnceLock};
use uuid::Uuid;

use super::Action;

#[derive(Debug, Deserialize)]
struct Input {
    connection_id: String,
}

pub struct ImportMarketplaceProducts;

fn info() -> &'static ActionInfo {
    static INFO: OnceLock<ActionInfo> = OnceLock::new();
    INFO.get_or_init(|| ActionInfo {
        name: "import_marketplace_products",
        method: "importMarketplaceProducts",
        title: "Импортировать товары маркетплейса",
        description: "Загружает каталог товаров кабинета в a007_marketplace_product. \
                      Диспетчер по площадке: WB/YM/Ozon/ЛеманаПро. На площадку ничего не пишет.",
        capability: "action:import_marketplace_products",
        reversible: true,
        write_tables: &["a007_marketplace_product"],
        input_schema: json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "type": "object",
            "required": ["connection_id"],
            "additionalProperties": false,
            "properties": {
                "connection_id": {
                    "type": "string",
                    "minLength": 1,
                    "description": "UUID кабинета a006_connection_mp"
                }
            }
        }),
    })
}

#[async_trait]
impl Action for ImportMarketplaceProducts {
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
        let connection_id =
            Uuid::parse_str(input.connection_id.trim()).context("connection_id: ожидается UUID")?;
        let connection = crate::domain::a006_connection_mp::service::get_by_id(connection_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("кабинет не найден: {connection_id}"))?;

        let marketplace_id = Uuid::parse_str(connection.marketplace_id.trim())
            .context("у кабинета некорректный marketplace_id")?;
        let marketplace = crate::domain::a005_marketplace::service::get_by_id(marketplace_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("маркетплейс не найден: {marketplace_id}"))?;
        let code = marketplace.base.code.to_lowercase();

        let session_id = Uuid::new_v4().to_string();
        let today = Utc::now().naive_utc().date();

        match code.as_str() {
            "mp-wb" | "wb" | "wildberries" => {
                use crate::usecases::u504_import_from_wildberries::{
                    ImportExecutor, ProgressTracker,
                };
                use contracts::usecases::u504_import_from_wildberries::request::{
                    ImportMode, ImportRequest,
                };
                let request = ImportRequest {
                    connection_id: connection.base.id.as_string(),
                    target_aggregates: vec!["a007_marketplace_product".to_string()],
                    date_from: today,
                    date_to: today,
                    mode: ImportMode::Background,
                };
                let executor = ImportExecutor::new(Arc::new(ProgressTracker::new()));
                executor
                    .execute_import(&session_id, &request, &connection)
                    .await?;
            }
            "mp-ym" | "ym" | "yandex" | "yandex_market" => {
                use crate::usecases::u503_import_from_yandex::{ImportExecutor, ProgressTracker};
                use contracts::usecases::u503_import_from_yandex::request::{
                    ImportMode, ImportRequest,
                };
                let request = ImportRequest {
                    connection_id: connection.base.id.as_string(),
                    target_aggregates: vec!["a007_marketplace_product".to_string()],
                    date_from: today,
                    date_to: today,
                    mode: ImportMode::Background,
                    incremental_by_update: false,
                };
                let executor = ImportExecutor::new(Arc::new(ProgressTracker::new()));
                executor
                    .execute_import(&session_id, &request, &connection)
                    .await?;
            }
            "mp-ozon" | "ozon" => {
                use crate::usecases::u502_import_from_ozon::{ImportExecutor, ProgressTracker};
                use contracts::usecases::u502_import_from_ozon::request::{
                    ImportMode, ImportRequest,
                };
                let request = ImportRequest {
                    connection_id: connection.base.id.as_string(),
                    target_aggregates: vec!["a007_marketplace_product".to_string()],
                    date_from: today,
                    date_to: today,
                    mode: ImportMode::Background,
                };
                let executor = ImportExecutor::new(Arc::new(ProgressTracker::new()));
                executor
                    .execute_import(&session_id, &request, &connection)
                    .await?;
            }
            "mp-lemanapro" | "lemanapro" | "lemana" => {
                use crate::usecases::u506_import_from_lemanapro::{
                    ImportExecutor, ProgressTracker,
                };
                use contracts::usecases::u506_import_from_lemanapro::request::{
                    ImportMode, ImportRequest,
                };
                let request = ImportRequest {
                    connection_id: connection.base.id.as_string(),
                    target_aggregates: vec!["a007_marketplace_product".to_string()],
                    date_from: today,
                    date_to: today,
                    mode: ImportMode::Background,
                };
                let executor = ImportExecutor::new(Arc::new(ProgressTracker::new()));
                executor
                    .execute_import(&session_id, &request, &connection)
                    .await?;
            }
            other => {
                anyhow::bail!(
                    "импорт товаров для площадки '{other}' (код a005) не поддержан Действием"
                );
            }
        }

        Ok(json!({
            "connection_id": input.connection_id,
            "marketplace_code": code,
            "session_id": session_id,
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
            "effect": "импорт товаров кабинета в a007_marketplace_product",
            "connection_id": input.connection_id,
            "write_tables": ["a007_marketplace_product"],
            "reversible": true,
            "note": "чтение API площадки; запись только в MPI",
        }))
    }
}
