//! API layer for WB Supply details

use crate::shared::api_utils::api_base;
use gloo_net::http::Request;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WbSupplyDetailDto {
    pub id: String,
    pub code: String,
    pub description: String,
    pub header: HeaderDto,
    pub info: InfoDto,
    pub source_meta: SourceMetaDto,
    pub metadata: MetadataDto,
    pub supply_orders: Vec<SupplyOrderDto>,
    pub document_date: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeaderDto {
    pub supply_id: String,
    pub connection_id: String,
    pub organization_id: String,
    pub marketplace_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InfoDto {
    pub name: Option<String>,
    pub is_b2b: bool,
    pub is_done: bool,
    pub created_at_wb: Option<String>,
    pub closed_at_wb: Option<String>,
    pub scan_dt: Option<String>,
    pub cargo_type: Option<i32>,
    pub cross_border_type: Option<i32>,
    pub destination_office_id: Option<i64>,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SupplyOrderDto {
    pub order_id: i64,
    pub order_uid: Option<String>,
    pub article: Option<String>,
    pub nm_id: Option<i64>,
    pub chrt_id: Option<i64>,
    pub barcodes: Vec<String>,
    pub price: Option<i64>,
    pub created_at: Option<String>,
    pub warehouse_id: Option<i64>,
    pub part_a: Option<i64>,
    pub part_b: Option<i64>,
    pub nomenclature_ref: Option<String>,
    pub base_nomenclature_ref: Option<String>,
    pub color_code: Option<String>,
    pub status: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StickerDto {
    pub order_id: i64,
    pub article: Option<String>,
    pub shkid: Option<String>,
    pub part_a: Option<i64>,
    pub part_b: Option<i64>,
    pub barcode: Option<String>,
    pub file: Option<String>,
    pub from_wb: bool,
}

/// Wrapper response from the stickers endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StickersResponse {
    pub stickers: Vec<StickerDto>,
    pub orders_total: usize,
    pub orders_with_numeric_id: usize,
    pub warning: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionInfo {
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrganizationInfo {
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NomenclatureInfo {
    pub description: String,
    pub article: String,
}

/// Fetch supply by its internal UUID.
pub async fn fetch_by_id(id: &str) -> Result<WbSupplyDetailDto, String> {
    let url = format!("{}/api/a029/wb-supply/{}", api_base(), id);
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

/// Fetch supply by its WB ID string like "WB-GI-32319994" (used when navigating from orders).
pub async fn fetch_by_wb_id(wb_id: &str) -> Result<WbSupplyDetailDto, String> {
    let url = format!("{}/api/a029/wb-supply/by-wb-id/{}", api_base(), wb_id);
    let response = Request::get(&url)
        .send()
        .await
        .map_err(|e| format!("Failed to fetch: {}", e))?;

    if response.status() == 404 {
        return Err(format!("Поставка {} не найдена в базе", wb_id));
    }
    if response.status() != 200 {
        return Err(format!("Server error: {}", response.status()));
    }

    let text = response
        .text()
        .await
        .map_err(|e| format!("Failed to read response: {}", e))?;
    serde_json::from_str(&text).map_err(|e| format!("Failed to parse: {}", e))
}

pub async fn fetch_supply_orders(id: &str) -> Result<Vec<SupplyOrderDto>, String> {
    let url = format!("{}/api/a029/wb-supply/{}/orders", api_base(), id);
    let response = Request::get(&url)
        .send()
        .await
        .map_err(|e| format!("Failed to fetch orders: {}", e))?;

    if response.status() != 200 {
        return Err(format!("Server error: {}", response.status()));
    }

    response
        .json()
        .await
        .map_err(|e| format!("Failed to parse orders: {}", e))
}

/// Fetch stickers for specific order IDs from WB API.
/// `order_ids`: numeric WB order IDs to include (only those with id > 0 will work).
pub async fn fetch_stickers_for_ids(
    supply_id: &str,
    order_ids: &[i64],
    sticker_type: &str,
) -> Result<StickersResponse, String> {
    let ids_str = order_ids
        .iter()
        .map(|id| id.to_string())
        .collect::<Vec<_>>()
        .join(",");
    let url = format!(
        "{}/api/a029/wb-supply/{}/stickers?type={}&order_ids={}",
        api_base(),
        supply_id,
        sticker_type,
        ids_str,
    );
    let response = Request::get(&url)
        .send()
        .await
        .map_err(|e| format!("Ошибка сети: {}", e))?;

    if response.status() != 200 {
        let body = response.text().await.unwrap_or_default();
        return Err(format!("Ошибка сервера {}: {}", response.status(), body));
    }

    response
        .json::<StickersResponse>()
        .await
        .map_err(|e| format!("Ошибка разбора ответа: {}", e))
}

/// Fetch stickers from WB API via backend proxy.
/// `sticker_type`: "png" | "svg" | "zplv" | "zplh"
pub async fn fetch_stickers(id: &str, sticker_type: &str) -> Result<StickersResponse, String> {
    let url = format!(
        "{}/api/a029/wb-supply/{}/stickers?type={}",
        api_base(),
        id,
        sticker_type
    );
    let response = Request::get(&url)
        .send()
        .await
        .map_err(|e| format!("Ошибка сети: {}", e))?;

    if response.status() != 200 {
        let body = response.text().await.unwrap_or_default();
        return Err(format!("Ошибка сервера {}: {}", response.status(), body));
    }

    response
        .json::<StickersResponse>()
        .await
        .map_err(|e| format!("Ошибка разбора ответа: {}", e))
}

pub async fn fetch_raw_json(raw_payload_ref: &str) -> Result<String, String> {
    let url = format!("{}/api/a029/raw/{}", api_base(), raw_payload_ref);
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
        .map_err(|e| format!("Failed to read: {}", e))?;
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
        .map_err(|e| format!("Failed to read: {}", e))?;
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
