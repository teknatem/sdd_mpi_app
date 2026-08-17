use crate::shared::error::ApiError;
use axum::{
    extract::{Path, Query},
    Json,
};
use contracts::quality::{
    CheckDetails, CheckResult, NipCleanupRequest, NipCleanupResult, NipGroupsResponse,
    NipProjectionRow, NipRepostRequest, NipRepostResult, QualityCheckInfo, QualityCheckOverview,
    QualityCheckReloadReport, QualityCheckRunRequest, QualityCheckRunSummary, QualityCheckSource,
};
use serde::Deserialize;

/// GET /api/quality/checks
pub async fn list_checks() -> Json<Vec<QualityCheckInfo>> {
    Json(crate::quality::list_checks())
}

/// GET /api/quality/checks/overview
pub async fn list_check_overviews() -> Result<Json<Vec<QualityCheckOverview>>, ApiError> {
    crate::quality::list_check_overviews()
        .await
        .map(Json)
        .map_err(|error| {
            tracing::error!("quality overview: {error}");
            ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        })
}

/// POST /api/quality/checks/:id/run
pub async fn run_check(
    Path(id): Path<String>,
    body: Option<Json<QualityCheckRunRequest>>,
) -> Result<Json<CheckResult>, ApiError> {
    let input = body
        .map(|Json(body)| body.input)
        .unwrap_or_else(|| serde_json::json!({}));
    match crate::quality::run_check_with_input(&id, input, "manual").await {
        Ok(details) => Ok(Json(details.result)),
        Err(e) if e.to_string().starts_with("NOT_FOUND:") => {
            tracing::warn!("Quality check not found: '{}'", id);
            Err(axum::http::StatusCode::NOT_FOUND.into())
        }
        Err(e) => {
            tracing::error!("Quality check '{}' failed: {}", id, e);
            Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into())
        }
    }
}

/// POST /api/quality/checks/reload
pub async fn reload_checks() -> Json<QualityCheckReloadReport> {
    Json(crate::quality::registry::reload().await)
}

#[derive(Debug, Deserialize)]
pub struct RunsQuery {
    #[serde(default = "default_runs_limit")]
    pub limit: i64,
}

fn default_runs_limit() -> i64 {
    25
}

/// GET /api/quality/checks/:id/runs
pub async fn list_runs(
    Path(id): Path<String>,
    Query(query): Query<RunsQuery>,
) -> Result<Json<Vec<QualityCheckRunSummary>>, ApiError> {
    match crate::quality::list_runs(&id, query.limit).await {
        Ok(runs) => Ok(Json(runs)),
        Err(error) if error.to_string().starts_with("NOT_FOUND:") => {
            Err(axum::http::StatusCode::NOT_FOUND.into())
        }
        Err(error) => {
            tracing::error!("quality runs '{}': {}", id, error);
            Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into())
        }
    }
}

/// GET /api/quality/checks/:id/details
pub async fn check_details(Path(id): Path<String>) -> Result<Json<CheckDetails>, ApiError> {
    match crate::quality::check_details(&id).await {
        Ok(details) => Ok(Json(details)),
        Err(e) if e.to_string().starts_with("NOT_FOUND:") => {
            tracing::warn!("Quality check details not found: '{}'", id);
            Err(axum::http::StatusCode::NOT_FOUND.into())
        }
        Err(e) => {
            tracing::error!("check_details '{}': {}", id, e);
            Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into())
        }
    }
}

/// GET /api/quality/checks/:id/sources
pub async fn list_sources(
    Path(id): Path<String>,
) -> Result<Json<Vec<QualityCheckSource>>, ApiError> {
    match crate::quality::list_check_sources(&id) {
        Ok(sources) => Ok(Json(sources)),
        Err(e) if e.to_string().starts_with("NOT_FOUND:") => {
            tracing::warn!("Quality check sources not found: '{}'", id);
            Err(axum::http::StatusCode::NOT_FOUND.into())
        }
        Err(e) => {
            tracing::error!("list_sources '{}': {}", id, e);
            Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into())
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct GroupsQuery {
    pub projection_table: String,
    #[serde(default)]
    pub page: i64,
    #[serde(default = "default_page_size")]
    pub page_size: i64,
    #[serde(default = "default_sort_groups")]
    pub sort_by: String,
    #[serde(default)]
    pub sort_desc: bool,
}

fn default_page_size() -> i64 {
    50
}
fn default_sort_groups() -> String {
    "missing_count".to_string()
}

/// GET /api/quality/checks/:id/groups?projection_table=...&page=0&page_size=50&sort_by=...&sort_desc=false
pub async fn list_groups(
    Path(id): Path<String>,
    Query(q): Query<GroupsQuery>,
) -> Result<Json<NipGroupsResponse>, ApiError> {
    match crate::quality::list_check_groups(
        &id,
        &q.projection_table,
        q.page,
        q.page_size,
        &q.sort_by,
        q.sort_desc,
    )
    .await
    {
        Ok(resp) => Ok(Json(resp)),
        Err(e) if e.to_string().starts_with("NOT_FOUND:") => {
            tracing::warn!("Quality check groups not found: '{}': {}", id, e);
            Err(axum::http::StatusCode::NOT_FOUND.into())
        }
        Err(e) => {
            tracing::error!("list_groups '{}': {}", id, e);
            Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into())
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct RowsQuery {
    pub projection_table: String,
    pub registrator_ref: String,
}

/// GET /api/quality/checks/:id/rows?projection_table=...&registrator_ref=...
pub async fn list_rows(
    Path(id): Path<String>,
    Query(q): Query<RowsQuery>,
) -> Result<Json<Vec<NipProjectionRow>>, ApiError> {
    match crate::quality::list_check_rows(&id, &q.projection_table, &q.registrator_ref).await {
        Ok(rows) => Ok(Json(rows)),
        Err(e) if e.to_string().starts_with("NOT_FOUND:") => {
            tracing::warn!("Quality check rows not found: '{}': {}", id, e);
            Err(axum::http::StatusCode::NOT_FOUND.into())
        }
        Err(e) => {
            tracing::error!("list_rows '{}': {}", id, e);
            Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into())
        }
    }
}

/// POST /api/quality/checks/:id/repost
pub async fn bulk_repost(
    Path(id): Path<String>,
    Json(body): Json<NipRepostRequest>,
) -> Result<Json<NipRepostResult>, ApiError> {
    match crate::quality::bulk_repost(&id, &body).await {
        Ok(result) => Ok(Json(result)),
        Err(e) if e.to_string().starts_with("NOT_FOUND:") => {
            tracing::warn!("Quality bulk_repost not found: '{}': {}", id, e);
            Err(axum::http::StatusCode::NOT_FOUND.into())
        }
        Err(e) => {
            tracing::error!("bulk_repost '{}': {}", id, e);
            Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into())
        }
    }
}

/// POST /api/quality/checks/:id/cleanup
pub async fn cleanup_orphans(
    Path(id): Path<String>,
    Json(body): Json<NipCleanupRequest>,
) -> Result<Json<NipCleanupResult>, ApiError> {
    match crate::quality::cleanup_orphans(&id, &body).await {
        Ok(result) => Ok(Json(result)),
        Err(e) if e.to_string().starts_with("NOT_FOUND:") => {
            tracing::warn!("Quality cleanup not found: '{}': {}", id, e);
            Err(axum::http::StatusCode::NOT_FOUND.into())
        }
        Err(e) => {
            tracing::error!("cleanup_orphans '{}': {}", id, e);
            Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into())
        }
    }
}
