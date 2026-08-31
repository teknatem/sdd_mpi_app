use crate::shared::error::ApiError;
use axum::{extract::Path, Json};
use once_cell::sync::Lazy;
use std::sync::Arc;

use crate::usecases;

// ============================================================================
// UseCase u501: Import from UT
// ============================================================================

static IMPORT_EXECUTOR: Lazy<Arc<usecases::u501_import_from_ut::ImportExecutor>> =
    Lazy::new(|| {
        let tracker = Arc::new(usecases::u501_import_from_ut::ProgressTracker::new());
        Arc::new(usecases::u501_import_from_ut::ImportExecutor::new(tracker))
    });

/// POST /api/u501/import/start
pub async fn u501_start_import(
    Json(request): Json<contracts::usecases::u501_import_from_ut::ImportRequest>,
) -> Result<Json<contracts::usecases::u501_import_from_ut::ImportResponse>, ApiError> {
    match IMPORT_EXECUTOR.start_import(request).await {
        Ok(response) => Ok(Json(response)),
        Err(e) => {
            tracing::error!("Failed to start import: {}", e);
            Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into())
        }
    }
}

/// GET /api/u501/import/:session_id/progress
pub async fn u501_get_progress(
    Path(session_id): Path<String>,
) -> Result<Json<contracts::usecases::u501_import_from_ut::progress::ImportProgress>, ApiError> {
    match IMPORT_EXECUTOR.get_progress(&session_id) {
        Some(progress) => Ok(Json(progress)),
        None => Err(axum::http::StatusCode::NOT_FOUND.into()),
    }
}

// ============================================================================
// UseCase u502: Import from OZON
// ============================================================================

static OZON_IMPORT_EXECUTOR: Lazy<Arc<usecases::u502_import_from_ozon::ImportExecutor>> =
    Lazy::new(|| {
        let tracker = Arc::new(usecases::u502_import_from_ozon::ProgressTracker::new());
        Arc::new(usecases::u502_import_from_ozon::ImportExecutor::new(
            tracker,
        ))
    });

/// POST /api/u502/import/start
pub async fn u502_start_import(
    Json(request): Json<contracts::usecases::u502_import_from_ozon::ImportRequest>,
) -> Result<Json<contracts::usecases::u502_import_from_ozon::ImportResponse>, ApiError> {
    match OZON_IMPORT_EXECUTOR.start_import(request).await {
        Ok(response) => Ok(Json(response)),
        Err(e) => {
            tracing::error!("Failed to start OZON import: {}", e);
            Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into())
        }
    }
}

/// GET /api/u502/import/:session_id/progress
pub async fn u502_get_progress(
    Path(session_id): Path<String>,
) -> Result<Json<contracts::usecases::u502_import_from_ozon::progress::ImportProgress>, ApiError> {
    match OZON_IMPORT_EXECUTOR.get_progress(&session_id) {
        Some(progress) => Ok(Json(progress)),
        None => Err(axum::http::StatusCode::NOT_FOUND.into()),
    }
}

// ============================================================================
// UseCase u503: Import from Yandex Market
// ============================================================================

static YANDEX_IMPORT_EXECUTOR: Lazy<Arc<usecases::u503_import_from_yandex::ImportExecutor>> =
    Lazy::new(|| {
        let tracker = Arc::new(usecases::u503_import_from_yandex::ProgressTracker::new());
        Arc::new(usecases::u503_import_from_yandex::ImportExecutor::new(
            tracker,
        ))
    });

/// POST /api/u503/import/start
pub async fn u503_start_import(
    Json(request): Json<contracts::usecases::u503_import_from_yandex::ImportRequest>,
) -> Result<Json<contracts::usecases::u503_import_from_yandex::ImportResponse>, ApiError> {
    match YANDEX_IMPORT_EXECUTOR.start_import(request).await {
        Ok(response) => Ok(Json(response)),
        Err(e) => {
            tracing::error!("Failed to start Yandex Market import: {}", e);
            Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into())
        }
    }
}

/// GET /api/u503/import/:session_id/progress
pub async fn u503_get_progress(
    Path(session_id): Path<String>,
) -> Result<Json<contracts::usecases::u503_import_from_yandex::progress::ImportProgress>, ApiError>
{
    match YANDEX_IMPORT_EXECUTOR.get_progress(&session_id) {
        Some(progress) => Ok(Json(progress)),
        None => Err(axum::http::StatusCode::NOT_FOUND.into()),
    }
}

// ============================================================================
// UseCase u504: Import from Wildberries
// ============================================================================

static WB_IMPORT_EXECUTOR: Lazy<Arc<usecases::u504_import_from_wildberries::ImportExecutor>> =
    Lazy::new(|| {
        let tracker = Arc::new(usecases::u504_import_from_wildberries::ProgressTracker::new());
        Arc::new(usecases::u504_import_from_wildberries::ImportExecutor::new(
            tracker,
        ))
    });

/// POST /api/u504/import/start
pub async fn u504_start_import(
    Json(request): Json<contracts::usecases::u504_import_from_wildberries::ImportRequest>,
) -> Result<Json<contracts::usecases::u504_import_from_wildberries::ImportResponse>, ApiError> {
    match WB_IMPORT_EXECUTOR.start_import(request).await {
        Ok(response) => Ok(Json(response)),
        Err(e) => {
            tracing::error!("Failed to start Wildberries import: {}", e);
            Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into())
        }
    }
}

/// GET /api/u504/import/:session_id/progress
pub async fn u504_get_progress(
    Path(session_id): Path<String>,
) -> Result<
    Json<contracts::usecases::u504_import_from_wildberries::progress::ImportProgress>,
    ApiError,
> {
    match WB_IMPORT_EXECUTOR.get_progress(&session_id) {
        Some(progress) => Ok(Json(progress)),
        None => Err(axum::http::StatusCode::NOT_FOUND.into()),
    }
}

// ============================================================================
// UseCase u505: Match Nomenclature
// ============================================================================

static MATCH_NOMENCLATURE_EXECUTOR: Lazy<Arc<usecases::u505_match_nomenclature::MatchExecutor>> =
    Lazy::new(|| {
        let tracker = Arc::new(usecases::u505_match_nomenclature::ProgressTracker::new());
        Arc::new(usecases::u505_match_nomenclature::MatchExecutor::new(
            tracker,
        ))
    });

/// POST /api/u505/match/start
pub async fn u505_start_matching(
    Json(request): Json<contracts::usecases::u505_match_nomenclature::MatchRequest>,
) -> Result<Json<contracts::usecases::u505_match_nomenclature::MatchResponse>, ApiError> {
    match MATCH_NOMENCLATURE_EXECUTOR.start_matching(request).await {
        Ok(response) => Ok(Json(response)),
        Err(e) => {
            tracing::error!("Failed to start nomenclature matching: {}", e);
            Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into())
        }
    }
}

/// GET /api/u505/match/:session_id/progress
pub async fn u505_get_progress(
    Path(session_id): Path<String>,
) -> Result<Json<contracts::usecases::u505_match_nomenclature::progress::MatchProgress>, ApiError> {
    match MATCH_NOMENCLATURE_EXECUTOR.get_progress(&session_id) {
        Some(progress) => Ok(Json(progress)),
        None => Err(axum::http::StatusCode::NOT_FOUND.into()),
    }
}

// ============================================================================
// UseCase u506: Import from LemanaPro
// ============================================================================

static LEMANAPRO_IMPORT_EXECUTOR: Lazy<Arc<usecases::u506_import_from_lemanapro::ImportExecutor>> =
    Lazy::new(|| {
        let tracker = Arc::new(usecases::u506_import_from_lemanapro::ProgressTracker::new());
        Arc::new(usecases::u506_import_from_lemanapro::ImportExecutor::new(
            tracker,
        ))
    });

/// POST /api/u506/import/start
pub async fn u506_start_import(
    Json(request): Json<contracts::usecases::u506_import_from_lemanapro::ImportRequest>,
) -> Result<Json<contracts::usecases::u506_import_from_lemanapro::ImportResponse>, ApiError> {
    match LEMANAPRO_IMPORT_EXECUTOR.start_import(request).await {
        Ok(response) => Ok(Json(response)),
        Err(e) => {
            tracing::error!("Failed to start LemanaPro import: {}", e);
            Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into())
        }
    }
}

/// GET /api/u506/import/:session_id/progress
pub async fn u506_get_progress(
    Path(session_id): Path<String>,
) -> Result<Json<contracts::usecases::u506_import_from_lemanapro::progress::ImportProgress>, ApiError>
{
    match LEMANAPRO_IMPORT_EXECUTOR.get_progress(&session_id) {
        Some(progress) => Ok(Json(progress)),
        None => Err(axum::http::StatusCode::NOT_FOUND.into()),
    }
}

// ============================================================================
// UseCase u507: Import from ERP (Production Output)
// ============================================================================

static ERP_IMPORT_EXECUTOR: Lazy<Arc<usecases::u507_import_from_erp::ImportExecutor>> =
    Lazy::new(|| {
        let tracker = Arc::new(usecases::u507_import_from_erp::ProgressTracker::new());
        Arc::new(usecases::u507_import_from_erp::ImportExecutor::new(tracker))
    });

/// POST /api/u507/import/start
pub async fn u507_start_import(
    Json(request): Json<contracts::usecases::u507_import_from_erp::ImportRequest>,
) -> Result<Json<contracts::usecases::u507_import_from_erp::ImportResponse>, ApiError> {
    match ERP_IMPORT_EXECUTOR.start_import(request).await {
        Ok(response) => Ok(Json(response)),
        Err(e) => {
            tracing::error!("Failed to start ERP import: {}", e);
            Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into())
        }
    }
}

/// GET /api/u507/import/:session_id/progress
pub async fn u507_get_progress(
    Path(session_id): Path<String>,
) -> Result<Json<contracts::usecases::u507_import_from_erp::progress::ImportProgress>, ApiError> {
    match ERP_IMPORT_EXECUTOR.get_progress(&session_id) {
        Some(progress) => Ok(Json(progress)),
        None => Err(axum::http::StatusCode::NOT_FOUND.into()),
    }
}

/// GET /api/u508/repost/projections
pub async fn u508_get_projections(
) -> Result<Json<Vec<contracts::usecases::u508_repost_documents::ProjectionOption>>, ApiError> {
    Ok(Json(
        usecases::u508_repost_documents::shared().list_available_projections(),
    ))
}

/// GET /api/u508/repost/aggregates
pub async fn u508_get_aggregates(
) -> Result<Json<Vec<contracts::usecases::u508_repost_documents::AggregateOption>>, ApiError> {
    Ok(Json(
        usecases::u508_repost_documents::shared().list_available_aggregates(),
    ))
}

/// POST /api/u508/repost/start
pub async fn u508_start_repost(
    Json(request): Json<contracts::usecases::u508_repost_documents::RepostRequest>,
) -> Result<Json<contracts::usecases::u508_repost_documents::RepostResponse>, ApiError> {
    match usecases::u508_repost_documents::shared()
        .start_repost(request)
        .await
    {
        Ok(response) => Ok(Json(response)),
        Err(e) => {
            tracing::error!("Failed to start projection repost: {}", e);
            Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into())
        }
    }
}

/// POST /api/u508/repost/aggregate/start
pub async fn u508_start_aggregate_repost(
    Json(request): Json<contracts::usecases::u508_repost_documents::AggregateRepostRequest>,
) -> Result<Json<contracts::usecases::u508_repost_documents::RepostResponse>, ApiError> {
    match usecases::u508_repost_documents::shared()
        .start_aggregate_repost(request)
        .await
    {
        Ok(response) => Ok(Json(response)),
        Err(e) => {
            tracing::error!("Failed to start aggregate repost: {}", e);
            Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into())
        }
    }
}

/// GET /api/u508/repost/:session_id/progress
pub async fn u508_get_progress(
    Path(session_id): Path<String>,
) -> Result<Json<contracts::usecases::u508_repost_documents::progress::RepostProgress>, ApiError> {
    match usecases::u508_repost_documents::shared().get_progress(&session_id) {
        Some(progress) => Ok(Json(progress)),
        None => Err(axum::http::StatusCode::NOT_FOUND.into()),
    }
}
