use crate::shared::error::ApiError;
use axum::{
    extract::{Path, Query},
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::domain::a025_bi_dashboard;
use contracts::domain::a025_bi_dashboard::aggregate::BiDashboard;

#[derive(Deserialize)]
pub struct BiDashboardListParams {
    pub limit: Option<u64>,
    pub offset: Option<u64>,
    pub sort_by: Option<String>,
    pub sort_desc: Option<bool>,
    pub q: Option<String>,
}

#[derive(Serialize)]
pub struct BiDashboardPaginatedResponse {
    pub items: Vec<BiDashboard>,
    pub total: u64,
    pub page: usize,
    pub page_size: usize,
    pub total_pages: usize,
}

/// GET /api/a025-bi-dashboard
pub async fn list_all() -> Result<Json<Vec<BiDashboard>>, ApiError> {
    match a025_bi_dashboard::service::list_all().await {
        Ok(v) => Ok(Json(v)),
        Err(_) => Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into()),
    }
}

/// GET /api/a025-bi-dashboard/list
pub async fn list_paginated(
    Query(params): Query<BiDashboardListParams>,
) -> Result<Json<BiDashboardPaginatedResponse>, ApiError> {
    let limit = params.limit.unwrap_or(100).clamp(10, 10000);
    let offset = params.offset.unwrap_or(0);
    let page = offset / limit;
    let sort_by = params.sort_by.as_deref().unwrap_or("created_at");
    let sort_desc = params.sort_desc.unwrap_or(true);
    let q = params.q.as_deref();

    match a025_bi_dashboard::service::list_paginated(page, limit, sort_by, sort_desc, q).await {
        Ok((items, total)) => {
            let page_size = limit as usize;
            let page_num = (offset as usize) / page_size;
            let total_pages = ((total as usize) + page_size - 1) / page_size;

            Ok(Json(BiDashboardPaginatedResponse {
                items,
                total,
                page: page_num,
                page_size,
                total_pages,
            }))
        }
        Err(_) => Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into()),
    }
}

/// GET /api/a025-bi-dashboard/owner/:user_id
pub async fn list_by_owner(
    Path(user_id): Path<String>,
) -> Result<Json<Vec<BiDashboard>>, ApiError> {
    match a025_bi_dashboard::service::list_by_owner(&user_id).await {
        Ok(v) => Ok(Json(v)),
        Err(_) => Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into()),
    }
}

/// GET /api/a025-bi-dashboard/public
pub async fn list_public() -> Result<Json<Vec<BiDashboard>>, ApiError> {
    match a025_bi_dashboard::service::list_public().await {
        Ok(v) => Ok(Json(v)),
        Err(_) => Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into()),
    }
}

/// GET /api/a025-bi-dashboard/:id
pub async fn get_by_id(Path(id): Path<String>) -> Result<Json<BiDashboard>, ApiError> {
    match a025_bi_dashboard::service::get_by_id(&id).await {
        Ok(Some(v)) => Ok(Json(v)),
        Ok(None) => Err(axum::http::StatusCode::NOT_FOUND.into()),
        Err(_) => Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into()),
    }
}

/// DELETE /api/a025-bi-dashboard/:id
pub async fn delete(Path(id): Path<String>) -> Result<(), ApiError> {
    match a025_bi_dashboard::service::delete(&id).await {
        Ok(()) => Ok(()),
        Err(_) => Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into()),
    }
}

/// POST /api/a025-bi-dashboard (upsert)
pub async fn upsert(
    Json(dto): Json<a025_bi_dashboard::service::BiDashboardDto>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if dto.id.is_some() {
        match a025_bi_dashboard::service::update(dto).await {
            Ok(_) => Ok(Json(json!({"success": true}))),
            Err(e) => {
                tracing::error!("Failed to update BI dashboard: {}", e);
                Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into())
            }
        }
    } else {
        match a025_bi_dashboard::service::create(dto).await {
            Ok(id) => Ok(Json(json!({"success": true, "id": id.to_string()}))),
            Err(e) => {
                tracing::error!("Failed to create BI dashboard: {}", e);
                Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into())
            }
        }
    }
}

/// POST /api/a025-bi-dashboard/testdata
pub async fn insert_test_data() -> axum::http::StatusCode {
    match a025_bi_dashboard::service::insert_test_data().await {
        Ok(_) => axum::http::StatusCode::OK,
        Err(e) => {
            tracing::error!("Failed to insert BI dashboard test data: {}", e);
            axum::http::StatusCode::INTERNAL_SERVER_ERROR
        }
    }
}
