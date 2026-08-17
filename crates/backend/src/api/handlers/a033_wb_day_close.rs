use crate::shared::error::ApiError;
use axum::{extract::Path, extract::Query, Json};
use contracts::domain::a033_wb_day_close::{
    ArchiveAndRecreateRequest, CompareRequest, CompareResponse, CreateActiveRequest,
    RepostProblematicRequest, RepostResult, WbDayClose, WbDayCloseListDto,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::domain::a033_wb_day_close::{advert_builder, repository::ListQuery, service};

// ─────────────────────────────────────────────────────────────────────────────
// Query params
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct ListQuery_ {
    pub connection_id: Option<String>,
    pub date_from: Option<String>,
    pub date_to: Option<String>,
    pub include_archived: Option<bool>,
    pub limit: Option<u64>,
    pub offset: Option<u64>,
}

// ─────────────────────────────────────────────────────────────────────────────
// GET /api/a033/wb-day-close
// ─────────────────────────────────────────────────────────────────────────────

pub async fn list_paginated(
    Query(q): Query<ListQuery_>,
) -> Result<Json<Vec<WbDayCloseListDto>>, ApiError> {
    let query = ListQuery {
        connection_id: q.connection_id,
        date_from: q.date_from,
        date_to: q.date_to,
        include_archived: q.include_archived,
        limit: q.limit,
        offset: q.offset,
    };

    service::list_paginated(query)
        .await
        .map(|items| Json(items.into_iter().map(|d| d.to_list_dto()).collect()))
        .map_err(|e| {
            tracing::error!("list a033: {}", e);
            ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        })
}

// ─────────────────────────────────────────────────────────────────────────────
// GET /api/a033/wb-day-close/:id
// ─────────────────────────────────────────────────────────────────────────────

// ─────────────────────────────────────────────────────────────────────────────
// GET /api/a033/wb-day-close/:id/advert-live
// Живые итоги p913/GL для диагностики стагнации снапшота
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct AdvertLiveTotals {
    pub p913_no_order: f64,
    pub p913_order_accrual: f64,
    pub p913_order_expense: f64,
    pub gl_no_order: f64,
    pub gl_order_accrual: f64,
    pub gl_order_expense: f64,
    /// Диагностика: количество строк p913 для order_accrual
    pub p913_accrual_rows: u64,
    /// Диагностика: количество GL-записей для order_accrual
    pub gl_accrual_entries: u64,
    /// Диагностика: поразрядный breakdown по a026 registrator_ref
    pub accrual_by_registrator: Vec<RegistratorRow>,
}

#[derive(Serialize)]
pub struct RegistratorRow {
    pub registrator_ref: String,
    pub p913_sum: f64,
    pub p913_rows: u64,
    pub gl_sum: f64,
    pub delta: f64,
}

pub async fn advert_live(Path(id): Path<String>) -> Result<Json<AdvertLiveTotals>, ApiError> {
    let uuid = Uuid::parse_str(&id).map_err(|_| axum::http::StatusCode::BAD_REQUEST)?;
    let doc = service::get_by_id(uuid)
        .await
        .map_err(|e| {
            tracing::error!("advert_live get a033 {}: {}", id, e);
            ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        })?
        .ok_or(ApiError::from(axum::http::StatusCode::NOT_FOUND))?;

    let (build_result, diag) = tokio::try_join!(
        advert_builder::build(&doc.connection_id, &doc.business_date),
        advert_builder::fetch_accrual_diagnostic(&doc.connection_id, &doc.business_date),
    )
    .map_err(|e| {
        tracing::error!("advert_live build {}: {}", id, e);
        ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
    })?;

    let p913_no_order: f64 = build_result.no_order_lines.iter().map(|r| r.amount).sum();
    let p913_order_accrual: f64 = build_result
        .order_accrual_lines
        .iter()
        .map(|r| r.amount)
        .sum();

    let accrual_by_registrator = diag
        .per_registrator
        .into_iter()
        .map(|r| RegistratorRow {
            delta: r.p913_sum - r.gl_sum,
            registrator_ref: r.registrator_ref,
            p913_sum: r.p913_sum,
            p913_rows: r.p913_rows,
            gl_sum: r.gl_sum,
        })
        .collect();

    Ok(Json(AdvertLiveTotals {
        p913_no_order,
        p913_order_accrual,
        p913_order_expense: build_result.snap_order_expense,
        gl_no_order: build_result.gl_no_order,
        gl_order_accrual: build_result.gl_order_accrual,
        gl_order_expense: build_result.gl_order_expense,
        p913_accrual_rows: diag.total_rows,
        gl_accrual_entries: diag.gl_entries,
        accrual_by_registrator,
    }))
}

pub async fn get_by_id(Path(id): Path<String>) -> Result<Json<WbDayClose>, ApiError> {
    let uuid = Uuid::parse_str(&id).map_err(|_| axum::http::StatusCode::BAD_REQUEST)?;
    match service::get_by_id(uuid).await {
        Ok(Some(doc)) => Ok(Json(doc)),
        Ok(None) => Err(axum::http::StatusCode::NOT_FOUND.into()),
        Err(e) => {
            tracing::error!("get a033 {}: {}", id, e);
            Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into())
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// GET /api/a033/wb-day-close/by-day/:connection_id/:business_date
// ─────────────────────────────────────────────────────────────────────────────

pub async fn list_by_day(
    Path((connection_id, business_date)): Path<(String, String)>,
) -> Result<Json<Vec<WbDayCloseListDto>>, ApiError> {
    service::list_by_day(&connection_id, &business_date)
        .await
        .map(|items| Json(items.into_iter().map(|d| d.to_list_dto()).collect()))
        .map_err(|e| {
            tracing::error!(
                "list_by_day a033 {}/{}: {}",
                connection_id,
                business_date,
                e
            );
            ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        })
}

// ─────────────────────────────────────────────────────────────────────────────
// POST /api/a033/wb-day-close
// ─────────────────────────────────────────────────────────────────────────────

pub async fn create_active(
    Json(body): Json<CreateActiveRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    service::create_active(&body.connection_id, &body.business_date)
        .await
        .map(|id| Json(serde_json::json!({ "id": id.to_string() })))
        .map_err(|e| {
            tracing::error!("create a033: {}", e);
            ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        })
}

// ─────────────────────────────────────────────────────────────────────────────
// POST /api/a033/wb-day-close/:id/recalculate
// ─────────────────────────────────────────────────────────────────────────────

pub async fn recalculate(Path(id): Path<String>) -> Result<axum::http::StatusCode, ApiError> {
    let uuid = Uuid::parse_str(&id).map_err(|_| axum::http::StatusCode::BAD_REQUEST)?;
    service::recalculate(uuid).await.map_err(|e| {
        tracing::error!("recalculate a033 {}: {}", id, e);
        ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
    })?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}

// ─────────────────────────────────────────────────────────────────────────────
// POST /api/a033/wb-day-close/:id/repost-problematic-a012
// ─────────────────────────────────────────────────────────────────────────────

pub async fn repost_problematic_a012(
    Path(id): Path<String>,
    Json(body): Json<RepostProblematicRequest>,
) -> Result<Json<RepostResult>, ApiError> {
    let uuid = Uuid::parse_str(&id).map_err(|_| axum::http::StatusCode::BAD_REQUEST)?;
    service::repost_problematic_a012(uuid, &body.only_problem_codes)
        .await
        .map(Json)
        .map_err(|e| {
            tracing::error!("repost a033 {}: {}", id, e);
            ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        })
}

// ─────────────────────────────────────────────────────────────────────────────
// POST /api/a033/wb-day-close/:id/archive-and-recreate
// ─────────────────────────────────────────────────────────────────────────────

pub async fn archive_and_recreate(
    Path(id): Path<String>,
    Json(body): Json<ArchiveAndRecreateRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let uuid = Uuid::parse_str(&id).map_err(|_| axum::http::StatusCode::BAD_REQUEST)?;
    service::archive_and_recreate(uuid, body.reason)
        .await
        .map(|new_id| Json(serde_json::json!({ "id": new_id.to_string() })))
        .map_err(|e| {
            tracing::error!("archive_and_recreate a033 {}: {}", id, e);
            ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        })
}

// ─────────────────────────────────────────────────────────────────────────────
// POST /api/a033/wb-day-close/compare
// ─────────────────────────────────────────────────────────────────────────────

pub async fn compare(Json(body): Json<CompareRequest>) -> Result<Json<CompareResponse>, ApiError> {
    let active_uuid =
        Uuid::parse_str(&body.active_id).map_err(|_| axum::http::StatusCode::BAD_REQUEST)?;
    let archived_uuid =
        Uuid::parse_str(&body.archived_id).map_err(|_| axum::http::StatusCode::BAD_REQUEST)?;

    service::compare(active_uuid, archived_uuid)
        .await
        .map(Json)
        .map_err(|e| {
            tracing::error!("compare a033: {}", e);
            ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        })
}
