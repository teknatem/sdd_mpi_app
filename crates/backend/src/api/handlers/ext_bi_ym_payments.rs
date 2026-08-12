//! External BI API handler for the YM payment report (p907) — for internal
//! Power BI consumers. Полный аналог `ext_bi_wb_finance` для Яндекс Маркета:
//! отдаёт строки отчёта `united-netting` за период в том виде, в каком они
//! пришли из CSV, плюс `connection_mp_ref` / `organization_ref` (кабинет и
//! организация) и служебный `loaded_at_utc` — по нему BI видит строки, которые
//! Маркет дозаполнил задним числом (`bank_order_id`/`act_id` приходят спустя
//! недели, фильтра «изменённые с» у отчёта нет).
//!
//! Внутренние поля связи (`id`, `marketplace_product_ref`, `marketplace_order_ref`,
//! `nomenclature_ref`, `payload_version`) наружу не отдаются — за пределами
//! приложения они бессмысленны.
//! Authentication is handled by the `check_api_key` middleware (X-Api-Key header).

use axum::{extract::Query, Json};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::projections::p907_ym_payment_report;

/// Hard cap on rows per request (matches repository MAX_LIMIT).
const MAX_LIMIT: i32 = 20_000;

/// Служебные поля модели p907, не имеющие смысла за пределами приложения.
const INTERNAL_FIELDS: [&str; 5] = [
    "id",
    "marketplace_product_ref",
    "marketplace_order_ref",
    "nomenclature_ref",
    "payload_version",
];

fn default_limit() -> i32 {
    5_000
}

// ─────────────────────────────────────────────
// Query params
// ─────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct PaymentsQuery {
    /// Начало периода по `transaction_date`, `YYYY-MM-DD` (обязательно).
    pub date_from: Option<String>,
    /// Конец периода по `transaction_date`, `YYYY-MM-DD`, включительно (обязательно).
    pub date_to: Option<String>,
    /// Фильтр по кабинету YM (= `connection_mp_ref`), опционально.
    pub connection_id: Option<String>,
    #[serde(default = "default_limit")]
    pub limit: i32,
    #[serde(default)]
    pub offset: i32,
}

#[derive(Debug, Serialize)]
pub struct PaymentsResponse {
    /// Строки отчёта в native-формате YM (+ `connection_mp_ref`, `organization_ref`).
    pub items: Vec<Value>,
    /// Общее число строк за период (ограниченный подсчёт).
    pub total: i32,
    pub limit: i32,
    pub offset: i32,
}

// ─────────────────────────────────────────────
// Handler
// ─────────────────────────────────────────────

/// Отчёт по платежам Яндекс Маркета (p907) за период — сырые native-строки YM.
///
/// GET /api/ext/v1/ym-payment-report
///   ?date_from=2026-06-01&date_to=2026-06-30   (обязательно)
///   &connection_id=<uuid>                       (опц.)
///   &limit=5000&offset=0                        (опц.)
///
/// Заголовок: `X-Api-Key: <ключ>`.
pub async fn list_payment_report(
    Query(q): Query<PaymentsQuery>,
) -> Result<Json<PaymentsResponse>, axum::http::StatusCode> {
    let date_from = q
        .date_from
        .as_deref()
        .filter(|s| !s.is_empty())
        .ok_or(axum::http::StatusCode::BAD_REQUEST)?;
    let date_to = q
        .date_to
        .as_deref()
        .filter(|s| !s.is_empty())
        .ok_or(axum::http::StatusCode::BAD_REQUEST)?;

    let limit = q.limit.clamp(1, MAX_LIMIT);
    let offset = q.offset.max(0);

    let (models, total) = p907_ym_payment_report::repository::list_raw_for_ext(
        date_from,
        date_to,
        q.connection_id.clone(),
        limit,
        offset,
    )
    .await
    .map_err(|e| {
        tracing::error!("[ext-api] ym-payment-report error: {}", e);
        axum::http::StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let items = models
        .into_iter()
        .map(|m| {
            let mut obj: Map<String, Value> = match serde_json::to_value(&m) {
                Ok(Value::Object(o)) => o,
                _ => Map::new(),
            };
            for field in INTERNAL_FIELDS {
                obj.remove(field);
            }
            Value::Object(obj)
        })
        .collect();

    Ok(Json(PaymentsResponse {
        items,
        total,
        limit,
        offset,
    }))
}
