use crate::shared::error::ApiError;
use axum::{extract::Path, Json};
use serde_json::json;

use crate::domain::a008_marketplace_sales;

/// GET /api/marketplace_sales
pub async fn list_all() -> Result<
    Json<Vec<contracts::domain::a008_marketplace_sales::aggregate::MarketplaceSales>>,
    ApiError,
> {
    match a008_marketplace_sales::service::list_all().await {
        Ok(v) => Ok(Json(v)),
        Err(_) => Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into()),
    }
}

/// GET /api/marketplace_sales/:id
pub async fn get_by_id(
    Path(id): Path<String>,
) -> Result<Json<contracts::domain::a008_marketplace_sales::aggregate::MarketplaceSales>, ApiError>
{
    let uuid = match uuid::Uuid::parse_str(&id) {
        Ok(uuid) => uuid,
        Err(_) => return Err(axum::http::StatusCode::BAD_REQUEST.into()),
    };
    match a008_marketplace_sales::service::get_by_id(uuid).await {
        Ok(Some(v)) => Ok(Json(v)),
        Ok(None) => Err(axum::http::StatusCode::NOT_FOUND.into()),
        Err(_) => Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into()),
    }
}

/// POST /api/marketplace_sales
pub async fn upsert(
    Json(dto): Json<contracts::domain::a008_marketplace_sales::aggregate::MarketplaceSalesDto>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let result = if dto.id.is_some() {
        a008_marketplace_sales::service::update(dto)
            .await
            .map(|_| uuid::Uuid::nil().to_string())
    } else {
        a008_marketplace_sales::service::create(dto)
            .await
            .map(|id| id.to_string())
    };
    match result {
        Ok(id) => Ok(Json(json!({"id": id}))),
        Err(e) => {
            tracing::error!("Failed to save marketplace_sales: {}", e);
            Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into())
        }
    }
}

/// DELETE /api/marketplace_sales/:id
pub async fn delete(Path(id): Path<String>) -> Result<(), ApiError> {
    let uuid = match uuid::Uuid::parse_str(&id) {
        Ok(uuid) => uuid,
        Err(_) => return Err(axum::http::StatusCode::BAD_REQUEST.into()),
    };
    match a008_marketplace_sales::service::delete(uuid).await {
        Ok(true) => Ok(()),
        Ok(false) => Err(axum::http::StatusCode::NOT_FOUND.into()),
        Err(_) => Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into()),
    }
}
