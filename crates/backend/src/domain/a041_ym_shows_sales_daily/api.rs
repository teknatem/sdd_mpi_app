//! External BI API handler for the Yandex Market sales funnel (a041).
//! Emits a flat JSON array of one row per `offer_id × date`.

use crate::shared::error::ApiError;
use axum::{extract::Query, Json};
use serde::{Deserialize, Serialize};

use crate::domain::a041_ym_shows_sales_daily;

const MAX_LIMIT: usize = 50_000;

fn default_limit() -> usize {
    5_000
}

#[derive(Debug, Deserialize)]
pub struct FunnelQuery {
    pub date_from: Option<String>,
    pub date_to: Option<String>,
    pub connection_id: Option<String>,
    #[serde(default = "default_limit")]
    pub limit: usize,
    #[serde(default)]
    pub offset: usize,
}

#[derive(Debug, Serialize)]
pub struct FunnelRow {
    pub date: String,
    pub connection_id: String,
    pub connection_name: Option<String>,
    pub organization_name: Option<String>,
    pub campaign_id: Option<String>,
    pub offer_id: String,
    pub offer_name: String,
    pub marketplace_product_ref: Option<String>,
    pub nomenclature_ref: Option<String>,
    pub brand_name: Option<String>,
    pub category_id: Option<String>,
    pub category_name: Option<String>,
    pub shows: Option<i64>,
    pub clicks: Option<i64>,
    pub cart_count: Option<i64>,
    pub order_count: Option<i64>,
    /// Заказано на сумму, ₽, по счётчику отчёта воронки. `null` — N/A.
    pub order_sum: Option<i64>,
    pub delivered_count: Option<i64>,
    /// Доставлено за период на сумму, ₽. `null` — N/A.
    pub delivered_sum: Option<i64>,
    pub cancel_count: Option<i64>,
    pub return_count: Option<i64>,
    pub click_through_conversion: Option<f64>,
    pub add_to_cart_conversion: Option<f64>,
    pub cart_to_order_conversion: Option<f64>,
}

#[derive(Debug, Serialize)]
pub struct FunnelResponse {
    pub items: Vec<FunnelRow>,
    pub total: usize,
}

fn required_date(value: Option<&str>) -> Result<&str, ApiError> {
    value
        .filter(|value| !value.is_empty())
        .ok_or(ApiError::from(axum::http::StatusCode::BAD_REQUEST))
}

/// GET /api/ext/v1/ym-sales-funnel — сохранённая дневная воронка YM.
/// Authentication is handled by the shared `X-Api-Key` middleware.
pub async fn list_funnel(
    Query(query): Query<FunnelQuery>,
) -> Result<Json<FunnelResponse>, ApiError> {
    let date_from = required_date(query.date_from.as_deref())?;
    let date_to = required_date(query.date_to.as_deref())?;
    let limit = query.limit.clamp(1, MAX_LIMIT);

    let result = a041_ym_shows_sales_daily::repository::product_rows_for_period(
        date_from,
        date_to,
        query.connection_id.as_deref(),
        limit,
        query.offset,
    )
    .await
    .map_err(|error| {
        tracing::error!("[ext-api] ym-funnel list error: {}", error);
        ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
    })?;

    let items = result
        .rows
        .into_iter()
        .map(|row| FunnelRow {
            date: row.date,
            connection_id: row.connection_id,
            connection_name: row.connection_name,
            organization_name: row.organization_name,
            campaign_id: row.campaign_id,
            offer_id: row.offer_id,
            offer_name: row.offer_name,
            marketplace_product_ref: row.marketplace_product_ref,
            nomenclature_ref: row.nomenclature_ref,
            brand_name: row.brand_name,
            category_id: row.category_id,
            category_name: row.category_name,
            shows: row.shows,
            clicks: row.clicks,
            cart_count: row.cart_count,
            order_count: row.order_count,
            order_sum: row.order_sum,
            delivered_count: row.delivered_count,
            delivered_sum: row.delivered_sum,
            cancel_count: row.cancel_count,
            return_count: row.return_count,
            click_through_conversion: row.click_through_conversion,
            add_to_cart_conversion: row.add_to_cart_conversion,
            cart_to_order_conversion: row.cart_to_order_conversion,
        })
        .collect();

    Ok(Json(FunnelResponse {
        items,
        total: result.total,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn required_period_values_reject_missing_and_empty_dates() {
        // Сравниваем по статусу, а не по значению: `ApiError` умышленно не
        // `PartialEq` — у ошибки нет осмысленного равенства, есть код ответа.
        for absent in [None, Some("")] {
            let status = required_date(absent).map(|_| ()).unwrap_err().status();
            assert_eq!(status, axum::http::StatusCode::BAD_REQUEST);
        }
        assert_eq!(
            required_date(Some("2026-08-01")).expect("дата задана"),
            "2026-08-01"
        );
    }

    #[test]
    fn limit_is_clamped_to_supported_range() {
        assert_eq!(0usize.clamp(1, MAX_LIMIT), 1);
        assert_eq!(usize::MAX.clamp(1, MAX_LIMIT), MAX_LIMIT);
    }
}
