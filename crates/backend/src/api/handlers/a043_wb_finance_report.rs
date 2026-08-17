use crate::shared::error::ApiError;
use axum::{
    extract::{Path, Query},
    Json,
};
use contracts::domain::{
    a043_wb_finance_report::{WbFinanceReportHeader, WbFinanceReportSourceMeta},
    common::AggregateId,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::domain::a043_wb_finance_report::{repository::FinanceReportListQuery, service};

fn normalized_page_size(limit: Option<usize>) -> usize {
    limit.unwrap_or(100).clamp(1, 500)
}

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    pub date_from: Option<String>,
    pub date_to: Option<String>,
    pub connection_id: Option<String>,
    pub period: Option<String>,
    pub search: Option<String>,
    pub sort_by: Option<String>,
    pub sort_desc: Option<bool>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

#[derive(Debug, Serialize)]
pub struct ListResponse<T> {
    pub items: Vec<T>,
    pub total: usize,
    pub page: usize,
    pub page_size: usize,
    pub total_pages: usize,
}

pub async fn list(
    Query(q): Query<ListQuery>,
) -> Result<
    Json<ListResponse<crate::domain::a043_wb_finance_report::repository::FinanceReportListRow>>,
    ApiError,
> {
    let page_size = normalized_page_size(q.limit);
    let offset = q.offset.unwrap_or(0);
    let result = service::list(FinanceReportListQuery {
        date_from: q.date_from,
        date_to: q.date_to,
        connection_id: q.connection_id,
        period: q.period,
        search: q.search,
        sort_by: q.sort_by.unwrap_or_else(|| "create_date".into()),
        sort_desc: q.sort_desc.unwrap_or(true),
        limit: page_size,
        offset,
    })
    .await
    .map_err(|e| {
        tracing::error!(error=%e, "a043 list failed");
        ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
    })?;
    let total_pages = result.total.div_ceil(page_size);
    Ok(Json(ListResponse {
        items: result.items,
        total: result.total,
        page: offset / page_size,
        page_size,
        total_pages,
    }))
}

#[derive(Debug, Serialize)]
pub struct DetailResponse {
    pub id: String,
    pub header: WbFinanceReportHeader,
    pub source_meta: WbFinanceReportSourceMeta,
    pub lines_count: usize,
}

pub async fn get(Path(id): Path<String>) -> Result<Json<DetailResponse>, ApiError> {
    let id = Uuid::parse_str(&id).map_err(|_| axum::http::StatusCode::BAD_REQUEST)?;
    let document = service::get_by_id(id)
        .await
        .map_err(|e| {
            tracing::error!(error=%e, "a043 get failed");
            ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        })?
        .ok_or(ApiError::from(axum::http::StatusCode::NOT_FOUND))?;
    Ok(Json(DetailResponse {
        id: document.base.id.as_string(),
        lines_count: document.lines.len(),
        header: document.header,
        source_meta: document.source_meta,
    }))
}

#[derive(Debug, Deserialize)]
pub struct LinesQuery {
    pub offset: Option<usize>,
    pub limit: Option<usize>,
}

#[derive(Debug, Serialize)]
pub struct LinesResponse {
    pub items: Vec<Value>,
    pub total: usize,
    pub offset: usize,
    pub limit: usize,
}

pub async fn lines(
    Path(id): Path<String>,
    Query(q): Query<LinesQuery>,
) -> Result<Json<LinesResponse>, ApiError> {
    let id = Uuid::parse_str(&id).map_err(|_| axum::http::StatusCode::BAD_REQUEST)?;
    let offset = q.offset.unwrap_or(0);
    let limit = normalized_page_size(q.limit);
    let page = service::lines(id, offset, limit)
        .await
        .map_err(|e| {
            tracing::error!(error=%e, "a043 lines failed");
            ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        })?
        .ok_or(ApiError::from(axum::http::StatusCode::NOT_FOUND))?;
    Ok(Json(LinesResponse {
        items: page.items,
        total: page.total,
        offset: page.offset,
        limit: page.limit,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_page_is_capped_at_five_hundred() {
        assert_eq!(normalized_page_size(Some(10_000)), 500);
        assert_eq!(normalized_page_size(Some(0)), 1);
        assert_eq!(normalized_page_size(None), 100);
    }
}
