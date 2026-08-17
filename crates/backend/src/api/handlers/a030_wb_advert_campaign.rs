use crate::shared::error::ApiError;
use axum::{
    extract::{Path, Query},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::domain::a030_wb_advert_campaign;

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    pub connection_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct WbAdvertCampaignListItemDto {
    pub id: String,
    pub advert_id: i64,
    pub description: String,
    pub connection_id: String,
    pub organization_id: String,
    pub marketplace_id: String,
    pub campaign_type: Option<i32>,
    pub status: Option<i32>,
    pub nm_count: i32,
    pub change_time: Option<String>,
    pub fetched_at: String,
    /// Сумма начислений из p913 (advert_clicks_order_accrual) — связано с этой кампанией.
    #[serde(default)]
    pub p913_accrual: f64,
    /// Сумма списаний из p913 (advert_clicks_order_expense) по этой кампании.
    #[serde(default)]
    pub p913_writeoff: f64,
    /// Остаток = accrual - writeoff.
    #[serde(default)]
    pub p913_balance: f64,
    /// Реализация: сумма заказов a012 (finished_price), к которым привязаны расходы кампании.
    #[serde(default)]
    pub p913_realization: f64,
    /// Первая дата документа a026 по этой кампании.
    pub report_date_from: Option<String>,
    /// Последняя дата документа a026 по этой кампании.
    pub report_date_to: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct WbAdvertCampaignDetailsDto {
    pub id: String,
    pub code: String,
    pub description: String,
    pub advert_id: i64,
    pub connection_id: String,
    pub organization_id: String,
    pub marketplace_id: String,
    pub campaign_type: Option<i32>,
    pub status: Option<i32>,
    pub change_time: Option<String>,
    pub fetched_at: String,
    pub info_json: serde_json::Value,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize)]
pub struct NmPositionDto {
    pub nm_id: i64,
    pub article: Option<String>,
    pub name: Option<String>,
    /// All per-nm fields from info_json as raw JSON
    pub nm_data: serde_json::Value,
}

pub async fn list(
    Query(query): Query<ListQuery>,
) -> Result<Json<Vec<WbAdvertCampaignListItemDto>>, ApiError> {
    let items = if let Some(connection_id) = query.connection_id.filter(|v| !v.trim().is_empty()) {
        a030_wb_advert_campaign::service::list_by_connection(&connection_id).await
    } else {
        a030_wb_advert_campaign::service::list_all().await
    };
    let items = items.map_err(|e| {
        tracing::error!("Failed to list a030_wb_advert_campaign: {}", e);
        ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
    })?;

    let p913_aggregates =
        crate::projections::p913_wb_advert_order_attr::repository::aggregate_by_campaign()
            .await
            .unwrap_or_else(|e| {
                tracing::warn!("Failed to aggregate p913 for a030 list: {}", e);
                std::collections::HashMap::new()
            });

    let date_ranges = crate::domain::a026_wb_advert_daily::repository::date_range_by_campaign()
        .await
        .unwrap_or_else(|e| {
            tracing::warn!("Failed to aggregate a026 date range for a030 list: {}", e);
            std::collections::HashMap::new()
        });

    Ok(Json(
        items
            .into_iter()
            .map(|item| {
                let key = item.header.advert_id.to_string();
                let (accrual, writeoff, realization) = p913_aggregates
                    .get(&key)
                    .copied()
                    .unwrap_or((0.0, 0.0, 0.0));
                let (date_from, date_to) = date_ranges
                    .get(&(item.header.connection_id.clone(), item.header.advert_id))
                    .cloned()
                    .map(|(a, b)| (Some(a), Some(b)))
                    .unwrap_or((None, None));
                WbAdvertCampaignListItemDto {
                    id: item.base.id.value().to_string(),
                    advert_id: item.header.advert_id,
                    description: item.base.description,
                    connection_id: item.header.connection_id,
                    organization_id: item.header.organization_id,
                    marketplace_id: item.header.marketplace_id,
                    campaign_type: item.header.campaign_type,
                    status: item.header.status,
                    nm_count: item.header.nm_count,
                    change_time: item.header.change_time,
                    fetched_at: item.source_meta.fetched_at,
                    p913_accrual: accrual,
                    p913_writeoff: writeoff,
                    p913_balance: accrual - writeoff,
                    p913_realization: realization,
                    report_date_from: date_from,
                    report_date_to: date_to,
                }
            })
            .collect(),
    ))
}

pub async fn get_by_id(
    Path(id): Path<String>,
) -> Result<Json<WbAdvertCampaignDetailsDto>, ApiError> {
    let id = Uuid::parse_str(&id).map_err(|_| StatusCode::BAD_REQUEST)?;
    let item = a030_wb_advert_campaign::service::get_by_id(id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to get a030_wb_advert_campaign: {}", e);
            ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        })?
        .ok_or(ApiError::from(axum::http::StatusCode::NOT_FOUND))?;

    Ok(Json(WbAdvertCampaignDetailsDto {
        id: item.base.id.value().to_string(),
        code: item.base.code,
        description: item.base.description,
        advert_id: item.header.advert_id,
        connection_id: item.header.connection_id,
        organization_id: item.header.organization_id,
        marketplace_id: item.header.marketplace_id,
        campaign_type: item.header.campaign_type,
        status: item.header.status,
        change_time: item.header.change_time,
        fetched_at: item.source_meta.fetched_at,
        info_json: item.source_meta.info_json,
        created_at: item.base.metadata.created_at.to_rfc3339(),
        updated_at: item.base.metadata.updated_at.to_rfc3339(),
    }))
}

/// GET /api/a030/wb-advert-campaign/:id/nm-positions
/// Returns nm_id positions from info_json, enriched with article and product name from a007.
pub async fn nm_positions(Path(id): Path<String>) -> Result<Json<Vec<NmPositionDto>>, ApiError> {
    let uuid = Uuid::parse_str(&id).map_err(|_| StatusCode::BAD_REQUEST)?;
    let item = a030_wb_advert_campaign::service::get_by_id(uuid)
        .await
        .map_err(|e| {
            tracing::error!("Failed to get campaign for nm_positions: {}", e);
            ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        })?
        .ok_or(ApiError::from(axum::http::StatusCode::NOT_FOUND))?;

    let connection_id = item.header.connection_id.clone();
    let nm_entries = extract_nm_entries(&item.source_meta.info_json);

    let mut positions: Vec<NmPositionDto> = Vec::with_capacity(nm_entries.len());
    for (nm_id, nm_data) in nm_entries {
        let sku = nm_id.to_string();
        let product = crate::domain::a007_marketplace_product::service::get_by_connection_and_sku(
            &connection_id,
            &sku,
        )
        .await
        .unwrap_or(None);

        positions.push(NmPositionDto {
            nm_id,
            article: product.as_ref().map(|p| p.article.clone()),
            name: product.as_ref().map(|p| p.base.description.clone()),
            nm_data,
        });
    }

    Ok(Json(positions))
}

// ── DTO for a026 stat rows returned from the campaign details endpoint ────────

#[derive(Debug, Serialize)]
pub struct AdvertDailyStatRow {
    pub id: String,
    pub document_no: String,
    pub document_date: String,
    pub lines_count: i32,
    pub total_views: i64,
    pub total_clicks: i64,
    pub total_orders: i64,
    pub total_sum: f64,
    pub total_sum_price: f64,
    pub is_posted: bool,
}

/// GET /api/a030/wb-advert-campaign/:id/advert-stats
/// Returns all a026_wb_advert_daily documents for this campaign in chronological order.
pub async fn advert_stats(
    Path(id): Path<String>,
) -> Result<Json<Vec<AdvertDailyStatRow>>, ApiError> {
    let uuid = Uuid::parse_str(&id).map_err(|_| StatusCode::BAD_REQUEST)?;
    let item = a030_wb_advert_campaign::service::get_by_id(uuid)
        .await
        .map_err(|e| {
            tracing::error!("Failed to get campaign for advert_stats: {}", e);
            ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        })?
        .ok_or(ApiError::from(axum::http::StatusCode::NOT_FOUND))?;

    let docs = crate::domain::a026_wb_advert_daily::service::list_by_advert_id(
        &item.header.connection_id,
        item.header.advert_id,
    )
    .await
    .map_err(|e| {
        tracing::error!(
            "Failed to list a026 docs for advert_id={}: {}",
            item.header.advert_id,
            e
        );
        ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
    })?;

    Ok(Json(
        docs.into_iter()
            .map(|d| AdvertDailyStatRow {
                id: d.base.id.value().to_string(),
                document_no: d.header.document_no,
                document_date: d.header.document_date,
                lines_count: d.lines.len() as i32,
                total_views: d.totals.views,
                total_clicks: d.totals.clicks,
                total_orders: d.totals.orders,
                total_sum: d.totals.sum,
                total_sum_price: d.totals.sum_price,
                is_posted: d.is_posted,
            })
            .collect(),
    ))
}

/// Extract (nm_id, raw_nm_object) pairs from the info_json of a campaign.
/// WB API places nm_ids in different locations depending on campaign type.
fn extract_nm_entries(info: &serde_json::Value) -> Vec<(i64, serde_json::Value)> {
    let mut result = Vec::new();

    // Format 0: nm_settings[] (snake_case, objects with nm_id field)
    if let Some(arr) = info.get("nm_settings").and_then(|v| v.as_array()) {
        for nm in arr {
            if let Some(nm_id) = nm.get("nm_id").and_then(|v| v.as_i64()) {
                result.push((nm_id, nm.clone()));
            }
        }
        if !result.is_empty() {
            return result;
        }
    }

    // Format 1: unitedParams[].nms[] (array of objects with nmId)
    if let Some(arr) = info.get("unitedParams").and_then(|v| v.as_array()) {
        for param in arr {
            if let Some(nms) = param.get("nms").and_then(|v| v.as_array()) {
                for nm in nms {
                    if let Some(nm_id) = nm.get("nmId").and_then(|v| v.as_i64()) {
                        result.push((nm_id, nm.clone()));
                    }
                }
            }
        }
        if !result.is_empty() {
            return result;
        }
    }

    // Format 2: params[].nms[] (array of objects with nmId)
    if let Some(arr) = info.get("params").and_then(|v| v.as_array()) {
        for param in arr {
            if let Some(nms) = param.get("nms").and_then(|v| v.as_array()) {
                for nm in nms {
                    if let Some(nm_id) = nm.get("nmId").and_then(|v| v.as_i64()) {
                        result.push((nm_id, nm.clone()));
                    } else if let Some(nm_id) = nm.as_i64() {
                        result.push((nm_id, serde_json::json!({"nmId": nm_id})));
                    }
                }
            }
        }
        if !result.is_empty() {
            return result;
        }
    }

    // Format 3: nm[] (top-level array of objects or numbers)
    if let Some(arr) = info.get("nm").and_then(|v| v.as_array()) {
        for nm in arr {
            if let Some(nm_id) = nm.get("nmId").and_then(|v| v.as_i64()) {
                result.push((nm_id, nm.clone()));
            } else if let Some(nm_id) = nm.as_i64() {
                result.push((nm_id, serde_json::json!({"nmId": nm_id})));
            }
        }
        if !result.is_empty() {
            return result;
        }
    }

    // Format 4: nmIds[] (top-level array of numbers)
    if let Some(arr) = info.get("nmIds").and_then(|v| v.as_array()) {
        for nm in arr {
            if let Some(nm_id) = nm.as_i64() {
                result.push((nm_id, serde_json::json!({"nmId": nm_id})));
            }
        }
    }

    result
}
