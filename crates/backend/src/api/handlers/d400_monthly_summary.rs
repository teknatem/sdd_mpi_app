use crate::shared::error::ApiError;
use axum::{extract::Query, Json};
use contracts::dashboards::d400_monthly_summary::{MonthlySummaryRequest, MonthlySummaryResponse};

use crate::dashboards::d400_monthly_summary::service;

/// GET /api/d400/monthly_summary?year=2025&month=12
pub async fn get_monthly_summary(
    Query(request): Query<MonthlySummaryRequest>,
) -> Result<Json<MonthlySummaryResponse>, ApiError> {
    tracing::info!(
        "D400 Dashboard: Getting monthly summary for {}-{:02}",
        request.year,
        request.month
    );

    match service::get_monthly_summary(request).await {
        Ok(response) => {
            tracing::info!(
                "D400 Dashboard: Returning {} rows for {} marketplaces",
                response.rows.len(),
                response.marketplaces.len()
            );
            Ok(Json(response))
        }
        Err(e) => {
            tracing::error!("D400 Dashboard: Failed to get monthly summary: {}", e);
            Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into())
        }
    }
}

/// GET /api/d400/periods
pub async fn get_available_periods() -> Result<Json<Vec<String>>, ApiError> {
    match service::get_available_periods().await {
        Ok(periods) => {
            tracing::info!(
                "D400 Dashboard: Returning {} available periods",
                periods.len()
            );
            Ok(Json(periods))
        }
        Err(e) => {
            tracing::error!("D400 Dashboard: Failed to get periods: {}", e);
            Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into())
        }
    }
}
