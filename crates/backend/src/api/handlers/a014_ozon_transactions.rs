use crate::shared::error::ApiError;
use axum::Json;
use serde::Deserialize;
use uuid::Uuid;

use crate::domain::a014_ozon_transactions;

#[derive(Debug, Deserialize)]
pub struct ListFilters {
    pub date_from: Option<String>,
    pub date_to: Option<String>,
    pub transaction_type: Option<String>,
    pub operation_type_name: Option<String>,
    pub posting_number: Option<String>,
}

/// Handler для получения списка всех транзакций с фильтрами
pub async fn list_all(
    axum::extract::Query(filters): axum::extract::Query<ListFilters>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let transactions = a014_ozon_transactions::service::list_with_filters_as_dto(
        filters.date_from,
        filters.date_to,
        filters.transaction_type,
        filters.operation_type_name,
        filters.posting_number,
    )
    .await
    .map_err(|e| {
        tracing::error!("Failed to list OZON transactions: {}", e);
        ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
    })?;

    Ok(Json(serde_json::json!(transactions)))
}

/// Handler для получения транзакции по ID
pub async fn get_by_id(
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let uuid = Uuid::parse_str(&id).map_err(|_| axum::http::StatusCode::BAD_REQUEST)?;

    let transaction = a014_ozon_transactions::service::get_by_id_as_dto(uuid)
        .await
        .map_err(|e| {
            tracing::error!("Failed to get OZON transaction by ID: {}", e);
            ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        })?
        .ok_or(ApiError::from(axum::http::StatusCode::NOT_FOUND))?;

    Ok(Json(serde_json::json!(transaction)))
}

/// Handler для удаления транзакции
pub async fn delete(
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let uuid = Uuid::parse_str(&id).map_err(|_| axum::http::StatusCode::BAD_REQUEST)?;

    let deleted = a014_ozon_transactions::service::delete(uuid)
        .await
        .map_err(|e| {
            tracing::error!("Failed to delete OZON transaction: {}", e);
            ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        })?;

    if !deleted {
        return Err(axum::http::StatusCode::NOT_FOUND.into());
    }

    Ok(Json(serde_json::json!({"success": true})))
}

/// Handler для получения транзакций по posting_number
pub async fn get_by_posting_number(
    axum::extract::Path(posting_number): axum::extract::Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    // Декодируем URL-кодированный posting_number
    let decoded_posting_number =
        urlencoding::decode(&posting_number).map_err(|_| axum::http::StatusCode::BAD_REQUEST)?;

    tracing::info!(
        "Getting transactions for posting_number: {} (original: {})",
        decoded_posting_number,
        posting_number
    );

    let transactions =
        a014_ozon_transactions::service::get_by_posting_number_as_dto(&decoded_posting_number)
            .await
            .map_err(|e| {
                tracing::error!("Failed to get OZON transactions by posting_number: {}", e);
                ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
            })?;

    tracing::info!(
        "Found {} transactions for posting_number: {}",
        transactions.len(),
        decoded_posting_number
    );
    Ok(Json(serde_json::json!(transactions)))
}

/// Handler для проведения документа
pub async fn post_document(
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let uuid = Uuid::parse_str(&id).map_err(|_| axum::http::StatusCode::BAD_REQUEST)?;

    a014_ozon_transactions::posting::post_document(uuid)
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

    a014_ozon_transactions::posting::unpost_document(uuid)
        .await
        .map_err(|e| {
            tracing::error!("Failed to unpost document: {}", e);
            ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        })?;

    Ok(Json(serde_json::json!({"success": true})))
}

/// Handler для получения проекций по registrator_ref
pub async fn get_projections(
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    // Получаем данные из всех проекций
    let p900_items = crate::projections::p900_mp_sales_register::service::get_by_registrator(&id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to get p900 projections: {}", e);
            ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        })?;

    let p902_items =
        crate::projections::p902_ozon_finance_realization::repository::get_by_registrator(&id)
            .await
            .map_err(|e| {
                tracing::error!("Failed to get p902 projections: {}", e);
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
        "p902_ozon_finance": p902_items,
        "p904_sales_data": p904_items,
    });

    Ok(Json(result))
}
