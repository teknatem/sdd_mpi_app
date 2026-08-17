use crate::shared::error::ApiError;
use axum::{
    extract::{Path, Query},
    Json,
};
use contracts::domain::a035_ym_settlement_recon::aggregate::{ReconLine, YmSettlementRecon};
use contracts::domain::common::AggregateId;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::domain::a035_ym_settlement_recon;
use crate::domain::a035_ym_settlement_recon::repository::{ReconListQuery, ReconListRow};

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
pub struct ReconListItemDto {
    pub id: String,
    pub bank_order_id: i64,
    pub bank_order_date: String,
    pub period_from: String,
    pub period_to: String,
    pub connection_id: String,
    pub connection_name: Option<String>,
    pub organization_name: Option<String>,
    pub bank_sum: f64,
    pub theoretical_sum: f64,
    pub deviation: f64,
    pub rows_count: i64,
    pub model: String,
    pub is_posted: bool,
}

impl From<ReconListRow> for ReconListItemDto {
    fn from(row: ReconListRow) -> Self {
        Self {
            id: row.id,
            bank_order_id: row.bank_order_id,
            bank_order_date: row.bank_order_date,
            period_from: row.period_from,
            period_to: row.period_to,
            connection_id: row.connection_id,
            connection_name: row.connection_name,
            organization_name: row.organization_name,
            bank_sum: row.bank_sum,
            theoretical_sum: row.theoretical_sum,
            deviation: row.deviation,
            rows_count: row.rows_count,
            model: row.model,
            is_posted: row.is_posted,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct PaginatedResponse {
    pub items: Vec<ReconListItemDto>,
    pub total: usize,
    pub page: usize,
    pub page_size: usize,
    pub total_pages: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReconDetailsDto {
    pub id: String,
    pub bank_order_id: i64,
    pub bank_order_date: String,
    pub period_from: String,
    pub period_to: String,
    pub connection_id: String,
    pub connection_name: Option<String>,
    pub organization_id: String,
    pub organization_name: Option<String>,
    pub marketplace_id: String,
    pub bank_sum: f64,
    pub theoretical_sum: f64,
    pub deviation: f64,
    pub model: String,
    pub is_posted: bool,
    pub created_at: String,
    pub updated_at: String,
    pub lines: Vec<ReconLine>,
    /// Перечень заказов, попавших в ордер: дата заказа и сумма перечисления.
    pub orders: Vec<ReconOrderDto>,
}

/// Заказ в составе банковского ордера (для карточки сверки).
#[derive(Debug, Clone, Serialize)]
pub struct ReconOrderDto {
    pub order_id: i64,
    /// UUID агрегата a013 для гиперссылки (пусто, если заказ не загружен).
    pub ym_order_id: Option<String>,
    pub status: Option<String>,
    pub order_date: String,
    pub amount: f64,
    pub rows_count: i64,
}

pub async fn list_paginated(
    Query(query): Query<ListQuery>,
) -> Result<Json<PaginatedResponse>, ApiError> {
    let page_size = query.limit.unwrap_or(500);
    let offset = query.offset.unwrap_or(0);
    let page = if page_size > 0 { offset / page_size } else { 0 };

    let list_query = ReconListQuery {
        date_from: query.date_from,
        date_to: query.date_to,
        connection_id: query.connection_id,
        search_query: query.search_query,
        sort_by: query
            .sort_by
            .unwrap_or_else(|| "bank_order_date".to_string()),
        sort_desc: query.sort_desc.unwrap_or(true),
        limit: page_size,
        offset,
    };

    match a035_ym_settlement_recon::service::list_paginated(list_query).await {
        Ok(result) => {
            let total_pages = if page_size > 0 {
                (result.total + page_size - 1) / page_size
            } else {
                1
            };
            let items = result.items.into_iter().map(Into::into).collect();
            Ok(Json(PaginatedResponse {
                items,
                total: result.total,
                page,
                page_size,
                total_pages,
            }))
        }
        Err(e) => {
            tracing::error!("Failed to list YM settlement recon documents: {}", e);
            Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into())
        }
    }
}

pub async fn get_by_id(Path(id): Path<String>) -> Result<Json<ReconDetailsDto>, ApiError> {
    let uuid = Uuid::parse_str(&id).map_err(|_| axum::http::StatusCode::BAD_REQUEST)?;
    let doc = match a035_ym_settlement_recon::service::get_by_id(uuid).await {
        Ok(Some(doc)) => doc,
        Ok(None) => return Err(axum::http::StatusCode::NOT_FOUND.into()),
        Err(e) => {
            tracing::error!("Failed to get YM settlement recon document {}: {}", id, e);
            return Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into());
        }
    };
    match build_details_dto(doc).await {
        Ok(dto) => Ok(Json(dto)),
        Err(e) => {
            tracing::error!(
                "Failed to enrich YM settlement recon document {}: {}",
                id,
                e
            );
            Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into())
        }
    }
}

async fn build_details_dto(doc: YmSettlementRecon) -> anyhow::Result<ReconDetailsDto> {
    let connection_name = resolve_connection_name(&doc.header.connection_id).await?;
    let organization_name = resolve_organization_name(&doc.header.organization_id).await?;
    let model = a035_ym_settlement_recon::repository::order_models(
        &doc.header.connection_id,
        doc.header.bank_order_id,
    )
    .await
    .unwrap_or_default();
    let orders = a035_ym_settlement_recon::repository::order_breakdown(
        &doc.header.connection_id,
        doc.header.bank_order_id,
    )
    .await
    .unwrap_or_default()
    .into_iter()
    .map(|o| ReconOrderDto {
        order_id: o.order_id,
        ym_order_id: o.ym_order_id,
        status: o.status,
        order_date: o.order_date,
        amount: o.amount,
        rows_count: o.rows_count,
    })
    .collect();

    Ok(ReconDetailsDto {
        id: doc.base.id.as_string(),
        bank_order_id: doc.header.bank_order_id,
        bank_order_date: doc.header.bank_order_date.clone(),
        period_from: doc.header.period_from.clone(),
        period_to: doc.header.period_to.clone(),
        connection_id: doc.header.connection_id.clone(),
        connection_name,
        organization_id: doc.header.organization_id.clone(),
        organization_name,
        marketplace_id: doc.header.marketplace_id.clone(),
        bank_sum: doc.totals.bank_sum,
        theoretical_sum: doc.totals.theoretical_sum,
        deviation: doc.totals.deviation,
        model,
        is_posted: doc.base.metadata.is_posted,
        created_at: doc.base.metadata.created_at.to_rfc3339(),
        updated_at: doc.base.metadata.updated_at.to_rfc3339(),
        lines: doc.lines,
        orders,
    })
}

async fn resolve_connection_name(connection_id: &str) -> anyhow::Result<Option<String>> {
    let Some(uuid) = Uuid::parse_str(connection_id).ok() else {
        return Ok(None);
    };
    let connection = crate::domain::a006_connection_mp::service::get_by_id(uuid).await?;
    Ok(connection.map(|item| item.base.description))
}

async fn resolve_organization_name(organization_id: &str) -> anyhow::Result<Option<String>> {
    let Some(uuid) = Uuid::parse_str(organization_id).ok() else {
        return Ok(None);
    };
    let organization = crate::domain::a002_organization::service::get_by_id(
        crate::shared::data::db::get_connection(),
        uuid,
    )
    .await?;
    Ok(organization.map(|item| item.base.description))
}

#[derive(Debug, Deserialize)]
pub struct GenerateRequest {
    #[serde(default)]
    pub date_from: Option<String>,
    #[serde(default)]
    pub date_to: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct GenerateResponse {
    pub created: usize,
    pub updated: usize,
    pub total: usize,
}

/// Команда «Сформировать ордера»: найти все банковские ордера (за период, если
/// задан) и upsert-нуть документы сверки. Идемпотентна по (кабинет, ордер).
pub async fn generate(
    Json(req): Json<GenerateRequest>,
) -> Result<Json<GenerateResponse>, ApiError> {
    let date_from = req.date_from.unwrap_or_default();
    let date_to = req.date_to.unwrap_or_default();
    match a035_ym_settlement_recon::service::generate(&date_from, &date_to).await {
        Ok(result) => Ok(Json(GenerateResponse {
            created: result.created,
            updated: result.updated,
            total: result.created + result.updated,
        })),
        Err(e) => {
            tracing::error!("Failed to generate YM settlement recon documents: {}", e);
            Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into())
        }
    }
}

/// Пересчитать один документ из текущего p907 (кнопка «Обновить» в карточке).
pub async fn recompute(Path(id): Path<String>) -> Result<Json<ReconDetailsDto>, ApiError> {
    let uuid = Uuid::parse_str(&id).map_err(|_| axum::http::StatusCode::BAD_REQUEST)?;
    let doc = match a035_ym_settlement_recon::service::recompute(uuid).await {
        Ok(Some(doc)) => doc,
        Ok(None) => return Err(axum::http::StatusCode::NOT_FOUND.into()),
        Err(e) => {
            tracing::error!(
                "Failed to recompute YM settlement recon document {}: {}",
                id,
                e
            );
            return Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into());
        }
    };
    match build_details_dto(doc).await {
        Ok(dto) => Ok(Json(dto)),
        Err(e) => {
            tracing::error!("Failed to enrich recomputed document {}: {}", id, e);
            Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into())
        }
    }
}

/// Провести документ: записать события «Дата оплаты поставщику» в p915 по заказам ордера.
pub async fn post_document(Path(id): Path<String>) -> Result<Json<serde_json::Value>, ApiError> {
    let uuid = Uuid::parse_str(&id).map_err(|_| axum::http::StatusCode::BAD_REQUEST)?;
    a035_ym_settlement_recon::service::post_document(uuid)
        .await
        .map_err(|e| {
            tracing::error!("Failed to post YM settlement recon document {}: {}", id, e);
            ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        })?;
    Ok(Json(serde_json::json!({"success": true})))
}

/// Отменить проведение: удалить события supplier_payment этого ордера из p915.
pub async fn unpost_document(Path(id): Path<String>) -> Result<Json<serde_json::Value>, ApiError> {
    let uuid = Uuid::parse_str(&id).map_err(|_| axum::http::StatusCode::BAD_REQUEST)?;
    a035_ym_settlement_recon::service::unpost_document(uuid)
        .await
        .map_err(|e| {
            tracing::error!(
                "Failed to unpost YM settlement recon document {}: {}",
                id,
                e
            );
            ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        })?;
    Ok(Json(serde_json::json!({"success": true})))
}
