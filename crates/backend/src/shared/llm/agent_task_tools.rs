//! Инструменты делегирования: постановка поручения специалисту другой
//! специализации и чтение результата.
//!
//! Каталог специализаций отдаётся готовым, а не оставляется на угадывание:
//! `AgentType::from_str` тотален и на неизвестной строке молча возвращает
//! `business_analyst`. Это худший из возможных отказов — поручение «успешно»
//! исполнит не тот специалист, и по результату это не отличить от правильного.
//!
//! Все гарды (глубина цепочки, самоделегирование, потолки очереди, дубли, длины)
//! стоят здесь, а не в промпте навыка: канал между агентами приглашает петлю
//! A→B→A, и каждый её виток — это реальный агентный прогон за реальные деньги.

use serde_json::{json, Value};

use super::types::ToolDefinition;
use contracts::domain::a017_llm_agent::aggregate::AgentType;
use contracts::domain::a042_agent_task::aggregate::{
    AgentTaskStatus, MAX_DELEGATION_DEPTH, MAX_OUTSTANDING_PER_CHAT,
};

use crate::domain::a042_agent_task::service as agent_task_service;

pub const AGENT_TASK_TOOL_NAMES: &[&str] = &[
    "list_agent_specializations",
    "create_agent_task",
    "list_my_agent_tasks",
    "get_agent_task_result",
];

/// Код регламента, который исполняет очередь. Фигурирует в тексте ответов
/// инструмента, чтобы модель могла честно сказать человеку, чего именно ждать.
const RUNNER_TASK_CODE: &str = "task029-agent-task-runner";

const MIN_REQUEST_CHARS: usize = 20;
const MAX_REQUEST_CHARS: usize = 8000;
const MAX_TITLE_CHARS: usize = 200;

/// Специализации, которым можно поручать.
///
/// Координатора здесь нет намеренно: он маршрутизатор задач, поручать ему —
/// поручать никому, а заодно это первый барьер против петли «координатор →
/// координатор».
fn delegatable() -> Vec<(AgentType, &'static str)> {
    vec![
        (
            AgentType::BusinessAnalyst,
            "Данные маркетплейсов, SQL-выборки, сводные отчёты и метаданные системы.",
        ),
        (
            AgentType::SalesAnalyst,
            "Продажи, выручка, заказы, маржа и прибыль.",
        ),
        (
            AgentType::Marketer,
            "Реклама, воронка продаж, поисковая аналитика, промо.",
        ),
        (
            AgentType::Financier,
            "Главная книга, сверка выручки, взаиморасчёты, комиссии.",
        ),
        (
            AgentType::SystemAdmin,
            "Диагностика системы, производительность, состояние заданий.",
        ),
        (
            AgentType::KbAdmin,
            "База знаний: пробелы, противоречия, подготовка статей.",
        ),
        (
            AgentType::PluginAdmin,
            "Разработка: плагины, проверки качества, разбор обращений.",
        ),
        (
            AgentType::Tester,
            "Обкатка пайплайна и проверки качества на локальной модели.",
        ),
    ]
}

fn delegatable_codes() -> Vec<&'static str> {
    delegatable().into_iter().map(|(t, _)| t.as_str()).collect()
}

pub fn agent_task_tool_definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "list_agent_specializations".into(),
            description: "Специализации AI-сотрудников, которым можно поручить подзадачу: код, \
                          название, зона ответственности. Вызывай ПЕРЕД create_agent_task и бери \
                          код отсюда: неизвестный код не отвергается, а молча превращается в \
                          бизнес-аналитика, и поручение уйдёт не тому специалисту."
                .into(),
            parameters: json!({ "type": "object", "properties": {} }),
        },
        ToolDefinition {
            name: "create_agent_task".into(),
            description:
                "Поставить поручение специалисту другой специализации. Ответа в этом ходе \
                          НЕ БУДЕТ: поручение исполнится отдельным прогоном регламента. Используй, \
                          когда вопрос требует компетенции, которой у тебя нет, а не для того, что \
                          можешь сделать сам."
                    .into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "target_agent_type": {
                        "type": "string",
                        "enum": delegatable_codes(),
                        "description": "Код специализации исполнителя из list_agent_specializations."
                    },
                    "title": {
                        "type": "string",
                        "description": "Короткий заголовок поручения (до 200 символов): попадёт в список поручений."
                    },
                    "request_text": {
                        "type": "string",
                        "description": "Полная постановка задачи. Исполнитель НЕ ВИДИТ этот диалог: \
                                        перескажи весь нужный контекст — период, кабинет/маркетплейс, \
                                        единицы измерения, что уже проверено и что именно нужно на выходе."
                    },
                    "context": {
                        "type": "object",
                        "description": "Необязательный структурный контекст (идентификаторы, даты, ссылки), \
                                        который нельзя терять при пересказе."
                    }
                },
                "required": ["target_agent_type", "title", "request_text"]
            }),
        },
        ToolDefinition {
            name: "list_my_agent_tasks".into(),
            description:
                "Поручения, поставленные из ЭТОГО диалога, с их статусами. Если в диалоге \
                          есть поручения — вызывай в начале хода, чтобы забрать готовые результаты \
                          и вплести их в ответ."
                    .into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "status": {
                        "type": "string",
                        "enum": ["pending", "processing", "done", "failed", "cancelled"],
                        "description": "Фильтр по статусу. Без него — все поручения диалога."
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Максимум записей (1-50, по умолчанию 20)."
                    }
                }
            }),
        },
        ToolDefinition {
            name: "get_agent_task_result".into(),
            description: "Результат конкретного поручения по его id. Пока статус не done, ответа \
                          нет — так и сообщи заказчику, не сочиняй ответ за исполнителя."
                .into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "task_id": { "type": "string", "description": "id поручения из create_agent_task или list_my_agent_tasks." }
                },
                "required": ["task_id"]
            }),
        },
    ]
}

pub async fn execute_agent_task_tool(
    name: &str,
    arguments: &str,
    chat_id: &str,
    agent_type: &AgentType,
    effect: &super::chat_effects::ChatEffect,
) -> Value {
    let args = serde_json::from_str::<Value>(arguments).unwrap_or_default();
    match name {
        "list_agent_specializations" => list_specializations(),
        "create_agent_task" => create_agent_task(&args, chat_id, agent_type, effect).await,
        "list_my_agent_tasks" => list_my_agent_tasks(&args, chat_id).await,
        "get_agent_task_result" => get_agent_task_result(&args).await,
        _ => json!({ "error": format!("Unknown agent task tool: {name}") }),
    }
}

fn list_specializations() -> Value {
    let items: Vec<Value> = delegatable()
        .into_iter()
        .map(|(agent_type, focus)| {
            json!({
                "agent_type": agent_type.as_str(),
                "display_name": agent_type.display_name(),
                "focus": focus,
            })
        })
        .collect();
    let total = items.len();
    json!({
        "specializations": items,
        "total": total,
        "note": format!(
            "Поручение исполняется регламентом «{RUNNER_TASK_CODE}» отдельным прогоном, \
             не в этом ходе диалога."
        ),
    })
}

/// Оболочка чата над Действием `create_agent_task`.
///
/// `agent_id` и `caller` сюда больше не приходят: заказчик, диалог и агент —
/// это провенанс, и он едет в `ChatEffect` (см. `ToolContext::effect`). Здесь
/// остаётся то, что действительно свойственно чату: цепочка делегирования,
/// потолок на диалог и распознавание дублей внутри хода.
async fn create_agent_task(
    args: &Value,
    chat_id: &str,
    agent_type: &AgentType,
    effect: &super::chat_effects::ChatEffect,
) -> Value {
    // ── Разбор и проверка аргументов ────────────────────────────────────────
    let Some(target_raw) = args.get("target_agent_type").and_then(Value::as_str) else {
        return json!({ "error": "target_agent_type обязателен — возьми код из list_agent_specializations." });
    };
    let codes = delegatable_codes();
    if !codes.contains(&target_raw) {
        return json!({
            "error": format!(
                "Неизвестная специализация '{target_raw}'. Допустимые: {}. \
                 Вызови list_agent_specializations.",
                codes.join(", ")
            )
        });
    }
    let target = AgentType::from_str(target_raw);

    if target == *agent_type {
        return json!({
            "error": format!(
                "Поручать самому себе бессмысленно: ты и есть {}. Сделай задачу сам \
                 или выбери другую специализацию.",
                agent_type.display_name()
            )
        });
    }

    let title = args
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_string();
    if title.is_empty() {
        return json!({ "error": "title обязателен." });
    }
    if title.chars().count() > MAX_TITLE_CHARS {
        return json!({ "error": format!("title длиннее {MAX_TITLE_CHARS} символов — это заголовок, а не постановка.") });
    }

    let request_text = args
        .get("request_text")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_string();
    let request_len = request_text.chars().count();
    if request_len < MIN_REQUEST_CHARS {
        return json!({
            "error": format!(
                "request_text короче {MIN_REQUEST_CHARS} символов. Исполнитель не видит твой \
                 диалог — перескажи контекст: период, кабинет, что нужно на выходе."
            )
        });
    }
    if request_len > MAX_REQUEST_CHARS {
        return json!({
            "error": format!(
                "request_text длиннее {MAX_REQUEST_CHARS} символов. Это постановка задачи, \
                 а не стенограмма: оставь суть и вынеси идентификаторы в context."
            )
        });
    }

    // ── Гард глубины: считаем сами, аргументам модели тут не место ──────────
    let chat_ref = (!chat_id.trim().is_empty()).then(|| chat_id.to_string());
    let chain = match agent_task_service::resolve_chain(chat_ref.as_deref()).await {
        Ok(chain) => chain,
        Err(e) => {
            return json!({ "error": format!("Не удалось определить цепочку поручений: {e}") })
        }
    };
    if chain.depth > MAX_DELEGATION_DEPTH {
        return json!({
            "error": format!(
                "Ты сам исполняешь поручение, а цепочка ограничена {MAX_DELEGATION_DEPTH} шагом — \
                 передать задачу дальше нельзя. Сделай, что можешь, и опиши недостающее прямо \
                 в своём ответе: заказчик увидит его целиком."
            )
        });
    }

    // ── Потолки очереди ─────────────────────────────────────────────────────
    if let Some(chat_ref) = chat_ref.as_deref() {
        match agent_task_service::count_outstanding_for_chat(chat_ref).await {
            Ok(count) if count >= MAX_OUTSTANDING_PER_CHAT => {
                return json!({
                    "error": format!(
                        "В этом диалоге уже {count} незакрытых поручений (предел {MAX_OUTSTANDING_PER_CHAT}). \
                         Дождись их результата через list_my_agent_tasks, прежде чем ставить новые."
                    )
                });
            }
            Ok(_) => {}
            Err(e) => {
                return json!({ "error": format!("Не удалось проверить очередь диалога: {e}") })
            }
        }

        // Повторный вызов инструмента в одном цикле — не ошибка модели, а её
        // обычное поведение: возвращаем уже созданное поручение.
        match agent_task_service::find_open_duplicate(chat_ref, &target, &request_text).await {
            Ok(Some(existing)) => {
                return json!({
                    "ok": true,
                    "duplicate": true,
                    "task_id": existing.to_string_id(),
                    "code": existing.base.code,
                    "status": existing.status.as_str(),
                    "note": "Такое поручение уже стоит в очереди — повторно не создавал.",
                });
            }
            Ok(None) => {}
            Err(e) => return json!({ "error": format!("Не удалось проверить дубли: {e}") }),
        }
    }

    // Потолок очереди целиком проверяет сама очередь (`a042::service::enqueue`):
    // это её свойство, а не свойство заказчика, и поручения от Процесса обязаны
    // упираться в тот же предел.

    // ── Постановка ──────────────────────────────────────────────────────────
    //
    // Всё выше — оболочка чата: цепочка делегирования, потолок на диалог, дубли.
    // Ниже — запись реестра Действий, общая с Этапом: та же схема входа, тот же
    // ключ идемпотентности, та же строка в журнале эффектов. Раньше здесь стоял
    // прямой `agent_task_service::enqueue`, и поручение из чата — в отличие от
    // поручения из Процесса — не оставляло следа в `sys_effect_log`.
    //
    // Провенанс (диалог, агент, заказчик, место в цепочке) едет актором, а не
    // входом Действия: поэтому у `create_agent_task` не появляется полей «для
    // чата», и схема входа остаётся одной на обе оболочки.
    let effect = effect
        .clone()
        .with_chain(chain.parent_task_ref, chain.depth);

    // Ключ различает поручения внутри одного хода по адресату и постановке:
    // повтор того же вызова схлопнется, два разных поручения — нет.
    let suffix = format!("{}:{}", target.as_str(), short_digest(&request_text));

    let mut input = json!({
        "title": title,
        "request_text": request_text,
        "target_agent_type": target.as_str(),
    });
    // Канон — имя поля агрегата (`payload`); инструмент чата исторически звал
    // его `context`. Принимаем оба, вниз отдаём одно.
    if let Some(payload) = args
        .get("payload")
        .or_else(|| args.get("context"))
        .filter(|value| value.is_object())
    {
        input["payload"] = payload.clone();
    }

    let mut outcome = effect.run("create_agent_task", input, &suffix).await;
    if outcome.get("ok").and_then(Value::as_bool) == Some(true) {
        outcome["note"] = json!(format!(
            "Результата ещё НЕТ и в этом ходе не будет. Поручение исполнится при ближайшем \
             прогоне регламента «{RUNNER_TASK_CODE}». Не вызывай get_agent_task_result сразу — \
             он вернёт pending. Заверши ход тем, что уже знаешь, и скажи человеку, кому и что \
             ты поручил. Результат заберёшь в следующем ходе через list_my_agent_tasks."
        ));
    }
    outcome
}

/// Короткий отпечаток текста для ключа идемпотентности.
///
/// Целиком постановку в ключ не кладём: ключ уходит в уникальный индекс БД, а
/// `request_text` бывает на тысячи символов.
fn short_digest(text: &str) -> String {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    text.trim().hash(&mut hasher);
    format!("{:x}", hasher.finish())
}

async fn list_my_agent_tasks(args: &Value, chat_id: &str) -> Value {
    if chat_id.trim().is_empty() {
        return json!({ "error": "Диалог не определён — список поручений недоступен." });
    }
    let limit = args
        .get("limit")
        .and_then(Value::as_i64)
        .unwrap_or(20)
        .clamp(1, 50) as u64;
    let status = args
        .get("status")
        .and_then(Value::as_str)
        .map(AgentTaskStatus::from_str);

    match agent_task_service::list_for_chat(chat_id, status, limit).await {
        Ok(items) => {
            let total = items.len();
            let done = items
                .iter()
                .filter(|t| t.status == AgentTaskStatus::Done)
                .count();
            json!({
                "tasks": items.iter().map(|t| json!({
                    "task_id": t.to_string_id(),
                    "code": t.base.code,
                    "title": t.base.description,
                    "status": t.status.as_str(),
                    "status_display": t.status.display_name(),
                    "target_agent_type": t.target_agent_type.as_str(),
                    "result_text": t.result_text,
                    "error": t.error,
                    "created_at": t.base.metadata.created_at.to_rfc3339(),
                    "finished_at": t.finished_at,
                })).collect::<Vec<_>>(),
                "total": total,
                "done": done,
            })
        }
        Err(e) => json!({ "error": format!("Не удалось прочитать поручения: {e}") }),
    }
}

async fn get_agent_task_result(args: &Value) -> Value {
    let Some(task_id) = args.get("task_id").and_then(Value::as_str) else {
        return json!({ "error": "task_id обязателен." });
    };
    match agent_task_service::get_by_id(task_id).await {
        Ok(Some(task)) => {
            let is_done = task.status == AgentTaskStatus::Done;
            let mut result = json!({
                "task_id": task.to_string_id(),
                "code": task.base.code,
                "title": task.base.description,
                "status": task.status.as_str(),
                "status_display": task.status.display_name(),
                "target_agent_type": task.target_agent_type.as_str(),
                "result_text": if is_done { task.result_text.clone() } else { None },
                "result_chat_ref": task.result_chat_ref,
                "result_artifact_ref": task.result_artifact_ref,
                "error": task.error,
                "attempts": task.attempts,
                "created_at": task.base.metadata.created_at.to_rfc3339(),
                "finished_at": task.finished_at,
            });
            if !is_done {
                let hint = match task.status {
                    AgentTaskStatus::Pending | AgentTaskStatus::Processing => format!(
                        "Результата ещё нет — поручение ждёт прогона регламента «{RUNNER_TASK_CODE}». \
                         Так и сообщи заказчику, не сочиняй ответ за исполнителя."
                    ),
                    AgentTaskStatus::Failed => {
                        "Поручение провалено. Сообщи заказчику причину из поля error, не выдавай \
                         догадку за результат."
                            .to_string()
                    }
                    AgentTaskStatus::Cancelled => "Поручение снято вручную.".to_string(),
                    AgentTaskStatus::Done => String::new(),
                };
                if let Value::Object(ref mut map) = result {
                    map.insert("hint".into(), Value::String(hint));
                }
            }
            result
        }
        Ok(None) => json!({ "error": format!("Поручение не найдено: {task_id}") }),
        Err(e) => json!({ "error": format!("Не удалось прочитать поручение: {e}") }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Координатор недоступен как цель: он маршрутизатор, а не исполнитель, и
    /// «координатор → координатор» было бы самой короткой петлёй в системе.
    #[test]
    fn coordinator_is_not_delegatable() {
        assert!(!delegatable_codes().contains(&AgentType::CoordinatorAdmin.as_str()));
        assert_eq!(delegatable().len(), 8);
    }

    /// Каждый код каталога обязан разбираться обратно в тот же тип: иначе
    /// поручение молча уедет к бизнес-аналитику.
    #[test]
    fn every_listed_code_parses_back() {
        for (agent_type, _) in delegatable() {
            assert_eq!(AgentType::from_str(agent_type.as_str()), agent_type);
        }
    }
}
