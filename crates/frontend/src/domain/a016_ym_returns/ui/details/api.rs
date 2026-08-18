//! Yandex Market Returns Details - API Layer
//!
//! DTOs and API functions for YM returns details

use crate::shared::api_utils::api_base;
use gloo_net::http::Request;
use serde::{Deserialize, Serialize};

/// Main return detail DTO
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YmReturnDetailDto {
    pub id: String,
    pub code: String,
    pub description: String,
    pub header: HeaderDto,
    pub lines: Vec<LineDto>,
    pub state: StateDto,
    pub source_meta: SourceMetaDto,
    pub is_posted: bool,
}

/// Return header information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeaderDto {
    pub return_id: i64,
    pub order_id: i64,
    pub connection_id: String,
    pub organization_id: String,
    pub marketplace_id: String,
    pub campaign_id: String,
    pub return_type: String,
    pub amount: Option<f64>,
    pub currency: Option<String>,
}

/// Return line item
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LineDto {
    pub item_id: i64,
    pub shop_sku: String,
    pub offer_id: String,
    pub name: String,
    pub count: i32,
    pub price: Option<f64>,
    pub return_reason: Option<String>,
    pub decisions: Vec<DecisionDto>,
    #[serde(default)]
    pub photos: Vec<String>,
}

/// Decision information for line item
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionDto {
    pub decision_type: String,
    pub amount: Option<f64>,
    pub currency: Option<String>,
    pub partner_compensation_amount: Option<f64>,
    pub comment: Option<String>,
}

/// Return state information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateDto {
    pub refund_status: String,
    pub created_at_source: Option<String>,
    pub updated_at_source: Option<String>,
    pub refund_date: Option<String>,
}

/// Source metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceMetaDto {
    pub raw_payload_ref: String,
    pub fetched_at: String,
    pub document_version: i32,
}

// Constants for table IDs
pub const TABLE_ID_LINES: &str = "a016-ym-return-lines-table";
pub const TABLE_ID_PROJECTIONS: &str = "a016-ym-return-p904-table";

// =============================================================================
// Linked entity info (for «Технические связи» navigation buttons)
// =============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionInfo {
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrganizationInfo {
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketplaceInfo {
    pub name: String,
}

// =============================================================================
// API functions
// =============================================================================

pub async fn fetch_by_id(id: &str) -> Result<YmReturnDetailDto, String> {
    let url = format!("{}/api/a016/ym-returns/{}", api_base(), id);
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
    let url = format!("{}/api/a016/raw/{}", api_base(), raw_payload_ref);
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
    let url = format!("{}/api/a016/ym-returns/{}/projections", api_base(), id);
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
    serde_json::from_str(&text).map_err(|e| format!("Failed to parse: {}", e))
}

pub async fn post_document(id: &str) -> Result<(), String> {
    let url = format!("{}/api/a016/ym-returns/{}/post", api_base(), id);
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
    let url = format!("{}/api/a016/ym-returns/{}/unpost", api_base(), id);
    let response = Request::post(&url)
        .send()
        .await
        .map_err(|e| format!("Failed to unpost document: {}", e))?;

    if response.status() != 200 {
        return Err(format!("Failed to unpost: status {}", response.status()));
    }
    Ok(())
}

/// Резолв внутреннего id исходного заказа (a013_ym_order) по номеру заказа.
/// Возвращает None, если заказ не загружен (404).
pub async fn fetch_source_order_id(order_no: &str) -> Result<Option<String>, String> {
    let url = format!(
        "{}/api/a016/ym-returns/source-order/{}",
        api_base(),
        order_no
    );
    let response = Request::get(&url)
        .send()
        .await
        .map_err(|e| format!("Failed to fetch source order: {}", e))?;

    if response.status() == 404 {
        return Ok(None);
    }
    if response.status() != 200 {
        return Err(format!("Server error: {}", response.status()));
    }

    let text = response
        .text()
        .await
        .map_err(|e| format!("Failed to read response: {}", e))?;
    let json: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("Failed to parse: {}", e))?;

    Ok(json
        .get("id")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string()))
}

pub async fn fetch_connection(id: &str) -> Result<ConnectionInfo, String> {
    let url = format!("{}/api/connection_mp/{}", api_base(), id);
    let response = Request::get(&url)
        .send()
        .await
        .map_err(|e| format!("Failed to fetch connection: {}", e))?;

    if response.status() != 200 {
        return Err(format!("Server error: {}", response.status()));
    }

    let text = response
        .text()
        .await
        .map_err(|e| format!("Failed to read response: {}", e))?;
    let json: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("Failed to parse: {}", e))?;

    Ok(ConnectionInfo {
        description: json
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
    })
}

pub async fn fetch_organization(id: &str) -> Result<OrganizationInfo, String> {
    let url = format!("{}/api/organization/{}", api_base(), id);
    let response = Request::get(&url)
        .send()
        .await
        .map_err(|e| format!("Failed to fetch organization: {}", e))?;

    if response.status() != 200 {
        return Err(format!("Server error: {}", response.status()));
    }

    let text = response
        .text()
        .await
        .map_err(|e| format!("Failed to read response: {}", e))?;
    let json: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("Failed to parse: {}", e))?;

    Ok(OrganizationInfo {
        description: json
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
    })
}

pub async fn fetch_marketplace(id: &str) -> Result<MarketplaceInfo, String> {
    let url = format!("{}/api/marketplace/{}", api_base(), id);
    let response = Request::get(&url)
        .send()
        .await
        .map_err(|e| format!("Failed to fetch marketplace: {}", e))?;

    if response.status() != 200 {
        return Err(format!("Server error: {}", response.status()));
    }

    let text = response
        .text()
        .await
        .map_err(|e| format!("Failed to read response: {}", e))?;
    let json: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("Failed to parse: {}", e))?;

    Ok(MarketplaceInfo {
        name: json
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
    })
}
