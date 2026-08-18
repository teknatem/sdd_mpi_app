use crate::shared::api_utils::api_base;
use contracts::domain::a019_llm_artifact::aggregate::LlmArtifact;
use gloo_net::http::Request;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmArtifactDto {
    pub id: Option<String>,
    pub code: Option<String>,
    pub description: String,
    pub comment: Option<String>,
    pub chat_id: String,
    pub agent_id: String,
    pub sql_query: String,
    pub query_params: Option<String>,
    pub visualization_config: Option<String>,
}

pub async fn fetch_artifact(id: &str) -> Result<LlmArtifact, String> {
    let url = format!("{}/api/a019-llm-artifact/{}", api_base(), id);

    let response = Request::get(&url)
        .send()
        .await
        .map_err(|e| format!("Failed to fetch: {}", e))?;

    if response.status() != 200 {
        return Err(format!("Server error: {}", response.status()));
    }

    let text = response
        .text()
        .await
        .map_err(|e| format!("Failed to read response: {}", e))?;

    serde_json::from_str::<LlmArtifact>(&text)
        .map_err(|e| format!("Failed to parse response: {}", e))
}

pub async fn update_artifact(dto: LlmArtifactDto) -> Result<(), String> {
    let url = format!("{}/api/a019-llm-artifact", api_base());

    let body = serde_json::to_string(&dto).map_err(|e| format!("Failed to serialize: {}", e))?;

    let response = Request::post(&url)
        .header("Content-Type", "application/json")
        .body(body)
        .map_err(|e| format!("Failed to build request: {}", e))?
        .send()
        .await
        .map_err(|e| format!("Failed to send: {}", e))?;

    if response.status() != 200 {
        return Err(format!("Server error: {}", response.status()));
    }

    Ok(())
}
