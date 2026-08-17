use crate::shared::error::ApiError;
use axum::{extract::Query, Json};
use contracts::domain::common::AggregateId;
use contracts::projections::p900_mp_sales_register::{
    SalesRegisterDetailDto, SalesRegisterDto, SalesRegisterListRequest, SalesRegisterListResponse,
    SalesRegisterStatsByDateRequest, SalesRegisterStatsByDateResponse,
    SalesRegisterStatsByMarketplaceResponse,
};
use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::projections::p900_mp_sales_register::{backfill, repository, service};

// Cache для организаций
static ORG_CACHE: Lazy<Arc<RwLock<Option<(std::time::Instant, HashMap<String, String>)>>>> =
    Lazy::new(|| Arc::new(RwLock::new(None)));

/// Получить кэш организаций (id -> description)
async fn get_org_map() -> HashMap<String, String> {
    let cache_ttl = std::time::Duration::from_secs(300); // 5 минут

    // Проверяем кэш
    {
        let cache = ORG_CACHE.read().await;
        if let Some((timestamp, map)) = cache.as_ref() {
            if timestamp.elapsed() < cache_ttl {
                return map.clone();
            }
        }
    }

    // Загружаем организации из БД
    let organizations = crate::domain::a002_organization::service::list_all(
        crate::shared::data::db::get_connection(),
    )
    .await
    .unwrap_or_default();

    let map: HashMap<String, String> = organizations
        .into_iter()
        .map(|org| (org.base.id.as_string(), org.base.description.clone()))
        .collect();

    // Обновляем кэш
    {
        let mut cache = ORG_CACHE.write().await;
        *cache = Some((std::time::Instant::now(), map.clone()));
    }

    map
}

/// Handler для получения списка продаж с фильтрами
pub async fn list_sales(
    Query(req): Query<SalesRegisterListRequest>,
) -> Result<Json<SalesRegisterListResponse>, ApiError> {
    let (items, total) = service::list_with_filters(
        &req.date_from,
        &req.date_to,
        req.marketplace,
        req.organization_ref,
        req.connection_mp_ref,
        req.status_norm,
        req.seller_sku,
        req.limit,
        req.offset,
    )
    .await
    .map_err(|e| {
        tracing::error!("Failed to list sales: {}", e);
        ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
    })?;

    // Получаем кэш организаций
    let org_map = get_org_map().await;

    let dtos: Vec<SalesRegisterDto> = items
        .into_iter()
        .map(|m| model_to_dto(m, &org_map))
        .collect();

    let has_more = total > (req.offset + dtos.len() as i32);

    Ok(Json(SalesRegisterListResponse {
        items: dtos,
        total_count: total,
        has_more,
    }))
}

/// Handler для получения детальной информации о продаже
pub async fn get_sale_detail(
    axum::extract::Path((marketplace, document_no, line_id)): axum::extract::Path<(
        String,
        String,
        String,
    )>,
) -> Result<Json<SalesRegisterDetailDto>, ApiError> {
    let item = service::get_by_id(&marketplace, &document_no, &line_id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to get sale detail: {}", e);
            ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        })?
        .ok_or(ApiError::from(axum::http::StatusCode::NOT_FOUND))?;

    // Получаем кэш организаций
    let org_map = get_org_map().await;
    let organization_name = org_map.get(&item.organization_ref).cloned();

    let dto = SalesRegisterDetailDto {
        sale: model_to_dto(item, &org_map),
        organization_name,
        connection_mp_name: None,
        marketplace_product_name: None,
    };

    Ok(Json(dto))
}

/// Handler для статистики по датам
pub async fn get_stats_by_date(
    Query(req): Query<SalesRegisterStatsByDateRequest>,
) -> Result<Json<SalesRegisterStatsByDateResponse>, ApiError> {
    let stats = service::calculate_daily_stats(&req.date_from, &req.date_to, req.marketplace)
        .await
        .map_err(|e| {
            tracing::error!("Failed to get stats by date: {}", e);
            ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        })?;

    Ok(Json(SalesRegisterStatsByDateResponse { data: stats }))
}

/// Handler для статистики по маркетплейсам
pub async fn get_stats_by_marketplace(
    Query(req): Query<SalesRegisterStatsByDateRequest>,
) -> Result<Json<SalesRegisterStatsByMarketplaceResponse>, ApiError> {
    let stats = service::calculate_marketplace_stats(&req.date_from, &req.date_to)
        .await
        .map_err(|e| {
            tracing::error!("Failed to get stats by marketplace: {}", e);
            ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        })?;

    Ok(Json(SalesRegisterStatsByMarketplaceResponse {
        data: stats,
    }))
}

/// Handler для запуска backfill marketplace_product_ref
pub async fn backfill_product_refs() -> Result<Json<serde_json::Value>, ApiError> {
    tracing::info!("Starting backfill of marketplace_product_ref");

    let stats = backfill::backfill_marketplace_product_refs()
        .await
        .map_err(|e| {
            tracing::error!("Backfill failed: {}", e);
            ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        })?;

    tracing::info!(
        "Backfill completed: total={}, updated={}, skipped={}, failed={}",
        stats.total_records,
        stats.records_updated,
        stats.records_skipped,
        stats.records_failed
    );

    Ok(Json(serde_json::json!({
        "success": true,
        "total_records": stats.total_records,
        "records_updated": stats.records_updated,
        "records_skipped": stats.records_skipped,
        "records_failed": stats.records_failed,
    })))
}

/// Преобразование Model в DTO
fn model_to_dto(model: repository::Model, org_map: &HashMap<String, String>) -> SalesRegisterDto {
    let organization_name = org_map.get(&model.organization_ref).cloned();

    SalesRegisterDto {
        marketplace: model.marketplace,
        document_no: model.document_no,
        line_id: model.line_id,
        scheme: model.scheme,
        document_type: model.document_type,
        document_version: model.document_version,
        connection_mp_ref: model.connection_mp_ref,
        organization_ref: model.organization_ref,
        organization_name,
        marketplace_product_ref: model.marketplace_product_ref,
        nomenclature_ref: model.nomenclature_ref,
        registrator_ref: model.registrator_ref,
        event_time_source: model.event_time_source,
        sale_date: model.sale_date,
        source_updated_at: model.source_updated_at,
        status_source: model.status_source,
        status_norm: model.status_norm,
        seller_sku: model.seller_sku,
        mp_item_id: model.mp_item_id,
        barcode: model.barcode,
        title: model.title,
        qty: model.qty,
        price_list: model.price_list,
        discount_total: model.discount_total,
        price_effective: model.price_effective,
        amount_line: model.amount_line,
        cost: model.cost,
        dealer_price_ut: model.dealer_price_ut,
        currency_code: model.currency_code,
        is_fact: model.is_fact,
        loaded_at_utc: model.loaded_at_utc,
        payload_version: model.payload_version,
        extra: model.extra,
    }
}

/// Handler для получения проекций по registrator_ref
pub async fn get_by_registrator(
    axum::extract::Path(registrator_ref): axum::extract::Path<String>,
) -> Result<Json<Vec<SalesRegisterDto>>, ApiError> {
    let items = service::get_by_registrator(&registrator_ref)
        .await
        .map_err(|e| {
            tracing::error!("Failed to get projections by registrator: {}", e);
            ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        })?;

    // Получаем кэш организаций
    let org_map = get_org_map().await;

    let dtos: Vec<SalesRegisterDto> = items
        .into_iter()
        .map(|m| model_to_dto(m, &org_map))
        .collect();

    Ok(Json(dtos))
}
