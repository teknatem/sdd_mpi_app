use crate::shared::error::ApiError;
use axum::{
    extract::{Path, Query},
    Json,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::domain::a020_wb_promotion;
use crate::domain::a020_wb_promotion::repository::{WbPromotionListQuery, WbPromotionListRow};
use crate::shared::data::raw_storage;
use contracts::domain::a020_wb_promotion::aggregate::WbPromotion;

#[derive(Debug, Deserialize)]
pub struct ListPromotionsQuery {
    pub date_from: Option<String>,
    pub date_to: Option<String>,
    pub connection_id: Option<String>,
    pub search_query: Option<String>,
    pub sort_by: Option<String>,
    pub sort_desc: Option<bool>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

#[derive(Debug, Serialize)]
pub struct PaginatedPromotionsResponse {
    pub items: Vec<WbPromotionListRow>,
    pub total: usize,
    pub page: usize,
    pub page_size: usize,
    pub total_pages: usize,
}

/// GET /api/a020/wb-promotions — список акций с пагинацией
pub async fn list_promotions(
    Query(query): Query<ListPromotionsQuery>,
) -> Result<Json<PaginatedPromotionsResponse>, ApiError> {
    let page_size = query.limit.unwrap_or(100);
    let offset = query.offset.unwrap_or(0);
    let page = if page_size > 0 { offset / page_size } else { 0 };
    let sort_by = query
        .sort_by
        .clone()
        .unwrap_or_else(|| "start_date_time".to_string());
    let sort_desc = query.sort_desc.unwrap_or(true);

    let list_query = WbPromotionListQuery {
        date_from: query.date_from.clone(),
        date_to: query.date_to.clone(),
        connection_id: query.connection_id.clone(),
        search_query: query.search_query.clone(),
        sort_by,
        sort_desc,
        limit: page_size,
        offset,
    };

    let (items, total) = a020_wb_promotion::repository::list_sql(list_query)
        .await
        .map_err(|e| {
            tracing::error!("Failed to list WB promotions: {}", e);
            ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        })?;

    let total_pages = if page_size > 0 {
        (total + page_size - 1) / page_size
    } else {
        1
    };

    Ok(Json(PaginatedPromotionsResponse {
        items,
        total,
        page,
        page_size,
        total_pages,
    }))
}

/// GET /api/a020/wb-promotions/:id — детали акции
pub async fn get_promotion_detail(Path(id): Path<String>) -> Result<Json<WbPromotion>, ApiError> {
    let uuid = Uuid::parse_str(&id).map_err(|_| axum::http::StatusCode::BAD_REQUEST)?;

    let promotion = a020_wb_promotion::service::get_by_id(uuid)
        .await
        .map_err(|e| {
            tracing::error!("Failed to get WB promotion {}: {}", id, e);
            ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        })?;

    promotion
        .map(Json)
        .ok_or(ApiError::from(axum::http::StatusCode::NOT_FOUND))
}

/// POST /api/a020/wb-promotions/:id/post
pub async fn post_promotion(Path(id): Path<String>) -> Result<axum::http::StatusCode, ApiError> {
    let uuid = Uuid::parse_str(&id).map_err(|_| axum::http::StatusCode::BAD_REQUEST)?;
    a020_wb_promotion::service::post(uuid).await.map_err(|e| {
        tracing::error!("Failed to post WB promotion {}: {}", id, e);
        ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
    })?;
    Ok(axum::http::StatusCode::OK)
}

/// POST /api/a020/wb-promotions/:id/unpost
pub async fn unpost_promotion(Path(id): Path<String>) -> Result<axum::http::StatusCode, ApiError> {
    let uuid = Uuid::parse_str(&id).map_err(|_| axum::http::StatusCode::BAD_REQUEST)?;
    a020_wb_promotion::service::unpost(uuid)
        .await
        .map_err(|e| {
            tracing::error!("Failed to unpost WB promotion {}: {}", id, e);
            ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        })?;
    Ok(axum::http::StatusCode::OK)
}

/// GET /api/a020/raw/:ref_id — сырой JSON
pub async fn get_raw_json(Path(ref_id): Path<String>) -> Result<Json<serde_json::Value>, ApiError> {
    let json_value = raw_storage::get_json_value_by_ref(&ref_id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to get raw JSON: {}", e);
            ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        })?;

    Ok(Json(json_value))
}
