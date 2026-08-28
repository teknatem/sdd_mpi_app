use anyhow::Result;
use contracts::domain::a006_connection_mp::aggregate::{AuthorizationType, ConnectionMP};
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT};
use reqwest::RequestBuilder;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io::Write;
use std::sync::{Arc, Mutex, OnceLock};

const YANDEX_LOG_PATH: &str = "yandex_api_requests.log";
const YANDEX_LOG_BACKUP_PATH: &str = "yandex_api_requests.log.1";
const YANDEX_LOG_MAX_BYTES: u64 = 10 * 1024 * 1024;
const YANDEX_LOG_MAX_ENTRY_CHARS: usize = 4_096;
static YANDEX_LOG_LOCK: Mutex<()> = Mutex::new(());

type YmReportGate = Arc<tokio::sync::Mutex<std::time::Instant>>;
static YM_REPORT_GATES: OnceLock<Mutex<HashMap<String, YmReportGate>>> = OnceLock::new();

fn ym_report_gate(key: &str) -> YmReportGate {
    let gates = YM_REPORT_GATES.get_or_init(|| Mutex::new(HashMap::new()));
    let mut gates = gates.lock().expect("YM report rate gate poisoned");
    gates
        .entry(key.to_string())
        .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(std::time::Instant::now())))
        .clone()
}

fn ym_header_wait(headers: &HeaderMap) -> Option<std::time::Duration> {
    if let Some(seconds) = headers
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
    {
        return Some(std::time::Duration::from_secs(seconds.max(1)));
    }
    headers
        .get("X-RateLimit-Resource-Until")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| chrono::DateTime::parse_from_rfc2822(v).ok())
        .map(|until| {
            let seconds = (until.with_timezone(&chrono::Utc) - chrono::Utc::now())
                .num_seconds()
                .max(1) as u64;
            std::time::Duration::from_secs(seconds)
        })
}

fn write_yandex_log(message: &str) {
    let Ok(_guard) = YANDEX_LOG_LOCK.lock() else {
        return;
    };

    if let Ok(metadata) = std::fs::metadata(YANDEX_LOG_PATH) {
        if metadata.len() >= YANDEX_LOG_MAX_BYTES {
            let _ = std::fs::remove_file(YANDEX_LOG_BACKUP_PATH);
            if metadata.len() >= YANDEX_LOG_MAX_BYTES * 2 {
                // Do not retain a legacy multi-gigabyte log as the backup.
                let _ = OpenOptions::new()
                    .write(true)
                    .truncate(true)
                    .open(YANDEX_LOG_PATH);
            } else {
                if std::fs::rename(YANDEX_LOG_PATH, YANDEX_LOG_BACKUP_PATH).is_err() {
                    let _ = OpenOptions::new()
                        .write(true)
                        .truncate(true)
                        .open(YANDEX_LOG_PATH);
                }
            }
        }
    }

    let mut chars = message.chars();
    let mut bounded: String = chars.by_ref().take(YANDEX_LOG_MAX_ENTRY_CHARS).collect();
    if chars.next().is_some() {
        bounded.push_str("… [truncated]");
    }

    if let Ok(mut file) = OpenOptions::new()
        .create(true)
        .append(true)
        .open(YANDEX_LOG_PATH)
    {
        let timestamp = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S%.3f");
        let _ = writeln!(file, "[{}] {}", timestamp, bounded);
    }
}

/// HTTP-клиент для работы с Yandex Market API
pub struct YandexApiClient {
    client: reqwest::Client,
}

/// Параметры отчёта «Аналитика продаж» кабинета.
///
/// Живой Partner API отклоняет запрос, если одновременно передать `businessId`
/// и `campaignId` (`Both businessId and campaignId are specified`). Здесь передаётся
/// один campaignId бизнеса как рабочий контекст; сам отчёт содержит данные кабинета.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GenerateShowsSalesRequest {
    campaign_id: i64,
    date_from: String,
    date_to: String,
    grouping: &'static str,
}

impl YandexApiClient {
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
                .build()
                .expect("Failed to create HTTP client"),
        }
    }

    /// Записать в лог-файл
    fn log_to_file(&self, message: &str) {
        write_yandex_log(message);
    }

    fn apply_auth(
        &self,
        request: RequestBuilder,
        connection: &ConnectionMP,
    ) -> Result<RequestBuilder> {
        let token = connection.api_key.trim();
        if token.is_empty() {
            anyhow::bail!("Yandex Market API token is required");
        }

        match &connection.authorization_type {
            AuthorizationType::ApiKey => Ok(request.header("Api-Key", token)),
            AuthorizationType::OAuth2 => {
                Ok(request.header("Authorization", format!("Bearer {}", token)))
            }
            AuthorizationType::BasicAuth => {
                anyhow::bail!("Basic Auth is not supported for Yandex Market API")
            }
        }
    }

    fn auth_log_label(&self, connection: &ConnectionMP) -> &'static str {
        match &connection.authorization_type {
            AuthorizationType::ApiKey => "Api-Key: ****",
            AuthorizationType::OAuth2 => "Authorization: Bearer ****",
            AuthorizationType::BasicAuth => "Basic Auth: unsupported",
        }
    }

    async fn generate_report_request(
        &self,
        connection: &ConnectionMP,
        resource: &str,
        url: &str,
        body: serde_json::Value,
        safe_interval_secs: u64,
    ) -> Result<String> {
        let business = connection
            .business_account_id
            .as_deref()
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| connection.base.code.as_str());
        let gate = ym_report_gate(&format!("{business}:{resource}"));
        for attempt in 1..=5 {
            let mut next_allowed = gate.lock().await;
            let now = std::time::Instant::now();
            if *next_allowed > now {
                tokio::time::sleep(*next_allowed - now).await;
            }
            let request = self
                .client
                .post(url)
                .header("Content-Type", "application/json")
                .json(&body);
            let response = self
                .apply_auth(request, connection)?
                .send()
                .await
                .map_err(|e| {
                    anyhow::anyhow!("Failed to request {resource} report generation: {e}")
                })?;
            let status = response.status();
            let headers = response.headers().clone();
            let remaining = headers
                .get("X-RateLimit-Resource-Remaining")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse::<u64>().ok());
            let adaptive_wait = ym_header_wait(&headers);
            let body = response.text().await.unwrap_or_default();

            if status.is_success() {
                *next_allowed = if remaining.is_some_and(|v| v > 0) {
                    std::time::Instant::now()
                } else {
                    std::time::Instant::now()
                        + adaptive_wait
                            .unwrap_or_else(|| std::time::Duration::from_secs(safe_interval_secs))
                };
                return Ok(body);
            }

            let rate_limited = status.as_u16() == 420
                || status == reqwest::StatusCode::TOO_MANY_REQUESTS
                || body.to_ascii_lowercase().contains("rate limit");
            if rate_limited {
                let wait = adaptive_wait
                    .unwrap_or_else(|| std::time::Duration::from_secs(safe_interval_secs));
                *next_allowed = std::time::Instant::now() + wait;
                self.log_to_file(&format!(
                    "{resource} rate-limited; retry in {}s ({attempt}/5)",
                    wait.as_secs()
                ));
                if attempt < 5 {
                    drop(next_allowed);
                    continue;
                }
            }
            anyhow::bail!("{resource} generation failed with status {status}: {body}");
        }
        unreachable!("bounded retry loop returns")
    }

    /// Получить список товаров через Yandex Market API
    /// Endpoint: POST /v2/businesses/{businessId}/offer-mappings
    pub async fn fetch_product_list(
        &self,
        connection: &ConnectionMP,
        limit: i32,
        page_token: Option<String>,
    ) -> Result<YandexProductListResponse> {
        // Проверка обязательных полей для Yandex API
        let business_id = connection.business_account_id.as_ref().ok_or_else(|| {
            anyhow::anyhow!("Business ID (БизнесАккаунтID) is required for Yandex Market API")
        })?;

        if connection.api_key.trim().is_empty() {
            anyhow::bail!("Yandex Market API token is required");
        }

        let url = format!(
            "https://api.partner.market.yandex.ru/v2/businesses/{}/offer-mappings",
            business_id
        );

        // Отправляем пагинацию через query-параметры, так как тело может игнорироваться API для этого эндпоинта
        #[derive(Serialize)]
        struct YandexListQueryParams {
            pub limit: i32,
            #[serde(skip_serializing_if = "Option::is_none", rename = "pageToken")]
            pub page_token: Option<String>,
        }

        let request_query = YandexListQueryParams {
            limit,
            page_token: page_token.clone(),
        };

        let token_preview = request_query
            .page_token
            .as_ref()
            .map(|t| &t[..t.len().min(50)])
            .map(|s| s.to_string());

        self.log_to_file(&format!(
            "=== REQUEST ===\nPOST {}\n{}\nQuery: limit={}, pageToken={:?}",
            url,
            self.auth_log_label(connection),
            request_query.limit,
            token_preview
        ));

        let request = self
            .client
            .post(&url)
            .header("Content-Type", "application/json")
            .query(&request_query);

        let response = match self.apply_auth(request, connection)?.send().await {
            Ok(resp) => resp,
            Err(e) => {
                let error_msg = format!("HTTP request failed: {:?}", e);
                self.log_to_file(&error_msg);
                tracing::error!("Yandex Market API connection error: {}", e);

                // Проверяем конкретные типы ошибок
                if e.is_timeout() {
                    anyhow::bail!("Request timeout: API не ответил в течение 60 секунд");
                } else if e.is_connect() {
                    anyhow::bail!("Connection error: не удалось подключиться к серверу Yandex Market. Проверьте интернет-соединение.");
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
            let body = response.text().await.unwrap_or_default();
            self.log_to_file(&format!("ERROR Response body:\n{}", body));
            tracing::error!("Yandex Market API request failed: {}", body);
            anyhow::bail!(
                "Yandex Market API request failed with status {}: {}",
                status,
                body
            );
        }

        let body = response.text().await?;
        self.log_to_file(&format!("Response received: {} bytes", body.len()));

        let preview: String = body.chars().take(500).collect::<String>();
        let preview = if preview.len() < body.len() {
            format!("{}...", preview)
        } else {
            preview
        };
        tracing::debug!("Yandex Market API response preview: {}", preview);

        match serde_json::from_str::<YandexProductListResponse>(&body) {
            Ok(data) => {
                self.log_to_file(&format!(
                    "Successfully parsed JSON. Items: {}, nextPageToken: {:?}, total: {:?}",
                    data.result.offer_mapping_entries.len(),
                    data.result.paging.next_page_token,
                    data.result.paging.total
                ));
                tracing::info!(
                    "Yandex API response: {} items, nextPageToken: {:?}, total: {:?}",
                    data.result.offer_mapping_entries.len(),
                    data.result.paging.next_page_token,
                    data.result.paging.total
                );
                Ok(data)
            }
            Err(e) => {
                let error_msg = format!("Failed to parse Yandex Market API JSON: {}", e);
                self.log_to_file(&error_msg);
                tracing::error!("Failed to parse Yandex Market API response. Error: {}", e);
                tracing::error!("Response body: {}", body);
                anyhow::bail!(
                    "Failed to parse Yandex Market API JSON: {}. Response: {}",
                    e,
                    preview
                )
            }
        }
    }

    /// Получить детальную информацию о товарах через Yandex Market API
    /// Endpoint: POST /v2/businesses/{businessId}/offer-cards
    pub async fn fetch_product_info(
        &self,
        connection: &ConnectionMP,
        offer_ids: Vec<String>,
    ) -> Result<YandexProductInfoResponse> {
        let business_id = connection.business_account_id.as_ref().ok_or_else(|| {
            anyhow::anyhow!("Business ID (БизнесАккаунтID) is required for Yandex Market API")
        })?;

        if connection.api_key.trim().is_empty() {
            anyhow::bail!("Yandex Market API token is required");
        }

        let url = format!(
            "https://api.partner.market.yandex.ru/v2/businesses/{}/offer-cards",
            business_id
        );

        let request_body = YandexProductInfoRequest { offer_ids };

        let body = serde_json::to_string(&request_body)?;
        self.log_to_file(&format!(
            "=== REQUEST ===\nPOST {}\n{}\nBody: {}",
            url,
            self.auth_log_label(connection),
            body
        ));

        let request = self
            .client
            .post(&url)
            .header("Content-Type", "application/json")
            .body(body);

        let response = match self.apply_auth(request, connection)?.send().await {
            Ok(resp) => resp,
            Err(e) => {
                let error_msg = format!("HTTP request failed: {:?}", e);
                self.log_to_file(&error_msg);
                tracing::error!("Yandex Market API connection error: {}", e);

                // Проверяем конкретные типы ошибок
                if e.is_timeout() {
                    anyhow::bail!("Request timeout: API не ответил в течение 60 секунд");
                } else if e.is_connect() {
                    anyhow::bail!("Connection error: не удалось подключиться к серверу Yandex Market. Проверьте интернет-соединение.");
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
            let body = response.text().await.unwrap_or_default();
            self.log_to_file(&format!("ERROR Response body:\n{}", body));
            tracing::error!("Yandex Market API request failed: {}", body);
            anyhow::bail!(
                "Yandex Market API request failed with status {}: {}",
                status,
                body
            );
        }

        let body = response.text().await?;
        self.log_to_file(&format!("Response received: {} bytes", body.len()));

        match serde_json::from_str::<YandexProductInfoResponse>(&body) {
            Ok(data) => {
                self.log_to_file("Successfully parsed JSON");
                Ok(data)
            }
            Err(e) => {
                let error_msg = format!("Failed to parse Yandex Market API JSON: {}", e);
                self.log_to_file(&error_msg);
                tracing::error!("Failed to parse Yandex Market API response. Error: {}", e);
                anyhow::bail!("Failed to parse Yandex Market API JSON: {}", e)
            }
        }
    }
}

impl Default for YandexApiClient {
    fn default() -> Self {
        Self::new()
    }
}

/// Декодирует байты CSV-отчёта YM. Если это валидный UTF-8 (с BOM или без) —
/// используем как есть (netting-отчёт). Иначе декодируем как Windows-1251
/// (отчёт о реализации приходит в CP1251).
fn decode_report_csv(raw_bytes: &[u8]) -> String {
    let body = raw_bytes
        .strip_prefix(&[0xEF, 0xBB, 0xBF])
        .unwrap_or(raw_bytes);
    match std::str::from_utf8(body) {
        Ok(text) => text.to_string(),
        Err(_) => {
            let (decoded, _, _) = encoding_rs::WINDOWS_1251.decode(body);
            decoded.into_owned()
        }
    }
}

// ============================================================================
// Request/Response structures для Yandex Market API
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YandexProductListRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "pageToken")]
    pub page_token: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YandexProductListResponse {
    pub result: YandexProductListResult,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YandexProductListResult {
    #[serde(rename = "offerMappings")]
    pub offer_mapping_entries: Vec<YandexOfferMappingEntry>,
    pub paging: YandexPaging,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YandexOfferMappingEntry {
    pub offer: YandexOffer,
    #[serde(default)]
    pub mapping: Option<YandexMapping>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YandexOffer {
    #[serde(rename = "offerId")]
    pub offer_id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub pictures: Vec<String>,
    #[serde(default)]
    pub barcodes: Vec<String>,
    #[serde(default)]
    pub vendor: Option<String>,
    #[serde(flatten)]
    pub extra: std::collections::HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YandexMapping {
    #[serde(rename = "marketSku")]
    pub market_sku: Option<i64>,
    #[serde(rename = "marketSkuName", default)]
    pub market_sku_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YandexPaging {
    #[serde(rename = "nextPageToken", skip_serializing_if = "Option::is_none")]
    pub next_page_token: Option<String>,
    /// Общее количество элементов (если API возвращает)
    #[serde(default)]
    pub total: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YandexProductInfoRequest {
    #[serde(rename = "offerIds")]
    pub offer_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YandexProductInfoResponse {
    pub result: YandexProductInfoResult,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YandexProductInfoResult {
    #[serde(rename = "offerMappings")]
    pub offer_mappings: Vec<YandexOfferMapping>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YandexOfferMapping {
    pub offer: YandexOfferCard,
    #[serde(default)]
    pub mapping: Option<YandexOfferCardMapping>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YandexOfferCard {
    #[serde(rename = "offerId")]
    pub offer_id: String,
    #[serde(default, rename = "parameterValues")]
    pub parameter_values: Vec<YandexParameterValue>,
    #[serde(default)]
    pub barcodes: Vec<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub pictures: Vec<String>,
    #[serde(default)]
    pub price: Option<YandexPrice>,
    #[serde(default)]
    pub vendor: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(flatten)]
    pub extra: std::collections::HashMap<String, serde_json::Value>, // Любые дополнительные поля
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YandexOfferCardMapping {
    #[serde(rename = "marketSku")]
    pub market_sku: Option<i64>,
    #[serde(rename = "marketSkuName")]
    pub market_sku_name: Option<String>,
    #[serde(rename = "marketCategoryId")]
    pub market_category_id: Option<i64>,
    #[serde(rename = "marketCategoryName")]
    pub market_category_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YandexParameterValue {
    #[serde(rename = "parameterId")]
    pub parameter_id: i64,
    #[serde(default)]
    pub value: Option<String>,
    #[serde(default, rename = "valueId")]
    pub value_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YandexCategory {
    pub id: i64,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YandexPrice {
    pub value: f64,
    pub currency: String,
}

/// Поле даты, по которому YM фильтрует заказы в `GET /campaigns/{id}/orders`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderDateField {
    /// `fromDate`/`toDate` — фильтр по дате оформления (создания) заказа.
    /// Подходит для полного бэкфилла за период.
    Creation,
    /// `updatedAtFrom`/`updatedAtTo` — фильтр по дате/времени обновления заказа
    /// (смена статуса). Подходит для инкрементальной синхронизации статусов:
    /// ловит и новые заказы, и изменения по ранее созданным.
    Updated,
}

/// Разбивает период `[from, to]` (включительно) на под-интервалы длиной не более
/// `max_days` календарных дней. YM ограничивает диапазон фильтра заказов 30 днями,
/// поэтому длинные периоды (бэкфилл на 60–90 дней) нужно резать на под-окна.
fn split_date_range(
    from: chrono::NaiveDate,
    to: chrono::NaiveDate,
    max_days: i64,
) -> Vec<(chrono::NaiveDate, chrono::NaiveDate)> {
    let mut out = Vec::new();
    if to < from {
        return out;
    }
    let mut start = from;
    while start <= to {
        let end = std::cmp::min(to, start + chrono::Duration::days(max_days - 1));
        out.push((start, end));
        start = end + chrono::Duration::days(1);
    }
    out
}

impl YandexApiClient {
    /// Получить список заказов через Yandex Market API с пагинацией.
    /// GET /campaigns/{campaignId}/orders
    ///
    /// `date_field` выбирает поле фильтрации:
    /// - `Creation` → `fromDate`/`toDate` (дата оформления);
    /// - `Updated`  → `updatedAtFrom`/`updatedAtTo` (дата обновления/смены статуса).
    ///
    /// Период автоматически режется на под-окна ≤ 30 дней (лимит YM); заказы
    /// дедуплицируются по `id` (последняя версия побеждает).
    pub async fn fetch_orders(
        &self,
        connection: &ConnectionMP,
        date_from: chrono::NaiveDate,
        date_to: chrono::NaiveDate,
        date_field: OrderDateField,
    ) -> Result<Vec<YmOrderItem>> {
        const MAX_WINDOW_DAYS: i64 = 30;
        let page_size = 50;

        // Сохраняем порядок первого появления id, но всегда держим последнюю версию.
        let mut by_id: std::collections::HashMap<i64, YmOrderItem> =
            std::collections::HashMap::new();
        let mut id_order: Vec<i64> = Vec::new();

        for (win_from, win_to) in split_date_range(date_from, date_to, MAX_WINDOW_DAYS) {
            let mut page = 1;
            loop {
                let response = self
                    .fetch_orders_page(connection, win_from, win_to, date_field, page, page_size)
                    .await?;

                let orders_count = response.orders.len();
                for o in response.orders {
                    if !by_id.contains_key(&o.id) {
                        id_order.push(o.id);
                    }
                    by_id.insert(o.id, o);
                }

                self.log_to_file(&format!(
                    "Fetched page {} [{}..{}] with {} orders (unique so far: {})",
                    page,
                    win_from,
                    win_to,
                    orders_count,
                    by_id.len()
                ));

                // Check if there are more pages
                if let Some(pager) = response.pager {
                    if let Some(pages_count) = pager.pages_count {
                        if page >= pages_count {
                            break;
                        }
                    }
                }

                // Stop if we got less than page_size orders (last page)
                if orders_count < page_size as usize {
                    break;
                }

                page += 1;

                // Safety limit to prevent infinite loops
                if page > 100 {
                    tracing::warn!("Reached maximum page limit (100), stopping pagination");
                    break;
                }
            }
        }

        let all_orders: Vec<YmOrderItem> = id_order
            .into_iter()
            .filter_map(|id| by_id.remove(&id))
            .collect();
        self.log_to_file(&format!("Total orders fetched: {}", all_orders.len()));
        Ok(all_orders)
    }

    /// Получить одну страницу заказов
    async fn fetch_orders_page(
        &self,
        connection: &ConnectionMP,
        date_from: chrono::NaiveDate,
        date_to: chrono::NaiveDate,
        date_field: OrderDateField,
        page: i32,
        page_size: i32,
    ) -> Result<YmOrdersResponse> {
        let campaign_id = connection.supplier_id.as_ref().ok_or_else(|| {
            anyhow::anyhow!(
                "Campaign ID (Идентификатор магазина) is required for Yandex Market API"
            )
        })?;

        if connection.api_key.trim().is_empty() {
            anyhow::bail!("Yandex Market API token is required");
        }

        let url = format!(
            "https://api.partner.market.yandex.ru/campaigns/{}/orders",
            campaign_id
        );

        // Параметры даты зависят от выбранного поля фильтрации.
        // Creation: fromDate/toDate в формате DD-MM-YYYY.
        // Updated:  updatedAtFrom/updatedAtTo в ISO 8601 со смещением МСК (+03:00),
        //           по границам суток окна.
        let mut query: Vec<(&str, String)> = match date_field {
            OrderDateField::Creation => vec![
                ("fromDate", date_from.format("%d-%m-%Y").to_string()),
                ("toDate", date_to.format("%d-%m-%Y").to_string()),
            ],
            OrderDateField::Updated => vec![
                (
                    "updatedAtFrom",
                    format!("{}T00:00:00+03:00", date_from.format("%Y-%m-%d")),
                ),
                (
                    "updatedAtTo",
                    format!("{}T23:59:59+03:00", date_to.format("%Y-%m-%d")),
                ),
            ],
        };
        query.push(("page", page.to_string()));
        query.push(("pageSize", page_size.to_string()));

        self.log_to_file(&format!(
            "=== REQUEST PAGE {} ({:?}) ===\nGET {}\n{}\nQuery: {:?}",
            page,
            date_field,
            url,
            self.auth_log_label(connection),
            query
        ));

        let request = self.client.get(&url).query(&query);

        let response = match self.apply_auth(request, connection)?.send().await {
            Ok(resp) => resp,
            Err(e) => {
                let error_msg = format!("HTTP request failed: {:?}", e);
                self.log_to_file(&error_msg);
                tracing::error!("Yandex Market Orders API connection error: {}", e);

                // Проверяем конкретные типы ошибок
                if e.is_timeout() {
                    anyhow::bail!("Request timeout: API не ответил в течение 60 секунд");
                } else if e.is_connect() {
                    anyhow::bail!("Connection error: не удалось подключиться к серверу Yandex Market. Проверьте интернет-соединение.");
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
            let body = response.text().await.unwrap_or_default();
            self.log_to_file(&format!("ERROR Response body:\n{}", body));
            tracing::error!("Yandex Market Orders API request failed: {}", body);
            anyhow::bail!(
                "Yandex Market Orders API failed with status {}: {}",
                status,
                body
            );
        }

        let body = response.text().await?;
        self.log_to_file(&format!("Response received: {} bytes", body.len()));

        match serde_json::from_str::<YmOrdersResponse>(&body) {
            Ok(data) => {
                let orders_count = data.orders.len();
                self.log_to_file(&format!("Successfully parsed {} orders", orders_count));
                Ok(data)
            }
            Err(e) => {
                self.log_to_file(&format!("Failed to parse JSON: {}", e));
                tracing::error!("Failed to parse Yandex Market orders response: {}", e);
                anyhow::bail!("Failed to parse orders response: {}", e)
            }
        }
    }

    /// Получить детали конкретного заказа
    /// GET /campaigns/{campaignId}/orders/{orderId}
    pub async fn fetch_order_details(
        &self,
        connection: &ConnectionMP,
        order_id: i64,
    ) -> Result<YmOrderItem> {
        let campaign_id = connection.supplier_id.as_ref().ok_or_else(|| {
            anyhow::anyhow!(
                "Campaign ID (Идентификатор магазина) is required for Yandex Market API"
            )
        })?;

        if connection.api_key.trim().is_empty() {
            anyhow::bail!("Yandex Market API token is required");
        }

        let url = format!(
            "https://api.partner.market.yandex.ru/campaigns/{}/orders/{}",
            campaign_id, order_id
        );

        self.log_to_file(&format!(
            "=== REQUEST ===\nGET {}\n{}",
            url,
            self.auth_log_label(connection)
        ));

        let request = self.client.get(&url);

        let response = match self.apply_auth(request, connection)?.send().await {
            Ok(resp) => resp,
            Err(e) => {
                let error_msg = format!("HTTP request failed: {:?}", e);
                self.log_to_file(&error_msg);
                tracing::error!("Yandex Market Order Details API connection error: {}", e);

                // Проверяем конкретные типы ошибок
                if e.is_timeout() {
                    anyhow::bail!("Request timeout: API не ответил в течение 60 секунд");
                } else if e.is_connect() {
                    anyhow::bail!("Connection error: не удалось подключиться к серверу Yandex Market. Проверьте интернет-соединение.");
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
            let body = response.text().await.unwrap_or_default();
            self.log_to_file(&format!("ERROR Response body:\n{}", body));
            tracing::error!("Yandex Market Order Details API request failed: {}", body);
            anyhow::bail!(
                "Yandex Market Order Details API failed with status {}: {}",
                status,
                body
            );
        }

        let body = response.text().await?;
        self.log_to_file(&format!("Response received: {} bytes", body.len()));

        match serde_json::from_str::<YmOrderDetailsResponse>(&body) {
            Ok(data) => {
                self.log_to_file("Successfully parsed order details");
                Ok(data.order)
            }
            Err(e) => {
                self.log_to_file(&format!("Failed to parse JSON: {}", e));
                tracing::error!(
                    "Failed to parse Yandex Market order details response: {}",
                    e
                );
                anyhow::bail!("Failed to parse order details response: {}", e)
            }
        }
    }

    /// Получить список магазинов (кампаний), доступных по токену подключения.
    /// GET /campaigns
    ///
    /// Для API-Key возвращает магазины кабинета, на который выпущен токен.
    /// Каждая кампания содержит `business.id` — это позволяет отобрать только
    /// магазины нужного бизнеса (двухуровневая модель «бизнес → магазин»).
    pub async fn fetch_campaigns(&self, connection: &ConnectionMP) -> Result<Vec<YmCampaign>> {
        if connection.api_key.trim().is_empty() {
            anyhow::bail!("Yandex Market API token is required");
        }

        let url = "https://api.partner.market.yandex.ru/campaigns";
        let page_size = 50;
        let mut all = Vec::new();
        let mut page = 1;

        loop {
            let query: Vec<(&str, String)> = vec![
                ("page", page.to_string()),
                ("pageSize", page_size.to_string()),
            ];

            self.log_to_file(&format!(
                "=== REQUEST CAMPAIGNS PAGE {} ===\nGET {}\n{}\nQuery: {:?}",
                page,
                url,
                self.auth_log_label(connection),
                query
            ));

            let request = self.client.get(url).query(&query);
            let response = self
                .apply_auth(request, connection)?
                .send()
                .await
                .map_err(|e| anyhow::anyhow!("Failed to fetch campaigns: {}", e))?;

            let status = response.status();
            self.log_to_file(&format!("Response status: {}", status));
            if !status.is_success() {
                let body = response.text().await.unwrap_or_default();
                self.log_to_file(&format!("ERROR Response body:\n{}", body));
                anyhow::bail!(
                    "Yandex Market Campaigns API failed with status {}: {}",
                    status,
                    body
                );
            }

            let body = response.text().await?;
            let parsed: YmCampaignsResponse = serde_json::from_str(&body).map_err(|e| {
                self.log_to_file(&format!("Failed to parse campaigns JSON: {}", e));
                anyhow::anyhow!("Failed to parse campaigns response: {}", e)
            })?;

            let count = parsed.campaigns.len();
            all.extend(parsed.campaigns);

            if let Some(pager) = parsed.pager {
                if let Some(pages_count) = pager.pages_count {
                    if page >= pages_count {
                        break;
                    }
                }
            }
            if count < page_size as usize {
                break;
            }
            page += 1;
            if page > 100 {
                tracing::warn!("Reached maximum campaigns page limit (100), stopping");
                break;
            }
        }

        self.log_to_file(&format!("Total campaigns fetched: {}", all.len()));
        Ok(all)
    }
}

// ============================================================================
// Campaigns structures (GET /campaigns)
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YmCampaignsResponse {
    #[serde(default)]
    pub campaigns: Vec<YmCampaign>,
    #[serde(default)]
    pub pager: Option<YmOrdersPager>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YmCampaign {
    pub id: i64,
    #[serde(default)]
    pub business: Option<YmCampaignBusiness>,
    #[serde(rename = "clientId", default)]
    pub client_id: Option<i64>,
    #[serde(default)]
    pub domain: Option<String>,
    /// Модель работы магазина: FBS / FBY / DBS / LAAS. Используется как
    /// `fulfillment_type` заказа (измерение, заменяющее «магазин» в аналитике YM).
    #[serde(rename = "placementType", default)]
    pub placement_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YmCampaignBusiness {
    pub id: i64,
    #[serde(default)]
    pub name: Option<String>,
}

// ============================================================================
// Orders structures
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YmOrdersResponse {
    pub orders: Vec<YmOrderItem>,
    #[serde(default)]
    pub pager: Option<YmOrdersPager>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YmOrdersPager {
    pub total: Option<i32>,
    pub from: Option<i32>,
    pub to: Option<i32>,
    #[serde(rename = "currentPage")]
    pub current_page: Option<i32>,
    #[serde(rename = "pagesCount")]
    pub pages_count: Option<i32>,
    #[serde(rename = "pageSize")]
    pub page_size: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YmOrdersPaging {
    #[serde(rename = "nextPageToken")]
    pub next_page_token: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YmOrderDetailsResponse {
    pub order: YmOrderItem,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YmOrderItem {
    pub id: i64,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(rename = "substatus", default)]
    pub substatus: Option<String>,
    #[serde(rename = "creationDate", default)]
    pub creation_date: Option<String>,
    #[serde(rename = "statusUpdateDate", default)]
    pub status_update_date: Option<String>,
    #[serde(rename = "deliveryDate", default)]
    pub delivery_date: Option<String>,
    #[serde(default)]
    pub items: Vec<YmOrderLineItem>,
    #[serde(default)]
    pub delivery: Option<YmOrderDelivery>,
    #[serde(default)]
    pub total: Option<f64>,
    #[serde(default)]
    pub currency: Option<String>,
    /// Платеж покупателя (общая стоимость товаров включая НДС, без доставки)
    #[serde(rename = "itemsTotal", default)]
    pub items_total: Option<f64>,
    /// Стоимость доставки
    #[serde(rename = "deliveryTotal", default)]
    pub delivery_total: Option<f64>,
    /// Субсидии от Маркета (вознаграждение продавцу за скидки)
    #[serde(default)]
    pub subsidies: Option<Vec<YmOrderSubsidy>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YmOrderLineItem {
    pub id: i64,
    #[serde(rename = "offerId", default)]
    pub offer_id: Option<String>,
    #[serde(rename = "shopSku", default)]
    pub shop_sku: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub count: i32,
    #[serde(default)]
    pub price: Option<f64>,
    #[serde(default)]
    pub subsidy: Option<f64>,
    #[serde(default)]
    pub total: Option<f64>,
    #[serde(default)]
    pub status: Option<String>,
    /// Цена товара после всех скидок (buyerPrice)
    #[serde(rename = "buyerPrice", default)]
    pub buyer_price: Option<f64>,
    /// Субсидии на уровне товара
    #[serde(default)]
    pub subsidies: Option<Vec<YmOrderItemSubsidy>>,
    /// Детали судьбы позиции (частичные отказы/возвраты).
    /// Присутствует только когда часть/всё количество отклонено или возвращено.
    #[serde(default)]
    pub details: Option<Vec<YmOrderItemDetail>>,
}

/// Деталь по позиции из items[].details
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YmOrderItemDetail {
    /// Количество единиц с данным статусом
    #[serde(rename = "itemCount", default)]
    pub item_count: Option<f64>,
    /// Статус единиц: REJECTED, RETURNED, ...
    #[serde(rename = "itemStatus", default)]
    pub item_status: Option<String>,
    /// Дата обновления статуса (строкой, формат DD-MM-YYYY)
    #[serde(rename = "updateDate", default)]
    pub update_date: Option<String>,
}

/// Субсидия на уровне заказа
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YmOrderSubsidy {
    /// Сумма субсидии
    #[serde(default)]
    pub amount: Option<f64>,
    /// Тип субсидии: YANDEX_CASHBACK, SUBSIDY, DELIVERY
    #[serde(rename = "type", default)]
    pub subsidy_type: Option<String>,
}

/// Субсидия на уровне товара
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YmOrderItemSubsidy {
    /// Сумма субсидии
    #[serde(default)]
    pub amount: Option<f64>,
    /// Тип субсидии
    #[serde(rename = "type", default)]
    pub subsidy_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YmOrderDelivery {
    #[serde(rename = "type", default)]
    pub delivery_type: Option<String>,
    #[serde(rename = "serviceName", default)]
    pub service_name: Option<String>,
    #[serde(default)]
    pub price: Option<f64>,
    #[serde(default)]
    pub dates: Option<YmOrderDeliveryDates>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YmOrderDeliveryDates {
    #[serde(rename = "realDeliveryDate", default)]
    pub real_delivery_date: Option<String>,
    #[serde(rename = "fromDate", default)]
    pub from_date: Option<String>,
    #[serde(rename = "toDate", default)]
    pub to_date: Option<String>,
}

// ============================================================================
// Returns structures (GET /v2/campaigns/{campaignId}/returns)
// ============================================================================

/// Wrapper for the API response (top level with status and result)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YmReturnsApiResponse {
    pub status: String,
    pub result: YmReturnsResult,
}

/// Inner result structure containing returns and paging
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YmReturnsResult {
    #[serde(default)]
    pub returns: Vec<YmReturnItem>,
    #[serde(default)]
    pub paging: Option<YmReturnsPaging>,
}

/// Token-based pagination structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YmReturnsPaging {
    #[serde(rename = "nextPageToken", default)]
    pub next_page_token: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YmReturnItem {
    /// ID возврата
    pub id: i64,
    /// ID заказа
    #[serde(rename = "orderId")]
    pub order_id: i64,
    /// Тип возврата: RETURN или UNREDEEMED
    #[serde(rename = "returnType", default)]
    pub return_type: Option<String>,
    /// Статус возврата денег: REFUNDED, REFUND_IN_PROGRESS, NOT_REFUNDED и т.д.
    #[serde(rename = "refundStatus", default)]
    pub refund_status: Option<String>,
    /// Дата создания возврата
    #[serde(rename = "creationDate", default)]
    pub created_at: Option<String>,
    /// Дата обновления возврата
    #[serde(rename = "updateDate", default)]
    pub updated_at: Option<String>,
    /// Общая сумма возврата
    #[serde(default)]
    pub amount: Option<YmReturnAmount>,
    /// Товары в возврате
    #[serde(default)]
    pub items: Vec<YmReturnItemLine>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YmReturnAmount {
    /// Сумма
    #[serde(default)]
    pub value: Option<f64>,
    /// Валюта
    #[serde(rename = "currencyId", default)]
    pub currency_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YmReturnItemLine {
    /// Market SKU (идентификатор товара в Маркете)
    #[serde(rename = "marketSku", default)]
    pub market_sku: Option<i64>,
    /// offerId (идентификатор товара продавца)
    #[serde(rename = "offerId", default)]
    pub offer_id: Option<String>,
    /// shopSku (артикул продавца)
    #[serde(rename = "shopSku", default)]
    pub shop_sku: Option<String>,
    /// Название товара
    #[serde(rename = "offerName", default)]
    pub offer_name: Option<String>,
    /// Количество
    #[serde(default)]
    pub count: i32,
    /// Цена товара
    #[serde(default)]
    pub price: Option<f64>,
    /// Причина возврата
    #[serde(rename = "returnReason", default)]
    pub return_reason: Option<String>,
    /// Комментарий покупателя
    #[serde(rename = "returnReasonComment", default)]
    pub return_reason_comment: Option<String>,
    /// Решения по возврату
    #[serde(default)]
    pub decisions: Vec<YmReturnDecision>,
    /// Фотографии дефектов
    #[serde(default)]
    pub photos: Vec<YmReturnPhoto>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YmReturnDecision {
    /// Тип решения: REFUND_MONEY, DECLINE_REFUND, REFUND_MONEY_INCLUDING_SHIPMENT и т.д.
    #[serde(rename = "decisionType", default)]
    pub decision_type: Option<String>,
    /// Сумма возврата
    #[serde(default)]
    pub amount: Option<YmReturnAmount>,
    /// Компенсация за обратную доставку
    #[serde(rename = "partnerCompensationAmount", default)]
    pub partner_compensation_amount: Option<YmReturnAmount>,
    /// Комментарий к решению
    #[serde(default)]
    pub comment: Option<String>,
    /// Дата решения
    #[serde(rename = "decisionDate", default)]
    pub decision_date: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YmReturnPhoto {
    /// URL фото
    #[serde(default)]
    pub url: Option<String>,
}

impl YandexApiClient {
    /// Получить список возвратов через Yandex Market API с пагинацией (token-based)
    /// GET /v2/campaigns/{campaignId}/returns
    /// Parameters (подтверждено докой YM):
    /// - date_from: начало периода — фильтр по дате ОБНОВЛЕНИЯ возврата (не создания)
    /// - date_to: конец периода — фильтр по дате ОБНОВЛЕНИЯ возврата
    ///   Поэтому поздние изменения статусов возврата сами попадают в окно загрузки.
    pub async fn fetch_returns(
        &self,
        connection: &ConnectionMP,
        date_from: chrono::NaiveDate,
        date_to: chrono::NaiveDate,
    ) -> Result<Vec<YmReturnItem>> {
        let mut all_returns = Vec::new();
        let mut page_token: Option<String> = None;
        let page_size = 100; // max allowed by Yandex Market API
        let mut page_count = 0;

        loop {
            page_count += 1;
            let response = self
                .fetch_returns_page(
                    connection,
                    date_from,
                    date_to,
                    page_token.clone(),
                    page_size,
                )
                .await?;

            let returns_count = response.returns.len();
            all_returns.extend(response.returns);

            self.log_to_file(&format!(
                "Fetched page {} with {} returns (total so far: {})",
                page_count,
                returns_count,
                all_returns.len()
            ));

            // Check if there are more pages (token-based pagination)
            let next_token = response.paging.and_then(|p| p.next_page_token);

            if next_token.is_none() {
                // No more pages
                break;
            }

            // Stop if we got no returns (last page)
            if returns_count == 0 {
                break;
            }

            page_token = next_token;

            // Safety limit to prevent infinite loops
            if page_count > 100 {
                tracing::warn!("Reached maximum page limit (100), stopping returns pagination");
                break;
            }
        }

        self.log_to_file(&format!("Total returns fetched: {}", all_returns.len()));
        Ok(all_returns)
    }

    /// Получить одну страницу возвратов (token-based pagination)
    async fn fetch_returns_page(
        &self,
        connection: &ConnectionMP,
        date_from: chrono::NaiveDate,
        date_to: chrono::NaiveDate,
        page_token: Option<String>,
        page_size: i32,
    ) -> Result<YmReturnsResult> {
        let campaign_id = connection.supplier_id.as_ref().ok_or_else(|| {
            anyhow::anyhow!(
                "Campaign ID (Идентификатор магазина) is required for Yandex Market API"
            )
        })?;

        if connection.api_key.trim().is_empty() {
            anyhow::bail!("Yandex Market API token is required");
        }

        let url = format!(
            "https://api.partner.market.yandex.ru/v2/campaigns/{}/returns",
            campaign_id
        );

        // Build query parameters
        // Per Yandex Market API: dates are YYYY-MM-DD, page size is `limit`,
        // pagination token is `pageToken` (NOT `page_token`/`pageSize`).
        let mut query_params: Vec<(&str, String)> = vec![
            ("fromDate", date_from.format("%Y-%m-%d").to_string()),
            ("toDate", date_to.format("%Y-%m-%d").to_string()),
            ("limit", page_size.to_string()),
        ];

        if let Some(ref token) = page_token {
            query_params.push(("pageToken", token.clone()));
        }

        self.log_to_file(&format!(
            "=== REQUEST RETURNS PAGE ===\nGET {}\n{}\nQuery: {:?}",
            url,
            self.auth_log_label(connection),
            query_params
        ));

        let request = self.client.get(&url).query(&query_params);

        let response = match self.apply_auth(request, connection)?.send().await {
            Ok(resp) => resp,
            Err(e) => {
                let error_msg = format!("HTTP request failed: {:?}", e);
                self.log_to_file(&error_msg);
                tracing::error!("Yandex Market Returns API connection error: {}", e);

                // Проверяем конкретные типы ошибок
                if e.is_timeout() {
                    anyhow::bail!("Request timeout: API не ответил в течение 60 секунд");
                } else if e.is_connect() {
                    anyhow::bail!("Connection error: не удалось подключиться к серверу Yandex Market. Проверьте интернет-соединение.");
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
            let body = response.text().await.unwrap_or_default();
            self.log_to_file(&format!("ERROR Response body:\n{}", body));
            tracing::error!("Yandex Market Returns API request failed: {}", body);
            anyhow::bail!(
                "Yandex Market Returns API failed with status {}: {}",
                status,
                body
            );
        }

        let body = response.text().await?;
        self.log_to_file(&format!("Returns response received: {} bytes", body.len()));

        match serde_json::from_str::<YmReturnsApiResponse>(&body) {
            Ok(api_response) => {
                let returns_count = api_response.result.returns.len();
                self.log_to_file(&format!("Successfully parsed {} returns", returns_count));
                Ok(api_response.result)
            }
            Err(e) => {
                self.log_to_file(&format!("Failed to parse returns JSON: {}", e));
                tracing::error!("Failed to parse Yandex Market returns response: {}", e);
                anyhow::bail!("Failed to parse returns response: {}", e)
            }
        }
    }

    // =========================================================================
    // Payment Report (united-netting) — two-phase async report generation
    // =========================================================================

    /// Phase 1: Request payment report generation.
    /// Endpoint: POST https://api.partner.market.yandex.ru/v2/reports/united-netting/generate
    /// Returns reportId that can be polled with `poll_report_status`.
    pub async fn generate_payment_report(
        &self,
        connection: &ConnectionMP,
        date_from: chrono::NaiveDate,
        date_to: chrono::NaiveDate,
    ) -> Result<String> {
        // businessId goes in the JSON body; format is a query parameter
        let business_id: i64 = connection
            .business_account_id
            .as_ref()
            .filter(|s| !s.is_empty())
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "businessId (БизнесАккаунтID) is required for YM payment report generation"
                )
            })?
            .parse()
            .map_err(|e| anyhow::anyhow!("businessId must be an integer: {}", e))?;

        let url =
            "https://api.partner.market.yandex.ru/v2/reports/united-netting/generate?format=CSV";

        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct GeneratePaymentReportRequest {
            business_id: i64,
            date_from: String,
            date_to: String,
        }

        let body = GeneratePaymentReportRequest {
            business_id,
            date_from: date_from.format("%Y-%m-%d").to_string(),
            date_to: date_to.format("%Y-%m-%d").to_string(),
        };

        self.log_to_file(&format!(
            "=== GENERATE PAYMENT REPORT ===\nPOST {}\nbusinessId={}, date_from={}, date_to={}",
            url, business_id, date_from, date_to
        ));

        let resp_body = self
            .generate_report_request(
                connection,
                "united-netting",
                url,
                serde_json::to_value(&body)?,
                120,
            )
            .await?;

        let parsed: serde_json::Value = serde_json::from_str(&resp_body)
            .map_err(|e| anyhow::anyhow!("Failed to parse generate response: {}", e))?;

        let report_id = parsed
            .pointer("/result/reportId")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("reportId not found in response: {}", resp_body))?
            .to_string();

        self.log_to_file(&format!("Payment report generated, reportId={}", report_id));
        Ok(report_id)
    }

    /// Phase 1 (отчёт о реализации): запросить генерацию месячного отчёта.
    ///
    /// Отчёт `/reports/goods-realization` — **помесячный, на кампанию**: тело
    /// требует `campaignId` + `year` + `month` (подтверждено ответом YM API).
    /// `campaignId` берётся из `connection.supplier_id` (Идентификатор магазина).
    /// Возвращает reportId для последующего поллинга `poll_report_status`.
    pub async fn generate_realization_report(
        &self,
        connection: &ConnectionMP,
        year: i32,
        month: u32,
    ) -> Result<String> {
        let campaign_id: i64 = connection
            .supplier_id
            .as_ref()
            .filter(|s| !s.is_empty())
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "campaignId (Идентификатор магазина / supplier_id) is required for YM realization report"
                )
            })?
            .parse()
            .map_err(|e| anyhow::anyhow!("campaignId must be an integer: {}", e))?;

        let url =
            "https://api.partner.market.yandex.ru/v2/reports/goods-realization/generate?format=CSV";

        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct GenerateRealizationReportRequest {
            campaign_id: i64,
            year: i32,
            month: u32,
        }

        let body = GenerateRealizationReportRequest {
            campaign_id,
            year,
            month,
        };

        self.log_to_file(&format!(
            "=== GENERATE REALIZATION REPORT ===\nPOST {}\ncampaignId={}, year={}, month={}",
            url, campaign_id, year, month
        ));

        // Официальный базовый лимит — один запрос в две минуты. Он разделяется
        // кампаниями бизнеса, поэтому применяется общий process-wide gate.
        let resp_body = self
            .generate_report_request(
                connection,
                "goods-realization",
                url,
                serde_json::to_value(&body)?,
                120,
            )
            .await?;

        let parsed: serde_json::Value = serde_json::from_str(&resp_body)
            .map_err(|e| anyhow::anyhow!("Failed to parse generate response: {}", e))?;

        let report_id = parsed
            .pointer("/result/reportId")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("reportId not found in response: {}", resp_body))?
            .to_string();

        self.log_to_file(&format!(
            "Realization report generated, reportId={}",
            report_id
        ));
        Ok(report_id)
    }

    /// Генерация отчёта «Аналитика продаж» (shows-sales) — верх воронки YM.
    /// Endpoint: POST /v2/reports/shows-sales/generate?format=JSON
    ///
    /// `grouping=OFFERS` + разбивка по дням даёт строку на `offerId × день`: показы,
    /// клики, корзину, заказы (шт. и сумма), доставки (шт. и сумма), отмены/невыкупы и возвраты.
    /// Это единственный источник показов YM — в orders-API их нет.
    ///
    /// В запрос передаётся один `campaignId` бизнеса. Повторять запрос для остальных
    /// магазинов не нужно: завершённые дни отчёта совпадают на уровне кабинета.
    ///
    /// Глубина истории ограничена тарифом YM: без подписки/Лайт — 90 дней, Медиум — 400.
    pub async fn generate_shows_sales_report(
        &self,
        connection: &ConnectionMP,
        date_from: chrono::NaiveDate,
        date_to: chrono::NaiveDate,
    ) -> Result<String> {
        let campaign_id: i64 = connection
            .supplier_id
            .as_ref()
            .filter(|s| !s.is_empty())
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "campaignId (Идентификатор магазина / supplier_id) is required for YM shows-sales report"
                )
            })?
            .parse()
            .map_err(|e| anyhow::anyhow!("campaignId must be an integer: {}", e))?;

        let url =
            "https://api.partner.market.yandex.ru/v2/reports/shows-sales/generate?format=JSON";

        let body = GenerateShowsSalesRequest {
            campaign_id,
            date_from: date_from.format("%Y-%m-%d").to_string(),
            date_to: date_to.format("%Y-%m-%d").to_string(),
            grouping: "OFFERS",
        };

        self.log_to_file(&format!(
            "=== GENERATE SHOWS-SALES REPORT ===\nPOST {}\ncampaignId={}, date_from={}, date_to={}",
            url, campaign_id, date_from, date_to
        ));

        // Без подписки: один запрос в 10 минут и один одновременно формируемый отчёт.
        let resp_body = self
            .generate_report_request(
                connection,
                "shows-sales",
                url,
                serde_json::to_value(&body)?,
                600,
            )
            .await?;

        let parsed: serde_json::Value = serde_json::from_str(&resp_body)
            .map_err(|e| anyhow::anyhow!("Failed to parse generate response: {}", e))?;

        let report_id = parsed
            .pointer("/result/reportId")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("reportId not found in response: {}", resp_body))?
            .to_string();

        self.log_to_file(&format!(
            "Shows-sales report generated, reportId={}",
            report_id
        ));
        Ok(report_id)
    }

    /// Скачивает готовый отчёт как текст. YM отдаёт JSON-отчёты то напрямую, то
    /// упакованными в ZIP (зависит от размера), поэтому распаковываем по сигнатуре
    /// архива, а не по расширению ссылки.
    pub async fn download_report_text(&self, url: &str, subdir: &str) -> Result<String> {
        let response = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to download report file: {}", e))?;
        let status = response.status();
        if !status.is_success() {
            let err_body = response.text().await.unwrap_or_default();
            anyhow::bail!(
                "download_report_text failed with status {}: {}",
                status,
                err_body
            );
        }
        let bytes = response
            .bytes()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to read report file body: {}", e))?
            .to_vec();

        let subdir = subdir.to_string();
        tokio::task::spawn_blocking(move || -> Result<String> {
            let dir_path = format!("downloads/{}", subdir);
            let dir = std::path::Path::new(&dir_path);
            std::fs::create_dir_all(dir)
                .map_err(|e| anyhow::anyhow!("Failed to create downloads dir: {}", e))?;
            let ts = chrono::Utc::now().format("%Y%m%d_%H%M%S");

            // Сигнатура ZIP — "PK\x03\x04".
            if bytes.starts_with(b"PK\x03\x04") {
                let _ = std::fs::write(dir.join(format!("report_{}.zip", ts)), &bytes);
                let cursor = std::io::Cursor::new(&bytes[..]);
                let mut archive = zip::ZipArchive::new(cursor)
                    .map_err(|e| anyhow::anyhow!("Failed to open ZIP archive: {}", e))?;
                for i in 0..archive.len() {
                    let mut file = archive
                        .by_index(i)
                        .map_err(|e| anyhow::anyhow!("Failed to read ZIP entry {}: {}", i, e))?;
                    if !file.is_file() {
                        continue;
                    }
                    use std::io::Read;
                    let mut raw: Vec<u8> = Vec::new();
                    file.read_to_end(&mut raw)
                        .map_err(|e| anyhow::anyhow!("Failed to read ZIP entry: {}", e))?;
                    let content = decode_report_csv(&raw);
                    let _ = std::fs::write(dir.join(format!("report_{}.txt", ts)), &content);
                    return Ok(content);
                }
                anyhow::bail!("Report ZIP archive contains no files");
            }

            let content = decode_report_csv(&bytes);
            let _ = std::fs::write(dir.join(format!("report_{}.txt", ts)), &content);
            Ok(content)
        })
        .await
        .map_err(|e| anyhow::anyhow!("spawn_blocking panicked: {}", e))?
    }

    /// Phase 2: Poll report generation status.
    /// Endpoint: GET https://api.partner.market.yandex.ru/v2/reports/info/{reportId}
    /// Returns (status_str, Option<download_url>).
    /// status_str values: "PENDING" | "PROCESSING" | "DONE" | "FAILED"
    pub async fn poll_report_status(
        &self,
        connection: &ConnectionMP,
        report_id: &str,
    ) -> Result<(String, Option<String>)> {
        let url = format!(
            "https://api.partner.market.yandex.ru/v2/reports/info/{}",
            report_id
        );

        let request = self.client.get(&url);

        let response = self
            .apply_auth(request, connection)?
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to poll report status: {}", e))?;

        let status = response.status();
        if !status.is_success() {
            let err_body = response.text().await.unwrap_or_default();
            anyhow::bail!(
                "poll_report_status failed with status {}: {}",
                status,
                err_body
            );
        }

        let resp_body = response.text().await?;

        let parsed: serde_json::Value = serde_json::from_str(&resp_body)
            .map_err(|e| anyhow::anyhow!("Failed to parse status response: {}", e))?;

        let report_status = parsed
            .pointer("/result/status")
            .and_then(|v| v.as_str())
            .unwrap_or("UNKNOWN")
            .to_string();

        let file_url = parsed
            .pointer("/result/file")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        self.log_to_file(&format!(
            "Poll report status ({}): status={}, file_available={}",
            report_id,
            report_status,
            file_url.is_some()
        ));

        Ok((report_status, file_url))
    }

    /// Phase 3: Download the generated CSV report from the signed URL.
    /// Returns the raw CSV text content.
    /// Downloads a ZIP archive from the given URL, saves it under `downloads/{subdir}/`,
    /// extracts the first CSV file found inside, saves that too, and returns (csv_text, zip_path, csv_path).
    pub async fn download_report_zip(
        &self,
        url: &str,
        subdir: &str,
    ) -> Result<(String, String, String)> {
        self.log_to_file(&format!(
            "Downloading payment report ZIP (target={})",
            subdir
        ));

        let response = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to download report file: {}", e))?;

        let status = response.status();
        if !status.is_success() {
            let err_body = response.text().await.unwrap_or_default();
            anyhow::bail!(
                "download_report_zip failed with status {}: {}",
                status,
                err_body
            );
        }

        let bytes = response
            .bytes()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to read report file body: {}", e))?;

        self.log_to_file(&format!(
            "Downloaded payment report ZIP, size={} bytes",
            bytes.len()
        ));

        // Save ZIP and extract CSV in a blocking thread (zip operations are synchronous)
        let log_closure = |msg: String| write_yandex_log(&msg);

        let bytes_vec = bytes.to_vec();
        let subdir = subdir.to_string();
        let (csv_text, zip_path, csv_path) =
            tokio::task::spawn_blocking(move || -> Result<(String, String, String)> {
                // Ensure downloads directory exists
                let dir_path = format!("downloads/{}", subdir);
                let dir = std::path::Path::new(&dir_path);
                std::fs::create_dir_all(dir)
                    .map_err(|e| anyhow::anyhow!("Failed to create downloads dir: {}", e))?;

                // Save ZIP with timestamp
                let ts = chrono::Utc::now().format("%Y%m%d_%H%M%S");
                let zip_path = dir.join(format!("report_{}.zip", ts));
                std::fs::write(&zip_path, &bytes_vec)
                    .map_err(|e| anyhow::anyhow!("Failed to save ZIP to disk: {}", e))?;
                let zip_path_str = zip_path.to_string_lossy().to_string();
                log_closure(format!("Saved ZIP to: {}", zip_path_str));

                // Extract CSV from ZIP
                let cursor = std::io::Cursor::new(&bytes_vec[..]);
                let mut archive = zip::ZipArchive::new(cursor)
                    .map_err(|e| anyhow::anyhow!("Failed to open ZIP archive: {}", e))?;

                log_closure(format!("ZIP contains {} file(s):", archive.len()));
                for i in 0..archive.len() {
                    if let Ok(f) = archive.by_index(i) {
                        log_closure(format!(
                            "  ZIP entry[{}]: {} ({} bytes uncompressed)",
                            i,
                            f.name(),
                            f.size()
                        ));
                    }
                }

                for i in 0..archive.len() {
                    let mut file = archive
                        .by_index(i)
                        .map_err(|e| anyhow::anyhow!("Failed to read ZIP entry {}: {}", i, e))?;

                    let name = file.name().to_string();
                    let lower = name.to_lowercase();

                    if lower.ends_with(".csv") {
                        use std::io::Read;
                        let mut raw_bytes: Vec<u8> = Vec::new();
                        file.read_to_end(&mut raw_bytes).map_err(|e| {
                            anyhow::anyhow!("Failed to read {} from ZIP: {}", name, e)
                        })?;

                        // Save raw CSV bytes to disk before decoding
                        let csv_path = dir.join(format!("report_{}.csv", ts));
                        std::fs::write(&csv_path, &raw_bytes)
                            .map_err(|e| anyhow::anyhow!("Failed to save CSV to disk: {}", e))?;
                        let csv_path_str = csv_path.to_string_lossy().to_string();
                        log_closure(format!("Saved CSV to: {}", csv_path_str));

                        // Decode: UTF-8 (BOM-aware) если валиден; иначе Windows-1251.
                        // Netting-отчёт YM приходит в UTF-8, отчёт о реализации — в CP1251.
                        let content = decode_report_csv(&raw_bytes);

                        let preview: String = content.chars().take(500).collect();
                        log_closure(format!(
                            "CSV '{}': {} chars, first 500 chars: {:?}",
                            name,
                            content.len(),
                            preview
                        ));

                        return Ok((content, zip_path_str, csv_path_str));
                    }
                }

                anyhow::bail!("No .csv file found inside the payment report ZIP archive")
            })
            .await
            .map_err(|e| anyhow::anyhow!("spawn_blocking panicked: {}", e))??;

        Ok((csv_text, zip_path, csv_path))
    }

    /// Скачивает ZIP и извлекает ВСЕ .csv-файлы внутри (отчёт о реализации YM —
    /// многофайловый: transferred_to_delivery / delivered / returned / unredeemed
    /// / lost_items). Возвращает список (имя_файла_в_архиве, содержимое).
    pub async fn download_report_csvs(
        &self,
        url: &str,
        subdir: &str,
    ) -> Result<Vec<(String, String)>> {
        self.log_to_file(&format!(
            "Downloading multi-CSV report ZIP (target={})",
            subdir
        ));

        let response = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to download report file: {}", e))?;
        let status = response.status();
        if !status.is_success() {
            let err_body = response.text().await.unwrap_or_default();
            anyhow::bail!(
                "download_report_csvs failed with status {}: {}",
                status,
                err_body
            );
        }
        let bytes = response
            .bytes()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to read report file body: {}", e))?;

        let log_closure = |msg: String| write_yandex_log(&msg);

        let bytes_vec = bytes.to_vec();
        let subdir = subdir.to_string();
        let result = tokio::task::spawn_blocking(move || -> Result<Vec<(String, String)>> {
            let dir_path = format!("downloads/{}", subdir);
            let dir = std::path::Path::new(&dir_path);
            std::fs::create_dir_all(dir)
                .map_err(|e| anyhow::anyhow!("Failed to create downloads dir: {}", e))?;
            let ts = chrono::Utc::now().format("%Y%m%d_%H%M%S");
            let zip_path = dir.join(format!("report_{}.zip", ts));
            std::fs::write(&zip_path, &bytes_vec)
                .map_err(|e| anyhow::anyhow!("Failed to save ZIP to disk: {}", e))?;

            let cursor = std::io::Cursor::new(&bytes_vec[..]);
            let mut archive = zip::ZipArchive::new(cursor)
                .map_err(|e| anyhow::anyhow!("Failed to open ZIP archive: {}", e))?;

            let mut out: Vec<(String, String)> = Vec::new();
            for i in 0..archive.len() {
                let mut file = archive
                    .by_index(i)
                    .map_err(|e| anyhow::anyhow!("Failed to read ZIP entry {}: {}", i, e))?;
                let name = file.name().to_string();
                if !name.to_lowercase().ends_with(".csv") {
                    continue;
                }
                use std::io::Read;
                let mut raw_bytes: Vec<u8> = Vec::new();
                file.read_to_end(&mut raw_bytes)
                    .map_err(|e| anyhow::anyhow!("Failed to read {} from ZIP: {}", name, e))?;

                let stem = std::path::Path::new(&name)
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("part");
                let saved = dir.join(format!("report_{}__{}.csv", ts, stem));
                let _ = std::fs::write(&saved, &raw_bytes);

                let content = decode_report_csv(&raw_bytes);
                log_closure(format!("Extracted CSV '{}': {} chars", name, content.len()));
                out.push((name, content));
            }

            if out.is_empty() {
                anyhow::bail!("No .csv files found inside the report ZIP archive");
            }
            Ok(out)
        })
        .await
        .map_err(|e| anyhow::anyhow!("spawn_blocking panicked: {}", e))??;

        Ok(result)
    }
}

/// Строка отчёта «Аналитика продаж» (shows-sales, `grouping=OFFERS`) —
/// один товар (`offerId`) за один день.
///
/// Все метрики опциональны: YM опускает нули и, в зависимости от тарифа/периода,
/// может не отдать часть колонок. `None` означает «данных нет» (N/A), а не «ноль» —
/// подмена нулём делает воронку правдоподобной, но ложной.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YmShowsSalesRow {
    /// День месяца. Дата собирается из day/month/year — единой колонки даты нет.
    #[serde(default)]
    pub day: Option<serde_json::Value>,
    #[serde(default)]
    pub month: Option<serde_json::Value>,
    #[serde(default)]
    pub year: Option<serde_json::Value>,
    /// «Ваш SKU» — ключ товара, совпадает с `shop_sku` строк заказа a013.
    #[serde(default)]
    pub offer_id: Option<String>,
    #[serde(default)]
    pub offer_name: Option<String>,
    /// Показы моих товаров, шт. Настоящие impressions (в отличие от WB).
    #[serde(default)]
    pub shows: Option<i64>,
    /// Показы товаров всех продавцов по этому MSKU — рынок, не мои показы.
    #[serde(default)]
    pub by_msku_shows: Option<i64>,
    #[serde(default)]
    pub clicks: Option<i64>,
    #[serde(default)]
    pub to_cart: Option<i64>,
    #[serde(default)]
    pub order_items: Option<i64>,
    /// Заказано на сумму, ₽. Живой JSON отдаёт целое; `null` = N/A, не ноль.
    #[serde(default)]
    pub order_items_total_amount: Option<i64>,
    #[serde(default)]
    pub order_items_delivered_count: Option<i64>,
    /// Доставлено за период на сумму, ₽.
    #[serde(default)]
    pub order_items_delivered_total_amount: Option<i64>,
    /// «Отмены и невыкупы за период» — счётчик отказов YM.
    #[serde(default)]
    pub order_items_canceled_count: Option<i64>,
    #[serde(default)]
    pub order_items_returned_count: Option<i64>,
}

impl YmShowsSalesRow {
    /// Дата строки `YYYY-MM-DD` из day/month/year. YM отдаёт их то числами, то строками
    /// (а месяц — иногда названием), поэтому разбираем оба варианта.
    pub fn date(&self) -> Option<String> {
        // Фактический JSON reports/shows-sales сейчас отдаёт `day` целой датой
        // (`10-08-2026`), а `month` — строкой `08-2026`. Сначала принимаем
        // полную дату, затем оставляем совместимость со старой компонентной формой.
        if let Some(serde_json::Value::String(day)) = self.day.as_ref() {
            for format in ["%d-%m-%Y", "%Y-%m-%d", "%d.%m.%Y"] {
                if let Ok(date) = chrono::NaiveDate::parse_from_str(day.trim(), format) {
                    return Some(date.format("%Y-%m-%d").to_string());
                }
            }
        }

        let day = numeric_part(self.day.as_ref())?;
        let month = numeric_part(self.month.as_ref())?;
        let year = numeric_part(self.year.as_ref())?;
        if !(1..=31).contains(&day) || !(1..=12).contains(&month) || !(2000..=2100).contains(&year)
        {
            return None;
        }
        Some(format!("{year:04}-{month:02}-{day:02}"))
    }

    /// Есть ли в строке хоть одна ненулевая метрика (разреженность проекции).
    pub fn has_activity(&self) -> bool {
        [
            self.shows,
            self.clicks,
            self.to_cart,
            self.order_items,
            self.order_items_total_amount,
            self.order_items_delivered_count,
            self.order_items_delivered_total_amount,
            self.order_items_canceled_count,
            self.order_items_returned_count,
        ]
        .iter()
        .any(|v| v.unwrap_or(0) != 0)
    }
}

/// Число из JSON-значения, пришедшего числом или строкой.
fn numeric_part(value: Option<&serde_json::Value>) -> Option<i64> {
    match value? {
        serde_json::Value::Number(n) => n.as_i64(),
        serde_json::Value::String(s) => {
            let value = s.trim();
            value.parse::<i64>().ok().or_else(|| {
                let prefix: String = value.chars().take_while(|ch| ch.is_ascii_digit()).collect();
                (!prefix.is_empty())
                    .then(|| prefix.parse::<i64>().ok())
                    .flatten()
            })
        }
        _ => None,
    }
}

/// Разбирает тело отчёта shows-sales (JSON). Строки лежат либо в `result.rows`,
/// либо в `result` массивом, либо массивом в корне — принимаем все три формы,
/// чтобы импорт не падал из-за обёртки.
pub fn parse_shows_sales_report(body: &str) -> Result<Vec<YmShowsSalesRow>> {
    let parsed: serde_json::Value = serde_json::from_str(body).map_err(|e| {
        let snippet: String = body.chars().take(400).collect();
        anyhow::anyhow!(
            "Failed to parse YM shows-sales report: {} | body: {}",
            e,
            snippet
        )
    })?;

    let rows_value = parsed
        .pointer("/result/rows")
        .or_else(|| parsed.pointer("/result/offers"))
        .or_else(|| parsed.pointer("/rows"))
        .or_else(|| parsed.pointer("/result"))
        .cloned()
        .unwrap_or(parsed);

    let array = rows_value.as_array().cloned().ok_or_else(|| {
        let snippet: String = body.chars().take(400).collect();
        anyhow::anyhow!("YM shows-sales report has no row array | body: {}", snippet)
    })?;

    let mut rows = Vec::with_capacity(array.len());
    for item in array {
        let row: YmShowsSalesRow = serde_json::from_value(item)
            .map_err(|e| anyhow::anyhow!("Failed to decode YM shows-sales row: {}", e))?;
        rows.push(row);
    }
    Ok(rows)
}

#[cfg(test)]
mod request_contract_tests {
    use super::*;

    #[test]
    fn offer_mapping_token_serializes_as_official_page_token() {
        let request = YandexProductListRequest {
            limit: Some(100),
            page_token: Some("next-token".into()),
        };
        let json = serde_json::to_value(request).unwrap();
        assert_eq!(json["pageToken"], "next-token");
        assert!(json.get("page_token").is_none());
    }

    #[test]
    fn retry_after_has_priority_for_report_limiter() {
        let mut headers = HeaderMap::new();
        headers.insert(reqwest::header::RETRY_AFTER, HeaderValue::from_static("17"));
        headers.insert(
            "X-RateLimit-Resource-Until",
            HeaderValue::from_static("Thu, 10 Jul 2036 00:42:42 GMT"),
        );
        assert_eq!(ym_header_wait(&headers).unwrap().as_secs(), 17);
    }

    #[test]
    fn shows_sales_store_request_uses_campaign_id_only() {
        let request = GenerateShowsSalesRequest {
            campaign_id: 136_982_050,
            date_from: "2026-08-01".into(),
            date_to: "2026-08-05".into(),
            grouping: "OFFERS",
        };
        let json = serde_json::to_value(request).unwrap();

        assert_eq!(json["campaignId"], 136_982_050);
        assert_eq!(json["grouping"], "OFFERS");
        assert!(json.get("businessId").is_none());
    }

    #[test]
    fn shows_sales_row_parses_live_report_date_format() {
        let row: YmShowsSalesRow = serde_json::from_value(serde_json::json!({
            "day": "10-08-2026",
            "month": "08-2026",
            "year": 2026,
            "offerId": "SKU-1",
            "shows": 17
        }))
        .unwrap();

        assert_eq!(row.date().as_deref(), Some("2026-08-10"));
    }

    #[test]
    fn shows_sales_report_parses_root_rows_from_live_shape() {
        let rows = parse_shows_sales_report(
            r#"{"rows":[{"day":"10-08-2026","month":"08-2026","year":2026,"offerId":"SKU-1","shows":17}]}"#,
        )
        .unwrap();

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].date().as_deref(), Some("2026-08-10"));
        assert_eq!(rows[0].offer_id.as_deref(), Some("SKU-1"));
        assert_eq!(rows[0].shows, Some(17));
    }

    #[test]
    fn shows_sales_row_parses_order_and_delivered_amounts() {
        let row: YmShowsSalesRow = serde_json::from_value(serde_json::json!({
            "day": "24-08-2026",
            "month": "08-2026",
            "year": 2026,
            "offerId": "SKU-1",
            "orderItems": 1,
            "orderItemsTotalAmount": 40826,
            "orderItemsDeliveredCount": 1,
            "orderItemsDeliveredTotalAmount": 7054
        }))
        .unwrap();

        assert_eq!(row.order_items, Some(1));
        assert_eq!(row.order_items_total_amount, Some(40826));
        assert_eq!(row.order_items_delivered_count, Some(1));
        assert_eq!(row.order_items_delivered_total_amount, Some(7054));
    }

    #[test]
    fn shows_sales_row_keeps_missing_amounts_as_na() {
        let row: YmShowsSalesRow = serde_json::from_value(serde_json::json!({
            "day": "24-08-2026",
            "month": "08-2026",
            "year": 2026,
            "offerId": "SKU-1",
            "shows": 44,
            "orderItems": null,
            "orderItemsTotalAmount": null
        }))
        .unwrap();

        assert!(row.order_items.is_none());
        assert!(row.order_items_total_amount.is_none());
        assert!(row.order_items_delivered_total_amount.is_none());
    }
}
