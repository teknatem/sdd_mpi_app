//! Сводка качества работы LLM-подсистемы для дашборда d407.
//!
//! Отдаётся одним ответом: страница показывает связанные срезы (инструменты,
//! вердикты, причины провалов, ценность статей KB), и разбивать их на отдельные
//! запросы значило бы показывать картину, собранную из разных моментов времени.

use axum::{extract::Query, Json};
use contracts::dashboards::d407_llm_quality::LlmQualityOverview;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct OverviewQuery {
    #[serde(default = "default_days")]
    pub days: i64,
}

fn default_days() -> i64 {
    30
}

/// GET /api/llm-quality/overview?days=30
pub async fn overview(
    Query(query): Query<OverviewQuery>,
) -> Result<Json<LlmQualityOverview>, axum::http::StatusCode> {
    let raw = crate::shared::llm::verdicts::quality_overview(query.days)
        .await
        .map_err(|error| {
            tracing::error!("llm quality overview: {error}");
            axum::http::StatusCode::INTERNAL_SERVER_ERROR
        })?;

    // Строки приходят из sqlx-материализатора нетипизированными; типизируем на
    // границе API, чтобы расхождение имён колонок и DTO падало здесь, а не
    // «пустым дашбордом» без объяснений.
    serde_json::from_value(raw).map(Json).map_err(|error| {
        tracing::error!("llm quality overview: несовпадение формы данных: {error}");
        axum::http::StatusCode::INTERNAL_SERVER_ERROR
    })
}
