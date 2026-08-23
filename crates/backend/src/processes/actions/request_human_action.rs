//! Действие `request_human_action` — позвать человека.
//!
//! Само ожидание — это **состояние экземпляра**, а не эта операция (ADR-0011
//! п.9, п.13): экземпляр встаёт в `waiting` по ребру графа, и инбокс есть
//! список таких экземпляров. Действие отвечает за другое — чтобы просьба была
//! видна **вне** экрана процессов.
//!
//! Поэтому оно порождает `sys_ticket`, а не собственную очередь: очередей к
//! человеку в приложении уже три (тикеты, поручения `a042`, нарушения quality),
//! и четвёртая ничего не добавила бы, кроме ещё одного места, куда надо не
//! забыть посмотреть.
//!
//! Тикет несёт `request_key` — токен экземпляра. Им же адресуется событие
//! `human.action.done`, поэтому «сделано» из инбокса и «сделано» откуда угодно
//! ещё — это один и тот же факт с одним и тем же ключом.

use anyhow::Result;
use async_trait::async_trait;
use contracts::processes::{ActionActor, ActionInfo};
use contracts::system::tickets::{CreateTicketRequest, TicketPriority, TicketType};
use sea_orm::DatabaseConnection;
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::OnceLock;

use super::Action;
use crate::system::tickets::service as tickets;

/// Автор тикетов, порождённых механизмом.
///
/// Синтетический пользователь, а не «первый администратор»: у просьбы от
/// Процесса нет человека-заказчика, и подставлять сюда живого — значит вменять
/// ему чужую работу. Следствие, о котором надо знать: тикеты видны
/// администраторам (список тикетов фильтруется по автору), а обычному
/// пользователю просьба видна в инбоксе механизма.
const PROCESS_AUTHOR: &str = "process";

#[derive(Debug, Deserialize)]
struct Input {
    title: String,
    request_text: String,
    #[serde(default)]
    request_key: Option<String>,
}

pub struct RequestHumanAction;

fn info() -> &'static ActionInfo {
    static INFO: OnceLock<ActionInfo> = OnceLock::new();
    INFO.get_or_init(|| ActionInfo {
        name: "request_human_action",
        method: "requestHumanAction",
        title: "Позвать человека",
        description: "Заводит тикет с просьбой к человеку и обратной ссылкой на экземпляр \
                      процесса. Само ожидание задаётся ребром графа, а не этим Действием.",
        capability: "action:request_human_action",
        // Обратимо: тикет закрывается отдельным действием человека.
        reversible: true,
        write_tables: &["sys_ticket"],
        input_schema: json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "type": "object",
            "required": ["title", "request_text"],
            "additionalProperties": false,
            "properties": {
                "title": { "type": "string", "minLength": 1, "maxLength": 255 },
                "request_text": { "type": "string", "minLength": 1 },
                "request_key": {
                    "type": "string",
                    "description": "Ключ просьбы; по умолчанию — токен экземпляра процесса"
                }
            }
        }),
    })
}

/// Ключ просьбы по умолчанию — **токен корреляции экземпляра**, а не его
/// идентификатор.
///
/// Это не мелочь: тем же ключом адресуется событие `human.action.done`, и
/// ожидание на ребре графа строит токен именно из корреляции. Возьми мы id,
/// «сделано» никогда не сошлось бы с ожиданием — молча.
async fn request_key_of(db: &DatabaseConnection, actor: &ActionActor) -> Result<String> {
    let Some(instance_id) = actor.instance_id() else {
        return Ok("manual".to_string());
    };
    Ok(crate::processes::instances::find(db, instance_id)
        .await?
        .map(|instance| instance.correlation_token)
        .unwrap_or_else(|| instance_id.to_string()))
}

/// Контекст просьбы: по нему человек находит, откуда она взялась, а механизм —
/// какое событие её закроет.
fn context(actor: &ActionActor, request_key: &str) -> String {
    json!({
        "kind": "process_request",
        "request_key": request_key,
        "instance_id": actor.instance_id(),
        "stage_code": actor.stage_code(),
    })
    .to_string()
}

#[async_trait]
impl Action for RequestHumanAction {
    fn info(&self) -> &ActionInfo {
        info()
    }

    async fn execute(
        &self,
        db: &DatabaseConnection,
        input: &Value,
        actor: &ActionActor,
    ) -> Result<Value> {
        let input: Input = serde_json::from_value(input.clone())?;
        let request_key = match input.request_key.clone() {
            Some(key) => key,
            None => request_key_of(db, actor).await?,
        };

        let ticket = tickets::create(
            &tickets::Requester {
                user_id: PROCESS_AUTHOR.to_string(),
                is_admin: true,
            },
            CreateTicketRequest {
                title: input.title,
                description: input.request_text,
                ticket_type: TicketType::Improvement,
                priority: TicketPriority::Normal,
                deadline: None,
                assignee_user_id: None,
                tags: vec!["процесс".to_string()],
                context_page_key: None,
                context_json: Some(context(actor, &request_key)),
                origin: None,
            },
        )
        .await?;

        Ok(json!({
            "ticket_id": ticket.id,
            "code": ticket.code,
            "request_key": request_key,
        }))
    }

    async fn plan(
        &self,
        db: &DatabaseConnection,
        input: &Value,
        actor: &ActionActor,
    ) -> Result<Value> {
        let input: Input = serde_json::from_value(input.clone())?;
        let request_key = match input.request_key.clone() {
            Some(key) => key,
            None => request_key_of(db, actor).await?,
        };
        Ok(json!({
            "effect": "тикет с просьбой к человеку",
            "title": input.title,
            "request_key": request_key,
        }))
    }
}
