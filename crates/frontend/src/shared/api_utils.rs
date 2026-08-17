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

/// Ошибка обращения к API.
///
/// **Зачем тип, а не строка.** Раньше `send` отдавал `format!("HTTP error: {}",
/// status)`, и различать «сессии больше нет» от «нет такой записи» приходилось
/// поиском подстроки в тексте — контракт, который ломается от любой правки
/// формулировки. Бэкенд теперь отвечает документом RFC 9457
/// (`type`/`title`/`status`/`detail`/`code`), и здесь он разбирается один раз.
///
/// `Display` сохраняет прежний вид сообщения (`HTTP error: 404`), поэтому
/// вызывающий код, работающий со `String`, менять не нужно — он просто получает
/// более внятный текст, когда сервер прислал `detail`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApiError {
    /// Запрос не ушёл: сеть, CORS, отсутствующее окно.
    Network(String),
    /// Сервер ответил ошибкой.
    Status {
        status: u16,
        /// Машиночитаемый код из тела (`not_found`, `bad_request`, …), если он был.
        code: Option<String>,
        /// Человекочитаемая причина из тела, если она была.
        detail: Option<String>,
    },
    /// Ответ пришёл, но не разбирается в ожидаемый тип.
    Decode(String),
}

impl ApiError {
    /// Код ответа, если ошибка пришла от сервера.
    pub fn status(&self) -> Option<u16> {
        match self {
            Self::Status { status, .. } => Some(*status),
            _ => None,
        }
    }

    /// Сессии больше нет — типизированная замена поиску `"HTTP error: 401"`.
    pub fn is_unauthorized(&self) -> bool {
        self.status() == Some(401)
    }

    /// Прав не хватает.
    pub fn is_forbidden(&self) -> bool {
        self.status() == Some(403)
    }

    /// Объекта не существует.
    pub fn is_not_found(&self) -> bool {
        self.status() == Some(404)
    }
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Network(message) => write!(f, "Fetch failed: {message}"),
            Self::Status {
                status,
                detail: Some(detail),
                ..
            } if !detail.is_empty() => write!(f, "HTTP error: {status} — {detail}"),
            Self::Status { status, .. } => write!(f, "HTTP error: {status}"),
            Self::Decode(message) => write!(f, "{message}"),
        }
    }
}

/// GET `path` and deserialize the JSON body.
///
/// `path` is relative to the API root and must start with `/api/`.
/// Аутентификация — сессионная cookie, заголовки ставить не нужно.
pub async fn get_json<T: serde::de::DeserializeOwned>(path: &str) -> Result<T, String> {
    try_get_json(path).await.map_err(|error| error.to_string())
}

/// То же, что [`get_json`], но с типизированной ошибкой.
pub async fn try_get_json<T: serde::de::DeserializeOwned>(path: &str) -> Result<T, ApiError> {
    let opts = RequestInit::new();
    opts.set_method("GET");
    opts.set_mode(RequestMode::Cors);

    let request = Request::new_with_str_and_init(&api_url(path), &opts)
        .map_err(|e| ApiError::Network(format!("Failed to create request: {:?}", e)))?;

    send(request).await
}

/// POST `body` as JSON to `path` and deserialize the JSON response.
pub async fn post_json<B: serde::Serialize, T: serde::de::DeserializeOwned>(
    path: &str,
    body: &B,
) -> Result<T, String> {
    try_post_json(path, body)
        .await
        .map_err(|error| error.to_string())
}

/// То же, что [`post_json`], но с типизированной ошибкой.
pub async fn try_post_json<B: serde::Serialize, T: serde::de::DeserializeOwned>(
    path: &str,
    body: &B,
) -> Result<T, ApiError> {
    let payload = serde_json::to_string(body).map_err(|e| ApiError::Decode(e.to_string()))?;

    let opts = RequestInit::new();
    opts.set_method("POST");
    opts.set_mode(RequestMode::Cors);
    opts.set_body(&JsValue::from_str(&payload));

    let request = Request::new_with_str_and_init(&api_url(path), &opts)
        .map_err(|e| ApiError::Network(format!("Failed to create request: {:?}", e)))?;

    request
        .headers()
        .set("Content-Type", "application/json")
        .map_err(|e| ApiError::Network(format!("Failed to set header: {:?}", e)))?;

    send(request).await
}

/// Общая часть: отправить запрос и разобрать JSON-тело ответа.
async fn send<T: serde::de::DeserializeOwned>(request: Request) -> Result<T, ApiError> {
    let window = web_sys::window().ok_or_else(|| ApiError::Network("No window object".into()))?;

    let response_value = wasm_bindgen_futures::JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(|e| ApiError::Network(format!("{:?}", e)))?;

    let response: Response = response_value
        .dyn_into()
        .map_err(|_| ApiError::Network("Not a Response".into()))?;

    if !response.ok() {
        return Err(problem_from(&response).await);
    }

    let json = wasm_bindgen_futures::JsFuture::from(
        response
            .json()
            .map_err(|e| ApiError::Decode(format!("Failed to parse JSON: {:?}", e)))?,
    )
    .await
    .map_err(|e| ApiError::Decode(format!("Failed to get JSON: {:?}", e)))?;

    serde_wasm_bindgen::from_value(json).map_err(|e| ApiError::Decode(e.to_string()))
}

/// Разобрать тело ошибки. Тело может отсутствовать или быть не problem+json
/// (например, ответ прокси) — тогда остаётся один статус, и это нормально.
async fn problem_from(response: &Response) -> ApiError {
    let status = response.status();

    let body = match response.text() {
        Ok(promise) => wasm_bindgen_futures::JsFuture::from(promise)
            .await
            .ok()
            .and_then(|value| value.as_string()),
        Err(_) => None,
    };

    let problem = body
        .as_deref()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(text).ok());

    let field = |name: &str| -> Option<String> {
        problem
            .as_ref()
            .and_then(|value| value.get(name))
            .and_then(|value| value.as_str())
            .filter(|text| !text.is_empty())
            .map(|text| text.to_string())
    };

    ApiError::Status {
        status,
        code: field("code"),
        detail: field("detail").or_else(|| field("error")),
    }
}
