use crate::shared::error::ApiError;
use axum::{
    extract::{Path, Query},
    Json,
};
use contracts::projections::p905_wb_commission_history::dto::{
    CommissionDeleteResponse, CommissionHistoryDto, CommissionListRequest, CommissionListResponse,
    CommissionSaveRequest, CommissionSaveResponse, CommissionSyncResponse,
};

use crate::projections::p905_wb_commission_history::repository;

/// Handler для получения списка комиссий с фильтрами
pub async fn list_commissions(
    Query(req): Query<CommissionListRequest>,
) -> Result<Json<CommissionListResponse>, ApiError> {
    let (items, total) = repository::list_with_filters(
        req.date_from,
        req.date_to,
        req.subject_id,
        &req.sort_by.unwrap_or_else(|| "date".to_string()),
        req.sort_desc.unwrap_or(true),
        req.limit.unwrap_or(50),
        req.offset.unwrap_or(0),
    )
    .await
    .map_err(|e| {
        tracing::error!("Failed to list commissions: {}", e);
        ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
    })?;

    let dtos: Vec<CommissionHistoryDto> = items.into_iter().map(model_to_dto).collect();

    Ok(Json(CommissionListResponse {
        items: dtos,
        total_count: total,
    }))
}

/// Handler для получения комиссии по ID
pub async fn get_commission(
    Path(id): Path<String>,
) -> Result<Json<CommissionHistoryDto>, ApiError> {
    let item = repository::get_by_id(&id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to get commission: {}", e);
            ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        })?
        .ok_or(ApiError::from(axum::http::StatusCode::NOT_FOUND))?;

    Ok(Json(model_to_dto(item)))
}

/// Handler для создания/обновления комиссии
pub async fn save_commission(
    Json(req): Json<CommissionSaveRequest>,
) -> Result<Json<CommissionSaveResponse>, ApiError> {
    // Генерируем raw_json если не предоставлен
    let raw_json = req.raw_json.unwrap_or_else(|| {
        serde_json::json!({
            "kgvpBooking": req.kgvp_booking,
            "kgvpMarketplace": req.kgvp_marketplace,
            "kgvpPickup": req.kgvp_pickup,
            "kgvpSupplier": req.kgvp_supplier,
            "kgvpSupplierExpress": req.kgvp_supplier_express,
            "paidStorageKgvp": req.paid_storage_kgvp,
            "parentID": req.parent_id,
            "parentName": req.parent_name,
            "subjectID": req.subject_id,
            "subjectName": req.subject_name,
        })
        .to_string()
    });

    let date = chrono::NaiveDate::parse_from_str(&req.date, "%Y-%m-%d")
        .map_err(|_| axum::http::StatusCode::BAD_REQUEST)?;

    let is_new = req.id.is_none();
    let id = req
        .id
        .clone()
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    let entry = repository::CommissionEntry {
        id: id.clone(),
        date,
        subject_id: req.subject_id,
        subject_name: req.subject_name,
        parent_id: req.parent_id,
        parent_name: req.parent_name,
        kgvp_booking: req.kgvp_booking,
        kgvp_marketplace: req.kgvp_marketplace,
        kgvp_pickup: req.kgvp_pickup,
        kgvp_supplier: req.kgvp_supplier,
        kgvp_supplier_express: req.kgvp_supplier_express,
        paid_storage_kgvp: req.paid_storage_kgvp,
        raw_json,
        payload_version: 1,
    };

    repository::upsert_entry(&entry).await.map_err(|e| {
        tracing::error!("Failed to save commission: {}", e);
        ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
    })?;

    let message = if is_new {
        "Commission record created successfully"
    } else {
        "Commission record updated successfully"
    };

    Ok(Json(CommissionSaveResponse {
        id,
        message: message.to_string(),
    }))
}

/// Handler для удаления комиссии
pub async fn delete_commission(
    Path(id): Path<String>,
) -> Result<Json<CommissionDeleteResponse>, ApiError> {
    let deleted = repository::delete_by_id(&id).await.map_err(|e| {
        tracing::error!("Failed to delete commission: {}", e);
        ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
    })?;

    if deleted == 0 {
        return Ok(Json(CommissionDeleteResponse {
            success: false,
            message: "Commission record not found".to_string(),
        }));
    }

    Ok(Json(CommissionDeleteResponse {
        success: true,
        message: "Commission record deleted successfully".to_string(),
    }))
}

/// Handler для синхронизации комиссий с API
/// DEPRECATED: Используйте u504 Import from Wildberries вместо этого
pub async fn sync_commissions() -> Result<Json<CommissionSyncResponse>, ApiError> {
    Ok(Json(CommissionSyncResponse {
        status: "deprecated".to_string(),
        message: "Эта функция устарела. Используйте 'Импорт из Wildberries' (u504) и выберите 'p905_wb_commission_history' для синхронизации комиссий.".to_string(),
        new_records_count: 0,
        updated_count: 0,
        skipped_count: 0,
    }))
}

/// Преобразование Model в DTO
fn model_to_dto(model: repository::Model) -> CommissionHistoryDto {
    CommissionHistoryDto {
        id: model.id,
        date: model.date,
        subject_id: model.subject_id,
        subject_name: model.subject_name,
        parent_id: model.parent_id,
        parent_name: model.parent_name,
        kgvp_booking: model.kgvp_booking,
        kgvp_marketplace: model.kgvp_marketplace,
        kgvp_pickup: model.kgvp_pickup,
        kgvp_supplier: model.kgvp_supplier,
        kgvp_supplier_express: model.kgvp_supplier_express,
        paid_storage_kgvp: model.paid_storage_kgvp,
        raw_json: model.raw_json,
        loaded_at_utc: model.loaded_at_utc,
        payload_version: model.payload_version,
    }
}
