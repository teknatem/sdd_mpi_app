use axum::{
    extract::{Path, Query},
    Json,
};
use contracts::domain::{
    a041_ym_shows_sales_daily::aggregate::{YmShowsSalesDailyLine, YmShowsSalesDailyMetrics},
    common::AggregateId,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::domain::a041_ym_shows_sales_daily::{
    self,
    repository::{YmShowsSalesListQuery, YmShowsSalesListRow},
};

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    pub date_from: Option<String>,
    pub date_to: Option<String>,
    pub connection_id: Option<String>,
    pub search_query: Option<String>,
    pub sort_by: Option<String>,
    pub sort_desc: Option<bool>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

#[derive(Debug, Serialize)]
pub struct ListItemDto {
    pub id: String,
    pub document_no: String,
    pub document_date: String,
    pub campaign_id: Option<String>,
    pub lines_count: i32,
    pub total_shows: Option<i64>,
    pub total_clicks: Option<i64>,
    pub total_to_cart: Option<i64>,
    pub total_order_items: Option<i64>,
    pub connection_id: String,
    pub connection_name: Option<String>,
    pub organization_name: Option<String>,
    pub fetched_at: String,
}

impl From<YmShowsSalesListRow> for ListItemDto {
    fn from(r: YmShowsSalesListRow) -> Self {
        Self {
            id: r.id,
            document_no: r.document_no,
            document_date: r.document_date,
            campaign_id: r.campaign_id,
            lines_count: r.lines_count,
            total_shows: r.total_shows,
            total_clicks: r.total_clicks,
            total_to_cart: r.total_to_cart,
            total_order_items: r.total_order_items,
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
    pub campaign_id: Option<String>,
    pub totals: YmShowsSalesDailyMetrics,
    pub source: String,
    pub fetched_at: String,
    pub lines: Vec<YmShowsSalesDailyLine>,
}

pub async fn list_paginated(
    Query(q): Query<ListQuery>,
) -> Result<Json<PaginatedResponse>, axum::http::StatusCode> {
    let page_size = q.limit.unwrap_or(100).clamp(1, 500);
    let offset = q.offset.unwrap_or(0);
    let page = offset / page_size;
    let result = a041_ym_shows_sales_daily::service::list_paginated(YmShowsSalesListQuery {
        date_from: q.date_from,
        date_to: q.date_to,
        connection_id: q.connection_id,
        search_query: q.search_query,
        sort_by: q.sort_by.unwrap_or_else(|| "document_date".into()),
        sort_desc: q.sort_desc.unwrap_or(true),
        limit: page_size,
        offset,
    })
    .await
    .map_err(|e| {
        tracing::error!("Failed to list YM shows-sales: {e}");
        axum::http::StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let total_pages = result.total.div_ceil(page_size);
    Ok(Json(PaginatedResponse {
        items: result.items.into_iter().map(Into::into).collect(),
        total: result.total,
        page,
        page_size,
        total_pages,
    }))
}

pub async fn get_by_id(Path(id): Path<String>) -> Result<Json<DetailsDto>, axum::http::StatusCode> {
    let uuid = Uuid::parse_str(&id).map_err(|_| axum::http::StatusCode::BAD_REQUEST)?;
    let doc = a041_ym_shows_sales_daily::service::get_by_id(uuid)
        .await
        .map_err(|e| {
            tracing::error!("Failed to get YM shows-sales {id}: {e}");
            axum::http::StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(axum::http::StatusCode::NOT_FOUND)?;
    Ok(Json(DetailsDto {
        id: doc.base.id.as_string(),
        document_no: doc.header.document_no,
        document_date: doc.header.document_date,
        connection_id: doc.header.connection_id,
        organization_id: doc.header.organization_id,
        marketplace_id: doc.header.marketplace_id,
        campaign_id: doc.header.campaign_id,
        totals: doc.totals,
        source: doc.source_meta.source,
        fetched_at: doc.source_meta.fetched_at,
        lines: doc.lines,
    }))
}
