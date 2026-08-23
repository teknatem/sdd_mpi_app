use chrono::Utc;
use contracts::domain::a017_llm_agent::aggregate::LlmAgentId;
use contracts::domain::a018_llm_chat::aggregate::{
    ArtifactAction, ChatRole, LlmChat, LlmChatAttachment, LlmChatId, LlmChatListItem,
    LlmChatMessage, ToolTraceEntry,
};
use contracts::domain::a019_llm_artifact::aggregate::LlmArtifactId;
use contracts::domain::common::{AggregateId, BaseAggregate, EntityMetadata};
use sea_orm::entity::prelude::*;
use sea_orm::prelude::Expr;
use sea_orm::{
    ColumnTrait, EntityTrait, FromQueryResult, PaginatorTrait, QueryFilter, QueryOrder, Set,
};
use uuid::Uuid;

/// Парсинг id из БД. Битый id — это повреждение данных: логируем громко и
/// возвращаем детерминированный nil-UUID (виден в UI как 0000…), а не случайный
/// правдоподобный id, который маскирует проблему.
fn parse_db_uuid(raw: &str, field: &str) -> Uuid {
    Uuid::parse_str(raw).unwrap_or_else(|e| {
        tracing::error!("a018: malformed UUID in DB field {field}: '{raw}': {e}");
        Uuid::nil()
    })
}

mod chat {
    use sea_orm::entity::prelude::*;
    use serde::{Deserialize, Serialize};

    #[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
    #[sea_orm(table_name = "a018_llm_chat")]
    pub struct Model {
        #[sea_orm(primary_key, auto_increment = false)]
        pub id: String,
        pub code: String,
        pub description: String,
        pub comment: Option<String>,
        pub agent_id: String,
        pub model_name: String,
        pub is_deleted: bool,
        pub is_posted: bool,
        pub created_at: Option<chrono::DateTime<chrono::Utc>>,
        pub updated_at: Option<chrono::DateTime<chrono::Utc>>,
        pub version: i32,
        pub rating: Option<i32>,
        pub owner_user_id: Option<String>,
        pub is_shared: bool,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}

    impl ActiveModelBehavior for ActiveModel {}
}

mod message {
    use sea_orm::entity::prelude::*;
    use serde::{Deserialize, Serialize};

    #[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
    #[sea_orm(table_name = "a018_llm_chat_message")]
    pub struct Model {
        #[sea_orm(primary_key, auto_increment = false)]
        pub id: String,
        pub chat_id: String,
        pub role: String,
        pub content: String,
        pub tokens_used: Option<i32>,
        pub prompt_tokens: Option<i32>,
        pub completion_tokens: Option<i32>,
        pub cached_prompt_tokens: Option<i32>,
        pub cost_micro: Option<i64>,
        pub currency: Option<String>,
        pub model_name: Option<String>,
        pub confidence: Option<f64>,
        pub duration_ms: Option<i64>,
        pub created_at: String,
        pub artifact_id: Option<String>,
        pub artifact_action: Option<String>,
        pub tool_trace_json: Option<String>,
        pub skill_trace_json: Option<String>,
        pub intent: Option<String>,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}

    impl ActiveModelBehavior for ActiveModel {}
}

mod attachment {
    use sea_orm::entity::prelude::*;
    use serde::{Deserialize, Serialize};

    #[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
    #[sea_orm(table_name = "a018_llm_chat_attachment")]
    pub struct Model {
        #[sea_orm(primary_key, auto_increment = false)]
        pub id: String,
        pub chat_id: Option<String>,
        pub message_id: Option<String>,
        pub filename: String,
        pub filepath: String,
        pub s3_file_id: Option<String>,
        pub content_type: String,
        pub file_size: i64,
        pub created_at: String,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}

    impl ActiveModelBehavior for ActiveModel {}
}

pub mod context_package {
    use sea_orm::entity::prelude::*;
    use serde::{Deserialize, Serialize};

    #[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
    #[sea_orm(table_name = "a018_llm_chat_context_package")]
    pub struct Model {
        #[sea_orm(primary_key, auto_increment = false)]
        pub id: String,
        pub chat_id: Option<String>,
        pub page_key: String,
        pub page_type: String,
        pub entity_index: Option<String>,
        pub entity_id: Option<String>,
        pub title: String,
        pub context_json: String,
        pub rendered_text: String,
        pub created_at: String,
        pub use_count: i32,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}

    impl ActiveModelBehavior for ActiveModel {}
}

mod tool_trace {
    use sea_orm::entity::prelude::*;
    use serde::{Deserialize, Serialize};

    #[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
    #[sea_orm(table_name = "sys_tool_trace")]
    pub struct Model {
        #[sea_orm(primary_key, auto_increment = false)]
        pub id: String,
        pub chat_id: String,
        pub message_id: String,
        pub iteration: i64,
        pub call_index: i64,
        pub stage: String,
        pub tool: String,
        pub ok: bool,
        pub ms: i64,
        pub summary: Option<String>,
        pub input_json: Option<String>,
        pub output_json: Option<String>,
        pub created_at: String,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}

    impl ActiveModelBehavior for ActiveModel {}
}

/// Входные данные для вставки пакета контекста.
pub struct NewContextPackage {
    pub id: String,
    pub chat_id: Option<String>,
    pub page_key: String,
    pub page_type: String,
    pub entity_index: Option<String>,
    pub entity_id: Option<String>,
    pub title: String,
    pub context_json: String,
    pub rendered_text: String,
    pub created_at: String,
}

impl From<chat::Model> for LlmChat {
    fn from(m: chat::Model) -> Self {
        let metadata = EntityMetadata {
            created_at: m.created_at.unwrap_or_else(Utc::now),
            updated_at: m.updated_at.unwrap_or_else(Utc::now),
            is_deleted: m.is_deleted,
            is_posted: m.is_posted,
            version: m.version,
        };
        let uuid = parse_db_uuid(&m.id, "chat.id");
        let agent_uuid = parse_db_uuid(&m.agent_id, "chat.agent_id");

        LlmChat {
            base: BaseAggregate {
                id: LlmChatId::new(uuid),
                code: m.code,
                description: m.description,
                comment: m.comment,
                metadata,
            },
            agent_id: LlmAgentId::new(agent_uuid),
            model_name: m.model_name,
            rating: m.rating,
            owner_user_id: m.owner_user_id,
            is_shared: m.is_shared,
        }
    }
}

impl From<message::Model> for LlmChatMessage {
    fn from(m: message::Model) -> Self {
        let id = parse_db_uuid(&m.id, "message.id");
        let chat_id = parse_db_uuid(&m.chat_id, "message.chat_id");
        let role = ChatRole::from_str(&m.role).unwrap_or(ChatRole::User);
        let created_at = chrono::DateTime::parse_from_rfc3339(&m.created_at)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now());

        let artifact_id = m
            .artifact_id
            .and_then(|id_str| Uuid::parse_str(&id_str).ok().map(LlmArtifactId::new));

        let artifact_action = m
            .artifact_action
            .and_then(|action_str| ArtifactAction::from_str(&action_str).ok());

        LlmChatMessage {
            id,
            chat_id: LlmChatId::new(chat_id),
            role,
            content: m.content,
            tokens_used: m.tokens_used,
            prompt_tokens: m.prompt_tokens,
            completion_tokens: m.completion_tokens,
            cached_prompt_tokens: m.cached_prompt_tokens,
            cost_micro: m.cost_micro,
            currency: m.currency,
            model_name: m.model_name,
            confidence: m.confidence,
            duration_ms: m.duration_ms,
            created_at,
            artifact_id,
            artifact_action,
            tool_trace: m.tool_trace_json,
            skill_trace: m.skill_trace_json,
            intent: m.intent,
            attachments: Vec::new(), // Загружаются отдельно при необходимости
        }
    }
}

impl From<attachment::Model> for LlmChatAttachment {
    fn from(m: attachment::Model) -> Self {
        let id = parse_db_uuid(&m.id, "attachment.id");
        let chat_id = parse_db_uuid(
            m.chat_id.as_deref().unwrap_or_default(),
            "attachment.chat_id",
        );
        let message_id = m
            .message_id
            .as_deref()
            .map(|value| parse_db_uuid(value, "attachment.message_id"));
        let created_at = chrono::DateTime::parse_from_rfc3339(&m.created_at)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now());

        LlmChatAttachment {
            id,
            chat_id: LlmChatId::new(chat_id),
            message_id,
            filename: m.filename,
            filepath: m.filepath,
            s3_file_id: m.s3_file_id,
            content_type: m.content_type,
            file_size: m.file_size,
            created_at,
        }
    }
}

// ============================================================================
// Chat Repository Functions
// ============================================================================

/// Сводка ранней части диалога (компакция истории): (summary_text, summary_upto).
/// Колонки живут только в БД (0171) и не входят в contract-агрегат.
pub async fn get_chat_summary(
    db: &DatabaseConnection,
    chat_id: &LlmChatId,
) -> Result<Option<(String, String)>, DbErr> {
    use sea_orm::{ConnectionTrait, DatabaseBackend, Statement};
    let row = db
        .query_one(Statement::from_sql_and_values(
            DatabaseBackend::Sqlite,
            "SELECT summary_text, summary_upto FROM a018_llm_chat WHERE id = ? LIMIT 1",
            [chat_id.as_string().into()],
        ))
        .await?;
    let Some(row) = row else { return Ok(None) };
    let text: Option<String> = row.try_get("", "summary_text").unwrap_or(None);
    let upto: Option<String> = row.try_get("", "summary_upto").unwrap_or(None);
    Ok(match (text, upto) {
        (Some(t), Some(u)) if !t.is_empty() => Some((t, u)),
        _ => None,
    })
}

/// Сохранить сводку компакции истории чата.
pub async fn set_chat_summary(
    db: &DatabaseConnection,
    chat_id: &LlmChatId,
    summary_text: &str,
    summary_upto: &str,
) -> Result<(), DbErr> {
    use sea_orm::{ConnectionTrait, DatabaseBackend, Statement};
    db.execute(Statement::from_sql_and_values(
        DatabaseBackend::Sqlite,
        "UPDATE a018_llm_chat SET summary_text = ?, summary_upto = ? WHERE id = ?",
        [
            summary_text.into(),
            summary_upto.into(),
            chat_id.as_string().into(),
        ],
    ))
    .await?;
    Ok(())
}

/// Получить все чаты (не удаленные)
pub async fn list_all(db: &DatabaseConnection) -> Result<Vec<LlmChat>, DbErr> {
    let models = chat::Entity::find()
        .filter(chat::Column::IsDeleted.eq(false))
        .order_by_desc(chat::Column::CreatedAt)
        .all(db)
        .await?;

    Ok(models.into_iter().map(|m| m.into()).collect())
}

/// Получить чаты с пагинацией
pub async fn list_paginated(
    db: &DatabaseConnection,
    page: u64,
    page_size: u64,
) -> Result<(Vec<LlmChat>, u64), DbErr> {
    let query = chat::Entity::find()
        .filter(chat::Column::IsDeleted.eq(false))
        .order_by_desc(chat::Column::CreatedAt);

    let paginator = query.paginate(db, page_size);
    let total = paginator.num_items().await?;
    let models = paginator.fetch_page(page).await?;

    Ok((models.into_iter().map(|m| m.into()).collect(), total))
}

#[derive(Debug, FromQueryResult)]
struct ChatWithStats {
    id: String,
    code: String,
    description: String,
    agent_id: String,
    agent_name: Option<String>,
    agent_type: Option<String>,
    model_name: String,
    created_at: Option<chrono::DateTime<chrono::Utc>>,
    message_count: Option<i64>,
    last_message_at: Option<String>,
    rating: Option<i32>,
    owner_user_id: Option<String>,
    is_shared: bool,
}

/// Получить список чатов с подсчетом сообщений и временем последнего сообщения.
///
/// Разграничение доступа: `is_admin` (superadmin) видит все чаты; обычный пользователь —
/// только свои (`owner_user_id = viewer_id`) и помеченные общим доступом (`is_shared = 1`).
pub async fn list_with_stats(
    db: &DatabaseConnection,
    viewer_id: &str,
    is_admin: bool,
) -> Result<Vec<LlmChatListItem>, DbErr> {
    // Для админа — без фильтра по владельцу; для остального — свои + общие.
    let (access_filter, values) = if is_admin {
        (String::new(), vec![])
    } else {
        (
            " AND (c.owner_user_id = ? OR c.is_shared = 1)".to_string(),
            vec![sea_orm::Value::from(viewer_id.to_string())],
        )
    };

    let sql = format!(
        r#"
        SELECT
            c.id,
            c.code,
            c.description,
            c.agent_id,
            a.description as agent_name,
            a.agent_type as agent_type,
            c.model_name,
            c.created_at,
            c.rating,
            c.owner_user_id,
            c.is_shared,
            COUNT(m.id) as message_count,
            MAX(m.created_at) as last_message_at
        FROM a018_llm_chat c
        LEFT JOIN a017_llm_agent a ON c.agent_id = a.id
        LEFT JOIN a018_llm_chat_message m ON c.id = m.chat_id
        WHERE c.is_deleted = 0{access_filter}
        GROUP BY c.id, c.code, c.description, c.agent_id, a.description, a.agent_type, c.model_name, c.created_at, c.rating, c.owner_user_id, c.is_shared
        ORDER BY c.created_at DESC
    "#
    );

    let results = ChatWithStats::find_by_statement(sea_orm::Statement::from_sql_and_values(
        db.get_database_backend(),
        &sql,
        values,
    ))
    .all(db)
    .await?;

    Ok(results
        .into_iter()
        .map(|r| {
            let last_message_at = r.last_message_at.and_then(|s| {
                chrono::DateTime::parse_from_rfc3339(&s)
                    .ok()
                    .map(|dt| dt.with_timezone(&Utc))
            });

            LlmChatListItem {
                id: r.id,
                code: r.code,
                description: r.description,
                agent_id: r.agent_id,
                agent_name: r.agent_name,
                agent_type: r.agent_type,
                model_name: r.model_name,
                created_at: r.created_at.unwrap_or_else(Utc::now),
                message_count: r.message_count,
                last_message_at,
                rating: r.rating,
                owner_user_id: r.owner_user_id,
                is_shared: r.is_shared,
            }
        })
        .collect())
}

/// Найти чат по ID
pub async fn find_by_id(db: &DatabaseConnection, id: &LlmChatId) -> Result<Option<LlmChat>, DbErr> {
    let model = chat::Entity::find_by_id(id.as_string()).one(db).await?;
    Ok(model.map(|m| m.into()))
}

/// Вставить новый чат
pub async fn insert(db: &DatabaseConnection, chat: &LlmChat) -> Result<(), DbErr> {
    let now = Utc::now();
    let active_model = chat::ActiveModel {
        id: Set(chat.base.id.as_string()),
        code: Set(chat.base.code.clone()),
        description: Set(chat.base.description.clone()),
        comment: Set(chat.base.comment.clone()),
        agent_id: Set(chat.agent_id.as_string()),
        model_name: Set(chat.model_name.clone()),
        is_deleted: Set(false),
        is_posted: Set(false),
        created_at: Set(Some(now)),
        updated_at: Set(Some(now)),
        version: Set(1),
        rating: Set(chat.rating),
        owner_user_id: Set(chat.owner_user_id.clone()),
        is_shared: Set(chat.is_shared),
    };

    active_model.insert(db).await?;
    Ok(())
}

/// Обновить чат
pub async fn update(db: &DatabaseConnection, chat: &LlmChat) -> Result<(), DbErr> {
    let now = Utc::now();
    let active_model = chat::ActiveModel {
        id: Set(chat.base.id.as_string()),
        code: Set(chat.base.code.clone()),
        description: Set(chat.base.description.clone()),
        comment: Set(chat.base.comment.clone()),
        agent_id: Set(chat.agent_id.as_string()),
        model_name: Set(chat.model_name.clone()),
        is_deleted: Set(chat.base.metadata.is_deleted),
        is_posted: Set(chat.base.metadata.is_posted),
        created_at: Set(Some(chat.base.metadata.created_at)),
        updated_at: Set(Some(now)),
        version: Set(chat.base.metadata.version + 1),
        rating: Set(chat.rating),
        owner_user_id: Set(chat.owner_user_id.clone()),
        is_shared: Set(chat.is_shared),
    };

    chat::Entity::update(active_model).exec(db).await?;
    Ok(())
}

/// Мягкое удаление чата
pub async fn soft_delete(db: &DatabaseConnection, id: &LlmChatId) -> Result<(), DbErr> {
    let now = Utc::now();
    chat::Entity::update_many()
        .col_expr(chat::Column::IsDeleted, Expr::value(true))
        .col_expr(chat::Column::UpdatedAt, Expr::value(now))
        .filter(chat::Column::Id.eq(id.as_string()))
        .exec(db)
        .await?;
    Ok(())
}

// ============================================================================
// Message Repository Functions
// ============================================================================

/// Найти все сообщения чата
pub async fn find_messages_by_chat_id(
    db: &DatabaseConnection,
    chat_id: &LlmChatId,
) -> Result<Vec<LlmChatMessage>, DbErr> {
    let models = message::Entity::find()
        .filter(message::Column::ChatId.eq(chat_id.as_string()))
        .order_by_asc(message::Column::CreatedAt)
        .all(db)
        .await?;

    Ok(models.into_iter().map(|m| m.into()).collect())
}

/// Вставить сообщение
pub async fn insert_message(
    db: &DatabaseConnection,
    message: &LlmChatMessage,
) -> Result<(), DbErr> {
    let active_model = message::ActiveModel {
        id: Set(message.id.to_string()),
        chat_id: Set(message.chat_id.as_string()),
        role: Set(message.role.as_str().to_string()),
        content: Set(message.content.clone()),
        tokens_used: Set(message.tokens_used),
        prompt_tokens: Set(message.prompt_tokens),
        completion_tokens: Set(message.completion_tokens),
        cached_prompt_tokens: Set(message.cached_prompt_tokens),
        cost_micro: Set(message.cost_micro),
        currency: Set(message.currency.clone()),
        model_name: Set(message.model_name.clone()),
        confidence: Set(message.confidence),
        duration_ms: Set(message.duration_ms),
        created_at: Set(message.created_at.to_rfc3339()),
        artifact_id: Set(message.artifact_id.map(|id| id.as_string())),
        artifact_action: Set(message
            .artifact_action
            .as_ref()
            .map(|a| a.as_str().to_string())),
        tool_trace_json: Set(message.tool_trace.clone()),
        skill_trace_json: Set(message.skill_trace.clone()),
        intent: Set(message.intent.clone()),
    };

    active_model.insert(db).await?;
    Ok(())
}

// ============================================================================
// Tool Trace Repository Functions (sys_tool_trace)
// ============================================================================

impl From<tool_trace::Model> for ToolTraceEntry {
    fn from(m: tool_trace::Model) -> Self {
        let created_at = chrono::DateTime::parse_from_rfc3339(&m.created_at)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now());
        ToolTraceEntry {
            id: m.id,
            chat_id: m.chat_id,
            message_id: m.message_id,
            iteration: m.iteration,
            call_index: m.call_index,
            stage: m.stage,
            tool: m.tool,
            ok: m.ok,
            ms: m.ms,
            summary: m.summary,
            input: m.input_json.and_then(|s| serde_json::from_str(&s).ok()),
            output: m.output_json.and_then(|s| serde_json::from_str(&s).ok()),
            created_at,
        }
    }
}

/// Записать пачку строк журнала вызовов инструментов одним batch-insert'ом.
pub async fn insert_tool_trace_batch(
    db: &DatabaseConnection,
    entries: &[ToolTraceEntry],
) -> Result<(), DbErr> {
    if entries.is_empty() {
        return Ok(());
    }
    let models: Vec<tool_trace::ActiveModel> = entries
        .iter()
        .map(|e| tool_trace::ActiveModel {
            id: Set(e.id.clone()),
            chat_id: Set(e.chat_id.clone()),
            message_id: Set(e.message_id.clone()),
            iteration: Set(e.iteration),
            call_index: Set(e.call_index),
            stage: Set(e.stage.clone()),
            tool: Set(e.tool.clone()),
            ok: Set(e.ok),
            ms: Set(e.ms),
            summary: Set(e.summary.clone()),
            input_json: Set(e.input.as_ref().and_then(|v| serde_json::to_string(v).ok())),
            output_json: Set(e
                .output
                .as_ref()
                .and_then(|v| serde_json::to_string(v).ok())),
            created_at: Set(e.created_at.to_rfc3339()),
        })
        .collect();
    tool_trace::Entity::insert_many(models).exec(db).await?;
    Ok(())
}

/// Получить полный журнал вызовов инструментов для сообщения (по порядку вызовов).
pub async fn find_tool_trace_by_message(
    db: &DatabaseConnection,
    message_id: &str,
) -> Result<Vec<ToolTraceEntry>, DbErr> {
    let models = tool_trace::Entity::find()
        .filter(tool_trace::Column::MessageId.eq(message_id))
        .order_by_asc(tool_trace::Column::Iteration)
        .order_by_asc(tool_trace::Column::CallIndex)
        .all(db)
        .await?;
    Ok(models.into_iter().map(Into::into).collect())
}

/// Все вызовы инструмента `tool` в чате, уже записанные в журнал.
///
/// Опора детерминированного гейта создания тикета: трасса пишется пачкой ПОСЛЕ того,
/// как ответ ассистента сохранён (см. `insert_tool_trace_batch`), поэтому во время
/// tool-цикла в таблице лежат только вызовы предыдущих ходов. Значит сам факт наличия
/// строки здесь означает «это было в прошлом ходе, пользователь успел ответить».
pub async fn find_tool_trace_by_tool(
    db: &DatabaseConnection,
    chat_id: &str,
    tool: &str,
) -> Result<Vec<ToolTraceEntry>, DbErr> {
    let models = tool_trace::Entity::find()
        .filter(tool_trace::Column::ChatId.eq(chat_id))
        .filter(tool_trace::Column::Tool.eq(tool))
        .order_by_asc(tool_trace::Column::CreatedAt)
        .all(db)
        .await?;
    Ok(models.into_iter().map(Into::into).collect())
}

// ============================================================================
// Attachment Repository Functions
// ============================================================================

/// Все вложения чата (в т.ч. привязанные к сообщениям), новейшие последними.
pub async fn find_attachments_by_chat_id(
    db: &DatabaseConnection,
    chat_id: &LlmChatId,
) -> Result<Vec<LlmChatAttachment>, DbErr> {
    let models = attachment::Entity::find()
        .filter(attachment::Column::ChatId.eq(chat_id.as_string()))
        .order_by_asc(attachment::Column::CreatedAt)
        .all(db)
        .await?;
    Ok(models.into_iter().map(|m| m.into()).collect())
}

/// Найти все вложения для сообщения
pub async fn find_attachments_by_message_id(
    db: &DatabaseConnection,
    message_id: &Uuid,
) -> Result<Vec<LlmChatAttachment>, DbErr> {
    let models = attachment::Entity::find()
        .filter(attachment::Column::MessageId.eq(message_id.to_string()))
        .order_by_asc(attachment::Column::CreatedAt)
        .all(db)
        .await?;

    Ok(models.into_iter().map(|m| m.into()).collect())
}

/// Найти одно вложение по его id.
pub async fn find_attachment_by_id(
    db: &DatabaseConnection,
    id: &Uuid,
) -> Result<Option<LlmChatAttachment>, DbErr> {
    let model = attachment::Entity::find_by_id(id.to_string())
        .one(db)
        .await?;
    Ok(model.map(|m| m.into()))
}

pub async fn delete_attachment_by_id(db: &DatabaseConnection, id: &Uuid) -> Result<(), DbErr> {
    attachment::Entity::delete_by_id(id.to_string())
        .exec(db)
        .await?;
    Ok(())
}

/// Привязать вложение к сообщению точечным UPDATE по id вложения.
/// Не затрагивает другие вложения (в т.ч. незавершённые загрузки из других чатов).
pub async fn bind_attachment_to_message(
    db: &DatabaseConnection,
    attachment_id: &Uuid,
    message_id: &Uuid,
) -> Result<(), DbErr> {
    attachment::Entity::update_many()
        .col_expr(
            attachment::Column::MessageId,
            Expr::value(message_id.to_string()),
        )
        .filter(attachment::Column::Id.eq(attachment_id.to_string()))
        .filter(attachment::Column::MessageId.is_null())
        .exec(db)
        .await?;
    Ok(())
}

/// Вставить вложение
pub async fn insert_attachment(
    db: &DatabaseConnection,
    attachment: &LlmChatAttachment,
) -> Result<(), DbErr> {
    let active_model = attachment::ActiveModel {
        id: Set(attachment.id.to_string()),
        chat_id: Set(Some(attachment.chat_id.as_string())),
        message_id: Set(attachment.message_id.map(|id| id.to_string())),
        filename: Set(attachment.filename.clone()),
        filepath: Set(attachment.filepath.clone()),
        s3_file_id: Set(attachment.s3_file_id.clone()),
        content_type: Set(attachment.content_type.clone()),
        file_size: Set(attachment.file_size),
        created_at: Set(attachment.created_at.to_rfc3339()),
    };

    active_model.insert(db).await?;
    Ok(())
}

/// Удалить все вложения сообщения
pub async fn delete_attachments_by_message_id(
    db: &DatabaseConnection,
    message_id: &Uuid,
) -> Result<(), DbErr> {
    attachment::Entity::delete_many()
        .filter(attachment::Column::MessageId.eq(message_id.to_string()))
        .exec(db)
        .await?;
    Ok(())
}

// ============================================================================
// Context Package Repository Functions
// ============================================================================

/// Вставить пакет контекста страницы.
pub async fn insert_context_package(
    db: &DatabaseConnection,
    pkg: NewContextPackage,
) -> Result<(), DbErr> {
    let active_model = context_package::ActiveModel {
        id: Set(pkg.id),
        chat_id: Set(pkg.chat_id),
        page_key: Set(pkg.page_key),
        page_type: Set(pkg.page_type),
        entity_index: Set(pkg.entity_index),
        entity_id: Set(pkg.entity_id),
        title: Set(pkg.title),
        context_json: Set(pkg.context_json),
        rendered_text: Set(pkg.rendered_text),
        created_at: Set(pkg.created_at),
        use_count: Set(0),
    };
    active_model.insert(db).await?;
    Ok(())
}

/// Получить один пакет контекста по id.
pub async fn find_context_by_id(
    db: &DatabaseConnection,
    id: &str,
) -> Result<Option<context_package::Model>, DbErr> {
    context_package::Entity::find_by_id(id.to_string())
        .one(db)
        .await
}

/// Получить пакеты контекста, привязанные к чату (старые → новые).
pub async fn list_context_by_chat(
    db: &DatabaseConnection,
    chat_id: &str,
) -> Result<Vec<context_package::Model>, DbErr> {
    context_package::Entity::find()
        .filter(context_package::Column::ChatId.eq(chat_id))
        .order_by_asc(context_package::Column::CreatedAt)
        .all(db)
        .await
}
