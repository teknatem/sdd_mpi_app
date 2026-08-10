use chrono::Utc;
use contracts::domain::a038_llm_connection::aggregate::{
    LlmConnection, LlmConnectionId, LlmProviderType,
};
use contracts::domain::common::{BaseAggregate, EntityMetadata};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::shared::data::db::get_connection;
use sea_orm::entity::prelude::*;
use sea_orm::{
    ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder, QuerySelect, Set,
};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "a038_llm_connection")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub code: String,
    pub description: String,
    pub comment: Option<String>,
    pub provider_type: String,
    pub api_endpoint: String,
    pub api_key: String,
    pub model_name: String,
    pub temperature: f64,
    pub max_tokens: i32,
    pub context_window: i32,
    pub is_primary: bool,
    pub available_models: Option<String>,
    pub allowed_models: Option<String>,
    pub image_input_models: Option<String>,
    pub price_in_per_mtok: Option<f64>,
    pub price_out_per_mtok: Option<f64>,
    pub price_cached_per_mtok: Option<f64>,
    pub currency: Option<String>,
    pub is_deleted: bool,
    pub is_posted: bool,
    pub created_at: Option<chrono::DateTime<chrono::Utc>>,
    pub updated_at: Option<chrono::DateTime<chrono::Utc>>,
    pub version: i32,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}

impl From<Model> for LlmConnection {
    fn from(m: Model) -> Self {
        let metadata = EntityMetadata {
            created_at: m.created_at.unwrap_or_else(Utc::now),
            updated_at: m.updated_at.unwrap_or_else(Utc::now),
            is_deleted: m.is_deleted,
            is_posted: m.is_posted,
            version: m.version,
        };
        let uuid = Uuid::parse_str(&m.id).unwrap_or_else(|_| Uuid::new_v4());
        let provider_type =
            LlmProviderType::from_str(&m.provider_type).unwrap_or(LlmProviderType::OpenAI);

        LlmConnection {
            base: BaseAggregate::with_metadata(
                LlmConnectionId(uuid),
                m.code,
                m.description,
                m.comment.clone(),
                metadata,
            ),
            provider_type,
            api_endpoint: m.api_endpoint,
            api_key: m.api_key,
            model_name: m.model_name,
            temperature: m.temperature,
            max_tokens: m.max_tokens,
            context_window: m.context_window,
            is_primary: m.is_primary,
            available_models: m.available_models,
            allowed_models: m.allowed_models,
            image_input_models: m.image_input_models,
            price_in_per_mtok: m.price_in_per_mtok,
            price_out_per_mtok: m.price_out_per_mtok,
            price_cached_per_mtok: m.price_cached_per_mtok,
            currency: m.currency,
        }
    }
}

fn conn() -> &'static DatabaseConnection {
    get_connection()
}

pub async fn list_all() -> anyhow::Result<Vec<LlmConnection>> {
    let mut items: Vec<LlmConnection> = Entity::find()
        .filter(Column::IsDeleted.eq(false))
        .all(conn())
        .await?
        .into_iter()
        .map(Into::into)
        .collect();
    items.sort_by(|a, b| {
        a.base
            .description
            .to_lowercase()
            .cmp(&b.base.description.to_lowercase())
    });
    Ok(items)
}

/// Пагинированный список подключений LLM
pub async fn list_paginated(
    limit: u64,
    offset: u64,
    sort_by: &str,
    sort_desc: bool,
) -> anyhow::Result<(Vec<LlmConnection>, u64)> {
    let total = Entity::find()
        .filter(Column::IsDeleted.eq(false))
        .count(conn())
        .await?;

    let mut query = Entity::find().filter(Column::IsDeleted.eq(false));

    query = match sort_by {
        "code" => {
            if sort_desc {
                query.order_by_desc(Column::Code)
            } else {
                query.order_by_asc(Column::Code)
            }
        }
        "description" => {
            if sort_desc {
                query.order_by_desc(Column::Description)
            } else {
                query.order_by_asc(Column::Description)
            }
        }
        "provider_type" => {
            if sort_desc {
                query.order_by_desc(Column::ProviderType)
            } else {
                query.order_by_asc(Column::ProviderType)
            }
        }
        "model_name" => {
            if sort_desc {
                query.order_by_desc(Column::ModelName)
            } else {
                query.order_by_asc(Column::ModelName)
            }
        }
        _ => {
            if sort_desc {
                query.order_by_desc(Column::Description)
            } else {
                query.order_by_asc(Column::Description)
            }
        }
    };

    let items: Vec<LlmConnection> = query
        .offset(offset)
        .limit(limit)
        .all(conn())
        .await?
        .into_iter()
        .map(Into::into)
        .collect();

    Ok((items, total))
}

pub async fn find_by_id(id: &str) -> anyhow::Result<Option<LlmConnection>> {
    let model = Entity::find_by_id(id.to_string())
        .filter(Column::IsDeleted.eq(false))
        .one(conn())
        .await?;

    Ok(model.map(Into::into))
}

pub async fn find_primary() -> anyhow::Result<Option<LlmConnection>> {
    let model = Entity::find()
        .filter(Column::IsDeleted.eq(false))
        .filter(Column::IsPrimary.eq(true))
        .one(conn())
        .await?;

    Ok(model.map(Into::into))
}

pub async fn insert(item: &LlmConnection) -> anyhow::Result<()> {
    let now = Utc::now();
    let active = ActiveModel {
        id: Set(item.to_string_id()),
        code: Set(item.base.code.clone()),
        description: Set(item.base.description.clone()),
        comment: Set(item.base.comment.clone()),
        provider_type: Set(item.provider_type.as_str().to_string()),
        api_endpoint: Set(item.api_endpoint.clone()),
        api_key: Set(item.api_key.clone()),
        model_name: Set(item.model_name.clone()),
        temperature: Set(item.temperature),
        max_tokens: Set(item.max_tokens),
        context_window: Set(item.context_window),
        is_primary: Set(item.is_primary),
        available_models: Set(item.available_models.clone()),
        allowed_models: Set(item.allowed_models.clone()),
        image_input_models: Set(item.image_input_models.clone()),
        price_in_per_mtok: Set(item.price_in_per_mtok),
        price_out_per_mtok: Set(item.price_out_per_mtok),
        price_cached_per_mtok: Set(item.price_cached_per_mtok),
        currency: Set(item.currency.clone()),
        is_deleted: Set(false),
        is_posted: Set(false),
        created_at: Set(Some(now)),
        updated_at: Set(Some(now)),
        version: Set(1),
    };

    Entity::insert(active).exec(conn()).await?;
    Ok(())
}

pub async fn update(item: &LlmConnection) -> anyhow::Result<()> {
    let now = Utc::now();
    let active = ActiveModel {
        id: Set(item.to_string_id()),
        code: Set(item.base.code.clone()),
        description: Set(item.base.description.clone()),
        comment: Set(item.base.comment.clone()),
        provider_type: Set(item.provider_type.as_str().to_string()),
        api_endpoint: Set(item.api_endpoint.clone()),
        api_key: Set(item.api_key.clone()),
        model_name: Set(item.model_name.clone()),
        temperature: Set(item.temperature),
        max_tokens: Set(item.max_tokens),
        context_window: Set(item.context_window),
        is_primary: Set(item.is_primary),
        available_models: Set(item.available_models.clone()),
        allowed_models: Set(item.allowed_models.clone()),
        image_input_models: Set(item.image_input_models.clone()),
        price_in_per_mtok: Set(item.price_in_per_mtok),
        price_out_per_mtok: Set(item.price_out_per_mtok),
        price_cached_per_mtok: Set(item.price_cached_per_mtok),
        currency: Set(item.currency.clone()),
        is_deleted: Set(false),
        is_posted: Set(false),
        created_at: Set(Some(item.base.metadata.created_at)),
        updated_at: Set(Some(now)),
        version: Set(item.base.metadata.version + 1),
    };

    Entity::update(active).exec(conn()).await?;
    Ok(())
}

pub async fn soft_delete(id: &str) -> anyhow::Result<()> {
    let now = Utc::now();
    Entity::update_many()
        .col_expr(Column::IsDeleted, Expr::value(true))
        .col_expr(Column::UpdatedAt, Expr::value(Some(now)))
        .filter(Column::Id.eq(id))
        .exec(conn())
        .await?;
    Ok(())
}

/// Снять флаг is_primary со всех подключений
pub async fn clear_all_primary() -> anyhow::Result<()> {
    Entity::update_many()
        .col_expr(Column::IsPrimary, Expr::value(false))
        .exec(conn())
        .await?;
    Ok(())
}
