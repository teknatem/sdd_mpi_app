//! External BI API for 1C nomenclature (a004) and its marketplace SKU bridge
//! (a007). Grain of `/nomenclature` is one 1C item; marketplace codes live in
//! `/nomenclature-skus` so Power BI can relate them without duplicating
//! attributes. Authentication: `check_api_key` (`X-Api-Key`).

use crate::domain::a004_nomenclature;
use crate::shared::error::ApiError;
use axum::{extract::Query, Json};
use serde::{Deserialize, Serialize};

const MAX_LIMIT: usize = 50_000;

fn default_limit() -> usize {
    50_000
}

fn today_iso() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

fn parse_iso_date(value: Option<&str>) -> Result<String, ApiError> {
    let raw = value
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("")
        .to_string();
    if raw.is_empty() {
        return Ok(today_iso());
    }
    chrono::NaiveDate::parse_from_str(&raw, "%Y-%m-%d")
        .map(|_| raw)
        .map_err(|_| ApiError::from(axum::http::StatusCode::BAD_REQUEST))
}

#[derive(Debug, Deserialize)]
pub struct NomenclatureQuery {
    /// Дилерская цена на дату `YYYY-MM-DD` (последняя ненулевая `p906` ≤ date).
    /// Пусто — сегодня.
    pub date: Option<String>,
    #[serde(default = "default_limit")]
    pub limit: usize,
    #[serde(default)]
    pub offset: usize,
}

#[derive(Debug, Deserialize)]
pub struct SkuQuery {
    #[serde(default = "default_limit")]
    pub limit: usize,
    #[serde(default)]
    pub offset: usize,
}

#[derive(Debug, Serialize)]
pub struct NomenclatureRow {
    pub id: String,
    pub name: String,
    pub article: String,
    pub category: String,
    pub line: String,
    pub color: String,
    pub size: String,
    pub format: String,
    pub sink: String,
    /// Комплект (сборка). `false` — разбор.
    pub is_assembly: bool,
    /// Последняя ненулевая дилерская цена на запрошенную дату; `null` если нет.
    pub dealer_price: Option<f64>,
}

#[derive(Debug, Serialize)]
pub struct NomenclatureResponse {
    pub items: Vec<NomenclatureRow>,
    pub total: usize,
}

#[derive(Debug, Serialize)]
pub struct NomenclatureSkuRow {
    pub nomenclature_id: String,
    /// `WB` / `YM` / `OZON` / …
    pub marketplace: String,
    /// Код товара на площадке: WB `nm_id`, YM `shop_sku`.
    pub sku: String,
    /// Числовой `nm_id` для связи с воронкой/остатками WB. `null` для других МП.
    pub wb_nm_id: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct NomenclatureSkuResponse {
    pub items: Vec<NomenclatureSkuRow>,
    pub total: usize,
}

/// Справочник номенклатуры 1С (только товары, сопоставленные с маркетплейсом).
///
/// GET /api/ext/v1/nomenclature
///   ?date=2026-08-26        (опц.; пусто → сегодня)
///   &limit=50000&offset=0   (опц.)
///
/// Заголовок: `X-Api-Key: <ключ>`.
pub async fn list_nomenclature(
    Query(q): Query<NomenclatureQuery>,
) -> Result<Json<NomenclatureResponse>, ApiError> {
    let as_of = parse_iso_date(q.date.as_deref())?;
    let limit = q.limit.clamp(1, MAX_LIMIT);

    let result = a004_nomenclature::repository::bi_nomenclature_rows(&as_of, limit, q.offset)
        .await
        .map_err(|e| {
            tracing::error!("[ext-api] nomenclature error: {}", e);
            ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        })?;

    let items = result
        .rows
        .into_iter()
        .map(|r| NomenclatureRow {
            id: r.id,
            name: r.name,
            article: r.article,
            category: r.category,
            line: r.line,
            color: r.color,
            size: r.size,
            format: r.format,
            sink: r.sink,
            is_assembly: r.is_assembly,
            dealer_price: r.dealer_price,
        })
        .collect();

    Ok(Json(NomenclatureResponse {
        items,
        total: result.total,
    }))
}

/// Мост номенклатура 1С ↔ SKU маркетплейса. Одна строка — один код площадки.
///
/// GET /api/ext/v1/nomenclature-skus
///   ?limit=50000&offset=0
///
/// Заголовок: `X-Api-Key: <ключ>`.
pub async fn list_nomenclature_skus(
    Query(q): Query<SkuQuery>,
) -> Result<Json<NomenclatureSkuResponse>, ApiError> {
    let limit = q.limit.clamp(1, MAX_LIMIT);

    let result = a004_nomenclature::repository::bi_nomenclature_sku_rows(limit, q.offset)
        .await
        .map_err(|e| {
            tracing::error!("[ext-api] nomenclature-skus error: {}", e);
            ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        })?;

    let items = result
        .rows
        .into_iter()
        .map(|r| NomenclatureSkuRow {
            nomenclature_id: r.nomenclature_id,
            marketplace: r.marketplace,
            sku: r.sku,
            wb_nm_id: r.wb_nm_id,
        })
        .collect();

    Ok(Json(NomenclatureSkuResponse {
        items,
        total: result.total,
    }))
}
