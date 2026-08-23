use chrono::Utc;
use contracts::domain::a017_llm_agent::aggregate::LlmAgentId;
use contracts::domain::a018_llm_chat::aggregate::LlmChatId;
use contracts::domain::a019_llm_artifact::aggregate::{
    ArtifactStatus, ArtifactType, LlmArtifact, LlmArtifactId,
};
use contracts::domain::common::{AggregateId, BaseAggregate, EntityMetadata};
use sea_orm::entity::prelude::*;
use sea_orm::prelude::Expr;
use sea_orm::{
    ColumnTrait, ConnectionTrait, DatabaseBackend, EntityTrait, PaginatorTrait, QueryFilter,
    QueryOrder, Set, Statement,
};
use uuid::Uuid;

/// Idempotently create/update the plugin card for the latest user message.
/// Accepting a generic connection lets the plugin workflow include this write
/// in the same transaction as plugin, revision and snapshot.
pub async fn upsert_plugin_artifact<C: ConnectionTrait>(
    conn: &C,
    plugin_id: &str,
    plugin_code: &str,
    plugin_title: &str,
    chat_id: &str,
    agent_id: &str,
) -> Result<(String, Option<String>), DbErr> {
    let source_message_id = conn
        .query_one(Statement::from_sql_and_values(
            DatabaseBackend::Sqlite,
            "SELECT id FROM a018_llm_chat_message \
             WHERE chat_id = ? AND role = 'user' ORDER BY created_at DESC, id DESC LIMIT 1",
            vec![chat_id.into()],
        ))
        .await?
        .and_then(|row| row.try_get::<String>("", "id").ok());
    let code = source_message_id
        .as_deref()
        .map(|message_id| {
            format!(
                "PLUGIN-{plugin_code}-{}",
                message_id.chars().take(8).collect::<String>()
            )
        })
        .unwrap_or_else(|| format!("PLUGIN-{plugin_code}"));
    let query_params = serde_json::json!({
        "plugin_id": plugin_id,
        "code": plugin_code,
        "title": plugin_title,
        "source_message_id": source_message_id,
    })
    .to_string();

    if let Some(row) = conn
        .query_one(Statement::from_sql_and_values(
            DatabaseBackend::Sqlite,
            "SELECT id FROM a019_llm_artifact WHERE code = ? AND is_deleted = 0 LIMIT 1",
            vec![code.clone().into()],
        ))
        .await?
    {
        let artifact_id = row.try_get::<String>("", "id")?;
        conn.execute(Statement::from_sql_and_values(
            DatabaseBackend::Sqlite,
            "UPDATE a019_llm_artifact SET description = ?, query_params = ?, updated_at = datetime('now'), \
             version = version + 1 WHERE id = ?",
            vec![
                plugin_title.into(),
                query_params.into(),
                artifact_id.clone().into(),
            ],
        ))
        .await?;
        return Ok((artifact_id, source_message_id));
    }

    let artifact_id = Uuid::new_v4().to_string();
    conn.execute(Statement::from_sql_and_values(
        DatabaseBackend::Sqlite,
        "INSERT INTO a019_llm_artifact \
         (id, code, description, comment, chat_id, agent_id, artifact_type, status, sql_query, \
          query_params, execution_count, is_deleted, is_posted, created_at, updated_at, version) \
         VALUES (?, ?, ?, ?, ?, ?, 'plugin', 'active', '', ?, 0, 0, 0, datetime('now'), datetime('now'), 1)",
        vec![
            artifact_id.clone().into(),
            code.into(),
            plugin_title.into(),
            format!("Плагин {plugin_code}").into(),
            chat_id.into(),
            agent_id.into(),
            query_params.into(),
        ],
    ))
    .await?;
    Ok((artifact_id, source_message_id))
}

mod artifact {
    use sea_orm::entity::prelude::*;
    use serde::{Deserialize, Serialize};

    #[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
    #[sea_orm(table_name = "a019_llm_artifact")]
    pub struct Model {
        #[sea_orm(primary_key, auto_increment = false)]
        pub id: String,
        pub code: String,
        pub description: String,
        pub comment: Option<String>,
        pub chat_id: String,
        pub agent_id: String,
        pub artifact_type: String,
        pub status: String,
        pub sql_query: String,
        pub query_params: Option<String>,
        pub visualization_config: Option<String>,
        pub last_executed_at: Option<String>,
        pub execution_count: i32,
        pub is_deleted: bool,
        pub is_posted: bool,
        pub created_at: Option<chrono::DateTime<chrono::Utc>>,
        pub updated_at: Option<chrono::DateTime<chrono::Utc>>,
        pub version: i32,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}

    impl ActiveModelBehavior for ActiveModel {}
}

impl From<artifact::Model> for LlmArtifact {
    fn from(m: artifact::Model) -> Self {
        let metadata = EntityMetadata {
            created_at: m.created_at.unwrap_or_else(Utc::now),
            updated_at: m.updated_at.unwrap_or_else(Utc::now),
            is_deleted: m.is_deleted,
            is_posted: m.is_posted,
            version: m.version,
        };

        let uuid = Uuid::parse_str(&m.id).unwrap_or_else(|_| Uuid::new_v4());
        let chat_uuid = Uuid::parse_str(&m.chat_id).unwrap_or_else(|_| Uuid::new_v4());
        let agent_uuid = Uuid::parse_str(&m.agent_id).unwrap_or_else(|_| Uuid::new_v4());

        let artifact_type =
            ArtifactType::from_str(&m.artifact_type).unwrap_or(ArtifactType::SqlQuery);
        let status = ArtifactStatus::from_str(&m.status).unwrap_or(ArtifactStatus::Active);

        let last_executed_at = m.last_executed_at.and_then(|s| {
            chrono::DateTime::parse_from_rfc3339(&s)
                .ok()
                .map(|dt| dt.with_timezone(&Utc))
        });

        LlmArtifact {
            base: BaseAggregate {
                id: LlmArtifactId::new(uuid),
                code: m.code,
                description: m.description,
                comment: m.comment,
                metadata,
            },
            chat_id: LlmChatId::new(chat_uuid),
            agent_id: LlmAgentId::new(agent_uuid),
            artifact_type,
            status,
            sql_query: m.sql_query,
            query_params: m.query_params,
            visualization_config: m.visualization_config,
            last_executed_at,
            execution_count: m.execution_count,
        }
    }
}

// ============================================================================
// Artifact Repository Functions
// ============================================================================

/// Получить все артефакты (не удаленные)
pub async fn list_all(db: &DatabaseConnection) -> Result<Vec<LlmArtifact>, DbErr> {
    let models = artifact::Entity::find()
        .filter(artifact::Column::IsDeleted.eq(false))
        .order_by_desc(artifact::Column::CreatedAt)
        .all(db)
        .await?;

    Ok(models.into_iter().map(|m| m.into()).collect())
}

/// Получить артефакты конкретного чата
pub async fn list_by_chat_id(
    db: &DatabaseConnection,
    chat_id: &LlmChatId,
) -> Result<Vec<LlmArtifact>, DbErr> {
    let models = artifact::Entity::find()
        .filter(artifact::Column::ChatId.eq(chat_id.as_string()))
        .filter(artifact::Column::IsDeleted.eq(false))
        .order_by_desc(artifact::Column::CreatedAt)
        .all(db)
        .await?;

    Ok(models.into_iter().map(|m| m.into()).collect())
}

/// Получить артефакты с пагинацией
pub async fn list_paginated(
    db: &DatabaseConnection,
    page: u64,
    page_size: u64,
) -> Result<(Vec<LlmArtifact>, u64), DbErr> {
    let query = artifact::Entity::find()
        .filter(artifact::Column::IsDeleted.eq(false))
        .order_by_desc(artifact::Column::CreatedAt);

    let paginator = query.paginate(db, page_size);
    let total = paginator.num_items().await?;
    let models = paginator.fetch_page(page).await?;

    Ok((models.into_iter().map(|m| m.into()).collect(), total))
}

/// Найти артефакт по ID
pub async fn find_by_id(
    db: &DatabaseConnection,
    id: &LlmArtifactId,
) -> Result<Option<LlmArtifact>, DbErr> {
    let model = artifact::Entity::find_by_id(id.as_string()).one(db).await?;
    Ok(model.map(|m| m.into()))
}

/// Вставить новый артефакт
pub async fn insert(db: &DatabaseConnection, artifact: &LlmArtifact) -> Result<(), DbErr> {
    let now = Utc::now();
    let active_model = artifact::ActiveModel {
        id: Set(artifact.base.id.as_string()),
        code: Set(artifact.base.code.clone()),
        description: Set(artifact.base.description.clone()),
        comment: Set(artifact.base.comment.clone()),
        chat_id: Set(artifact.chat_id.as_string()),
        agent_id: Set(artifact.agent_id.as_string()),
        artifact_type: Set(artifact.artifact_type.as_str().to_string()),
        status: Set(artifact.status.as_str().to_string()),
        sql_query: Set(artifact.sql_query.clone()),
        query_params: Set(artifact.query_params.clone()),
        visualization_config: Set(artifact.visualization_config.clone()),
        last_executed_at: Set(artifact.last_executed_at.map(|dt| dt.to_rfc3339())),
        execution_count: Set(artifact.execution_count),
        is_deleted: Set(false),
        is_posted: Set(false),
        created_at: Set(Some(now)),
        updated_at: Set(Some(now)),
        version: Set(1),
    };

    active_model.insert(db).await?;
    Ok(())
}

/// Обновить артефакт
pub async fn update(db: &DatabaseConnection, artifact: &LlmArtifact) -> Result<(), DbErr> {
    let now = Utc::now();
    let active_model = artifact::ActiveModel {
        id: Set(artifact.base.id.as_string()),
        code: Set(artifact.base.code.clone()),
        description: Set(artifact.base.description.clone()),
        comment: Set(artifact.base.comment.clone()),
        chat_id: Set(artifact.chat_id.as_string()),
        agent_id: Set(artifact.agent_id.as_string()),
        artifact_type: Set(artifact.artifact_type.as_str().to_string()),
        status: Set(artifact.status.as_str().to_string()),
        sql_query: Set(artifact.sql_query.clone()),
        query_params: Set(artifact.query_params.clone()),
        visualization_config: Set(artifact.visualization_config.clone()),
        last_executed_at: Set(artifact.last_executed_at.map(|dt| dt.to_rfc3339())),
        execution_count: Set(artifact.execution_count),
        is_deleted: Set(artifact.base.metadata.is_deleted),
        is_posted: Set(artifact.base.metadata.is_posted),
        created_at: Set(Some(artifact.base.metadata.created_at)),
        updated_at: Set(Some(now)),
        version: Set(artifact.base.metadata.version + 1),
    };

    artifact::Entity::update(active_model).exec(db).await?;
    Ok(())
}

/// Мягкое удаление артефакта
pub async fn soft_delete(db: &DatabaseConnection, id: &LlmArtifactId) -> Result<(), DbErr> {
    let now = Utc::now();
    artifact::Entity::update_many()
        .col_expr(artifact::Column::IsDeleted, Expr::value(true))
        .col_expr(artifact::Column::UpdatedAt, Expr::value(now))
        .filter(artifact::Column::Id.eq(id.as_string()))
        .exec(db)
        .await?;
    Ok(())
}
