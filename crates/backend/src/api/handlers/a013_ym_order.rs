use crate::shared::error::ApiError;
use axum::{extract::Query, Json};
use chrono::NaiveDate;
use contracts::domain::a013_ym_order::aggregate::{YmOrder, YmOrderListDto};
use contracts::domain::common::AggregateId;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::domain::a013_ym_order;
use crate::shared::data::raw_storage;

/// Handler для получения списка Yandex Market Orders (full - с JSON parsing)
pub async fn list_orders() -> Result<Json<Vec<YmOrder>>, ApiError> {
    let items = a013_ym_order::service::list_all().await.map_err(|e| {
        tracing::error!("Failed to list Yandex Market orders: {}", e);
        ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
    })?;

    Ok(Json(items))
}

/// Query parameters для быстрого списка
#[derive(Debug, Deserialize)]
pub struct ListQueryParams {
    pub date_from: Option<String>,
    pub date_to: Option<String>,
    pub organization_id: Option<String>,
    pub search_document_no: Option<String>,
    pub status_norm: Option<String>,
    #[serde(default = "default_sort_by")]
    pub sort_by: String,
    #[serde(default = "default_sort_desc")]
    pub sort_desc: bool,
    #[serde(default = "default_limit")]
    pub limit: usize,
    #[serde(default)]
    pub offset: usize,
}

fn default_sort_by() -> String {
    "delivery_date".to_string()
}

fn default_sort_desc() -> bool {
    true
}

fn default_limit() -> usize {
    1000
}

/// Paginated response для списка
#[derive(Debug, Serialize)]
pub struct PaginatedYmOrderResponse {
    pub items: Vec<YmOrderListDto>,
    pub total: usize,
    pub page: usize,
    pub page_size: usize,
    pub total_pages: usize,
}

/// Handler для получения списка с серверной пагинацией
pub async fn list_orders_fast(
    Query(params): Query<ListQueryParams>,
) -> Result<Json<PaginatedYmOrderResponse>, ApiError> {
    let page_size = params.limit;
    let offset = params.offset;
    let page = if page_size > 0 { offset / page_size } else { 0 };

    let query = a013_ym_order::repository::YmOrderListQuery {
        date_from: params.date_from,
        date_to: params.date_to,
        organization_id: params.organization_id,
        search_document_no: params.search_document_no,
        status_norm: params.status_norm,
        sort_by: params.sort_by,
        sort_desc: params.sort_desc,
        limit: page_size,
        offset,
    };

    let result = a013_ym_order::repository::list_sql(query)
        .await
        .map_err(|e| {
            tracing::error!("Failed to list Yandex Market orders: {}", e);
            ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        })?;

    let total = result.total;
    let total_pages = if page_size > 0 {
        (total + page_size - 1) / page_size
    } else {
        0
    };

    // Load organizations for enrichment
    let organizations = crate::domain::a002_organization::service::list_all(
        crate::shared::data::db::get_connection(),
    )
    .await
    .map_err(|e| {
        tracing::error!("Failed to load organizations: {}", e);
        ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
    })?;

    let org_map: std::collections::HashMap<String, String> = organizations
        .into_iter()
        .map(|org| (org.base.id.as_string(), org.base.description.clone()))
        .collect();

    let items: Vec<YmOrderListDto> = result
        .items
        .into_iter()
        .map(|row| {
            let organization_name = row
                .organization_id
                .as_ref()
                .and_then(|org_id| org_map.get(org_id).cloned());

            YmOrderListDto {
                id: row.id,
                document_no: row.document_no,
                status_changed_at: row.status_changed_at.unwrap_or_default(),
                creation_date: row.creation_date.unwrap_or_default(),
                delivery_date: row.delivery_date.unwrap_or_default(),
                campaign_id: row.campaign_id.unwrap_or_default(),
                status_norm: row.status_norm.unwrap_or_default(),
                total_qty: row.total_qty.unwrap_or(0.0),
                total_amount: row.total_amount.unwrap_or(0.0),
                total_amount_api: row.total_amount_api,
                lines_count: row.lines_count.unwrap_or(0) as usize,
                delivery_total: row.delivery_total,
                subsidies_total: row.subsidies_total.unwrap_or(0.0),
                is_posted: row.is_posted,
                is_error: row.is_error,
                organization_name,
                total_dealer_amount: row.total_dealer_amount,
                margin_pro: row.margin_pro,
                realization_date: row.realization_date.unwrap_or_default(),
                payment_date: row.payment_date.unwrap_or_default(),
            }
        })
        .collect();

    Ok(Json(PaginatedYmOrderResponse {
        items,
        total,
        page,
        page_size,
        total_pages,
    }))
}

/// Handler для получения детальной информации о Yandex Market Order
pub async fn get_order_detail(
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<Json<YmOrder>, ApiError> {
    let uuid = Uuid::parse_str(&id).map_err(|_| axum::http::StatusCode::BAD_REQUEST)?;

    let item = a013_ym_order::service::get_by_id(uuid)
        .await
        .map_err(|e| {
            tracing::error!("Failed to get Yandex Market order detail: {}", e);
            ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        })?
        .ok_or(ApiError::from(axum::http::StatusCode::NOT_FOUND))?;

    Ok(Json(item))
}

/// Handler для получения raw JSON от Yandex Market API по raw_payload_ref
pub async fn get_raw_json(
    axum::extract::Path(ref_id): axum::extract::Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let json_value = raw_storage::get_json_value_by_ref(&ref_id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to get raw JSON: {}", e);
            ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        })?;

    Ok(Json(json_value))
}

/// Handler для проведения документа
pub async fn post_document(
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let uuid = Uuid::parse_str(&id).map_err(|_| axum::http::StatusCode::BAD_REQUEST)?;

    a013_ym_order::posting::post_document(uuid)
        .await
        .map_err(|e| {
            tracing::error!("Failed to post document: {}", e);
            ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        })?;

    Ok(Json(serde_json::json!({"success": true})))
}

/// Handler для отмены проведения документа
pub async fn unpost_document(
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let uuid = Uuid::parse_str(&id).map_err(|_| axum::http::StatusCode::BAD_REQUEST)?;

    a013_ym_order::posting::unpost_document(uuid)
        .await
        .map_err(|e| {
            tracing::error!("Failed to unpost document: {}", e);
            ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        })?;

    Ok(Json(serde_json::json!({"success": true})))
}

#[derive(Deserialize)]
pub struct PostPeriodRequest {
    pub from: String,
    pub to: String,
}

#[derive(Deserialize)]
pub struct BatchOperationRequest {
    pub ids: Vec<String>,
}

/// Handler для проведения документов за период
pub async fn post_period(
    Query(req): Query<PostPeriodRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let from = NaiveDate::parse_from_str(&req.from, "%Y-%m-%d")
        .map_err(|_| axum::http::StatusCode::BAD_REQUEST)?;
    let to = NaiveDate::parse_from_str(&req.to, "%Y-%m-%d")
        .map_err(|_| axum::http::StatusCode::BAD_REQUEST)?;

    let documents = a013_ym_order::service::list_all().await.map_err(|e| {
        tracing::error!("Failed to list documents: {}", e);
        ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
    })?;

    let mut posted_count = 0;
    let mut failed_count = 0;

    for doc in documents {
        let doc_date = doc.source_meta.fetched_at.date_naive();
        if doc_date >= from && doc_date <= to {
            match a013_ym_order::posting::post_document(doc.base.id.value()).await {
                Ok(_) => {
                    posted_count += 1;
                    tracing::info!("Posted document: {}", doc.base.id.as_string());
                }
                Err(e) => {
                    failed_count += 1;
                    tracing::error!("Failed to post document {}: {}", doc.base.id.as_string(), e);
                }
            }
        }
    }

    Ok(Json(serde_json::json!({
        "success": true,
        "posted_count": posted_count,
        "failed_count": failed_count
    })))
}

/// Handler для пакетного проведения документов (до 100 документов за раз)
pub async fn batch_post_documents(
    Json(req): Json<BatchOperationRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let total = req.ids.len();
    let mut succeeded = 0;
    let mut failed = 0;

    for id_str in req.ids {
        let uuid = match Uuid::parse_str(&id_str) {
            Ok(uuid) => uuid,
            Err(_) => {
                failed += 1;
                continue;
            }
        };

        match a013_ym_order::posting::post_document(uuid).await {
            Ok(_) => succeeded += 1,
            Err(_) => failed += 1,
        }
    }

    tracing::info!(
        "Batch posted {} YM Order documents (succeeded: {}, failed: {})",
        total,
        succeeded,
        failed
    );

    Ok(Json(serde_json::json!({
        "success": true,
        "succeeded": succeeded,
        "failed": failed,
        "total": total
    })))
}

/// Handler для пакетной отмены проведения документов (до 100 документов за раз)
pub async fn batch_unpost_documents(
    Json(req): Json<BatchOperationRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let total = req.ids.len();
    let mut succeeded = 0;
    let mut failed = 0;

    for id_str in req.ids {
        let uuid = match Uuid::parse_str(&id_str) {
            Ok(uuid) => uuid,
            Err(_) => {
                failed += 1;
                continue;
            }
        };

        match a013_ym_order::posting::unpost_document(uuid).await {
            Ok(_) => succeeded += 1,
            Err(_) => failed += 1,
        }
    }

    tracing::info!(
        "Batch unposted {} YM Order documents (succeeded: {}, failed: {})",
        total,
        succeeded,
        failed
    );

    Ok(Json(serde_json::json!({
        "success": true,
        "succeeded": succeeded,
        "failed": failed,
        "total": total
    })))
}

/// Handler для получения проекций по registrator_ref
pub async fn get_projections(
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    // Получаем данные из проекций p900 и p904
    let p900_items = crate::projections::p900_mp_sales_register::service::get_by_registrator(&id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to get p900 projections: {}", e);
            ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        })?;

    let p904_items = crate::projections::p904_sales_data::repository::get_by_registrator(&id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to get p904 projections: {}", e);
            ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        })?;

    // Объединяем результаты
    let result = serde_json::json!({
        "p900_sales_register": p900_items,
        "p904_sales_data": p904_items,
    });

    Ok(Json(result))
}
