use chrono::Utc;
use contracts::domain::a017_llm_agent::aggregate::LlmAgentId;
use contracts::domain::a018_llm_chat::aggregate::LlmChatId;
use contracts::domain::a031_kb_edit::aggregate::{KbEdit, KbEditId, KbEditStatus, KbEditType};
use contracts::domain::common::{AggregateId, BaseAggregate, EntityMetadata};
use sea_orm::entity::prelude::*;
use sea_orm::prelude::Expr;
use sea_orm::{ColumnTrait, Condition, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder, Set};
use uuid::Uuid;

mod kb_edit_entity {
    use sea_orm::entity::prelude::*;
    use serde::{Deserialize, Serialize};

    #[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
    #[sea_orm(table_name = "a031_kb_edit")]
    pub struct Model {
        #[sea_orm(primary_key, auto_increment = false)]
        pub id: String,
        pub code: String,
        pub description: String,
        pub comment: Option<String>,
        pub edit_type: String,
        pub status: String,
        pub title: String,
        pub agent_summary: String,
        pub target_articles: String,
        pub applied_articles: String,
        pub source_chat_ids: String,
        pub agent_id: Option<String>,
        pub chat_id: Option<String>,
        pub analyze_task_run_id: Option<String>,
        pub post_task_run_id: Option<String>,
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

impl From<kb_edit_entity::Model> for KbEdit {
    fn from(m: kb_edit_entity::Model) -> Self {
        let metadata = EntityMetadata {
            created_at: m.created_at.unwrap_or_else(Utc::now),
            updated_at: m.updated_at.unwrap_or_else(Utc::now),
            is_deleted: m.is_deleted,
            is_posted: m.is_posted,
            version: m.version,
        };
        let uuid = Uuid::parse_str(&m.id).unwrap_or_else(|_| Uuid::new_v4());
        let target_articles = serde_json::from_str(&m.target_articles).unwrap_or_default();
        let applied_articles = serde_json::from_str(&m.applied_articles).unwrap_or_default();
        let source_chat_ids = serde_json::from_str(&m.source_chat_ids).unwrap_or_default();
        let agent_id = m
            .agent_id
            .as_deref()
            .and_then(|id| Uuid::parse_str(id).ok())
            .map(LlmAgentId::new);
        let chat_id = m
            .chat_id
            .as_deref()
            .and_then(|id| Uuid::parse_str(id).ok())
            .map(LlmChatId::new);

        KbEdit {
            base: BaseAggregate {
                id: KbEditId::new(uuid),
                code: m.code,
                description: m.description,
                comment: m.comment,
                metadata,
            },
            edit_type: KbEditType::from_str(&m.edit_type),
            status: KbEditStatus::from_str(&m.status),
            title: m.title,
            agent_summary: m.agent_summary,
            target_articles,
            applied_articles,
            source_chat_ids,
            agent_id,
            chat_id,
            analyze_task_run_id: m.analyze_task_run_id,
            post_task_run_id: m.post_task_run_id,
        }
    }
}

pub async fn list_paginated(
    db: &DatabaseConnection,
    page: u64,
    page_size: u64,
    sort_by: &str,
    sort_desc: bool,
    status: Option<&str>,
    q: Option<&str>,
) -> Result<(Vec<KbEdit>, u64), DbErr> {
    let mut query =
        kb_edit_entity::Entity::find().filter(kb_edit_entity::Column::IsDeleted.eq(false));

    if let Some(status) = status.map(str::trim).filter(|s| !s.is_empty()) {
        query = query.filter(kb_edit_entity::Column::Status.eq(status));
    }

    if let Some(search) = q.map(str::trim).filter(|s| !s.is_empty()) {
        query = query.filter(
            Condition::any()
                .add(kb_edit_entity::Column::Code.contains(search))
                .add(kb_edit_entity::Column::Title.contains(search))
                .add(kb_edit_entity::Column::AgentSummary.contains(search)),
        );
    }

    query = match (sort_by, sort_desc) {
        ("title", true) => query.order_by_desc(kb_edit_entity::Column::Title),
        ("title", false) => query.order_by_asc(kb_edit_entity::Column::Title),
        ("status", true) => query.order_by_desc(kb_edit_entity::Column::Status),
        ("status", false) => query.order_by_asc(kb_edit_entity::Column::Status),
        ("edit_type", true) => query.order_by_desc(kb_edit_entity::Column::EditType),
        ("edit_type", false) => query.order_by_asc(kb_edit_entity::Column::EditType),
        ("created_at", false) => query.order_by_asc(kb_edit_entity::Column::CreatedAt),
        _ => query.order_by_desc(kb_edit_entity::Column::CreatedAt),
    };

    let paginator = query.paginate(db, page_size);
    let total = paginator.num_items().await?;
    let models = paginator.fetch_page(page).await?;
    Ok((models.into_iter().map(Into::into).collect(), total))
}

pub async fn list_by_status(
    db: &DatabaseConnection,
    status: KbEditStatus,
) -> Result<Vec<KbEdit>, DbErr> {
    let models = kb_edit_entity::Entity::find()
        .filter(kb_edit_entity::Column::IsDeleted.eq(false))
        .filter(kb_edit_entity::Column::Status.eq(status.as_str()))
        .order_by_asc(kb_edit_entity::Column::CreatedAt)
        .all(db)
        .await?;
    Ok(models.into_iter().map(Into::into).collect())
}

pub async fn find_by_id(db: &DatabaseConnection, id: &KbEditId) -> Result<Option<KbEdit>, DbErr> {
    let model = kb_edit_entity::Entity::find_by_id(id.as_string())
        .filter(kb_edit_entity::Column::IsDeleted.eq(false))
        .one(db)
        .await?;
    Ok(model.map(Into::into))
}

pub async fn insert(db: &DatabaseConnection, item: &KbEdit) -> Result<(), DbErr> {
    let now = Utc::now();
    let active_model = kb_edit_entity::ActiveModel {
        id: Set(item.base.id.as_string()),
        code: Set(item.base.code.clone()),
        description: Set(item.base.description.clone()),
        comment: Set(item.base.comment.clone()),
        edit_type: Set(item.edit_type.as_str().to_string()),
        status: Set(item.status.as_str().to_string()),
        title: Set(item.title.clone()),
        agent_summary: Set(item.agent_summary.clone()),
        target_articles: Set(
            serde_json::to_string(&item.target_articles).unwrap_or_else(|_| "[]".to_string())
        ),
        applied_articles: Set(
            serde_json::to_string(&item.applied_articles).unwrap_or_else(|_| "[]".to_string())
        ),
        source_chat_ids: Set(
            serde_json::to_string(&item.source_chat_ids).unwrap_or_else(|_| "[]".to_string())
        ),
        agent_id: Set(item.agent_id.map(|id| id.as_string())),
        chat_id: Set(item.chat_id.map(|id| id.as_string())),
        analyze_task_run_id: Set(item.analyze_task_run_id.clone()),
        post_task_run_id: Set(item.post_task_run_id.clone()),
        is_deleted: Set(false),
        is_posted: Set(false),
        created_at: Set(Some(now)),
        updated_at: Set(Some(now)),
        version: Set(1),
    };
    active_model.insert(db).await?;
    Ok(())
}

pub async fn update(db: &DatabaseConnection, item: &KbEdit) -> Result<(), DbErr> {
    let now = Utc::now();
    let active_model = kb_edit_entity::ActiveModel {
        id: Set(item.base.id.as_string()),
        code: Set(item.base.code.clone()),
        description: Set(item.base.description.clone()),
        comment: Set(item.base.comment.clone()),
        edit_type: Set(item.edit_type.as_str().to_string()),
        status: Set(item.status.as_str().to_string()),
        title: Set(item.title.clone()),
        agent_summary: Set(item.agent_summary.clone()),
        target_articles: Set(
            serde_json::to_string(&item.target_articles).unwrap_or_else(|_| "[]".to_string())
        ),
        applied_articles: Set(
            serde_json::to_string(&item.applied_articles).unwrap_or_else(|_| "[]".to_string())
        ),
        source_chat_ids: Set(
            serde_json::to_string(&item.source_chat_ids).unwrap_or_else(|_| "[]".to_string())
        ),
        agent_id: Set(item.agent_id.map(|id| id.as_string())),
        chat_id: Set(item.chat_id.map(|id| id.as_string())),
        analyze_task_run_id: Set(item.analyze_task_run_id.clone()),
        post_task_run_id: Set(item.post_task_run_id.clone()),
        is_deleted: Set(item.base.metadata.is_deleted),
        is_posted: Set(item.base.metadata.is_posted),
        created_at: Set(Some(item.base.metadata.created_at)),
        updated_at: Set(Some(now)),
        version: Set(item.base.metadata.version + 1),
    };
    kb_edit_entity::Entity::update(active_model)
        .exec(db)
        .await?;
    Ok(())
}

pub async fn soft_delete(db: &DatabaseConnection, id: &KbEditId) -> Result<(), DbErr> {
    kb_edit_entity::Entity::update_many()
        .col_expr(kb_edit_entity::Column::IsDeleted, Expr::value(true))
        .col_expr(kb_edit_entity::Column::UpdatedAt, Expr::value(Utc::now()))
        .filter(kb_edit_entity::Column::Id.eq(id.as_string()))
        .exec(db)
        .await?;
    Ok(())
}
