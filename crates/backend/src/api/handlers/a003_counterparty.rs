use crate::shared::error::ApiError;
use axum::{extract::Path, Json};
use serde_json::json;

use crate::domain::a003_counterparty;

/// GET /api/counterparty
pub async fn list_all(
) -> Result<Json<Vec<contracts::domain::a003_counterparty::aggregate::Counterparty>>, ApiError> {
    match a003_counterparty::service::list_all().await {
        Ok(v) => Ok(Json(v)),
        Err(_) => Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into()),
    }
}

/// GET /api/counterparty/:id
pub async fn get_by_id(
    Path(id): Path<String>,
) -> Result<Json<contracts::domain::a003_counterparty::aggregate::Counterparty>, ApiError> {
    let uuid = match uuid::Uuid::parse_str(&id) {
        Ok(uuid) => uuid,
        Err(_) => return Err(axum::http::StatusCode::BAD_REQUEST.into()),
    };
    match a003_counterparty::service::get_by_id(uuid).await {
        Ok(Some(v)) => Ok(Json(v)),
        Ok(None) => Err(axum::http::StatusCode::NOT_FOUND.into()),
        Err(_) => Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into()),
    }
}

/// POST /api/counterparty
pub async fn upsert(
    Json(dto): Json<contracts::domain::a003_counterparty::aggregate::CounterpartyDto>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let result = if dto.id.is_some() {
        a003_counterparty::service::update(dto)
            .await
            .map(|_| uuid::Uuid::nil().to_string())
    } else {
        a003_counterparty::service::create(dto)
            .await
            .map(|id| id.to_string())
    };
    match result {
        Ok(id) => Ok(Json(json!({"id": id}))),
        Err(_) => Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into()),
    }
}

/// DELETE /api/counterparty/:id
pub async fn delete(Path(id): Path<String>) -> Result<(), ApiError> {
    let uuid = match uuid::Uuid::parse_str(&id) {
        Ok(uuid) => uuid,
        Err(_) => return Err(axum::http::StatusCode::BAD_REQUEST.into()),
    };
    match a003_counterparty::service::delete(uuid).await {
        Ok(true) => Ok(()),
        Ok(false) => Err(axum::http::StatusCode::NOT_FOUND.into()),
        Err(_) => Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into()),
    }
}
