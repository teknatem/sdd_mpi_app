//! Действие `repost_documents` — перепровести агрегат за период.
//!
//! Самое тяжёлое из первых Действий: прогон идёт минутами и трогает проводки
//! Главной книги. Поэтому исполняется он **не отходя**
//! (`RepostExecutor::repost_aggregate_inline`): журнал эффектов обязан записать,
//! что произошло, а не что началось.

use anyhow::Result;
use async_trait::async_trait;
use contracts::processes::{ActionActor, ActionInfo};
use contracts::usecases::u508_repost_documents::aggregate_request::AggregateRepostRequest;
use sea_orm::DatabaseConnection;
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::{Arc, OnceLock};
use uuid::Uuid;

use super::Action;
use crate::usecases::u508_repost_documents::{ProgressTracker, RepostExecutor};

#[derive(Debug, Deserialize)]
struct Input {
    aggregate_key: String,
    date_from: String,
    date_to: String,
    #[serde(default)]
    only_posted: bool,
    #[serde(default)]
    connection_mp_refs: Vec<String>,
}

impl From<Input> for AggregateRepostRequest {
    fn from(input: Input) -> Self {
        AggregateRepostRequest {
            aggregate_key: input.aggregate_key,
            date_from: input.date_from,
            date_to: input.date_to,
            only_posted: input.only_posted,
            connection_mp_refs: input.connection_mp_refs,
        }
    }
}

pub struct RepostDocuments;

fn info() -> &'static ActionInfo {
    static INFO: OnceLock<ActionInfo> = OnceLock::new();
    INFO.get_or_init(|| ActionInfo {
        name: "repost_documents",
        method: "repostDocuments",
        title: "Перепровести документы агрегата",
        description: "Перепроводит документы агрегата за период с пересборкой связанных \
                      проекций и проводок Главной книги (u508).",
        capability: "action:repost_documents",
        // Обратного эффекта нет: перепроведение перезаписывает проводки, и
        // «вернуть как было» можно только новым перепроведением на прежних
        // данных. Человек должен видеть это в плане.
        reversible: false,
        // Перепроведение переписывает Главную книгу и обороты проекций:
        // одновременный импорт в те же таблицы даёт несводимый результат.
        write_tables: &[
            "sys_general_ledger",
            "p903_wb_finance_report",
            "p904_sales_data",
            "p907_ym_payment_report",
            "p909_mp_order_line_turnovers",
        ],
        input_schema: json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "type": "object",
            "required": ["aggregate_key", "date_from", "date_to"],
            "additionalProperties": false,
            "properties": {
                "aggregate_key": {
                    "type": "string",
                    "minLength": 1,
                    "description": "Ключ агрегата: a012_wb_sales, a015_wb_orders, …"
                },
                "date_from": { "type": "string", "pattern": "^\\d{4}-\\d{2}-\\d{2}$" },
                "date_to": { "type": "string", "pattern": "^\\d{4}-\\d{2}-\\d{2}$" },
                "only_posted": { "type": "boolean", "default": false },
                "connection_mp_refs": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Ограничение по кабинетам; пусто — все"
                }
            }
        }),
    })
}

#[async_trait]
impl Action for RepostDocuments {
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
        let request: AggregateRepostRequest = input.into();
        let session_id = Uuid::new_v4().to_string();
        let executor = RepostExecutor::new(Arc::new(ProgressTracker::new()));
        executor
            .repost_aggregate_inline(&session_id, &request)
            .await?;

        let progress = executor.get_progress(&session_id);
        Ok(json!({
            "aggregate_key": request.aggregate_key,
            "date_from": request.date_from,
            "date_to": request.date_to,
            "session_id": session_id,
            "processed": progress.as_ref().map(|p| p.processed),
            "total": progress.as_ref().map(|p| p.total),
            "errors": progress.as_ref().map(|p| p.errors).unwrap_or_default(),
            "error_messages": progress.as_ref().map(|p| p.error_messages.clone()).unwrap_or_default(),
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
            "effect": "перепроведение документов с пересборкой проекций и проводок ГК",
            "aggregate_key": input.aggregate_key,
            "date_from": input.date_from,
            "date_to": input.date_to,
            "only_posted": input.only_posted,
            "connection_mp_refs": input.connection_mp_refs,
            "reversible": false,
            "note": "прежние проводки за период будут перезаписаны",
        }))
    }
}
