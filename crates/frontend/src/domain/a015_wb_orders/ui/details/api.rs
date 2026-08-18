//! API layer for WB Orders details

use crate::shared::api_utils::api_base;
use contracts::projections::p903_wb_finance_report::dto::WbFinanceReportDto;
use gloo_net::http::Request;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WbOrderDetailDto {
    pub id: String,
    pub code: String,
    pub description: String,
    pub header: HeaderDto,
    pub line: LineDto,
    pub state: StateDto,
    pub warehouse: WarehouseDto,
    pub geography: GeographyDto,
    pub source_meta: SourceMetaDto,
    pub metadata: MetadataDto,
    pub marketplace_product_ref: Option<String>,
    pub nomenclature_ref: Option<String>,
    pub base_nomenclature_ref: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeaderDto {
    pub document_no: String,
    pub connection_id: String,
    pub organization_id: String,
    pub marketplace_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LineDto {
    pub line_id: String,
    pub supplier_article: String,
    pub nm_id: i64,
    pub barcode: String,
    pub category: Option<String>,
    pub subject: Option<String>,
    pub brand: Option<String>,
    pub tech_size: Option<String>,
    pub qty: f64,
    pub total_price: Option<f64>,
    pub discount_percent: Option<f64>,
    pub spp: Option<f64>,
    pub finished_price: Option<f64>,
    pub price_with_disc: Option<f64>,
    pub dealer_price_ut: Option<f64>,
    pub margin_pro: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateDto {
    pub order_dt: String,
    pub last_change_dt: Option<String>,
    pub is_cancel: bool,
    pub cancel_dt: Option<String>,
    pub is_supply: Option<bool>,
    pub is_realization: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WarehouseDto {
    pub warehouse_name: Option<String>,
    pub warehouse_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeographyDto {
    pub country_name: Option<String>,
    pub oblast_okrug_name: Option<String>,
    pub region_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceMetaDto {
    pub income_id: Option<i64>,
    pub sticker: Option<String>,
    pub g_number: Option<String>,
    pub raw_payload_ref: String,
    #[serde(default)]
    pub marketplace_raw_payload_ref: Option<String>,
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
pub struct MarketplaceProductInfo {
    pub description: String,
    pub article: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NomenclatureInfo {
    pub description: String,
    pub article: String,
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
pub struct MarketplaceInfo {
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WbSupplyLinkInfo {
    pub id: String,
    pub supply_id: String,
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WbSalesListItemDto {
    pub id: String,
    pub header: WbSalesHeaderDto,
    pub line: WbSalesLineDto,
    pub state: WbSalesStateDto,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WbSalesHeaderDto {
    pub document_no: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WbSalesLineDto {
    pub supplier_article: String,
    pub qty: f64,
    pub finished_price: Option<f64>,
    pub total_price: Option<f64>,
    pub discount_percent: Option<f64>,
    pub price_list: Option<f64>,
    pub discount_total: Option<f64>,
    pub price_effective: Option<f64>,
    pub spp: Option<f64>,
    pub payment_sale_amount: Option<f64>,
    pub amount_line: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WbSalesStateDto {
    pub sale_dt: String,
    pub event_type: String,
}

pub async fn fetch_by_id(id: &str) -> Result<WbOrderDetailDto, String> {
    let url = format!("{}/api/a015/wb-orders/{}", api_base(), id);
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

/// Движения проекций документа (p909 + p916) как raw JSON для закладки «Проекции».
pub async fn fetch_projections(id: &str) -> Result<serde_json::Value, String> {
    let url = format!("{}/api/a015/wb-orders/{}/projections", api_base(), id);
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
    serde_json::from_str(&text).map_err(|e| format!("Failed to parse JSON: {}", e))
}

pub async fn fetch_raw_json(raw_payload_ref: &str) -> Result<String, String> {
    let url = format!("{}/api/a015/raw/{}", api_base(), raw_payload_ref);
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

pub async fn fetch_finance_reports(srid: &str) -> Result<Vec<WbFinanceReportDto>, String> {
    let url = format!(
        "{}/api/p903/finance-report/search-by-srid?srid={}",
        api_base(),
        srid
    );
    let response = Request::get(&url)
        .send()
        .await
        .map_err(|e| format!("Failed to fetch finance reports: {}", e))?;

    if response.status() != 200 {
        return Err(format!("Server error: {}", response.status()));
    }

    response
        .json()
        .await
        .map_err(|e| format!("Failed to parse finance reports: {}", e))
}

pub async fn fetch_wb_sales(document_no: &str) -> Result<Vec<WbSalesListItemDto>, String> {
    let encoded_document_no = urlencoding::encode(document_no);
    let url = format!(
        "{}/api/a012/wb-sales/search-by-srid?srid={}",
        api_base(),
        encoded_document_no
    );
    let response = Request::get(&url)
        .send()
        .await
        .map_err(|e| format!("Failed to fetch wb sales: {}", e))?;

    if response.status() != 200 {
        return Err(format!("Server error: {}", response.status()));
    }

    response
        .json()
        .await
        .map_err(|e| format!("Failed to parse wb sales: {}", e))
}

pub async fn fetch_supply_for_order(order_id: &str) -> Result<Option<WbSupplyLinkInfo>, String> {
    let url = format!("{}/api/a029/wb-supply/by-order/{}", api_base(), order_id);
    let response = Request::get(&url)
        .send()
        .await
        .map_err(|e| format!("Failed to fetch supply link: {}", e))?;

    if response.status() == 404 {
        return Ok(None);
    }
    if response.status() != 200 {
        return Err(format!("Server error: {}", response.status()));
    }

    response
        .json()
        .await
        .map(Some)
        .map_err(|e| format!("Failed to parse supply link: {}", e))
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

pub async fn post_document(id: &str) -> Result<(), String> {
    let url = format!("{}/api/a015/wb-orders/{}/post", api_base(), id);
    let response = Request::post(&url)
        .send()
        .await
        .map_err(|e| format!("Failed to post document: {}", e))?;

    if response.status() != 200 {
        return Err(format!("Failed to post: status {}", response.status()));
    }

    Ok(())
}

#[allow(dead_code)]
pub async fn unpost_document(id: &str) -> Result<(), String> {
    let url = format!("{}/api/a015/wb-orders/{}/unpost", api_base(), id);
    let response = Request::post(&url)
        .send()
        .await
        .map_err(|e| format!("Failed to unpost document: {}", e))?;

    if response.status() != 200 {
        return Err(format!("Failed to unpost: status {}", response.status()));
    }

    Ok(())
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
