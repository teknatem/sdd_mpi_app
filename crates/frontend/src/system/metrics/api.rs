//! Запросы страницы метрик. Роуты admin-only: не-админ получит 403, и это
//! правильный ответ — фронт лишь не показывает ему пункт меню.

use contracts::system::metrics::{CollectResultDto, MetricSeriesDto, ProjectMetricsDto};

use crate::shared::api_utils::{get_json, post_json};

pub async fn get_latest() -> Result<ProjectMetricsDto, String> {
    get_json("/api/system/metrics/latest").await
}

/// Ряды для спарклайнов. Ключи фильтруются на бэкенде по каталогу, поэтому
/// сюда можно передавать всё, что показано на странице.
pub async fn get_series(keys: &[String], limit: u64) -> Result<Vec<MetricSeriesDto>, String> {
    if keys.is_empty() {
        return Ok(Vec::new());
    }
    get_json(&format!(
        "/api/system/metrics/series?keys={}&limit={limit}",
        keys.join(",")
    ))
    .await
}

pub async fn collect_now() -> Result<CollectResultDto, String> {
    post_json("/api/system/metrics/collect", &()).await
}
