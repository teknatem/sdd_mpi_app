//! External BI API for daily WB advertising data (a026).

use crate::shared::error::ApiError;
use axum::{extract::Query, Json};
use serde::{Deserialize, Serialize};

use crate::domain::a026_wb_advert_daily;

const MAX_LIMIT: usize = 50_000;
fn default_limit() -> usize {
    5_000
}

#[derive(Debug, Deserialize)]
pub struct AdvertQuery {
    pub date_from: Option<String>,
    pub date_to: Option<String>,
    pub connection_id: Option<String>,
    #[serde(default = "default_limit")]
    pub limit: usize,
    #[serde(default)]
    pub offset: usize,
}

#[derive(Debug, Serialize)]
pub struct AdvertRow {
    pub date: String,
    pub connection_id: String,
    pub connection_name: Option<String>,
    pub organization_name: Option<String>,
    pub advert_id: i64,
    pub nm_id: i64,
    pub nm_name: String,
    pub nomenclature_ref: Option<String>,
    pub app_types: Vec<i32>,
    pub placements: Vec<String>,
    pub views: i64,
    pub clicks: i64,
    pub ctr: f64,
    pub cpc: f64,
    pub atbs: i64,
    pub orders: i64,
    pub shks: i64,
    pub sum: f64,
    pub sum_price: f64,
    pub cr: f64,
    pub canceled: i64,
}

#[derive(Debug, Serialize)]
pub struct AdvertResponse {
    pub items: Vec<AdvertRow>,
    pub total: usize,
}

/// GET /api/ext/v1/wb-advert-daily
/// Required: date_from, date_to. Optional: connection_id, limit, offset.
/// Authentication: X-Api-Key.
pub async fn list_advert(
    Query(query): Query<AdvertQuery>,
) -> Result<Json<AdvertResponse>, ApiError> {
    let date_from = query
        .date_from
        .as_deref()
        .filter(|value| !value.is_empty())
        .ok_or(ApiError::from(axum::http::StatusCode::BAD_REQUEST))?;
    let date_to = query
        .date_to
        .as_deref()
        .filter(|value| !value.is_empty())
        .ok_or(ApiError::from(axum::http::StatusCode::BAD_REQUEST))?;
    let result = a026_wb_advert_daily::repository::product_rows_for_period(
        date_from,
        date_to,
        query.connection_id.as_deref(),
        query.limit.clamp(1, MAX_LIMIT),
        query.offset,
    )
    .await
    .map_err(|error| {
        tracing::error!("[ext-api] wb-advert-daily list error: {error}");
        ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
    })?;

    let items = result
        .rows
        .into_iter()
        .map(|row| AdvertRow {
            date: row.date,
            connection_id: row.connection_id,
            connection_name: row.connection_name,
            organization_name: row.organization_name,
            advert_id: row.advert_id,
            nm_id: row.nm_id,
            nm_name: row.nm_name,
            nomenclature_ref: row.nomenclature_ref,
            app_types: row.app_types,
            placements: row.placements,
            views: row.metrics.views,
            clicks: row.metrics.clicks,
            ctr: row.metrics.ctr,
            cpc: row.metrics.cpc,
            atbs: row.metrics.atbs,
            orders: row.metrics.orders,
            shks: row.metrics.shks,
            sum: row.metrics.sum,
            sum_price: row.metrics.sum_price,
            cr: row.metrics.cr,
            canceled: row.metrics.canceled,
        })
        .collect();
    Ok(Json(AdvertResponse {
        items,
        total: result.total,
    }))
}
