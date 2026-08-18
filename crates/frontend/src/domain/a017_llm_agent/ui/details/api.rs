//! LLM Agent Details - API Layer
//!
//! DTOs and API functions for LLM Agent details

use crate::shared::api_utils::api_base;
use contracts::domain::a017_llm_agent::aggregate::LlmAgent;
use contracts::domain::a038_llm_connection::aggregate::LlmConnection;
use serde::Deserialize;
use wasm_bindgen::JsCast;
use web_sys::{Request, RequestInit, RequestMode, Response};

/// Один навык из каталога специализации.
#[derive(Deserialize, Clone)]
pub struct SkillDto {
    pub id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub description: String,
}

/// Ответ /api/a017-llm-agent/skills: core (по умолчанию) + extended (по запросу).
#[derive(Deserialize)]
pub struct EmployeeSkillsResponse {
    #[serde(default)]
    pub core: Vec<SkillDto>,
    #[serde(default)]
    pub extended: Vec<SkillDto>,
}

/// Простой GET → текст ответа (общий помощник).
async fn get_text(url: &str) -> Result<String, String> {
    let opts = RequestInit::new();
    opts.set_method("GET");
    opts.set_mode(RequestMode::Cors);
    let request = Request::new_with_str_and_init(url, &opts).map_err(|e| format!("{e:?}"))?;
    request
        .headers()
        .set("Accept", "application/json")
        .map_err(|e| format!("{e:?}"))?;
    let window = web_sys::window().ok_or_else(|| "no window".to_string())?;
    let resp_value = wasm_bindgen_futures::JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(|e| format!("{e:?}"))?;
    let resp: Response = resp_value.dyn_into().map_err(|e| format!("{e:?}"))?;
    if !resp.ok() {
        return Err(format!("HTTP {}", resp.status()));
    }
    let text = wasm_bindgen_futures::JsFuture::from(resp.text().map_err(|e| format!("{e:?}"))?)
        .await
        .map_err(|e| format!("{e:?}"))?;
    text.as_string().ok_or_else(|| "bad text".to_string())
}

/// Список технических подключений a038 (для селекта «Подключение»).
pub async fn fetch_connections() -> Result<Vec<LlmConnection>, String> {
    let url = format!("{}/api/a038-llm-connection", api_base());
    let text = get_text(&url).await?;
    serde_json::from_str::<Vec<LlmConnection>>(&text).map_err(|e| format!("{e}"))
}

/// Навыки (core/extended) для специализации.
pub async fn fetch_employee_skills(agent_type: &str) -> Result<EmployeeSkillsResponse, String> {
    let url = format!(
        "{}/api/a017-llm-agent/skills?agent_type={}",
        api_base(),
        agent_type
    );
    let text = get_text(&url).await?;
    serde_json::from_str::<EmployeeSkillsResponse>(&text).map_err(|e| format!("{e}"))
}

/// Fetch LLM agent by ID from API
pub async fn fetch_agent(id: &str) -> Result<LlmAgent, String> {
    let opts = RequestInit::new();
    opts.set_method("GET");
    opts.set_mode(RequestMode::Cors);

    let url = format!("{}/api/a017-llm-agent/{}", api_base(), id);
    let request = Request::new_with_str_and_init(&url, &opts).map_err(|e| format!("{e:?}"))?;
    request
        .headers()
        .set("Accept", "application/json")
        .map_err(|e| format!("{e:?}"))?;

    let window = web_sys::window().ok_or_else(|| "no window".to_string())?;
    let resp_value = wasm_bindgen_futures::JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(|e| format!("{e:?}"))?;
    let resp: Response = resp_value.dyn_into().map_err(|e| format!("{e:?}"))?;

    if !resp.ok() {
        return Err(format!("HTTP {}", resp.status()));
    }

    let text = wasm_bindgen_futures::JsFuture::from(resp.text().map_err(|e| format!("{e:?}"))?)
        .await
        .map_err(|e| format!("{e:?}"))?;
    let text: String = text.as_string().ok_or_else(|| "bad text".to_string())?;
    let agent: LlmAgent = serde_json::from_str(&text).map_err(|e| format!("{e}"))?;

    Ok(agent)
}

/// Save (create or update) LLM agent via API
pub async fn save_agent(dto: serde_json::Value) -> Result<(), String> {
    let opts = RequestInit::new();
    opts.set_method("POST");
    opts.set_mode(RequestMode::Cors);

    let body = serde_json::to_string(&dto).map_err(|e| format!("{e}"))?;
    opts.set_body(&wasm_bindgen::JsValue::from_str(&body));

    let url = format!("{}/api/a017-llm-agent", api_base());
    let request = Request::new_with_str_and_init(&url, &opts).map_err(|e| format!("{e:?}"))?;
    request
        .headers()
        .set("Accept", "application/json")
        .map_err(|e| format!("{e:?}"))?;
    request
        .headers()
        .set("Content-Type", "application/json")
        .map_err(|e| format!("{e:?}"))?;

    let window = web_sys::window().ok_or_else(|| "no window".to_string())?;
    let resp_value = wasm_bindgen_futures::JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(|e| format!("{e:?}"))?;
    let resp: Response = resp_value.dyn_into().map_err(|e| format!("{e:?}"))?;

    if !resp.ok() {
        return Err(format!("HTTP {}", resp.status()));
    }

    Ok(())
}
