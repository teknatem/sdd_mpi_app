//! API utilities for frontend-backend communication
//!
//! Provides helper functions for constructing API URLs and making requests.

use wasm_bindgen::{JsCast, JsValue};
use web_sys::{Request, RequestInit, RequestMode, Response};

/// Get the base URL for API requests
///
/// Constructs the API base URL from the current window location,
/// using port 3000 for the backend server.
///
/// # Returns
/// - API base URL like "http://localhost:3000" or "https://example.com:3000"
/// - Empty string if window is not available
///
/// # Example
/// ```rust
/// let url = format!("{}/api/nomenclature/{}", api_base(), id);
/// ```
pub fn api_base() -> String {
    let window = match web_sys::window() {
        Some(w) => w,
        None => return String::new(),
    };
    let location = window.location();
    let protocol = location.protocol().unwrap_or_else(|_| "http:".to_string());

    // Используем host (включает порт), а не hostname + :3000
    // Это работает когда backend и frontend на одном порту
    let host = location
        .host()
        .unwrap_or_else(|_| "127.0.0.1:3000".to_string());

    // Если host уже содержит порт, используем как есть
    // Иначе добавляем :3000
    let full_host = if host.contains(':') {
        host
    } else {
        format!("{}:3000", host)
    };

    format!("{}//{}", protocol, full_host)
}

/// Build a full API URL from a path
///
/// # Arguments
/// * `path` - The API path (should start with "/api/")
///
/// # Example
/// ```rust
/// let url = api_url("/api/nomenclature/123");
/// ```
pub fn api_url(path: &str) -> String {
    format!("{}{}", api_base(), path)
}

/// GET `path` and deserialize the JSON body.
///
/// `path` is relative to the API root and must start with `/api/`.
/// Аутентификация — сессионная cookie, заголовки ставить не нужно.
pub async fn get_json<T: serde::de::DeserializeOwned>(path: &str) -> Result<T, String> {
    let opts = RequestInit::new();
    opts.set_method("GET");
    opts.set_mode(RequestMode::Cors);

    let request = Request::new_with_str_and_init(&api_url(path), &opts)
        .map_err(|e| format!("Failed to create request: {:?}", e))?;

    send(request).await
}

/// POST `body` as JSON to `path` and deserialize the JSON response.
pub async fn post_json<B: serde::Serialize, T: serde::de::DeserializeOwned>(
    path: &str,
    body: &B,
) -> Result<T, String> {
    let payload = serde_json::to_string(body).map_err(|e| e.to_string())?;

    let opts = RequestInit::new();
    opts.set_method("POST");
    opts.set_mode(RequestMode::Cors);
    opts.set_body(&JsValue::from_str(&payload));

    let request = Request::new_with_str_and_init(&api_url(path), &opts)
        .map_err(|e| format!("Failed to create request: {:?}", e))?;

    request
        .headers()
        .set("Content-Type", "application/json")
        .map_err(|e| format!("Failed to set header: {:?}", e))?;

    send(request).await
}

/// Общая часть: отправить запрос и разобрать JSON-тело ответа.
///
/// Текст ошибки намеренно содержит код статуса (`HTTP error: 404`) — вызывающий
/// код различает «сессии больше нет» по подстроке.
async fn send<T: serde::de::DeserializeOwned>(request: Request) -> Result<T, String> {
    let window = web_sys::window().ok_or("No window object")?;

    let response_value = wasm_bindgen_futures::JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(|e| format!("Fetch failed: {:?}", e))?;

    let response: Response = response_value
        .dyn_into()
        .map_err(|_| "Not a Response".to_string())?;

    if !response.ok() {
        return Err(format!("HTTP error: {}", response.status()));
    }

    let json = wasm_bindgen_futures::JsFuture::from(
        response
            .json()
            .map_err(|e| format!("Failed to parse JSON: {:?}", e))?,
    )
    .await
    .map_err(|e| format!("Failed to get JSON: {:?}", e))?;

    serde_wasm_bindgen::from_value(json).map_err(|e| e.to_string())
}
