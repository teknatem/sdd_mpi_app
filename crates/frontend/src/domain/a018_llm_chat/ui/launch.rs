//! Запуск чата с контекстом страницы — общий для всех точек входа.
//!
//! Раньше вся цепочка (подключение → чат → контекст → вкладка) жила приватно
//! внутри кнопки в шапке. Как только чат понадобилось открывать ещё и со
//! страницы — с готовым вопросом, — это стало общим механизмом: страница знает,
//! что спросить, но не должна знать про выбор подключения и порядок вызовов.

use crate::layout::global_context::AppGlobalContext;
use crate::shared::api_utils::api_base;

pub const CHAT_DETAIL_PREFIX: &str = "a018_llm_chat_details_";

/// Создать чат, прикрепить к нему контекст страницы и открыть вкладку.
///
/// `first_message` — вопрос, который страница задаёт за пользователя. Он не
/// отправляется отсюда: страница деталей чата забирает его из состояния формы
/// при загрузке и шлёт сама, потому что только там уже есть живой view-model с
/// обработкой стриминга.
pub fn launch_chat_with_context(
    ctx: AppGlobalContext,
    page_key: String,
    label: String,
    first_message: Option<String>,
) {
    wasm_bindgen_futures::spawn_local(async move {
        let result: Result<String, String> = async {
            let (agent_id, model) = fetch_default_agent().await?;
            let desc = derive_title(&label);
            let chat_id = create_chat(&desc, &agent_id, &model).await?;
            // Контекст не обязателен: с вкладки без полезного контекста чат тоже
            // должен открываться.
            if !page_key.is_empty() {
                add_context(&chat_id, &page_key, &label, true).await?;
            }
            Ok(chat_id)
        }
        .await;

        match result {
            Ok(chat_id) => {
                if let Some(message) = first_message {
                    ctx.set_form_state(
                        super::pending_first_message_key(&chat_id),
                        serde_json::Value::String(message),
                    );
                }
                let key = format!("{}{}", CHAT_DETAIL_PREFIX, chat_id);
                ctx.open_tab(&key, "AI чат");
            }
            Err(e) => leptos::logging::log!("AI чат: ошибка создания: {}", e),
        }
    });
}

/// Заголовок чата из заголовка страницы.
pub fn derive_title(label: &str) -> String {
    let l = label.trim();
    if l.is_empty() {
        return "AI чат".to_string();
    }
    let max = 60;
    let chars: Vec<char> = l.chars().collect();
    let base = if chars.len() > max {
        let t: String = chars.into_iter().take(max).collect();
        format!("{}…", t.trim_end())
    } else {
        l.to_string()
    };
    format!("AI: {}", base)
}

// ─── API ─────────────────────────────────────────────────────────────────────

async fn http_request(method: &str, url: &str, body: Option<String>) -> Result<String, String> {
    use wasm_bindgen::JsCast;
    use web_sys::{Request, RequestInit, RequestMode, Response};

    let opts = RequestInit::new();
    opts.set_method(method);
    opts.set_mode(RequestMode::Cors);
    if let Some(b) = &body {
        opts.set_body(&wasm_bindgen::JsValue::from_str(b));
    }

    let request = Request::new_with_str_and_init(url, &opts).map_err(|e| format!("{e:?}"))?;
    request
        .headers()
        .set("Accept", "application/json")
        .map_err(|e| format!("{e:?}"))?;
    if body.is_some() {
        request
            .headers()
            .set("Content-Type", "application/json")
            .map_err(|e| format!("{e:?}"))?;
    }

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

/// Подключение по умолчанию: основное (is_primary), иначе первое. Возвращает (id, model).
async fn fetch_default_agent() -> Result<(String, String), String> {
    let url = format!("{}/api/a038-llm-connection", api_base());
    let text = http_request("GET", &url, None).await?;
    let agents: Vec<serde_json::Value> = serde_json::from_str(&text).map_err(|e| format!("{e}"))?;
    if agents.is_empty() {
        return Err("Нет доступных LLM-подключений".to_string());
    }
    let chosen = agents
        .iter()
        .find(|a| {
            a.get("is_primary")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
        })
        .or_else(|| agents.first())
        .unwrap();
    let id = chosen
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "agent without id".to_string())?
        .to_string();
    let model = chosen
        .get("model_name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    Ok((id, model))
}

async fn create_chat(description: &str, agent_id: &str, model: &str) -> Result<String, String> {
    let model_value = if model.trim().is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::Value::String(model.to_string())
    };
    let dto = serde_json::json!({
        "id": serde_json::Value::Null,
        "code": serde_json::Value::Null,
        "description": description,
        "comment": serde_json::Value::Null,
        "agent_id": agent_id,
        "model_name": model_value,
    });
    let url = format!("{}/api/a018-llm-chat", api_base());
    let text = http_request("POST", &url, Some(dto.to_string())).await?;
    let v: serde_json::Value = serde_json::from_str(&text).map_err(|e| format!("{e}"))?;
    v.get("id")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| "No chat id in response".to_string())
}

pub async fn add_context(
    chat_id: &str,
    page_key: &str,
    label: &str,
    with_session_snapshot: bool,
) -> Result<(), String> {
    let dto = serde_json::json!({
        "page_key": page_key,
        "label": label,
        "with_session_snapshot": with_session_snapshot,
    });
    let url = format!("{}/api/a018-llm-chat/{}/context", api_base(), chat_id);
    http_request("POST", &url, Some(dto.to_string())).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn title_falls_back_when_label_is_empty() {
        assert_eq!(derive_title("   "), "AI чат");
    }

    #[test]
    fn title_prefixes_the_page_name() {
        assert_eq!(derive_title("Метрики проекта"), "AI: Метрики проекта");
    }

    #[test]
    fn title_truncates_long_labels_without_splitting_chars() {
        let long = "я".repeat(80);
        let title = derive_title(&long);
        assert!(title.ends_with('…'));
        assert!(title.chars().count() <= 65);
    }
}
