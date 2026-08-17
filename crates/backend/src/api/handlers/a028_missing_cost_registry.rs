use crate::shared::error::ApiError;
use axum::{
    extract::{Path, Query},
    Json,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::domain::a028_missing_cost_registry;
use crate::domain::a028_missing_cost_registry::repository::{
    MissingCostRegistryListQuery, MissingCostRegistryListRow,
};
use contracts::domain::a028_missing_cost_registry::aggregate::{
    MissingCostRegistryLine, MissingCostRegistryUpdateDto,
};

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    pub date_from: Option<String>,
    pub date_to: Option<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
    pub search_query: Option<String>,
    pub sort_by: Option<String>,
    pub sort_desc: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct MissingCostRegistryListItemDto {
    pub id: String,
    pub document_no: String,
    pub document_date: String,
    pub updated_at: String,
    pub is_posted: bool,
    pub lines_total: usize,
    pub lines_filled: usize,
    pub lines_missing: usize,
}

fn parse_lines(lines_json: &Option<String>) -> Vec<MissingCostRegistryLine> {
    lines_json
        .as_deref()
        .and_then(|value| serde_json::from_str(value).ok())
        .unwrap_or_default()
}

impl From<MissingCostRegistryListRow> for MissingCostRegistryListItemDto {
    fn from(value: MissingCostRegistryListRow) -> Self {
        let lines = parse_lines(&value.lines_json);
        let lines_total = lines.len();
        let lines_filled = lines
            .iter()
            .filter(|line| line.cost.is_some_and(|cost| cost > 0.0))
            .count();

        Self {
            id: value.id,
            document_no: value.document_no,
            document_date: value.document_date,
            updated_at: value.updated_at,
            is_posted: value.is_posted,
            lines_total,
            lines_filled,
            lines_missing: lines_total.saturating_sub(lines_filled),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct PaginatedResponse {
    pub items: Vec<MissingCostRegistryListItemDto>,
    pub total: usize,
    pub page: usize,
    pub page_size: usize,
    pub total_pages: usize,
}

pub async fn list_paginated(
    Query(query): Query<ListQuery>,
) -> Result<Json<PaginatedResponse>, ApiError> {
    let page_size = query.limit.unwrap_or(100);
    let offset = query.offset.unwrap_or(0);
    let page = if page_size > 0 { offset / page_size } else { 0 };

    let list_query = MissingCostRegistryListQuery {
        date_from: query.date_from,
        date_to: query.date_to,
        search_query: query.search_query,
        sort_by: query.sort_by.unwrap_or_else(|| "document_date".to_string()),
        sort_desc: query.sort_desc.unwrap_or(true),
        limit: page_size,
        offset,
    };

    match a028_missing_cost_registry::service::list_paginated(list_query).await {
        Ok(result) => {
            let total_pages = if page_size > 0 {
                (result.total + page_size - 1) / page_size
            } else {
                1
            };
            Ok(Json(PaginatedResponse {
                items: result.items.into_iter().map(Into::into).collect(),
                total: result.total,
                page,
                page_size,
                total_pages,
            }))
        }
        Err(error) => {
            tracing::error!("Failed to list a028 missing cost registry: {}", error);
            Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into())
        }
    }
}

pub async fn get_by_id(
    Path(id): Path<String>,
) -> Result<
    Json<contracts::domain::a028_missing_cost_registry::aggregate::MissingCostRegistry>,
    ApiError,
> {
    let uuid = Uuid::parse_str(&id).map_err(|_| axum::http::StatusCode::BAD_REQUEST)?;
    match a028_missing_cost_registry::service::get_by_id(uuid).await {
        Ok(Some(doc)) => Ok(Json(doc)),
        Ok(None) => Err(axum::http::StatusCode::NOT_FOUND.into()),
        Err(error) => {
            tracing::error!("Failed to get a028 document {}: {}", id, error);
            Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into())
        }
    }
}

pub async fn update_document(
    Path(id): Path<String>,
    Json(dto): Json<MissingCostRegistryUpdateDto>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let uuid = Uuid::parse_str(&id).map_err(|_| axum::http::StatusCode::BAD_REQUEST)?;
    a028_missing_cost_registry::service::update_document(uuid, dto)
        .await
        .map_err(|error| {
            tracing::error!("Failed to update a028 document {}: {}", id, error);
            ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        })?;
    Ok(Json(serde_json::json!({"success": true})))
}

pub async fn post_document(Path(id): Path<String>) -> Result<Json<serde_json::Value>, ApiError> {
    let uuid = Uuid::parse_str(&id).map_err(|_| axum::http::StatusCode::BAD_REQUEST)?;
    a028_missing_cost_registry::service::post_document(uuid)
        .await
        .map_err(|error| {
            tracing::error!("Failed to post a028 document {}: {}", id, error);
            ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        })?;
    Ok(Json(
        serde_json::json!({"success": true, "message": "Document posted"}),
    ))
}

pub async fn unpost_document(Path(id): Path<String>) -> Result<Json<serde_json::Value>, ApiError> {
    let uuid = Uuid::parse_str(&id).map_err(|_| axum::http::StatusCode::BAD_REQUEST)?;
    a028_missing_cost_registry::service::unpost_document(uuid)
        .await
        .map_err(|error| {
            tracing::error!("Failed to unpost a028 document {}: {}", id, error);
            ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        })?;
    Ok(Json(
        serde_json::json!({"success": true, "message": "Document unposted"}),
    ))
}
