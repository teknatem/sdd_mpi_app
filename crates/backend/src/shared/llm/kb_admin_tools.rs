use super::knowledge_base::{knowledge_base_dir, reload_knowledge_base, write_kb_document};
use super::types::ToolDefinition;
use contracts::domain::a031_kb_edit::aggregate::{KbEditStatus, KbEditType};
use contracts::domain::common::AggregateId;
use serde_json::json;

pub fn kb_admin_tool_definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "list_kb_documents".into(),
            description: "Получить список markdown-документов базы знаний: id, title, tags, related, source_path, source_kind. \
                          source_kind=business_obsidian означает бизнес-базу организации в Obsidian; \
                          source_kind=app_embedded означает встроенную техническую документацию приложения."
                .into(),
            parameters: json!({ "type": "object", "properties": {} }),
        },
        ToolDefinition {
            name: "get_kb_document".into(),
            description: "Получить документ базы знаний по id или path. Embedded-документы можно читать как \
                          технический контекст приложения, но нельзя переписывать через write_kb_document."
                .into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "id": { "type": "string", "description": "ID документа из list_kb_documents (опционально)." },
                    "path": { "type": "string", "description": "Путь source_path или относительный путь в KB (опционально)." }
                }
            }),
        },
        ToolDefinition {
            name: "create_kb_edit".into(),
            description: "Создать тикет a031_kb_edit с общей концепцией, вопросами к пользователю \
                          или предлагаемым списком бизнес-статей Obsidian. Не создавай тикеты на \
                          публикацию технической документации приложения в Obsidian."
                .into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "title": { "type": "string" },
                    "edit_type": { "type": "string", "enum": ["gap", "proposal", "contradiction", "question", "all_good"] },
                    "agent_summary": { "type": "string" },
                    "questions": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Для edit_type=question: конкретные вопросы пользователю, 3-7 пунктов."
                    },
                    "target_articles": { "type": "array", "items": { "type": "string" } },
                    "source_chat_ids": { "type": "array", "items": { "type": "string" } },
                    "analyze_task_run_id": { "type": "string" }
                },
                "required": ["title", "agent_summary"]
            }),
        },
        ToolDefinition {
            name: "update_kb_edit_articles".into(),
            description: "Обновить согласуемый список бизнес-статей target_articles у тикета a031_kb_edit. \
                          target_articles должны относиться к Obsidian-базе организации, а не к embedded \
                          технической документации приложения."
                .into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "id": { "type": "string" },
                    "target_articles": { "type": "array", "items": { "type": "string" } }
                },
                "required": ["id", "target_articles"]
            }),
        },
        ToolDefinition {
            name: "list_open_kb_edits".into(),
            description: "Получить незакрытые тикеты KB или тикеты указанного статуса.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "status": { "type": "string", "enum": ["pending", "in_dialog", "approved", "processing"] }
                }
            }),
        },
        ToolDefinition {
            name: "write_kb_document".into(),
            description: "Записать полный markdown-документ в Obsidian-каталог бизнес-базы организации. \
                          path должен быть относительным и оканчиваться на .md. Запрещено писать сюда \
                          технические детали приложения, SQL-схемы, DataView/API инструкции и embedded docs."
                .into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "content": { "type": "string" }
                },
                "required": ["path", "content"]
            }),
        },
    ]
}

pub async fn execute_kb_admin_tool(
    name: &str,
    arguments: &str,
    agent_id: &str,
) -> serde_json::Value {
    match name {
        "list_kb_documents" => list_kb_documents(),
        "get_kb_document" => get_kb_document(arguments),
        "create_kb_edit" => create_kb_edit(arguments, agent_id).await,
        "update_kb_edit_articles" => update_kb_edit_articles(arguments).await,
        "list_open_kb_edits" => list_open_kb_edits(arguments).await,
        "write_kb_document" => write_document(arguments),
        _ => json!({ "error": format!("Unknown KB admin tool: {}", name) }),
    }
}

fn list_kb_documents() -> serde_json::Value {
    let kb = super::knowledge_base::kb_read();
    let mut docs = kb
        .all_docs()
        .into_iter()
        .map(|doc| {
            // Техдокументация лежит копией внутри каталога знаний, поэтому по
            // пути её от бизнес-статьи не отличить — класс берём из `kind`.
            let source_kind = if doc.is_embedded() {
                "app_embedded"
            } else {
                "business_obsidian"
            };
            json!({
                "id": doc.id,
                "title": doc.title,
                "tags": doc.tags,
                "related": doc.related,
                "source_path": doc.source_path,
                "source_kind": source_kind,
            })
        })
        .collect::<Vec<_>>();
    docs.sort_by(|a, b| {
        a.get("id")
            .and_then(|v| v.as_str())
            .cmp(&b.get("id").and_then(|v| v.as_str()))
    });
    let total = docs.len();
    json!({ "documents": docs, "total": total })
}

fn get_kb_document(arguments: &str) -> serde_json::Value {
    let args = serde_json::from_str::<serde_json::Value>(arguments).unwrap_or_default();
    let id = args.get("id").and_then(|v| v.as_str()).unwrap_or_default();
    if !id.is_empty() {
        let kb = super::knowledge_base::kb_read();
        return match kb.get(id) {
            Some(doc) => json!({
                "id": doc.id,
                "title": doc.title,
                "tags": doc.tags,
                "related": doc.related,
                "source_path": doc.source_path,
                "content": doc.content,
            }),
            None => json!({ "error": format!("Document not found: {}", id) }),
        };
    }

    let Some(path) = args.get("path").and_then(|v| v.as_str()) else {
        return json!({ "error": "Either id or path is required." });
    };
    let base = knowledge_base_dir();
    let path = std::path::Path::new(path);
    let full_path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        base.join(path)
    };
    match std::fs::read_to_string(&full_path) {
        Ok(content) => json!({ "path": full_path.display().to_string(), "content": content }),
        Err(e) => json!({ "error": format!("Cannot read '{}': {}", full_path.display(), e) }),
    }
}

async fn create_kb_edit(arguments: &str, agent_id: &str) -> serde_json::Value {
    let args = match serde_json::from_str::<serde_json::Value>(arguments) {
        Ok(v) => v,
        Err(e) => return json!({ "error": format!("Invalid JSON arguments: {}", e) }),
    };
    let title = args
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap_or("Редактирование базы знаний")
        .to_string();
    let agent_summary = args
        .get("agent_summary")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    if agent_summary.trim().is_empty() {
        return json!({ "error": "agent_summary is required" });
    }
    let edit_type = args
        .get("edit_type")
        .and_then(|v| v.as_str())
        .map(KbEditType::from_str)
        .unwrap_or(KbEditType::Proposal);
    let questions = string_array_arg(&args, "questions");
    let agent_summary = if matches!(edit_type, KbEditType::Question) && !questions.is_empty() {
        format!(
            "{}\n\n## Вопросы к пользователю\n{}",
            agent_summary.trim(),
            questions
                .iter()
                .enumerate()
                .map(|(idx, question)| format!("{}. {}", idx + 1, question))
                .collect::<Vec<_>>()
                .join("\n")
        )
    } else {
        agent_summary
    };
    let target_articles = string_array_arg(&args, "target_articles");
    let source_chat_ids = string_array_arg(&args, "source_chat_ids");
    let analyze_task_run_id = args
        .get("analyze_task_run_id")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let agent_uuid = match uuid::Uuid::parse_str(agent_id) {
        Ok(id) => id,
        Err(e) => return json!({ "error": format!("Invalid agent_id: {}", e) }),
    };

    match crate::domain::a031_kb_edit::service::create_with_chat(
        title,
        agent_summary,
        edit_type,
        target_articles,
        source_chat_ids,
        contracts::domain::a017_llm_agent::aggregate::LlmAgentId::new(agent_uuid),
        analyze_task_run_id,
    )
    .await
    {
        Ok(id) => json!({ "success": true, "id": id.to_string() }),
        Err(e) => json!({ "error": e.to_string() }),
    }
}

async fn update_kb_edit_articles(arguments: &str) -> serde_json::Value {
    let args = serde_json::from_str::<serde_json::Value>(arguments).unwrap_or_default();
    let Some(id) = args.get("id").and_then(|v| v.as_str()) else {
        return json!({ "error": "id is required" });
    };
    let articles = string_array_arg(&args, "target_articles");
    match crate::domain::a031_kb_edit::service::update_target_articles(id, articles).await {
        Ok(()) => json!({ "success": true }),
        Err(e) => json!({ "error": e.to_string() }),
    }
}

async fn list_open_kb_edits(arguments: &str) -> serde_json::Value {
    let args = serde_json::from_str::<serde_json::Value>(arguments).unwrap_or_default();
    let status = args
        .get("status")
        .and_then(|v| v.as_str())
        .map(KbEditStatus::from_str)
        .unwrap_or(KbEditStatus::Approved);
    match crate::domain::a031_kb_edit::service::list_by_status(status).await {
        Ok(items) => json!({
            "items": items.iter().map(|item| json!({
                "id": item.base.id.as_string(),
                "title": item.title,
                "status": item.status.as_str(),
                "edit_type": item.edit_type.as_str(),
                "agent_summary": item.agent_summary,
                "target_articles": item.target_articles,
            })).collect::<Vec<_>>(),
            "total": items.len()
        }),
        Err(e) => json!({ "error": e.to_string() }),
    }
}

fn write_document(arguments: &str) -> serde_json::Value {
    let args = serde_json::from_str::<serde_json::Value>(arguments).unwrap_or_default();
    let Some(path) = args.get("path").and_then(|v| v.as_str()) else {
        return json!({ "error": "path is required" });
    };
    let Some(content) = args.get("content").and_then(|v| v.as_str()) else {
        return json!({ "error": "content is required" });
    };
    match write_kb_document(path, content).and_then(|written| {
        reload_knowledge_base()?;
        Ok(written)
    }) {
        Ok(written) => json!({ "success": true, "path": written.display().to_string() }),
        Err(e) => json!({ "error": e.to_string() }),
    }
}

fn string_array_arg(args: &serde_json::Value, key: &str) -> Vec<String> {
    args.get(key)
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default()
}
