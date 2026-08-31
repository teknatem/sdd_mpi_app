//! Действие `match_nomenclature` — сопоставить a007 с a004 по артикулу (u505).
//!
//! Вызывает `MatchExecutor::execute_matching` синхронно (без `tokio::spawn`):
//! журнал эффектов обязан знать итог, а не только session_id.

use anyhow::Result;
use async_trait::async_trait;
use contracts::processes::{ActionActor, ActionInfo};
use contracts::usecases::u505_match_nomenclature::{progress::MatchStatus, request::MatchRequest};
use sea_orm::DatabaseConnection;
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::{Arc, OnceLock};
use uuid::Uuid;

use super::{MatchExecutor, ProgressTracker};
use crate::processes::actions::Action;

#[derive(Debug, Deserialize)]
struct Input {
    #[serde(default)]
    marketplace_id: Option<String>,
    #[serde(default)]
    overwrite_existing: bool,
    #[serde(default = "default_true")]
    ignore_case: bool,
}

fn default_true() -> bool {
    true
}

pub struct MatchNomenclature;

fn info() -> &'static ActionInfo {
    static INFO: OnceLock<ActionInfo> = OnceLock::new();
    INFO.get_or_init(|| ActionInfo {
        name: "match_nomenclature",
        method: "matchNomenclature",
        title: "Сопоставить номенклатуру",
        description: "Массово связывает a007_marketplace_product с a004_nomenclature по артикулу \
                      (u505). Однозначные совпадения заполняют nomenclature_ref; неоднозначные \
                      и отсутствующие — очищают связь.",
        capability: "action:match_nomenclature",
        reversible: true,
        write_tables: &["a007_marketplace_product", "a004_nomenclature"],
        input_schema: json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "type": "object",
            "additionalProperties": false,
            "properties": {
                "marketplace_id": {
                    "type": ["string", "null"],
                    "description": "UUID a005; пусто — все площадки"
                },
                "overwrite_existing": {
                    "type": "boolean",
                    "default": false
                },
                "ignore_case": {
                    "type": "boolean",
                    "default": true
                }
            }
        }),
    })
}

#[async_trait]
impl Action for MatchNomenclature {
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
        let request = MatchRequest {
            marketplace_id: input.marketplace_id.clone(),
            overwrite_existing: input.overwrite_existing,
            ignore_case: input.ignore_case,
        };

        let session_id = Uuid::new_v4().to_string();
        let tracker = Arc::new(ProgressTracker::new());
        // create_session нужен до execute_matching — иначе прогресс не пишется.
        let products = crate::domain::a007_marketplace_product::repository::list_for_matching(
            request.marketplace_id.as_deref(),
            !request.overwrite_existing,
        )
        .await?;
        tracker.create_session(session_id.clone(), Some(products.len() as i32));

        let executor = MatchExecutor::new(Arc::clone(&tracker));
        executor.execute_matching(&session_id, &request).await?;

        let progress = tracker.get_progress(&session_id);
        let status = progress
            .as_ref()
            .map(|p| match p.status {
                MatchStatus::Completed => "completed",
                MatchStatus::CompletedWithErrors => "completed_with_errors",
                MatchStatus::Failed => "failed",
                MatchStatus::InProgress => "in_progress",
            })
            .unwrap_or("unknown");

        Ok(json!({
            "session_id": session_id,
            "status": status,
            "processed": progress.as_ref().map(|p| p.processed).unwrap_or(0),
            "matched": progress.as_ref().map(|p| p.matched).unwrap_or(0),
            "cleared": progress.as_ref().map(|p| p.cleared).unwrap_or(0),
            "skipped": progress.as_ref().map(|p| p.skipped).unwrap_or(0),
            "ambiguous": progress.as_ref().map(|p| p.ambiguous).unwrap_or(0),
            "errors": progress.as_ref().map(|p| p.errors).unwrap_or(0),
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
            "effect": "сопоставление a007 ↔ a004 по артикулу (u505)",
            "marketplace_id": input.marketplace_id,
            "overwrite_existing": input.overwrite_existing,
            "ignore_case": input.ignore_case,
            "write_tables": ["a007_marketplace_product", "a004_nomenclature"],
            "reversible": true,
        }))
    }
}
