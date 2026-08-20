use anyhow::{Context, Result};
use contracts::domain::a006_connection_mp::aggregate::ConnectionMP;
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io::{Cursor, Read, Write};
use std::sync::{Arc, Mutex, OnceLock};
use uuid::Uuid;

use super::progress_tracker::ProgressTracker;
use crate::shared::marketplaces::wildberries::datetime::{
    format_wb_cursor_datetime, format_wb_local_datetime_seconds, parse_wb_datetime, wb_day_end_utc,
    wb_day_start_utc,
};

const WB_ORDERS_MAX_RATE_LIMIT_SLEEP_SECS: u64 = 300;
const WB_FINANCE_V1_MIN_INTERVAL_SECS: u64 = 61;
const WB_FINANCE_V1_FALLBACK_RETRY_SECS: u64 = 65;

type FinanceGate = Arc<tokio::sync::Mutex<Option<std::time::Instant>>>;
static WB_FINANCE_V1_GATES: OnceLock<Mutex<HashMap<String, FinanceGate>>> = OnceLock::new();

fn wb_finance_v1_gate(connection_id: &str) -> FinanceGate {
    let gates = WB_FINANCE_V1_GATES.get_or_init(|| Mutex::new(HashMap::new()));
    let mut gates = gates.lock().expect("WB finance rate gate poisoned");
    gates
        .entry(connection_id.to_string())
        .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(None)))
        .clone()
}

#[derive(Debug, Clone, Default)]
struct WbRateLimitHeaders {
    retry_seconds: Option<u64>,
    limit: Option<u64>,
    reset_seconds: Option<u64>,
    remaining: Option<u64>,
}

impl WbRateLimitHeaders {
    fn from_headers(headers: &HeaderMap) -> Self {
        fn parse_header(headers: &HeaderMap, name: &str) -> Option<u64> {
            headers
                .get(name)
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.trim().parse::<u64>().ok())
        }

        Self {
            retry_seconds: parse_header(headers, "Retry-After")
                .or_else(|| parse_header(headers, "X-Ratelimit-Retry")),
            limit: parse_header(headers, "X-Ratelimit-Limit"),
            reset_seconds: parse_header(headers, "X-Ratelimit-Reset"),
            remaining: parse_header(headers, "X-Ratelimit-Remaining"),
        }
    }

    fn is_empty(&self) -> bool {
        self.retry_seconds.is_none()
            && self.limit.is_none()
            && self.reset_seconds.is_none()
            && self.remaining.is_none()
    }

    fn to_log_fields(&self) -> String {
        if self.is_empty() {
            return "not provided".to_string();
        }

        format!(
            "retry={}s, reset={}s, limit={}, remaining={}",
            self.retry_seconds
                .map(|v| v.to_string())
                .unwrap_or_else(|| "n/a".to_string()),
            self.reset_seconds
                .map(|v| v.to_string())
                .unwrap_or_else(|| "n/a".to_string()),
            self.limit
                .map(|v| v.to_string())
                .unwrap_or_else(|| "n/a".to_string()),
            self.remaining
                .map(|v| v.to_string())
                .unwrap_or_else(|| "n/a".to_string()),
        )
    }

    fn to_error_suffix(&self) -> String {
        if self.is_empty() {
            String::new()
        } else {
            format!(" | X-Ratelimit: {}", self.to_log_fields())
        }
    }
}

/// HTTP-клиент для работы с Wildberries Supplier API
pub struct WildberriesApiClient {
    client: reqwest::Client,
    /// Привязка к сессии импорта: учёт HTTP для `sys_task_runs` / UI «Активные».
    http_track: Arc<Mutex<Option<(Arc<ProgressTracker>, String)>>>,
}

pub struct HttpTrackingGuard {
    http_track: Arc<Mutex<Option<(Arc<ProgressTracker>, String)>>>,
}

impl Drop for HttpTrackingGuard {
    fn drop(&mut self) {
        if let Ok(mut guard) = self.http_track.lock() {
            *guard = None;
        }
    }
}

impl WildberriesApiClient {
    pub fn new() -> Self {
        let mut headers = HeaderMap::new();
        headers.insert(ACCEPT, HeaderValue::from_static("application/json"));

        Self {
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(60)) // Увеличен таймаут для медленных API
                .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
                .default_headers(headers)
                .danger_accept_invalid_certs(true) // Временно для отладки
                .no_proxy()
                .redirect(reqwest::redirect::Policy::limited(10)) // РЎР»РµРґРѕРІР°С‚СЊ редиректам
                .build()
                .expect("Failed to create HTTP client"),
            http_track: Arc::new(Mutex::new(None)),
        }
    }

    /// Включает учёт трафика для текущей сессии импорта.
    /// Каждый `ImportExecutor` принадлежит ровно одному менеджеру задачи, поэтому
    /// параллельный вызов невозможен — планировщик не запускает одну задачу дважды.
    pub fn bind_http_tracking(
        &self,
        tracker: Arc<ProgressTracker>,
        session_id: String,
    ) -> HttpTrackingGuard {
        if let Ok(mut g) = self.http_track.lock() {
            if g.is_some() {
                tracing::warn!(
                    "bind_http_tracking: overwriting existing tracking for session {}",
                    session_id
                );
            }
            *g = Some((tracker, session_id));
        }
        HttpTrackingGuard {
            http_track: Arc::clone(&self.http_track),
        }
    }

    pub fn clear_http_tracking(&self) {
        if let Ok(mut g) = self.http_track.lock() {
            *g = None;
        }
    }

    /// Читает тело ответа и при активной привязке увеличивает счётчики HTTP в трекере.
    pub(crate) async fn read_body_tracked(
        &self,
        response: reqwest::Response,
    ) -> Result<String, reqwest::Error> {
        self.read_body_tracked_with_request_bytes(response, 0).await
    }

    pub(crate) async fn read_body_tracked_with_request_bytes(
        &self,
        response: reqwest::Response,
        request_body_len: u64,
    ) -> Result<String, reqwest::Error> {
        let text = response.text().await?;
        let response_bytes = text.len() as u64;
        if let Ok(guard) = self.http_track.lock() {
            if let Some((tracker, sid)) = guard.as_ref() {
                tracker.record_http_exchange(sid, request_body_len, response_bytes);
            }
        }
        Ok(text)
    }

    fn record_http_request_attempt(&self, request_body_len: u64) {
        if let Ok(guard) = self.http_track.lock() {
            if let Some((tracker, sid)) = guard.as_ref() {
                tracker.record_http_request_attempt(sid, request_body_len);
            }
        }
    }

    fn record_http_response_body(&self, response_body_len: u64) {
        if let Ok(guard) = self.http_track.lock() {
            if let Some((tracker, sid)) = guard.as_ref() {
                tracker.record_http_response_body(sid, response_body_len);
            }
        }
    }

    fn set_tracked_current_item(&self, aggregate_index: &str, label: impl Into<String>) {
        if let Ok(guard) = self.http_track.lock() {
            if let Some((tracker, sid)) = guard.as_ref() {
                tracker.set_current_item(sid, aggregate_index, Some(label.into()));
            }
        }
    }

    async fn read_body_for_recorded_request(
        &self,
        response: reqwest::Response,
    ) -> Result<String, reqwest::Error> {
        let text = response.text().await?;
        self.record_http_response_body(text.len() as u64);
        Ok(text)
    }

    /// Диагностическая функция для тестирования различных вариантов запроса
    pub async fn diagnostic_fetch_all_variations(
        &self,
        connection: &ConnectionMP,
    ) -> Result<Vec<DiagnosticResult>> {
        let mut results = Vec::new();

        // Вариант 1: Текущая реализация (пустой фильтр, limit=100)
        results.push(
            self.test_request_variation(
                connection,
                "Current implementation",
                100,
                WildberriesSettings {
                    cursor: WildberriesCursor::default(),
                    filter: WildberriesFilter::default(),
                },
            )
            .await,
        );

        // Вариант 2: Увеличенный limit до 1000
        results.push(
            self.test_request_variation(
                connection,
                "Increased limit to 1000",
                1000,
                WildberriesSettings {
                    cursor: WildberriesCursor::default(),
                    filter: WildberriesFilter::default(),
                },
            )
            .await,
        );

        // Вариант 3: Без settings вообще (минимальный запрос)
        results.push(
            self.test_minimal_request(connection, "Minimal request (no settings)", 1000)
                .await,
        );

        // Вариант 4: С явным textSearch пустым
        results.push(
            self.test_request_variation(
                connection,
                "Empty textSearch filter",
                1000,
                WildberriesSettings {
                    cursor: WildberriesCursor::default(),
                    filter: WildberriesFilter {
                        find_by_nm_id: None,
                        with_photo: None,
                    },
                },
            )
            .await,
        );

        // Вариант 5: Альтернативный endpoint - Marketplace API
        results.push(
            self.test_alternative_endpoint(
                connection,
                "Alternative: Marketplace API v3",
                "https://marketplace-api.wildberries.ru",
                "/api/v3/goods/list",
            )
            .await,
        );

        // Вариант 6: Альтернативный endpoint - Supplier API (stocks)
        results.push(
            self.test_stocks_endpoint(connection, "Alternative: Supplier stocks API")
                .await,
        );

        // Вариант 7: РљР РРўРР§Р•РЎРљРР™ РўР•РЎРў - Попытка получить товары БЕЗ фильтра categories
        // Все предыдущие запросы возвращают только subjectID=7717
        // Попробуем запросить с явным указанием что хотим все категории
        results.push(
            self.test_without_category_filter(
                connection,
                "WITHOUT category filter (attempt to get ALL subjects)",
                1000,
            )
            .await,
        );

        // Вариант 8: РђР РҐРР’РќР«Р• РўРћР’РђР Р« - /content/v2/get/cards/trash
        // РљР РРўРР§РќРћ: Возможно большинство товаров в корзине/архиве!
        results.push(
            self.test_trash_endpoint(
                connection,
                "TRASH/Archive endpoint - check deleted/archived products",
                1000,
            )
            .await,
        );

        // Вариант 9: РџРћР›РЈР§РРўР¬ РЎРџРРЎРћРљ Р’РЎР•РҐ РљРђРўР•Р“РћР РР™ РџР РћР”РђР’Р¦Рђ
        // Проверить сколько категорий (subjects) используется
        results.push(
            self.test_get_all_subjects(connection, "Get ALL subjects/categories used by seller")
                .await,
        );

        // Вариант 10: РџР РћР”РћР›Р–РРўР¬ РџРђР“РРќРђР¦РР® - получить РЎР›Р•Р”РЈР®Р©РЈР® страницу
        // Возможно API возвращает товары по категориям постранично
        results.push(
            self.test_pagination_continuation(
                connection,
                "Continue pagination to get NEXT page of products",
            )
            .await,
        );

        Ok(results)
    }

    async fn test_request_variation(
        &self,
        connection: &ConnectionMP,
        test_name: &str,
        limit: i32,
        settings: WildberriesSettings,
    ) -> DiagnosticResult {
        self.log_to_file(&format!(
            "\n========== DIAGNOSTIC TEST: {} ==========",
            test_name
        ));

        let base_url = if let Some(ref supplier_id) = connection.supplier_id {
            if supplier_id.starts_with("http") {
                supplier_id.trim_end_matches('/')
            } else {
                "https://content-api.wildberries.ru"
            }
        } else {
            "https://content-api.wildberries.ru"
        };

        let url = format!("{}/content/v2/get/cards/list", base_url);

        let request_body = WildberriesProductListRequest { settings, limit };

        let body = match serde_json::to_string(&request_body) {
            Ok(b) => b,
            Err(e) => {
                return DiagnosticResult {
                    test_name: test_name.to_string(),
                    success: false,
                    error: Some(format!("Failed to serialize request: {}", e)),
                    total_returned: 0,
                    cursor_total: 0,
                    response_headers: None,
                };
            }
        };

        self.log_to_file(&format!("Request body: {}", body));
        let request_body_len = body.len() as u64;

        let response = match self
            .client
            .post(&url)
            .header("Authorization", &connection.api_key)
            .header("Content-Type", "application/json")
            .body(body)
            .send()
            .await
        {
            Ok(resp) => resp,
            Err(e) => {
                self.log_to_file(&format!("Request failed: {}", e));
                return DiagnosticResult {
                    test_name: test_name.to_string(),
                    success: false,
                    error: Some(format!("HTTP request failed: {}", e)),
                    total_returned: 0,
                    cursor_total: 0,
                    response_headers: None,
                };
            }
        };

        let status = response.status();
        let headers = response.headers().clone();
        self.log_to_file(&format!("Response status: {}", status));
        self.log_to_file(&format!("Response headers: {:?}", headers));

        if !status.is_success() {
            let body = self
                .read_body_tracked_with_request_bytes(response, request_body_len)
                .await
                .unwrap_or_default();
            self.log_to_file(&format!("Error response body: {}", body));
            return DiagnosticResult {
                test_name: test_name.to_string(),
                success: false,
                error: Some(format!("API returned status {}: {}", status, body)),
                total_returned: 0,
                cursor_total: 0,
                response_headers: Some(format!("{:?}", headers)),
            };
        }

        let body = match self
            .read_body_tracked_with_request_bytes(response, request_body_len)
            .await
        {
            Ok(b) => b,
            Err(e) => {
                return DiagnosticResult {
                    test_name: test_name.to_string(),
                    success: false,
                    error: Some(format!("Failed to read response body: {}", e)),
                    total_returned: 0,
                    cursor_total: 0,
                    response_headers: Some(format!("{:?}", headers)),
                };
            }
        };

        self.log_to_file(&format!("Response body: {}", body));

        match serde_json::from_str::<WildberriesProductListResponse>(&body) {
            Ok(data) => {
                self.log_to_file(&format!(
                    "вњ“ Success: {} items, cursor.total={}",
                    data.cards.len(),
                    data.cursor.total
                ));
                DiagnosticResult {
                    test_name: test_name.to_string(),
                    success: true,
                    error: None,
                    total_returned: data.cards.len() as i32,
                    cursor_total: data.cursor.total as i32,
                    response_headers: Some(format!("{:?}", headers)),
                }
            }
            Err(e) => {
                self.log_to_file(&format!("Failed to parse response: {}", e));
                DiagnosticResult {
                    test_name: test_name.to_string(),
                    success: false,
                    error: Some(format!("Failed to parse JSON: {}", e)),
                    total_returned: 0,
                    cursor_total: 0,
                    response_headers: Some(format!("{:?}", headers)),
                }
            }
        }
    }

    async fn test_minimal_request(
        &self,
        connection: &ConnectionMP,
        test_name: &str,
        limit: i32,
    ) -> DiagnosticResult {
        self.log_to_file(&format!(
            "\n========== DIAGNOSTIC TEST: {} ==========",
            test_name
        ));

        let base_url = if let Some(ref supplier_id) = connection.supplier_id {
            if supplier_id.starts_with("http") {
                supplier_id.trim_end_matches('/')
            } else {
                "https://content-api.wildberries.ru"
            }
        } else {
            "https://content-api.wildberries.ru"
        };

        let url = format!("{}/content/v2/get/cards/list", base_url);

        // Минимальный запрос - только limit
        let body = format!(r#"{{"limit":{}}}"#, limit);
        self.log_to_file(&format!("Minimal request body: {}", body));
        let request_body_len = body.len() as u64;

        let response = match self
            .client
            .post(&url)
            .header("Authorization", &connection.api_key)
            .header("Content-Type", "application/json")
            .body(body)
            .send()
            .await
        {
            Ok(resp) => resp,
            Err(e) => {
                self.log_to_file(&format!("Request failed: {}", e));
                return DiagnosticResult {
                    test_name: test_name.to_string(),
                    success: false,
                    error: Some(format!("HTTP request failed: {}", e)),
                    total_returned: 0,
                    cursor_total: 0,
                    response_headers: None,
                };
            }
        };

        let status = response.status();
        let headers = response.headers().clone();
        self.log_to_file(&format!("Response status: {}", status));

        if !status.is_success() {
            let body = self
                .read_body_tracked_with_request_bytes(response, request_body_len)
                .await
                .unwrap_or_default();
            self.log_to_file(&format!("Error response body: {}", body));
            return DiagnosticResult {
                test_name: test_name.to_string(),
                success: false,
                error: Some(format!("API returned status {}: {}", status, body)),
                total_returned: 0,
                cursor_total: 0,
                response_headers: Some(format!("{:?}", headers)),
            };
        }

        let body = match self
            .read_body_tracked_with_request_bytes(response, request_body_len)
            .await
        {
            Ok(b) => b,
            Err(e) => {
                return DiagnosticResult {
                    test_name: test_name.to_string(),
                    success: false,
                    error: Some(format!("Failed to read response body: {}", e)),
                    total_returned: 0,
                    cursor_total: 0,
                    response_headers: Some(format!("{:?}", headers)),
                };
            }
        };

        self.log_to_file(&format!("Response body: {}", body));

        match serde_json::from_str::<WildberriesProductListResponse>(&body) {
            Ok(data) => {
                self.log_to_file(&format!(
                    "вњ“ Success: {} items, cursor.total={}",
                    data.cards.len(),
                    data.cursor.total
                ));
                DiagnosticResult {
                    test_name: test_name.to_string(),
                    success: true,
                    error: None,
                    total_returned: data.cards.len() as i32,
                    cursor_total: data.cursor.total as i32,
                    response_headers: Some(format!("{:?}", headers)),
                }
            }
            Err(e) => {
                self.log_to_file(&format!("Failed to parse response: {}", e));
                DiagnosticResult {
                    test_name: test_name.to_string(),
                    success: false,
                    error: Some(format!("Failed to parse JSON: {}", e)),
                    total_returned: 0,
                    cursor_total: 0,
                    response_headers: Some(format!("{:?}", headers)),
                }
            }
        }
    }

    async fn test_alternative_endpoint(
        &self,
        connection: &ConnectionMP,
        test_name: &str,
        base_url: &str,
        endpoint_path: &str,
    ) -> DiagnosticResult {
        self.log_to_file(&format!(
            "\n========== DIAGNOSTIC TEST: {} ==========",
            test_name
        ));
        self.log_to_file(&format!("Testing endpoint: {}{}", base_url, endpoint_path));

        let url = format!("{}{}", base_url, endpoint_path);

        // Пробуем простой GET запрос
        let response = match self
            .client
            .get(&url)
            .header("Authorization", &connection.api_key)
            .send()
            .await
        {
            Ok(resp) => resp,
            Err(e) => {
                self.log_to_file(&format!("Request failed: {}", e));
                return DiagnosticResult {
                    test_name: test_name.to_string(),
                    success: false,
                    error: Some(format!("HTTP request failed: {}", e)),
                    total_returned: 0,
                    cursor_total: 0,
                    response_headers: None,
                };
            }
        };

        let status = response.status();
        let headers = response.headers().clone();
        self.log_to_file(&format!("Response status: {}", status));
        self.log_to_file(&format!("Response headers: {:?}", headers));

        if !status.is_success() {
            let body = self.read_body_tracked(response).await.unwrap_or_default();
            self.log_to_file(&format!("Error response body: {}", body));

            // 404 или 405 означает что endpoint не существует или метод не поддерживается
            if status.as_u16() == 404 || status.as_u16() == 405 {
                return DiagnosticResult {
                    test_name: test_name.to_string(),
                    success: false,
                    error: Some(format!("Endpoint not available ({})", status)),
                    total_returned: 0,
                    cursor_total: 0,
                    response_headers: Some(format!("{:?}", headers)),
                };
            }

            return DiagnosticResult {
                test_name: test_name.to_string(),
                success: false,
                error: Some(format!("API returned status {}: {}", status, body)),
                total_returned: 0,
                cursor_total: 0,
                response_headers: Some(format!("{:?}", headers)),
            };
        }

        let body = match self.read_body_tracked(response).await {
            Ok(b) => b,
            Err(e) => {
                return DiagnosticResult {
                    test_name: test_name.to_string(),
                    success: false,
                    error: Some(format!("Failed to read response body: {}", e)),
                    total_returned: 0,
                    cursor_total: 0,
                    response_headers: Some(format!("{:?}", headers)),
                };
            }
        };

        self.log_to_file(&format!(
            "Response body (first 500 chars): {}",
            body.chars().take(500).collect::<String>()
        ));

        // Пробуем распарсить как наш стандартный ответ
        match serde_json::from_str::<WildberriesProductListResponse>(&body) {
            Ok(data) => {
                self.log_to_file(&format!(
                    "вњ“ Success (parseable as standard response): {} items, cursor.total={}",
                    data.cards.len(),
                    data.cursor.total
                ));
                DiagnosticResult {
                    test_name: test_name.to_string(),
                    success: true,
                    error: None,
                    total_returned: data.cards.len() as i32,
                    cursor_total: data.cursor.total as i32,
                    response_headers: Some(format!("{:?}", headers)),
                }
            }
            Err(_) => {
                // Не парсится как стандартный ответ, но запрос успешный
                self.log_to_file("Response structure is different from standard format");
                DiagnosticResult {
                    test_name: test_name.to_string(),
                    success: false,
                    error: Some(
                        "Response has different structure (not standard cards format)".to_string(),
                    ),
                    total_returned: 0,
                    cursor_total: 0,
                    response_headers: Some(format!("{:?}", headers)),
                }
            }
        }
    }

    async fn test_stocks_endpoint(
        &self,
        connection: &ConnectionMP,
        test_name: &str,
    ) -> DiagnosticResult {
        self.log_to_file(&format!(
            "\n========== DIAGNOSTIC TEST: {} ==========",
            test_name
        ));

        // Supplier stocks API endpoint
        let url = "https://suppliers-api.wildberries.ru/api/v1/supplier/stocks";
        self.log_to_file(&format!("Testing endpoint: {}", url));

        let response = match self
            .client
            .get(url)
            .header("Authorization", &connection.api_key)
            .send()
            .await
        {
            Ok(resp) => resp,
            Err(e) => {
                self.log_to_file(&format!("Request failed: {}", e));
                return DiagnosticResult {
                    test_name: test_name.to_string(),
                    success: false,
                    error: Some(format!("HTTP request failed: {}", e)),
                    total_returned: 0,
                    cursor_total: 0,
                    response_headers: None,
                };
            }
        };

        let status = response.status();
        let headers = response.headers().clone();
        self.log_to_file(&format!("Response status: {}", status));
        self.log_to_file(&format!("Response headers: {:?}", headers));

        if !status.is_success() {
            let body = self.read_body_tracked(response).await.unwrap_or_default();
            self.log_to_file(&format!("Error response body: {}", body));

            if status.as_u16() == 404 {
                return DiagnosticResult {
                    test_name: test_name.to_string(),
                    success: false,
                    error: Some("Stocks endpoint not available".to_string()),
                    total_returned: 0,
                    cursor_total: 0,
                    response_headers: Some(format!("{:?}", headers)),
                };
            }

            return DiagnosticResult {
                test_name: test_name.to_string(),
                success: false,
                error: Some(format!("API returned status {}: {}", status, body)),
                total_returned: 0,
                cursor_total: 0,
                response_headers: Some(format!("{:?}", headers)),
            };
        }

        let body = match self.read_body_tracked(response).await {
            Ok(b) => b,
            Err(e) => {
                return DiagnosticResult {
                    test_name: test_name.to_string(),
                    success: false,
                    error: Some(format!("Failed to read response body: {}", e)),
                    total_returned: 0,
                    cursor_total: 0,
                    response_headers: Some(format!("{:?}", headers)),
                };
            }
        };

        self.log_to_file(&format!(
            "Response body (first 500 chars): {}",
            body.chars().take(500).collect::<String>()
        ));

        // Stocks API возвращает массив с другой структурой
        // Пробуем распарсить и посчитать количество товаров
        match serde_json::from_str::<serde_json::Value>(&body) {
            Ok(json) => {
                if let Some(stocks) = json.as_array() {
                    let count = stocks.len();
                    self.log_to_file(&format!("вњ“ Success: Stocks API returned {} items", count));
                    DiagnosticResult {
                        test_name: test_name.to_string(),
                        success: true,
                        error: None,
                        total_returned: count as i32,
                        cursor_total: count as i32, // Stocks API не имеет cursor.total
                        response_headers: Some(format!("{:?}", headers)),
                    }
                } else {
                    self.log_to_file("Response is not an array");
                    DiagnosticResult {
                        test_name: test_name.to_string(),
                        success: false,
                        error: Some("Stocks response is not an array".to_string()),
                        total_returned: 0,
                        cursor_total: 0,
                        response_headers: Some(format!("{:?}", headers)),
                    }
                }
            }
            Err(e) => {
                self.log_to_file(&format!("Failed to parse stocks response: {}", e));
                DiagnosticResult {
                    test_name: test_name.to_string(),
                    success: false,
                    error: Some(format!("Failed to parse JSON: {}", e)),
                    total_returned: 0,
                    cursor_total: 0,
                    response_headers: Some(format!("{:?}", headers)),
                }
            }
        }
    }

    async fn test_get_all_subjects(
        &self,
        connection: &ConnectionMP,
        test_name: &str,
    ) -> DiagnosticResult {
        self.log_to_file(&format!(
            "\n========== DIAGNOSTIC TEST: {} ==========",
            test_name
        ));
        self.log_to_file("рџ“Љ Getting list of ALL subjects/categories from seller account");
        self.log_to_file("This will show how many categories are used");

        let base_url = if let Some(ref supplier_id) = connection.supplier_id {
            if supplier_id.starts_with("http") {
                supplier_id.trim_end_matches('/')
            } else {
                "https://content-api.wildberries.ru"
            }
        } else {
            "https://content-api.wildberries.ru"
        };

        // Endpoint для получения списка subjects
        let url = format!("{}/content/v2/object/all?limit=1000", base_url);
        self.log_to_file(&format!("GET request to: {}", url));

        let response = match self
            .client
            .get(&url)
            .header("Authorization", &connection.api_key)
            .send()
            .await
        {
            Ok(resp) => resp,
            Err(e) => {
                self.log_to_file(&format!("Request failed: {}", e));
                return DiagnosticResult {
                    test_name: test_name.to_string(),
                    success: false,
                    error: Some(format!("HTTP request failed: {}", e)),
                    total_returned: 0,
                    cursor_total: 0,
                    response_headers: None,
                };
            }
        };

        let status = response.status();
        let headers = response.headers().clone();
        self.log_to_file(&format!("Response status: {}", status));

        if !status.is_success() {
            let body = self.read_body_tracked(response).await.unwrap_or_default();
            self.log_to_file(&format!("Error response body: {}", body));
            return DiagnosticResult {
                test_name: test_name.to_string(),
                success: false,
                error: Some(format!("API returned status {}: {}", status, body)),
                total_returned: 0,
                cursor_total: 0,
                response_headers: Some(format!("{:?}", headers)),
            };
        }

        let body = match self.read_body_tracked(response).await {
            Ok(b) => b,
            Err(e) => {
                return DiagnosticResult {
                    test_name: test_name.to_string(),
                    success: false,
                    error: Some(format!("Failed to read response body: {}", e)),
                    total_returned: 0,
                    cursor_total: 0,
                    response_headers: Some(format!("{:?}", headers)),
                };
            }
        };

        self.log_to_file(&format!(
            "Response body preview: {}",
            body.chars().take(1000).collect::<String>()
        ));

        // Попробуем распарсить как JSON
        match serde_json::from_str::<serde_json::Value>(&body) {
            Ok(json) => {
                if let Some(data) = json.get("data").and_then(|d| d.as_array()) {
                    self.log_to_file(&format!(
                        "вњ“ Found {} subjects/categories available to this seller!",
                        data.len()
                    ));

                    // Найдем уникальные subjectID
                    let mut subject_ids = Vec::new();
                    for item in data.iter().take(20) {
                        if let Some(id) = item.get("subjectID").and_then(|i| i.as_i64()) {
                            if let Some(name) = item.get("subjectName").and_then(|n| n.as_str()) {
                                self.log_to_file(&format!("  - SubjectID {}: {}", id, name));
                                subject_ids.push(id);
                            }
                        }
                    }
                    if data.len() > 20 {
                        self.log_to_file(&format!("  ... and {} more", data.len() - 20));
                    }

                    if subject_ids.contains(&7717) {
                        self.log_to_file("вњ“ SubjectID 7717 is in the list!");
                    }

                    if data.len() > 1 {
                        self.log_to_file(&format!(
                            "рџ”Ґ IMPORTANT: Seller has {} categories, but API returns only from ONE (7717)!",
                            data.len()
                        ));
                        self.log_to_file(
                            "This confirms: either need to query each category separately,",
                        );
                        self.log_to_file(
                            "OR continue pagination to get products from other categories.",
                        );
                    }

                    DiagnosticResult {
                        test_name: test_name.to_string(),
                        success: true,
                        error: None,
                        total_returned: data.len() as i32,
                        cursor_total: data.len() as i32,
                        response_headers: Some(format!("{:?}", headers)),
                    }
                } else {
                    self.log_to_file("Failed to find 'data' array in response");
                    DiagnosticResult {
                        test_name: test_name.to_string(),
                        success: false,
                        error: Some("No 'data' array in response".to_string()),
                        total_returned: 0,
                        cursor_total: 0,
                        response_headers: Some(format!("{:?}", headers)),
                    }
                }
            }
            Err(e) => {
                self.log_to_file(&format!("Failed to parse response: {}", e));
                DiagnosticResult {
                    test_name: test_name.to_string(),
                    success: false,
                    error: Some(format!("Failed to parse JSON: {}", e)),
                    total_returned: 0,
                    cursor_total: 0,
                    response_headers: Some(format!("{:?}", headers)),
                }
            }
        }
    }

    async fn test_pagination_continuation(
        &self,
        connection: &ConnectionMP,
        test_name: &str,
    ) -> DiagnosticResult {
        self.log_to_file(&format!(
            "\n========== DIAGNOSTIC TEST: {} ==========",
            test_name
        ));
        self.log_to_file("рџ”„ Testing pagination: Continue from FIRST page cursor");
        self.log_to_file("Hypothesis: API returns products by categories page-by-page");

        let base_url = if let Some(ref supplier_id) = connection.supplier_id {
            if supplier_id.starts_with("http") {
                supplier_id.trim_end_matches('/')
            } else {
                "https://content-api.wildberries.ru"
            }
        } else {
            "https://content-api.wildberries.ru"
        };

        let url = format!("{}/content/v2/get/cards/list", base_url);

        // РЎРЅР°С‡Р°Р»Р° получим первую страницу для извлечения cursor
        self.log_to_file("Step 1: Get FIRST page to extract cursor...");

        let first_request = WildberriesProductListRequest {
            settings: WildberriesSettings {
                cursor: WildberriesCursor::default(),
                filter: WildberriesFilter::default(),
            },
            limit: 100,
        };

        let body1 = match serde_json::to_string(&first_request) {
            Ok(b) => b,
            Err(e) => {
                return DiagnosticResult {
                    test_name: test_name.to_string(),
                    success: false,
                    error: Some(format!("Failed to serialize request: {}", e)),
                    total_returned: 0,
                    cursor_total: 0,
                    response_headers: None,
                };
            }
        };
        let request_body1_len = body1.len() as u64;

        let response1 = match self
            .client
            .post(&url)
            .header("Authorization", &connection.api_key)
            .header("Content-Type", "application/json")
            .body(body1)
            .send()
            .await
        {
            Ok(resp) => resp,
            Err(e) => {
                self.log_to_file(&format!("First request failed: {}", e));
                return DiagnosticResult {
                    test_name: test_name.to_string(),
                    success: false,
                    error: Some(format!("HTTP request failed: {}", e)),
                    total_returned: 0,
                    cursor_total: 0,
                    response_headers: None,
                };
            }
        };

        let body1_text = match self
            .read_body_tracked_with_request_bytes(response1, request_body1_len)
            .await
        {
            Ok(b) => b,
            Err(e) => {
                return DiagnosticResult {
                    test_name: test_name.to_string(),
                    success: false,
                    error: Some(format!("Failed to read response body: {}", e)),
                    total_returned: 0,
                    cursor_total: 0,
                    response_headers: None,
                };
            }
        };

        let first_page: WildberriesProductListResponse = match serde_json::from_str(&body1_text) {
            Ok(data) => data,
            Err(e) => {
                self.log_to_file(&format!("Failed to parse first page: {}", e));
                return DiagnosticResult {
                    test_name: test_name.to_string(),
                    success: false,
                    error: Some(format!("Failed to parse first page: {}", e)),
                    total_returned: 0,
                    cursor_total: 0,
                    response_headers: None,
                };
            }
        };

        self.log_to_file(&format!(
            "First page: {} items, cursor.total={}, cursor.updatedAt={:?}, cursor.nmID={:?}",
            first_page.cards.len(),
            first_page.cursor.total,
            first_page.cursor.updated_at,
            first_page.cursor.nm_id
        ));

        // Теперь запросим Р’РўРћР РЈР® страницу используя cursor из первой
        self.log_to_file("Step 2: Get SECOND page using cursor from first page...");

        let second_request = WildberriesProductListRequest {
            settings: WildberriesSettings {
                cursor: first_page.cursor.clone(),
                filter: WildberriesFilter::default(),
            },
            limit: 100,
        };

        let body2 = match serde_json::to_string(&second_request) {
            Ok(b) => b,
            Err(e) => {
                return DiagnosticResult {
                    test_name: test_name.to_string(),
                    success: false,
                    error: Some(format!("Failed to serialize second request: {}", e)),
                    total_returned: first_page.cards.len() as i32,
                    cursor_total: first_page.cursor.total as i32,
                    response_headers: None,
                };
            }
        };

        self.log_to_file(&format!("Second request body: {}", body2));
        let request_body2_len = body2.len() as u64;

        let response2 = match self
            .client
            .post(&url)
            .header("Authorization", &connection.api_key)
            .header("Content-Type", "application/json")
            .body(body2)
            .send()
            .await
        {
            Ok(resp) => resp,
            Err(e) => {
                self.log_to_file(&format!("Second request failed: {}", e));
                return DiagnosticResult {
                    test_name: test_name.to_string(),
                    success: false,
                    error: Some(format!("Second request failed: {}", e)),
                    total_returned: first_page.cards.len() as i32,
                    cursor_total: first_page.cursor.total as i32,
                    response_headers: None,
                };
            }
        };

        let status2 = response2.status();
        let headers2 = response2.headers().clone();
        self.log_to_file(&format!("Second response status: {}", status2));

        if !status2.is_success() {
            let body = self
                .read_body_tracked_with_request_bytes(response2, request_body2_len)
                .await
                .unwrap_or_default();
            self.log_to_file(&format!("Error response body: {}", body));
            return DiagnosticResult {
                test_name: test_name.to_string(),
                success: false,
                error: Some(format!(
                    "Second request returned status {}: {}",
                    status2, body
                )),
                total_returned: first_page.cards.len() as i32,
                cursor_total: first_page.cursor.total as i32,
                response_headers: Some(format!("{:?}", headers2)),
            };
        }

        let body2_text = match self
            .read_body_tracked_with_request_bytes(response2, request_body2_len)
            .await
        {
            Ok(b) => b,
            Err(e) => {
                return DiagnosticResult {
                    test_name: test_name.to_string(),
                    success: false,
                    error: Some(format!("Failed to read second response: {}", e)),
                    total_returned: first_page.cards.len() as i32,
                    cursor_total: first_page.cursor.total as i32,
                    response_headers: Some(format!("{:?}", headers2)),
                };
            }
        };

        match serde_json::from_str::<WildberriesProductListResponse>(&body2_text) {
            Ok(second_page) => {
                self.log_to_file(&format!(
                    "вњ“ Second page: {} items, cursor.total={}",
                    second_page.cards.len(),
                    second_page.cursor.total
                ));

                // Проверим subjectID на второй странице
                let mut unique_subjects = std::collections::HashSet::new();
                for card in &second_page.cards {
                    unique_subjects.insert(card.subject_id);
                }

                self.log_to_file(&format!(
                    "Second page has {} unique subjectIDs: {:?}",
                    unique_subjects.len(),
                    unique_subjects
                ));

                if second_page.cards.is_empty() {
                    self.log_to_file(
                        "вљ пёЏ Second page is EMPTY! All products were on first page.",
                    );
                    self.log_to_file("This means cursor.total matches actual product count.");
                } else if unique_subjects.len() > 1 || !unique_subjects.contains(&7717) {
                    self.log_to_file("рџ”Ґ JACKPOT! Second page has DIFFERENT categories!");
                    self.log_to_file("Solution: Need to continue pagination to get ALL products!");
                } else if unique_subjects.contains(&7717) {
                    self.log_to_file("Still subjectID=7717. Need to continue further...");
                }

                DiagnosticResult {
                    test_name: test_name.to_string(),
                    success: true,
                    error: None,
                    total_returned: second_page.cards.len() as i32,
                    cursor_total: second_page.cursor.total as i32,
                    response_headers: Some(format!("{:?}", headers2)),
                }
            }
            Err(e) => {
                self.log_to_file(&format!("Failed to parse second page: {}", e));
                DiagnosticResult {
                    test_name: test_name.to_string(),
                    success: false,
                    error: Some(format!("Failed to parse second page: {}", e)),
                    total_returned: first_page.cards.len() as i32,
                    cursor_total: first_page.cursor.total as i32,
                    response_headers: Some(format!("{:?}", headers2)),
                }
            }
        }
    }

    async fn test_trash_endpoint(
        &self,
        connection: &ConnectionMP,
        test_name: &str,
        limit: i32,
    ) -> DiagnosticResult {
        self.log_to_file(&format!(
            "\n========== DIAGNOSTIC TEST: {} ==========",
            test_name
        ));
        self.log_to_file("рџ—‘пёЏ CRITICAL: Checking TRASH/ARCHIVE endpoint");
        self.log_to_file("Maybe most products are ARCHIVED/DELETED?");

        let base_url = if let Some(ref supplier_id) = connection.supplier_id {
            if supplier_id.starts_with("http") {
                supplier_id.trim_end_matches('/')
            } else {
                "https://content-api.wildberries.ru"
            }
        } else {
            "https://content-api.wildberries.ru"
        };

        // TRASH endpoint!
        let url = format!("{}/content/v2/get/cards/trash", base_url);
        self.log_to_file(&format!("Using TRASH endpoint: {}", url));

        let request_body = WildberriesProductListRequest {
            settings: WildberriesSettings {
                cursor: WildberriesCursor::default(),
                filter: WildberriesFilter::default(),
            },
            limit,
        };

        let body = match serde_json::to_string(&request_body) {
            Ok(b) => b,
            Err(e) => {
                return DiagnosticResult {
                    test_name: test_name.to_string(),
                    success: false,
                    error: Some(format!("Failed to serialize request: {}", e)),
                    total_returned: 0,
                    cursor_total: 0,
                    response_headers: None,
                };
            }
        };

        self.log_to_file(&format!("Request body: {}", body));
        let request_body_len = body.len() as u64;

        let response = match self
            .client
            .post(&url)
            .header("Authorization", &connection.api_key)
            .header("Content-Type", "application/json")
            .body(body)
            .send()
            .await
        {
            Ok(resp) => resp,
            Err(e) => {
                self.log_to_file(&format!("Request failed: {}", e));
                return DiagnosticResult {
                    test_name: test_name.to_string(),
                    success: false,
                    error: Some(format!("HTTP request failed: {}", e)),
                    total_returned: 0,
                    cursor_total: 0,
                    response_headers: None,
                };
            }
        };

        let status = response.status();
        let headers = response.headers().clone();
        self.log_to_file(&format!("Response status: {}", status));
        self.log_to_file(&format!("Response headers: {:?}", headers));

        if !status.is_success() {
            let body = self
                .read_body_tracked_with_request_bytes(response, request_body_len)
                .await
                .unwrap_or_default();
            self.log_to_file(&format!("Error response body: {}", body));
            return DiagnosticResult {
                test_name: test_name.to_string(),
                success: false,
                error: Some(format!("API returned status {}: {}", status, body)),
                total_returned: 0,
                cursor_total: 0,
                response_headers: Some(format!("{:?}", headers)),
            };
        }

        let body = match self
            .read_body_tracked_with_request_bytes(response, request_body_len)
            .await
        {
            Ok(b) => b,
            Err(e) => {
                return DiagnosticResult {
                    test_name: test_name.to_string(),
                    success: false,
                    error: Some(format!("Failed to read response body: {}", e)),
                    total_returned: 0,
                    cursor_total: 0,
                    response_headers: Some(format!("{:?}", headers)),
                };
            }
        };

        self.log_to_file(&format!(
            "Response body preview: {}",
            body.chars().take(500).collect::<String>()
        ));

        match serde_json::from_str::<WildberriesProductListResponse>(&body) {
            Ok(data) => {
                self.log_to_file(&format!(
                    "вњ“ Success: {} items in TRASH, cursor.total={}",
                    data.cards.len(),
                    data.cursor.total
                ));

                if data.cursor.total > 100 {
                    self.log_to_file(&format!(
                        "рџ”Ґ JACKPOT! Found {} archived products! This might be the missing products!",
                        data.cursor.total
                    ));
                } else {
                    self.log_to_file("Not many archived products found.");
                }

                // Проверяем уникальные subjectID в архиве
                let mut unique_subjects = std::collections::HashSet::new();
                for card in &data.cards {
                    unique_subjects.insert(card.subject_id);
                }
                self.log_to_file(&format!(
                    "Archived products have {} unique subjectIDs: {:?}",
                    unique_subjects.len(),
                    unique_subjects
                ));

                DiagnosticResult {
                    test_name: test_name.to_string(),
                    success: true,
                    error: None,
                    total_returned: data.cards.len() as i32,
                    cursor_total: data.cursor.total as i32,
                    response_headers: Some(format!("{:?}", headers)),
                }
            }
            Err(e) => {
                self.log_to_file(&format!("Failed to parse response: {}", e));
                DiagnosticResult {
                    test_name: test_name.to_string(),
                    success: false,
                    error: Some(format!("Failed to parse JSON: {}", e)),
                    total_returned: 0,
                    cursor_total: 0,
                    response_headers: Some(format!("{:?}", headers)),
                }
            }
        }
    }

    async fn test_without_category_filter(
        &self,
        connection: &ConnectionMP,
        test_name: &str,
        limit: i32,
    ) -> DiagnosticResult {
        self.log_to_file(&format!(
            "\n========== DIAGNOSTIC TEST: {} ==========",
            test_name
        ));
        self.log_to_file("CRITICAL: Testing if API filters by subjectID/category");
        self.log_to_file("Previous requests returned ONLY subjectID=7717");
        self.log_to_file("Trying to request ALL categories at once");

        let base_url = if let Some(ref supplier_id) = connection.supplier_id {
            if supplier_id.starts_with("http") {
                supplier_id.trim_end_matches('/')
            } else {
                "https://content-api.wildberries.ru"
            }
        } else {
            "https://content-api.wildberries.ru"
        };

        let url = format!("{}/content/v2/get/cards/list", base_url);

        // Попробуем РЎРћР’РЎР•Рњ минимальный запрос - без cursor вообще
        let body = format!(r#"{{"limit":{}}}"#, limit);
        self.log_to_file(&format!("Minimal request (no cursor at all): {}", body));
        let request_body_len = body.len() as u64;

        let response = match self
            .client
            .post(&url)
            .header("Authorization", &connection.api_key)
            .header("Content-Type", "application/json")
            .body(body)
            .send()
            .await
        {
            Ok(resp) => resp,
            Err(e) => {
                self.log_to_file(&format!("Request failed: {}", e));
                return DiagnosticResult {
                    test_name: test_name.to_string(),
                    success: false,
                    error: Some(format!("HTTP request failed: {}", e)),
                    total_returned: 0,
                    cursor_total: 0,
                    response_headers: None,
                };
            }
        };

        let status = response.status();
        let headers = response.headers().clone();
        self.log_to_file(&format!("Response status: {}", status));

        if !status.is_success() {
            let body = self
                .read_body_tracked_with_request_bytes(response, request_body_len)
                .await
                .unwrap_or_default();
            self.log_to_file(&format!("Error response body: {}", body));
            return DiagnosticResult {
                test_name: test_name.to_string(),
                success: false,
                error: Some(format!("API returned status {}: {}", status, body)),
                total_returned: 0,
                cursor_total: 0,
                response_headers: Some(format!("{:?}", headers)),
            };
        }

        let body = match self
            .read_body_tracked_with_request_bytes(response, request_body_len)
            .await
        {
            Ok(b) => b,
            Err(e) => {
                return DiagnosticResult {
                    test_name: test_name.to_string(),
                    success: false,
                    error: Some(format!("Failed to read response body: {}", e)),
                    total_returned: 0,
                    cursor_total: 0,
                    response_headers: Some(format!("{:?}", headers)),
                };
            }
        };

        self.log_to_file(&format!("Response body: {}", body));

        match serde_json::from_str::<WildberriesProductListResponse>(&body) {
            Ok(data) => {
                // Проверяем уникальные subjectID
                let mut unique_subjects = std::collections::HashSet::new();
                for card in &data.cards {
                    unique_subjects.insert(card.subject_id);
                }

                self.log_to_file(&format!(
                    "вњ“ Success: {} items, cursor.total={}",
                    data.cards.len(),
                    data.cursor.total
                ));
                self.log_to_file(&format!(
                    "IMPORTANT: Found {} unique subjectIDs: {:?}",
                    unique_subjects.len(),
                    unique_subjects
                ));

                if unique_subjects.len() == 1 {
                    self.log_to_file(
                        "вљ пёЏ WARNING: Still only ONE subjectID! API might be filtering by category.",
                    );
                } else {
                    self.log_to_file(&format!(
                        "вњ“ GOOD: Multiple subjectIDs found! This approach might work."
                    ));
                }

                DiagnosticResult {
                    test_name: test_name.to_string(),
                    success: true,
                    error: None,
                    total_returned: data.cards.len() as i32,
                    cursor_total: data.cursor.total as i32,
                    response_headers: Some(format!("{:?}", headers)),
                }
            }
            Err(e) => {
                self.log_to_file(&format!("Failed to parse response: {}", e));
                DiagnosticResult {
                    test_name: test_name.to_string(),
                    success: false,
                    error: Some(format!("Failed to parse JSON: {}", e)),
                    total_returned: 0,
                    cursor_total: 0,
                    response_headers: Some(format!("{:?}", headers)),
                }
            }
        }
    }

    /// Записать в лог-файл
    fn log_to_file(&self, message: &str) {
        if let Ok(mut file) = OpenOptions::new()
            .create(true)
            .append(true)
            .open("wildberries_api_requests.log")
        {
            let timestamp = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S%.3f");
            let _ = writeln!(file, "[{}] {}", timestamp, message);
        }
    }

    /// Получить список товаров через POST /content/v2/get/cards/list
    pub async fn fetch_product_list(
        &self,
        connection: &ConnectionMP,
        limit: i32,
        cursor: Option<WildberriesCursor>,
    ) -> Result<WildberriesProductListResponse> {
        // РСЃРїРѕР»СЊР·СѓРµРј URL из настроек подключения, если задан, иначе default
        let base_url = if let Some(ref supplier_id) = connection.supplier_id {
            if supplier_id.starts_with("http") {
                // Если supplier_id содержит полный URL, используем его как base URL
                supplier_id.trim_end_matches('/')
            } else {
                "https://content-api.wildberries.ru"
            }
        } else {
            "https://content-api.wildberries.ru"
        };

        let url = format!("{}/content/v2/get/cards/list", base_url);

        if connection.api_key.trim().is_empty() {
            anyhow::bail!("API Key is required for Wildberries API");
        }

        self.log_to_file(&format!("Using API URL: {}", url));

        // Wildberries Content API v2 ожидает limit ВНУТРИ settings.cursor.limit,
        // а в фильтре обязательно withPhoto (-1 = все карточки, иначе WB режет выдачу).
        let mut request_cursor = cursor.unwrap_or_default();
        request_cursor.limit = Some(limit);
        let request_body = WildberriesProductListRequest {
            settings: WildberriesSettings {
                cursor: request_cursor,
                filter: WildberriesFilter {
                    find_by_nm_id: None,
                    with_photo: Some(-1),
                },
            },
            limit,
        };

        let body = serde_json::to_string(&request_body)?;
        self.log_to_file(&format!(
            "=== REQUEST ===\nPOST {}\nAuthorization: ****\nBody: {}",
            url, body
        ));
        let request_body_len = body.len() as u64;

        let response = match self
            .client
            .post(&url)
            .header("Authorization", &connection.api_key)
            .header("Content-Type", "application/json")
            .body(body)
            .send()
            .await
        {
            Ok(resp) => resp,
            Err(e) => {
                let error_msg = format!("HTTP request failed: {:?}", e);
                self.log_to_file(&error_msg);
                tracing::error!("Wildberries API connection error: {}", e);

                // Проверяем конкретные типы ошибок
                if e.is_timeout() {
                    anyhow::bail!("Request timeout: API не ответил в течение 60 секунд");
                } else if e.is_connect() {
                    anyhow::bail!("Connection error: не удалось подключиться к серверу WB. Проверьте интернет-соединение.");
                } else if e.is_request() {
                    anyhow::bail!("Request error: проблема при отправке запроса - {}", e);
                } else {
                    anyhow::bail!("Unknown error: {}", e);
                }
            }
        };

        let status = response.status();
        self.log_to_file(&format!("Response status: {}", status));

        if !status.is_success() {
            let body = self
                .read_body_tracked_with_request_bytes(response, request_body_len)
                .await
                .unwrap_or_default();
            self.log_to_file(&format!("ERROR Response body:\n{}", body));
            tracing::error!("Wildberries API request failed: {}", body);
            anyhow::bail!(
                "Wildberries API request failed with status {}: {}",
                status,
                body
            );
        }

        let body = self
            .read_body_tracked_with_request_bytes(response, request_body_len)
            .await?;
        self.log_to_file(&format!("=== RESPONSE BODY ===\n{}\n", body));

        let preview: String = body.chars().take(500).collect::<String>();
        let preview = if preview.len() < body.len() {
            format!("{}...", preview)
        } else {
            preview
        };
        tracing::debug!("Wildberries API response preview: {}", preview);

        match serde_json::from_str::<WildberriesProductListResponse>(&body) {
            Ok(data) => {
                let cursor_str = data
                    .cursor
                    .updated_at
                    .as_ref()
                    .map(|s| s.as_str())
                    .unwrap_or("none");

                self.log_to_file(&format!(
                    "=== PARSED RESPONSE ===\nItems: {}\nCursor.total: {}\nCursor.updatedAt: {}\nCursor.nmID: {:?}",
                    data.cards.len(),
                    data.cursor.total,
                    cursor_str,
                    data.cursor.nm_id
                ));

                if data.cards.is_empty() {
                    self.log_to_file("вљ  WARNING: Empty cards array - no more products!");
                } else {
                    let first_nm_id = data.cards.first().map(|c| c.nm_id);
                    let last_nm_id = data.cards.last().map(|c| c.nm_id);
                    self.log_to_file(&format!(
                        "Product range: first nmID={:?}, last nmID={:?}",
                        first_nm_id, last_nm_id
                    ));
                }

                tracing::info!(
                    "Wildberries API response: {} items, total: {}, cursor: updatedAt={}, nmID={:?}",
                    data.cards.len(),
                    data.cursor.total,
                    cursor_str,
                    data.cursor.nm_id
                );
                Ok(data)
            }
            Err(e) => {
                let error_msg = format!("Failed to parse Wildberries API JSON: {}", e);
                self.log_to_file(&error_msg);
                tracing::error!("Failed to parse Wildberries API response. Error: {}", e);
                tracing::error!("Response body: {}", body);
                anyhow::bail!(
                    "Failed to parse Wildberries API JSON: {}. Response: {}",
                    e,
                    preview
                )
            }
        }
    }

    /// Получить данные по продажам через Statistics API
    /// GET /api/v1/supplier/sales
    /// ВАЖНО: Загружает Р’РЎР• записи с учетом пагинации API
    pub async fn fetch_sales(
        &self,
        connection: &ConnectionMP,
        date_from: chrono::NaiveDate,
        date_to: chrono::NaiveDate,
    ) -> Result<Vec<(WbSaleRow, String)>> {
        let url = "https://statistics-api.wildberries.ru/api/v1/supplier/sales";

        if connection.api_key.trim().is_empty() {
            anyhow::bail!("API Key is required for Wildberries API");
        }

        let date_from_str = date_from.format("%Y-%m-%d").to_string();
        let date_to_str = date_to.format("%Y-%m-%d").to_string();

        // API Wildberries Statistics может возвращать до 100,000 записей за запрос,
        // но рекомендуется делать запросы с флагом page для пагинации
        // РЎРѕРіР»Р°СЃРЅРѕ документации: если записей больше, то нужно делать повторные запросы
        // используя параметр flag=1 для получения следующих страниц

        let mut all_sales: Vec<(WbSaleRow, String)> = Vec::new();
        let mut page_flag = 0; // 0 = первая страница, 1 = следующие страницы

        self.log_to_file(&format!(
            "\nв•”в•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•—"
        ));
        self.log_to_file(&format!("в•‘ WILDBERRIES SALES API - LOADING ALL RECORDS"));
        self.log_to_file(&format!("в•‘ Period: {} to {}", date_from_str, date_to_str));
        self.log_to_file(&format!(
            "в•љв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ќ"
        ));

        loop {
            self.log_to_file(&format!(
                "\nв”Њв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”ђ"
            ));
            self.log_to_file(&format!(
                "в”‚ Request #{} (flag={})",
                (page_flag + 1),
                page_flag
            ));

            self.log_to_file(&format!(
                "=== REQUEST ===\nGET {}?dateFrom={}&dateTo={}&flag={}\nAuthorization: ****",
                url, date_from_str, date_to_str, page_flag
            ));

            self.record_http_request_attempt(0);
            let response = match self
                .client
                .get(url)
                .header("Authorization", &connection.api_key)
                .query(&[
                    ("dateFrom", date_from_str.as_str()),
                    ("dateTo", date_to_str.as_str()),
                    ("flag", &page_flag.to_string()),
                ])
                .send()
                .await
            {
                Ok(resp) => resp,
                Err(e) => {
                    let error_msg = format!("HTTP request failed: {:?}", e);
                    self.log_to_file(&error_msg);
                    tracing::error!("Wildberries Sales API connection error: {}", e);

                    // Проверяем конкретные типы ошибок
                    if e.is_timeout() {
                        anyhow::bail!("Request timeout: API не ответил в течение 60 секунд");
                    } else if e.is_connect() {
                        anyhow::bail!("Connection error: не удалось подключиться к серверу WB. Проверьте интернет-соединение.");
                    } else if e.is_request() {
                        anyhow::bail!("Request error: проблема при отправке запроса - {}", e);
                    } else {
                        anyhow::bail!("Unknown error: {}", e);
                    }
                }
            };

            let status = response.status();
            self.log_to_file(&format!("Response status: {}", status));

            if !status.is_success() {
                let body = self
                    .read_body_for_recorded_request(response)
                    .await
                    .unwrap_or_default();
                self.log_to_file(&format!("ERROR Response body:\n{}", body));
                tracing::error!("Wildberries Sales API request failed: {}", body);
                anyhow::bail!(
                    "Wildberries Sales API failed with status {}: {}",
                    status,
                    body
                );
            }

            let body = self.read_body_for_recorded_request(response).await?;
            let body_preview = if body.chars().count() > 5000 {
                let preview: String = body.chars().take(5000).collect();
                format!("{}... (total {} chars)", preview, body.len())
            } else {
                body.clone()
            };
            self.log_to_file(&format!(
                "=== RESPONSE BODY PREVIEW ===\n{}\n",
                body_preview
            ));

            match serde_json::from_str::<Vec<WbSaleRow>>(&body) {
                Ok(page_data) => {
                    let page_count = page_data.len();
                    self.log_to_file(&format!("в”‚ Received: {} records", page_count));
                    self.log_to_file(&format!(
                        "в”‚ Total so far: {} records",
                        all_sales.len() + page_count
                    ));

                    if page_data.is_empty() {
                        self.log_to_file(&format!("в”‚ вњ“ Empty response - all records loaded"));
                        self.log_to_file(&format!(
                            "в””в”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”"
                        ));
                        break;
                    }

                    // Парсим тело как массив serde_json::Value для сохранения оригинального JSON
                    // Если не получается вЂ” используем пустой объект как fallback
                    let raw_values: Vec<serde_json::Value> =
                        serde_json::from_str(&body).unwrap_or_default();

                    let page_pairs: Vec<(WbSaleRow, String)> = page_data
                        .into_iter()
                        .zip(raw_values.into_iter())
                        .map(|(row, raw_val)| {
                            let raw_str = serde_json::to_string(&raw_val)
                                .unwrap_or_else(|_| "{}".to_string());
                            (row, raw_str)
                        })
                        .collect();

                    // Добавляем полученные данные
                    all_sales.extend(page_pairs);

                    // API WB Statistics возвращает максимум 100,000 записей за запрос
                    // Если получили меньше, значит это последняя страница
                    if page_count < 100000 {
                        self.log_to_file(&format!(
                            "в”‚ вњ“ Received {} records (less than limit) - last page",
                            page_count
                        ));
                        self.log_to_file(&format!(
                            "в””в”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”"
                        ));
                        break;
                    }

                    self.log_to_file(&format!(
                        "в”‚ в†’ More records may be available, requesting next page..."
                    ));
                    self.log_to_file(&format!(
                        "в””в”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”"
                    ));

                    // Переходим к следующей странице
                    page_flag = 1;
                }
                Err(e) => {
                    self.log_to_file(&format!("Failed to parse JSON: {}", e));
                    tracing::error!("Failed to parse Wildberries sales response: {}", e);
                    anyhow::bail!("Failed to parse sales response: {}", e)
                }
            }

            // Небольшая задержка между запросами для снижения нагрузки на API
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        }

        self.log_to_file(&format!(
            "\nв•”в•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•—"
        ));
        self.log_to_file(&format!(
            "в•‘ COMPLETED: Loaded {} total sale records",
            all_sales.len()
        ));
        // all_sales содержит пары (WbSaleRow, raw_json_string)
        self.log_to_file(&format!(
            "в•љв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ќ\n"
        ));

        tracing::info!(
            "вњ“ Wildberries Sales API: Successfully loaded {} total records for period {} to {}",
            all_sales.len(),
            date_from_str,
            date_to_str
        );

        Ok(all_sales)
    }

    /// Загрузить финансовые отчеты из Wildberries по периоду (reportDetailByPeriod)
    /// Возвращает только ЕЖЕДНЕВНЫЕ отчеты (report_type = 1)
    ///
    /// ВАЖНО: API имеет лимит 1 запрос в минуту!
    /// РСЃРїРѕР»СЊР·СѓРµС‚СЃСЏ пагинация через rrdid для загрузки больших объемов данных.
    pub async fn fetch_finance_report_by_period(
        &self,
        connection: &ConnectionMP,
        date_from: chrono::NaiveDate,
        date_to: chrono::NaiveDate,
    ) -> Result<Vec<WbFinanceReportRow>> {
        let url = "https://statistics-api.wildberries.ru/api/v5/supplier/reportDetailByPeriod";

        if connection.api_key.trim().is_empty() {
            anyhow::bail!("API Key is required for Wildberries API");
        }

        let date_from_str = date_from.format("%Y-%m-%d").to_string();
        let date_to_str = date_to.format("%Y-%m-%d").to_string();

        self.log_to_file(&format!(
            "\nв•”в•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•—"
        ));
        self.log_to_file(&format!(
            "в•‘ WILDBERRIES FINANCE REPORT API - reportDetailByPeriod"
        ));
        self.log_to_file(&format!("в•‘ Period: {} to {}", date_from_str, date_to_str));
        self.log_to_file(&format!(
            "в•‘ Rate limit: 1 request per minute (using pagination)"
        ));
        self.log_to_file(&format!(
            "в•љв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ќ"
        ));

        let period = "daily";
        let mut all_daily_reports: Vec<WbFinanceReportRow> = Vec::new();
        let mut rrdid: i64 = 0; // Начинаем с 0 для первой страницы
        let limit = 100000; // Максимальный лимит записей
        let mut page_num = 1;

        loop {
            self.log_to_file(&format!(
                "\nв”Њв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”ђ"
            ));
            self.log_to_file(&format!(
                "в”‚ Page {}: rrdid={}, limit={}",
                page_num, rrdid, limit
            ));
            self.log_to_file(&format!(
                "в””в”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”"
            ));

            self.log_to_file(&format!(
                "=== REQUEST ===\nGET {}?dateFrom={}&dateTo={}&rrdid={}&limit={}&period={}\nAuthorization: ****",
                url, date_from_str, date_to_str, rrdid, limit, period
            ));

            let response = match self
                .client
                .get(url)
                .header("Authorization", &connection.api_key)
                .query(&[
                    ("dateFrom", date_from_str.as_str()),
                    ("dateTo", date_to_str.as_str()),
                    ("rrdid", &rrdid.to_string()),
                    ("limit", &limit.to_string()),
                    ("period", period),
                ])
                .send()
                .await
            {
                Ok(resp) => resp,
                Err(e) => {
                    let error_msg = format!("HTTP request failed: {:?}", e);
                    self.log_to_file(&error_msg);
                    tracing::error!("Wildberries Finance Report API connection error: {}", e);

                    // Проверяем конкретные типы ошибок
                    if e.is_timeout() {
                        anyhow::bail!("Request timeout: API не ответил в течение 60 секунд");
                    } else if e.is_connect() {
                        anyhow::bail!("Connection error: не удалось подключиться к серверу WB. Проверьте интернет-соединение.");
                    } else if e.is_request() {
                        anyhow::bail!("Request error: проблема при отправке запроса - {}", e);
                    } else {
                        anyhow::bail!("Unknown error: {}", e);
                    }
                }
            };

            let status = response.status();
            self.log_to_file(&format!("Response status: {}", status));

            // Обработка 429 Too Many Requests - ждем и повторяем
            if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
                self.log_to_file(&format!(
                    "в”‚ вљ пёЏ Rate limit hit (429). Waiting 65 seconds before retry..."
                ));
                tracing::warn!("WB Finance Report API rate limit hit. Waiting 65 seconds...");
                tokio::time::sleep(tokio::time::Duration::from_secs(65)).await;
                continue;
            }

            // Обработка 204 No Content - нет данных
            if status == reqwest::StatusCode::NO_CONTENT {
                self.log_to_file(&format!("в”‚ No more data (204 No Content)"));
                break;
            }

            if !status.is_success() {
                let body = self
                    .read_body_for_recorded_request(response)
                    .await
                    .unwrap_or_default();
                self.log_to_file(&format!("ERROR Response body:\n{}", body));
                tracing::error!("Wildberries Finance Report API request failed: {}", body);
                anyhow::bail!(
                    "Wildberries Finance Report API failed with status {}: {}",
                    status,
                    body
                );
            }

            let body = self.read_body_tracked(response).await?;

            // Пустой ответ - конец данных
            if body.trim().is_empty() || body.trim() == "[]" {
                self.log_to_file(&format!("в”‚ Empty response - no more data"));
                break;
            }

            let body_preview = if body.chars().count() > 5000 {
                let preview: String = body.chars().take(5000).collect();
                format!("{}... (total {} chars)", preview, body.len())
            } else {
                body.clone()
            };
            self.log_to_file(&format!(
                "=== RESPONSE BODY PREVIEW ===\n{}\n",
                body_preview
            ));

            // Парсим записи
            let page_rows: Vec<WbFinanceReportRow> = match serde_json::from_str(&body) {
                Ok(rows) => rows,
                Err(e) => {
                    self.log_to_file(&format!("Failed to parse JSON: {}", e));
                    tracing::error!("Failed to parse Wildberries finance report response: {}", e);
                    anyhow::bail!("Failed to parse finance report response: {}", e)
                }
            };

            let page_count = page_rows.len();
            self.log_to_file(&format!(
                "в”‚ Received {} records on page {}",
                page_count, page_num
            ));

            if page_count == 0 {
                self.log_to_file(&format!("в”‚ No records on this page - done"));
                break;
            }

            // Находим максимальный rrd_id для следующей страницы
            let max_rrd_id = page_rows.iter().filter_map(|r| r.rrd_id).max().unwrap_or(0);

            // Фильтруем только ЕЖЕДНЕВНЫЕ отчеты (report_type = 1)
            let daily_rows: Vec<WbFinanceReportRow> = page_rows
                .into_iter()
                .filter(|row| row.report_type == Some(1))
                .collect();

            self.log_to_file(&format!(
                "в”‚ Filtered {} daily records (report_type=1)",
                daily_rows.len()
            ));

            all_daily_reports.extend(daily_rows);

            // Если получили меньше записей чем лимит, значит это последняя страница
            if page_count < limit as usize {
                self.log_to_file(&format!(
                    "в”‚ Received {} < {} records - this is the last page",
                    page_count, limit
                ));
                break;
            }

            // Подготовка к следующей странице
            rrdid = max_rrd_id;
            page_num += 1;

            self.log_to_file(&format!(
                "в”‚ в†’ More records may be available. Next rrdid={}",
                rrdid
            ));
            self.log_to_file(&format!(
                "в”‚ вЏі Waiting 65 seconds before next request (rate limit: 1 req/min)..."
            ));

            // ВАЖНО: API имеет лимит 1 запрос в минуту!
            // Ждем 65 секунд для надежности
            tokio::time::sleep(tokio::time::Duration::from_secs(65)).await;
        }

        // Логируем первые 3 записи для проверки загрузки полей
        for (idx, row) in all_daily_reports.iter().take(3).enumerate() {
            self.log_to_file(&format!(
                "\n=== Sample Record {} ===\nrrd_id: {:?}\ncommission_percent: {:?}\nppvz_sales_commission: {:?}\nretail_price_withdisc_rub: {:?}\nretail_amount: {:?}\n",
                idx + 1,
                row.rrd_id,
                row.commission_percent,
                row.ppvz_sales_commission,
                row.retail_price_withdisc_rub,
                row.retail_amount
            ));
            tracing::info!(
                "WB Finance Report sample {}: rrd_id={:?}, commission_percent={:?}, ppvz_sales_commission={:?}",
                idx + 1,
                row.rrd_id,
                row.commission_percent,
                row.ppvz_sales_commission
            );
        }

        self.log_to_file(&format!(
            "\nв•”в•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•—"
        ));
        self.log_to_file(&format!(
            "в•‘ COMPLETED: Loaded {} daily finance report records ({} pages)",
            all_daily_reports.len(),
            page_num
        ));
        self.log_to_file(&format!(
            "в•љв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ќ\n"
        ));

        tracing::info!(
            "вњ“ Wildberries Finance Report API: Successfully loaded {} daily records for period {} to {}",
            all_daily_reports.len(),
            date_from_str,
            date_to_str
        );

        Ok(all_daily_reports)
    }

    /// Получить данные по заказам через Statistics API (Backfill mode)
    /// GET /api/v1/supplier/orders
    ///
    /// РЎС‚СЂР°С‚РµРіРёСЏ:
    /// - flag=0 (инкремент по lastChangeDate)
    /// - dateFrom = курсор lastChangeDate
    /// - для следующей страницы курсор сдвигаем на +1мс от максимального lastChangeDate
    /// - соблюдаем лимит API (1 запрос/мин) и обрабатываем 429
    ///
    /// date_to используется как soft-stop / фильтр.
    pub async fn fetch_orders(
        &self,
        connection: &ConnectionMP,
        date_from: chrono::NaiveDate,
        date_to: chrono::NaiveDate,
    ) -> Result<Vec<WbOrderRow>> {
        let url = "https://statistics-api.wildberries.ru/api/v1/supplier/orders";

        if connection.api_key.trim().is_empty() {
            anyhow::bail!("API Key is required for Wildberries API");
        }

        let mut all_orders = Vec::new();
        let mut page_num = 1;
        let mut cursor = format!("{}T00:00:00", date_from.format("%Y-%m-%d"));
        let soft_stop =
            wb_day_end_utc(date_to).ok_or_else(|| anyhow::anyhow!("Invalid date_to value"))?;
        // Сколько строк WB вообще отдал и какая из них самая ранняя. Нужно, чтобы отличить
        // «за период заказов не было» (WB вернул пусто) от «WB не хранит такую глубину и
        // отдал данные новее запрошенного окна» — во втором случае раньше импорт молча
        // завершался нулём, и это выглядело как зависшая загрузка.
        let mut received_rows = 0usize;
        let mut earliest_change: Option<chrono::DateTime<chrono::Utc>> = None;

        self.log_to_file(&format!(
            "\nв•”в•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•—"
        ));
        self.log_to_file(&format!("в•‘ WILDBERRIES ORDERS API - BACKFILL BY CURSOR"));
        self.log_to_file(&format!("в•‘ Period: {} to {}", date_from, date_to));
        self.log_to_file(&format!("в•‘ API URL: {}", url));
        self.log_to_file(&format!(
            "в•‘ Method: flag=0 with lastChangeDate cursor (1 req/min)"
        ));
        self.log_to_file(&format!(
            "в•љв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ќ"
        ));

        loop {
            self.log_to_file(&format!(
                "\nв”Њв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”ђ"
            ));
            self.log_to_file(&format!(
                "в”‚ Page {}: dateFrom={}, flag=0",
                page_num, cursor
            ));

            self.log_to_file(&format!(
                "=== REQUEST ===\nGET {}?dateFrom={}&flag=0\nAuthorization: ****",
                url, cursor
            ));

            self.set_tracked_current_item(
                "a015_wb_orders",
                format!("WB Orders API: запрос страницы {page_num}, dateFrom={cursor}"),
            );
            self.record_http_request_attempt(0);

            let response = match self
                .client
                .get(url)
                .header("Authorization", &connection.api_key)
                .query(&[("dateFrom", cursor.as_str()), ("flag", "0")])
                .send()
                .await
            {
                Ok(resp) => resp,
                Err(e) => {
                    let error_debug = format!("{e:?}");
                    let error_msg = format!("HTTP request to Orders API failed: {error_debug}");
                    self.log_to_file(&error_msg);
                    tracing::error!("Wildberries Orders API request error: {}", error_debug);
                    self.set_tracked_current_item(
                        "a015_wb_orders",
                        format!("WB Orders API: ошибка запроса страницы {page_num}"),
                    );

                    if e.is_timeout() {
                        anyhow::bail!(
                            "WB Orders API timeout: сервер не ответил за 60 секунд.\n\
                             URL: {url}?dateFrom={cursor}&flag=0\n\
                             Детали: {error_debug}"
                        );
                    } else if e.is_connect() {
                        anyhow::bail!(
                            "WB Orders API connection failed: не удалось установить соединение с statistics-api.wildberries.ru.\n\
                             Это сетевая ошибка до HTTP-ответа, не ответ 429 и не ошибка формата данных.\n\
                             URL: {url}?dateFrom={cursor}&flag=0\n\
                             Проверьте доступ к хосту, DNS/proxy/firewall/VPN и TLS-соединение.\n\
                             Детали: {error_debug}"
                        );
                    } else if e.is_request() {
                        anyhow::bail!(
                            "WB Orders API request build/send error.\n\
                             URL: {url}?dateFrom={cursor}&flag=0\n\
                             Детали: {error_debug}"
                        );
                    } else {
                        anyhow::bail!(
                            "WB Orders API request failed before receiving a response.\n\
                             URL: {url}?dateFrom={cursor}&flag=0\n\
                             Детали: {error_debug}"
                        );
                    }
                }
            };

            let status = response.status();
            let final_url = response.url().clone();
            self.log_to_file(&format!("Response status: {}", status));
            self.log_to_file(&format!("Final URL: {}", final_url));

            if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
                let rate_headers = WbRateLimitHeaders::from_headers(response.headers());
                let wait_secs = rate_headers.retry_seconds.unwrap_or(65).max(1);
                let rate_fields = rate_headers.to_log_fields();
                let body = self
                    .read_body_for_recorded_request(response)
                    .await
                    .unwrap_or_default();
                self.log_to_file(&format!(
                    "в”‚ вљ пёЏ Rate limit hit (429). Waiting {} seconds before retry. X-Ratelimit: {}",
                    wait_secs, rate_fields
                ));
                if !body.trim().is_empty() {
                    self.log_to_file(&format!("Rate limit response body:\n{}", body));
                }

                if wait_secs > WB_ORDERS_MAX_RATE_LIMIT_SLEEP_SECS {
                    let message = format!(
                        "WB Orders API rate limit returned a long retry window: {} seconds. \
                         The task will finish now and can be retried by the next scheduled/manual run. \
                         X-Ratelimit: {}",
                        wait_secs, rate_fields
                    );
                    self.log_to_file(&message);
                    self.set_tracked_current_item(
                        "a015_wb_orders",
                        format!(
                            "WB Orders API: лимит запросов (429), WB просит ждать {} сек.; задача завершена",
                            wait_secs
                        ),
                    );
                    tracing::warn!("{}", message);
                    anyhow::bail!("WB_RATE_LIMIT_DEFERRED: {}", message);
                }

                self.set_tracked_current_item(
                    "a015_wb_orders",
                    format!(
                        "WB Orders API: лимит запросов (429), ожидание {} сек. {}",
                        wait_secs, rate_fields
                    ),
                );
                tracing::warn!(
                    "WB Orders API rate limit hit. Waiting {} seconds. X-Ratelimit: {}",
                    wait_secs,
                    rate_fields
                );
                tokio::time::sleep(tokio::time::Duration::from_secs(wait_secs)).await;
                continue;
            }

            // Логируем заголовки ответа для диагностики
            self.log_to_file(&format!("Response headers:"));
            for (name, value) in response.headers() {
                if let Ok(val_str) = value.to_str() {
                    self.log_to_file(&format!("  {}: {}", name, val_str));
                }
            }

            if !status.is_success() {
                let body = self.read_body_tracked(response).await.unwrap_or_default();
                self.log_to_file(&format!("ERROR Response body:\n{}", body));
                tracing::error!(
                    "Wildberries Orders API request failed for cursor {}: {}",
                    cursor,
                    body
                );

                // РЎРїРµС†РёР°Р»СЊРЅР°СЏ обработка для 302 редиректов
                if status.as_u16() == 302 || status.as_u16() == 301 {
                    anyhow::bail!(
                        "Wildberries Orders API returned redirect {} for cursor {}. \
                        This may indicate:\n\
                        1. Incorrect API endpoint URL\n\
                        2. Missing or invalid authentication\n\
                        3. API endpoint has moved\n\
                        Response: {}\n\
                        Check Wildberries API documentation for the correct endpoint.",
                        status,
                        cursor,
                        body
                    );
                }

                anyhow::bail!(
                    "Wildberries Orders API failed with status {} for cursor {}: {}",
                    status,
                    cursor,
                    body
                );
            }

            // Читаем тело ответа
            let body = match self.read_body_for_recorded_request(response).await {
                Ok(b) => b,
                Err(e) => {
                    self.log_to_file(&format!("в”‚ вљ пёЏ Failed to read response body: {}", e));
                    tracing::error!("Failed to read response body for cursor {}: {}", cursor, e);
                    anyhow::bail!("Failed to read response body: {}", e);
                }
            };

            self.log_to_file(&format!("Body length: {} bytes", body.len()));

            // Проверяем, не пустой ли ответ
            let body_trimmed = body.trim();
            if body_trimmed.is_empty() || body_trimmed == "[]" {
                self.log_to_file(&format!("в”‚ Empty response, all records loaded"));
                self.log_to_file(&format!("в”‚ Total so far: {} records", all_orders.len()));
                self.log_to_file(&format!(
                    "в””в”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”"
                ));
                break;
            }

            let body_preview = if body.chars().count() > 5000 {
                let preview: String = body.chars().take(5000).collect();
                format!("{}... (total {} chars)", preview, body.len())
            } else {
                body.clone()
            };
            self.log_to_file(&format!(
                "=== RESPONSE BODY PREVIEW ===\n{}\n",
                body_preview
            ));

            match serde_json::from_str::<Vec<WbOrderRow>>(&body) {
                Ok(page_data) => {
                    let page_count = page_data.len();
                    self.log_to_file(&format!(
                        "в”‚ Received: {} rows on page {}",
                        page_count, page_num
                    ));
                    self.log_to_file(&format!(
                        "в”‚ Total so far: {} records",
                        all_orders.len() + page_count
                    ));
                    self.log_to_file(&format!(
                        "в””в”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”"
                    ));

                    let mut max_last_change = None::<chrono::DateTime<chrono::Utc>>;
                    let mut kept_rows = 0usize;
                    received_rows += page_count;
                    for row in page_data {
                        let row_last_change =
                            row.last_change_date.as_deref().and_then(parse_wb_datetime);

                        if let Some(parsed) = row_last_change {
                            if max_last_change.map(|v| parsed > v).unwrap_or(true) {
                                max_last_change = Some(parsed);
                            }
                            if earliest_change.map(|v| parsed < v).unwrap_or(true) {
                                earliest_change = Some(parsed);
                            }
                        }

                        // soft-stop по date_to: строки после date_to не включаем
                        let include_row = row_last_change.map(|dt| dt <= soft_stop).unwrap_or(true);
                        if include_row {
                            all_orders.push(row);
                            kept_rows += 1;
                        }
                    }

                    self.log_to_file(&format!(
                        "в”‚ Kept {} rows after soft-stop filter",
                        kept_rows
                    ));

                    let Some(max_dt) = max_last_change else {
                        self.log_to_file("в”‚ No lastChangeDate found on page; stopping");
                        break;
                    };

                    if max_dt > soft_stop {
                        self.log_to_file(&format!(
                            "в”‚ Soft-stop reached (max lastChangeDate {} > date_to {})",
                            max_dt, soft_stop
                        ));
                        break;
                    }

                    let next_cursor_dt = max_dt + chrono::Duration::milliseconds(1);
                    cursor = format_wb_cursor_datetime(&next_cursor_dt);
                    page_num += 1;
                }
                Err(e) => {
                    self.log_to_file(&format!("Failed to parse JSON: {}", e));
                    self.log_to_file(&format!("Response body: {}", body_preview));
                    tracing::error!("Failed to parse Wildberries orders response: {}", e);
                    anyhow::bail!("Failed to parse orders response: {}", e)
                }
            }

            // Лимит WB Statistics: 1 запрос в минуту
            tokio::time::sleep(tokio::time::Duration::from_secs(65)).await;
        }

        self.log_to_file(&format!(
            "\nв•”в•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•—"
        ));
        // WB игнорирует dateFrom за пределами своей глубины хранения и отдаёт самые старые
        // из доступных строк. Тогда soft-stop отсеивает вообще всё, и «успешный» импорт
        // приносит 0 записей. Это не успех — это недоступный период, и сказать об этом надо
        // прямо, вместе с фактической границей, до которой WB ещё отдаёт заказы.
        if let Some(message) = unavailable_orders_period_message(
            date_from,
            date_to,
            all_orders.len(),
            received_rows,
            earliest_change,
        ) {
            self.log_to_file(&format!("в•‘ ABORT: {message}"));
            anyhow::bail!("{message}");
        }

        self.log_to_file(&format!(
            "в•‘ COMPLETED: Loaded {} total order records",
            all_orders.len()
        ));
        self.log_to_file(&format!(
            "в•љв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ќ\n"
        ));

        tracing::info!(
            "вњ“ Wildberries Orders API: Successfully loaded {} total records for period {} to {}",
            all_orders.len(),
            date_from,
            date_to
        );

        Ok(all_orders)
    }

    pub async fn fetch_documents_list(
        &self,
        connection: &ConnectionMP,
        date_from: chrono::NaiveDate,
        date_to: chrono::NaiveDate,
    ) -> Result<Vec<WbDocumentListItem>> {
        // WB Documents List API: empirical limit ~1 req/10 s (burst 5).
        // We add an inter-page delay of 11 s.
        //
        // ВАЖНО: API не гарантирует фильтрацию по дате через beginTime/endTime.
        // Сортировка desc по дате позволяет сделать early-exit: как только видим документ
        // старше date_from — дальше не идём, т.к. всё остальное ещё старше.
        const PAGE_DELAY_SECS: u64 = 11;
        // Максимум попыток на одну страницу при 429 (защита от вечной петли).
        const MAX_RETRIES_PER_PAGE: u32 = 3;
        const RATE_LIMIT_DEFAULT_WAIT_SECS: u64 = 15;
        // Если API говорит ждать больше этого порога — исчерпана дневная квота;
        // немедленно возвращаем ошибку, чтобы не ждать часами.
        const QUOTA_EXHAUSTED_THRESHOLD_SECS: u64 = 300; // 5 минут

        let url = "https://documents-api.wildberries.ru/api/v1/documents/list";

        if connection.api_key.trim().is_empty() {
            anyhow::bail!("API Key is required for Wildberries API");
        }

        let begin_time = date_from.format("%Y-%m-%d").to_string();
        let end_time = date_to.format("%Y-%m-%d").to_string();
        let limit = 50usize;
        let mut offset = 0usize;
        let mut all_documents = Vec::new();

        'pages: loop {
            // Delay before every page except the very first.
            if offset > 0 {
                tokio::time::sleep(tokio::time::Duration::from_secs(PAGE_DELAY_SECS)).await;
            }

            let mut retries = 0u32;
            let batch = loop {
                let response = self
                    .client
                    .get(url)
                    .header("Authorization", &connection.api_key)
                    .query(&[
                        ("locale", "ru"),
                        ("beginTime", begin_time.as_str()),
                        ("endTime", end_time.as_str()),
                        ("sort", "date"),
                        ("order", "desc"),
                        ("limit", &limit.to_string()),
                        ("offset", &offset.to_string()),
                    ])
                    .send()
                    .await
                    .map_err(|e| anyhow::anyhow!("Failed to fetch WB documents list: {}", e))?;

                let status = response.status();

                if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
                    let retry_after = response
                        .headers()
                        .get("X-Ratelimit-Retry")
                        .and_then(|v| v.to_str().ok())
                        .and_then(|s| s.parse::<u64>().ok())
                        .unwrap_or(RATE_LIMIT_DEFAULT_WAIT_SECS);

                    // Если API просит ждать слишком долго — дневная квота исчерпана.
                    // Немедленно возвращаем ошибку вместо многочасового ожидания.
                    // Префикс QUOTA_EXHAUSTED: позволяет worker-у отложить следующий запуск на 24ч.
                    if retry_after > QUOTA_EXHAUSTED_THRESHOLD_SECS {
                        anyhow::bail!(
                            "QUOTA_EXHAUSTED: WB Documents API: дневная квота исчерпана. \
                             API требует ждать {} с (~{} ч). \
                             Следующий запуск автоматически перенесён на 24 ч.",
                            retry_after,
                            retry_after / 3600
                        );
                    }

                    retries += 1;
                    if retries > MAX_RETRIES_PER_PAGE {
                        anyhow::bail!(
                            "WB Documents List API: превышено {} попыток при rate-limit (offset={}). \
                             Задача остановлена.",
                            MAX_RETRIES_PER_PAGE, offset
                        );
                    }
                    let wait_secs = retry_after.max(RATE_LIMIT_DEFAULT_WAIT_SECS);
                    tracing::warn!(
                        "WB Documents API 429 (попытка {}/{}): ждём {} с (offset={}).",
                        retries,
                        MAX_RETRIES_PER_PAGE,
                        wait_secs,
                        offset
                    );
                    tokio::time::sleep(tokio::time::Duration::from_secs(wait_secs)).await;
                    continue;
                }

                if !status.is_success() {
                    let body = self.read_body_tracked(response).await.unwrap_or_default();
                    anyhow::bail!(
                        "Wildberries documents list failed with status {}: {}",
                        status,
                        body
                    );
                }

                let body = self.read_body_tracked(response).await?;
                let parsed: WbDocumentsListResponse = serde_json::from_str(&body).map_err(|e| {
                    anyhow::anyhow!("Failed to parse WB documents list response: {}", e)
                })?;
                break parsed.data.documents;
            };

            let batch_len = batch.len();

            // Применяем клиентскую фильтрацию по дате и early-exit.
            // API сортирует desc, поэтому первый документ старше date_from = конец диапазона.
            for doc in batch {
                // creation_time может быть "YYYY-MM-DD" или "YYYY-MM-DDTHH:MM:SSZ"
                let doc_date = doc
                    .creation_time
                    .get(..10)
                    .and_then(|s| chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").ok());

                if let Some(d) = doc_date {
                    if d > date_to {
                        // Документ новее окна — пропускаем (API мог вернуть лишнее)
                        continue;
                    }
                    if d < date_from {
                        // Документ старше окна — дальше всё ещё старше, останавливаемся
                        tracing::debug!(
                            "WB Documents: early-exit на дате {} (date_from={}), offset={}",
                            d,
                            date_from,
                            offset
                        );
                        break 'pages;
                    }
                }
                all_documents.push(doc);
            }

            if batch_len < limit {
                break;
            }

            offset += limit;
        }

        Ok(all_documents)
    }

    pub async fn download_document(
        &self,
        connection: &ConnectionMP,
        service_name: &str,
        extension: &str,
    ) -> Result<WbDocumentDownloadFile> {
        let url = "https://documents-api.wildberries.ru/api/v1/documents/download";

        if connection.api_key.trim().is_empty() {
            anyhow::bail!("API Key is required for Wildberries API");
        }

        let response = self
            .client
            .get(url)
            .header("Authorization", &connection.api_key)
            .query(&[("serviceName", service_name), ("extension", extension)])
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to download WB document: {}", e))?;

        let status = response.status();
        if !status.is_success() {
            let body = self.read_body_tracked(response).await.unwrap_or_default();
            anyhow::bail!(
                "Wildberries document download failed with status {}: {}",
                status,
                body
            );
        }

        let body = self.read_body_tracked(response).await?;
        let parsed: WbDocumentDownloadResponse = serde_json::from_str(&body)
            .map_err(|e| anyhow::anyhow!("Failed to parse WB document download response: {}", e))?;

        Ok(parsed.data)
    }

    /// Получить тарифы комиссий по категориям
    /// GET https://common-api.wildberries.ru/api/v1/tariffs/commission?locale=ru
    ///
    /// Требует авторизацию через API ключ
    pub async fn fetch_commission_tariffs(
        &self,
        connection: &ConnectionMP,
    ) -> Result<Vec<CommissionTariffRow>> {
        let url = "https://common-api.wildberries.ru/api/v1/tariffs/commission?locale=ru";

        if connection.api_key.trim().is_empty() {
            anyhow::bail!("API Key is required for Wildberries Commission Tariffs API");
        }

        self.log_to_file(&format!(
            "\nв•”в•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•—"
        ));
        self.log_to_file(&format!("в•‘ WILDBERRIES COMMISSION TARIFFS API"));
        self.log_to_file(&format!("в•‘ URL: {}", url));
        self.log_to_file(&format!("в•‘ Method: GET (requires Authorization header)"));
        self.log_to_file(&format!(
            "в•љв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ђв•ќ"
        ));

        self.log_to_file(&format!(
            "=== REQUEST ===\nGET {}\nAuthorization: ****",
            url
        ));

        let response = match self
            .client
            .get(url)
            .header("Authorization", &connection.api_key)
            .send()
            .await
        {
            Ok(resp) => resp,
            Err(e) => {
                let error_msg = format!("HTTP request failed: {:?}", e);
                self.log_to_file(&error_msg);
                tracing::error!("Wildberries Commission Tariffs API connection error: {}", e);

                // Проверяем конкретные типы ошибок
                if e.is_timeout() {
                    anyhow::bail!("Request timeout: API не ответил в течение 60 секунд");
                } else if e.is_connect() {
                    anyhow::bail!("Connection error: не удалось подключиться к серверу WB. Проверьте интернет-соединение.");
                } else if e.is_request() {
                    anyhow::bail!("Request error: проблема при отправке запроса - {}", e);
                } else {
                    anyhow::bail!("Unknown error: {}", e);
                }
            }
        };

        let status = response.status();
        self.log_to_file(&format!("Response status: {}", status));

        if !status.is_success() {
            let body = self.read_body_tracked(response).await.unwrap_or_default();
            self.log_to_file(&format!("ERROR Response body:\n{}", body));
            tracing::error!(
                "Wildberries Commission Tariffs API request failed: {}",
                body
            );
            anyhow::bail!(
                "Wildberries Commission Tariffs API failed with status {}: {}",
                status,
                body
            );
        }

        let body = self.read_body_tracked(response).await?;
        self.log_to_file(&format!("=== RESPONSE BODY ===\n{}\n", body));

        // Parse JSON response
        let parsed: CommissionTariffResponse = serde_json::from_str(&body).map_err(|e| {
            self.log_to_file(&format!("ERROR: Failed to parse JSON: {}", e));
            anyhow::anyhow!("Failed to parse commission tariffs response: {}", e)
        })?;

        self.log_to_file(&format!(
            "вњ“ Successfully parsed {} commission tariff records",
            parsed.report.len()
        ));

        tracing::info!(
            "вњ“ Wildberries Commission Tariffs API: Successfully loaded {} tariff records",
            parsed.report.len()
        );

        Ok(parsed.report)
    }

    /// Получить страницу цен товаров из WB Prices API
    /// GET https://discounts-prices-api.wildberries.ru/api/v2/list/goods/filter?limit=N&offset=N
    pub async fn fetch_goods_prices(
        &self,
        connection: &ConnectionMP,
        limit: i32,
        offset: i32,
    ) -> Result<Vec<WbGoodsPriceRow>> {
        let url = format!(
            "https://discounts-prices-api.wildberries.ru/api/v2/list/goods/filter?limit={}&offset={}",
            limit, offset
        );

        if connection.api_key.trim().is_empty() {
            anyhow::bail!("API Key is required for Wildberries Prices API");
        }

        self.log_to_file(&format!(
            "=== REQUEST ===\nGET {}\nAuthorization: ****",
            url
        ));

        let response = match self
            .client
            .get(&url)
            .header("Authorization", &connection.api_key)
            .send()
            .await
        {
            Ok(resp) => resp,
            Err(e) => {
                let error_msg = format!("HTTP request failed: {:?}", e);
                self.log_to_file(&error_msg);
                tracing::error!("Wildberries Prices API connection error: {}", e);
                if e.is_timeout() {
                    anyhow::bail!("Request timeout: WB Prices API не ответил в течение 60 секунд");
                } else if e.is_connect() {
                    anyhow::bail!("Connection error: не удалось подключиться к discounts-prices-api.wildberries.ru");
                } else {
                    anyhow::bail!("Unknown error: {}", e);
                }
            }
        };

        let status = response.status();
        self.log_to_file(&format!("Response status: {}", status));

        if !status.is_success() {
            let body = self.read_body_tracked(response).await.unwrap_or_default();
            self.log_to_file(&format!("ERROR Response body:\n{}", body));
            tracing::error!("Wildberries Prices API request failed: {}", body);
            anyhow::bail!(
                "Wildberries Prices API failed with status {}: {}",
                status,
                body
            );
        }

        let body = self.read_body_tracked(response).await?;
        self.log_to_file(&format!(
            "=== RESPONSE BODY ===\n{}\n",
            &body[..body.len().min(2000)]
        ));

        let parsed: WbGoodsPriceFilterResponse = serde_json::from_str(&body).map_err(|e| {
            self.log_to_file(&format!("ERROR: Failed to parse JSON: {}", e));
            anyhow::anyhow!("Failed to parse WB Prices response: {}", e)
        })?;

        let rows = parsed.data.map(|d| d.list_goods).unwrap_or_default();
        self.log_to_file(&format!("вњ“ Parsed {} goods price rows", rows.len()));
        tracing::info!(
            "WB Prices API: loaded {} rows (offset={})",
            rows.len(),
            offset
        );

        Ok(rows)
    }

    /// GET /api/v1/calendar/promotions вЂ” список акций из WB Calendar API
    pub async fn fetch_calendar_promotions(
        &self,
        connection: &ConnectionMP,
        start_date_time: &str,
        end_date_time: &str,
        all_promo: bool,
    ) -> Result<Vec<WbCalendarPromotion>> {
        let url = format!(
            "https://dp-calendar-api.wildberries.ru/api/v1/calendar/promotions?startDateTime={}&endDateTime={}&allPromo={}",
            start_date_time, end_date_time, all_promo
        );

        if connection.api_key.trim().is_empty() {
            anyhow::bail!("API Key is required for Wildberries Promotion API");
        }

        self.log_to_file(&format!(
            "=== REQUEST ===\nGET {}\nAuthorization: ****",
            url
        ));

        let response = match self
            .client
            .get(&url)
            .header("Authorization", &connection.api_key)
            .send()
            .await
        {
            Ok(resp) => resp,
            Err(e) => {
                let error_msg = format!("HTTP request failed: {:?}", e);
                self.log_to_file(&error_msg);
                tracing::error!("WB Promotion API connection error: {}", e);
                if e.is_timeout() {
                    anyhow::bail!(
                        "Request timeout: WB Promotion API не ответил в течение 60 секунд"
                    );
                } else if e.is_connect() {
                    anyhow::bail!("Connection error: не удалось подключиться к dp-calendar-api.wildberries.ru");
                } else {
                    anyhow::bail!("Unknown error: {}", e);
                }
            }
        };

        let status = response.status();
        self.log_to_file(&format!("Response status: {}", status));

        if !status.is_success() {
            let body = self.read_body_tracked(response).await.unwrap_or_default();
            self.log_to_file(&format!("ERROR Response body:\n{}", body));
            tracing::error!("WB Promotion API request failed: {}", body);
            anyhow::bail!(
                "WB Promotion Calendar API failed with status {}: {}",
                status,
                body
            );
        }

        let body = self.read_body_tracked(response).await?;
        let body_preview: String = body.chars().take(2000).collect();
        self.log_to_file(&format!("=== RESPONSE BODY ===\n{}\n", body_preview));

        let parsed: WbCalendarPromotionsResponse = serde_json::from_str(&body).map_err(|e| {
            let snippet: String = body.chars().take(400).collect();
            self.log_to_file(&format!(
                "ERROR: Failed to parse JSON: {}\nRaw body: {}",
                e, snippet
            ));
            tracing::error!(
                "WB Calendar Promotions parse error: {} | body: {}",
                e,
                snippet
            );
            anyhow::anyhow!("Failed to parse WB Calendar Promotions response: {}", e)
        })?;

        let promotions = if let Some(data) = parsed.data {
            let mut all = data.promotions;
            all.extend(data.upcoming_promos);
            all
        } else {
            vec![]
        };
        self.log_to_file(&format!("вњ“ Parsed {} promotions", promotions.len()));
        tracing::info!("WB Calendar API: loaded {} promotions", promotions.len());

        Ok(promotions)
    }

    /// GET /api/v1/calendar/promotions/details вЂ” детальная информация по списку акций (до 100 ID за раз)
    pub async fn fetch_promotion_details(
        &self,
        connection: &ConnectionMP,
        promotion_ids: &[i64],
    ) -> Result<Vec<WbCalendarPromotionDetail>> {
        if promotion_ids.is_empty() {
            return Ok(vec![]);
        }
        if connection.api_key.trim().is_empty() {
            anyhow::bail!("API Key is required for Wildberries Promotion Details API");
        }

        // Формируем query string: promotionIDs=1&promotionIDs=2&...
        let query: String = promotion_ids
            .iter()
            .map(|id| format!("promotionIDs={}", id))
            .collect::<Vec<_>>()
            .join("&");

        let url = format!(
            "https://dp-calendar-api.wildberries.ru/api/v1/calendar/promotions/details?{}",
            query
        );

        self.log_to_file(&format!(
            "=== REQUEST ===\nGET {}\nAuthorization: ****",
            url
        ));

        let response = match self
            .client
            .get(&url)
            .header("Authorization", &connection.api_key)
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => {
                tracing::error!("WB Promotion Details API connection error: {}", e);
                anyhow::bail!("Promotion details request error: {}", e);
            }
        };

        let status = response.status();
        if !status.is_success() {
            let err_body = self.read_body_tracked(response).await.unwrap_or_default();
            tracing::warn!("WB Promotion Details API failed: {} - {}", status, err_body);
            return Ok(vec![]);
        }

        let body = self.read_body_tracked(response).await?;
        let body_preview: String = body.chars().take(500).collect();
        self.log_to_file(&format!("=== DETAILS RESPONSE ===\n{}\n", body_preview));

        let parsed: WbCalendarPromotionDetailsResponse = match serde_json::from_str(&body) {
            Ok(p) => p,
            Err(e) => {
                let snippet: String = body.chars().take(400).collect();
                tracing::error!(
                    "WB Promotion Details parse error: {} | body: {}",
                    e,
                    snippet
                );
                return Ok(vec![]);
            }
        };

        let details = parsed.data.map(|d| d.promotions).unwrap_or_default();
        tracing::info!("WB Promotion Details: {} promotions loaded", details.len());

        Ok(details)
    }

    /// GET /api/v1/calendar/promotions/nomenclatures вЂ” список nmId товаров для акции
    /// Обязательные параметры: promotionID + inAction
    /// Не работает для акций типа "auto"
    pub async fn fetch_promotion_nomenclatures(
        &self,
        connection: &ConnectionMP,
        promotion_id: i64,
        promotion_type: Option<&str>,
    ) -> Result<Vec<i64>> {
        // Автоматические акции не поддерживают этот эндпоинт
        if promotion_type.map(|t| t == "auto").unwrap_or(false) {
            tracing::debug!(
                "Skipping nomenclatures for auto promotion {} (not supported)",
                promotion_id
            );
            return Ok(vec![]);
        }

        if connection.api_key.trim().is_empty() {
            anyhow::bail!("API Key is required for Wildberries Promotion Nomenclatures API");
        }

        let mut all_nm_ids: Vec<i64> = Vec::new();
        let page_size: u32 = 1000;

        // Загружаем оба состояния: участвующие (inAction=true) и подходящие (inAction=false)
        for in_action in [true, false] {
            let mut offset: u32 = 0;
            loop {
                let url = format!(
                    "https://dp-calendar-api.wildberries.ru/api/v1/calendar/promotions/nomenclatures?promotionID={}&inAction={}&limit={}&offset={}",
                    promotion_id, in_action, page_size, offset
                );

                self.log_to_file(&format!(
                    "=== REQUEST ===\nGET {}\nAuthorization: ****",
                    url
                ));

                let response = match self
                    .client
                    .get(&url)
                    .header("Authorization", &connection.api_key)
                    .send()
                    .await
                {
                    Ok(resp) => resp,
                    Err(e) => {
                        let error_msg = format!("HTTP request failed: {:?}", e);
                        self.log_to_file(&error_msg);
                        tracing::error!("WB Promotion Nomenclatures API connection error: {}", e);
                        break;
                    }
                };

                let status = response.status();
                self.log_to_file(&format!("Response status: {}", status));

                if !status.is_success() {
                    let err_body = self.read_body_tracked(response).await.unwrap_or_default();
                    self.log_to_file(&format!("ERROR Response body:\n{}", err_body));
                    tracing::warn!(
                        "WB Promotion Nomenclatures API failed for promotionID={} inAction={}: {} - {}",
                        promotion_id, in_action, status, err_body
                    );
                    break;
                }

                let body = self.read_body_tracked(response).await.unwrap_or_default();
                let body_preview: String = body.chars().take(500).collect();
                self.log_to_file(&format!("=== RESPONSE BODY ===\n{}\n", body_preview));

                let parsed: WbPromotionNomenclaturesResponse = match serde_json::from_str(&body) {
                    Ok(p) => p,
                    Err(e) => {
                        let snippet: String = body.chars().take(400).collect();
                        tracing::error!(
                            "WB Promotion Nomenclatures parse error: {} | body: {}",
                            e,
                            snippet
                        );
                        break;
                    }
                };

                let items = parsed.data.map(|d| d.nomenclatures).unwrap_or_default();

                let page_len = items.len() as u32;
                for item in items {
                    if !all_nm_ids.contains(&item.nm_id) {
                        all_nm_ids.push(item.nm_id);
                    }
                }

                if page_len < page_size {
                    break;
                }
                offset += page_size;
            }
        }

        tracing::info!(
            "WB Promotion Nomenclatures: {} unique nmIds for promotionID={}",
            all_nm_ids.len(),
            promotion_id
        );

        Ok(all_nm_ids)
    }

    /// GET /adv/v1/promotion/count вЂ” получить все advertId рекламных кампаний (статусы 7, 9, 11)
    pub async fn fetch_advert_campaign_ids(&self, connection: &ConnectionMP) -> Result<Vec<i64>> {
        let url = "https://advert-api.wildberries.ru/adv/v1/promotion/count";

        if connection.api_key.trim().is_empty() {
            anyhow::bail!("API Key is required for Wildberries Advert API");
        }

        self.log_to_file(&format!(
            "=== REQUEST ===\nGET {}\nAuthorization: ****",
            url
        ));

        let response = match self
            .client
            .get(url)
            .header("Authorization", &connection.api_key)
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => {
                tracing::error!("WB Advert campaign list connection error: {}", e);
                anyhow::bail!("Connection error for advert campaign list: {}", e);
            }
        };

        let status = response.status();
        let rate_limit = WbRateLimitHeaders::from_headers(response.headers());
        self.log_to_file(&format!("Response status: {}", status));
        self.log_to_file(&format!(
            "Response X-Ratelimit headers: {}",
            rate_limit.to_log_fields()
        ));

        if !status.is_success() {
            let body = self.read_body_tracked(response).await.unwrap_or_default();
            self.log_to_file(&format!("ERROR Response body:\n{}", body));
            tracing::error!(
                "WB Advert campaign list failed: {} - {}{}",
                status,
                body,
                rate_limit.to_error_suffix()
            );
            let body_preview: String = body.chars().take(120).collect();
            let body_preview = body_preview.trim();
            anyhow::bail!(
                "WB Advert API: {} — {}{}",
                status,
                if body_preview.is_empty() {
                    "(пустой ответ)"
                } else {
                    body_preview
                },
                rate_limit.to_error_suffix()
            );
        }

        let body = self.read_body_tracked(response).await?;
        let body_preview: String = body.chars().take(1000).collect();
        self.log_to_file(&format!("=== RESPONSE BODY ===\n{}\n", body_preview));

        let parsed: WbAdvertCampaignListResponse = match serde_json::from_str(&body) {
            Ok(p) => p,
            Err(e) => {
                let snippet: String = body.chars().take(400).collect();
                tracing::error!(
                    "WB Advert campaign list parse error: {} | body: {}",
                    e,
                    snippet
                );
                anyhow::bail!("Failed to parse WB advert campaign list: {}", e);
            }
        };

        let ids: Vec<i64> = parsed
            .adverts
            .clone()
            .unwrap_or_default()
            .into_iter()
            .flat_map(|g| g.advert_list.into_iter().map(|e| e.advert_id))
            .collect();

        tracing::info!("WB Advert: found {} campaign IDs", ids.len());
        self.log_to_file(&format!("вњ“ Found {} advertIds", ids.len()));

        Ok(ids)
    }

    pub async fn fetch_advert_campaign_summaries(
        &self,
        connection: &ConnectionMP,
    ) -> Result<Vec<WbAdvertCampaignSummary>> {
        let url = "https://advert-api.wildberries.ru/adv/v1/promotion/count";

        if connection.api_key.trim().is_empty() {
            anyhow::bail!("API Key is required for Wildberries Advert API");
        }

        let response = self
            .client
            .get(url)
            .header("Authorization", &connection.api_key)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("Connection error for advert campaign list: {}", e))?;

        let status = response.status();
        let rate_limit = WbRateLimitHeaders::from_headers(response.headers());
        self.log_to_file(&format!(
            "WB Advert campaign summaries X-Ratelimit headers: {}",
            rate_limit.to_log_fields()
        ));
        if !status.is_success() {
            let body = self.read_body_tracked(response).await.unwrap_or_default();
            let body_preview: String = body.chars().take(120).collect();
            anyhow::bail!(
                "WB Advert API: {} — {}{}",
                status,
                if body_preview.trim().is_empty() {
                    "(пустой ответ)"
                } else {
                    body_preview.trim()
                },
                rate_limit.to_error_suffix()
            );
        }

        let body = self.read_body_tracked(response).await?;
        let parsed: WbAdvertCampaignListResponse = serde_json::from_str(&body).map_err(|e| {
            let snippet: String = body.chars().take(400).collect();
            anyhow::anyhow!(
                "Failed to parse WB advert campaign list: {} | body: {}",
                e,
                snippet
            )
        })?;

        let mut result = Vec::new();
        for group in parsed.adverts.unwrap_or_default() {
            for entry in group.advert_list {
                result.push(WbAdvertCampaignSummary {
                    advert_id: entry.advert_id,
                    campaign_type: group.campaign_type,
                    status: group.status,
                    change_time: entry.change_time,
                });
            }
        }
        Ok(result)
    }

    /// GET /api/advert/v2/adverts — настройки кампаний, включая места размещения.
    pub async fn fetch_advert_campaigns(
        &self,
        connection: &ConnectionMP,
        ids: &[i64],
    ) -> Result<Vec<WbAdvertCampaign>> {
        if ids.is_empty() {
            return Ok(vec![]);
        }

        let ids_str = ids
            .iter()
            .map(|id| id.to_string())
            .collect::<Vec<_>>()
            .join(",");
        let url = format!(
            "https://advert-api.wildberries.ru/api/advert/v2/adverts?ids={}",
            ids_str
        );

        let response = self
            .client
            .get(&url)
            .header("Authorization", &connection.api_key)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("Connection error for advert campaigns: {}", e))?;

        let status = response.status();
        let rate_limit = WbRateLimitHeaders::from_headers(response.headers());
        self.log_to_file(&format!(
            "WB Advert campaigns X-Ratelimit headers: {}",
            rate_limit.to_log_fields()
        ));
        if !status.is_success() {
            let body = self.read_body_tracked(response).await.unwrap_or_default();
            tracing::warn!(
                "WB Advert campaigns failed: {} - {}{}",
                status,
                body,
                rate_limit.to_error_suffix()
            );
            anyhow::bail!(
                "WB Advert campaigns failed with status {}: {}{}",
                status,
                body,
                rate_limit.to_error_suffix()
            );
        }

        let body = self.read_body_tracked(response).await?;
        let parsed: WbAdvertCampaignsResponse = serde_json::from_str(&body).map_err(|e| {
            let snippet: String = body.chars().take(400).collect();
            anyhow::anyhow!(
                "Failed to parse WB advert campaigns: {} | body: {}",
                e,
                snippet
            )
        })?;

        Ok(parsed.adverts)
    }

    pub async fn fetch_advert_campaign_info_values(
        &self,
        connection: &ConnectionMP,
        ids: &[i64],
    ) -> Result<Vec<serde_json::Value>> {
        if ids.is_empty() {
            return Ok(vec![]);
        }

        let ids_str = ids
            .iter()
            .map(|id| id.to_string())
            .collect::<Vec<_>>()
            .join(",");
        let url = format!(
            "https://advert-api.wildberries.ru/api/advert/v2/adverts?ids={}",
            ids_str
        );

        const MAX_ATTEMPTS: u32 = 3;
        let mut successful_body = None;
        for attempt in 1..=MAX_ATTEMPTS {
            let response = self
                .client
                .get(&url)
                .header("Authorization", &connection.api_key)
                .send()
                .await
                .map_err(|e| anyhow::anyhow!("Connection error for advert campaigns: {}", e))?;

            let status = response.status();
            let rate_limit = WbRateLimitHeaders::from_headers(response.headers());
            self.log_to_file(&format!(
                "WB Advert campaigns info X-Ratelimit headers: {}",
                rate_limit.to_log_fields()
            ));
            let response_body = self.read_body_tracked(response).await.unwrap_or_default();
            if status.is_success() {
                successful_body = Some(response_body);
                break;
            }
            if status == reqwest::StatusCode::TOO_MANY_REQUESTS && attempt < MAX_ATTEMPTS {
                let wait = rate_limit.retry_seconds.unwrap_or(1).max(1);
                tokio::time::sleep(std::time::Duration::from_secs(wait)).await;
                continue;
            }
            anyhow::bail!(
                "WB Advert campaigns failed with status {}: {}{}",
                status,
                response_body,
                rate_limit.to_error_suffix()
            );
        }
        let body = successful_body.expect("successful advert response body");
        let parsed: serde_json::Value = serde_json::from_str(&body).map_err(|e| {
            let snippet: String = body.chars().take(400).collect();
            anyhow::anyhow!(
                "Failed to parse WB advert campaigns info: {} | body: {}",
                e,
                snippet
            )
        })?;

        if let Some(adverts) = parsed.get("adverts").and_then(|v| v.as_array()) {
            Ok(adverts.clone())
        } else if let Some(items) = parsed.as_array() {
            Ok(items.clone())
        } else {
            Ok(vec![parsed])
        }
    }

    /// GET /adv/v3/fullstats вЂ” статистика рекламных кампаний (макс 50 ID за запрос)
    pub async fn fetch_advert_fullstats(
        &self,
        connection: &ConnectionMP,
        ids: &[i64],
        begin_date: &str,
        end_date: &str,
    ) -> Result<Vec<WbAdvertFullStat>> {
        if ids.is_empty() {
            return Ok(vec![]);
        }

        let ids_str = ids
            .iter()
            .map(|id| id.to_string())
            .collect::<Vec<_>>()
            .join(",");

        let url = format!(
            "https://advert-api.wildberries.ru/adv/v3/fullstats?ids={}&beginDate={}&endDate={}",
            ids_str, begin_date, end_date
        );

        self.log_to_file(&format!(
            "=== REQUEST ===\nGET {}\nAuthorization: ****",
            url
        ));

        let response = match self
            .client
            .get(&url)
            .header("Authorization", &connection.api_key)
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => {
                tracing::error!("WB Advert fullstats connection error: {}", e);
                anyhow::bail!("Connection error for advert fullstats: {}", e);
            }
        };

        let status = response.status();
        let rate_limit = WbRateLimitHeaders::from_headers(response.headers());
        self.log_to_file(&format!("Response status: {}", status));
        self.log_to_file(&format!(
            "Response X-Ratelimit headers: {}",
            rate_limit.to_log_fields()
        ));

        if !status.is_success() {
            let body = self.read_body_tracked(response).await.unwrap_or_default();
            self.log_to_file(&format!("ERROR Response body:\n{}", body));
            tracing::warn!(
                "WB Advert fullstats failed: {} - {}{}",
                status,
                body,
                rate_limit.to_error_suffix()
            );
            let body_preview: String = body.chars().take(120).collect();
            let body_preview = body_preview.trim();
            anyhow::bail!(
                "WB Advert API fullstats: {} — {}{}",
                status,
                if body_preview.is_empty() {
                    "(пустой ответ)"
                } else {
                    body_preview
                },
                rate_limit.to_error_suffix()
            );
        }

        let body = self.read_body_tracked(response).await?;
        let body_preview: String = body.chars().take(2000).collect();
        self.log_to_file(&format!("=== FULLSTATS RESPONSE ===\n{}\n", body_preview));

        if body.trim() == "null" {
            tracing::info!(
                "WB Advert fullstats returned null for ids=[{}]; treating as empty stats",
                ids_str
            );
            return Ok(Vec::new());
        }

        let parsed: Vec<WbAdvertFullStat> = match serde_json::from_str(&body) {
            Ok(p) => p,
            Err(e) => {
                let snippet: String = body.chars().take(400).collect();
                tracing::error!("WB Advert fullstats parse error: {} | body: {}", e, snippet);
                anyhow::bail!(
                    "Failed to parse WB advert fullstats: {} | body: {}",
                    e,
                    snippet
                );
            }
        };

        tracing::info!(
            "WB Advert fullstats: {} campaigns for ids=[{}]",
            parsed.len(),
            ids_str
        );

        Ok(parsed)
    }

    /// POST /api/analytics/v3/sales-funnel/products — статистика карточек за период.
    /// Используется как discovery: возвращает nmID товаров с активностью и признак
    /// наличия следующей страницы (пагинация через limit/offset). Лимит: 3 запроса/мин.
    pub async fn fetch_sales_funnel_products(
        &self,
        connection: &ConnectionMP,
        date_from: &str,
        date_to: &str,
        offset: usize,
        limit: usize,
    ) -> Result<(Vec<i64>, bool)> {
        let url =
            "https://seller-analytics-api.wildberries.ru/api/analytics/v3/sales-funnel/products";

        let request_body = serde_json::json!({
            "selectedPeriod": { "start": date_from, "end": date_to },
            "orderBy": { "field": "openCard", "mode": "desc" },
            "skipDeletedNm": false,
            "limit": limit,
            "offset": offset
        });
        let body = serde_json::to_string(&request_body)?;

        self.log_to_file(&format!(
            "=== REQUEST ===\nPOST {}\nAuthorization: ****\n{}",
            url, body
        ));

        let response = match self
            .client
            .post(url)
            .header("Authorization", &connection.api_key)
            .header("Content-Type", "application/json")
            // Аналитика WB (сравнение периодов по всем товарам) считается дольше
            // обычных запросов — переопределяем глобальный 60с-таймаут клиента.
            .timeout(std::time::Duration::from_secs(180))
            .body(body)
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => {
                self.log_to_file(&format!("CONNECTION ERROR (products): {e:?}"));
                tracing::error!("WB sales-funnel products connection error: {}", e);
                anyhow::bail!("Connection error for sales-funnel products: {}", e);
            }
        };

        let status = response.status();
        let rate_limit = WbRateLimitHeaders::from_headers(response.headers());
        self.log_to_file(&format!("Response status: {}", status));
        self.log_to_file(&format!(
            "Response X-Ratelimit headers: {}",
            rate_limit.to_log_fields()
        ));

        if !status.is_success() {
            let resp_body = self.read_body_tracked(response).await.unwrap_or_default();
            self.log_to_file(&format!("ERROR Response body:\n{}", resp_body));
            let body_preview: String = resp_body.chars().take(200).collect();
            let body_preview = body_preview.trim();
            anyhow::bail!(
                "WB sales-funnel products: {} — {}{}",
                status,
                if body_preview.is_empty() {
                    "(пустой ответ)"
                } else {
                    body_preview
                },
                rate_limit.to_error_suffix()
            );
        }

        let resp_body = self.read_body_tracked(response).await?;
        let body_preview: String = resp_body.chars().take(2000).collect();
        self.log_to_file(&format!(
            "=== SALES FUNNEL PRODUCTS RESPONSE (offset {}) ===\n{}\n",
            offset, body_preview
        ));

        if resp_body.trim() == "null" || resp_body.trim().is_empty() {
            return Ok((Vec::new(), false));
        }

        let parsed: serde_json::Value = serde_json::from_str(&resp_body).map_err(|e| {
            let snippet: String = resp_body.chars().take(400).collect();
            anyhow::anyhow!(
                "Failed to parse WB sales-funnel products: {} | body: {}",
                e,
                snippet
            )
        })?;

        // Ответ обёрнут в {"data": {"products": [...], "currency": ...}}.
        // Каждый элемент products — {"product": {nmId, ...}, "statistic": {...}};
        // извлекаем nmID толерантно (product.nmId либо nmId в корне элемента).
        let data = parsed.get("data").unwrap_or(&parsed);
        let items = data
            .get("products")
            .or_else(|| data.get("cards"))
            .or_else(|| data.get("items"))
            .or_else(|| Some(data))
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();

        let mut nm_ids = Vec::with_capacity(items.len());
        for item in &items {
            let nm_id = item
                .get("product")
                .and_then(|p| p.get("nmId").or_else(|| p.get("nmID")))
                .or_else(|| item.get("nmId"))
                .or_else(|| item.get("nmID"))
                .and_then(|v| v.as_i64());
            if let Some(id) = nm_id {
                nm_ids.push(id);
            }
        }

        // Пагинация по offset: следующая страница есть, если вернули полную страницу.
        let is_next_page = items.len() >= limit;

        tracing::info!(
            "WB sales-funnel products: offset={}, nm_ids={}, is_next_page={}",
            offset,
            nm_ids.len(),
            is_next_page
        );

        Ok((nm_ids, is_next_page))
    }

    /// POST /api/analytics/v3/sales-funnel/products — тот же эндпоинт, что и discovery,
    /// но извлекает полные карточки (`products[].product`) с остатками и рейтингами
    /// для ежедневного снимка a037. Пагинация limit/offset, лимит 3 запроса/мин.
    pub async fn fetch_sales_funnel_products_full(
        &self,
        connection: &ConnectionMP,
        date_from: &str,
        date_to: &str,
        offset: usize,
        limit: usize,
    ) -> Result<(Vec<WbProductSnapshotRow>, bool)> {
        let url =
            "https://seller-analytics-api.wildberries.ru/api/analytics/v3/sales-funnel/products";

        let request_body = serde_json::json!({
            "selectedPeriod": { "start": date_from, "end": date_to },
            "orderBy": { "field": "openCard", "mode": "desc" },
            "skipDeletedNm": false,
            "limit": limit,
            "offset": offset
        });
        let body = serde_json::to_string(&request_body)?;

        self.log_to_file(&format!(
            "=== REQUEST ===\nPOST {}\nAuthorization: ****\n{}",
            url, body
        ));

        let response = match self
            .client
            .post(url)
            .header("Authorization", &connection.api_key)
            .header("Content-Type", "application/json")
            .timeout(std::time::Duration::from_secs(180))
            .body(body)
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => {
                self.log_to_file(&format!("CONNECTION ERROR (products_full): {e:?}"));
                tracing::error!("WB sales-funnel products_full connection error: {}", e);
                anyhow::bail!(
                    "Connection error for sales-funnel products (snapshot): {}",
                    e
                );
            }
        };

        let status = response.status();
        let rate_limit = WbRateLimitHeaders::from_headers(response.headers());
        self.log_to_file(&format!("Response status: {}", status));
        self.log_to_file(&format!(
            "Response X-Ratelimit headers: {}",
            rate_limit.to_log_fields()
        ));

        if !status.is_success() {
            let resp_body = self.read_body_tracked(response).await.unwrap_or_default();
            self.log_to_file(&format!("ERROR Response body:\n{}", resp_body));
            let body_preview: String = resp_body.chars().take(200).collect();
            let body_preview = body_preview.trim();
            anyhow::bail!(
                "WB sales-funnel products (snapshot): {} — {}{}",
                status,
                if body_preview.is_empty() {
                    "(пустой ответ)"
                } else {
                    body_preview
                },
                rate_limit.to_error_suffix()
            );
        }

        let resp_body = self.read_body_tracked(response).await?;
        let body_preview: String = resp_body.chars().take(2000).collect();
        self.log_to_file(&format!(
            "=== SALES FUNNEL PRODUCTS FULL RESPONSE (offset {}) ===\n{}\n",
            offset, body_preview
        ));

        if resp_body.trim() == "null" || resp_body.trim().is_empty() {
            return Ok((Vec::new(), false));
        }

        let parsed: serde_json::Value = serde_json::from_str(&resp_body).map_err(|e| {
            let snippet: String = resp_body.chars().take(400).collect();
            anyhow::anyhow!(
                "Failed to parse WB sales-funnel products (snapshot): {} | body: {}",
                e,
                snippet
            )
        })?;

        // {"data": {"products": [{"product": {nmId, stocks{wb,mp,balanceSum}, productRating, ...}}]}}
        let data = parsed.get("data").unwrap_or(&parsed);
        let items = data
            .get("products")
            .or_else(|| data.get("cards"))
            .or_else(|| data.get("items"))
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();

        let mut rows = Vec::with_capacity(items.len());
        for item in &items {
            let product = item.get("product").unwrap_or(item);
            let nm_id = product
                .get("nmId")
                .or_else(|| product.get("nmID"))
                .and_then(|v| v.as_i64());
            let Some(nm_id) = nm_id else { continue };
            let stocks = product.get("stocks");
            rows.push(WbProductSnapshotRow {
                nm_id,
                title: product
                    .get("title")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string(),
                vendor_code: product
                    .get("vendorCode")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string(),
                brand_name: product
                    .get("brandName")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string(),
                subject_id: product
                    .get("subjectId")
                    .or_else(|| product.get("subjectID"))
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0),
                subject_name: product
                    .get("subjectName")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string(),
                stock_wb: stocks
                    .and_then(|s| s.get("wb"))
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0),
                stock_mp: stocks
                    .and_then(|s| s.get("mp"))
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0),
                stock_balance_sum: stocks
                    .and_then(|s| s.get("balanceSum"))
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0),
                product_rating: product
                    .get("productRating")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0),
                feedback_rating: product
                    .get("feedbackRating")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0),
            });
        }

        let is_next_page = items.len() >= limit;
        tracing::info!(
            "WB sales-funnel products_full: offset={}, rows={}, is_next_page={}",
            offset,
            rows.len(),
            is_next_page
        );

        Ok((rows, is_next_page))
    }

    /// POST /api/analytics/v3/sales-funnel/products/history — воронка продаж
    /// по карточкам товаров по дням. Лимит: 3 запроса/мин, данные ~за последнюю неделю.
    pub async fn fetch_sales_funnel_history(
        &self,
        connection: &ConnectionMP,
        nm_ids: &[i64],
        date_from: &str,
        date_to: &str,
    ) -> Result<Vec<WbSalesFunnelHistoryItem>> {
        if nm_ids.is_empty() {
            return Ok(vec![]);
        }

        let url = "https://seller-analytics-api.wildberries.ru/api/analytics/v3/sales-funnel/products/history";

        let request_body = serde_json::json!({
            "selectedPeriod": { "start": date_from, "end": date_to },
            "nmIds": nm_ids,
            "skipDeletedNm": false,
            "aggregationLevel": "day"
        });
        let body = serde_json::to_string(&request_body)?;

        self.log_to_file(&format!(
            "=== REQUEST ===\nPOST {}\nAuthorization: ****\n{}",
            url, body
        ));

        let response = match self
            .client
            .post(url)
            .header("Authorization", &connection.api_key)
            .header("Content-Type", "application/json")
            // Аналитика WB считается дольше обычных запросов —
            // переопределяем глобальный 60с-таймаут клиента.
            .timeout(std::time::Duration::from_secs(180))
            .body(body)
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => {
                self.log_to_file(&format!("CONNECTION ERROR (history): {e:?}"));
                tracing::error!("WB sales-funnel history connection error: {}", e);
                anyhow::bail!("Connection error for sales-funnel history: {}", e);
            }
        };

        let status = response.status();
        let rate_limit = WbRateLimitHeaders::from_headers(response.headers());
        self.log_to_file(&format!("Response status: {}", status));
        self.log_to_file(&format!(
            "Response X-Ratelimit headers: {}",
            rate_limit.to_log_fields()
        ));

        if !status.is_success() {
            let resp_body = self.read_body_tracked(response).await.unwrap_or_default();
            self.log_to_file(&format!("ERROR Response body:\n{}", resp_body));
            tracing::warn!(
                "WB sales-funnel history failed: {} - {}{}",
                status,
                resp_body,
                rate_limit.to_error_suffix()
            );
            let body_preview: String = resp_body.chars().take(200).collect();
            let body_preview = body_preview.trim();
            anyhow::bail!(
                "WB sales-funnel history: {} — {}{}",
                status,
                if body_preview.is_empty() {
                    "(пустой ответ)"
                } else {
                    body_preview
                },
                rate_limit.to_error_suffix()
            );
        }

        let resp_body = self.read_body_tracked(response).await?;
        let body_preview: String = resp_body.chars().take(2000).collect();
        self.log_to_file(&format!(
            "=== SALES FUNNEL HISTORY RESPONSE ===\n{}\n",
            body_preview
        ));

        if resp_body.trim() == "null" || resp_body.trim().is_empty() {
            return Ok(Vec::new());
        }

        let parsed: serde_json::Value = serde_json::from_str(&resp_body).map_err(|e| {
            let snippet: String = resp_body.chars().take(400).collect();
            anyhow::anyhow!(
                "Failed to parse WB sales-funnel history: {} | body: {}",
                e,
                snippet
            )
        })?;

        // Ответ — массив items либо обёртка {"data": [...]}.
        let items_value = if parsed.is_array() {
            parsed
        } else {
            parsed
                .get("data")
                .cloned()
                .unwrap_or(serde_json::Value::Null)
        };

        if items_value.is_null() {
            return Ok(Vec::new());
        }

        let items: Vec<WbSalesFunnelHistoryItem> =
            serde_json::from_value(items_value).map_err(|e| {
                let snippet: String = resp_body.chars().take(400).collect();
                anyhow::anyhow!(
                    "Failed to decode WB sales-funnel history items: {} | body: {}",
                    e,
                    snippet
                )
            })?;

        tracing::info!(
            "WB sales-funnel history: {} products for {} nm_ids",
            items.len(),
            nm_ids.len()
        );

        Ok(items)
    }

    /// Создаёт асинхронный CSV-отчёт WB `DETAIL_HISTORY_REPORT`.
    pub async fn create_sales_funnel_detail_report(
        &self,
        connection: &ConnectionMP,
        download_id: Uuid,
        date_from: &str,
        date_to: &str,
    ) -> Result<()> {
        let url = "https://seller-analytics-api.wildberries.ru/api/v2/nm-report/downloads";
        let request_body = serde_json::json!({
            "id": download_id.to_string(),
            "reportType": "DETAIL_HISTORY_REPORT",
            "userReportName": format!("a036-{}-{}", date_from, date_to),
            "params": {
                "nmIDs": [],
                "subjectIds": [],
                "brandNames": [],
                "tagIds": [],
                "startDate": date_from,
                "endDate": date_to,
                "timezone": "Europe/Moscow",
                "aggregationLevel": "day",
                "skipDeletedNm": false
            }
        });
        let body = serde_json::to_string(&request_body)?;
        self.record_http_request_attempt(body.len() as u64);

        let response = self
            .client
            .post(url)
            .header("Authorization", &connection.api_key)
            .header("Content-Type", "application/json")
            .timeout(std::time::Duration::from_secs(180))
            .body(body)
            .send()
            .await
            .context("WB DETAIL_HISTORY_REPORT create request failed")?;
        let status = response.status();
        let rate_limit = WbRateLimitHeaders::from_headers(response.headers());
        let response_body = self.read_body_for_recorded_request(response).await?;
        if !status.is_success() {
            anyhow::bail!(
                "WB DETAIL_HISTORY_REPORT create: {} — {}{}",
                status,
                response_body.chars().take(500).collect::<String>(),
                rate_limit.to_error_suffix()
            );
        }

        tracing::info!(
            "WB DETAIL_HISTORY_REPORT created: download_id={}, period={}..{}",
            download_id,
            date_from,
            date_to
        );
        Ok(())
    }

    /// Возвращает текущий статус ранее созданного CSV-отчёта.
    pub async fn get_sales_funnel_detail_report_status(
        &self,
        connection: &ConnectionMP,
        download_id: Uuid,
    ) -> Result<WbAnalyticsReportStatus> {
        let url = "https://seller-analytics-api.wildberries.ru/api/v2/nm-report/downloads";
        self.record_http_request_attempt(0);
        let response = self
            .client
            .get(url)
            .header("Authorization", &connection.api_key)
            .query(&[("filter[downloadIds]", download_id.to_string())])
            .timeout(std::time::Duration::from_secs(180))
            .send()
            .await
            .context("WB DETAIL_HISTORY_REPORT status request failed")?;
        let status = response.status();
        let rate_limit = WbRateLimitHeaders::from_headers(response.headers());
        let response_body = self.read_body_for_recorded_request(response).await?;
        if !status.is_success() {
            anyhow::bail!(
                "WB DETAIL_HISTORY_REPORT status: {} — {}{}",
                status,
                response_body.chars().take(500).collect::<String>(),
                rate_limit.to_error_suffix()
            );
        }

        let envelope: WbAnalyticsReportListResponse = serde_json::from_str(&response_body)
            .with_context(|| {
                format!(
                    "Invalid WB DETAIL_HISTORY_REPORT status response: {}",
                    response_body.chars().take(500).collect::<String>()
                )
            })?;
        let report = envelope
            .data
            .into_iter()
            .find(|item| item.id == download_id.to_string())
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "WB DETAIL_HISTORY_REPORT {} is absent from status response",
                    download_id
                )
            })?;
        Ok(report)
    }

    /// Скачивает ZIP готового CSV-отчёта.
    pub async fn download_sales_funnel_detail_report(
        &self,
        connection: &ConnectionMP,
        download_id: Uuid,
    ) -> Result<Vec<u8>> {
        let url = format!(
            "https://seller-analytics-api.wildberries.ru/api/v2/nm-report/downloads/file/{}",
            download_id
        );
        self.record_http_request_attempt(0);
        let response = self
            .client
            .get(&url)
            .header("Authorization", &connection.api_key)
            .header(ACCEPT, "application/zip")
            .timeout(std::time::Duration::from_secs(300))
            .send()
            .await
            .context("WB DETAIL_HISTORY_REPORT download request failed")?;
        let status = response.status();
        let rate_limit = WbRateLimitHeaders::from_headers(response.headers());
        if !status.is_success() {
            let response_body = self.read_body_for_recorded_request(response).await?;
            anyhow::bail!(
                "WB DETAIL_HISTORY_REPORT download: {} — {}{}",
                status,
                response_body.chars().take(500).collect::<String>(),
                rate_limit.to_error_suffix()
            );
        }
        let bytes = response
            .bytes()
            .await
            .context("Failed to read WB DETAIL_HISTORY_REPORT ZIP")?;
        self.record_http_response_body(bytes.len() as u64);
        if bytes.is_empty() {
            anyhow::bail!("WB DETAIL_HISTORY_REPORT returned an empty ZIP");
        }
        Ok(bytes.to_vec())
    }

    /// POST /api/v2/search-report/table/details — поисковая аналитика по товарам за период
    /// (видимость % в выдаче, переходы, позиция; счётчик показов WB не отдаёт → impressions=0).
    /// Требует подписки «Джем».
    ///
    /// ВАЖНО: точная форма запроса/ответа WB не верифицирована офлайн — парсинг сделан
    /// толерантно (несколько кандидатов-ключей, метрики берутся из `{current}`), сырой
    /// ответ логируется (`=== SEARCH REPORT RESPONSE ===`). При первом живом прогоне
    /// сверить имена полей и при необходимости поправить `extract_*` ниже.
    pub async fn fetch_search_report(
        &self,
        connection: &ConnectionMP,
        date_from: &str,
        date_to: &str,
    ) -> Result<Vec<WbSearchReportRow>> {
        // The main `/report` method only returns summary widgets and groups.
        // Product rows are provided by `/table/details`, including when no group filter is set.
        let url = "https://seller-analytics-api.wildberries.ru/api/v2/search-report/table/details";
        // pastPeriod обязателен: окно той же длины, оканчивающееся за день до currentPeriod.
        let (past_start, past_end) = past_period(date_from, date_to);
        let request_body = serde_json::json!({
            "currentPeriod": { "start": date_from, "end": date_to },
            "pastPeriod": { "start": past_start, "end": past_end },
            "nmIds": [],
            "orderBy": { "field": "openCard", "mode": "desc" },
            "positionCluster": "all",
            "includeSubstitutedSKUs": true,
            "includeSearchTexts": true,
            "limit": 1000,
            "offset": 0
        });
        let body = serde_json::to_string(&request_body)?;
        self.log_to_file(&format!("=== REQUEST ===\nPOST {}\n{}", url, body));

        let response = match self
            .client
            .post(url)
            .header("Authorization", &connection.api_key)
            .header("Content-Type", "application/json")
            .timeout(std::time::Duration::from_secs(180))
            .body(body)
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => {
                self.log_to_file(&format!("CONNECTION ERROR (search-report): {e:?}"));
                anyhow::bail!("Connection error for search-report: {}", e);
            }
        };

        let status = response.status();
        self.log_to_file(&format!("Response status: {}", status));
        if status.as_u16() == 403 {
            // Нет подписки «Джем» / нет доступа — мягкая деградация.
            anyhow::bail!("WB search-report: 403 (нет доступа/подписки «Джем»)");
        }
        if !status.is_success() {
            let resp_body = self.read_body_tracked(response).await.unwrap_or_default();
            let preview: String = resp_body.chars().take(200).collect();
            anyhow::bail!("WB search-report: {} — {}", status, preview.trim());
        }

        let resp_body = self.read_body_tracked(response).await?;
        let preview: String = resp_body.chars().take(2000).collect();
        self.log_to_file(&format!("=== SEARCH REPORT RESPONSE ===\n{}\n", preview));
        if resp_body.trim() == "null" || resp_body.trim().is_empty() {
            return Ok(Vec::new());
        }

        let parsed: serde_json::Value = serde_json::from_str(&resp_body)
            .map_err(|e| anyhow::anyhow!("Failed to parse WB search-report: {}", e))?;
        let data = parsed.get("data").unwrap_or(&parsed);
        let items = data
            .get("products")
            .or_else(|| data.get("items"))
            .or_else(|| data.get("cards"))
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();

        let rows = items
            .iter()
            .filter_map(parse_search_report_row)
            .collect::<Vec<_>>();
        tracing::info!("WB search-report: {} product rows", rows.len());
        Ok(rows)
    }

    /// POST /api/v2/search-report/product/search-texts — топ поисковых запросов по товарам.
    /// Та же оговорка про верификацию полей, что и у `fetch_search_report`.
    pub async fn fetch_search_texts(
        &self,
        connection: &ConnectionMP,
        nm_ids: &[i64],
        date_from: &str,
        date_to: &str,
        top_limit: i64,
    ) -> Result<Vec<WbSearchQueryRow>> {
        if nm_ids.is_empty() {
            return Ok(Vec::new());
        }
        let url =
            "https://seller-analytics-api.wildberries.ru/api/v2/search-report/product/search-texts";
        let (past_start, past_end) = past_period(date_from, date_to);
        let request_body = serde_json::json!({
            "currentPeriod": { "start": date_from, "end": date_to },
            "pastPeriod": { "start": past_start, "end": past_end },
            "nmIds": nm_ids,
            "topOrderBy": "openCard",
            "includeSubstitutedSKUs": true,
            "includeSearchTexts": true,
            "orderBy": { "field": "avgPosition", "mode": "asc" },
            "limit": top_limit
        });
        let body = serde_json::to_string(&request_body)?;
        self.log_to_file(&format!("=== REQUEST ===\nPOST {}\n{}", url, body));

        let response = match self
            .client
            .post(url)
            .header("Authorization", &connection.api_key)
            .header("Content-Type", "application/json")
            .timeout(std::time::Duration::from_secs(180))
            .body(body)
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => {
                self.log_to_file(&format!("CONNECTION ERROR (search-texts): {e:?}"));
                anyhow::bail!("Connection error for search-texts: {}", e);
            }
        };

        let status = response.status();
        self.log_to_file(&format!("Response status: {}", status));
        if status.as_u16() == 403 {
            anyhow::bail!("WB search-texts: 403 (нет доступа/подписки «Джем»)");
        }
        if !status.is_success() {
            let resp_body = self.read_body_tracked(response).await.unwrap_or_default();
            let preview: String = resp_body.chars().take(200).collect();
            anyhow::bail!("WB search-texts: {} — {}", status, preview.trim());
        }

        let resp_body = self.read_body_tracked(response).await?;
        let preview: String = resp_body.chars().take(2000).collect();
        self.log_to_file(&format!("=== SEARCH TEXTS RESPONSE ===\n{}\n", preview));
        if resp_body.trim() == "null" || resp_body.trim().is_empty() {
            return Ok(Vec::new());
        }

        let parsed: serde_json::Value = serde_json::from_str(&resp_body)
            .map_err(|e| anyhow::anyhow!("Failed to parse WB search-texts: {}", e))?;
        let data = parsed.get("data").unwrap_or(&parsed);
        // Ответ может быть массивом по товарам (каждый с массивом запросов) либо плоским.
        let mut rows: Vec<WbSearchQueryRow> = Vec::new();
        if let Some(products) = data
            .get("products")
            .or_else(|| data.get("items"))
            .and_then(|v| v.as_array())
        {
            for product in products {
                let nm_id = json_i64(product, &["nmId", "nmID", "nm_id"]).unwrap_or(0);
                // Current WB response is a flat `data.items` array: one item per search text.
                if product.get("text").is_some() || product.get("searchText").is_some() {
                    rows.push(parse_search_query_row(nm_id, product));
                    continue;
                }
                if let Some(texts) = product
                    .get("searchTexts")
                    .or_else(|| product.get("texts"))
                    .or_else(|| product.get("queries"))
                    .and_then(|v| v.as_array())
                {
                    for t in texts {
                        rows.push(parse_search_query_row(nm_id, t));
                    }
                }
            }
        } else if let Some(arr) = data.as_array() {
            for t in arr {
                let nm_id = json_i64(t, &["nmId", "nmID", "nm_id"]).unwrap_or(0);
                rows.push(parse_search_query_row(nm_id, t));
            }
        }
        tracing::info!(
            "WB search-texts: {} query rows for {} nm_ids",
            rows.len(),
            nm_ids.len()
        );
        Ok(rows)
    }
}

impl Default for WildberriesApiClient {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Request/Response structures для Wildberries API
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WildberriesProductListRequest {
    pub settings: WildberriesSettings,
    pub limit: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WildberriesSettings {
    pub cursor: WildberriesCursor,
    pub filter: WildberriesFilter,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WildberriesCursor {
    #[serde(rename = "updatedAt", skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
    #[serde(rename = "nmID", skip_serializing_if = "Option::is_none")]
    pub nm_id: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<i32>,
    #[serde(default, skip_serializing)]
    pub total: i64,
}

impl Default for WildberriesCursor {
    fn default() -> Self {
        Self {
            updated_at: None,
            nm_id: None,
            limit: None,
            total: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WildberriesFilter {
    #[serde(rename = "findByNmID", skip_serializing_if = "Option::is_none")]
    pub find_by_nm_id: Option<Vec<i64>>,
    #[serde(rename = "withPhoto", skip_serializing_if = "Option::is_none")]
    pub with_photo: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WildberriesProductListResponse {
    pub cards: Vec<WildberriesCard>,
    pub cursor: WildberriesCursor,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WildberriesCard {
    #[serde(rename = "nmID")]
    pub nm_id: i64,
    #[serde(rename = "imtID")]
    pub imt_id: i64,
    #[serde(rename = "subjectID")]
    pub subject_id: i64,
    #[serde(rename = "vendorCode")]
    pub vendor_code: String,
    #[serde(default)]
    pub brand: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub photos: Vec<WildberriesPhoto>,
    #[serde(default)]
    pub video: Option<String>,
    #[serde(default)]
    pub dimensions: Option<WildberriesDimensions>,
    #[serde(default)]
    pub characteristics: Vec<WildberriesCharacteristic>,
    #[serde(default)]
    pub sizes: Vec<WildberriesSize>,
    #[serde(default)]
    pub tags: Vec<WildberriesTag>,
    #[serde(rename = "createdAt", default)]
    pub created_at: String,
    #[serde(rename = "updatedAt", default)]
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WildberriesPhoto {
    #[serde(default)]
    pub big: Option<String>,
    #[serde(default)]
    pub small: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WildberriesDimensions {
    #[serde(default)]
    pub length: Option<i32>,
    #[serde(default)]
    pub width: Option<i32>,
    #[serde(default)]
    pub height: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WildberriesCharacteristic {
    #[serde(
        rename = "Наименование характеристики",
        default
    )]
    pub name: Option<String>,
    #[serde(rename = "Значение характеристики", default)]
    pub value: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WildberriesSize {
    #[serde(rename = "techSize", default)]
    pub tech_size: Option<String>,
    #[serde(rename = "wbSize", default)]
    pub wb_size: Option<String>,
    #[serde(default)]
    pub price: Option<i32>,
    #[serde(rename = "discountedPrice", default)]
    pub discounted_price: Option<i32>,
    #[serde(default)]
    pub barcode: Option<String>,
    #[serde(default)]
    pub skus: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WildberriesTag {
    #[serde(default)]
    pub id: Option<i32>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub color: Option<String>,
}

// ============================================================================
// Sales structures
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WbSaleRow {
    /// Уникальный идентификатор строки продажи
    #[serde(default)]
    pub srid: Option<String>,
    /// Номенклатурный номер товара
    #[serde(rename = "nmId", default)]
    pub nm_id: Option<i64>,
    /// Артикул продавца
    #[serde(rename = "supplierArticle", default)]
    pub supplier_article: Option<String>,
    /// Штрихкод
    #[serde(default)]
    pub barcode: Option<String>,
    /// Название товара
    #[serde(default)]
    pub brand: Option<String>,
    /// Предмет
    #[serde(default)]
    pub subject: Option<String>,
    /// Категория
    #[serde(default)]
    pub category: Option<String>,
    /// Дата продажи
    #[serde(rename = "date", default)]
    pub sale_dt: Option<String>,
    /// Дата последнего изменения записи
    #[serde(rename = "lastChangeDate", default)]
    pub last_change_date: Option<String>,
    /// РЎРєР»Р°Рґ
    #[serde(rename = "warehouseName", default)]
    pub warehouse_name: Option<String>,
    /// РЎС‚СЂР°РЅР°
    #[serde(rename = "countryName", default)]
    pub country_name: Option<String>,
    /// Р РµРіРёРѕРЅ
    #[serde(rename = "oblastOkrugName", default)]
    pub region_name: Option<String>,
    /// Цена без скидки
    #[serde(rename = "priceWithDisc", default)]
    pub price_with_disc: Option<f64>,
    /// РЎРєРёРґРєР° продавца
    #[serde(rename = "discount", default)]
    pub discount: Option<f64>,
    /// Количество
    #[serde(rename = "quantity", default)]
    pub quantity: Option<i32>,
    /// Тип документа: sale или return
    #[serde(rename = "saleID", default)]
    pub sale_id: Option<String>,
    /// Номер заказа
    #[serde(rename = "odid", default)]
    pub order_id: Option<i64>,
    /// SPP (РЎРѕРіР»Р°СЃРѕРІР°РЅРЅР°СЏ скидка продавца)
    #[serde(rename = "spp", default)]
    pub spp: Option<f64>,
    /// Вознаграждение
    #[serde(rename = "forPay", default)]
    pub for_pay: Option<f64>,
    /// РС‚РѕРіРѕРІР°СЏ стоимость
    #[serde(rename = "finishedPrice", default)]
    pub finished_price: Option<f64>,
    /// Флаг поставки
    #[serde(rename = "isSupply", default)]
    pub is_supply: Option<bool>,
    /// Флаг реализации
    #[serde(rename = "isRealization", default)]
    pub is_realization: Option<bool>,
    /// Полная цена
    #[serde(rename = "totalPrice", default)]
    pub total_price: Option<f64>,
    /// Процент скидки
    #[serde(rename = "discountPercent", default)]
    pub discount_percent: Option<f64>,
    /// РЎСѓРјРјР° платежа за продажу
    #[serde(rename = "paymentSaleAmount", default)]
    pub payment_sale_amount: Option<f64>,
    /// Тип склада
    #[serde(rename = "warehouseType", default)]
    pub warehouse_type: Option<String>,
}

// ============================================================================
// Finance Report structures
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WbFinanceReportRow {
    /// ID строки отчета
    #[serde(default)]
    pub rrd_id: Option<i64>,
    /// Дата строки финансового отчёта
    #[serde(default)]
    pub rr_dt: Option<String>,
    /// Номенклатурный номер товара
    #[serde(default)]
    pub nm_id: Option<i64>,
    /// Артикул продавца
    #[serde(default)]
    pub sa_name: Option<String>,
    /// Категория товара
    #[serde(default)]
    pub subject_name: Option<String>,
    /// Тип операции по заказу
    #[serde(default)]
    pub supplier_oper_name: Option<String>,
    /// Количество товаров
    #[serde(default)]
    pub quantity: Option<i32>,
    /// Р РѕР·РЅРёС‡РЅР°СЏ цена за единицу товара
    #[serde(default)]
    pub retail_price: Option<f64>,
    /// Общая сумма продажи
    #[serde(default)]
    pub retail_amount: Option<f64>,
    /// Цена продажи с учетом скидок
    #[serde(default)]
    pub retail_price_withdisc_rub: Option<f64>,
    /// Процент комиссии Wildberries
    #[serde(default)]
    pub commission_percent: Option<f64>,
    /// Комиссия за эквайринг
    #[serde(default)]
    pub acquiring_fee: Option<f64>,
    /// Процент комиссии за эквайринг
    #[serde(default)]
    pub acquiring_percent: Option<f64>,
    /// РЎСѓРјРјР°, уплаченная покупателем за доставку
    #[serde(default)]
    pub delivery_amount: Option<f64>,
    /// РЎС‚РѕРёРјРѕСЃС‚СЊ доставки на стороне продавца
    #[serde(default)]
    pub delivery_rub: Option<f64>,
    /// РЎСѓРјРјР° вознаграждения Вайлдберриз за текущий период (ВВ), без НДС
    #[serde(default)]
    pub ppvz_vw: Option<f64>,
    /// НДС с вознаграждения Вайлдберриз
    #[serde(default)]
    pub ppvz_vw_nds: Option<f64>,
    /// Комиссия WB за продажу
    #[serde(default)]
    pub ppvz_sales_commission: Option<f64>,
    /// РЎСѓРјРјР° возврата за возвращённые товары
    #[serde(default)]
    pub return_amount: Option<f64>,
    /// РЎСѓРјРјР° штрафа, удержанного с продавца
    #[serde(default)]
    pub penalty: Option<f64>,
    /// Дополнительные (корректирующие) выплаты продавцу
    #[serde(default)]
    pub additional_payment: Option<f64>,
    /// Плата за хранение товаров на складе
    #[serde(default)]
    pub storage_fee: Option<f64>,
    /// РЎРєРѕСЂСЂРµРєС‚РёСЂРѕРІР°РЅРЅС‹Рµ расходы на логистику
    #[serde(default)]
    pub rebill_logistic_cost: Option<f64>,
    /// Тип бонуса или штрафа
    #[serde(default)]
    pub bonus_type_name: Option<String>,
    /// Тип отчета (1 = daily, 2 = weekly)
    #[serde(default)]
    pub report_type: Option<i32>,

    // ============ Дополнительные поля из API (для полного JSON) ============
    /// ID реализационного отчета
    #[serde(default)]
    pub realizationreport_id: Option<i64>,
    /// Дата начала периода отчета
    #[serde(default)]
    pub date_from: Option<String>,
    /// Дата окончания периода отчета
    #[serde(default)]
    pub date_to: Option<String>,
    /// Дата создания отчета
    #[serde(default)]
    pub create_dt: Option<String>,
    /// Валюта
    #[serde(default)]
    pub currency_name: Option<String>,
    /// Код договора поставщика
    #[serde(default)]
    pub suppliercontract_code: Option<String>,
    /// ID сборочного задания
    #[serde(default)]
    pub gi_id: Option<i64>,
    /// Процент доставки
    #[serde(default)]
    pub dlv_prc: Option<f64>,
    /// Дата начала действия фикс. тарифа
    #[serde(default)]
    pub fix_tariff_date_from: Option<String>,
    /// Дата окончания действия фикс. тарифа
    #[serde(default)]
    pub fix_tariff_date_to: Option<String>,
    /// Бренд товара
    #[serde(default)]
    pub brand_name: Option<String>,
    /// Р Р°Р·РјРµСЂ товара
    #[serde(default)]
    pub ts_name: Option<String>,
    /// Штрихкод товара
    #[serde(default)]
    pub barcode: Option<String>,
    /// Тип документа
    #[serde(default)]
    pub doc_type_name: Option<String>,
    /// Процент скидки
    #[serde(default)]
    pub sale_percent: Option<f64>,
    /// Название склада
    #[serde(default)]
    pub office_name: Option<String>,
    /// Дата заказа
    #[serde(default)]
    pub order_dt: Option<String>,
    /// Дата продажи
    #[serde(default)]
    pub sale_dt: Option<String>,
    /// ID поставки
    #[serde(default)]
    pub shk_id: Option<i64>,
    /// Тип коробов
    #[serde(default)]
    pub gi_box_type_name: Option<String>,
    /// РЎРєРёРґРєР° на товар для отчета
    #[serde(default)]
    pub product_discount_for_report: Option<f64>,
    /// Промо поставщика
    #[serde(default)]
    pub supplier_promo: Option<f64>,
    /// РЎРѕРіР»Р°СЃРѕРІР°РЅРЅР°СЏ скидка продавца
    #[serde(default)]
    pub ppvz_spp_prc: Option<f64>,
    /// Базовый процент комиссии
    #[serde(default)]
    pub ppvz_kvw_prc_base: Option<f64>,
    /// Процент комиссии
    #[serde(default)]
    pub ppvz_kvw_prc: Option<f64>,
    /// Процент повышения рейтинга поставщика
    #[serde(default)]
    pub sup_rating_prc_up: Option<f64>,
    /// Участие в КГВП v2
    #[serde(default)]
    pub is_kgvp_v2: Option<i32>,
    /// К перечислению за товар
    #[serde(default)]
    pub ppvz_for_pay: Option<f64>,
    /// Вознаграждение
    #[serde(default)]
    pub ppvz_reward: Option<f64>,
    /// Тип процессинга платежа
    #[serde(default)]
    pub payment_processing: Option<String>,
    /// Банк-эквайер
    #[serde(default)]
    pub acquiring_bank: Option<String>,
    /// Название пункта выдачи
    #[serde(default)]
    pub ppvz_office_name: Option<String>,
    /// ID пункта выдачи
    #[serde(default)]
    pub ppvz_office_id: Option<i64>,
    /// ID поставщика
    #[serde(default)]
    pub ppvz_supplier_id: Option<i64>,
    /// Название поставщика
    #[serde(default)]
    pub ppvz_supplier_name: Option<String>,
    /// РРќРќ поставщика
    #[serde(default)]
    pub ppvz_inn: Option<String>,
    /// Номер декларации
    #[serde(default)]
    pub declaration_number: Option<String>,
    /// ID стикера
    #[serde(default)]
    pub sticker_id: Option<String>,
    /// РЎС‚СЂР°РЅР° продажи
    #[serde(default)]
    pub site_country: Option<String>,
    /// Доставка силами продавца
    #[serde(default)]
    pub srv_dbs: Option<bool>,
    /// Организация, предоставившая логистику
    #[serde(default)]
    pub rebill_logistic_org: Option<String>,
    /// Удержания
    #[serde(default)]
    pub deduction: Option<f64>,
    /// Приемка
    #[serde(default)]
    pub acceptance: Option<f64>,
    /// ID сборочного задания
    #[serde(default)]
    pub assembly_id: Option<i64>,
    /// Код маркировки
    #[serde(default)]
    pub kiz: Option<String>,
    /// Уникальный идентификатор строки
    #[serde(default)]
    pub srid: Option<String>,
    /// Юридическое лицо
    #[serde(default)]
    pub is_legal_entity: Option<bool>,
    /// ID возврата
    #[serde(default)]
    pub trbx_id: Option<String>,
    /// РЎСѓРјРјР° софинансирования рассрочки
    #[serde(default)]
    pub installment_cofinancing_amount: Option<f64>,
    /// Процент скидки WiBES
    #[serde(default)]
    pub wibes_wb_discount_percent: Option<f64>,
    /// РЎСѓРјРјР° кэшбэка
    #[serde(default)]
    pub cashback_amount: Option<f64>,
    /// РЎРєРёРґРєР° по кэшбэку
    #[serde(default)]
    pub cashback_discount: Option<f64>,
    /// РР·РјРµРЅРµРЅРёРµ комиссии по кэшбэку
    #[serde(default)]
    pub cashback_commission_change: Option<f64>,
    /// Уникальный ID заказа
    #[serde(default)]
    pub order_uid: Option<String>,
}

// ============================================================================
// Orders structures
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WbOrderRow {
    /// Дата заказа
    #[serde(default)]
    pub date: Option<String>,
    /// Дата последнего изменения
    #[serde(rename = "lastChangeDate", default)]
    pub last_change_date: Option<String>,
    /// Название склада
    #[serde(rename = "warehouseName", default)]
    pub warehouse_name: Option<String>,
    /// Тип склада
    #[serde(rename = "warehouseType", default)]
    pub warehouse_type: Option<String>,
    /// Название страны
    #[serde(rename = "countryName", default)]
    pub country_name: Option<String>,
    /// Название области/округа
    #[serde(rename = "oblastOkrugName", default)]
    pub oblast_okrug_name: Option<String>,
    /// Название региона
    #[serde(rename = "regionName", default)]
    pub region_name: Option<String>,
    /// Артикул продавца
    #[serde(rename = "supplierArticle", default)]
    pub supplier_article: Option<String>,
    /// nmId (ID номенклатуры WB)
    #[serde(rename = "nmId", default)]
    pub nm_id: Option<i64>,
    /// Баркод
    #[serde(default)]
    pub barcode: Option<String>,
    /// Категория
    #[serde(default)]
    pub category: Option<String>,
    /// Предмет
    #[serde(default)]
    pub subject: Option<String>,
    /// Бренд
    #[serde(default)]
    pub brand: Option<String>,
    /// Р Р°Р·РјРµСЂ
    #[serde(rename = "techSize", default)]
    pub tech_size: Option<String>,
    /// Номер поставки
    #[serde(rename = "incomeID", default)]
    pub income_id: Option<i64>,
    /// Флаг поставки
    #[serde(rename = "isSupply", default)]
    pub is_supply: Option<bool>,
    /// Флаг реализации
    #[serde(rename = "isRealization", default)]
    pub is_realization: Option<bool>,
    /// Цена без скидки
    #[serde(rename = "totalPrice", default)]
    pub total_price: Option<f64>,
    /// Процент скидки
    #[serde(rename = "discountPercent", default)]
    pub discount_percent: Option<f64>,
    /// SPP (РЎРѕРіР»Р°СЃРѕРІР°РЅРЅР°СЏ скидка продавца)
    #[serde(default)]
    pub spp: Option<f64>,
    /// РС‚РѕРіРѕРІР°СЏ цена для клиента
    #[serde(rename = "finishedPrice", default)]
    pub finished_price: Option<f64>,
    /// Цена с учетом скидки
    #[serde(rename = "priceWithDisc", default)]
    pub price_with_disc: Option<f64>,
    /// Флаг отмены заказа
    #[serde(rename = "isCancel", default)]
    pub is_cancel: Option<bool>,
    /// Дата отмены
    #[serde(rename = "cancelDate", default)]
    pub cancel_date: Option<String>,
    /// ID стикера
    #[serde(default)]
    pub sticker: Option<String>,
    /// G-номер
    #[serde(rename = "gNumber", default)]
    pub g_number: Option<String>,
    /// SRID - уникальный идентификатор заказа
    #[serde(default)]
    pub srid: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WbDocumentListItem {
    pub service_name: String,
    pub name: String,
    pub category: String,
    pub extensions: Vec<String>,
    pub creation_time: String,
    pub viewed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WbDocumentsListData {
    pub documents: Vec<WbDocumentListItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WbDocumentsListResponse {
    pub data: WbDocumentsListData,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WbDocumentDownloadFile {
    pub file_name: String,
    pub extension: String,
    pub document: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WbDocumentDownloadResponse {
    pub data: WbDocumentDownloadFile,
}

// ============================================================================
// Commission Tariffs structures
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommissionTariffRow {
    #[serde(rename = "kgvpBooking")]
    pub kgvp_booking: f64,
    #[serde(rename = "kgvpMarketplace")]
    pub kgvp_marketplace: f64,
    #[serde(rename = "kgvpPickup")]
    pub kgvp_pickup: f64,
    #[serde(rename = "kgvpSupplier")]
    pub kgvp_supplier: f64,
    #[serde(rename = "kgvpSupplierExpress")]
    pub kgvp_supplier_express: f64,
    #[serde(rename = "paidStorageKgvp")]
    pub paid_storage_kgvp: f64,
    #[serde(rename = "parentID")]
    pub parent_id: i32,
    #[serde(rename = "parentName")]
    pub parent_name: String,
    #[serde(rename = "subjectID")]
    pub subject_id: i32,
    #[serde(rename = "subjectName")]
    pub subject_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommissionTariffResponse {
    pub report: Vec<CommissionTariffRow>,
}

// ============================================================================
// Diagnostic structures
// ============================================================================

#[derive(Debug, Clone)]
pub struct DiagnosticResult {
    pub test_name: String,
    pub success: bool,
    pub error: Option<String>,
    pub total_returned: i32,
    pub cursor_total: i32,
    pub response_headers: Option<String>,
}

// ============================================================================
// WB Prices API structures (GET /api/v2/list/goods/filter)
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WbGoodsPriceFilterResponse {
    #[serde(default)]
    pub data: Option<WbGoodsPriceData>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WbGoodsPriceData {
    #[serde(rename = "listGoods", default)]
    pub list_goods: Vec<WbGoodsPriceRow>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WbGoodsPriceRow {
    #[serde(rename = "nmID")]
    pub nm_id: i64,
    #[serde(rename = "vendorCode", default)]
    pub vendor_code: Option<String>,
    #[serde(default)]
    pub discount: Option<i32>,
    #[serde(rename = "editableSizePrice", default)]
    pub editable_size_price: bool,
    #[serde(default)]
    pub sizes: Vec<WbGoodsSize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WbGoodsSize {
    #[serde(rename = "sizeID", default)]
    pub size_id: Option<i64>,
    #[serde(default)]
    pub price: Option<f64>,
    #[serde(rename = "discountedPrice", default)]
    pub discounted_price: Option<f64>,
    #[serde(rename = "techSizeName", default)]
    pub tech_size_name: Option<String>,
}

// ============================================================================
// WB Calendar Promotions API structures
// ============================================================================

/// Ответ GET /api/v1/calendar/promotions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WbCalendarPromotionsResponse {
    #[serde(default)]
    pub data: Option<WbCalendarPromotionsData>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WbCalendarPromotionsData {
    #[serde(default)]
    pub promotions: Vec<WbCalendarPromotion>,
    #[serde(rename = "upcomingPromos", default)]
    pub upcoming_promos: Vec<WbCalendarPromotion>,
}

/// Одна акция из WB Calendar API
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WbCalendarPromotion {
    /// WB использует поле "id" (не "promotionID")
    pub id: i64,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(rename = "startDateTime", default)]
    pub start_date_time: Option<String>,
    #[serde(rename = "endDateTime", default)]
    pub end_date_time: Option<String>,
    /// Тип акции: "auto", "regular", etc.
    #[serde(rename = "type", default)]
    pub promotion_type: Option<String>,
    #[serde(rename = "exceptionProductsCount", default)]
    pub exception_products_count: Option<i32>,
    #[serde(rename = "inPromoActionTotal", default)]
    pub in_promo_action_total: Option<i32>,
}

/// Ответ GET /api/v1/calendar/promotions/details
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WbCalendarPromotionDetailsResponse {
    #[serde(default)]
    pub data: Option<WbCalendarPromotionDetailsData>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WbCalendarPromotionDetailsData {
    #[serde(default)]
    pub promotions: Vec<WbCalendarPromotionDetail>,
}

/// Детальные данные акции из /details
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WbCalendarPromotionDetail {
    pub id: i64,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub advantages: Vec<String>,
    #[serde(rename = "startDateTime", default)]
    pub start_date_time: Option<String>,
    #[serde(rename = "endDateTime", default)]
    pub end_date_time: Option<String>,
    #[serde(rename = "inPromoActionLeftovers", default)]
    pub in_promo_action_leftovers: Option<i32>,
    #[serde(rename = "inPromoActionTotal", default)]
    pub in_promo_action_total: Option<i32>,
    #[serde(rename = "notInPromoActionLeftovers", default)]
    pub not_in_promo_action_leftovers: Option<i32>,
    #[serde(rename = "notInPromoActionTotal", default)]
    pub not_in_promo_action_total: Option<i32>,
    #[serde(rename = "participationPercentage", default)]
    pub participation_percentage: Option<f64>,
    #[serde(rename = "type", default)]
    pub promotion_type: Option<String>,
    #[serde(rename = "exceptionProductsCount", default)]
    pub exception_products_count: Option<i32>,
    #[serde(default)]
    pub ranging: Vec<WbPromotionRanging>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WbPromotionRanging {
    #[serde(default)]
    pub condition: Option<String>,
    #[serde(rename = "participationRate", default)]
    pub participation_rate: Option<f64>,
    #[serde(default)]
    pub boost: Option<f64>,
}

/// Ответ GET /api/v1/calendar/promotions/nomenclatures
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WbPromotionNomenclaturesResponse {
    #[serde(default)]
    pub data: Option<WbPromotionNomenclaturesData>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WbPromotionNomenclaturesData {
    #[serde(default)]
    pub nomenclatures: Vec<WbPromotionNmItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WbPromotionNmItem {
    /// API возвращает поле "id" (это nmId товара)
    #[serde(rename = "id")]
    pub nm_id: i64,
    #[serde(rename = "inAction", default)]
    pub in_action: bool,
    #[serde(default)]
    pub price: Option<f64>,
    #[serde(rename = "planPrice", default)]
    pub plan_price: Option<f64>,
    #[serde(default)]
    pub discount: Option<f64>,
    #[serde(rename = "planDiscount", default)]
    pub plan_discount: Option<f64>,
}

// ============================================================================
// WB Advertising Campaigns API structures (/adv/v3/fullstats)
// ============================================================================

/// Ответ GET /adv/v1/promotion/count вЂ” список рекламных кампаний по типу/статусу
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WbAdvertCampaignListResponse {
    #[serde(default)]
    pub adverts: Option<Vec<WbAdvertCampaignGroup>>,
    #[serde(default)]
    pub all: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WbAdvertCampaignGroup {
    #[serde(rename = "type", default)]
    pub campaign_type: Option<i32>,
    #[serde(default)]
    pub status: Option<i32>,
    #[serde(default)]
    pub count: Option<i32>,
    #[serde(rename = "advert_list", default)]
    pub advert_list: Vec<WbAdvertCampaignEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WbAdvertCampaignEntry {
    #[serde(rename = "advertId")]
    pub advert_id: i64,
    #[serde(rename = "changeTime", default)]
    pub change_time: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WbAdvertCampaignSummary {
    pub advert_id: i64,
    pub campaign_type: Option<i32>,
    pub status: Option<i32>,
    pub change_time: Option<String>,
}

/// РЎС‚Р°С‚РёСЃС‚РёРєР° на уровне одного товара (nmId) внутри дня и типа приложения
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WbAdvertCampaignsResponse {
    #[serde(default)]
    pub adverts: Vec<WbAdvertCampaign>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WbAdvertCampaign {
    pub id: i64,
    #[serde(default)]
    pub settings: WbAdvertCampaignSettings,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WbAdvertCampaignSettings {
    #[serde(default)]
    pub placements: WbAdvertCampaignPlacements,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WbAdvertCampaignPlacements {
    #[serde(default)]
    pub search: bool,
    #[serde(default)]
    pub recommendations: bool,
}

/// РЎС‚Р°С‚РёСЃС‚РёРєР° на уровне одного товара (nmId) внутри дня и типа приложения
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WbAdvertFullStatNm {
    #[serde(rename = "nmId")]
    pub nm_id: i64,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub views: i64,
    #[serde(default)]
    pub clicks: i64,
    #[serde(default)]
    pub ctr: f64,
    #[serde(default)]
    pub cpc: f64,
    #[serde(default)]
    pub atbs: i64,
    #[serde(default)]
    pub orders: i64,
    #[serde(default)]
    pub shks: i64,
    #[serde(default)]
    pub sum: f64,
    #[serde(rename = "sum_price", default)]
    pub sum_price: f64,
    #[serde(default)]
    pub cr: f64,
    #[serde(default)]
    pub canceled: i64,
}

/// РЎС‚Р°С‚РёСЃС‚РёРєР° по типу приложения (appType: 1=iOS, 32=Android, 64=Web)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WbAdvertFullStatApp {
    #[serde(rename = "appType")]
    pub app_type: i32,
    #[serde(default)]
    pub nms: Vec<WbAdvertFullStatNm>,
    #[serde(default)]
    pub views: i64,
    #[serde(default)]
    pub clicks: i64,
    #[serde(default)]
    pub ctr: f64,
    #[serde(default)]
    pub cpc: f64,
    #[serde(default)]
    pub atbs: i64,
    #[serde(default)]
    pub orders: i64,
    #[serde(default)]
    pub shks: i64,
    #[serde(default)]
    pub sum: f64,
    #[serde(rename = "sum_price", default)]
    pub sum_price: f64,
    #[serde(default)]
    pub cr: f64,
    #[serde(default)]
    pub canceled: i64,
}

/// РЎС‚Р°С‚РёСЃС‚РёРєР° за один день по кампании
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WbAdvertFullStatDay {
    pub date: String,
    #[serde(default)]
    pub apps: Vec<WbAdvertFullStatApp>,
    #[serde(default)]
    pub views: i64,
    #[serde(default)]
    pub clicks: i64,
    #[serde(default)]
    pub ctr: f64,
    #[serde(default)]
    pub cpc: f64,
    #[serde(default)]
    pub atbs: i64,
    #[serde(default)]
    pub orders: i64,
    #[serde(default)]
    pub shks: i64,
    #[serde(default)]
    pub sum: f64,
    #[serde(rename = "sum_price", default)]
    pub sum_price: f64,
    #[serde(default)]
    pub cr: f64,
    #[serde(default)]
    pub canceled: i64,
}

/// РЎРІРѕРґРЅР°СЏ статистика по одной рекламной кампании за период
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WbAdvertFullStat {
    #[serde(rename = "advertId")]
    pub advert_id: i64,
    #[serde(default)]
    pub days: Vec<WbAdvertFullStatDay>,
    #[serde(default)]
    pub views: i64,
    #[serde(default)]
    pub clicks: i64,
    #[serde(default)]
    pub ctr: f64,
    #[serde(default)]
    pub cpc: f64,
    #[serde(default)]
    pub atbs: i64,
    #[serde(default)]
    pub orders: i64,
    #[serde(default)]
    pub shks: i64,
    #[serde(default)]
    pub sum: f64,
    #[serde(rename = "sum_price", default)]
    pub sum_price: f64,
    #[serde(default)]
    pub cr: f64,
    #[serde(default)]
    pub canceled: i64,
}

// ============================================================================
// WB Supply (FBS) structs and methods
// ============================================================================

/// Поставка из /api/v3/supplies
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WbSupplyRow {
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(rename = "isB2b", default)]
    pub is_b2b: Option<bool>,
    #[serde(default)]
    pub done: Option<bool>,
    #[serde(rename = "createdAt", default)]
    pub created_at: Option<String>,
    #[serde(rename = "closedAt", default)]
    pub closed_at: Option<String>,
    #[serde(rename = "scanDt", default)]
    pub scan_dt: Option<String>,
    #[serde(rename = "cargoType", default)]
    pub cargo_type: Option<i32>,
    #[serde(rename = "crossBorderType", default)]
    pub cross_border_type: Option<i32>,
    #[serde(rename = "destinationOfficeId", default)]
    pub destination_office_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WbSuppliesResponse {
    pub next: i64,
    #[serde(default)]
    pub supplies: Vec<WbSupplyRow>,
}

/// Заказ внутри поставки из /api/v3/supplies/{id}/orders
#[derive(Debug, Clone, Serialize, Deserialize)]
struct WbSupplyOrderIdsResponse {
    #[serde(rename = "orderIds", default)]
    pub order_ids: Vec<i64>,
}

/// РЎС‚РёРєРµСЂ из /api/v3/orders/stickers
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WbStickerRow {
    #[serde(rename = "orderId", default)]
    pub order_id: i64,
    /// WB returns partA/partB as either integers or quoted strings — handle both.
    #[serde(rename = "partA", default, deserialize_with = "deser_str_or_i64")]
    pub part_a: Option<i64>,
    #[serde(rename = "partB", default, deserialize_with = "deser_str_or_i64")]
    pub part_b: Option<i64>,
    #[serde(default)]
    pub barcode: Option<String>,
    #[serde(default)]
    pub file: Option<String>,
}

/// Deserializes a field that WB sometimes sends as an integer and sometimes as a quoted string.
fn deser_str_or_i64<'de, D>(deserializer: D) -> Result<Option<i64>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::{self, Unexpected, Visitor};
    use std::fmt;

    struct StrOrI64;

    impl<'de> Visitor<'de> for StrOrI64 {
        type Value = Option<i64>;

        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "an integer or a string containing an integer")
        }

        fn visit_i64<E: de::Error>(self, v: i64) -> Result<Self::Value, E> {
            Ok(if v == 0 { None } else { Some(v) })
        }

        fn visit_u64<E: de::Error>(self, v: u64) -> Result<Self::Value, E> {
            Ok(if v == 0 { None } else { Some(v as i64) })
        }

        fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
            if v.is_empty() {
                return Ok(None);
            }
            v.parse::<i64>()
                .map(|n| if n == 0 { None } else { Some(n) })
                .map_err(|_| de::Error::invalid_value(Unexpected::Str(v), &self))
        }

        fn visit_none<E: de::Error>(self) -> Result<Self::Value, E> {
            Ok(None)
        }

        fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
            Ok(None)
        }
    }

    deserializer.deserialize_any(StrOrI64)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WbStickersResponse {
    #[serde(default)]
    pub stickers: Vec<WbStickerRow>,
}

/// Диагностика пустого результата загрузки заказов.
///
/// WB игнорирует `dateFrom` за пределами своей глубины хранения и отдаёт самые старые из
/// доступных строк. Тогда soft-stop по `date_to` отсеивает вообще всё, и импорт «успешно»
/// завершается нулём — на экране это выглядит как зависшая загрузка без объяснений.
/// Отличаем такой случай от честного «за период заказов не было» (WB вернул пусто) и
/// называем фактическую границу, до которой WB ещё отдаёт заказы.
///
/// `None` — ситуация штатная, ошибку поднимать не нужно.
fn unavailable_orders_period_message(
    date_from: chrono::NaiveDate,
    date_to: chrono::NaiveDate,
    kept_rows: usize,
    received_rows: usize,
    earliest_change: Option<chrono::DateTime<chrono::Utc>>,
) -> Option<String> {
    if kept_rows > 0 || received_rows == 0 {
        return None;
    }
    let boundary = format_wb_local_datetime_seconds(&earliest_change?);
    Some(format!(
        "Wildberries не отдаёт заказы за период {date_from} — {date_to}: в ответе {received_rows} строк, \
         и самая ранняя из них — {boundary} (МСК). Statistics API хранит заказы ограниченное время, \
         более ранние периоды через него недоступны. Запросите период начиная с {boundary}."
    ))
}

fn supply_matches_window(
    supply: &WbSupplyRow,
    range_start: chrono::DateTime<chrono::Utc>,
    range_end: chrono::DateTime<chrono::Utc>,
) -> bool {
    let created_at = supply.created_at.as_deref().and_then(parse_wb_datetime);
    let closed_at = supply.closed_at.as_deref().and_then(parse_wb_datetime);
    let scan_dt = supply.scan_dt.as_deref().and_then(parse_wb_datetime);

    let in_range = |value: Option<chrono::DateTime<chrono::Utc>>| {
        value
            .map(|dt| dt >= range_start && dt <= range_end)
            .unwrap_or(false)
    };

    if in_range(created_at) || in_range(closed_at) || in_range(scan_dt) {
        return true;
    }

    !supply.done.unwrap_or(false) && created_at.map(|dt| dt <= range_end).unwrap_or(true)
}

impl WildberriesApiClient {
    pub async fn fetch_supplies(
        &self,
        connection: &contracts::domain::a006_connection_mp::aggregate::ConnectionMP,
        date_from: chrono::NaiveDate,
        date_to: chrono::NaiveDate,
    ) -> anyhow::Result<Vec<WbSupplyRow>> {
        let url = "https://marketplace-api.wildberries.ru/api/v3/supplies";
        let mut all_supplies: Vec<WbSupplyRow> = Vec::new();
        let mut next_cursor: i64 = 0;
        let range_start =
            wb_day_start_utc(date_from).ok_or_else(|| anyhow::anyhow!("Invalid date_from"))?;
        let range_end =
            wb_day_end_utc(date_to).ok_or_else(|| anyhow::anyhow!("Invalid date_to"))?;

        loop {
            let next_str = next_cursor.to_string();
            let response = match self
                .client
                .get(url)
                .header("Authorization", &connection.api_key)
                .query(&[("limit", "1000"), ("next", next_str.as_str())])
                .send()
                .await
            {
                Ok(r) => r,
                Err(e) => {
                    return Err(anyhow::anyhow!("HTTP error fetching supplies: {}", e));
                }
            };

            if response.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
                tracing::warn!("WB supplies API rate limit, sleeping 65s");
                tokio::time::sleep(std::time::Duration::from_secs(65)).await;
                continue;
            }

            if !response.status().is_success() {
                let status = response.status();
                let body = self.read_body_tracked(response).await.unwrap_or_default();
                return Err(anyhow::anyhow!(
                    "WB supplies API error {}: {}",
                    status,
                    body
                ));
            }

            let parsed: WbSuppliesResponse = response
                .json()
                .await
                .map_err(|e| anyhow::anyhow!("Failed to parse WB supplies response: {}", e))?;

            let page_supplies = parsed.supplies;
            let new_next = parsed.next;

            for supply in page_supplies {
                if supply_matches_window(&supply, range_start, range_end) {
                    all_supplies.push(supply);
                }
            }

            if new_next == 0 {
                break;
            }
            next_cursor = new_next;

            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }

        tracing::info!(
            "Fetched {} supplies in date range {}-{}",
            all_supplies.len(),
            date_from,
            date_to
        );
        Ok(all_supplies)
    }

    pub async fn fetch_supply_orders(
        &self,
        connection: &contracts::domain::a006_connection_mp::aggregate::ConnectionMP,
        supply_id: &str,
    ) -> anyhow::Result<Vec<i64>> {
        let url = format!(
            "https://marketplace-api.wildberries.ru/api/marketplace/v3/supplies/{}/order-ids",
            supply_id
        );

        let response = self
            .client
            .get(&url)
            .header("Authorization", &connection.api_key)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to fetch supply order ids: {}", e))?;

        let status = response.status();

        // 404 means WB has no orders for this supply (expected for old/closed supplies)
        if status == reqwest::StatusCode::NOT_FOUND {
            let body = self.read_body_tracked(response).await.unwrap_or_default();
            tracing::info!(
                "WB supply orders 404 for supply {} — body: {}",
                supply_id,
                body
            );
            return Ok(vec![]);
        }

        if !status.is_success() {
            let body = self.read_body_tracked(response).await.unwrap_or_default();
            return Err(anyhow::anyhow!(
                "WB supply order ids API error {}: {}",
                status,
                body
            ));
        }

        let body = response
            .text()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to read supply order ids response: {}", e))?;
        tracing::info!(
            "WB supply order ids raw response for {}: {}",
            supply_id,
            &body[..body.len().min(500)]
        );

        let parsed: WbSupplyOrderIdsResponse = serde_json::from_str(&body).map_err(|e| {
            anyhow::anyhow!(
                "Failed to parse supply order ids response: {}\nBody: {}",
                e,
                &body[..body.len().min(300)]
            )
        })?;

        Ok(parsed.order_ids)
    }

    pub async fn fetch_supply_order_ids(
        &self,
        connection: &contracts::domain::a006_connection_mp::aggregate::ConnectionMP,
        supply_id: &str,
    ) -> anyhow::Result<Vec<i64>> {
        let url = format!(
            "https://marketplace-api.wildberries.ru/api/marketplace/v3/supplies/{}/order-ids",
            supply_id
        );

        let response = self
            .client
            .get(&url)
            .header("Authorization", &connection.api_key)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to fetch supply order ids: {}", e))?;

        let status = response.status();
        if status == reqwest::StatusCode::NOT_FOUND {
            let body = self.read_body_tracked(response).await.unwrap_or_default();
            tracing::info!(
                "WB supply order ids 404 for supply {} — body: {}",
                supply_id,
                body
            );
            return Ok(vec![]);
        }

        if !status.is_success() {
            let body = self.read_body_tracked(response).await.unwrap_or_default();
            return Err(anyhow::anyhow!(
                "WB supply order ids API error {}: {}",
                status,
                body
            ));
        }

        let body = response
            .text()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to read supply order ids response: {}", e))?;
        tracing::info!(
            "WB supply order ids raw response for {}: {}",
            supply_id,
            &body[..body.len().min(500)]
        );

        let parsed: WbSupplyOrderIdsResponse = serde_json::from_str(&body).map_err(|e| {
            anyhow::anyhow!(
                "Failed to parse supply order ids response: {}\nBody: {}",
                e,
                &body[..body.len().min(300)]
            )
        })?;

        Ok(parsed
            .order_ids
            .into_iter()
            .filter(|&order_id| order_id > 0)
            .collect())
    }

    pub async fn fetch_order_stickers(
        &self,
        connection: &contracts::domain::a006_connection_mp::aggregate::ConnectionMP,
        order_ids: &[i64],
        sticker_type: &str,
        width: i32,
        height: i32,
    ) -> anyhow::Result<Vec<WbStickerRow>> {
        if order_ids.is_empty() {
            return Ok(vec![]);
        }

        // WB API limit: max 100 order IDs per request
        const BATCH_SIZE: usize = 100;
        let url = "https://marketplace-api.wildberries.ru/api/v3/orders/stickers";
        let mut all_stickers: Vec<WbStickerRow> = Vec::new();

        for chunk in order_ids.chunks(BATCH_SIZE) {
            let body = serde_json::json!({ "orders": chunk });
            let request_body = body.to_string();
            let request_body_len = request_body.len() as u64;

            let response = self
                .client
                .post(url)
                .header("Authorization", &connection.api_key)
                .header("Content-Type", "application/json")
                .query(&[
                    ("type", sticker_type),
                    ("width", &width.to_string()),
                    ("height", &height.to_string()),
                ])
                .body(request_body)
                .send()
                .await
                .map_err(|e| anyhow::anyhow!("Failed to fetch stickers: {}", e))?;

            if !response.status().is_success() {
                let status = response.status();
                let body_text = self
                    .read_body_tracked_with_request_bytes(response, request_body_len)
                    .await
                    .unwrap_or_default();
                return Err(anyhow::anyhow!(
                    "WB stickers API error {}: {}",
                    status,
                    body_text
                ));
            }

            let body_text = self
                .read_body_tracked_with_request_bytes(response, request_body_len)
                .await
                .map_err(|e| anyhow::anyhow!("Failed to read stickers response body: {}", e))?;

            tracing::debug!(
                "WB stickers raw response (batch {} ids): {}",
                chunk.len(),
                &body_text[..body_text.len().min(500)]
            );

            let parsed: WbStickersResponse = serde_json::from_str(&body_text).map_err(|e| {
                anyhow::anyhow!(
                    "Failed to parse stickers JSON: {}\nRaw: {}",
                    e,
                    &body_text[..body_text.len().min(500)]
                )
            })?;

            all_stickers.extend(parsed.stickers);
        }

        Ok(all_stickers)
    }

    /// Fetches brand-new FBS orders from /api/v3/orders/new (no cursor pagination).
    /// These are orders in "waiting" status — just placed, not yet in any supply.
    /// Call this for real-time order visibility without the statistics API delay.
    pub async fn fetch_new_marketplace_orders(
        &self,
        connection: &contracts::domain::a006_connection_mp::aggregate::ConnectionMP,
    ) -> anyhow::Result<Vec<WbMarketplaceOrderRow>> {
        let url = "https://marketplace-api.wildberries.ru/api/v3/orders/new";
        self.record_http_request_attempt(0);

        let response = self
            .client
            .get(url)
            .header("Authorization", &connection.api_key)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to fetch new marketplace orders: {}", e))?;

        let status = response.status();
        if !status.is_success() {
            let body = self
                .read_body_for_recorded_request(response)
                .await
                .unwrap_or_default();
            return Err(anyhow::anyhow!(
                "WB /api/v3/orders/new error {}: {}",
                status,
                body
            ));
        }

        let body = self
            .read_body_for_recorded_request(response)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to read new orders response: {}", e))?;

        // /api/v3/orders/new returns {"orders": [...]} without pagination
        let parsed: WbMarketplaceOrdersResponse = serde_json::from_str(&body).map_err(|e| {
            anyhow::anyhow!(
                "Failed to parse new orders response: {}\nBody: {}",
                e,
                &body[..body.len().min(500)]
            )
        })?;

        tracing::info!(
            "Fetched {} new marketplace orders from /api/v3/orders/new",
            parsed.orders.len()
        );
        Ok(parsed.orders)
    }

    /// Fetches all FBS orders from /api/v3/orders with cursor pagination.
    /// Returns orders with supplyId field — the real-time link between orders and supplies.
    pub async fn fetch_marketplace_orders(
        &self,
        connection: &contracts::domain::a006_connection_mp::aggregate::ConnectionMP,
        date_from: i64,
        date_to: i64,
    ) -> anyhow::Result<Vec<WbMarketplaceOrderRow>> {
        let mut all_orders: Vec<WbMarketplaceOrderRow> = Vec::new();
        let mut next_cursor: i64 = 0;
        let limit = 1000i64;

        loop {
            let url = "https://marketplace-api.wildberries.ru/api/v3/orders";
            self.record_http_request_attempt(0);
            let response = self
                .client
                .get(url)
                .header("Authorization", &connection.api_key)
                .query(&[
                    ("limit", limit.to_string()),
                    ("next", next_cursor.to_string()),
                    ("dateFrom", date_from.to_string()),
                    ("dateTo", date_to.to_string()),
                ])
                .send()
                .await
                .map_err(|e| anyhow::anyhow!("Failed to fetch marketplace orders: {}", e))?;

            let status = response.status();
            if !status.is_success() {
                let body = self
                    .read_body_for_recorded_request(response)
                    .await
                    .unwrap_or_default();
                return Err(anyhow::anyhow!(
                    "WB marketplace orders API error {}: {}",
                    status,
                    body
                ));
            }

            let body = self
                .read_body_for_recorded_request(response)
                .await
                .map_err(|e| {
                    anyhow::anyhow!("Failed to read marketplace orders response: {}", e)
                })?;

            let parsed: WbMarketplaceOrdersResponse = serde_json::from_str(&body).map_err(|e| {
                anyhow::anyhow!(
                    "Failed to parse marketplace orders response: {}\nBody: {}",
                    e,
                    &body[..body.len().min(500)]
                )
            })?;

            let page_count = parsed.orders.len();
            tracing::info!(
                "Marketplace orders page: {} records, next={}",
                page_count,
                parsed.next
            );

            all_orders.extend(parsed.orders);

            if parsed.next == 0 || page_count == 0 {
                break;
            }
            next_cursor = parsed.next;
        }

        tracing::info!(
            "Fetched {} marketplace orders total (dateFrom={}, dateTo={})",
            all_orders.len(),
            date_from,
            date_to
        );
        Ok(all_orders)
    }

    /// GET https://returns-api.wildberries.ru/api/v1/claims
    ///
    /// Загружает заявки покупателей на возврат товара.
    /// Requires: WB token with "Buyers Returns" category.
    /// Returns last 14 days only. Fetches both is_archive=false and is_archive=true.
    pub async fn fetch_claims(&self, connection: &ConnectionMP) -> Result<Vec<WbClaimRow>> {
        const BASE_URL: &str = "https://returns-api.wildberries.ru/api/v1/claims";
        const PAGE_LIMIT: u32 = 200;

        if connection.api_key.trim().is_empty() {
            anyhow::bail!("API Key is required for WB Buyers Returns API");
        }

        let mut all_claims: Vec<WbClaimRow> = Vec::new();

        for is_archive in [false, true] {
            let archive_label = if is_archive { "archive" } else { "active" };
            let mut offset: u32 = 0;
            let mut page = 0u32;

            loop {
                page += 1;
                self.log_to_file(&format!(
                    "=== WB Claims ({archive_label}) page {page} offset={offset} ==="
                ));
                self.record_http_request_attempt(0);

                let resp = match self
                    .client
                    .get(BASE_URL)
                    .header("Authorization", connection.api_key.trim())
                    .query(&[
                        ("is_archive", is_archive.to_string()),
                        ("limit", PAGE_LIMIT.to_string()),
                        ("offset", offset.to_string()),
                    ])
                    .send()
                    .await
                {
                    Ok(r) => r,
                    Err(e) => {
                        anyhow::bail!("WB Claims API request failed: {}", e);
                    }
                };

                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                self.record_http_response_body(body.len() as u64);

                if status == 429 {
                    tracing::warn!("WB Claims API rate limit hit, sleeping 60s");
                    tokio::time::sleep(std::time::Duration::from_secs(60)).await;
                    continue;
                }

                if status == 404 {
                    // 404 from feedbacks-api auth gateway means the API key
                    // lacks the "Buyers Returns" token category.
                    tracing::warn!(
                        "WB Claims API: 404 Not Found — API key does not have \
                         'Buyers Returns' (Возвраты покупателей) permission. \
                         Skipping claims import. Response: {}",
                        &body[..body.len().min(300)]
                    );
                    return Ok(all_claims);
                }

                if !status.is_success() {
                    anyhow::bail!(
                        "WB Claims API returned status {}: {}",
                        status,
                        &body[..body.len().min(500)]
                    );
                }

                let parsed: WbClaimsResponse = match serde_json::from_str(&body) {
                    Ok(v) => v,
                    Err(e) => {
                        anyhow::bail!(
                            "Failed to parse WB Claims response: {}: {}",
                            e,
                            &body[..body.len().min(500)]
                        );
                    }
                };

                let page_len = parsed.claims.len();
                self.log_to_file(&format!(
                    "WB Claims ({archive_label}) page {page}: {page_len} items"
                ));

                all_claims.extend(parsed.claims);

                if page_len < PAGE_LIMIT as usize {
                    break;
                }
                offset += PAGE_LIMIT;
            }
        }

        tracing::info!("WB Claims: fetched {} total", all_claims.len());
        Ok(all_claims)
    }
}

/// Order from /api/v3/orders — marketplace FBS orders with real-time supplyId.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WbMarketplaceOrderRow {
    pub id: i64,
    #[serde(rename = "orderUid", default)]
    pub order_uid: Option<String>,
    #[serde(default)]
    pub article: Option<String>,
    #[serde(rename = "nmId", default)]
    pub nm_id: Option<i64>,
    #[serde(rename = "chrtId", default)]
    pub chrt_id: Option<i64>,
    #[serde(default)]
    pub rid: Option<String>,
    #[serde(rename = "createdAt", default)]
    pub created_at: Option<String>,
    #[serde(rename = "warehouseId", default)]
    pub warehouse_id: Option<i64>,
    #[serde(rename = "salePrice", default)]
    pub sale_price: Option<i64>,
    #[serde(rename = "scanPrice", default)]
    pub scan_price: Option<i64>,
    #[serde(default)]
    pub price: Option<i64>,
    #[serde(rename = "finalPrice", default)]
    pub final_price: Option<i64>,
    #[serde(rename = "convertedPrice", default)]
    pub converted_price: Option<i64>,
    #[serde(rename = "convertedFinalPrice", default)]
    pub converted_final_price: Option<i64>,
    #[serde(rename = "currencyCode", default)]
    pub currency_code: Option<i32>,
    #[serde(rename = "convertedCurrencyCode", default)]
    pub converted_currency_code: Option<i32>,
    #[serde(rename = "cargoType", default)]
    pub cargo_type: Option<i32>,
    /// Supply ID in format "WB-GI-XXXXXXXX" — the key for linking orders to supplies.
    #[serde(rename = "supplyId", default)]
    pub supply_id: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(rename = "isZeroOrder", default)]
    pub is_zero_order: Option<bool>,
    #[serde(default)]
    pub skus: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WbMarketplaceOrdersResponse {
    #[serde(default)]
    pub next: i64,
    #[serde(default)]
    pub orders: Vec<WbMarketplaceOrderRow>,
}

/// Заявка покупателя на возврат из GET /api/v1/claims
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WbClaimRow {
    pub id: String,
    #[serde(rename = "claim_type", default)]
    pub claim_type: Option<i32>,
    #[serde(default)]
    pub status: Option<i32>,
    #[serde(rename = "status_ex", default)]
    pub status_ex: Option<i32>,
    #[serde(rename = "nm_id", default)]
    pub nm_id: Option<i64>,
    #[serde(rename = "imt_name", default)]
    pub imt_name: Option<String>,
    #[serde(rename = "user_comment", default)]
    pub user_comment: Option<String>,
    #[serde(rename = "wb_comment", default)]
    pub wb_comment: Option<String>,
    #[serde(default)]
    pub dt: Option<String>,
    #[serde(rename = "order_dt", default)]
    pub order_dt: Option<String>,
    #[serde(rename = "dt_update", default)]
    pub dt_update: Option<String>,
    #[serde(rename = "delivery_dt", default)]
    pub delivery_dt: Option<String>,
    #[serde(default)]
    pub price: Option<f64>,
    #[serde(rename = "currency_code", default)]
    pub currency_code: Option<String>,
    #[serde(default)]
    pub srid: Option<String>,
    #[serde(rename = "origin_id_info", default)]
    pub origin_id_info: Option<String>,
    #[serde(default)]
    pub actions: Option<Vec<String>>,
    #[serde(rename = "is_archive", default)]
    pub is_archive: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WbClaimsResponse {
    #[serde(default)]
    pub claims: Vec<WbClaimRow>,
    #[serde(default)]
    pub total: Option<i64>,
}

// ============================================================================
// WB Sales Funnel (Analytics API v3)
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WbSalesFunnelHistoryItem {
    pub product: WbSalesFunnelProduct,
    #[serde(default)]
    pub history: Vec<WbSalesFunnelHistoryDay>,
    #[serde(default)]
    pub currency: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WbSalesFunnelProduct {
    #[serde(rename = "nmId", alias = "nmID", default)]
    pub nm_id: i64,
    #[serde(default)]
    pub title: String,
    #[serde(rename = "vendorCode", default)]
    pub vendor_code: String,
    #[serde(rename = "brandName", default)]
    pub brand_name: String,
    #[serde(rename = "subjectId", alias = "subjectID", default)]
    pub subject_id: i64,
    #[serde(rename = "subjectName", default)]
    pub subject_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WbSalesFunnelHistoryDay {
    #[serde(rename = "date", alias = "dt", default)]
    pub date: String,
    #[serde(rename = "openCount", default)]
    pub open_count: i64,
    #[serde(rename = "cartCount", default)]
    pub cart_count: i64,
    #[serde(rename = "orderCount", default)]
    pub order_count: i64,
    #[serde(rename = "orderSum", default)]
    pub order_sum: f64,
    #[serde(rename = "buyoutCount", default)]
    pub buyout_count: i64,
    #[serde(rename = "buyoutSum", default)]
    pub buyout_sum: f64,
    /// Отмены. Имя поля у v3-истории не зафиксировано документацией — принимаем
    /// известные варианты; отсутствие поля даёт `None` (N/A), а не 0.
    #[serde(
        rename = "cancelCount",
        alias = "cancelsCount",
        alias = "canceledCount",
        default
    )]
    pub cancel_count: Option<i64>,
    #[serde(
        rename = "cancelSum",
        alias = "cancelSumRub",
        alias = "cancelsSumRub",
        default
    )]
    pub cancel_sum: Option<f64>,
    #[serde(rename = "buyoutPercent", default)]
    pub buyout_percent: f64,
    #[serde(rename = "addToCartConversion", default)]
    pub add_to_cart_conversion: f64,
    #[serde(rename = "cartToOrderConversion", default)]
    pub cart_to_order_conversion: f64,
    #[serde(rename = "addToWishlistCount", default)]
    pub add_to_wishlist_count: i64,
}

#[derive(Debug, Clone, Deserialize)]
struct WbAnalyticsReportListResponse {
    #[serde(default)]
    data: Vec<WbAnalyticsReportStatus>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WbAnalyticsReportStatus {
    pub id: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub size: i64,
    #[serde(rename = "startDate", default)]
    pub start_date: String,
    #[serde(rename = "endDate", default)]
    pub end_date: String,
}

/// Плоская строка CSV-отчёта WB `DETAIL_HISTORY_REPORT`.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct WbSalesFunnelDetailRow {
    #[serde(rename = "nmID")]
    pub nm_id: i64,
    #[serde(rename = "dt")]
    pub date: String,
    #[serde(rename = "openCardCount")]
    pub open_count: i64,
    #[serde(rename = "addToCartCount")]
    pub cart_count: i64,
    #[serde(rename = "ordersCount")]
    pub order_count: i64,
    #[serde(rename = "ordersSumRub")]
    pub order_sum: f64,
    #[serde(rename = "buyoutsCount")]
    pub buyout_count: i64,
    #[serde(rename = "buyoutsSumRub")]
    pub buyout_sum: f64,
    #[serde(rename = "cancelCount")]
    pub cancel_count: i64,
    #[serde(rename = "cancelSumRub")]
    pub cancel_sum: f64,
    #[serde(rename = "addToCartConversion")]
    pub add_to_cart_conversion: f64,
    #[serde(rename = "cartToOrderConversion")]
    pub cart_to_order_conversion: f64,
    #[serde(rename = "buyoutPercent")]
    pub buyout_percent: f64,
    #[serde(rename = "addToWishlist")]
    pub add_to_wishlist_count: i64,
    pub currency: String,
}

const WB_DETAIL_HISTORY_REQUIRED_HEADERS: &[&str] = &[
    "nmID",
    "dt",
    "openCardCount",
    "addToCartCount",
    "ordersCount",
    "ordersSumRub",
    "buyoutsCount",
    "buyoutsSumRub",
    "cancelCount",
    "cancelSumRub",
    "addToCartConversion",
    "cartToOrderConversion",
    "buyoutPercent",
    "addToWishlist",
    "currency",
];

/// Строго разбирает все CSV-файлы из ZIP, полученного от WB.
pub fn parse_sales_funnel_detail_zip(bytes: &[u8]) -> Result<Vec<WbSalesFunnelDetailRow>> {
    let cursor = Cursor::new(bytes);
    let mut archive =
        zip::ZipArchive::new(cursor).context("WB DETAIL_HISTORY_REPORT is not a valid ZIP")?;
    let mut rows = Vec::new();
    let mut csv_files = 0usize;

    for index in 0..archive.len() {
        let mut file = archive
            .by_index(index)
            .with_context(|| format!("Failed to open ZIP entry {}", index))?;
        if !file.is_file() || !file.name().to_ascii_lowercase().ends_with(".csv") {
            continue;
        }
        csv_files += 1;
        let file_name = file.name().to_string();
        let mut csv_bytes = Vec::new();
        file.read_to_end(&mut csv_bytes)
            .with_context(|| format!("Failed to read CSV entry {}", file_name))?;

        let csv_text = match std::str::from_utf8(&csv_bytes) {
            Ok(value) => std::borrow::Cow::Borrowed(value),
            Err(_) => encoding_rs::WINDOWS_1251.decode(&csv_bytes).0,
        };
        let csv_text = csv_text.trim_start_matches('\u{feff}');
        let mut reader = csv::ReaderBuilder::new()
            .has_headers(true)
            .flexible(false)
            .trim(csv::Trim::All)
            .from_reader(csv_text.as_bytes());
        let headers = reader
            .headers()
            .with_context(|| format!("Failed to read CSV headers from {}", file_name))?
            .clone();
        let missing: Vec<&str> = WB_DETAIL_HISTORY_REQUIRED_HEADERS
            .iter()
            .copied()
            .filter(|required| !headers.iter().any(|actual| actual == *required))
            .collect();
        if !missing.is_empty() {
            anyhow::bail!(
                "WB DETAIL_HISTORY_REPORT CSV {} misses required headers: {}",
                file_name,
                missing.join(", ")
            );
        }

        for (row_index, result) in reader.deserialize::<WbSalesFunnelDetailRow>().enumerate() {
            let row = result.with_context(|| {
                format!(
                    "Failed to parse WB DETAIL_HISTORY_REPORT {} row {}",
                    file_name,
                    row_index + 2
                )
            })?;
            rows.push(row);
        }
    }

    if csv_files == 0 {
        anyhow::bail!("WB DETAIL_HISTORY_REPORT ZIP contains no CSV files");
    }
    Ok(rows)
}

/// Строка ежедневного снимка товара WB (для агрегата a037): сырые остатки и рейтинги
/// из `products[].product` эндпоинта /api/analytics/v3/sales-funnel/products.
#[derive(Debug, Clone)]
pub struct WbProductSnapshotRow {
    pub nm_id: i64,
    pub title: String,
    pub vendor_code: String,
    pub brand_name: String,
    pub subject_id: i64,
    pub subject_name: String,
    pub stock_wb: i64,
    pub stock_mp: i64,
    pub stock_balance_sum: f64,
    pub product_rating: f64,
    pub feedback_rating: f64,
}

// ============================================================================
// WB Search Analytics (search-report v2) — для агрегата a040
// ============================================================================

/// Метрики поиска по одному товару (nm_id) за период. Имена полей WB не
/// верифицированы офлайн — см. оговорку в `fetch_search_report`.
#[derive(Debug, Clone, Default)]
pub struct WbSearchReportRow {
    pub nm_id: i64,
    pub title: String,
    pub vendor_code: String,
    pub brand_name: String,
    pub subject_id: i64,
    pub subject_name: String,
    pub impressions: i64,
    pub open_card: i64,
    pub ctr: f64,
    pub add_to_cart: i64,
    pub orders: i64,
    pub avg_position: f64,
    pub visibility: f64,
}

/// Статистика по одному поисковому запросу для товара.
#[derive(Debug, Clone, Default)]
pub struct WbSearchQueryRow {
    pub nm_id: i64,
    pub text: String,
    pub frequency: i64,
    pub impressions: i64,
    pub clicks: i64,
    pub orders: i64,
    pub avg_position: f64,
}

/// Первый существующий ключ → i64 (принимает как число, так и объект `{current}`).
fn json_i64(value: &serde_json::Value, keys: &[&str]) -> Option<i64> {
    for k in keys {
        if let Some(v) = value.get(*k) {
            if let Some(n) = v.as_i64() {
                return Some(n);
            }
            if let Some(n) = v.as_f64() {
                return Some(n.round() as i64);
            }
            if let Some(c) = v.get("current").and_then(|c| c.as_f64()) {
                return Some(c.round() as i64);
            }
        }
    }
    None
}

/// Первый существующий ключ → f64 (принимает число или объект `{current}`).
fn json_f64(value: &serde_json::Value, keys: &[&str]) -> Option<f64> {
    for k in keys {
        if let Some(v) = value.get(*k) {
            if let Some(n) = v.as_f64() {
                return Some(n);
            }
            if let Some(c) = v.get("current").and_then(|c| c.as_f64()) {
                return Some(c);
            }
        }
    }
    None
}

/// Прошлый период для search-report: окно той же длины, оканчивающееся за день
/// до `date_from`. Обязательное поле `pastPeriod` в запросе WB.
fn past_period(date_from: &str, date_to: &str) -> (String, String) {
    use chrono::{Duration, NaiveDate};
    let parse = |s: &str| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok();
    match (parse(date_from), parse(date_to)) {
        (Some(from), Some(to)) => {
            let len = (to - from).num_days().max(0);
            let past_end = from - Duration::days(1);
            let past_start = past_end - Duration::days(len);
            (
                past_start.format("%Y-%m-%d").to_string(),
                past_end.format("%Y-%m-%d").to_string(),
            )
        }
        _ => (date_from.to_string(), date_to.to_string()),
    }
}

fn json_str(value: &serde_json::Value, keys: &[&str]) -> String {
    for k in keys {
        if let Some(s) = value.get(*k).and_then(|v| v.as_str()) {
            return s.to_string();
        }
    }
    String::new()
}

fn parse_search_report_row(item: &serde_json::Value) -> Option<WbSearchReportRow> {
    // Метрики могут лежать в объекте `product` и/или в корне строки.
    let product = item.get("product").unwrap_or(item);
    let nm_id = json_i64(product, &["nmId", "nmID", "nm_id"])
        .or_else(|| json_i64(item, &["nmId", "nmID", "nm_id"]))?;
    Some(WbSearchReportRow {
        nm_id,
        title: json_str(product, &["title", "name"]),
        vendor_code: json_str(product, &["vendorCode", "vendor_code"]),
        brand_name: json_str(product, &["brandName", "brand"]),
        subject_id: json_i64(product, &["subjectId", "subjectID"]).unwrap_or(0),
        subject_name: json_str(product, &["subjectName", "subject"]),
        // `/table/details` does not expose impression count. `visibility` is a percentage
        // and must not be written into impressions (it is bounded by 100).
        impressions: json_i64(item, &["impressions", "views", "shows"]).unwrap_or(0),
        open_card: json_i64(item, &["openCard", "openCardCount", "clicks"]).unwrap_or(0),
        ctr: json_f64(item, &["ctr"]).unwrap_or(0.0),
        add_to_cart: json_i64(item, &["addToCart", "addToCartCount", "tocart"]).unwrap_or(0),
        orders: json_i64(item, &["orders", "ordersCount"]).unwrap_or(0),
        avg_position: json_f64(item, &["avgPosition", "position"]).unwrap_or(0.0),
        visibility: json_f64(item, &["visibility"]).unwrap_or(0.0),
    })
}

fn parse_search_query_row(nm_id: i64, item: &serde_json::Value) -> WbSearchQueryRow {
    WbSearchQueryRow {
        nm_id,
        text: json_str(item, &["text", "searchText", "query", "name"]),
        frequency: json_i64(item, &["frequency", "requestCount", "freq"]).unwrap_or(0),
        impressions: json_i64(item, &["impressions", "views", "shows"]).unwrap_or(0),
        clicks: json_i64(item, &["clicks", "openCard", "openCardCount"]).unwrap_or(0),
        orders: json_i64(item, &["orders", "ordersCount"]).unwrap_or(0),
        avg_position: json_f64(item, &["avgPosition", "position"]).unwrap_or(0.0),
    }
}

#[derive(Debug, Clone)]
pub struct WbFinanceV1FetchedReport {
    pub header: serde_json::Value,
    pub report_id: String,
    pub lines: Vec<serde_json::Value>,
    pub pages_count: i32,
    pub last_rrd_id: Option<String>,
}

fn json_scalar_text(value: Option<&serde_json::Value>) -> Option<String> {
    match value? {
        serde_json::Value::String(v) => Some(v.clone()),
        serde_json::Value::Number(v) => Some(v.to_string()),
        _ => None,
    }
}

fn advancing_rrd_id(current: &str, next: &str) -> Result<u64> {
    let current_number = current
        .parse::<u64>()
        .with_context(|| format!("Invalid WB Finance rrdId cursor: {current}"))?;
    let next_number = next
        .parse::<u64>()
        .with_context(|| format!("Invalid WB Finance rrdId cursor: {next}"))?;
    if next_number <= current_number {
        anyhow::bail!("WB Finance rrdId did not advance: {current} -> {next}");
    }
    Ok(next_number)
}

fn finance_retry_seconds(headers: &HeaderMap) -> u64 {
    headers
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| {
            v.parse::<u64>().ok().or_else(|| {
                chrono::DateTime::parse_from_rfc2822(v).ok().map(|until| {
                    (until.with_timezone(&chrono::Utc) - chrono::Utc::now())
                        .num_seconds()
                        .max(1) as u64
                })
            })
        })
        .or_else(|| WbRateLimitHeaders::from_headers(headers).retry_seconds)
        .unwrap_or(WB_FINANCE_V1_FALLBACK_RETRY_SECS)
        .max(1)
}

impl WildberriesApiClient {
    async fn wait_finance_v1_gate(&self, connection: &ConnectionMP) {
        let gate = wb_finance_v1_gate(&connection.to_string_id());
        let mut next_allowed = gate.lock().await;
        if let Some(next) = *next_allowed {
            let now = std::time::Instant::now();
            if next > now {
                tokio::time::sleep(next - now).await;
            }
        }
        *next_allowed = Some(
            std::time::Instant::now()
                + std::time::Duration::from_secs(WB_FINANCE_V1_MIN_INTERVAL_SECS),
        );
    }

    async fn post_finance_v1(
        &self,
        connection: &ConnectionMP,
        url: &str,
        payload: &serde_json::Value,
    ) -> Result<(reqwest::StatusCode, String)> {
        if connection.api_key.trim().is_empty() {
            anyhow::bail!("API Key is required for Wildberries Finance API");
        }
        let request_body_len = serde_json::to_vec(payload)?.len() as u64;
        for attempt in 1..=4 {
            self.wait_finance_v1_gate(connection).await;
            self.record_http_request_attempt(request_body_len);
            let response = self
                .client
                .post(url)
                .header("Authorization", &connection.api_key)
                .json(payload)
                .send()
                .await
                .with_context(|| format!("WB Finance API request failed: {url}"))?;
            let status = response.status();
            let headers = response.headers().clone();
            if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
                let wait = finance_retry_seconds(&headers);
                let body = self.read_body_tracked(response).await.unwrap_or_default();
                if attempt == 4 {
                    anyhow::bail!(
                        "WB Finance API rate limit after {attempt} attempts: {url}; response={body}"
                    );
                }
                tracing::warn!(url, attempt, wait, "WB Finance API rate limit");
                let gate = wb_finance_v1_gate(&connection.to_string_id());
                let mut next_allowed = gate.lock().await;
                *next_allowed = Some(
                    std::time::Instant::now()
                        + std::time::Duration::from_secs(wait.max(WB_FINANCE_V1_MIN_INTERVAL_SECS)),
                );
                continue;
            }
            if status == reqwest::StatusCode::NO_CONTENT {
                self.record_http_response_body(0);
                return Ok((status, String::new()));
            }
            let body = self.read_body_tracked(response).await?;
            if !status.is_success() {
                anyhow::bail!("WB Finance API returned {status} for {url}: {body}");
            }
            return Ok((status, body));
        }
        unreachable!("bounded retry loop returns")
    }

    /// Новый Finance API: список ежедневных отчётов и полная детализация каждого reportId.
    pub async fn fetch_finance_reports_v1(
        &self,
        connection: &ConnectionMP,
        date_from: chrono::NaiveDate,
        date_to: chrono::NaiveDate,
    ) -> Result<Vec<WbFinanceV1FetchedReport>> {
        let minimum = chrono::NaiveDate::from_ymd_opt(2025, 1, 1).expect("valid date");
        if date_from < minimum {
            anyhow::bail!("WB Finance API v1 supports dates starting from 2025-01-01");
        }
        if date_to < date_from {
            anyhow::bail!("date_to must not be earlier than date_from");
        }

        const LIST_URL: &str =
            "https://finance-api.wildberries.ru/api/finance/v1/sales-reports/list";
        const DETAIL_BASE: &str =
            "https://finance-api.wildberries.ru/api/finance/v1/sales-reports/detailed";
        let mut headers = Vec::<serde_json::Value>::new();
        let mut offset = 0usize;
        loop {
            let payload = serde_json::json!({
                "dateFrom": date_from.format("%Y-%m-%d").to_string(),
                "dateTo": date_to.format("%Y-%m-%d").to_string(),
                "limit": 1000,
                "offset": offset,
                "period": "daily"
            });
            let (status, body) = self.post_finance_v1(connection, LIST_URL, &payload).await?;
            if status == reqwest::StatusCode::NO_CONTENT {
                break;
            }
            let page: Vec<serde_json::Value> =
                serde_json::from_str(&body).context("Invalid WB Finance report list response")?;
            let count = page.len();
            headers.extend(page);
            if count < 1000 {
                break;
            }
            offset += count;
        }

        let mut reports = Vec::with_capacity(headers.len());
        for header in headers {
            let report_id = json_scalar_text(header.get("reportId")).ok_or_else(|| {
                anyhow::anyhow!("WB Finance report header has no reportId: {header}")
            })?;
            let url = format!("{DETAIL_BASE}/{report_id}");
            let mut rrd_id = "0".to_string();
            let mut last_rrd_id = None;
            let mut pages_count = 0i32;
            let mut lines = Vec::new();
            loop {
                let cursor_number = rrd_id
                    .parse::<u64>()
                    .with_context(|| format!("Invalid WB Finance rrdId cursor: {rrd_id}"))?;
                let payload = serde_json::json!({ "limit": 100000, "rrdId": cursor_number });
                let (status, body) = self.post_finance_v1(connection, &url, &payload).await?;
                if status == reqwest::StatusCode::NO_CONTENT {
                    break;
                }
                let page: Vec<serde_json::Value> =
                    serde_json::from_str(&body).with_context(|| {
                        format!("Invalid WB Finance detail response for report {report_id}")
                    })?;
                if page.is_empty() {
                    anyhow::bail!(
                        "WB Finance detail returned 200 with an empty page for report {report_id}"
                    );
                }
                let next = json_scalar_text(page.last().and_then(|v| v.get("rrdId"))).ok_or_else(
                    || {
                        anyhow::anyhow!(
                            "Last WB Finance detail row has no rrdId for report {report_id}"
                        )
                    },
                )?;
                advancing_rrd_id(&rrd_id, &next).with_context(|| {
                    format!("Invalid detail cursor sequence for report {report_id}")
                })?;
                pages_count += 1;
                rrd_id = next.clone();
                last_rrd_id = Some(next);
                lines.extend(page);
            }
            reports.push(WbFinanceV1FetchedReport {
                header,
                report_id,
                lines,
                pages_count,
                last_rrd_id,
            });
        }
        Ok(reports)
    }
}

#[cfg(test)]
mod finance_v1_contract_tests {
    use super::*;

    #[test]
    fn scalar_ids_are_never_converted_through_f64() {
        let numeric: serde_json::Value = serde_json::from_str("90071992547409930").unwrap();
        assert_eq!(
            json_scalar_text(Some(&numeric)).as_deref(),
            Some("90071992547409930")
        );
        assert_eq!(
            json_scalar_text(Some(&serde_json::json!("90071992547409931"))).as_deref(),
            Some("90071992547409931")
        );
    }

    #[test]
    fn repeated_or_decreasing_cursor_is_rejected() {
        assert!(advancing_rrd_id("10", "10").is_err());
        assert!(advancing_rrd_id("10", "9").is_err());
        assert_eq!(advancing_rrd_id("10", "11").unwrap(), 11);
    }
}

#[cfg(test)]
mod orders_period_tests {
    use super::*;
    use chrono::{NaiveDate, TimeZone, Utc};

    fn day(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    /// Реальный случай: запросили август 2025, WB отдал 10 248 строк начиная с 08.02.2026,
    /// soft-stop отсеял всё. Раньше это тихо завершалось нулём — теперь внятная ошибка.
    #[test]
    fn reports_boundary_when_whole_response_is_outside_window() {
        let earliest = Utc.with_ymd_and_hms(2026, 2, 7, 21, 43, 45).unwrap();
        let message = unavailable_orders_period_message(
            day(2025, 8, 1),
            day(2025, 8, 31),
            0,
            10_248,
            Some(earliest),
        )
        .expect("ожидали сообщение о недоступном периоде");
        // Границу показываем в МСК — как её видит продавец в личном кабинете WB.
        assert!(message.contains("2026-02-08T00:43:45"), "{message}");
        assert!(message.contains("10248"), "{message}");
    }

    #[test]
    fn stays_silent_when_rows_were_kept_or_wb_returned_nothing() {
        let earliest = Utc.with_ymd_and_hms(2026, 7, 1, 0, 0, 0).unwrap();
        // Что-то попало в окно — штатный импорт.
        assert!(unavailable_orders_period_message(
            day(2026, 7, 1),
            day(2026, 7, 31),
            42,
            100,
            Some(earliest)
        )
        .is_none());
        // WB вернул пусто — это честное «заказов за период не было», не ошибка.
        assert!(
            unavailable_orders_period_message(day(2026, 7, 1), day(2026, 7, 31), 0, 0, None)
                .is_none()
        );
    }
}

#[cfg(test)]
mod sales_funnel_detail_report_tests {
    use super::*;
    use zip::write::SimpleFileOptions;

    fn zip_with_csv(csv: &str) -> Vec<u8> {
        let cursor = Cursor::new(Vec::new());
        let mut writer = zip::ZipWriter::new(cursor);
        writer
            .start_file("detail.csv", SimpleFileOptions::default())
            .unwrap();
        writer.write_all(csv.as_bytes()).unwrap();
        writer.finish().unwrap().into_inner()
    }

    #[test]
    fn parses_detail_history_report_zip() {
        let csv = concat!(
            "nmID,dt,openCardCount,addToCartCount,ordersCount,ordersSumRub,",
            "buyoutsCount,buyoutsSumRub,cancelCount,cancelSumRub,",
            "addToCartConversion,cartToOrderConversion,buyoutPercent,addToWishlist,currency\n",
            "70027655,2026-06-01,10,4,2,1234.5,1,617.25,1,617.25,40,50,50,3,RUB\n"
        );
        let rows = parse_sales_funnel_detail_zip(&zip_with_csv(csv)).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].nm_id, 70027655);
        assert_eq!(rows[0].date, "2026-06-01");
        assert_eq!(rows[0].open_count, 10);
        assert_eq!(rows[0].cart_count, 4);
        assert_eq!(rows[0].add_to_wishlist_count, 3);
        assert_eq!(rows[0].currency, "RUB");
    }

    #[test]
    fn rejects_detail_history_report_with_missing_headers() {
        let error = parse_sales_funnel_detail_zip(&zip_with_csv("nmID,dt\n1,2026-06-01\n"))
            .unwrap_err()
            .to_string();
        assert!(error.contains("misses required headers"));
        assert!(error.contains("openCardCount"));
    }
}

#[cfg(test)]
mod search_analytics_tests {
    use super::*;

    #[test]
    fn parses_search_details_product_from_current_wb_shape() {
        let item = serde_json::json!({
            "nmId": 268913787,
            "name": "Test product",
            "vendorCode": "SKU-1",
            "subjectName": "Subject",
            "brandName": "Brand",
            "avgPosition": { "current": 12.5, "dynamics": 1 },
            "openCard": { "current": 42, "dynamics": 2 },
            "addToCart": { "current": 7, "dynamics": 0 },
            "orders": { "current": 3, "dynamics": 0 },
            "visibility": { "current": 81.2, "dynamics": 4 }
        });

        let row = parse_search_report_row(&item).expect("product must be parsed");
        assert_eq!(row.nm_id, 268913787);
        assert_eq!(row.title, "Test product");
        assert_eq!(row.open_card, 42);
        assert_eq!(row.add_to_cart, 7);
        assert_eq!(row.orders, 3);
        assert_eq!(row.avg_position, 12.5);
        assert_eq!(row.visibility, 81.2);
        assert_eq!(row.impressions, 0);
    }

    #[test]
    fn parses_flat_search_text_item() {
        let item = serde_json::json!({
            "text": "test query",
            "nmId": 211131895,
            "openCard": { "current": 9 },
            "orders": { "current": 2 },
            "avgPosition": { "current": 6.5 }
        });

        let row = parse_search_query_row(211131895, &item);
        assert_eq!(row.nm_id, 211131895);
        assert_eq!(row.text, "test query");
        assert_eq!(row.clicks, 9);
        assert_eq!(row.orders, 2);
        assert_eq!(row.avg_position, 6.5);
    }
}
