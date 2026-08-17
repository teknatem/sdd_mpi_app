use crate::shared::error::ApiError;
use axum::{extract::Path, Json};
use serde_json::json;
use uuid::Uuid;

use crate::domain::a009_ozon_returns;

/// GET /api/ozon_returns
pub async fn list_all(
) -> Result<Json<Vec<contracts::domain::a009_ozon_returns::aggregate::OzonReturnsListDto>>, ApiError>
{
    match a009_ozon_returns::service::list_all().await {
        Ok(aggregates) => {
            let list_dtos: Vec<_> = aggregates
                .into_iter()
                .map(|agg| agg.to_list_dto())
                .collect();
            Ok(Json(list_dtos))
        }
        Err(_) => Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into()),
    }
}

/// GET /api/ozon_returns/:id
pub async fn get_by_id(
    Path(id): Path<String>,
) -> Result<Json<contracts::domain::a009_ozon_returns::aggregate::OzonReturnsDetailDto>, ApiError> {
    let uuid = match Uuid::parse_str(&id) {
        Ok(uuid) => uuid,
        Err(_) => return Err(axum::http::StatusCode::BAD_REQUEST.into()),
    };
    match a009_ozon_returns::service::get_by_id(uuid).await {
        Ok(Some(v)) => {
            let detail_dto = v.to_detail_dto();
            Ok(Json(detail_dto))
        }
        Ok(None) => Err(axum::http::StatusCode::NOT_FOUND.into()),
        Err(_) => Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into()),
    }
}

/// POST /api/ozon_returns
pub async fn upsert(
    Json(dto): Json<contracts::domain::a009_ozon_returns::aggregate::OzonReturnsDto>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let result = if dto.id.is_some() {
        a009_ozon_returns::service::update(dto)
            .await
            .map(|_| Uuid::nil().to_string())
    } else {
        a009_ozon_returns::service::create(dto)
            .await
            .map(|id| id.to_string())
    };
    match result {
        Ok(id) => Ok(Json(json!({"id": id}))),
        Err(e) => {
            tracing::error!("Failed to save ozon_returns: {}", e);
            Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into())
        }
    }
}

/// DELETE /api/ozon_returns/:id
pub async fn delete(Path(id): Path<String>) -> Result<(), ApiError> {
    let uuid = match Uuid::parse_str(&id) {
        Ok(uuid) => uuid,
        Err(_) => return Err(axum::http::StatusCode::BAD_REQUEST.into()),
    };
    match a009_ozon_returns::service::delete(uuid).await {
        Ok(true) => Ok(()),
        Ok(false) => Err(axum::http::StatusCode::NOT_FOUND.into()),
        Err(_) => Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into()),
    }
}

/// Handler для проведения документа возврата
pub async fn post_ozon_return(
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let uuid = Uuid::parse_str(&id).map_err(|_| axum::http::StatusCode::BAD_REQUEST)?;

    a009_ozon_returns::posting::post_document(uuid)
        .await
        .map_err(|e| {
            tracing::error!("Failed to post OZON return: {}", e);
            ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        })?;

    Ok(Json(serde_json::json!({"success": true})))
}

/// Handler для отмены проведения документа возврата
pub async fn unpost_ozon_return(
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let uuid = Uuid::parse_str(&id).map_err(|_| axum::http::StatusCode::BAD_REQUEST)?;

    a009_ozon_returns::posting::unpost_document(uuid)
        .await
        .map_err(|e| {
            tracing::error!("Failed to unpost OZON return: {}", e);
            ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        })?;

    Ok(Json(serde_json::json!({"success": true})))
}
