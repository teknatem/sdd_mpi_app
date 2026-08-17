use crate::shared::error::ApiError;
use axum::{
    extract::{Path, Query},
    Json,
};
use contracts::domain::a036_wb_sales_funnel_daily::aggregate::{
    WbSalesFunnelDaily, WbSalesFunnelDailyMetrics,
};
use contracts::domain::common::AggregateId;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

use crate::domain::a036_wb_sales_funnel_daily;
use crate::domain::a036_wb_sales_funnel_daily::repository::{
    WbSalesFunnelDailyListQuery, WbSalesFunnelDailyListRow,
};

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    pub date_from: Option<String>,
    pub date_to: Option<String>,
    pub connection_id: Option<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
    pub search_query: Option<String>,
    pub sort_by: Option<String>,
    pub sort_desc: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct WbSalesFunnelDailyListItemDto {
    pub id: String,
    pub document_no: String,
    pub document_date: String,
    pub currency: String,
    pub lines_count: i32,
    pub total_open_count: i64,
    pub total_cart_count: i64,
    pub total_order_count: i64,
    pub total_order_sum: f64,
    pub total_buyout_count: i64,
    pub total_buyout_sum: f64,
    /// `null` — счётчика отмен в источнике не было (N/A ≠ 0).
    pub total_cancel_count: Option<i64>,
    pub total_cancel_sum: Option<f64>,
    pub connection_id: String,
    pub connection_name: Option<String>,
    pub organization_name: Option<String>,
    pub fetched_at: String,
}

impl From<WbSalesFunnelDailyListRow> for WbSalesFunnelDailyListItemDto {
    fn from(row: WbSalesFunnelDailyListRow) -> Self {
        Self {
            id: row.id,
            document_no: row.document_no,
            document_date: row.document_date,
            currency: row.currency,
            lines_count: row.lines_count,
            total_open_count: row.total_open_count,
            total_cart_count: row.total_cart_count,
            total_order_count: row.total_order_count,
            total_order_sum: row.total_order_sum,
            total_buyout_count: row.total_buyout_count,
            total_buyout_sum: row.total_buyout_sum,
            total_cancel_count: row.total_cancel_count,
            total_cancel_sum: row.total_cancel_sum,
            connection_id: row.connection_id,
            connection_name: row.connection_name,
            organization_name: row.organization_name,
            fetched_at: row.fetched_at,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct PaginatedResponse {
    pub items: Vec<WbSalesFunnelDailyListItemDto>,
    pub total: usize,
    pub page: usize,
    pub page_size: usize,
    pub total_pages: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct WbSalesFunnelDailyLineDetailsDto {
    pub nm_id: i64,
    pub title: String,
    pub vendor_code: String,
    pub brand_name: String,
    pub subject_id: i64,
    pub subject_name: String,
    pub marketplace_product_ref: Option<String>,
    pub nomenclature_ref: Option<String>,
    pub nomenclature_article: Option<String>,
    pub nomenclature_name: Option<String>,
    pub metrics: WbSalesFunnelDailyMetrics,
}

#[derive(Debug, Clone, Serialize)]
pub struct WbSalesFunnelDailyDetailsDto {
    pub id: String,
    pub document_no: String,
    pub document_date: String,
    pub connection_id: String,
    pub connection_name: Option<String>,
    pub organization_id: String,
    pub organization_name: Option<String>,
    pub marketplace_id: String,
    pub marketplace_name: Option<String>,
    pub currency: String,
    pub totals: WbSalesFunnelDailyMetrics,
    pub source: String,
    pub fetched_at: String,
    pub created_at: String,
    pub updated_at: String,
    pub lines: Vec<WbSalesFunnelDailyLineDetailsDto>,
}

pub async fn list_paginated(
    Query(query): Query<ListQuery>,
) -> Result<Json<PaginatedResponse>, ApiError> {
    let page_size = query.limit.unwrap_or(100);
    let offset = query.offset.unwrap_or(0);
    let page = if page_size > 0 { offset / page_size } else { 0 };

    let list_query = WbSalesFunnelDailyListQuery {
        date_from: query.date_from,
        date_to: query.date_to,
        connection_id: query.connection_id,
        search_query: query.search_query,
        sort_by: query.sort_by.unwrap_or_else(|| "document_date".to_string()),
        sort_desc: query.sort_desc.unwrap_or(true),
        limit: page_size,
        offset,
    };

    match a036_wb_sales_funnel_daily::service::list_paginated(list_query).await {
        Ok(result) => {
            let total_pages = if page_size > 0 {
                (result.total + page_size - 1) / page_size
            } else {
                1
            };
            Ok(Json(PaginatedResponse {
                items: result.items.into_iter().map(Into::into).collect(),
                total: result.total,
                page,
                page_size,
                total_pages,
            }))
        }
        Err(e) => {
            tracing::error!("Failed to list WB sales funnel documents: {}", e);
            Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into())
        }
    }
}

#[derive(Debug, Serialize)]
pub struct WbSalesFunnelExportRowDto {
    pub document_date: String,
    pub nm_id: i64,
    pub title: String,
    pub vendor_code: String,
    pub brand_name: String,
    pub subject_name: String,
    pub nomenclature_article: Option<String>,
    pub metrics: WbSalesFunnelDailyMetrics,
}

/// Позиции всех документов под текущими фильтрами списка (без пагинации) —
/// источник CSV-выгрузки за период.
pub async fn export_lines(
    Query(query): Query<ListQuery>,
) -> Result<Json<Vec<WbSalesFunnelExportRowDto>>, ApiError> {
    let list_query = WbSalesFunnelDailyListQuery {
        date_from: query.date_from,
        date_to: query.date_to,
        connection_id: query.connection_id,
        search_query: query.search_query,
        sort_by: "document_date".to_string(),
        sort_desc: false,
        limit: 0,
        offset: 0,
    };

    match a036_wb_sales_funnel_daily::service::export_rows(list_query).await {
        Ok(rows) => Ok(Json(
            rows.into_iter()
                .map(|row| WbSalesFunnelExportRowDto {
                    document_date: row.document_date,
                    nm_id: row.nm_id,
                    title: row.title,
                    vendor_code: row.vendor_code,
                    brand_name: row.brand_name,
                    subject_name: row.subject_name,
                    nomenclature_article: row.nomenclature_article,
                    metrics: row.metrics,
                })
                .collect(),
        )),
        Err(e) => {
            tracing::error!("Failed to export WB sales funnel lines: {}", e);
            Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into())
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct ProductMetricsQuery {
    pub connection_id: String,
    pub date_from: String,
    pub date_to: String,
}

#[derive(Debug, Serialize)]
pub struct ProductMetricsDto {
    pub nm_id: i64,
    pub open_count: i64,
    pub cart_count: i64,
    pub order_count: i64,
}

/// Сумма метрик воронки по nm_id за период (для колонок в снимке товаров a037).
pub async fn get_product_metrics(
    Query(q): Query<ProductMetricsQuery>,
) -> Result<Json<Vec<ProductMetricsDto>>, ApiError> {
    match a036_wb_sales_funnel_daily::service::product_metrics_sum(
        &q.connection_id,
        &q.date_from,
        &q.date_to,
    )
    .await
    {
        Ok(rows) => Ok(Json(
            rows.into_iter()
                .map(|r| ProductMetricsDto {
                    nm_id: r.nm_id,
                    open_count: r.open_count,
                    cart_count: r.cart_count,
                    order_count: r.order_count,
                })
                .collect(),
        )),
        Err(e) => {
            tracing::error!("Failed to get WB funnel product metrics: {}", e);
            Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into())
        }
    }
}

#[derive(Debug, Serialize)]
pub struct BackfillFunnelResponse {
    pub inserted: usize,
}

/// Разовый бэкфилл стадии 1 универсальной воронки p916 из сохранённых документов a036.
pub async fn rebuild_funnel_projection() -> Result<Json<BackfillFunnelResponse>, ApiError> {
    match a036_wb_sales_funnel_daily::service::backfill_stage1_funnel().await {
        Ok(inserted) => Ok(Json(BackfillFunnelResponse { inserted })),
        Err(e) => {
            tracing::error!("Failed to backfill p916 stage-1 from a036: {}", e);
            Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into())
        }
    }
}

pub async fn get_by_id(
    Path(id): Path<String>,
) -> Result<Json<WbSalesFunnelDailyDetailsDto>, ApiError> {
    let uuid = Uuid::parse_str(&id).map_err(|_| axum::http::StatusCode::BAD_REQUEST)?;
    let doc = match a036_wb_sales_funnel_daily::service::get_by_id(uuid).await {
        Ok(Some(doc)) => doc,
        Ok(None) => return Err(axum::http::StatusCode::NOT_FOUND.into()),
        Err(e) => {
            tracing::error!("Failed to get WB sales funnel document {}: {}", id, e);
            return Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into());
        }
    };

    match build_details_dto(doc).await {
        Ok(dto) => Ok(Json(dto)),
        Err(e) => {
            tracing::error!("Failed to enrich WB sales funnel document {}: {}", id, e);
            Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into())
        }
    }
}

/// Проведение документа a036: пересобрать его движения воронки p916 (стадия 1).
pub async fn post(Path(id): Path<String>) -> Result<Json<serde_json::Value>, ApiError> {
    let uuid = Uuid::parse_str(&id).map_err(|_| axum::http::StatusCode::BAD_REQUEST)?;

    a036_wb_sales_funnel_daily::service::post_document(uuid)
        .await
        .map_err(|e| {
            tracing::error!("Failed to post a036 document {}: {}", id, e);
            ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        })?;

    Ok(Json(
        serde_json::json!({"success": true, "message": "Document posted"}),
    ))
}

/// Движения воронки p916, которые документ a036 порождает при импорте (маркетинговая
/// стадия: переходы/корзина/заказы-воронки). Закладка «Проекции» показывает JSON,
/// соответствующий записанным данным. registrator_ref = id документа.
pub async fn get_projections(Path(id): Path<String>) -> Result<Json<serde_json::Value>, ApiError> {
    let uuid = Uuid::parse_str(&id).map_err(|_| axum::http::StatusCode::BAD_REQUEST)?;
    let raw_ref = uuid.to_string();

    let p916_items =
        crate::projections::p916_mp_sales_funnel_turnovers::repository::list_by_registrator(
            "a036_wb_sales_funnel_daily",
            &raw_ref,
        )
        .await
        .map_err(|e| {
            tracing::error!("Failed to get p916 projections for {}: {}", id, e);
            ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        })?;

    Ok(Json(serde_json::json!({
        "p916_mp_sales_funnel_turnovers": p916_items
    })))
}

async fn build_details_dto(
    doc: WbSalesFunnelDaily,
) -> anyhow::Result<WbSalesFunnelDailyDetailsDto> {
    let connection_name = resolve_connection_name(&doc.header.connection_id).await?;
    let organization_name = resolve_organization_name(&doc.header.organization_id).await?;
    let marketplace_name = resolve_marketplace_name(&doc.header.marketplace_id).await?;

    let mut nomenclature_cache: HashMap<String, (Option<String>, Option<String>)> = HashMap::new();
    for line in &doc.lines {
        let Some(nom_ref) = line.nomenclature_ref.as_ref() else {
            continue;
        };
        if nomenclature_cache.contains_key(nom_ref) {
            continue;
        }

        let Some(uuid) = parse_uuid(nom_ref) else {
            nomenclature_cache.insert(nom_ref.clone(), (None, None));
            continue;
        };

        let nomenclature = crate::domain::a004_nomenclature::service::get_by_id(uuid).await?;
        let cached = nomenclature.map_or((None, None), |nom| {
            (Some(nom.article), Some(nom.base.description))
        });
        nomenclature_cache.insert(nom_ref.clone(), cached);
    }

    // Resolve a007 marketplace product ref per nm_id (read-only; для гиперссылки nmID).
    let mut product_ref_cache: HashMap<i64, Option<String>> = HashMap::new();
    for line in &doc.lines {
        if line.nm_id <= 0 || product_ref_cache.contains_key(&line.nm_id) {
            continue;
        }
        let article = line
            .nomenclature_ref
            .as_ref()
            .and_then(|nom_ref| nomenclature_cache.get(nom_ref).cloned())
            .and_then(|(article, _)| article);
        let product_ref =
            crate::domain::a007_marketplace_product::service::resolve_marketplace_product_ref(
                &doc.header.connection_id,
                &line.nm_id.to_string(),
                article.as_deref(),
            )
            .await
            .unwrap_or(None);
        product_ref_cache.insert(line.nm_id, product_ref);
    }

    let lines = doc
        .lines
        .iter()
        .map(|line| {
            let (article, name) = line
                .nomenclature_ref
                .as_ref()
                .and_then(|nom_ref| nomenclature_cache.get(nom_ref).cloned())
                .unwrap_or((None, None));

            WbSalesFunnelDailyLineDetailsDto {
                nm_id: line.nm_id,
                title: line.title.clone(),
                vendor_code: line.vendor_code.clone(),
                brand_name: line.brand_name.clone(),
                subject_id: line.subject_id,
                subject_name: line.subject_name.clone(),
                marketplace_product_ref: product_ref_cache.get(&line.nm_id).cloned().flatten(),
                nomenclature_ref: line.nomenclature_ref.clone(),
                nomenclature_article: article,
                nomenclature_name: name,
                metrics: line.metrics.clone(),
            }
        })
        .collect();

    Ok(WbSalesFunnelDailyDetailsDto {
        id: doc.base.id.as_string(),
        document_no: doc.header.document_no.clone(),
        document_date: doc.header.document_date.clone(),
        connection_id: doc.header.connection_id.clone(),
        connection_name,
        organization_id: doc.header.organization_id.clone(),
        organization_name,
        marketplace_id: doc.header.marketplace_id.clone(),
        marketplace_name,
        currency: doc.header.currency.clone(),
        totals: doc.totals.clone(),
        source: doc.source_meta.source.clone(),
        fetched_at: doc.source_meta.fetched_at.clone(),
        created_at: doc.base.metadata.created_at.to_rfc3339(),
        updated_at: doc.base.metadata.updated_at.to_rfc3339(),
        lines,
    })
}

async fn resolve_connection_name(connection_id: &str) -> anyhow::Result<Option<String>> {
    let Some(uuid) = parse_uuid(connection_id) else {
        return Ok(None);
    };
    let connection = crate::domain::a006_connection_mp::service::get_by_id(uuid).await?;
    Ok(connection.map(|item| item.base.description))
}

async fn resolve_organization_name(organization_id: &str) -> anyhow::Result<Option<String>> {
    let Some(uuid) = parse_uuid(organization_id) else {
        return Ok(None);
    };
    let organization = crate::domain::a002_organization::service::get_by_id(
        crate::shared::data::db::get_connection(),
        uuid,
    )
    .await?;
    Ok(organization.map(|item| item.base.description))
}

async fn resolve_marketplace_name(marketplace_id: &str) -> anyhow::Result<Option<String>> {
    let Some(uuid) = parse_uuid(marketplace_id) else {
        return Ok(None);
    };
    let marketplace = crate::domain::a005_marketplace::service::get_by_id(uuid).await?;
    Ok(marketplace.map(|item| item.base.description))
}

fn parse_uuid(value: &str) -> Option<Uuid> {
    Uuid::parse_str(value).ok()
}
