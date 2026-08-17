use crate::shared::error::ApiError;
use axum::{extract::Query, Json};
use chrono::NaiveDate;
use contracts::domain::a010_ozon_fbs_posting::aggregate::OzonFbsPosting;
use contracts::domain::common::AggregateId;
use serde::Deserialize;
use uuid::Uuid;

use crate::domain::a010_ozon_fbs_posting;
use crate::shared::data::raw_storage;

/// Handler для получения списка OZON FBS Posting
pub async fn list_postings() -> Result<Json<Vec<OzonFbsPosting>>, ApiError> {
    let items = a010_ozon_fbs_posting::service::list_all()
        .await
        .map_err(|e| {
            tracing::error!("Failed to list OZON FBS postings: {}", e);
            ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        })?;

    Ok(Json(items))
}

/// Handler для получения детальной информации о OZON FBS Posting
pub async fn get_posting_detail(
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<Json<OzonFbsPosting>, ApiError> {
    let uuid = Uuid::parse_str(&id).map_err(|_| axum::http::StatusCode::BAD_REQUEST)?;

    let item = a010_ozon_fbs_posting::service::get_by_id(uuid)
        .await
        .map_err(|e| {
            tracing::error!("Failed to get OZON FBS posting detail: {}", e);
            ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        })?
        .ok_or(ApiError::from(axum::http::StatusCode::NOT_FOUND))?;

    Ok(Json(item))
}

/// Handler для получения raw JSON от OZON API по raw_payload_ref
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

    a010_ozon_fbs_posting::posting::post_document(uuid)
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

    a010_ozon_fbs_posting::posting::unpost_document(uuid)
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

/// Handler для проведения документов за период
pub async fn post_period(
    Query(req): Query<PostPeriodRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let from = NaiveDate::parse_from_str(&req.from, "%Y-%m-%d")
        .map_err(|_| axum::http::StatusCode::BAD_REQUEST)?;
    let to = NaiveDate::parse_from_str(&req.to, "%Y-%m-%d")
        .map_err(|_| axum::http::StatusCode::BAD_REQUEST)?;

    // Получаем все документы
    let documents = a010_ozon_fbs_posting::service::list_all()
        .await
        .map_err(|e| {
            tracing::error!("Failed to list documents: {}", e);
            ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        })?;

    // Фильтруем по дате и проводим каждый
    let mut posted_count = 0;
    let mut failed_count = 0;

    for doc in documents {
        let doc_date = doc.source_meta.fetched_at.date_naive();
        if doc_date >= from && doc_date <= to {
            match a010_ozon_fbs_posting::posting::post_document(doc.base.id.value()).await {
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
