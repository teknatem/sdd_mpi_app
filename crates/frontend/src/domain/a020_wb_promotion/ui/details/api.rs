use crate::shared::api_utils::api_base;
use gloo_net::http::Request;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WbPromotionDetailDto {
    pub id: String,
    pub code: String,
    pub description: String,
    pub header: PromotionHeaderDto,
    pub data: PromotionDataDto,
    pub nomenclatures: Vec<PromotionNomenclatureDto>,
    pub source_meta: PromotionSourceMetaDto,
    pub metadata: PromotionMetadataDto,
    pub is_posted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromotionHeaderDto {
    pub document_no: String,
    pub connection_id: String,
    pub organization_id: String,
    pub marketplace_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromotionRangingDto {
    pub condition: Option<String>,
    pub participation_rate: Option<f64>,
    pub boost: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromotionDataDto {
    pub promotion_id: i64,
    pub name: String,
    pub description: Option<String>,
    #[serde(default)]
    pub advantages: Vec<String>,
    pub start_date_time: String,
    pub end_date_time: String,
    pub promotion_type: Option<String>,
    pub exception_products_count: Option<i32>,
    pub in_promo_action_total: Option<i32>,
    pub in_promo_action_leftovers: Option<i32>,
    pub not_in_promo_action_leftovers: Option<i32>,
    pub not_in_promo_action_total: Option<i32>,
    pub participation_percentage: Option<f64>,
    #[serde(default)]
    pub ranging: Vec<PromotionRangingDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromotionNomenclatureDto {
    pub nm_id: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromotionSourceMetaDto {
    pub raw_payload_ref: String,
    pub fetched_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromotionMetadataDto {
    pub created_at: String,
    pub updated_at: String,
    pub is_deleted: bool,
    pub is_posted: bool,
    pub version: i32,
}

pub async fn fetch_by_id(id: &str) -> Result<WbPromotionDetailDto, String> {
    let url = format!("{}/api/a020/wb-promotions/{}", api_base(), id);
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
    if raw_payload_ref.is_empty() {
        return Err("No raw payload ref".to_string());
    }
    let url = format!("{}/api/a020/raw/{}", api_base(), raw_payload_ref);
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
