use crate::shared::error::ApiError;
use axum::{
    extract::{Path, Query},
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::domain::a031_kb_edit;
use contracts::domain::a031_kb_edit::aggregate::KbEdit;

#[derive(Deserialize)]
pub struct KbEditListParams {
    pub limit: Option<u64>,
    pub offset: Option<u64>,
    pub sort_by: Option<String>,
    pub sort_desc: Option<bool>,
    pub status: Option<String>,
    pub q: Option<String>,
}

#[derive(Serialize)]
pub struct KbEditPaginatedResponse {
    pub items: Vec<KbEdit>,
    pub total: u64,
    pub page: usize,
    pub page_size: usize,
    pub total_pages: usize,
}

pub async fn list_paginated(
    Query(params): Query<KbEditListParams>,
) -> Result<Json<KbEditPaginatedResponse>, ApiError> {
    let limit = params.limit.unwrap_or(100).clamp(10, 1000);
    let offset = params.offset.unwrap_or(0);
    let page = offset / limit;
    let sort_by = params.sort_by.as_deref().unwrap_or("created_at");
    let sort_desc = params.sort_desc.unwrap_or(true);

    match a031_kb_edit::service::list_paginated(
        page,
        limit,
        sort_by,
        sort_desc,
        params.status.as_deref(),
        params.q.as_deref(),
    )
    .await
    {
        Ok((items, total)) => {
            let page_size = limit as usize;
            let page_num = (offset as usize) / page_size;
            let total_pages = ((total as usize) + page_size - 1) / page_size;
            Ok(Json(KbEditPaginatedResponse {
                items,
                total,
                page: page_num,
                page_size,
                total_pages,
            }))
        }
        Err(e) => {
            tracing::error!("Failed to list KB edits: {}", e);
            Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into())
        }
    }
}

pub async fn get_by_id(Path(id): Path<String>) -> Result<Json<KbEdit>, ApiError> {
    match a031_kb_edit::service::get_by_id(&id).await {
        Ok(Some(item)) => Ok(Json(item)),
        Ok(None) => Err(axum::http::StatusCode::NOT_FOUND.into()),
        Err(e) => {
            tracing::error!("Failed to get KB edit {}: {}", id, e);
            Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into())
        }
    }
}

pub async fn upsert(
    Json(dto): Json<a031_kb_edit::service::KbEditDto>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if dto.id.is_some() {
        match a031_kb_edit::service::update(dto).await {
            Ok(()) => Ok(Json(json!({"success": true}))),
            Err(e) => {
                tracing::error!("Failed to update KB edit: {}", e);
                Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into())
            }
        }
    } else {
        match a031_kb_edit::service::create(dto).await {
            Ok(id) => Ok(Json(json!({"success": true, "id": id.to_string()}))),
            Err(e) => {
                tracing::error!("Failed to create KB edit: {}", e);
                Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into())
            }
        }
    }
}

pub async fn delete(Path(id): Path<String>) -> Result<(), ApiError> {
    match a031_kb_edit::service::delete(&id).await {
        Ok(()) => Ok(()),
        Err(e) => {
            tracing::error!("Failed to delete KB edit {}: {}", id, e);
            Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into())
        }
    }
}

pub async fn approve(Path(id): Path<String>) -> Result<Json<serde_json::Value>, ApiError> {
    match a031_kb_edit::service::approve(&id).await {
        Ok(()) => Ok(Json(json!({"success": true}))),
        Err(e) => {
            tracing::error!("Failed to approve KB edit {}: {}", id, e);
            Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into())
        }
    }
}

pub async fn cancel(Path(id): Path<String>) -> Result<Json<serde_json::Value>, ApiError> {
    match a031_kb_edit::service::cancel(&id).await {
        Ok(()) => Ok(Json(json!({"success": true}))),
        Err(e) => {
            tracing::error!("Failed to cancel KB edit {}: {}", id, e);
            Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into())
        }
    }
}
