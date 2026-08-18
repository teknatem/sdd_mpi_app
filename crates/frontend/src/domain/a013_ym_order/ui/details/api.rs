//! API layer for YM Order details

use crate::shared::api_utils::api_base;
use gloo_net::http::Request;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NomenclatureInfo {
    pub description: String,
    pub article: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketplaceProductInfo {
    pub description: String,
    pub article: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YmOrderDetailDto {
    pub id: String,
    pub code: String,
    pub description: String,
    pub header: HeaderDto,
    pub lines: Vec<LineDto>,
    pub state: StateDto,
    pub source_meta: SourceMetaDto,
    pub metadata: MetadataDto,
    #[serde(default)]
    pub is_error: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeaderDto {
    pub document_no: String,
    pub connection_id: String,
    pub organization_id: String,
    pub marketplace_id: String,
    pub campaign_id: String,
    pub total_amount: Option<f64>,
    pub currency: Option<String>,
    #[serde(default)]
    pub items_total: Option<f64>,
    #[serde(default)]
    pub delivery_total: Option<f64>,
    #[serde(default)]
    pub subsidies_json: Option<String>,
    #[serde(default)]
    pub total_dealer_amount: Option<f64>,
    #[serde(default)]
    pub margin_pro: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LineDto {
    pub line_id: String,
    pub shop_sku: String,
    pub offer_id: String,
    pub name: String,
    pub qty: f64,
    pub price_list: Option<f64>,
    pub discount_total: Option<f64>,
    pub price_effective: Option<f64>,
    pub amount_line: Option<f64>,
    pub currency_code: Option<String>,
    #[serde(default)]
    pub buyer_price: Option<f64>,
    #[serde(default)]
    pub subsidies_json: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub price_plan: Option<f64>,
    #[serde(default)]
    pub marketplace_product_ref: Option<String>,
    #[serde(default)]
    pub nomenclature_ref: Option<String>,
    #[serde(default)]
    pub dealer_price_ut: Option<f64>,
    /// Детали судьбы позиции (частичные возвраты/отказы)
    #[serde(default)]
    pub details: Vec<LineDetailDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LineDetailDto {
    pub count: f64,
    pub status: String,
    #[serde(default)]
    pub update_date: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateDto {
    pub status_raw: String,
    pub substatus_raw: Option<String>,
    pub status_norm: String,
    pub status_changed_at: Option<String>,
    pub updated_at_source: Option<String>,
    pub creation_date: Option<String>,
    pub delivery_date: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceMetaDto {
    pub raw_payload_ref: String,
    pub fetched_at: String,
    pub document_version: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetadataDto {
    pub created_at: String,
    pub updated_at: String,
    pub is_deleted: bool,
    pub is_posted: bool,
    pub version: i32,
}

pub async fn fetch_by_id(id: &str) -> Result<YmOrderDetailDto, String> {
    let url = format!("{}/api/a013/ym-order/{}", api_base(), id);
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
    serde_json::from_str(&text).map_err(|e| format!("Failed to parse: {}", e))
}

pub async fn fetch_raw_json(raw_payload_ref: &str) -> Result<String, String> {
    let url = format!("{}/api/a013/raw/{}", api_base(), raw_payload_ref);
    let response = Request::get(&url)
        .send()
        .await
        .map_err(|e| format!("Failed to fetch raw JSON: {}", e))?;
    if response.status() != 200 {
        return Err(format!("Server error: {}", response.status()));
    }
    let text = response
        .text()
        .await
        .map_err(|e| format!("Failed to read response: {}", e))?;
    let json_value: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("Failed to parse JSON: {}", e))?;
    serde_json::to_string_pretty(&json_value).map_err(|e| format!("Failed to format JSON: {}", e))
}

pub async fn fetch_projections(id: &str) -> Result<serde_json::Value, String> {
    let url = format!("{}/api/a013/ym-order/{}/projections", api_base(), id);
    let response = Request::get(&url)
        .send()
        .await
        .map_err(|e| format!("Failed to fetch projections: {}", e))?;
    if response.status() != 200 {
        return Err(format!("Server error: {}", response.status()));
    }
    let text = response
        .text()
        .await
        .map_err(|e| format!("Failed to read response: {}", e))?;
    serde_json::from_str(&text).map_err(|e| format!("Failed to parse projections: {}", e))
}

pub async fn post_document(id: &str) -> Result<(), String> {
    let url = format!("{}/api/a013/ym-order/{}/post", api_base(), id);
    let response = Request::post(&url)
        .send()
        .await
        .map_err(|e| format!("Failed to post document: {}", e))?;
    if response.status() != 200 {
        return Err(format!("Failed to post: status {}", response.status()));
    }
    Ok(())
}

pub async fn unpost_document(id: &str) -> Result<(), String> {
    let url = format!("{}/api/a013/ym-order/{}/unpost", api_base(), id);
    let response = Request::post(&url)
        .send()
        .await
        .map_err(|e| format!("Failed to unpost document: {}", e))?;
    if response.status() != 200 {
        return Err(format!("Failed to unpost: status {}", response.status()));
    }
    Ok(())
}

pub async fn fetch_marketplace_product(id: &str) -> Result<MarketplaceProductInfo, String> {
    let url = format!("{}/api/marketplace_product/{}", api_base(), id);
    let response = Request::get(&url)
        .send()
        .await
        .map_err(|e| format!("Failed to fetch marketplace product: {}", e))?;
    if response.status() != 200 {
        return Err(format!("Server error: {}", response.status()));
    }
    let text = response
        .text()
        .await
        .map_err(|e| format!("Failed to read response: {}", e))?;
    let json: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("Failed to parse: {}", e))?;

    Ok(MarketplaceProductInfo {
        description: json
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        article: json
            .get("article")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
    })
}

/// DTO for p907 payment report records linked to this order
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YmPaymentReportLinkDto {
    pub id: String,
    pub record_key: String,
    pub transaction_date: Option<String>,
    pub transaction_type: Option<String>,
    pub transaction_id: Option<String>,
    pub transaction_sum: Option<f64>,
    pub bank_sum: Option<f64>,
    pub payment_status: Option<String>,
    pub transaction_source: Option<String>,
    pub shop_sku: Option<String>,
    pub offer_or_service_name: Option<String>,
    pub count: Option<i32>,
    pub comments: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PaymentReportListResponse {
    items: Vec<YmPaymentReportLinkDto>,
    total_count: i32,
    #[allow(dead_code)]
    has_more: bool,
}

pub async fn fetch_payment_reports_by_order(
    order_id: i64,
) -> Result<Vec<YmPaymentReportLinkDto>, String> {
    let url = format!(
        "{}/api/p907/payment-report?order_id={}&limit=1000&sort_by=transaction_date&sort_desc=false",
        api_base(),
        order_id
    );
    let response = Request::get(&url)
        .send()
        .await
        .map_err(|e| format!("Failed to fetch payment reports: {}", e))?;
    if response.status() != 200 {
        return Err(format!("Server error: {}", response.status()));
    }
    let text = response
        .text()
        .await
        .map_err(|e| format!("Failed to read response: {}", e))?;
    let resp: PaymentReportListResponse =
        serde_json::from_str(&text).map_err(|e| format!("Failed to parse: {}", e))?;
    Ok(resp.items)
}

/// Событие заказа из проекции p915_mp_order_events (для таймлайна на вкладке «Общие»).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderEventDto {
    #[serde(default)]
    pub event_date: String,
    #[serde(default)]
    pub event_type: String,
    #[serde(default)]
    pub amount: Option<f64>,
    #[serde(default)]
    pub registrator_type: String,
    #[serde(default)]
    pub registrator_ref: String,
}

/// События заказа из p915 по номеру заказа (document_no). Уже отсортированы по дате.
pub async fn fetch_order_events(order_id: &str) -> Result<Vec<OrderEventDto>, String> {
    let url = format!("{}/api/p915/order-events/by-order/{}", api_base(), order_id);
    let response = Request::get(&url)
        .send()
        .await
        .map_err(|e| format!("Failed to fetch order events: {}", e))?;
    if response.status() != 200 {
        return Err(format!("Server error: {}", response.status()));
    }
    let text = response
        .text()
        .await
        .map_err(|e| format!("Failed to read response: {}", e))?;
    serde_json::from_str(&text).map_err(|e| format!("Failed to parse order events: {}", e))
}

/// Наименование агрегата по id: GET url → поле `description` (фолбэк `code`).
/// Пустой/нерезолвящийся ответ → None.
async fn fetch_entity_name(url: &str) -> Option<String> {
    let response = Request::get(url).send().await.ok()?;
    if response.status() != 200 {
        return None;
    }
    let text = response.text().await.ok()?;
    let json: serde_json::Value = serde_json::from_str(&text).ok()?;
    let pick = |key: &str| {
        json.get(key)
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
    };
    pick("description").or_else(|| pick("code"))
}

pub async fn fetch_connection_name(id: &str) -> Option<String> {
    fetch_entity_name(&format!("{}/api/connection_mp/{}", api_base(), id)).await
}

pub async fn fetch_organization_name(id: &str) -> Option<String> {
    fetch_entity_name(&format!("{}/api/organization/{}", api_base(), id)).await
}

pub async fn fetch_marketplace_name(id: &str) -> Option<String> {
    fetch_entity_name(&format!("{}/api/marketplace/{}", api_base(), id)).await
}

pub async fn fetch_nomenclature(id: &str) -> Result<NomenclatureInfo, String> {
    let url = format!("{}/api/nomenclature/{}", api_base(), id);
    let response = Request::get(&url)
        .send()
        .await
        .map_err(|e| format!("Failed to fetch nomenclature: {}", e))?;
    if response.status() != 200 {
        return Err(format!("Server error: {}", response.status()));
    }
    let text = response
        .text()
        .await
        .map_err(|e| format!("Failed to read response: {}", e))?;
    let json: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("Failed to parse: {}", e))?;

    Ok(NomenclatureInfo {
        description: json
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        article: json
            .get("article")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
    })
}
