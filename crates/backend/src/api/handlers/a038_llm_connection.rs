use crate::shared::error::ApiError;
use axum::{
    extract::{Path, Query},
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::domain::a038_llm_connection;
use contracts::domain::a038_llm_connection::aggregate::LlmConnection;

#[derive(Deserialize)]
pub struct LlmConnectionListParams {
    pub limit: Option<u64>,
    pub offset: Option<u64>,
    pub sort_by: Option<String>,
    pub sort_desc: Option<bool>,
}

#[derive(Serialize)]
pub struct LlmConnectionPaginatedResponse {
    pub items: Vec<LlmConnection>,
    pub total: u64,
    pub page: usize,
    pub page_size: usize,
    pub total_pages: usize,
}

pub async fn list_all() -> Result<Json<Vec<LlmConnection>>, ApiError> {
    match a038_llm_connection::service::list_all().await {
        Ok(v) => Ok(Json(v)),
        Err(_) => Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into()),
    }
}

pub async fn list_paginated(
    Query(params): Query<LlmConnectionListParams>,
) -> Result<Json<LlmConnectionPaginatedResponse>, ApiError> {
    let limit = params.limit.unwrap_or(100).clamp(10, 10000);
    let offset = params.offset.unwrap_or(0);
    let sort_by = params.sort_by.as_deref().unwrap_or("description");
    let sort_desc = params.sort_desc.unwrap_or(false);

    match a038_llm_connection::service::list_paginated(limit, offset, sort_by, sort_desc).await {
        Ok((items, total)) => {
            let page_size = limit as usize;
            let page = (offset as usize) / page_size;
            let total_pages = ((total as usize) + page_size - 1) / page_size;

            Ok(Json(LlmConnectionPaginatedResponse {
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

pub async fn get_by_id(Path(id): Path<String>) -> Result<Json<LlmConnection>, ApiError> {
    match a038_llm_connection::service::get_by_id(&id).await {
        Ok(Some(v)) => Ok(Json(v)),
        Ok(None) => Err(axum::http::StatusCode::NOT_FOUND.into()),
        Err(_) => Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into()),
    }
}

pub async fn delete(Path(id): Path<String>) -> Result<(), ApiError> {
    match a038_llm_connection::service::delete(&id).await {
        Ok(()) => Ok(()),
        Err(_) => Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into()),
    }
}

pub async fn upsert(
    Json(dto): Json<a038_llm_connection::service::LlmConnectionDto>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if dto.id.is_some() {
        match a038_llm_connection::service::update(dto).await {
            Ok(_) => Ok(Json(json!({"success": true}))),
            Err(e) => {
                tracing::error!("Failed to update LLM connection: {}", e);
                Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into())
            }
        }
    } else {
        match a038_llm_connection::service::create(dto).await {
            Ok(id) => Ok(Json(json!({"success": true, "id": id.to_string()}))),
            Err(e) => {
                tracing::error!("Failed to create LLM connection: {}", e);
                Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into())
            }
        }
    }
}

pub async fn get_primary() -> Result<Json<LlmConnection>, ApiError> {
    match a038_llm_connection::service::get_primary().await {
        Ok(Some(v)) => Ok(Json(v)),
        Ok(None) => Err(axum::http::StatusCode::NOT_FOUND.into()),
        Err(_) => Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into()),
    }
}

pub async fn test_connection(Path(id): Path<String>) -> Result<Json<serde_json::Value>, ApiError> {
    use crate::shared::llm::provider_factory;

    let connection = match a038_llm_connection::service::get_by_id(&id).await {
        Ok(Some(v)) => v,
        Ok(None) => return Err(axum::http::StatusCode::NOT_FOUND.into()),
        Err(_) => return Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into()),
    };

    let provider = match provider_factory::create_provider(&connection, None) {
        Ok(provider) => provider,
        Err(e) => {
            return Ok(Json(json!({
                "success": false,
                "message": format!("Connection failed: {}", e),
                "provider": connection.provider_type.as_str(),
                "model": connection.model_name
            })));
        }
    };

    match provider.test_connection().await {
        Ok(()) => Ok(Json(json!({
            "success": true,
            "message": format!(
                "Successfully connected to {} ({})",
                connection.model_name,
                connection.provider_type.as_str()
            ),
            "provider": connection.provider_type.as_str(),
            "model": connection.model_name
        }))),
        Err(e) => Ok(Json(json!({
            "success": false,
            "message": format!("Connection failed: {}", e),
            "provider": connection.provider_type.as_str(),
            "model": connection.model_name
        }))),
    }
}

pub async fn fetch_models(Path(id): Path<String>) -> Result<Json<serde_json::Value>, ApiError> {
    use crate::shared::llm::provider_factory;

    let connection = match a038_llm_connection::service::get_by_id(&id).await {
        Ok(Some(v)) => v,
        Ok(None) => return Err(axum::http::StatusCode::NOT_FOUND.into()),
        Err(_) => return Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into()),
    };

    match provider_factory::list_models(&connection).await {
        Ok(model_list) => {
            let json_str = serde_json::to_string(&model_list).unwrap_or_default();
            let mut updated = connection.clone();
            updated.available_models = Some(json_str);
            updated.before_write();

            if let Err(e) = a038_llm_connection::repository::update(&updated).await {
                tracing::error!("Failed to save models: {}", e);
                return Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into());
            }

            Ok(Json(json!({
                "success": true,
                "models": model_list,
                "count": model_list.len(),
                "message": format!("Loaded {} models", model_list.len())
            })))
        }
        Err(e) => Ok(Json(json!({
            "success": false,
            "message": format!("Failed to fetch models: {}", e),
            "models": [],
            "count": 0
        }))),
    }
}
