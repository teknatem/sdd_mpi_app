//! LLM-инструменты рабочего каталога чата.
//!
//! Механика каталога — в [`super::chat_workspace`]; здесь только определения
//! инструментов для модели и диспетчер `execute_workspace_tool`.
//!
//! Инструменты работают относительно **активной активности**: модель не таскает
//! её имя в каждом вызове. Читать чужую задачу можно без переключения —
//! `read_chat_file("001-сверка/plan.md")`.

use super::chat_workspace::{Activity, ChatWorkspace, FileEntry, StepKind, LIVE_DOCUMENTS};
use super::types::ToolDefinition;
use contracts::domain::a018_llm_chat::aggregate::LlmChatId;
use serde_json::{json, Value};

/// Имена инструментов каталога (для маршрутизации в `execute_tool_call`).
pub const WORKSPACE_TOOL_NAMES: &[&str] = &[
    "list_chat_files",
    "read_chat_file",
    "write_chat_file",
    "update_plan_step",
    "save_step",
    "start_activity",
    "switch_activity",
];

/// Потолок на одно чтение: окно, а не весь файл — иначе смысл выноса на диск теряется.
const DEFAULT_READ_CHARS: usize = 8_000;
const MAX_READ_CHARS: usize = 40_000;

pub fn workspace_tool_definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "list_chat_files".into(),
            description: "Показать рабочий каталог чата: список задач (активностей), файлы \
                          активной задачи и вложения пользователя. Вызывай, когда нужно понять, \
                          что уже сделано, или найти имя ранее сохранённого шага."
                .into(),
            parameters: json!({ "type": "object", "properties": {} }),
        },
        ToolDefinition {
            name: "read_chat_file".into(),
            description: "Прочитать файл рабочего каталога или вложение пользователя. \
                          Имена внутри активной задачи: 'intake.md', 'plan.md', 'notes.md', \
                          'steps/003-calc-….json'. Файл другой задачи — с префиксом её каталога: \
                          '001-сверка-выручки/plan.md'. Вложение — по имени файла. \
                          Чтение оконное: если вернулось truncated: true, дочитай со смещением offset."
                .into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "description": "Имя файла из list_chat_files."
                    },
                    "offset": {
                        "type": "integer",
                        "description": "Смещение в символах (по умолчанию 0).",
                        "minimum": 0
                    },
                    "max_chars": {
                        "type": "integer",
                        "description": "Сколько символов вернуть (по умолчанию 8000, максимум 40000).",
                        "minimum": 1,
                        "maximum": MAX_READ_CHARS
                    }
                },
                "required": ["name"]
            }),
        },
        ToolDefinition {
            name: "write_chat_file".into(),
            description: "Перезаписать живой документ активной задачи целиком. Допустимы только \
                          'intake.md' (анкета: параметры задачи), 'plan.md' (план: шаги и статусы) \
                          и 'notes.md' (выводы, которые должны пережить сжатие истории). \
                          Эти три файла подставляются в контекст автоматически на каждом шаге — \
                          держи их в актуальном состоянии. Результаты работы сюда не пиши: \
                          для них есть save_step."
                .into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "description": "intake.md | plan.md | notes.md",
                        "enum": LIVE_DOCUMENTS
                    },
                    "content": {
                        "type": "string",
                        "description": "Полное новое содержимое файла (markdown с frontmatter)."
                    }
                },
                "required": ["name", "content"]
            }),
        },
        ToolDefinition {
            name: "update_plan_step".into(),
            description: "Отметить пункт плана выполненным (или снять отметку). Правит одну \
                          строку plan.md, остальной файл не трогает — используй вместо \
                          перезаписи всего плана через write_chat_file. \
                          Идентификаторы пунктов ('s1', 's2', …) видны в блоке активной задачи \
                          и возвращаются этим инструментом. \
                          Если пункт дал сохранённый результат — упомяни имя файла шага прямо \
                          в тексте пункта: система сверяет закрытые пункты с журналом steps/, \
                          и ссылка на несуществующий шаг попадёт в отчёт о качестве."
                .into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "step_id": {
                        "type": "string",
                        "description": "Идентификатор пункта плана, например 's2'."
                    },
                    "done": {
                        "type": "boolean",
                        "description": "true — отметить выполненным, false — снять отметку. По умолчанию true."
                    }
                },
                "required": ["step_id"]
            }),
        },
        ToolDefinition {
            name: "save_step".into(),
            description: "Сохранить результат шага в журнал активной задачи. Имя файла (с номером) \
                          присваивает система и возвращает его — ссылайся на это имя в ответе и в \
                          plan.md. Существенную выборку НЕ пересказывай в тексте: сохрани сюда, \
                          иначе она потеряется при сжатии истории и её придётся запрашивать заново."
                .into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "type": {
                        "type": "string",
                        "description": "query — выборка данных; calc — производный расчёт; \
                                        draft — черновик артефакта; report — итог для пользователя.",
                        "enum": StepKind::ALL
                    },
                    "description": {
                        "type": "string",
                        "description": "Короткое описание шага — попадёт в имя файла, по нему шаг потом ищут."
                    },
                    "content": {
                        "type": "string",
                        "description": "Содержимое: JSON выборки, расчёт или текст отчёта."
                    },
                    "ext": {
                        "type": "string",
                        "description": "Расширение без точки: json (по умолчанию), md, csv."
                    }
                },
                "required": ["type", "description", "content"]
            }),
        },
        ToolDefinition {
            name: "start_activity".into(),
            description: "Завести в этом чате новую задачу и сделать её активной. Вызывай, когда \
                          пользователь переключился на другую тему: у новой задачи будут своя \
                          анкета, свой план и свой журнал шагов, а прежняя останется нетронутой."
                .into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "description": {
                        "type": "string",
                        "description": "Суть задачи в нескольких словах — попадёт в имя каталога."
                    }
                },
                "required": ["description"]
            }),
        },
        ToolDefinition {
            name: "switch_activity".into(),
            description: "Вернуться к ранее заведённой задаче этого чата: её анкета и план снова \
                          станут активными. Имя задачи — из list_chat_files."
                .into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "description": "Имя каталога задачи, например '001-сверка-выручки-q2'."
                    }
                },
                "required": ["name"]
            }),
        },
    ]
}

fn err(message: impl Into<String>) -> Value {
    json!({ "error": message.into(), "_ok": false })
}

fn chat_key(chat_id: &str) -> Option<LlmChatId> {
    uuid::Uuid::parse_str(chat_id).ok().map(LlmChatId::new)
}

/// Имена вложений пользователя в этом чате.
async fn attachment_names(chat_id: &str) -> Vec<String> {
    let Some(id) = chat_key(chat_id) else {
        return Vec::new();
    };
    let db = crate::shared::data::db::get_connection();
    crate::domain::a018_llm_chat::repository::find_attachments_by_chat_id(db, &id)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|a| a.filename)
        .collect()
}

/// Описание для ленивого создания первой активности — берём название чата,
/// чтобы каталог назывался осмысленно, а не «001-задача».
async fn fallback_description(chat_id: &str) -> String {
    let Some(id) = chat_key(chat_id) else {
        return "задача".to_string();
    };
    let db = crate::shared::data::db::get_connection();
    match crate::domain::a018_llm_chat::repository::find_by_id(db, &id).await {
        Ok(Some(chat)) if !chat.base.description.trim().is_empty() => chat.base.description,
        _ => "задача".to_string(),
    }
}

/// Найти вложение чата по имени файла.
async fn read_attachment(chat_id: &str, name: &str) -> Option<Result<String, String>> {
    let id = chat_key(chat_id)?;
    let db = crate::shared::data::db::get_connection();
    let attachments =
        crate::domain::a018_llm_chat::repository::find_attachments_by_chat_id(db, &id)
            .await
            .ok()?;
    let attachment = attachments.into_iter().find(|a| a.filename == name)?;
    let bytes =
        match crate::domain::a018_llm_chat::service::load_attachment_bytes(&attachment).await {
            Ok(bytes) => bytes,
            Err(e) => return Some(Err(format!("Не удалось прочитать вложение: {e}"))),
        };
    Some(
        String::from_utf8(bytes.to_vec())
            .map_err(|_| format!("Вложение '{name}' не является текстом в UTF-8")),
    )
}

/// Разрешить имя в (активность, путь внутри неё).
///
/// Первый сегмент, совпавший с именем существующей задачи, означает обращение к
/// чужой задаче — читаем её без переключения активной.
async fn resolve_target(
    ws: &ChatWorkspace,
    active: Activity,
    name: &str,
) -> Result<(Activity, String), Value> {
    if let Some((head, rest)) = name.split_once('/') {
        let activities = ws
            .list_activities()
            .await
            .map_err(|e| err(format!("Не удалось прочитать каталог: {e}")))?;
        if activities.iter().any(|a| a.name == head) {
            return Ok((ws.activity(head), rest.to_string()));
        }
    }
    Ok((active, name.to_string()))
}

fn files_json(files: &[FileEntry]) -> Value {
    json!(files)
}

/// Положить шаблон анкеты в свежую задачу, если навык его принёс и анкеты ещё нет.
async fn seed_intake(activity: &Activity, intake_template: Option<&str>) {
    let Some(template) = intake_template else {
        return;
    };
    match activity
        .seed_if_absent(super::chat_workspace::INTAKE_FILE, template)
        .await
    {
        Ok(true) => tracing::info!("[workspace] шаблон анкеты положен в '{}'", activity.name()),
        Ok(false) => {}
        Err(e) => tracing::warn!("[workspace] не удалось положить шаблон анкеты: {e}"),
    }
}

/// Выполнить tool call рабочего каталога.
///
/// `intake_template` — шаблон анкеты от активных навыков (см.
/// `skills::intake_template_in`); кладётся в задачу при её создании.
pub async fn execute_workspace_tool(
    name: &str,
    arguments_json: &str,
    chat_id: &str,
    intake_template: Option<&str>,
) -> Value {
    let args: Value = serde_json::from_str(arguments_json).unwrap_or_else(|_| json!({}));

    let Some(ws) = ChatWorkspace::for_chat(chat_id) else {
        return err("Рабочий каталог недоступен: не удалось определить путь из конфигурации.");
    };

    match name {
        "start_activity" => {
            let description = args
                .get("description")
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim();
            if description.is_empty() {
                return err("Укажи description — суть новой задачи.");
            }
            match ws.start_activity(description).await {
                Ok(activity) => {
                    seed_intake(&activity, intake_template).await;
                    json!({
                        "activity": activity.name(),
                        "hint": "Задача создана и стала активной. Заполни intake.md, затем plan.md.",
                        "_ok": true,
                    })
                }
                Err(e) => err(format!("Не удалось завести задачу: {e}")),
            }
        }

        "switch_activity" => {
            let target = args
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim();
            let activities = match ws.list_activities().await {
                Ok(list) => list,
                Err(e) => return err(format!("Не удалось прочитать каталог: {e}")),
            };
            if !activities.iter().any(|a| a.name == target) {
                return json!({
                    "error": format!("Задачи '{}' в этом чате нет.", target),
                    "available": activities.iter().map(|a| &a.name).collect::<Vec<_>>(),
                    "_ok": false,
                });
            }
            match ws.set_active(target).await {
                Ok(()) => json!({
                    "activity": target,
                    "hint": "Задача активна. Её анкета и план подставлены в контекст.",
                    "_ok": true,
                }),
                Err(e) => err(format!("Не удалось переключить задачу: {e}")),
            }
        }

        "update_plan_step" => {
            let step_id = args
                .get("step_id")
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim();
            if step_id.is_empty() {
                return err("Укажи step_id — идентификатор пункта плана, например 's2'.");
            }
            let done = args.get("done").and_then(Value::as_bool).unwrap_or(true);
            match super::chat_workspace::update_plan_step(chat_id, step_id, done).await {
                Ok(steps) => json!({
                    "step_id": step_id,
                    "done": done,
                    "plan_steps": steps,
                    "hint": "Статус пункта обновлён. Закрытый пункт с сохранённым результатом \
                             должен упоминать имя файла шага — оно сверяется с журналом steps/.",
                    "_ok": true,
                }),
                Err(e) => err(format!("Не удалось обновить пункт плана: {e}")),
            }
        }

        "list_chat_files" => {
            let activities = match ws.list_activities().await {
                Ok(list) => list,
                Err(e) => return err(format!("Не удалось прочитать каталог: {e}")),
            };
            let active_files = match ws.active().await {
                Ok(Some(activity)) => activity.list().await.unwrap_or_default(),
                _ => Vec::new(),
            };
            let attachments = attachment_names(chat_id).await;
            json!({
                "activities": activities,
                "active_files": files_json(&active_files),
                "attachments": attachments,
                "hint": "Файлы активной задачи читай по имени; файл другой задачи — с префиксом \
                         её каталога ('001-…/plan.md').",
                "_ok": true,
            })
        }

        "read_chat_file" => {
            let target_name = args
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim();
            if target_name.is_empty() {
                return err("Укажи name — имя файла из list_chat_files.");
            }
            let offset = args.get("offset").and_then(Value::as_u64).unwrap_or(0) as usize;
            let max_chars = args
                .get("max_chars")
                .and_then(Value::as_u64)
                .map(|v| v as usize)
                .unwrap_or(DEFAULT_READ_CHARS)
                .clamp(1, MAX_READ_CHARS);

            let active = match ws.active().await {
                Ok(Some(activity)) => Some(activity),
                Ok(None) => None,
                Err(e) => return err(format!("Не удалось прочитать каталог: {e}")),
            };

            if let Some(active) = active {
                let (activity, rel) = match resolve_target(&ws, active, target_name).await {
                    Ok(pair) => pair,
                    Err(e) => return e,
                };
                match activity.read(&rel, offset, max_chars).await {
                    Ok((content, truncated)) => {
                        return json!({
                            "activity": activity.name(),
                            "name": rel,
                            "content": content,
                            "offset": offset,
                            "truncated": truncated,
                            "hint": truncated.then_some(
                                "Файл прочитан не полностью — повтори вызов со смещением offset."
                            ),
                            "_ok": true,
                        })
                    }
                    // Не файл каталога — возможно, это вложение пользователя.
                    Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                        return err(format!("Не удалось прочитать файл: {e}"))
                    }
                    Err(_) => {}
                }
            }

            match read_attachment(chat_id, target_name).await {
                Some(Ok(content)) => {
                    let window: String = content.chars().skip(offset).take(max_chars).collect();
                    let truncated = content.chars().count() > offset + window.chars().count();
                    json!({
                        "name": target_name,
                        "source": "attachment",
                        "content": window,
                        "offset": offset,
                        "truncated": truncated,
                        "_ok": true,
                    })
                }
                Some(Err(message)) => err(message),
                None => err(format!(
                    "Файла '{}' нет ни в рабочем каталоге, ни во вложениях. Проверь имя через list_chat_files.",
                    target_name
                )),
            }
        }

        "write_chat_file" => {
            let file = args
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim();
            let content = args.get("content").and_then(Value::as_str).unwrap_or("");
            if file.is_empty() {
                return err(format!(
                    "Укажи name — один из: {}.",
                    LIVE_DOCUMENTS.join(", ")
                ));
            }
            let activity = match ws.ensure_active(&fallback_description(chat_id).await).await {
                Ok(activity) => activity,
                Err(e) => return err(format!("Не удалось открыть задачу: {e}")),
            };
            // Шаблон кладём до записи: если модель пишет как раз intake.md,
            // её содержимое должно победить шаблон, а не наоборот.
            seed_intake(&activity, intake_template).await;
            match activity.write(file, content).await {
                Ok(()) => json!({
                    "activity": activity.name(),
                    "name": file,
                    "bytes": content.len(),
                    "_ok": true,
                }),
                Err(e) => err(e.to_string()),
            }
        }

        "save_step" => {
            let Some(kind) = args
                .get("type")
                .and_then(Value::as_str)
                .and_then(StepKind::parse)
            else {
                return err(format!(
                    "Укажи type — один из: {}.",
                    StepKind::ALL.join(", ")
                ));
            };
            let description = args
                .get("description")
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim();
            if description.is_empty() {
                return err("Укажи description — по нему шаг потом ищут в журнале.");
            }
            let content = args.get("content").and_then(Value::as_str).unwrap_or("");
            let ext = args.get("ext").and_then(Value::as_str).unwrap_or("json");

            let activity = match ws.ensure_active(&fallback_description(chat_id).await).await {
                Ok(activity) => activity,
                Err(e) => return err(format!("Не удалось открыть задачу: {e}")),
            };
            seed_intake(&activity, intake_template).await;
            match activity.save_step(kind, description, content, ext).await {
                Ok(file) => json!({
                    "activity": activity.name(),
                    "file": file,
                    "bytes": content.len(),
                    "hint": "Ссылайся на это имя в ответе и в plan.md — перечитать шаг можно через read_chat_file.",
                    "_ok": true,
                }),
                Err(e) => err(format!("Не удалось сохранить шаг: {e}")),
            }
        }

        other => err(format!("Неизвестный инструмент рабочего каталога: {other}")),
    }
}
