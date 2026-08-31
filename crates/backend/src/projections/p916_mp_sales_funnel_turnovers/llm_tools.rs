//! Инструменты чата для ремонта воронки.
//!
//! Ядро чата их имён не знает: набор объявляется провайдером
//! [`FunnelRepairTools`], который перечисляет `composition::llm_tools`.

use async_trait::async_trait;
use serde_json::{json, Value};

use crate::shared::llm::tool_executor::ToolContext;
use crate::shared::llm::tool_provider::ToolProvider;
use crate::shared::llm::types::{ToolCaller, ToolDefinition};

/// Провайдер набора `funnel_repair`.
pub struct FunnelRepairTools;

#[async_trait]
impl ToolProvider for FunnelRepairTools {
    fn bundle(&self) -> &'static str {
        "funnel_repair"
    }

    fn tool_names(&self) -> &'static [&'static str] {
        FUNNEL_REPAIR_TOOL_NAMES
    }

    fn definitions(&self) -> Vec<ToolDefinition> {
        funnel_repair_tool_definitions()
    }

    /// Само исполнение ремонта меняет данные — для него нужно право
    /// `data_repair_execute`. Диагностика и статус read-only.
    fn required_capability(&self, tool_name: &str) -> Option<&'static str> {
        (tool_name == "execute_funnel_repair")
            .then_some(crate::shared::llm::skill_policy::DATA_REPAIR_EXECUTE)
    }

    async fn execute(&self, name: &str, arguments: &str, cx: &ToolContext<'_>) -> Value {
        execute_funnel_repair_tool(name, arguments, cx.chat_id, cx.agent_id, cx.caller).await
    }
}

pub const FUNNEL_REPAIR_TOOL_NAMES: &[&str] = &[
    "prepare_funnel_repair",
    "execute_funnel_repair",
    "get_funnel_repair_status",
];

pub fn funnel_repair_tool_definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "prepare_funnel_repair".into(),
            description: "Read-only диагностика p916 за точный период. Возвращает неизменяемый repair_spec, preview_text и payload_hash. Покажи preview_text и ограничения пользователю; выполнение возможно только в следующем ходе после его явного согласия.".into(),
            parameters: json!({
                "type":"object",
                "properties":{
                    "target":{"type":"string","enum":["p916_mp_sales_funnel_turnovers"]},
                    "date_from":{"type":"string","description":"YYYY-MM-DD"},
                    "date_to":{"type":"string","description":"YYYY-MM-DD"},
                    "connection_mp_refs":{"type":"array","items":{"type":"string"},"description":"Пустой список = все подключения WB/YM"}
                },
                "required":["target","date_from","date_to"]
            }),
        },
        ToolDefinition {
            name: "execute_funnel_repair".into(),
            description: "Запустить ранее подготовленный и явно подтверждённый пользователем repair-план. Требует coordinator_admin, точный repair_spec и payload_hash из prepare_funnel_repair; изменённый или подтверждённый в том же ходе план отклоняется.".into(),
            parameters: json!({
                "type":"object",
                "properties":{
                    "repair_spec":{"type":"object"},
                    "payload_hash":{"type":"string"},
                    "confirm":{"type":"boolean"}
                },
                "required":["repair_spec","payload_hash","confirm"]
            }),
        },
        ToolDefinition {
            name: "get_funnel_repair_status".into(),
            description: "Получить сохранённый прогресс и итог p916 repair-run, включая QC до/после, ошибки и ограничения.".into(),
            parameters: json!({
                "type":"object",
                "properties":{"repair_run_id":{"type":"string"}},
                "required":["repair_run_id"]
            }),
        },
    ]
}

pub async fn execute_funnel_repair_tool(
    name: &str,
    arguments: &str,
    chat_id: &str,
    agent_id: &str,
    caller: Option<&ToolCaller>,
) -> Value {
    let args: Value = serde_json::from_str(arguments).unwrap_or_else(|_| json!({}));
    match name {
        "prepare_funnel_repair" => {
            let target = args
                .get("target")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let date_from = args
                .get("date_from")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let date_to = args
                .get("date_to")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let refs = args
                .get("connection_mp_refs")
                .and_then(Value::as_array)
                .map(|items| {
                    items
                        .iter()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default();
            match super::repair::prepare(target, date_from, date_to, refs).await {
                Ok(prepared) => {
                    json!({"ok":true,"prepared":prepared,"next_step":"Покажи preview_text, ограничения и действия пользователю. Не вызывай execute_funnel_repair в этом ходе; дождись следующего сообщения с явным согласием."})
                }
                Err(error) => json!({"ok":false,"error":error.to_string()}),
            }
        }
        "execute_funnel_repair" => {
            let Some(caller) = caller else {
                return json!({"ok":false,"error":"Фоновый сценарий не может запускать исправление данных от лица пользователя"});
            };
            if args.get("confirm").and_then(Value::as_bool) != Some(true) {
                return json!({"ok":false,"error":"Требуется явное согласие пользователя и confirm=true"});
            }
            let Some(spec_value) = args.get("repair_spec") else {
                return json!({"ok":false,"error":"repair_spec обязателен"});
            };
            let spec: super::repair::RepairSpec = match serde_json::from_value(spec_value.clone()) {
                Ok(value) => value,
                Err(error) => {
                    return json!({"ok":false,"error":format!("Некорректный repair_spec: {error}")})
                }
            };
            let hash = args
                .get("payload_hash")
                .and_then(Value::as_str)
                .unwrap_or_default();
            match prepared_in_previous_turn(chat_id, hash).await {
                Ok(true) => {}
                Ok(false) => {
                    return json!({
                        "ok":false,
                        "error":"План не проходил prepare_funnel_repair в предыдущем ходе этого чата либо был изменён.",
                        "next_step":"Снова вызови prepare_funnel_repair, покажи план пользователю и дождись следующего сообщения."
                    })
                }
                Err(error) => {
                    return json!({"ok":false,"error":format!("Не удалось проверить подтверждение: {error}")})
                }
            }
            match super::repair::start(spec, hash, chat_id, agent_id, Some(caller.user_id.as_str()))
                .await
            {
                Ok(run) => {
                    json!({"ok":true,"run":run,"next_step":"Периодически вызывай get_funnel_repair_status до финального статуса."})
                }
                Err(error) => json!({"ok":false,"error":error.to_string()}),
            }
        }
        "get_funnel_repair_status" => {
            let id = args
                .get("repair_run_id")
                .and_then(Value::as_str)
                .unwrap_or_default();
            match super::repair::get_for_chat(id, chat_id).await {
                Ok(Some(run)) => json!({"ok":true,"run":run}),
                Ok(None) => json!({"ok":false,"error":"Repair-run не найден"}),
                Err(error) => json!({"ok":false,"error":error.to_string()}),
            }
        }
        _ => json!({"ok":false,"error":format!("Unknown funnel repair tool: {name}")}),
    }
}

async fn prepared_in_previous_turn(chat_id: &str, payload_hash: &str) -> anyhow::Result<bool> {
    let db = crate::shared::data::db::get_connection();
    let entries = crate::domain::a018_llm_chat::repository::find_tool_trace_by_tool(
        db,
        chat_id,
        "prepare_funnel_repair",
    )
    .await?;
    Ok(entries.iter().any(|entry| {
        entry
            .output
            .as_ref()
            .and_then(|output| output.get("prepared"))
            .and_then(|prepared| prepared.get("payload_hash"))
            .and_then(Value::as_str)
            == Some(payload_hash)
    }))
}
