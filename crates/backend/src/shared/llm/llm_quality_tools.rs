//! Инструменты оценки качества работы агентов: выборка неоценённых диалогов,
//! выписка по диалогу и запись вердиктов.
//!
//! Выборка и выписка отдаются готовыми, а не собираются моделью через
//! `execute_query`: сгенерированный SQL — самая частая точка отказа фоновых
//! LLM-задач (диалект, имена колонок, окно), а сама выборка кандидатов
//! детерминирована и в модели не нуждается. Судье остаётся то, ради чего его
//! зовут, — суждение.

use serde_json::{json, Value};

use super::types::ToolDefinition;
use super::verdicts::{self, NewVerdict, FAILURE_KINDS, VERDICTS};

pub const LLM_QUALITY_TOOL_NAMES: &[&str] = &[
    "list_chats_for_review",
    "get_chat_digest",
    "record_chat_verdicts",
];

pub fn llm_quality_tool_definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "list_chats_for_review".into(),
            description: "Диалоги за окно, которым ещё не поставлен вердикт: chat_id, заголовок, \
                          модель, специализация сотрудника, число сообщений. Служебные чаты \
                          фоновых задач исключены. Начинай оценку с этого списка."
                .into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "lookback_days": {
                        "type": "integer",
                        "description": "Глубина окна в днях (1-90, по умолчанию 7)."
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Максимум диалогов за один прогон (1-50, по умолчанию 20)."
                    }
                }
            }),
        },
        ToolDefinition {
            name: "get_chat_digest".into(),
            description: "Выписка по диалогу для оценки: сообщения (подрезанные), сводка вызовов \
                          инструментов по каждому инструменту и тексты упавших вызовов. По текстам \
                          ошибок отличается «модель ошиблась» от «инструмент сломан»."
                .into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "chat_id": { "type": "string" },
                    "max_chars": {
                        "type": "integer",
                        "description": "Предел длины текста одного сообщения (200-4000, по умолчанию 1200)."
                    }
                },
                "required": ["chat_id"]
            }),
        },
        ToolDefinition {
            name: "record_chat_verdicts".into(),
            description:
                "Записать вердикты пачкой — по одному на диалог. Вызывай ОДИН раз в конце \
                          прогона со всеми оценками сразу. Повторная оценка уже оценённого диалога \
                          не ошибка: она молча пропускается и возвращается в skipped."
                    .into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "verdicts": {
                        "type": "array",
                        "description": "Оценки диалогов, по одной на chat_id.",
                        "items": {
                            "type": "object",
                            "properties": {
                                "chat_id": { "type": "string" },
                                "verdict": {
                                    "type": "string",
                                    "enum": VERDICTS,
                                    "description": "solved — задача пользователя решена; \
                                                    partial — ответ частичный или требовал \
                                                    доуточнения; failed — не решена."
                                },
                                "failure_kind": {
                                    "type": "string",
                                    "enum": FAILURE_KINDS,
                                    "description": "Причина для partial/failed. У solved не указывай. \
                                                    Срабатывания гардов видны в сводке ошибок \
                                                    инструментов с пометкой 'гард: <код>': \
                                                    tool_loop — модель повторяла один вызов; \
                                                    plan_drift — пункт плана закрыт, а шага, на \
                                                    который он ссылается, в журнале нет."
                                },
                                "reason": {
                                    "type": "string",
                                    "description": "1-3 предложения: что именно пошло не так и по чьей вине \
                                                    (модель, инструмент, отсутствующие данные, пробел в KB). \
                                                    Без обоснования вердикт бесполезен."
                                },
                                "skill_id": { "type": "string", "description": "Навык, в котором шла работа, если понятен." },
                                "message_id": { "type": "string", "description": "Оцениваемый ответ ассистента, если оценка про конкретный." },
                                "case_id": { "type": "string", "description": "Только для голден-сета." },
                                "source": { "type": "string", "enum": ["audit", "golden"] }
                            },
                            "required": ["chat_id", "verdict", "reason"]
                        }
                    },
                    "judge_session_id": {
                        "type": "string",
                        "description": "session_id прогона задачи-судьи из текста задания."
                    }
                },
                "required": ["verdicts"]
            }),
        },
    ]
}

pub async fn execute_llm_quality_tool(name: &str, arguments: &str) -> Value {
    let args = serde_json::from_str::<Value>(arguments).unwrap_or_default();
    match name {
        "list_chats_for_review" => list_chats_for_review(&args).await,
        "get_chat_digest" => get_chat_digest(&args).await,
        "record_chat_verdicts" => record_chat_verdicts(&args).await,
        _ => json!({ "error": format!("Unknown LLM quality tool: {name}") }),
    }
}

async fn list_chats_for_review(args: &Value) -> Value {
    let lookback = args
        .get("lookback_days")
        .and_then(Value::as_i64)
        .unwrap_or(7);
    let limit = args.get("limit").and_then(Value::as_i64).unwrap_or(20);

    match verdicts::chats_awaiting_verdict(lookback, limit).await {
        Ok(chats) => {
            let total = chats.len();
            json!({ "chats": chats, "total": total, "lookback_days": lookback })
        }
        Err(error) => json!({ "error": error }),
    }
}

async fn get_chat_digest(args: &Value) -> Value {
    let Some(chat_id) = args.get("chat_id").and_then(Value::as_str) else {
        return json!({ "error": "chat_id is required" });
    };
    let max_chars = args
        .get("max_chars")
        .and_then(Value::as_i64)
        .unwrap_or(1200)
        .clamp(200, 4000) as usize;

    match verdicts::chat_digest(chat_id, max_chars).await {
        Ok(digest) => digest,
        Err(error) => json!({ "error": error }),
    }
}

async fn record_chat_verdicts(args: &Value) -> Value {
    let Some(items) = args.get("verdicts").and_then(Value::as_array) else {
        return json!({ "error": "verdicts array is required" });
    };
    if items.is_empty() {
        return json!({ "error": "verdicts array is empty" });
    }

    let judge_session_id = args
        .get("judge_session_id")
        .and_then(Value::as_str)
        .map(str::to_string);

    let mut batch = Vec::with_capacity(items.len());
    for item in items {
        let chat_id = item
            .get("chat_id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let verdict = item
            .get("verdict")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let reason = item
            .get("reason")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        if reason.trim().is_empty() {
            return json!({
                "error": format!("Вердикт для chat_id={chat_id} без reason. Обоснование обязательно.")
            });
        }

        // Снимок контекста берём из БД, а не со слов модели: специализация, модель
        // и счётчики трассы — факты, и подставленные наугад они испортят разрезы.
        let context = chat_context(&chat_id).await;

        batch.push(NewVerdict {
            source: item
                .get("source")
                .and_then(Value::as_str)
                .unwrap_or(verdicts::SOURCE_AUDIT)
                .to_string(),
            chat_id,
            message_id: item
                .get("message_id")
                .and_then(Value::as_str)
                .map(str::to_string),
            case_id: item
                .get("case_id")
                .and_then(Value::as_str)
                .map(str::to_string),
            agent_type: context.agent_type,
            skill_id: item
                .get("skill_id")
                .and_then(Value::as_str)
                .map(str::to_string),
            intent: context.intent,
            model: context.model,
            verdict,
            failure_kind: item
                .get("failure_kind")
                .and_then(Value::as_str)
                .map(str::to_string),
            reason,
            tool_calls: context.tool_calls,
            tool_failures: context.tool_failures,
            judge_session_id: judge_session_id.clone(),
            judge_model: None,
        });
    }

    match verdicts::insert_batch(&batch).await {
        Ok((inserted, skipped)) => json!({
            "success": true,
            "inserted": inserted,
            "skipped_as_duplicate": skipped,
        }),
        Err(error) => json!({ "error": error }),
    }
}

#[derive(Default)]
struct ChatContext {
    agent_type: Option<String>,
    model: Option<String>,
    intent: Option<String>,
    tool_calls: i64,
    tool_failures: i64,
}

async fn chat_context(chat_id: &str) -> ChatContext {
    use crate::shared::data_access::row_json::{fetch_json_rows, JsonBind};

    let mut context = ChatContext::default();

    if let Ok((rows, _)) = fetch_json_rows(
        "SELECT a.agent_type AS agent_type, \
                COALESCE(c.model_name, '') AS model, \
                (SELECT m.intent FROM a018_llm_chat_message m \
                  WHERE m.chat_id = c.id AND m.intent IS NOT NULL \
                  ORDER BY m.created_at DESC LIMIT 1) AS intent \
         FROM a018_llm_chat c \
         LEFT JOIN a017_llm_agent a ON a.id = c.agent_id \
         WHERE c.id = ?",
        vec![JsonBind::Text(chat_id.to_string())],
    )
    .await
    {
        if let Some(row) = rows.first() {
            context.agent_type = row
                .get("agent_type")
                .and_then(Value::as_str)
                .map(str::to_string);
            context.model = row
                .get("model")
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
                .map(str::to_string);
            context.intent = row
                .get("intent")
                .and_then(Value::as_str)
                .map(str::to_string);
        }
    }

    if let Ok((rows, _)) = fetch_json_rows(
        "SELECT COUNT(*) AS calls, SUM(CASE WHEN ok = 0 THEN 1 ELSE 0 END) AS failures \
         FROM sys_tool_trace WHERE chat_id = ?",
        vec![JsonBind::Text(chat_id.to_string())],
    )
    .await
    {
        if let Some(row) = rows.first() {
            context.tool_calls = row.get("calls").and_then(Value::as_i64).unwrap_or(0);
            context.tool_failures = row.get("failures").and_then(Value::as_i64).unwrap_or(0);
        }
    }

    context
}
