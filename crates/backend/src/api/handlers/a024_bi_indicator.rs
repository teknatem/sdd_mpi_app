use crate::shared::error::ApiError;
use axum::{
    extract::{Path, Query},
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::domain::a024_bi_indicator;
use contracts::domain::a024_bi_indicator::aggregate::{
    BiIndicator, GenerateViewRequest, GenerateViewResponse,
};
use contracts::shared::analytics::IndicatorContext;
use contracts::shared::drilldown::{DrilldownRequest, DrilldownResponse};

#[derive(Deserialize)]
pub struct BiIndicatorListParams {
    pub limit: Option<u64>,
    pub offset: Option<u64>,
    pub sort_by: Option<String>,
    pub sort_desc: Option<bool>,
    pub q: Option<String>,
}

#[derive(Deserialize)]
pub struct IndicatorBatchLookupRequest {
    pub ids: Vec<String>,
}

#[derive(Serialize)]
pub struct BiIndicatorPaginatedResponse {
    pub items: Vec<BiIndicator>,
    pub total: u64,
    pub page: usize,
    pub page_size: usize,
    pub total_pages: usize,
}

/// GET /api/a024-bi-indicator
pub async fn list_all() -> Result<Json<Vec<BiIndicator>>, ApiError> {
    match a024_bi_indicator::service::list_all().await {
        Ok(v) => Ok(Json(v)),
        Err(_) => Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into()),
    }
}

/// GET /api/a024-bi-indicator/list
pub async fn list_paginated(
    Query(params): Query<BiIndicatorListParams>,
) -> Result<Json<BiIndicatorPaginatedResponse>, ApiError> {
    let limit = params.limit.unwrap_or(100).clamp(10, 10000);
    let offset = params.offset.unwrap_or(0);
    let page = offset / limit;
    let sort_by = params.sort_by.as_deref().unwrap_or("created_at");
    let sort_desc = params.sort_desc.unwrap_or(true);
    let q = params.q.as_deref();

    match a024_bi_indicator::service::list_paginated(page, limit, sort_by, sort_desc, q).await {
        Ok((items, total)) => {
            let page_size = limit as usize;
            let page_num = (offset as usize) / page_size;
            let total_pages = ((total as usize) + page_size - 1) / page_size;

            Ok(Json(BiIndicatorPaginatedResponse {
                items,
                total,
                page: page_num,
                page_size,
                total_pages,
            }))
        }
        Err(_) => Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into()),
    }
}

/// GET /api/a024-bi-indicator/owner/:user_id
pub async fn list_by_owner(
    Path(user_id): Path<String>,
) -> Result<Json<Vec<BiIndicator>>, ApiError> {
    match a024_bi_indicator::service::list_by_owner(&user_id).await {
        Ok(v) => Ok(Json(v)),
        Err(_) => Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into()),
    }
}

/// GET /api/a024-bi-indicator/public
pub async fn list_public() -> Result<Json<Vec<BiIndicator>>, ApiError> {
    match a024_bi_indicator::service::list_public().await {
        Ok(v) => Ok(Json(v)),
        Err(_) => Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into()),
    }
}

/// POST /api/a024-bi-indicator/resolve-batch
pub async fn resolve_batch(
    Json(req): Json<IndicatorBatchLookupRequest>,
) -> Result<Json<Vec<BiIndicator>>, ApiError> {
    match a024_bi_indicator::service::list_by_ids(&req.ids).await {
        Ok(v) => Ok(Json(v)),
        Err(_) => Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into()),
    }
}

/// GET /api/a024-bi-indicator/:id
pub async fn get_by_id(Path(id): Path<String>) -> Result<Json<BiIndicator>, ApiError> {
    match a024_bi_indicator::service::get_by_id(&id).await {
        Ok(Some(v)) => Ok(Json(v)),
        Ok(None) => Err(axum::http::StatusCode::NOT_FOUND.into()),
        Err(_) => Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into()),
    }
}

/// DELETE /api/a024-bi-indicator/:id
pub async fn delete(Path(id): Path<String>) -> Result<(), ApiError> {
    match a024_bi_indicator::service::delete(&id).await {
        Ok(()) => Ok(()),
        Err(_) => Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into()),
    }
}

/// POST /api/a024-bi-indicator
pub async fn upsert(
    Json(dto): Json<a024_bi_indicator::service::BiIndicatorDto>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let response_id = dto.id.clone();
    if dto.id.is_some() {
        match a024_bi_indicator::service::update(dto).await {
            Ok(_) => {
                if let Some(id) = response_id {
                    Ok(Json(json!({"success": true, "id": id})))
                } else {
                    Ok(Json(json!({"success": true})))
                }
            }
            Err(e) => {
                tracing::error!("Failed to update BI indicator: {}", e);
                if e.to_string().contains("Version conflict") {
                    Err(axum::http::StatusCode::CONFLICT.into())
                } else {
                    Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into())
                }
            }
        }
    } else {
        match a024_bi_indicator::service::create(dto).await {
            Ok(id) => Ok(Json(json!({"success": true, "id": id.to_string()}))),
            Err(e) => {
                tracing::error!("Failed to create BI indicator: {}", e);
                Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into())
            }
        }
    }
}

/// POST /api/a024-bi-indicator/testdata
pub async fn insert_test_data() -> axum::http::StatusCode {
    match a024_bi_indicator::service::insert_test_data().await {
        Ok(_) => axum::http::StatusCode::OK,
        Err(e) => {
            tracing::error!("Failed to insert test data: {}", e);
            axum::http::StatusCode::INTERNAL_SERVER_ERROR
        }
    }
}

/// POST /api/a024-bi-indicator/generate-view
pub async fn generate_view(
    Json(req): Json<GenerateViewRequest>,
) -> Result<Json<GenerateViewResponse>, (axum::http::StatusCode, String)> {
    match a024_bi_indicator::llm_support::generate_view(req).await {
        Ok(resp) => Ok(Json(resp)),
        Err(e) => {
            tracing::error!("Failed to generate BI indicator view: {}", e);
            Err((axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
        }
    }
}

// ---------------------------------------------------------------------------
// Compute (for DataView path)
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct ComputeParams {
    pub date_from: String,
    pub date_to: String,
    #[serde(default)]
    pub period2_from: Option<String>,
    #[serde(default)]
    pub period2_to: Option<String>,
    /// Comma-separated list of connection_mp UUIDs
    #[serde(default)]
    pub connection_mp_refs: Option<String>,
    #[serde(default)]
    pub params: std::collections::HashMap<String, String>,
}

#[derive(Deserialize)]
pub struct BatchComputeParams {
    pub indicator_ids: Vec<String>,
    pub date_from: String,
    pub date_to: String,
    #[serde(default)]
    pub period2_from: Option<String>,
    #[serde(default)]
    pub period2_to: Option<String>,
    /// Comma-separated list of connection_mp UUIDs
    #[serde(default)]
    pub connection_mp_refs: Option<String>,
    #[serde(default)]
    pub params: std::collections::HashMap<String, String>,
}

fn build_indicator_context(
    date_from: &str,
    date_to: &str,
    period2_from: &Option<String>,
    period2_to: &Option<String>,
    connection_mp_refs_raw: Option<&str>,
    params: &std::collections::HashMap<String, String>,
) -> IndicatorContext {
    let connection_mp_refs: Vec<String> = connection_mp_refs_raw
        .unwrap_or("")
        .split(',')
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect();

    let mut extra = std::collections::HashMap::new();
    if let Some(f) = period2_from {
        extra.insert("period2_from".to_string(), f.clone());
    }
    if let Some(t) = period2_to {
        extra.insert("period2_to".to_string(), t.clone());
    }
    extra.extend(params.clone());

    IndicatorContext {
        date_from: date_from.to_string(),
        date_to: date_to.to_string(),
        organization_ref: None,
        marketplace: None,
        connection_mp_refs,
        extra,
    }
}

/// POST /api/a024-bi-indicator/:id/compute
///
/// Вычисляет значение индикатора через его собственный data_spec
/// (DataView).
pub async fn compute(
    Path(id): Path<String>,
    Json(params): Json<ComputeParams>,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, String)> {
    let ctx = build_indicator_context(
        &params.date_from,
        &params.date_to,
        &params.period2_from,
        &params.period2_to,
        params.connection_mp_refs.as_deref(),
        &params.params,
    );

    match a024_bi_indicator::service::compute_indicator(&id, &ctx).await {
        Ok(val) => Ok(Json(serde_json::json!({
            "value": val.value,
            "previous_value": val.previous_value,
            "change_percent": val.change_percent,
            "status": val.status,
            "subtitle": val.subtitle,
            "details": val.details,
            "spark_points": val.spark_points,
        }))),
        Err(e) => {
            tracing::error!("compute error for indicator {}: {}", id, e);
            Err((axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
        }
    }
}

/// POST /api/a024-bi-indicator/compute-batch
///
/// Вычисляет набор BI индикаторов через их собственные data_spec (DataView).
pub async fn compute_batch(
    Json(params): Json<BatchComputeParams>,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, String)> {
    let ctx = build_indicator_context(
        &params.date_from,
        &params.date_to,
        &params.period2_from,
        &params.period2_to,
        params.connection_mp_refs.as_deref(),
        &params.params,
    );

    let indicator_ids = params.indicator_ids;
    let mut values = Vec::with_capacity(indicator_ids.len());
    for id in indicator_ids {
        match a024_bi_indicator::service::compute_indicator(&id, &ctx).await {
            Ok(val) => {
                values.push(serde_json::json!({
                    "id": id,
                    "value": val.value,
                    "previous_value": val.previous_value,
                    "change_percent": val.change_percent,
                    "status": val.status,
                    "subtitle": val.subtitle,
                    "details": val.details,
                    "spark_points": val.spark_points,
                }));
            }
            Err(err) => {
                tracing::warn!("batch compute skipped indicator {}: {}", id, err);
            }
        }
    }

    Ok(Json(serde_json::json!({ "values": values })))
}

// ---------------------------------------------------------------------------
// Drilldown
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct DrilldownParams {
    pub group_by: String,
    pub date_from: String,
    pub date_to: String,
    #[serde(default)]
    pub period2_from: Option<String>,
    #[serde(default)]
    pub period2_to: Option<String>,
    /// Comma-separated list of connection_mp UUIDs
    #[serde(default)]
    pub connection_mp_refs: Option<String>,
}

/// GET /api/a024-bi-indicator/:id/drilldown
///
/// Возвращает детализацию индикатора по выбранному измерению за 2 периода.
pub async fn drilldown(
    Path(id): Path<String>,
    Query(params): Query<DrilldownParams>,
) -> Result<Json<DrilldownResponse>, ApiError> {
    let connection_mp_refs: Vec<String> = params
        .connection_mp_refs
        .as_deref()
        .unwrap_or("")
        .split(',')
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect();

    let mut extra = std::collections::HashMap::new();
    if let Some(ref f) = params.period2_from {
        extra.insert("period2_from".to_string(), f.clone());
    }
    if let Some(ref t) = params.period2_to {
        extra.insert("period2_to".to_string(), t.clone());
    }

    let ctx = IndicatorContext {
        date_from: params.date_from.clone(),
        date_to: params.date_to.clone(),
        organization_ref: None,
        marketplace: None,
        connection_mp_refs,
        extra,
    };

    match a024_bi_indicator::service::get_indicator_drilldown(&id, params.group_by, &ctx).await {
        Ok(resp) => Ok(Json(resp)),
        Err(e) => {
            tracing::error!("Drilldown error for indicator {}: {}", id, e);
            Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into())
        }
    }
}

// ---------------------------------------------------------------------------
// Universal drilldown endpoint
// ---------------------------------------------------------------------------

/// POST /api/drilldown/execute
///
/// Универсальный schema drilldown без привязки к BI indicator.
pub async fn execute_drilldown(
    Json(req): Json<DrilldownRequest>,
) -> Result<Json<DrilldownResponse>, ApiError> {
    match crate::shared::drilldown::execute_schema_drilldown(&req).await {
        Ok(resp) => Ok(Json(resp)),
        Err(e) => {
            tracing::error!("Universal drilldown error: {}", e);
            Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into())
        }
    }
}
