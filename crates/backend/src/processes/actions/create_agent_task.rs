//! Действие `create_agent_task` — поставить поручение AI-сотруднику.
//!
//! Единственное из первых Действий, чей эффект виден человеку: поручение
//! попадает в очередь `a042` и будет исполнено сотрудником нужной
//! специализации. Поручение человеку (`request_human_action`) появится вместе с
//! экземплярами процессов — ожидание человека это состояние экземпляра, а не
//! отдельная сущность (ADR-0011 п.9, п.13).

use anyhow::Result;
use async_trait::async_trait;
use contracts::domain::a017_llm_agent::aggregate::AgentType;
use contracts::domain::common::AggregateId;
use contracts::processes::{ActionActor, ActionInfo};
use sea_orm::DatabaseConnection;
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::OnceLock;

use super::Action;
use crate::domain::a042_agent_task::service::{self as agent_task, EnqueueRequest};

#[derive(Debug, Deserialize)]
struct Input {
    title: String,
    request_text: String,
    target_agent_type: String,
    #[serde(default)]
    payload: Option<Value>,
}

pub struct CreateAgentTask;

fn info() -> &'static ActionInfo {
    static INFO: OnceLock<ActionInfo> = OnceLock::new();
    INFO.get_or_init(|| ActionInfo {
        name: "create_agent_task",
        method: "createAgentTask",
        title: "Поставить поручение AI-сотруднику",
        description: "Кладёт поручение в очередь a042 для сотрудника указанной \
                      специализации; исполняет его task029.",
        capability: "action:create_agent_task",
        // Обратимо: поручение отменяется отдельным эффектом.
        reversible: true,
        write_tables: &["a042_agent_task"],
        input_schema: json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "type": "object",
            "required": ["title", "request_text", "target_agent_type"],
            "additionalProperties": false,
            "properties": {
                "title": { "type": "string", "minLength": 1, "maxLength": 255 },
                "request_text": { "type": "string", "minLength": 1 },
                "target_agent_type": {
                    "type": "string",
                    "minLength": 1,
                    "description": "Специализация исполнителя (a017 agent_type)"
                },
                "payload": {
                    "type": "object",
                    "description": "Структурный контекст, который нельзя терять при пересказе"
                }
            }
        }),
    })
}

/// Полезная нагрузка поручения плюс обратная ссылка на того, кто просил.
fn payload_with_origin(payload: Option<Value>, actor: &ActionActor) -> String {
    let mut object = match payload {
        Some(Value::Object(map)) => map,
        Some(other) => {
            let mut map = serde_json::Map::new();
            map.insert("payload".to_string(), other);
            map
        }
        None => serde_json::Map::new(),
    };
    // У заказчика-человека провенанс уже лёг в колонки `requested_by_*`;
    // дублировать его в нагрузке незачем, а вот у Процесса колонок нет.
    if let ActionActor::Process { .. } = actor {
        object.insert(
            "requested_by".to_string(),
            json!({
                "kind": "process",
                "instance_id": actor.instance_id(),
                "stage_code": actor.stage_code(),
            }),
        );
    }
    Value::Object(object).to_string()
}

#[async_trait]
impl Action for CreateAgentTask {
    fn info(&self) -> &ActionInfo {
        info()
    }

    async fn execute(
        &self,
        _db: &DatabaseConnection,
        input: &Value,
        actor: &ActionActor,
    ) -> Result<Value> {
        let input: Input = serde_json::from_value(input.clone())?;
        // Куда положить обратную ссылку, решает не Действие, а форма провенанса:
        // у заказчика-человека для неё есть колонки (`requested_by_*` — чат,
        // агент, пользователь), у Процесса их нет, и ссылка едет в полезной
        // нагрузке. Исполнитель в обоих случаях видит, кто его позвал.
        let task = agent_task::enqueue(EnqueueRequest {
            title: input.title,
            request_text: input.request_text,
            target_agent_type: AgentType::from_str(&input.target_agent_type),
            payload_json: Some(payload_with_origin(input.payload, actor)),
            requested_by_agent_ref: actor.agent_ref().map(str::to_string),
            requested_by_chat_ref: actor.chat_ref().map(str::to_string),
            requested_by_user_ref: actor.user_id().map(str::to_string),
            parent_task_ref: actor.parent_task_ref().map(str::to_string),
            depth: actor.depth(),
        })
        .await?;
        Ok(json!({
            "task_id": task.base.id.as_string(),
            "code": task.base.code,
            "status": task.status.as_str(),
            "target_agent_type": task.target_agent_type.as_str(),
            "target_display_name": task.target_agent_type.display_name(),
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
            "effect": "постановка поручения в очередь a042",
            "title": input.title,
            "target_agent_type": input.target_agent_type,
            "request_text": input.request_text,
        }))
    }
}
