//! External BI API handler for the WB finance report (p903) — for internal
//! Power BI consumers. Returns the raw, native-WB report rows for a period
//! (the full `reportDetailByPeriod` object, preserved verbatim in `extra`),
//! plus `connection_mp_ref` / `organization_ref` so cabinets stay distinguishable.
//! Authentication is handled by the `check_api_key` middleware (X-Api-Key header).

use crate::shared::error::ApiError;
use axum::{extract::Query, Json};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::projections::p903_wb_finance_report;

/// Hard cap on rows per request (matches repository MAX_LIMIT).
const MAX_LIMIT: i32 = 20_000;

fn default_limit() -> i32 {
    5_000
}

// ─────────────────────────────────────────────
// Query params
// ─────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct FinanceQuery {
    /// Начало периода по `rr_dt`, `YYYY-MM-DD` (обязательно).
    pub date_from: Option<String>,
    /// Конец периода по `rr_dt`, `YYYY-MM-DD`, включительно (обязательно).
    pub date_to: Option<String>,
    /// Фильтр по кабинету WB (= `connection_mp_ref`), опционально.
    pub connection_id: Option<String>,
    #[serde(default = "default_limit")]
    pub limit: i32,
    #[serde(default)]
    pub offset: i32,
}

#[derive(Debug, Serialize)]
pub struct FinanceResponse {
    /// Сырые строки отчёта в native-формате WB (+ `connection_mp_ref`, `organization_ref`).
    pub items: Vec<Value>,
    /// Общее число строк за период (ограниченный подсчёт).
    pub total: i32,
    pub limit: i32,
    pub offset: i32,
}

// ─────────────────────────────────────────────
// Handler
// ─────────────────────────────────────────────

/// Финансовый отчёт WB (p903) за период — сырые native-строки WB.
///
/// GET /api/ext/v1/wb-finance-report
///   ?date_from=2026-06-01&date_to=2026-06-30   (обязательно)
///   &connection_id=<uuid>                       (опц.)
///   &limit=5000&offset=0                        (опц.)
///
/// Заголовок: `X-Api-Key: <ключ>`.
pub async fn list_finance_report(
    Query(q): Query<FinanceQuery>,
) -> Result<Json<FinanceResponse>, ApiError> {
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
    let offset = q.offset.max(0);

    let (models, total) = p903_wb_finance_report::repository::list_raw_for_ext(
        date_from,
        date_to,
        q.connection_id.clone(),
        limit,
        offset,
    )
    .await
    .map_err(|e| {
        tracing::error!("[ext-api] wb-finance-report error: {}", e);
        ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
    })?;

    let items = models
        .into_iter()
        .map(|m| {
            // Основной путь: сырой native-ряд WB из `extra`.
            let parsed = m
                .extra
                .as_deref()
                .and_then(|e| serde_json::from_str::<Value>(e).ok());
            let mut obj: Map<String, Value> = match parsed {
                Some(Value::Object(o)) => o,
                // Fallback для легаси-строк без `extra`: продвинутые колонки (WB-именами),
                // минус служебное поле `extra`.
                _ => match serde_json::to_value(&m) {
                    Ok(Value::Object(mut o)) => {
                        o.remove("extra");
                        o
                    }
                    _ => Map::new(),
                },
            };
            // Идентификация кабинета/организации (не пересекается с native-полями WB).
            obj.insert(
                "connection_mp_ref".to_string(),
                Value::String(m.connection_mp_ref.clone()),
            );
            obj.insert(
                "organization_ref".to_string(),
                Value::String(m.organization_ref.clone()),
            );
            Value::Object(obj)
        })
        .collect();

    Ok(Json(FinanceResponse {
        items,
        total,
        limit,
        offset,
    }))
}
