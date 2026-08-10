//! LLM-инструменты обратной связи: консультация по разделам приложения и оформление
//! тикета (`sys_ticket`) от лица собеседника.
//!
//! Оформление тикета — двухшаговое и детерминированное:
//!
//! 1. `ticket_validate(fields)` — чек-лист полноты. Возвращает `missing`, `preview_text`
//!    и `payload_hash` (отпечаток нормализованных полей).
//! 2. Модель показывает `preview_text` пользователю и ЖДЁТ его ответа.
//! 3. `ticket_create(fields)` — создание. Проходит, только если в журнале вызовов
//!    (`sys_tool_trace`) уже лежит `ticket_validate` этого чата с тем же `payload_hash`
//!    и `complete = true`.
//!
//! Гейт работает благодаря порядку записи: журнал сохраняется пачкой ПОСЛЕ ответа
//! ассистента, поэтому во время tool-цикла в таблице видны только вызовы прошлых ходов.
//! Иначе говоря, найденная запись = «превью было показано, пользователь успел ответить».

use super::types::{ToolCaller, ToolDefinition};
use contracts::system::tickets::{
    CreateTicketRequest, TicketPriority, TicketType, TICKET_ORIGIN_LLM_CHAT,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::system::tickets::service as ticket_service;

/// Имена инструментов тикетов (для маршрутизации в `execute_tool_call`).
pub const TICKET_TOOL_NAMES: &[&str] = &[
    "ticket_search",
    "ticket_validate",
    "ticket_create",
    "find_page_help",
    "get_user_recent_pages",
];

/// Тег раздела пользовательской документации в базе знаний.
const USER_GUIDE_TAG: &str = "user-guide";

/// Минимальная длина осмысленного заголовка.
const MIN_TITLE_CHARS: usize = 10;

/// Заголовки, которые ничего не сообщают о проблеме.
const USELESS_TITLES: &[&str] = &[
    "не работает",
    "ошибка",
    "проблема",
    "баг",
    "помогите",
    "вопрос",
    "тикет",
];

// ─── Определения инструментов ────────────────────────────────────────────────

pub fn ticket_tool_definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "find_page_help".into(),
            description: "Найти в базе знаний статьи пользовательской документации по \
                          разделу приложения. Вызывай ПЕРЕД тем, как отвечать на вопрос \
                          «как это работает / где найти / почему так»: сначала документация, \
                          потом собственные рассуждения. Возвращает список статей — полный \
                          текст бери через get_knowledge(id)."
                .into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "page_key": {
                        "type": "string",
                        "description": "Ключ страницы приложения (например 'p904_sales_data' \
                                        или 'a015_wb_orders_details_<uuid>'). Берётся из блока \
                                        контекста прикреплённых страниц."
                    },
                    "query": {
                        "type": "string",
                        "description": "Дополнительные слова-теги темы, если страница неизвестна."
                    }
                }
            }),
        },
        ToolDefinition {
            name: "get_user_recent_pages".into(),
            description: "Последние страницы, которые открывал собеседник. Помогает \
                          восстановить путь «был там → нажал сюда → сломалось», когда \
                          пользователь не помнит, где именно это случилось."
                .into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "limit": {
                        "type": "integer",
                        "description": "Сколько последних страниц вернуть (1–30, по умолчанию 10).",
                        "minimum": 1,
                        "maximum": 30
                    }
                }
            }),
        },
        ToolDefinition {
            name: "ticket_search".into(),
            description: "Поиск по тикетам собеседника (администратор видит все). \
                          Вызывай ПЕРЕД оформлением нового тикета: о такой же проблеме \
                          уже может быть заведено обращение — тогда сообщи его номер и \
                          статус вместо создания дубля."
                .into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Слова из номера или заголовка тикета."
                    },
                    "status": {
                        "type": "string",
                        "description": "Фильтр по статусу.",
                        "enum": ["new", "in_progress", "waiting", "resolved", "closed", "rejected"]
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Сколько вернуть (1–50, по умолчанию 10).",
                        "minimum": 1,
                        "maximum": 50
                    }
                }
            }),
        },
        ToolDefinition {
            name: "ticket_validate".into(),
            description: "Проверить черновик тикета на полноту. Ничего не создаёт и не \
                          сохраняет. Возвращает: complete (готов ли), missing (чего не \
                          хватает — по каждому пункту задай пользователю отдельный вопрос), \
                          warnings и preview_text (готовое превью для показа человеку). \
                          Обязательный шаг: без успешной валидации ticket_create откажет."
                .into(),
            parameters: ticket_fields_schema(
                "Черновик тикета. Собирается из диалога, не выдумывай содержимое за пользователя.",
            ),
        },
        ToolDefinition {
            name: "ticket_create".into(),
            description: "Создать тикет от имени собеседника. Вызывать ТОЛЬКО после того, \
                          как ticket_validate вернул complete=true, ты показал preview_text \
                          пользователю и он в ОТДЕЛЬНОЙ следующей реплике подтвердил создание. \
                          Поля должны быть в точности те же, что прошли валидацию — иначе \
                          отказ. К тикету автоматически прикладываются изображения из этого \
                          чата и контекст открытой страницы."
                .into(),
            parameters: ticket_fields_schema(
                "Поля тикета — ровно те же, что были переданы в ticket_validate.",
            ),
        },
    ]
}

/// Общая схема полей тикета для validate/create: один набор — один отпечаток.
fn ticket_fields_schema(intro: &str) -> Value {
    json!({
        "type": "object",
        "description": intro,
        "properties": {
            "ticket_type": {
                "type": "string",
                "description": "Тип обращения: bug — что-то работает неверно; improvement — \
                                доработать существующее; idea — новая возможность; \
                                question — вопрос, на который не удалось ответить в чате.",
                "enum": ["bug", "improvement", "idea", "question"]
            },
            "title": {
                "type": "string",
                "description": "Краткая суть одной строкой, минимум 10 символов. \
                                Не «не работает», а «в отчёте продаж не применяется фильтр кабинета»."
            },
            "description": {
                "type": "string",
                "description": "Подробности в markdown, разбитые на обязательные для типа \
                                разделы (см. missing из ticket_validate). Для bug: \
                                '## Что происходит', '## Как воспроизвести', '## Ожидаемое поведение'. \
                                Для improvement/idea: '## Как сейчас', '## Как хочется', '## Зачем'."
            },
            "priority": {
                "type": "string",
                "description": "Приоритет со слов пользователя: high — работа заблокирована, \
                                low — мелочь, не мешает.",
                "enum": ["low", "normal", "high"]
            },
            "page_key": {
                "type": "string",
                "description": "Ключ страницы, где всё происходит (из блока контекста). \
                                Для bug обязателен."
            },
            "tags": {
                "type": "array",
                "description": "Необязательные метки.",
                "items": { "type": "string" }
            }
        },
        "required": ["ticket_type", "title", "description"]
    })
}

// ─── Нормализация полей и отпечаток ──────────────────────────────────────────

/// Нормализованный черновик: то, что валидируется, показывается и хэшируется.
#[derive(Debug, Clone, PartialEq)]
struct TicketDraft {
    ticket_type: TicketType,
    title: String,
    description: String,
    priority: TicketPriority,
    page_key: Option<String>,
    tags: Vec<String>,
}

impl TicketDraft {
    fn parse(args: &Value) -> Self {
        let str_field = |key: &str| -> String {
            args.get(key)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .trim()
                .to_string()
        };
        let mut tags: Vec<String> = args
            .get("tags")
            .and_then(Value::as_array)
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str())
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect()
            })
            .unwrap_or_default();
        tags.sort();
        tags.dedup();

        Self {
            ticket_type: str_field("ticket_type").as_str().into(),
            title: str_field("title"),
            description: str_field("description"),
            priority: str_field("priority").as_str().into(),
            page_key: Some(str_field("page_key")).filter(|s| !s.is_empty()),
            tags,
        }
    }

    /// Отпечаток нормализованного черновика.
    ///
    /// Устойчив к порядку ключей в JSON, регистру и переформатированию пробелов —
    /// повторная выдача моделью того же текста с другими отступами не должна ронять
    /// гейт. Но любая правка содержания (слово в заголовке, шаг воспроизведения,
    /// приоритет) отпечаток меняет — а значит требует новой валидации и нового
    /// подтверждения пользователя.
    fn payload_hash(&self) -> String {
        let canonical = format!(
            "{}\u{1}{}\u{1}{}\u{1}{}\u{1}{}\u{1}{}",
            self.ticket_type.as_str(),
            normalize_ws(&self.title),
            normalize_ws(&self.description),
            self.priority.as_str(),
            self.page_key.clone().unwrap_or_default(),
            self.tags.join(","),
        );
        let digest = Sha256::digest(canonical.as_bytes());
        format!("{:x}", digest)
    }

    fn preview_text(&self) -> String {
        format!(
            "Тип: {}\nЗаголовок: {}\nПриоритет: {}\nСтраница: {}\nМетки: {}\n\n{}",
            self.ticket_type.label_ru(),
            self.title,
            self.priority.label_ru(),
            self.page_key.as_deref().unwrap_or("—"),
            if self.tags.is_empty() {
                "—".to_string()
            } else {
                self.tags.join(", ")
            },
            self.description
        )
    }
}

/// Схлопнуть пробельные последовательности и привести к нижнему регистру —
/// каноническая форма текстового поля для отпечатка.
fn normalize_ws(s: &str) -> String {
    s.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// Результат чек-листа: чего не хватает и на что стоит обратить внимание.
struct Checklist {
    missing: Vec<String>,
    warnings: Vec<String>,
}

fn check(draft: &TicketDraft) -> Checklist {
    let mut missing = Vec::new();
    let mut warnings = Vec::new();

    let title_lc = draft.title.to_lowercase();
    if draft.title.is_empty() {
        missing.push("Заголовок: краткая суть обращения одной строкой.".to_string());
    } else if draft.title.chars().count() < MIN_TITLE_CHARS
        || USELESS_TITLES.contains(&title_lc.trim_end_matches(['.', '!', '?']))
    {
        missing.push(format!(
            "Заголовок слишком общий («{}»): нужна конкретная формулировка — что именно и где.",
            draft.title
        ));
    }

    if draft.description.trim().is_empty() {
        missing.push("Описание: подробности обращения.".to_string());
    }

    let has_section = |title: &str| {
        draft
            .description
            .to_lowercase()
            .contains(&title.to_lowercase())
    };

    match draft.ticket_type {
        TicketType::Bug => {
            for section in [
                "## Что происходит",
                "## Как воспроизвести",
                "## Ожидаемое поведение",
            ] {
                if !has_section(section) {
                    missing.push(format!("Раздел описания «{section}»."));
                }
            }
            if draft.page_key.is_none() {
                missing.push(
                    "Страница, на которой воспроизводится проблема (page_key из контекста)."
                        .to_string(),
                );
            }
        }
        TicketType::Improvement | TicketType::Idea => {
            for section in ["## Как сейчас", "## Как хочется", "## Зачем"] {
                if !has_section(section) {
                    missing.push(format!("Раздел описания «{section}»."));
                }
            }
        }
        TicketType::Question => {
            warnings.push(
                "Тип «Вопрос»: тикет здесь — крайняя мера. Сначала попробуй ответить сам, \
                 опираясь на find_page_help и search_knowledge; заводи тикет, только если \
                 ответа нет и пользователь на нём настаивает."
                    .to_string(),
            );
        }
    }

    if matches!(draft.priority, TicketPriority::High) {
        warnings.push(
            "Высокий приоритет: убедись, что работа пользователя действительно заблокирована."
                .to_string(),
        );
    }

    Checklist { missing, warnings }
}

// ─── Диспетчер ───────────────────────────────────────────────────────────────

/// Выполнить вызов инструмента тикетов от лица собеседника.
pub async fn execute_ticket_tool(
    name: &str,
    arguments_json: &str,
    chat_id: &str,
    caller: &ToolCaller,
) -> Value {
    let args: Value = serde_json::from_str(arguments_json).unwrap_or_else(|_| json!({}));

    match name {
        "find_page_help" => find_page_help(&args),
        "get_user_recent_pages" => get_user_recent_pages(&args, caller).await,
        "ticket_search" => ticket_search(&args, caller).await,
        "ticket_validate" => ticket_validate(&args),
        "ticket_create" => ticket_create(&args, chat_id, caller).await,
        other => json!({ "error": format!("Unknown ticket tool: {other}") }),
    }
}

fn find_page_help(args: &Value) -> Value {
    let page_key = args
        .get("page_key")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_string();
    let query = args
        .get("query")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_string();

    // Теги ищем от частного к общему: конкретная страница → её базовый ключ
    // (список без `_details_<id>`) → весь пользовательский раздел.
    let mut tags: Vec<String> = vec![USER_GUIDE_TAG.to_string()];
    if !page_key.is_empty() {
        tags.push(format!("page:{page_key}"));
        if let Some(base) = base_page_key(&page_key) {
            tags.push(format!("page:{base}"));
            tags.push(base);
        }
        tags.push(page_key.clone());
    }
    tags.retain(|t| !t.is_empty());

    // Текст вопроса идёт полноценным запросом по содержимому статей, а не
    // крошится в псевдо-теги: слово из вопроса почти никогда не является тегом.
    let kb = super::knowledge_base::kb_read();
    let tag_refs: Vec<&str> = tags.iter().map(String::as_str).collect();
    let results = kb.search(&super::kb_search::Query {
        text: query.clone(),
        tags: kb.normalize_tags(&tag_refs),
        limit: super::kb_search::MAX_LIMIT,
        ..Default::default()
    });

    let items: Vec<Value> = results
        .iter()
        .map(|(doc, _)| {
            json!({
                "id": doc.id,
                "title": doc.title,
                "summary": doc.summary,
                "tags": doc.canonical_tags,
            })
        })
        .collect();

    json!({
        "ok": true,
        "searched_tags": tags,
        "results": items,
        "total": items.len(),
        "hint": if items.is_empty() {
            "По этому разделу пользовательской документации пока нет. Отвечай по контексту \
             страницы и общим знаниям о системе, честно предупредив, что инструкции нет."
        } else {
            "Полный текст статьи — get_knowledge(id)."
        }
    })
}

/// Базовый ключ страницы: `a015_wb_orders_details_<uuid>` → `a015_wb_orders`.
fn base_page_key(page_key: &str) -> Option<String> {
    for marker in ["_details_id_", "_details_"] {
        if let Some(pos) = page_key.find(marker) {
            return Some(page_key[..pos].to_string());
        }
    }
    None
}

async fn get_user_recent_pages(args: &Value, caller: &ToolCaller) -> Value {
    let limit = args
        .get("limit")
        .and_then(Value::as_u64)
        .unwrap_or(10)
        .clamp(1, 30);

    match crate::system::history::repository::list_recent(&caller.user_id, limit).await {
        Ok(items) => {
            let pages: Vec<Value> = items
                .iter()
                .map(|p| {
                    json!({
                        "page_key": p.tab_key,
                        "title": p.title,
                        "opened_at": p.opened_at,
                    })
                })
                .collect();
            json!({ "ok": true, "count": pages.len(), "pages": pages })
        }
        Err(e) => json!({ "error": format!("Не удалось получить историю страниц: {e}") }),
    }
}

fn requester(caller: &ToolCaller) -> ticket_service::Requester {
    ticket_service::Requester {
        user_id: caller.user_id.clone(),
        is_admin: caller.is_admin,
    }
}

async fn ticket_search(args: &Value, caller: &ToolCaller) -> Value {
    let limit = args
        .get("limit")
        .and_then(Value::as_u64)
        .unwrap_or(10)
        .clamp(1, 50);

    let params = ticket_service::ListParams {
        status: args
            .get("status")
            .and_then(Value::as_str)
            .map(str::to_string),
        ticket_type: None,
        search: args
            .get("query")
            .and_then(Value::as_str)
            .map(str::to_string)
            .filter(|s| !s.trim().is_empty()),
        limit,
        offset: 0,
    };

    match ticket_service::list(&requester(caller), params).await {
        Ok(response) => {
            let items: Vec<Value> = response
                .items
                .iter()
                .map(|t| {
                    json!({
                        "code": t.code,
                        "title": t.title,
                        "ticket_type": t.ticket_type.label_ru(),
                        "status": t.status.label_ru(),
                        "priority": t.priority.label_ru(),
                        "created_at": t.created_at,
                        "updated_at": t.updated_at,
                    })
                })
                .collect();
            json!({
                "ok": true,
                "count": items.len(),
                "total": response.total,
                "tickets": items,
                "hint": "Поиск идёт по номеру и заголовку. Если похожий тикет уже есть — \
                         назови его номер и статус вместо создания нового."
            })
        }
        Err(e) => json!({ "error": format!("Не удалось получить список тикетов: {e}") }),
    }
}

fn ticket_validate(args: &Value) -> Value {
    let draft = TicketDraft::parse(args);
    let Checklist { missing, warnings } = check(&draft);
    let complete = missing.is_empty();

    json!({
        "ok": true,
        "complete": complete,
        "missing": missing,
        "warnings": warnings,
        "payload_hash": draft.payload_hash(),
        "preview_text": draft.preview_text(),
        "next_step": if complete {
            "Покажи preview_text пользователю целиком и спроси, создавать ли тикет. \
             Вызывай ticket_create только после его подтверждения — в следующем ходе и \
             ровно с теми же полями."
        } else {
            "Задай пользователю вопросы по пунктам missing — по одному за раз, — затем \
             вызови ticket_validate снова с дополненными полями."
        }
    })
}

async fn ticket_create(args: &Value, chat_id: &str, caller: &ToolCaller) -> Value {
    let draft = TicketDraft::parse(args);
    let hash = draft.payload_hash();

    // Гейт: та же самая версия полей должна была пройти валидацию в прошлом ходе.
    match validation_confirmed(chat_id, &hash).await {
        Ok(true) => {}
        Ok(false) => {
            let Checklist { missing, .. } = check(&draft);
            return json!({
                "error": "Создание отклонено: эти поля не проходили ticket_validate в \
                          предыдущем ходе диалога.",
                "reason": if missing.is_empty() {
                    "Поля выглядят полными, но пользователь ещё не видел превью и не \
                     подтвердил создание (либо после валидации ты изменил формулировки)."
                } else {
                    "Черновик неполон — см. missing."
                },
                "missing": missing,
                "payload_hash": hash,
                "next_step": "Вызови ticket_validate с этими полями, покажи пользователю \
                              preview_text и дождись его ответа. Создавать тикет можно \
                              только следующим ходом."
            });
        }
        Err(e) => return json!({ "error": format!("Не удалось проверить журнал валидации: {e}") }),
    }

    // Защита от повторного создания: журнал валидации остаётся валидным и после
    // создания тикета, поэтому второй вызов с теми же полями прошёл бы гейт.
    if let Ok(existing) = ticket_service::list_by_source_chat(chat_id).await {
        if let Some(dup) = existing
            .iter()
            .find(|t| normalize_ws(&t.title) == normalize_ws(&draft.title))
        {
            return json!({
                "error": "Тикет с таким заголовком уже оформлен из этого диалога.",
                "ticket_code": dup.code,
                "ticket_id": dup.id,
                "status": dup.status.label_ru(),
                "next_step": format!(
                    "Сообщи пользователю, что обращение {} уже создано. Новый тикет нужен \
                     только если это ДРУГАЯ проблема — тогда переформулируй заголовок и \
                     пройди ticket_validate заново.",
                    dup.code
                )
            });
        }
    }

    // Контекст страницы: сначала явно указанный моделью, иначе — свежий пакет чата.
    let chat_context = latest_chat_context(chat_id).await;
    let context_page_key = draft
        .page_key
        .clone()
        .or_else(|| chat_context.as_ref().map(|(key, _)| key.clone()));
    let context_json = chat_context.map(|(_, json)| json);

    let request = CreateTicketRequest {
        title: draft.title.clone(),
        description: draft.description.clone(),
        ticket_type: draft.ticket_type,
        priority: draft.priority,
        deadline: None,
        assignee_user_id: None,
        tags: draft.tags.clone(),
        context_page_key,
        context_json,
        origin: Some(TICKET_ORIGIN_LLM_CHAT.to_string()),
    };

    let ticket = match ticket_service::create_from_chat(&requester(caller), request, chat_id).await
    {
        Ok(t) => t,
        Err(e) => return json!({ "error": format!("Не удалось создать тикет: {e}") }),
    };

    let attached = copy_chat_images(chat_id, &ticket.id, caller).await;

    json!({
        "ok": true,
        "ticket_code": ticket.code,
        "ticket_id": ticket.id,
        "status": ticket.status.label_ru(),
        "attachments_copied": attached,
        "next_step": format!(
            "Сообщи пользователю номер {} и что обращение принято в работу.",
            ticket.code
        )
    })
}

/// Есть ли в журнале успешная валидация ровно этих полей.
async fn validation_confirmed(chat_id: &str, payload_hash: &str) -> anyhow::Result<bool> {
    let db = crate::shared::data::db::get_connection();
    let entries = crate::domain::a018_llm_chat::repository::find_tool_trace_by_tool(
        &db,
        chat_id,
        "ticket_validate",
    )
    .await?;

    Ok(entries.iter().any(|entry| {
        let Some(output) = entry.output.as_ref() else {
            return false;
        };
        output.get("complete").and_then(Value::as_bool) == Some(true)
            && output.get("payload_hash").and_then(Value::as_str) == Some(payload_hash)
    }))
}

/// Свежайший пакет контекста страницы, привязанный к чату: (page_key, context_json).
async fn latest_chat_context(chat_id: &str) -> Option<(String, String)> {
    let db = crate::shared::data::db::get_connection();
    let packages = crate::domain::a018_llm_chat::repository::list_context_by_chat(&db, chat_id)
        .await
        .ok()?;
    let latest = packages.last()?;
    Some((latest.page_key.clone(), latest.context_json.clone()))
}

/// Перенести изображения чата во вложения тикета.
///
/// Именно копия объекта в S3, а не общая ссылка на тот же `s3_file_id`: удаление
/// вложения тикета стирает объект из хранилища и иначе унесло бы картинку из чата.
async fn copy_chat_images(chat_id: &str, ticket_id: &str, caller: &ToolCaller) -> usize {
    use contracts::domain::a018_llm_chat::aggregate::LlmChatId;
    use contracts::domain::common::AggregateId;

    let Ok(chat_uuid) = LlmChatId::from_string(chat_id) else {
        return 0;
    };
    let db = crate::shared::data::db::get_connection();
    let attachments = match crate::domain::a018_llm_chat::repository::find_attachments_by_chat_id(
        &db, &chat_uuid,
    )
    .await
    {
        Ok(items) => items,
        Err(e) => {
            tracing::warn!("ticket_create: не удалось получить вложения чата {chat_id}: {e}");
            return 0;
        }
    };

    let mut copied = 0usize;
    for attachment in attachments {
        let Some(s3_file_id) = attachment.s3_file_id.as_deref() else {
            continue; // текстовые вложения лежат на диске, в тикет их не тянем
        };
        if !attachment.content_type.starts_with("image/") {
            continue;
        }
        let downloaded = match crate::system::s3::service::download(s3_file_id).await {
            Ok(Some(file)) => file,
            Ok(None) => continue,
            Err(e) => {
                tracing::warn!("ticket_create: не скачать вложение {s3_file_id}: {e}");
                continue;
            }
        };
        let upload = crate::system::s3::service::UploadedFile {
            filename: attachment.filename.clone(),
            content_type: downloaded
                .content_type
                .clone()
                .or_else(|| Some(attachment.content_type.clone())),
            bytes: downloaded.bytes,
        };
        match crate::system::tickets::service::upload_attachment(
            &requester(caller),
            ticket_id,
            None,
            upload,
        )
        .await
        {
            Ok(_) => copied += 1,
            Err(e) => tracing::warn!("ticket_create: не приложить файл к тикету: {e}"),
        }
    }
    copied
}

#[cfg(test)]
mod tests {
    use super::*;

    fn full_bug_args() -> Value {
        json!({
            "ticket_type": "bug",
            "title": "В отчёте продаж не применяется фильтр кабинета",
            "description": "## Что происходит\nФильтр игнорируется.\n\
                            ## Как воспроизвести\n1. Открыть отчёт\n2. Выбрать кабинет\n\
                            ## Ожидаемое поведение\nСтроки только выбранного кабинета.",
            "priority": "high",
            "page_key": "p904_sales_data",
            "tags": ["отчёты", "фильтры"]
        })
    }

    #[test]
    fn full_bug_draft_is_complete() {
        let draft = TicketDraft::parse(&full_bug_args());
        assert!(check(&draft).missing.is_empty());
    }

    #[test]
    fn bug_without_sections_and_page_is_incomplete() {
        let draft = TicketDraft::parse(&json!({
            "ticket_type": "bug",
            "title": "В отчёте продаж не применяется фильтр кабинета",
            "description": "Всё сломалось",
        }));
        let missing = check(&draft).missing;
        // три раздела описания + страница
        assert_eq!(missing.len(), 4, "missing = {missing:?}");
    }

    #[test]
    fn generic_title_is_rejected() {
        let draft = TicketDraft::parse(&json!({
            "ticket_type": "idea",
            "title": "Не работает",
            "description": "## Как сейчас\nа\n## Как хочется\nб\n## Зачем\nв",
        }));
        assert!(check(&draft)
            .missing
            .iter()
            .any(|m| m.contains("Заголовок")));
    }

    #[test]
    fn improvement_requires_its_own_sections() {
        let draft = TicketDraft::parse(&json!({
            "ticket_type": "improvement",
            "title": "Добавить экспорт таблицы продаж в Excel",
            "description": "## Как сейчас\nвыгружаем руками\n## Как хочется\nкнопка\n## Зачем\nэкономия часа в неделю",
        }));
        assert!(check(&draft).missing.is_empty());
    }

    #[test]
    fn hash_ignores_key_order_case_and_whitespace() {
        let a = TicketDraft::parse(&full_bug_args());
        let b = TicketDraft::parse(&json!({
            "tags": ["фильтры", "отчёты"],
            "page_key": "p904_sales_data",
            "priority": "high",
            "title": "  В отчёте продаж НЕ применяется фильтр кабинета  ",
            "description": "## Что происходит\nФильтр игнорируется.\
                            \n                            ## Как воспроизвести\n1. Открыть отчёт\n2. Выбрать кабинет\
                            \n                            ## Ожидаемое поведение\nСтроки только выбранного кабинета.",
            "ticket_type": "bug"
        }));
        assert_eq!(a.payload_hash(), b.payload_hash());
    }

    #[test]
    fn hash_changes_when_wording_changes() {
        let a = TicketDraft::parse(&full_bug_args());
        let mut changed = full_bug_args();
        changed["title"] = json!("В отчёте продаж не применяется фильтр склада");
        let b = TicketDraft::parse(&changed);
        assert_ne!(a.payload_hash(), b.payload_hash());
    }

    #[test]
    fn base_page_key_strips_detail_suffix() {
        assert_eq!(
            base_page_key("a015_wb_orders_details_2f0c9d1e"),
            Some("a015_wb_orders".to_string())
        );
        assert_eq!(base_page_key("p904_sales_data"), None);
    }
}
