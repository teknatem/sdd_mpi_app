use crate::shared::error::ApiError;
use axum::{
    extract::{Path, Query},
    Json,
};
use contracts::projections::p901_nomenclature_barcodes::{
    BarcodeByIdResponse, BarcodeListRequest, BarcodeListResponse, BarcodesByNomenclatureResponse,
};

use crate::projections::p901_nomenclature_barcodes::{repository, service};

/// Handler для получения номенклатуры по штрихкоду и источнику
/// Требует обязательный query параметр source (1C, OZON, WB, YM)
pub async fn get_by_barcode(
    Path(barcode): Path<String>,
    Query(req): Query<contracts::projections::p901_nomenclature_barcodes::BarcodeByIdRequest>,
) -> Result<Json<BarcodeByIdResponse>, ApiError> {
    // source - обязательный параметр
    if req.source.is_empty() {
        tracing::error!("Source parameter is required for barcode lookup");
        return Err(axum::http::StatusCode::BAD_REQUEST.into());
    }

    let model = repository::get_by_barcode_and_source(&barcode, &req.source)
        .await
        .map_err(|e| {
            tracing::error!(
                "Failed to get barcode {} with source {}: {}",
                barcode,
                req.source,
                e
            );
            ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        })?
        .ok_or(ApiError::from(axum::http::StatusCode::NOT_FOUND))?;

    let dto = service::model_to_dto(&model);

    Ok(Json(BarcodeByIdResponse { barcode: dto }))
}

/// Handler для получения всех штрихкодов по nomenclature_ref
pub async fn get_barcodes_by_nomenclature(
    Path(nomenclature_ref): Path<String>,
    Query(req): Query<
        contracts::projections::p901_nomenclature_barcodes::BarcodesByNomenclatureRequest,
    >,
) -> Result<Json<BarcodesByNomenclatureResponse>, ApiError> {
    let models = repository::get_by_nomenclature_ref(&nomenclature_ref, req.include_inactive)
        .await
        .map_err(|e| {
            tracing::error!(
                "Failed to get barcodes for nomenclature {}: {}",
                nomenclature_ref,
                e
            );
            ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        })?;

    let dtos = service::models_to_dtos(models);
    let total_count = dtos.len();

    Ok(Json(BarcodesByNomenclatureResponse {
        nomenclature_ref,
        barcodes: dtos,
        total_count,
    }))
}

/// Handler для получения списка штрихкодов с фильтрами
pub async fn list_barcodes(
    Query(req): Query<BarcodeListRequest>,
) -> Result<Json<BarcodeListResponse>, ApiError> {
    let (models, total_count) = repository::list_with_filters(
        req.nomenclature_ref,
        req.article,
        req.source,
        req.include_inactive,
        req.limit,
        req.offset,
    )
    .await
    .map_err(|e| {
        tracing::error!("Failed to list barcodes: {}", e);
        ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
    })?;

    let dtos = service::barcodes_with_nomenclature_to_dtos(models);

    Ok(Json(BarcodeListResponse {
        barcodes: dtos,
        total_count,
        limit: req.limit,
        offset: req.offset,
    }))
}
