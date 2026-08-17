use crate::shared::error::ApiError;
use axum::{extract::Path, Json};
use serde_json::json;

use crate::domain::a005_marketplace;

/// GET /api/marketplace
pub async fn list_all(
) -> Result<Json<Vec<contracts::domain::a005_marketplace::aggregate::Marketplace>>, ApiError> {
    match a005_marketplace::service::list_all().await {
        Ok(v) => Ok(Json(v)),
        Err(_) => Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into()),
    }
}

/// GET /api/marketplace/:id
pub async fn get_by_id(
    Path(id): Path<String>,
) -> Result<Json<contracts::domain::a005_marketplace::aggregate::Marketplace>, ApiError> {
    let uuid = match uuid::Uuid::parse_str(&id) {
        Ok(uuid) => uuid,
        Err(_) => return Err(axum::http::StatusCode::BAD_REQUEST.into()),
    };
    match a005_marketplace::service::get_by_id(uuid).await {
        Ok(Some(v)) => Ok(Json(v)),
        Ok(None) => Err(axum::http::StatusCode::NOT_FOUND.into()),
        Err(_) => Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into()),
    }
}

/// POST /api/marketplace
pub async fn upsert(
    Json(dto): Json<contracts::domain::a005_marketplace::aggregate::MarketplaceDto>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let result = if dto.id.is_some() {
        a005_marketplace::service::update(dto)
            .await
            .map(|_| uuid::Uuid::nil().to_string())
    } else {
        a005_marketplace::service::create(dto)
            .await
            .map(|id| id.to_string())
    };
    match result {
        Ok(id) => Ok(Json(json!({"id": id}))),
        Err(_) => Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into()),
    }
}

/// DELETE /api/marketplace/:id
pub async fn delete(Path(id): Path<String>) -> Result<(), ApiError> {
    let uuid = match uuid::Uuid::parse_str(&id) {
        Ok(uuid) => uuid,
        Err(_) => return Err(axum::http::StatusCode::BAD_REQUEST.into()),
    };
    match a005_marketplace::service::delete(uuid).await {
        Ok(true) => Ok(()),
        Ok(false) => Err(axum::http::StatusCode::NOT_FOUND.into()),
        Err(_) => Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into()),
    }
}

/// POST /api/marketplace/testdata
pub async fn insert_test_data() -> axum::http::StatusCode {
    match a005_marketplace::service::insert_test_data().await {
        Ok(_) => axum::http::StatusCode::OK,
        Err(_) => axum::http::StatusCode::INTERNAL_SERVER_ERROR,
    }
}
