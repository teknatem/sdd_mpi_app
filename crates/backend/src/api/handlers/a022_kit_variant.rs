use crate::shared::error::ApiError;
use axum::{
    extract::{Path, Query},
    Json,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::domain::a022_kit_variant;
use crate::domain::a022_kit_variant::repository::{KitVariantListQuery, KitVariantListRow};

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    pub limit: Option<usize>,
    pub offset: Option<usize>,
    pub search_query: Option<String>,
    pub sort_by: Option<String>,
    pub sort_desc: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct KitVariantListItemDto {
    pub id: String,
    pub code: String,
    pub description: String,
    pub owner_ref: Option<String>,
    pub owner_description: Option<String>,
    pub owner_article: Option<String>,
    pub goods_json: Option<String>,
    pub connection_id: String,
    pub fetched_at: String,
}

impl From<KitVariantListRow> for KitVariantListItemDto {
    fn from(r: KitVariantListRow) -> Self {
        Self {
            id: r.id,
            code: r.code,
            description: r.description,
            owner_ref: r.owner_ref,
            owner_description: r.owner_description,
            owner_article: r.owner_article,
            goods_json: r.goods_json,
            connection_id: r.connection_id,
            fetched_at: r.fetched_at,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct PaginatedResponse {
    pub items: Vec<KitVariantListItemDto>,
    pub total: usize,
    pub page: usize,
    pub page_size: usize,
    pub total_pages: usize,
}

/// GET /api/a022/kit-variant/list
pub async fn list_paginated(
    Query(query): Query<ListQuery>,
) -> Result<Json<PaginatedResponse>, ApiError> {
    let page_size = query.limit.unwrap_or(100);
    let offset = query.offset.unwrap_or(0);
    let page = if page_size > 0 { offset / page_size } else { 0 };

    let list_query = KitVariantListQuery {
        search_query: query.search_query,
        sort_by: query.sort_by.unwrap_or_else(|| "description".to_string()),
        sort_desc: query.sort_desc.unwrap_or(false),
        limit: page_size,
        offset,
    };

    match a022_kit_variant::service::list_paginated(list_query).await {
        Ok(result) => {
            let total_pages = if page_size > 0 {
                (result.total + page_size - 1) / page_size
            } else {
                1
            };
            Ok(Json(PaginatedResponse {
                items: result.items.into_iter().map(|r| r.into()).collect(),
                total: result.total,
                page,
                page_size,
                total_pages,
            }))
        }
        Err(e) => {
            tracing::error!("Failed to list kit variants: {}", e);
            Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into())
        }
    }
}

/// GET /api/a022/kit-variant/:id
pub async fn get_by_id(
    Path(id): Path<String>,
) -> Result<Json<contracts::domain::a022_kit_variant::aggregate::KitVariant>, ApiError> {
    let uuid = Uuid::parse_str(&id).map_err(|_| axum::http::StatusCode::BAD_REQUEST)?;
    match a022_kit_variant::service::get_by_id(uuid).await {
        Ok(Some(item)) => Ok(Json(item)),
        Ok(None) => Err(axum::http::StatusCode::NOT_FOUND.into()),
        Err(e) => {
            tracing::error!("Failed to get kit variant {}: {}", id, e);
            Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into())
        }
    }
}
