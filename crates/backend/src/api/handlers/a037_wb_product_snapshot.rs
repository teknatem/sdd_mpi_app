use crate::shared::error::ApiError;
use axum::{
    extract::{Path, Query},
    Json,
};
use contracts::domain::a037_wb_product_snapshot::aggregate::{
    WbProductSnapshot, WbProductSnapshotState,
};
use contracts::domain::common::AggregateId;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

use crate::domain::a037_wb_product_snapshot;
use crate::domain::a037_wb_product_snapshot::repository::{
    WbProductSnapshotListQuery, WbProductSnapshotListRow,
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
pub struct WbProductSnapshotListItemDto {
    pub id: String,
    pub document_no: String,
    pub document_date: String,
    pub lines_count: i32,
    pub total_stock_wb: i64,
    pub total_stock_mp: i64,
    pub total_balance_sum: f64,
    pub connection_id: String,
    pub connection_name: Option<String>,
    pub organization_name: Option<String>,
    pub fetched_at: String,
}

impl From<WbProductSnapshotListRow> for WbProductSnapshotListItemDto {
    fn from(row: WbProductSnapshotListRow) -> Self {
        Self {
            id: row.id,
            document_no: row.document_no,
            document_date: row.document_date,
            lines_count: row.lines_count,
            total_stock_wb: row.total_stock_wb,
            total_stock_mp: row.total_stock_mp,
            total_balance_sum: row.total_balance_sum,
            connection_id: row.connection_id,
            connection_name: row.connection_name,
            organization_name: row.organization_name,
            fetched_at: row.fetched_at,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct PaginatedResponse {
    pub items: Vec<WbProductSnapshotListItemDto>,
    pub total: usize,
    pub page: usize,
    pub page_size: usize,
    pub total_pages: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct WbProductSnapshotLineDetailsDto {
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
    pub state: WbProductSnapshotState,
}

#[derive(Debug, Clone, Serialize)]
pub struct WbProductSnapshotDetailsDto {
    pub id: String,
    pub document_no: String,
    pub document_date: String,
    pub connection_id: String,
    pub connection_name: Option<String>,
    pub organization_id: String,
    pub organization_name: Option<String>,
    pub marketplace_id: String,
    pub marketplace_name: Option<String>,
    pub total_stock_wb: i64,
    pub total_stock_mp: i64,
    pub total_balance_sum: f64,
    pub source: String,
    pub fetched_at: String,
    pub created_at: String,
    pub updated_at: String,
    pub lines: Vec<WbProductSnapshotLineDetailsDto>,
}

pub async fn list_paginated(
    Query(query): Query<ListQuery>,
) -> Result<Json<PaginatedResponse>, ApiError> {
    let page_size = query.limit.unwrap_or(100);
    let offset = query.offset.unwrap_or(0);
    let page = if page_size > 0 { offset / page_size } else { 0 };

    let list_query = WbProductSnapshotListQuery {
        date_from: query.date_from,
        date_to: query.date_to,
        connection_id: query.connection_id,
        search_query: query.search_query,
        sort_by: query.sort_by.unwrap_or_else(|| "document_date".to_string()),
        sort_desc: query.sort_desc.unwrap_or(true),
        limit: page_size,
        offset,
    };

    match a037_wb_product_snapshot::service::list_paginated(list_query).await {
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
            tracing::error!("Failed to list WB product snapshot documents: {}", e);
            Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into())
        }
    }
}

pub async fn get_by_id(
    Path(id): Path<String>,
) -> Result<Json<WbProductSnapshotDetailsDto>, ApiError> {
    let uuid = Uuid::parse_str(&id).map_err(|_| axum::http::StatusCode::BAD_REQUEST)?;
    let doc = match a037_wb_product_snapshot::service::get_by_id(uuid).await {
        Ok(Some(doc)) => doc,
        Ok(None) => return Err(axum::http::StatusCode::NOT_FOUND.into()),
        Err(e) => {
            tracing::error!("Failed to get WB product snapshot document {}: {}", id, e);
            return Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into());
        }
    };

    match build_details_dto(doc).await {
        Ok(dto) => Ok(Json(dto)),
        Err(e) => {
            tracing::error!(
                "Failed to enrich WB product snapshot document {}: {}",
                id,
                e
            );
            Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into())
        }
    }
}

async fn build_details_dto(doc: WbProductSnapshot) -> anyhow::Result<WbProductSnapshotDetailsDto> {
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

            WbProductSnapshotLineDetailsDto {
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
                state: line.state.clone(),
            }
        })
        .collect();

    Ok(WbProductSnapshotDetailsDto {
        id: doc.base.id.as_string(),
        document_no: doc.header.document_no.clone(),
        document_date: doc.header.snapshot_date.clone(),
        connection_id: doc.header.connection_id.clone(),
        connection_name,
        organization_id: doc.header.organization_id.clone(),
        organization_name,
        marketplace_id: doc.header.marketplace_id.clone(),
        marketplace_name,
        total_stock_wb: doc.totals.total_stock_wb,
        total_stock_mp: doc.totals.total_stock_mp,
        total_balance_sum: doc.totals.total_balance_sum,
        source: doc.source_meta.source.clone(),
        fetched_at: doc.source_meta.fetched_at.clone(),
        created_at: doc.base.metadata.created_at.to_rfc3339(),
        updated_at: doc.base.metadata.updated_at.to_rfc3339(),
        lines,
    })
}

// ── Динамика по товару ──────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct SeriesQuery {
    pub connection_id: String,
    pub nm_id: i64,
    pub date_from: String,
    pub date_to: String,
}

#[derive(Debug, Serialize)]
pub struct SeriesPointDto {
    pub date: String,
    pub stock_wb: i64,
    pub stock_mp: i64,
    pub stock_balance_sum: f64,
    pub product_rating: f64,
    pub feedback_rating: f64,
}

#[derive(Debug, Serialize)]
pub struct SeriesResponse {
    pub nm_id: i64,
    pub points: Vec<SeriesPointDto>,
}

pub async fn get_series(Query(q): Query<SeriesQuery>) -> Result<Json<SeriesResponse>, ApiError> {
    match a037_wb_product_snapshot::service::series_for_nm(
        &q.connection_id,
        q.nm_id,
        &q.date_from,
        &q.date_to,
    )
    .await
    {
        Ok(points) => Ok(Json(SeriesResponse {
            nm_id: q.nm_id,
            points: points
                .into_iter()
                .map(|p| SeriesPointDto {
                    date: p.date,
                    stock_wb: p.stock_wb,
                    stock_mp: p.stock_mp,
                    stock_balance_sum: p.stock_balance_sum,
                    product_rating: p.product_rating,
                    feedback_rating: p.feedback_rating,
                })
                .collect(),
        })),
        Err(e) => {
            tracing::error!("Failed to get WB product snapshot series: {}", e);
            Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into())
        }
    }
}

// ── Изменения рейтинга/оценки vs предыдущий снимок ──────────────────────────

#[derive(Debug, Deserialize)]
pub struct RatingChangesQuery {
    pub id: String,
}

#[derive(Debug, Serialize)]
pub struct RatingChangeDto {
    pub nm_id: i64,
    pub title: String,
    pub vendor_code: String,
    pub brand_name: String,
    pub nomenclature_article: Option<String>,
    pub marketplace_product_ref: Option<String>,
    pub product_rating_old: f64,
    pub product_rating_new: f64,
    pub product_rating_delta: f64,
    pub feedback_rating_old: f64,
    pub feedback_rating_new: f64,
    pub feedback_rating_delta: f64,
}

#[derive(Debug, Serialize)]
pub struct RatingChangesResponse {
    pub has_previous: bool,
    pub prev_date: Option<String>,
    pub prev_document_no: Option<String>,
    pub rows: Vec<RatingChangeDto>,
}

fn rating_changed(a: f64, b: f64) -> bool {
    (a - b).abs() > 1e-9
}

pub async fn get_rating_changes(
    Query(q): Query<RatingChangesQuery>,
) -> Result<Json<RatingChangesResponse>, ApiError> {
    let uuid = Uuid::parse_str(&q.id).map_err(|_| axum::http::StatusCode::BAD_REQUEST)?;
    let current = match a037_wb_product_snapshot::service::get_by_id(uuid).await {
        Ok(Some(doc)) => doc,
        Ok(None) => return Err(axum::http::StatusCode::NOT_FOUND.into()),
        Err(e) => {
            tracing::error!("rating_changes: failed to get current {}: {}", q.id, e);
            return Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into());
        }
    };

    let previous = match a037_wb_product_snapshot::service::previous_before(
        &current.header.connection_id,
        &current.header.snapshot_date,
    )
    .await
    {
        Ok(prev) => prev,
        Err(e) => {
            tracing::error!("rating_changes: failed to get previous: {}", e);
            return Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into());
        }
    };

    let Some(previous) = previous else {
        return Ok(Json(RatingChangesResponse {
            has_previous: false,
            prev_date: None,
            prev_document_no: None,
            rows: Vec::new(),
        }));
    };

    // Карта прошлого снимка: nm_id → (product_rating, feedback_rating).
    let prev_map: HashMap<i64, (f64, f64)> = previous
        .lines
        .iter()
        .map(|l| (l.nm_id, (l.state.product_rating, l.state.feedback_rating)))
        .collect();

    match build_rating_changes(&current, &prev_map).await {
        Ok(rows) => Ok(Json(RatingChangesResponse {
            has_previous: true,
            prev_date: Some(previous.header.snapshot_date.clone()),
            prev_document_no: Some(previous.header.document_no.clone()),
            rows,
        })),
        Err(e) => {
            tracing::error!("rating_changes: failed to build rows: {}", e);
            Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into())
        }
    }
}

async fn build_rating_changes(
    current: &WbProductSnapshot,
    prev_map: &HashMap<i64, (f64, f64)>,
) -> anyhow::Result<Vec<RatingChangeDto>> {
    let mut rows = Vec::new();
    for line in &current.lines {
        // Только позиции, присутствующие и в текущем, и в прошлом документе.
        let Some(&(prev_pr, prev_fr)) = prev_map.get(&line.nm_id) else {
            continue;
        };
        let pr = line.state.product_rating;
        let fr = line.state.feedback_rating;
        if !rating_changed(pr, prev_pr) && !rating_changed(fr, prev_fr) {
            continue;
        }

        let article = match line.nomenclature_ref.as_ref().and_then(|r| parse_uuid(r)) {
            Some(uuid) => crate::domain::a004_nomenclature::service::get_by_id(uuid)
                .await?
                .map(|nom| nom.article),
            None => None,
        };
        let marketplace_product_ref = if line.nm_id > 0 {
            crate::domain::a007_marketplace_product::service::resolve_marketplace_product_ref(
                &current.header.connection_id,
                &line.nm_id.to_string(),
                article.as_deref(),
            )
            .await
            .unwrap_or(None)
        } else {
            None
        };

        rows.push(RatingChangeDto {
            nm_id: line.nm_id,
            title: line.title.clone(),
            vendor_code: line.vendor_code.clone(),
            brand_name: line.brand_name.clone(),
            nomenclature_article: article,
            marketplace_product_ref,
            product_rating_old: prev_pr,
            product_rating_new: pr,
            product_rating_delta: pr - prev_pr,
            feedback_rating_old: prev_fr,
            feedback_rating_new: fr,
            feedback_rating_delta: fr - prev_fr,
        });
    }
    Ok(rows)
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
