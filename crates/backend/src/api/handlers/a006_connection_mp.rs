use crate::shared::error::ApiError;
use axum::{extract::Path, Json};
use serde_json::json;

use crate::domain::a006_connection_mp;

/// GET /api/connection_mp
pub async fn list_all(
) -> Result<Json<Vec<contracts::domain::a006_connection_mp::aggregate::ConnectionMP>>, ApiError> {
    match a006_connection_mp::service::list_all().await {
        Ok(v) => Ok(Json(v)),
        Err(_) => Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into()),
    }
}

/// GET /api/connection_mp/:id
pub async fn get_by_id(
    Path(id): Path<String>,
) -> Result<Json<contracts::domain::a006_connection_mp::aggregate::ConnectionMP>, ApiError> {
    let uuid = match uuid::Uuid::parse_str(&id) {
        Ok(uuid) => uuid,
        Err(_) => return Err(axum::http::StatusCode::BAD_REQUEST.into()),
    };
    match a006_connection_mp::service::get_by_id(uuid).await {
        Ok(Some(v)) => Ok(Json(v)),
        Ok(None) => Err(axum::http::StatusCode::NOT_FOUND.into()),
        Err(_) => Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into()),
    }
}

/// POST /api/connection_mp
pub async fn upsert(
    Json(dto): Json<contracts::domain::a006_connection_mp::aggregate::ConnectionMPDto>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let result = if dto.id.is_some() {
        a006_connection_mp::service::update(dto)
            .await
            .map(|_| uuid::Uuid::nil().to_string())
    } else {
        a006_connection_mp::service::create(dto)
            .await
            .map(|id| id.to_string())
    };
    match result {
        Ok(id) => Ok(Json(json!({"id": id}))),
        Err(e) => {
            tracing::error!("Failed to save connection_mp: {}", e);
            Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into())
        }
    }
}

/// DELETE /api/connection_mp/:id
pub async fn delete(Path(id): Path<String>) -> Result<(), ApiError> {
    let uuid = match uuid::Uuid::parse_str(&id) {
        Ok(uuid) => uuid,
        Err(_) => return Err(axum::http::StatusCode::BAD_REQUEST.into()),
    };
    match a006_connection_mp::service::delete(uuid).await {
        Ok(true) => Ok(()),
        Ok(false) => Err(axum::http::StatusCode::NOT_FOUND.into()),
        Err(_) => Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into()),
    }
}

/// POST /api/connection_mp/test
pub async fn test_connection(
    Json(dto): Json<contracts::domain::a006_connection_mp::aggregate::ConnectionMPDto>,
) -> Result<Json<contracts::domain::a006_connection_mp::aggregate::ConnectionTestResult>, ApiError>
{
    match a006_connection_mp::service::test_connection(dto).await {
        Ok(result) => Ok(Json(result)),
        Err(_) => Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into()),
    }
}

/// POST /api/connection_mp/seller_info
pub async fn seller_info(
    Json(dto): Json<contracts::domain::a006_connection_mp::aggregate::ConnectionMPDto>,
) -> Result<Json<contracts::domain::a006_connection_mp::aggregate::ConnectionTestResult>, ApiError>
{
    match a006_connection_mp::service::seller_info(dto).await {
        Ok(result) => Ok(Json(result)),
        Err(_) => Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into()),
    }
}
