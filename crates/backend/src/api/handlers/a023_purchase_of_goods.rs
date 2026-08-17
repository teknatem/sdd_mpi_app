use crate::shared::error::ApiError;
use axum::{
    extract::{Path, Query},
    Json,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::domain::a023_purchase_of_goods;
use crate::domain::a023_purchase_of_goods::repository::{
    PurchaseOfGoodsListQuery, PurchaseOfGoodsListRow,
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
pub struct PurchaseOfGoodsListItemDto {
    pub id: String,
    pub document_no: String,
    pub document_date: String,
    pub counterparty_key: String,
    pub counterparty_description: Option<String>,
    pub lines_json: Option<String>,
    pub connection_id: String,
    pub fetched_at: String,
    pub is_posted: bool,
}

impl From<PurchaseOfGoodsListRow> for PurchaseOfGoodsListItemDto {
    fn from(r: PurchaseOfGoodsListRow) -> Self {
        Self {
            id: r.id,
            document_no: r.document_no,
            document_date: r.document_date,
            counterparty_key: r.counterparty_key,
            counterparty_description: r.counterparty_description,
            lines_json: r.lines_json,
            connection_id: r.connection_id,
            fetched_at: r.fetched_at,
            is_posted: r.is_posted,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct PaginatedResponse {
    pub items: Vec<PurchaseOfGoodsListItemDto>,
    pub total: usize,
    pub page: usize,
    pub page_size: usize,
    pub total_pages: usize,
}

/// GET /api/a023/purchase-of-goods/list
pub async fn list_paginated(
    Query(query): Query<ListQuery>,
) -> Result<Json<PaginatedResponse>, ApiError> {
    let page_size = query.limit.unwrap_or(100);
    let offset = query.offset.unwrap_or(0);
    let page = if page_size > 0 { offset / page_size } else { 0 };

    let list_query = PurchaseOfGoodsListQuery {
        date_from: query.date_from,
        date_to: query.date_to,
        search_query: query.search_query,
        sort_by: query.sort_by.unwrap_or_else(|| "document_date".to_string()),
        sort_desc: query.sort_desc.unwrap_or(true),
        limit: page_size,
        offset,
    };

    match a023_purchase_of_goods::service::list_paginated(list_query).await {
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
            tracing::error!("Failed to list purchase of goods: {}", e);
            Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into())
        }
    }
}

/// GET /api/a023/purchase-of-goods/:id
pub async fn get_by_id(
    Path(id): Path<String>,
) -> Result<Json<contracts::domain::a023_purchase_of_goods::aggregate::PurchaseOfGoods>, ApiError> {
    let uuid = Uuid::parse_str(&id).map_err(|_| axum::http::StatusCode::BAD_REQUEST)?;
    match a023_purchase_of_goods::service::get_by_id(uuid).await {
        Ok(Some(doc)) => Ok(Json(doc)),
        Ok(None) => Err(axum::http::StatusCode::NOT_FOUND.into()),
        Err(e) => {
            tracing::error!("Failed to get purchase of goods {}: {}", id, e);
            Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into())
        }
    }
}

/// POST /api/a023/purchase-of-goods/:id/post
pub async fn post_purchase_of_goods(
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let uuid = Uuid::parse_str(&id).map_err(|_| axum::http::StatusCode::BAD_REQUEST)?;
    a023_purchase_of_goods::service::post_document(uuid)
        .await
        .map_err(|e| {
            tracing::error!("Failed to post purchase of goods {}: {}", id, e);
            ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        })?;
    Ok(Json(
        serde_json::json!({"success": true, "message": "Document posted"}),
    ))
}

/// POST /api/a023/purchase-of-goods/:id/unpost
pub async fn unpost_purchase_of_goods(
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let uuid = Uuid::parse_str(&id).map_err(|_| axum::http::StatusCode::BAD_REQUEST)?;
    a023_purchase_of_goods::service::unpost_document(uuid)
        .await
        .map_err(|e| {
            tracing::error!("Failed to unpost purchase of goods {}: {}", id, e);
            ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        })?;
    Ok(Json(
        serde_json::json!({"success": true, "message": "Document unposted"}),
    ))
}
