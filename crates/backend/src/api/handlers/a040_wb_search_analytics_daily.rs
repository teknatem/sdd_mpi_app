use crate::shared::error::ApiError;
use axum::{
    extract::{Path, Query},
    Json,
};
use contracts::domain::a040_wb_search_analytics_daily::aggregate::WbSearchAnalyticsDailyLine;
use contracts::domain::common::AggregateId;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::domain::a040_wb_search_analytics_daily;
use crate::domain::a040_wb_search_analytics_daily::repository::{
    WbSearchAnalyticsListQuery, WbSearchAnalyticsListRow,
};

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    pub date_from: Option<String>,
    pub date_to: Option<String>,
    pub connection_id: Option<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
    pub search_query: Option<String>,
    pub sort_by: Option<String>,
    pub sort_desc: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct ListItemDto {
    pub id: String,
    pub document_no: String,
    pub document_date: String,
    pub lines_count: i32,
    pub total_impressions: i64,
    pub total_open_card: i64,
    pub total_orders: i64,
    pub connection_id: String,
    pub connection_name: Option<String>,
    pub organization_name: Option<String>,
    pub fetched_at: String,
}

impl From<WbSearchAnalyticsListRow> for ListItemDto {
    fn from(r: WbSearchAnalyticsListRow) -> Self {
        Self {
            id: r.id,
            document_no: r.document_no,
            document_date: r.document_date,
            lines_count: r.lines_count,
            total_impressions: r.total_impressions,
            total_open_card: r.total_open_card,
            total_orders: r.total_orders,
            connection_id: r.connection_id,
            connection_name: r.connection_name,
            organization_name: r.organization_name,
            fetched_at: r.fetched_at,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct PaginatedResponse {
    pub items: Vec<ListItemDto>,
    pub total: usize,
    pub page: usize,
    pub page_size: usize,
    pub total_pages: usize,
}

#[derive(Debug, Serialize)]
pub struct DetailsDto {
    pub id: String,
    pub document_no: String,
    pub document_date: String,
    pub connection_id: String,
    pub organization_id: String,
    pub marketplace_id: String,
    pub total_impressions: i64,
    pub total_open_card: i64,
    pub total_orders: i64,
    pub source: String,
    pub fetched_at: String,
    pub lines: Vec<WbSearchAnalyticsDailyLine>,
}

pub async fn list_paginated(
    Query(query): Query<ListQuery>,
) -> Result<Json<PaginatedResponse>, ApiError> {
    let page_size = query.limit.unwrap_or(100);
    let offset = query.offset.unwrap_or(0);
    let page = if page_size > 0 { offset / page_size } else { 0 };

    let list_query = WbSearchAnalyticsListQuery {
        date_from: query.date_from,
        date_to: query.date_to,
        connection_id: query.connection_id,
        search_query: query.search_query,
        sort_by: query.sort_by.unwrap_or_else(|| "document_date".to_string()),
        sort_desc: query.sort_desc.unwrap_or(true),
        limit: page_size,
        offset,
    };

    match a040_wb_search_analytics_daily::service::list_paginated(list_query).await {
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
        Err(e) => {
            tracing::error!("Failed to list WB search analytics: {}", e);
            Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into())
        }
    }
}

pub async fn get_by_id(Path(id): Path<String>) -> Result<Json<DetailsDto>, ApiError> {
    let uuid = Uuid::parse_str(&id).map_err(|_| axum::http::StatusCode::BAD_REQUEST)?;
    let doc = match a040_wb_search_analytics_daily::service::get_by_id(uuid).await {
        Ok(Some(doc)) => doc,
        Ok(None) => return Err(axum::http::StatusCode::NOT_FOUND.into()),
        Err(e) => {
            tracing::error!("Failed to get WB search analytics {}: {}", id, e);
            return Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into());
        }
    };

    Ok(Json(DetailsDto {
        id: doc.base.id.as_string(),
        document_no: doc.header.document_no.clone(),
        document_date: doc.header.snapshot_date.clone(),
        connection_id: doc.header.connection_id.clone(),
        organization_id: doc.header.organization_id.clone(),
        marketplace_id: doc.header.marketplace_id.clone(),
        total_impressions: doc.totals.total_impressions,
        total_open_card: doc.totals.total_open_card,
        total_orders: doc.totals.total_orders,
        source: doc.source_meta.source.clone(),
        fetched_at: doc.source_meta.fetched_at.clone(),
        lines: doc.lines,
    }))
}
