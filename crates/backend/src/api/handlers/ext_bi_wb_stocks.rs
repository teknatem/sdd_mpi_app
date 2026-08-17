//! External BI API handler for WB stock balances (a037) — for internal Power BI
//! consumers. Emits a flat JSON array of one row per `nm_id`, at product level
//! (WB warehouses + seller warehouses; no per-warehouse / barcode / in-transit
//! detail — that is not stored). Latest snapshot by default, or the snapshot
//! `<= date` when `date` is given.
//! Authentication is handled by the `check_api_key` middleware (X-Api-Key header).

use crate::shared::error::ApiError;
use axum::{extract::Query, Json};
use serde::{Deserialize, Serialize};

use crate::domain::a037_wb_product_snapshot;

/// Hard cap on the number of rows a single request may return.
const MAX_LIMIT: usize = 50_000;

fn default_limit() -> usize {
    50_000
}

// ─────────────────────────────────────────────
// Query params
// ─────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct StocksQuery {
    /// Фильтр по кабинету WB (a006_connection_mp), опционально. Пусто → все кабинеты.
    pub connection_id: Option<String>,
    /// Остаток на дату `YYYY-MM-DD` (берётся снимок `<= date`). Пусто → последний снимок.
    pub date: Option<String>,
    #[serde(default = "default_limit")]
    pub limit: usize,
    #[serde(default)]
    pub offset: usize,
}

// ─────────────────────────────────────────────
// Response DTO
// ─────────────────────────────────────────────

/// Остаток по товару (`nm_id`) на дату снимка.
#[derive(Debug, Serialize)]
pub struct StockRow {
    /// Фактическая дата снимка (`<=` запрошенной `date`), `YYYY-MM-DD`.
    pub snapshot_date: String,
    pub connection_id: String,
    pub connection_name: Option<String>,
    pub organization_name: Option<String>,
    pub nm_id: i64,
    pub vendor_code: String,
    pub brand_name: String,
    pub subject_id: i64,
    pub subject_name: String,
    pub title: String,
    /// Остаток на складах WB, шт.
    pub stock_wb: i64,
    /// Остаток на складах продавца, шт.
    pub stock_mp: i64,
    /// Сумма остатков.
    pub stock_balance_sum: f64,
}

#[derive(Debug, Serialize)]
pub struct StockResponse {
    pub items: Vec<StockRow>,
    /// Общее число строк (до пагинации).
    pub total: usize,
}

// ─────────────────────────────────────────────
// Handler
// ─────────────────────────────────────────────

/// Остатки WB на уровне товара: последние или на заданную дату.
///
/// GET /api/ext/v1/wb-stocks
///   ?connection_id=<uuid>   (опц.; пусто → все кабинеты)
///   &date=2026-07-10        (опц.; пусто → последний снимок)
///   &limit=50000&offset=0   (опц.)
///
/// Заголовок: `X-Api-Key: <ключ>`.
pub async fn list_stocks(Query(q): Query<StocksQuery>) -> Result<Json<StockResponse>, ApiError> {
    let limit = q.limit.clamp(1, MAX_LIMIT);

    let result = a037_wb_product_snapshot::repository::stock_rows(
        q.connection_id.as_deref(),
        q.date.as_deref(),
        limit,
        q.offset,
    )
    .await
    .map_err(|e| {
        tracing::error!("[ext-api] wb-stocks error: {}", e);
        ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
    })?;

    let items = result
        .rows
        .into_iter()
        .map(|r| StockRow {
            snapshot_date: r.snapshot_date,
            connection_id: r.connection_id,
            connection_name: r.connection_name,
            organization_name: r.organization_name,
            nm_id: r.nm_id,
            vendor_code: r.vendor_code,
            brand_name: r.brand_name,
            subject_id: r.subject_id,
            subject_name: r.subject_name,
            title: r.title,
            stock_wb: r.stock_wb,
            stock_mp: r.stock_mp,
            stock_balance_sum: r.stock_balance_sum,
        })
        .collect();

    Ok(Json(StockResponse {
        items,
        total: result.total,
    }))
}
