//! External BI API handler for the WB sales funnel (a036) — for internal
//! Power BI consumers. Emits a flat JSON array of one row per `nm_id × date`.
//! Authentication is handled by the `check_api_key` middleware (X-Api-Key header).

use crate::shared::error::ApiError;
use axum::{extract::Query, Json};
use serde::{Deserialize, Serialize};

use crate::domain::a036_wb_sales_funnel_daily;

/// Hard cap on the number of rows a single request may return.
const MAX_LIMIT: usize = 50_000;

fn default_limit() -> usize {
    5_000
}

// ─────────────────────────────────────────────
// Query params
// ─────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct FunnelQuery {
    /// Начало периода, ISO `YYYY-MM-DD` (обязательно).
    pub date_from: Option<String>,
    /// Конец периода, ISO `YYYY-MM-DD`, включительно (обязательно).
    pub date_to: Option<String>,
    /// Фильтр по кабинету WB (a006_connection_mp), опционально.
    pub connection_id: Option<String>,
    #[serde(default = "default_limit")]
    pub limit: usize,
    #[serde(default)]
    pub offset: usize,
}

// ─────────────────────────────────────────────
// Response DTO — поля в порядке стадий воронки
// ─────────────────────────────────────────────

/// Одна строка воронки: товар (`nm_id`) за конкретный день.
#[derive(Debug, Serialize)]
pub struct FunnelRow {
    /// Дата воронки, `YYYY-MM-DD`.
    pub date: String,
    pub connection_id: String,
    pub connection_name: Option<String>,
    pub organization_name: Option<String>,
    pub currency: String,
    pub nm_id: i64,
    pub vendor_code: String,
    pub brand_name: String,
    pub subject_id: i64,
    pub subject_name: String,
    pub title: String,
    /// Переходы в карточку товара.
    pub open_count: i64,
    /// Добавления в «Отложенные».
    pub add_to_wishlist_count: i64,
    /// Положили в корзину, шт.
    pub cart_count: i64,
    /// Заказали товаров, шт.
    pub order_count: i64,
    /// Заказали на сумму.
    pub order_sum: f64,
    /// Выкупили товаров, шт.
    pub buyout_count: i64,
    /// Выкупили на сумму.
    pub buyout_sum: f64,
    /// Отменили товаров, шт. по счётчику воронки WB. `null` — счётчика в источнике
    /// не было (данные до подключения отмен); это N/A, а не ноль.
    pub cancel_count: Option<i64>,
    /// Отменили на сумму. `null` — см. `cancel_count`.
    pub cancel_sum: Option<f64>,
    /// Процент выкупа.
    pub buyout_percent: f64,
    /// Конверсия в корзину, %.
    pub add_to_cart_conversion: f64,
    /// Конверсия в заказ, %.
    pub cart_to_order_conversion: f64,
}

#[derive(Debug, Serialize)]
pub struct FunnelResponse {
    pub items: Vec<FunnelRow>,
    /// Общее число строк за период (до пагинации).
    pub total: usize,
}

// ─────────────────────────────────────────────
// Handler
// ─────────────────────────────────────────────

/// Выгрузка воронки продаж WB за период — плоские строки `nm_id × дата`.
///
/// GET /api/ext/v1/wb-sales-funnel
///   ?date_from=2026-07-08&date_to=2026-07-14   (обязательно)
///   &connection_id=<uuid>                      (опционально)
///   &limit=5000&offset=0                       (опционально)
///
/// Заголовок: `X-Api-Key: <ключ>` (см. `[external_api].api_key` в config.toml).
pub async fn list_funnel(Query(q): Query<FunnelQuery>) -> Result<Json<FunnelResponse>, ApiError> {
    let date_from = q
        .date_from
        .as_deref()
        .filter(|s| !s.is_empty())
        .ok_or(ApiError::from(axum::http::StatusCode::BAD_REQUEST))?;
    let date_to = q
        .date_to
        .as_deref()
        .filter(|s| !s.is_empty())
        .ok_or(ApiError::from(axum::http::StatusCode::BAD_REQUEST))?;

    let limit = q.limit.clamp(1, MAX_LIMIT);

    let result = a036_wb_sales_funnel_daily::repository::product_rows_for_period(
        date_from,
        date_to,
        q.connection_id.as_deref(),
        limit,
        q.offset,
    )
    .await
    .map_err(|e| {
        tracing::error!("[ext-api] wb-funnel list error: {}", e);
        ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
    })?;

    let items = result
        .rows
        .into_iter()
        .map(|r| FunnelRow {
            date: r.date,
            connection_id: r.connection_id,
            connection_name: r.connection_name,
            organization_name: r.organization_name,
            currency: r.currency,
            nm_id: r.nm_id,
            vendor_code: r.vendor_code,
            brand_name: r.brand_name,
            subject_id: r.subject_id,
            subject_name: r.subject_name,
            title: r.title,
            open_count: r.open_count,
            add_to_wishlist_count: r.add_to_wishlist_count,
            cart_count: r.cart_count,
            order_count: r.order_count,
            order_sum: r.order_sum,
            buyout_count: r.buyout_count,
            buyout_sum: r.buyout_sum,
            cancel_count: r.cancel_count,
            cancel_sum: r.cancel_sum,
            buyout_percent: r.buyout_percent,
            add_to_cart_conversion: r.add_to_cart_conversion,
            cart_to_order_conversion: r.cart_to_order_conversion,
        })
        .collect();

    Ok(Json(FunnelResponse {
        items,
        total: result.total,
    }))
}
