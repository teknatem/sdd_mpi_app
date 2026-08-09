use axum::{
    extract::{Path, Query},
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::domain::a042_agent_task;
use contracts::domain::a042_agent_task::aggregate::AgentTask;

#[derive(Deserialize)]
pub struct AgentTaskListParams {
    pub limit: Option<u64>,
    pub offset: Option<u64>,
    pub sort_by: Option<String>,
    pub sort_desc: Option<bool>,
    pub status: Option<String>,
    pub target_agent_type: Option<String>,
    pub q: Option<String>,
}

#[derive(Serialize)]
pub struct AgentTaskPaginatedResponse {
    pub items: Vec<AgentTask>,
    pub total: u64,
    pub page: usize,
    pub page_size: usize,
    pub total_pages: usize,
}

pub async fn list_paginated(
    Query(params): Query<AgentTaskListParams>,
) -> Result<Json<AgentTaskPaginatedResponse>, axum::http::StatusCode> {
    let limit = params.limit.unwrap_or(100).clamp(10, 1000);
    let offset = params.offset.unwrap_or(0);
    let sort_by = params.sort_by.as_deref().unwrap_or("created_at");
    let sort_desc = params.sort_desc.unwrap_or(true);

    match a042_agent_task::service::list_paginated(
        limit,
        offset,
        sort_by,
        sort_desc,
        params.status.as_deref(),
        params.target_agent_type.as_deref(),
        params.q.as_deref(),
    )
    .await
    {
        Ok((items, total)) => {
            let page_size = limit as usize;
            let page = (offset as usize) / page_size;
            let total_pages = ((total as usize) + page_size - 1) / page_size;
            Ok(Json(AgentTaskPaginatedResponse {
                items,
                total,
                page,
                page_size,
                total_pages,
            }))
        }
        Err(e) => {
            tracing::error!("Failed to list agent tasks: {}", e);
            Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn get_by_id(Path(id): Path<String>) -> Result<Json<AgentTask>, axum::http::StatusCode> {
    match a042_agent_task::service::get_by_id(&id).await {
        Ok(Some(item)) => Ok(Json(item)),
        Ok(None) => Err(axum::http::StatusCode::NOT_FOUND),
        Err(e) => {
            tracing::error!("Failed to get agent task {}: {}", id, e);
            Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn delete(Path(id): Path<String>) -> Result<(), axum::http::StatusCode> {
    match a042_agent_task::service::delete(&id).await {
        Ok(()) => Ok(()),
        Err(e) => {
            tracing::error!("Failed to delete agent task {}: {}", id, e);
            Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Снять поручение. Недопустимый переход (например отмена уже готового) —
/// это ошибка вызывающего, а не сервера: отдаём 409 с текстом причины.
pub async fn cancel(
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, String)> {
    match a042_agent_task::service::cancel(&id).await {
        Ok(()) => Ok(Json(json!({"success": true}))),
        Err(e) => {
            tracing::error!("Failed to cancel agent task {}: {}", id, e);
            Err((axum::http::StatusCode::CONFLICT, e.to_string()))
        }
    }
}

/// Вернуть поручение в очередь (сброс попыток и паузы).
pub async fn requeue(
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, String)> {
    match a042_agent_task::service::requeue(&id).await {
        Ok(()) => Ok(Json(json!({"success": true}))),
        Err(e) => {
            tracing::error!("Failed to requeue agent task {}: {}", id, e);
            Err((axum::http::StatusCode::CONFLICT, e.to_string()))
        }
    }
}
