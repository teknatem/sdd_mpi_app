use crate::shared::error::ApiError;
use axum::{
    extract::{Path, Query},
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::domain::a001_connection_1c;

#[derive(Deserialize)]
pub struct Connection1CListParams {
    pub limit: Option<u64>,
    pub offset: Option<u64>,
    pub sort_by: Option<String>,
    pub sort_desc: Option<bool>,
}

#[derive(Serialize)]
pub struct Connection1CPaginatedResponse {
    pub items: Vec<contracts::domain::a001_connection_1c::aggregate::Connection1CDatabase>,
    pub total: u64,
    pub page: usize,
    pub page_size: usize,
    pub total_pages: usize,
}

/// GET /api/connection_1c
pub async fn list_all() -> Result<
    Json<Vec<contracts::domain::a001_connection_1c::aggregate::Connection1CDatabase>>,
    ApiError,
> {
    match a001_connection_1c::service::list_all().await {
        Ok(v) => Ok(Json(v)),
        Err(_) => Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into()),
    }
}

/// GET /api/connection_1c/list
pub async fn list_paginated(
    Query(params): Query<Connection1CListParams>,
) -> Result<Json<Connection1CPaginatedResponse>, ApiError> {
    let limit = params.limit.unwrap_or(100).clamp(10, 10000);
    let offset = params.offset.unwrap_or(0);
    let sort_by = params.sort_by.as_deref().unwrap_or("description");
    let sort_desc = params.sort_desc.unwrap_or(false);

    match a001_connection_1c::service::list_paginated(limit, offset, sort_by, sort_desc).await {
        Ok((items, total)) => {
            let page_size = limit as usize;
            let page = (offset as usize) / page_size;
            let total_pages = ((total as usize) + page_size - 1) / page_size;

            Ok(Json(Connection1CPaginatedResponse {
                items,
                total,
                page,
                page_size,
                total_pages,
            }))
        }
        Err(_) => Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into()),
    }
}

/// GET /api/connection_1c/:id
pub async fn get_by_id(
    Path(id): Path<String>,
) -> Result<Json<contracts::domain::a001_connection_1c::aggregate::Connection1CDatabase>, ApiError>
{
    let uuid = match uuid::Uuid::parse_str(&id) {
        Ok(uuid) => uuid,
        Err(_) => return Err(axum::http::StatusCode::BAD_REQUEST.into()),
    };
    match a001_connection_1c::service::get_by_id(uuid).await {
        Ok(Some(v)) => Ok(Json(v)),
        Ok(None) => Err(axum::http::StatusCode::NOT_FOUND.into()),
        Err(_) => Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into()),
    }
}

/// DELETE /api/connection_1c/:id
pub async fn delete(Path(id): Path<String>) -> Result<(), ApiError> {
    let uuid = match uuid::Uuid::parse_str(&id) {
        Ok(uuid) => uuid,
        Err(_) => return Err(axum::http::StatusCode::BAD_REQUEST.into()),
    };

    match a001_connection_1c::service::delete(uuid).await {
        Ok(true) => Ok(()),
        Ok(false) => Err(axum::http::StatusCode::NOT_FOUND.into()),
        Err(_) => Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into()),
    }
}

/// POST /api/connection_1c
pub async fn upsert(
    Json(dto): Json<contracts::domain::a001_connection_1c::aggregate::Connection1CDatabaseDto>,
) -> Result<Json<serde_json::Value>, ApiError> {
    // Определяем операцию: create или update
    let result = if dto.id.is_some() {
        a001_connection_1c::service::update(dto)
            .await
            .map(|_| uuid::Uuid::nil().to_string())
    } else {
        a001_connection_1c::service::create(dto)
            .await
            .map(|id| id.to_string())
    };

    match result {
        Ok(id) => Ok(Json(json!({"id": id}))),
        Err(_) => Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into()),
    }
}

/// POST /api/connection_1c/test
pub async fn test_connection(
    Json(dto): Json<contracts::domain::a001_connection_1c::aggregate::Connection1CDatabaseDto>,
) -> Result<Json<contracts::domain::a001_connection_1c::aggregate::ConnectionTestResult>, ApiError>
{
    match a001_connection_1c::service::test_connection(dto).await {
        Ok(result) => Ok(Json(result)),
        Err(_) => Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into()),
    }
}

/// POST /api/connection_1c/testdata
pub async fn insert_test_data() -> axum::http::StatusCode {
    match a001_connection_1c::service::insert_test_data().await {
        Ok(_) => axum::http::StatusCode::OK,
        Err(_) => axum::http::StatusCode::INTERNAL_SERVER_ERROR,
    }
}
