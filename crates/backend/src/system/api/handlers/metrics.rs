//! HTTP-обёртки над `system::metrics`. Логики здесь нет.

use axum::{extract::Query, http::StatusCode, Json};
use contracts::system::metrics::{
    CollectResultDto, MetricSeriesDto, MetricSnapshotDto, ProjectMetricsDto,
};
use serde::Deserialize;

use crate::system::metrics as svc;

fn internal(error: anyhow::Error) -> (StatusCode, String) {
    tracing::error!("[metrics] {error}");
    (StatusCode::INTERNAL_SERVER_ERROR, error.to_string())
}

/// GET /api/system/metrics/latest
pub async fn latest() -> Result<Json<ProjectMetricsDto>, (StatusCode, String)> {
    svc::latest().await.map(Json).map_err(internal)
}

#[derive(Deserialize)]
pub struct SeriesQuery {
    /// Ключи метрик через запятую; неизвестные каталогу отбрасываются.
    pub keys: String,
    #[serde(default = "default_limit")]
    pub limit: u64,
}

fn default_limit() -> u64 {
    60
}

/// GET /api/system/metrics/series?keys=a,b&limit=60
pub async fn series(
    Query(query): Query<SeriesQuery>,
) -> Result<Json<Vec<MetricSeriesDto>>, (StatusCode, String)> {
    let keys: Vec<String> = query
        .keys
        .split(',')
        .map(|key| key.trim().to_string())
        .filter(|key| !key.is_empty())
        .collect();
    svc::series(&keys, query.limit)
        .await
        .map(Json)
        .map_err(internal)
}

#[derive(Deserialize)]
pub struct SnapshotsQuery {
    #[serde(default = "default_snapshots_limit")]
    pub limit: u64,
}

fn default_snapshots_limit() -> u64 {
    50
}

/// GET /api/system/metrics/snapshots?limit=50
pub async fn snapshots(
    Query(query): Query<SnapshotsQuery>,
) -> Result<Json<Vec<MetricSnapshotDto>>, (StatusCode, String)> {
    svc::repository::list_snapshots(query.limit.clamp(1, 500))
        .await
        .map(|rows| Json(rows.iter().map(|row| row.to_dto()).collect()))
        .map_err(internal)
}

/// POST /api/system/metrics/collect — пересобрать снимок вручную.
pub async fn collect() -> Result<Json<CollectResultDto>, (StatusCode, String)> {
    svc::collect_and_store("manual")
        .await
        .map(|(snapshot_id, collect_ms, metrics_written)| {
            Json(CollectResultDto {
                snapshot_id,
                collect_ms,
                metrics_written,
            })
        })
        .map_err(internal)
}
