use crate::shared::error::ApiError;
use axum::{
    extract::{Path, Query},
    Json,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::domain::a021_production_output;
use crate::domain::a021_production_output::repository::{
    ProductionOutputListQuery, ProductionOutputListRow,
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
pub struct ProductionOutputListItemDto {
    pub id: String,
    pub document_no: String,
    pub document_date: String,
    pub description: String,
    pub article: String,
    pub count: i64,
    pub amount: f64,
    pub cost_of_production: Option<f64>,
    pub nomenclature_ref: Option<String>,
    pub nomenclature_code: Option<String>,
    pub nomenclature_article: Option<String>,
    pub connection_id: String,
    pub fetched_at: String,
    pub is_posted: bool,
}

impl From<ProductionOutputListRow> for ProductionOutputListItemDto {
    fn from(r: ProductionOutputListRow) -> Self {
        Self {
            id: r.id,
            document_no: r.document_no,
            document_date: r.document_date,
            description: r.description,
            article: r.article,
            count: r.count,
            amount: r.amount,
            cost_of_production: r.cost_of_production,
            nomenclature_ref: r.nomenclature_ref,
            nomenclature_code: r.nomenclature_code,
            nomenclature_article: r.nomenclature_article,
            connection_id: r.connection_id,
            fetched_at: r.fetched_at,
            is_posted: r.is_posted,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct PaginatedResponse {
    pub items: Vec<ProductionOutputListItemDto>,
    pub total: usize,
    pub page: usize,
    pub page_size: usize,
    pub total_pages: usize,
}

/// GET /api/a021/production-output/list
pub async fn list_paginated(
    Query(query): Query<ListQuery>,
) -> Result<Json<PaginatedResponse>, ApiError> {
    let page_size = query.limit.unwrap_or(100);
    let offset = query.offset.unwrap_or(0);
    let page = if page_size > 0 { offset / page_size } else { 0 };

    let list_query = ProductionOutputListQuery {
        date_from: query.date_from,
        date_to: query.date_to,
        search_query: query.search_query,
        sort_by: query.sort_by.unwrap_or_else(|| "document_date".to_string()),
        sort_desc: query.sort_desc.unwrap_or(true),
        limit: page_size,
        offset,
    };

    match a021_production_output::service::list_paginated(list_query).await {
        Ok(result) => {
            let total_pages = if page_size > 0 {
                (result.total + page_size - 1) / page_size
            } else {
                1
            };
            Ok(Json(PaginatedResponse {
                items: result.items.into_iter().map(|r| r.into()).collect(),
                total: result.total,
                page,
                page_size,
                total_pages,
            }))
        }
        Err(e) => {
            tracing::error!("Failed to list production output: {}", e);
            Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into())
        }
    }
}

/// GET /api/a021/production-output/:id
pub async fn get_by_id(
    Path(id): Path<String>,
) -> Result<Json<contracts::domain::a021_production_output::aggregate::ProductionOutput>, ApiError>
{
    let uuid = Uuid::parse_str(&id).map_err(|_| axum::http::StatusCode::BAD_REQUEST)?;
    match a021_production_output::service::get_by_id(uuid).await {
        Ok(Some(doc)) => Ok(Json(doc)),
        Ok(None) => Err(axum::http::StatusCode::NOT_FOUND.into()),
        Err(e) => {
            tracing::error!("Failed to get production output {}: {}", id, e);
            Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into())
        }
    }
}

/// POST /api/a021/production-output/:id/post
pub async fn post_production_output(
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let uuid = Uuid::parse_str(&id).map_err(|_| axum::http::StatusCode::BAD_REQUEST)?;
    a021_production_output::service::post_document(uuid)
        .await
        .map_err(|e| {
            tracing::error!("Failed to post production output {}: {}", id, e);
            ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        })?;
    Ok(Json(
        serde_json::json!({"success": true, "message": "Document posted"}),
    ))
}

/// POST /api/a021/production-output/:id/unpost
pub async fn unpost_production_output(
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let uuid = Uuid::parse_str(&id).map_err(|_| axum::http::StatusCode::BAD_REQUEST)?;
    a021_production_output::service::unpost_document(uuid)
        .await
        .map_err(|e| {
            tracing::error!("Failed to unpost production output {}: {}", id, e);
            ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        })?;
    Ok(Json(
        serde_json::json!({"success": true, "message": "Document unposted"}),
    ))
}
