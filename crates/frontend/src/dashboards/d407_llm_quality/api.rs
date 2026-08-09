use contracts::dashboards::d407_llm_quality::LlmQualityOverview;
use gloo_net::http::Request;

pub async fn get_llm_quality_overview(days: i64) -> Result<LlmQualityOverview, String> {
    let response = Request::get(&format!("/api/llm-quality/overview?days={days}"))
        .send()
        .await
        .map_err(|error| format!("Request failed: {error}"))?;
    if !response.ok() {
        return Err(format!("HTTP {}", response.status()));
    }
    response
        .json()
        .await
        .map_err(|error| format!("Failed to parse response: {error}"))
}
