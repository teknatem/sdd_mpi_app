use chrono::Utc;
use contracts::domain::a025_bi_dashboard::aggregate::{
    BiDashboard, BiDashboardId, BiDashboardStatus, DashboardLayout,
};
use contracts::domain::common::{AggregateId, BaseAggregate, EntityMetadata};
use contracts::shared::data_view::FilterRef;
use sea_orm::entity::prelude::*;
use sea_orm::prelude::Expr;
use sea_orm::{ColumnTrait, Condition, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder, Set};
use uuid::Uuid;

mod bi_dashboard {
    use sea_orm::entity::prelude::*;
    use serde::{Deserialize, Serialize};

    #[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
    #[sea_orm(table_name = "a025_bi_dashboard")]
    pub struct Model {
        #[sea_orm(primary_key, auto_increment = false)]
        pub id: String,
        pub code: String,
        pub description: String,
        pub comment: Option<String>,
        pub layout_json: String,
        pub global_filters_json: String,
        pub status: String,
        pub owner_user_id: String,
        pub is_public: bool,
        pub rating: Option<i32>,
        pub created_by: Option<String>,
        pub updated_by: Option<String>,
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

impl From<bi_dashboard::Model> for BiDashboard {
    fn from(m: bi_dashboard::Model) -> Self {
        let metadata = EntityMetadata {
            created_at: m.created_at.unwrap_or_else(Utc::now),
            updated_at: m.updated_at.unwrap_or_else(Utc::now),
            is_deleted: m.is_deleted,
            is_posted: m.is_posted,
            version: m.version,
        };

        let uuid = Uuid::parse_str(&m.id).unwrap_or_else(|_| Uuid::new_v4());

        let layout: DashboardLayout =
            serde_json::from_str(&m.layout_json).unwrap_or_else(|_| DashboardLayout::default());

        let filters: Vec<FilterRef> =
            serde_json::from_str(&m.global_filters_json).unwrap_or_default();

        let status = BiDashboardStatus::from_str(&m.status).unwrap_or(BiDashboardStatus::Draft);

        let rating = m.rating.and_then(|r| {
            let r = r as u8;
            if r >= 1 && r <= 5 {
                Some(r)
            } else {
                None
            }
        });

        BiDashboard {
            base: BaseAggregate {
                id: BiDashboardId::new(uuid),
                code: m.code,
                description: m.description,
                comment: m.comment,
                metadata,
            },
            layout,
            filters,
            status,
            owner_user_id: m.owner_user_id,
            is_public: m.is_public,
            rating,
            created_by: m.created_by,
            updated_by: m.updated_by,
        }
    }
}

// ============================================================================
// Repository functions
// ============================================================================

/// Получить все дашборды с пагинацией
pub async fn list_paginated(
    db: &DatabaseConnection,
    page: u64,
    page_size: u64,
    sort_by: &str,
    sort_desc: bool,
    q: Option<&str>,
) -> Result<(Vec<BiDashboard>, u64), DbErr> {
    let mut query = bi_dashboard::Entity::find().filter(bi_dashboard::Column::IsDeleted.eq(false));

    if let Some(search) = q.map(str::trim).filter(|s| !s.is_empty()) {
        query = query.filter(
            Condition::any()
                .add(bi_dashboard::Column::Code.contains(search))
                .add(bi_dashboard::Column::Description.contains(search)),
        );
    }

    query = match (sort_by, sort_desc) {
        ("code", true) => query.order_by_desc(bi_dashboard::Column::Code),
        ("code", false) => query.order_by_asc(bi_dashboard::Column::Code),
        ("description", true) => query.order_by_desc(bi_dashboard::Column::Description),
        ("description", false) => query.order_by_asc(bi_dashboard::Column::Description),
        ("status", true) => query.order_by_desc(bi_dashboard::Column::Status),
        ("status", false) => query.order_by_asc(bi_dashboard::Column::Status),
        ("created_at", false) => query.order_by_asc(bi_dashboard::Column::CreatedAt),
        _ => query.order_by_desc(bi_dashboard::Column::CreatedAt),
    };

    let paginator = query.paginate(db, page_size);
    let total = paginator.num_items().await?;
    let models = paginator.fetch_page(page).await?;

    Ok((models.into_iter().map(|m| m.into()).collect(), total))
}

/// Получить все дашборды (без пагинации)
pub async fn list_all(db: &DatabaseConnection) -> Result<Vec<BiDashboard>, DbErr> {
    let models = bi_dashboard::Entity::find()
        .filter(bi_dashboard::Column::IsDeleted.eq(false))
        .order_by_desc(bi_dashboard::Column::CreatedAt)
        .all(db)
        .await?;

    Ok(models.into_iter().map(|m| m.into()).collect())
}

/// Получить дашборды конкретного владельца
pub async fn list_by_owner(
    db: &DatabaseConnection,
    owner_user_id: &str,
) -> Result<Vec<BiDashboard>, DbErr> {
    let models = bi_dashboard::Entity::find()
        .filter(bi_dashboard::Column::IsDeleted.eq(false))
        .filter(bi_dashboard::Column::OwnerUserId.eq(owner_user_id))
        .order_by_desc(bi_dashboard::Column::CreatedAt)
        .all(db)
        .await?;

    Ok(models.into_iter().map(|m| m.into()).collect())
}

/// Получить публичные дашборды
pub async fn list_public(db: &DatabaseConnection) -> Result<Vec<BiDashboard>, DbErr> {
    let models = bi_dashboard::Entity::find()
        .filter(bi_dashboard::Column::IsDeleted.eq(false))
        .filter(bi_dashboard::Column::IsPublic.eq(true))
        .order_by_desc(bi_dashboard::Column::CreatedAt)
        .all(db)
        .await?;

    Ok(models.into_iter().map(|m| m.into()).collect())
}

/// Найти дашборд по ID
pub async fn find_by_id(
    db: &DatabaseConnection,
    id: &BiDashboardId,
) -> Result<Option<BiDashboard>, DbErr> {
    let model = bi_dashboard::Entity::find_by_id(id.as_string())
        .one(db)
        .await?;
    Ok(model.map(|m| m.into()))
}

/// Вставить новый дашборд
pub async fn insert(db: &DatabaseConnection, dashboard: &BiDashboard) -> Result<(), DbErr> {
    let now = Utc::now();

    let layout_json =
        serde_json::to_string(&dashboard.layout).unwrap_or_else(|_| r#"{"groups":[]}"#.to_string());
    let global_filters_json =
        serde_json::to_string(&dashboard.filters).unwrap_or_else(|_| "[]".to_string());

    let active_model = bi_dashboard::ActiveModel {
        id: Set(dashboard.base.id.as_string()),
        code: Set(dashboard.base.code.clone()),
        description: Set(dashboard.base.description.clone()),
        comment: Set(dashboard.base.comment.clone()),
        layout_json: Set(layout_json),
        global_filters_json: Set(global_filters_json),
        status: Set(dashboard.status.as_str().to_string()),
        owner_user_id: Set(dashboard.owner_user_id.clone()),
        is_public: Set(dashboard.is_public),
        rating: Set(dashboard.rating.map(|r| r as i32)),
        created_by: Set(dashboard.created_by.clone()),
        updated_by: Set(dashboard.updated_by.clone()),
        is_deleted: Set(false),
        is_posted: Set(false),
        created_at: Set(Some(now)),
        updated_at: Set(Some(now)),
        version: Set(1),
    };

    active_model.insert(db).await?;
    Ok(())
}

/// Обновить дашборд
pub async fn update(db: &DatabaseConnection, dashboard: &BiDashboard) -> Result<(), DbErr> {
    let now = Utc::now();

    let layout_json =
        serde_json::to_string(&dashboard.layout).unwrap_or_else(|_| r#"{"groups":[]}"#.to_string());
    let global_filters_json =
        serde_json::to_string(&dashboard.filters).unwrap_or_else(|_| "[]".to_string());

    let active_model = bi_dashboard::ActiveModel {
        id: Set(dashboard.base.id.as_string()),
        code: Set(dashboard.base.code.clone()),
        description: Set(dashboard.base.description.clone()),
        comment: Set(dashboard.base.comment.clone()),
        layout_json: Set(layout_json),
        global_filters_json: Set(global_filters_json),
        status: Set(dashboard.status.as_str().to_string()),
        owner_user_id: Set(dashboard.owner_user_id.clone()),
        is_public: Set(dashboard.is_public),
        rating: Set(dashboard.rating.map(|r| r as i32)),
        created_by: Set(dashboard.created_by.clone()),
        updated_by: Set(dashboard.updated_by.clone()),
        is_deleted: Set(dashboard.base.metadata.is_deleted),
        is_posted: Set(dashboard.base.metadata.is_posted),
        created_at: Set(Some(dashboard.base.metadata.created_at)),
        updated_at: Set(Some(now)),
        version: Set(dashboard.base.metadata.version + 1),
    };

    bi_dashboard::Entity::update(active_model).exec(db).await?;
    Ok(())
}

/// Мягкое удаление дашборда
pub async fn soft_delete(db: &DatabaseConnection, id: &BiDashboardId) -> Result<(), DbErr> {
    let now = Utc::now();
    bi_dashboard::Entity::update_many()
        .col_expr(bi_dashboard::Column::IsDeleted, Expr::value(true))
        .col_expr(bi_dashboard::Column::UpdatedAt, Expr::value(now))
        .filter(bi_dashboard::Column::Id.eq(id.as_string()))
        .exec(db)
        .await?;
    Ok(())
}
