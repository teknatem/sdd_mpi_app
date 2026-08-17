//! Каталог LLM-навыков (skills) — read-only обзор реестра для UI.
//!
//! GET /api/llm-skills → versioned catalog snapshot with package metadata.

use crate::shared::error::ApiError;
use crate::system::auth::extractor::CurrentUser;
use axum::{http::StatusCode, Json};

/// Список навыков из реестра `shared/llm/skills.rs`.
pub async fn list() -> Json<serde_json::Value> {
    Json(crate::shared::llm::skills::catalog())
}

/// Rebuild the package registry atomically. Route authorization requires full
/// access to a017_llm_agent for POST.
pub async fn reload(CurrentUser(claims): CurrentUser) -> Json<serde_json::Value> {
    Json(crate::shared::llm::skills::reload(Some(claims.sub)).await)
}

pub async fn access_matrix() -> Result<Json<serde_json::Value>, ApiError> {
    crate::shared::llm::skill_policy::matrix_json()
        .await
        .map(Json)
        .map_err(|error| {
            tracing::error!("[skills] failed to load access matrix: {}", error);
            ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        })
}

pub async fn save_access_matrix(
    Json(request): Json<crate::shared::llm::skill_policy::SaveMatrixRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    match crate::shared::llm::skill_policy::save_matrix(request).await {
        Ok(value) => Ok(Json(value)),
        Err(error) if error.to_string().contains("REVISION_CONFLICT") => Err((
            StatusCode::CONFLICT,
            Json(serde_json::json!({
                "error": "Матрица была изменена другим пользователем. Обновите страницу."
            })),
        )),
        Err(error) => {
            tracing::error!("[skills] failed to save access matrix: {}", error);
            Err((
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": error.to_string() })),
            ))
        }
    }
}
