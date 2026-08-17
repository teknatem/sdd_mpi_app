use crate::shared::error::ApiError;
use axum::{extract::Path, extract::Query, Json};
use contracts::domain::a007_marketplace_product::aggregate::MarketplaceProductListItemDto;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::domain::a007_marketplace_product;

#[derive(Debug, Clone, Serialize)]
pub struct PaginatedMarketplaceProductResponse {
    pub items: Vec<MarketplaceProductListItemDto>,
    pub total: usize,
    pub page: usize,
    pub page_size: usize,
    pub total_pages: usize,
}

#[derive(Debug, Deserialize)]
pub struct ListMarketplaceProductsQuery {
    pub marketplace_ref: Option<String>,
    pub connection_mp_ref: Option<String>,
    pub problems_only: Option<bool>,
    pub search: Option<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
    pub sort_by: Option<String>,
    pub sort_desc: Option<bool>,
}

/// GET /api/a007/marketplace-product
pub async fn list_paginated(
    Query(query): Query<ListMarketplaceProductsQuery>,
) -> Result<Json<PaginatedMarketplaceProductResponse>, ApiError> {
    use a007_marketplace_product::repository::MarketplaceProductListQuery;

    let limit = query.limit.unwrap_or(100);
    let offset = query.offset.unwrap_or(0);
    let page = if limit > 0 { offset / limit } else { 0 };

    let list_query = MarketplaceProductListQuery {
        marketplace_ref: query.marketplace_ref,
        connection_mp_ref: query.connection_mp_ref,
        problems_only: query.problems_only.unwrap_or(false),
        search: query.search,
        sort_by: query.sort_by.unwrap_or_else(|| "code".to_string()),
        sort_desc: query.sort_desc.unwrap_or(false),
        limit,
        offset,
    };

    match a007_marketplace_product::service::list_paginated(list_query).await {
        Ok(result) => {
            let total_pages = if limit > 0 {
                (result.total + limit - 1) / limit
            } else {
                0
            };

            let items = result
                .items
                .into_iter()
                .map(|p| MarketplaceProductListItemDto {
                    id: p.base.id.0.to_string(),
                    code: p.base.code,
                    description: p.base.description,
                    marketplace_ref: p.marketplace_ref,
                    connection_mp_ref: p.connection_mp_ref,
                    marketplace_sku: p.marketplace_sku,
                    barcode: p.barcode,
                    article: p.article,
                    nomenclature_ref: p.nomenclature_ref,
                    is_posted: p.base.metadata.is_posted,
                    created_at: p.base.metadata.created_at.to_rfc3339(),
                })
                .collect();

            Ok(Json(PaginatedMarketplaceProductResponse {
                items,
                total: result.total,
                page,
                page_size: limit,
                total_pages,
            }))
        }
        Err(e) => {
            tracing::error!("Failed to list marketplace products: {}", e);
            Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into())
        }
    }
}

/// GET /api/marketplace_product
pub async fn list_all() -> Result<
    Json<Vec<contracts::domain::a007_marketplace_product::aggregate::MarketplaceProduct>>,
    ApiError,
> {
    match a007_marketplace_product::service::list_all().await {
        Ok(v) => Ok(Json(v)),
        Err(_) => Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into()),
    }
}

/// GET /api/marketplace_product/:id
pub async fn get_by_id(
    Path(id): Path<String>,
) -> Result<
    Json<contracts::domain::a007_marketplace_product::aggregate::MarketplaceProduct>,
    ApiError,
> {
    let uuid = match uuid::Uuid::parse_str(&id) {
        Ok(uuid) => uuid,
        Err(_) => return Err(axum::http::StatusCode::BAD_REQUEST.into()),
    };
    match a007_marketplace_product::service::get_by_id(uuid).await {
        Ok(Some(v)) => Ok(Json(v)),
        Ok(None) => Err(axum::http::StatusCode::NOT_FOUND.into()),
        Err(_) => Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into()),
    }
}

/// POST /api/marketplace_product
pub async fn upsert(
    Json(dto): Json<contracts::domain::a007_marketplace_product::aggregate::MarketplaceProductDto>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let result = if dto.id.is_some() {
        a007_marketplace_product::service::update(dto)
            .await
            .map(|_| uuid::Uuid::nil().to_string())
    } else {
        a007_marketplace_product::service::create(dto)
            .await
            .map(|id| id.to_string())
    };
    match result {
        Ok(id) => Ok(Json(json!({"id": id}))),
        Err(e) => {
            tracing::error!("Failed to save marketplace_product: {}", e);
            Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into())
        }
    }
}

/// DELETE /api/marketplace_product/:id
pub async fn delete(Path(id): Path<String>) -> Result<(), ApiError> {
    let uuid = match uuid::Uuid::parse_str(&id) {
        Ok(uuid) => uuid,
        Err(_) => return Err(axum::http::StatusCode::BAD_REQUEST.into()),
    };
    match a007_marketplace_product::service::delete(uuid).await {
        Ok(true) => Ok(()),
        Ok(false) => Err(axum::http::StatusCode::NOT_FOUND.into()),
        Err(_) => Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into()),
    }
}

/// POST /api/marketplace_product/testdata
pub async fn insert_test_data() -> axum::http::StatusCode {
    match a007_marketplace_product::service::insert_test_data().await {
        Ok(_) => axum::http::StatusCode::OK,
        Err(e) => {
            tracing::error!("Failed to insert test data: {}", e);
            axum::http::StatusCode::INTERNAL_SERVER_ERROR
        }
    }
}
