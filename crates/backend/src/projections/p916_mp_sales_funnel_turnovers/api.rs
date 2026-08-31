//! HTTP-хендлеры пересбора и диагностики воронки.
//!
//! Живут у проекции, а не в `api/handlers/usecases.rs`: маршрут исторически
//! лежит под `/api/u508/...`, потому что пересбором управляет движок
//! перепроведения, но предмет операции — воронка. Хендлеры в каталоге ядра
//! делали его зависимым от маркетплейсной проекции.

use axum::{
    extract::{Json, Query},
    Json as JsonResponse,
};
use serde::Deserialize;

use crate::shared::error::ApiError;

/// POST /api/u508/repost/funnel/start — пересбор воронки p916 за период
/// (перепроведение a015/a012/a026 и заказов YM плюс стадия 1 из a036).
pub async fn start_funnel_rebuild(
    Json(request): Json<
        contracts::projections::p916_mp_sales_funnel_turnovers::dto::FunnelRebuildRequest,
    >,
) -> Result<JsonResponse<contracts::usecases::u508_repost_documents::RepostResponse>, ApiError> {
    let tracker = crate::usecases::u508_repost_documents::shared()
        .progress_tracker
        .clone();
    match super::service::start_rebuild(tracker, request).await {
        Ok(response) => Ok(JsonResponse(response)),
        Err(e) => {
            tracing::error!("Failed to start funnel rebuild: {}", e);
            Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into())
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct FunnelDiagnosticsQuery {
    pub date_from: String,
    pub date_to: String,
    /// Список кабинетов через запятую; пусто → все кабинеты.
    #[serde(default)]
    pub connection_mp_refs: Option<String>,
}

/// GET /api/u508/repost/funnel/diagnostics — сводка воронки за период
/// (смотрят после пересбора, чтобы увидеть, что изменилось).
pub async fn funnel_diagnostics(
    Query(query): Query<FunnelDiagnosticsQuery>,
) -> Result<
    JsonResponse<contracts::projections::p916_mp_sales_funnel_turnovers::dto::FunnelPeriodSummary>,
    ApiError,
> {
    let connection_mp_refs: Vec<String> = query
        .connection_mp_refs
        .unwrap_or_default()
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    match super::repository::funnel_period_summary(
        &query.date_from,
        &query.date_to,
        &connection_mp_refs,
    )
    .await
    {
        Ok(summary) => Ok(JsonResponse(summary)),
        Err(e) => {
            tracing::error!("Failed to compute funnel diagnostics: {}", e);
            Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into())
        }
    }
}
