use chrono::Utc;
use contracts::domain::a024_bi_indicator::aggregate::{
    BiIndicator, BiIndicatorId, BiIndicatorStatus, DataSpec, DrillSpec, ParamDef, ViewSpec,
};
use contracts::domain::common::{AggregateId, BaseAggregate, EntityMetadata};
use sea_orm::entity::prelude::*;
use sea_orm::prelude::Expr;
use sea_orm::{ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder, Set};
use uuid::Uuid;

mod bi_indicator {
    use sea_orm::entity::prelude::*;
    use serde::{Deserialize, Serialize};

    #[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
    #[sea_orm(table_name = "a024_bi_indicator")]
    pub struct Model {
        #[sea_orm(primary_key, auto_increment = false)]
        pub id: String,
        pub code: String,
        pub description: String,
        pub comment: Option<String>,
        pub explanation: Option<String>,
        pub data_spec_json: String,
        pub params_json: String,
        pub view_spec_json: String,
        pub drill_spec_json: Option<String>,
        pub status: String,
        pub owner_user_id: String,
        pub is_public: bool,
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

impl From<bi_indicator::Model> for BiIndicator {
    fn from(m: bi_indicator::Model) -> Self {
        let metadata = EntityMetadata {
            created_at: m.created_at.unwrap_or_else(Utc::now),
            updated_at: m.updated_at.unwrap_or_else(Utc::now),
            is_deleted: m.is_deleted,
            is_posted: m.is_posted,
            version: m.version,
        };

        let uuid = Uuid::parse_str(&m.id).unwrap_or_else(|_| Uuid::new_v4());

        let data_spec: DataSpec =
            serde_json::from_str(&m.data_spec_json).unwrap_or_else(|_| DataSpec::default());

        let params: Vec<ParamDef> = serde_json::from_str(&m.params_json).unwrap_or_default();

        let view_spec: ViewSpec =
            serde_json::from_str(&m.view_spec_json).unwrap_or_else(|_| ViewSpec::default());

        let drill_spec: Option<DrillSpec> = m
            .drill_spec_json
            .as_deref()
            .and_then(|s| serde_json::from_str(s).ok());

        let status = BiIndicatorStatus::from_str(&m.status).unwrap_or(BiIndicatorStatus::Draft);

        BiIndicator {
            base: BaseAggregate {
                id: BiIndicatorId::new(uuid),
                code: m.code,
                description: m.description,
                comment: m.comment,
                metadata,
            },
            explanation: m.explanation,
            data_spec,
            params,
            view_spec,
            drill_spec,
            status,
            owner_user_id: m.owner_user_id,
            is_public: m.is_public,
            created_by: m.created_by,
            updated_by: m.updated_by,
        }
    }
}

// ============================================================================
// Repository functions
// ============================================================================

fn normalize_search_text(value: &str) -> String {
    value.trim().to_lowercase()
}

fn sort_models(models: &mut [bi_indicator::Model], sort_by: &str, sort_desc: bool) {
    models.sort_by(|left, right| {
        let ordering = match sort_by {
            "code" => left.code.to_lowercase().cmp(&right.code.to_lowercase()),
            "description" => left
                .description
                .to_lowercase()
                .cmp(&right.description.to_lowercase()),
            "status" => left.status.cmp(&right.status),
            "created_at" => left.created_at.cmp(&right.created_at),
            _ => left.created_at.cmp(&right.created_at),
        };

        if sort_desc {
            ordering.reverse()
        } else {
            ordering
        }
    });
}

/// Получить все индикаторы с пагинацией
pub async fn list_paginated(
    db: &DatabaseConnection,
    page: u64,
    page_size: u64,
    sort_by: &str,
    sort_desc: bool,
    q: Option<&str>,
) -> Result<(Vec<BiIndicator>, u64), DbErr> {
    let mut query = bi_indicator::Entity::find().filter(bi_indicator::Column::IsDeleted.eq(false));

    if let Some(search) = q.map(normalize_search_text).filter(|s| !s.is_empty()) {
        let mut models = query.all(db).await?;
        models.retain(|item| {
            normalize_search_text(&item.code).contains(&search)
                || normalize_search_text(&item.description).contains(&search)
        });
        sort_models(&mut models, sort_by, sort_desc);

        let total = models.len() as u64;
        let offset = page.saturating_mul(page_size) as usize;
        let items = models
            .into_iter()
            .skip(offset)
            .take(page_size as usize)
            .map(Into::into)
            .collect();

        return Ok((items, total));
    }

    query = match (sort_by, sort_desc) {
        ("code", true) => query.order_by_desc(bi_indicator::Column::Code),
        ("code", false) => query.order_by_asc(bi_indicator::Column::Code),
        ("description", true) => query.order_by_desc(bi_indicator::Column::Description),
        ("description", false) => query.order_by_asc(bi_indicator::Column::Description),
        ("status", true) => query.order_by_desc(bi_indicator::Column::Status),
        ("status", false) => query.order_by_asc(bi_indicator::Column::Status),
        ("created_at", false) => query.order_by_asc(bi_indicator::Column::CreatedAt),
        _ => query.order_by_desc(bi_indicator::Column::CreatedAt),
    };

    let paginator = query.paginate(db, page_size);
    let total = paginator.num_items().await?;
    let models = paginator.fetch_page(page).await?;

    Ok((models.into_iter().map(|m| m.into()).collect(), total))
}

/// Получить все индикаторы (без пагинации)
pub async fn list_all(db: &DatabaseConnection) -> Result<Vec<BiIndicator>, DbErr> {
    let models = bi_indicator::Entity::find()
        .filter(bi_indicator::Column::IsDeleted.eq(false))
        .order_by_desc(bi_indicator::Column::CreatedAt)
        .all(db)
        .await?;

    Ok(models.into_iter().map(|m| m.into()).collect())
}

/// Получить индикаторы конкретного владельца
pub async fn list_by_owner(
    db: &DatabaseConnection,
    owner_user_id: &str,
) -> Result<Vec<BiIndicator>, DbErr> {
    let models = bi_indicator::Entity::find()
        .filter(bi_indicator::Column::IsDeleted.eq(false))
        .filter(bi_indicator::Column::OwnerUserId.eq(owner_user_id))
        .order_by_desc(bi_indicator::Column::CreatedAt)
        .all(db)
        .await?;

    Ok(models.into_iter().map(|m| m.into()).collect())
}

/// Получить публичные индикаторы
pub async fn list_public(db: &DatabaseConnection) -> Result<Vec<BiIndicator>, DbErr> {
    let models = bi_indicator::Entity::find()
        .filter(bi_indicator::Column::IsDeleted.eq(false))
        .filter(bi_indicator::Column::IsPublic.eq(true))
        .order_by_desc(bi_indicator::Column::CreatedAt)
        .all(db)
        .await?;

    Ok(models.into_iter().map(|m| m.into()).collect())
}

/// Найти индикатор по ID
pub async fn find_by_id(
    db: &DatabaseConnection,
    id: &BiIndicatorId,
) -> Result<Option<BiIndicator>, DbErr> {
    let model = bi_indicator::Entity::find_by_id(id.as_string())
        .one(db)
        .await?;
    Ok(model.map(|m| m.into()))
}

/// Найти индикатор по code
pub async fn find_by_code(
    db: &DatabaseConnection,
    code: &str,
) -> Result<Option<BiIndicator>, DbErr> {
    let model = bi_indicator::Entity::find()
        .filter(bi_indicator::Column::IsDeleted.eq(false))
        .filter(bi_indicator::Column::Code.eq(code))
        .one(db)
        .await?;
    Ok(model.map(|m| m.into()))
}

/// Получить набор индикаторов по списку id
pub async fn list_by_ids(
    db: &DatabaseConnection,
    ids: &[String],
) -> Result<Vec<BiIndicator>, DbErr> {
    if ids.is_empty() {
        return Ok(vec![]);
    }

    let models = bi_indicator::Entity::find()
        .filter(bi_indicator::Column::IsDeleted.eq(false))
        .filter(bi_indicator::Column::Id.is_in(ids.iter().cloned()))
        .all(db)
        .await?;

    Ok(models.into_iter().map(Into::into).collect())
}

/// Вставить новый индикатор
pub async fn insert(db: &DatabaseConnection, indicator: &BiIndicator) -> Result<(), DbErr> {
    let now = Utc::now();

    let data_spec_json =
        serde_json::to_string(&indicator.data_spec).unwrap_or_else(|_| "{}".to_string());
    let params_json = serde_json::to_string(&indicator.params).unwrap_or_else(|_| "[]".to_string());
    let view_spec_json =
        serde_json::to_string(&indicator.view_spec).unwrap_or_else(|_| "{}".to_string());
    let drill_spec_json = indicator
        .drill_spec
        .as_ref()
        .and_then(|d| serde_json::to_string(d).ok());

    let active_model = bi_indicator::ActiveModel {
        id: Set(indicator.base.id.as_string()),
        code: Set(indicator.base.code.clone()),
        description: Set(indicator.base.description.clone()),
        comment: Set(indicator.base.comment.clone()),
        explanation: Set(indicator.explanation.clone()),
        data_spec_json: Set(data_spec_json),
        params_json: Set(params_json),
        view_spec_json: Set(view_spec_json),
        drill_spec_json: Set(drill_spec_json),
        status: Set(indicator.status.as_str().to_string()),
        owner_user_id: Set(indicator.owner_user_id.clone()),
        is_public: Set(indicator.is_public),
        created_by: Set(indicator.created_by.clone()),
        updated_by: Set(indicator.updated_by.clone()),
        is_deleted: Set(false),
        is_posted: Set(false),
        created_at: Set(Some(now)),
        updated_at: Set(Some(now)),
        version: Set(1),
    };

    active_model.insert(db).await?;
    Ok(())
}

/// Обновить индикатор
pub async fn update(db: &DatabaseConnection, indicator: &BiIndicator) -> Result<(), DbErr> {
    let now = Utc::now();

    let data_spec_json =
        serde_json::to_string(&indicator.data_spec).unwrap_or_else(|_| "{}".to_string());
    let params_json = serde_json::to_string(&indicator.params).unwrap_or_else(|_| "[]".to_string());
    let view_spec_json =
        serde_json::to_string(&indicator.view_spec).unwrap_or_else(|_| "{}".to_string());
    let drill_spec_json = indicator
        .drill_spec
        .as_ref()
        .and_then(|d| serde_json::to_string(d).ok());

    let active_model = bi_indicator::ActiveModel {
        id: Set(indicator.base.id.as_string()),
        code: Set(indicator.base.code.clone()),
        description: Set(indicator.base.description.clone()),
        comment: Set(indicator.base.comment.clone()),
        explanation: Set(indicator.explanation.clone()),
        data_spec_json: Set(data_spec_json),
        params_json: Set(params_json),
        view_spec_json: Set(view_spec_json),
        drill_spec_json: Set(drill_spec_json),
        status: Set(indicator.status.as_str().to_string()),
        owner_user_id: Set(indicator.owner_user_id.clone()),
        is_public: Set(indicator.is_public),
        created_by: Set(indicator.created_by.clone()),
        updated_by: Set(indicator.updated_by.clone()),
        is_deleted: Set(indicator.base.metadata.is_deleted),
        is_posted: Set(indicator.base.metadata.is_posted),
        created_at: Set(Some(indicator.base.metadata.created_at)),
        updated_at: Set(Some(now)),
        version: Set(indicator.base.metadata.version + 1),
    };

    bi_indicator::Entity::update(active_model).exec(db).await?;
    Ok(())
}

/// Мягкое удаление индикатора
pub async fn soft_delete(db: &DatabaseConnection, id: &BiIndicatorId) -> Result<(), DbErr> {
    let now = Utc::now();
    bi_indicator::Entity::update_many()
        .col_expr(bi_indicator::Column::IsDeleted, Expr::value(true))
        .col_expr(bi_indicator::Column::UpdatedAt, Expr::value(now))
        .filter(bi_indicator::Column::Id.eq(id.as_string()))
        .exec(db)
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::normalize_search_text;

    #[test]
    fn normalize_search_text_handles_cyrillic_case_insensitively() {
        assert!(
            normalize_search_text("Название индикатора").contains(&normalize_search_text("назв"))
        );
        assert!(normalize_search_text("доХод").contains(&normalize_search_text("ДОХ")));
    }
}
