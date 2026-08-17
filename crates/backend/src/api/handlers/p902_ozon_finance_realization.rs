use crate::shared::error::ApiError;
use axum::{extract::Query, Json};
use contracts::projections::p902_ozon_finance_realization::dto::{
    OzonFinanceRealizationByIdResponse, OzonFinanceRealizationDto,
    OzonFinanceRealizationListRequest, OzonFinanceRealizationListResponse,
    OzonFinanceRealizationStatsRequest, OzonFinanceRealizationStatsResponse,
};

use crate::projections::p902_ozon_finance_realization::repository;

/// Handler для получения списка финансовых данных с фильтрами
pub async fn list_finance_realization(
    Query(req): Query<OzonFinanceRealizationListRequest>,
) -> Result<Json<OzonFinanceRealizationListResponse>, ApiError> {
    let (items, total) = repository::list_with_filters(
        &req.date_from,
        &req.date_to,
        req.posting_number,
        req.sku,
        req.connection_mp_ref,
        req.organization_ref,
        req.operation_type,
        req.is_return,
        req.has_posting_ref,
        &req.sort_by,
        req.sort_desc,
        req.limit,
        req.offset,
    )
    .await
    .map_err(|e| {
        tracing::error!("Failed to list finance realization: {}", e);
        ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
    })?;

    let dtos: Vec<OzonFinanceRealizationDto> = items.into_iter().map(model_to_dto).collect();

    let has_more = total > (req.offset + dtos.len() as i32);

    Ok(Json(OzonFinanceRealizationListResponse {
        items: dtos,
        total_count: total,
        has_more,
    }))
}

/// Handler для получения детальной информации по композитному ключу
pub async fn get_finance_realization_detail(
    axum::extract::Path((posting_number, sku, operation_type)): axum::extract::Path<(
        String,
        String,
        String,
    )>,
) -> Result<Json<OzonFinanceRealizationByIdResponse>, ApiError> {
    let item = repository::get_by_id(&posting_number, &sku, &operation_type)
        .await
        .map_err(|e| {
            tracing::error!("Failed to get finance realization detail: {}", e);
            ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        })?
        .ok_or(ApiError::from(axum::http::StatusCode::NOT_FOUND))?;

    Ok(Json(OzonFinanceRealizationByIdResponse {
        item: model_to_dto_simple(item),
    }))
}

/// Handler для получения статистики по периоду
pub async fn get_stats(
    Query(req): Query<OzonFinanceRealizationStatsRequest>,
) -> Result<Json<OzonFinanceRealizationStatsResponse>, ApiError> {
    let stats = repository::get_stats(&req.date_from, &req.date_to, req.connection_mp_ref)
        .await
        .map_err(|e| {
            tracing::error!("Failed to get finance realization stats: {}", e);
            ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        })?;

    Ok(Json(OzonFinanceRealizationStatsResponse {
        total_rows: stats.total_rows,
        total_quantity: stats.total_quantity,
        total_amount: stats.total_amount,
        total_commission: stats.total_commission,
        total_payout: stats.total_payout,
        unique_postings: stats.unique_postings,
        linked_postings: stats.linked_postings,
    }))
}

/// Преобразование ModelWithSaleDate в DTO
fn model_to_dto(model: repository::ModelWithSaleDate) -> OzonFinanceRealizationDto {
    use chrono::DateTime;

    // Форматирование loaded_at_utc из RFC3339 в "YYYY-MM-DD HH:MM:SS"
    let loaded_at_formatted = DateTime::parse_from_rfc3339(&model.loaded_at_utc)
        .ok()
        .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
        .unwrap_or_else(|| model.loaded_at_utc.clone());

    OzonFinanceRealizationDto {
        posting_number: model.posting_number,
        sku: model.sku,
        document_type: model.document_type,
        registrator_ref: model.registrator_ref,
        connection_mp_ref: model.connection_mp_ref,
        organization_ref: model.organization_ref,
        posting_ref: model.posting_ref,
        accrual_date: model.accrual_date,
        sale_date: model.sale_date, // NEW: Дата продажи из p900_sales_register
        operation_date: model.operation_date,
        delivery_date: model.delivery_date,
        delivery_schema: model.delivery_schema,
        delivery_region: model.delivery_region,
        delivery_city: model.delivery_city,
        quantity: model.quantity,
        price: model.price,
        amount: model.amount,
        commission_amount: model.commission_amount,
        commission_percent: model.commission_percent,
        services_amount: model.services_amount,
        payout_amount: model.payout_amount,
        operation_type: model.operation_type,
        operation_type_name: model.operation_type_name,
        is_return: model.is_return,
        currency_code: model.currency_code,
        loaded_at_utc: loaded_at_formatted, // Отформатированная дата
        payload_version: model.payload_version,
        extra: model.extra,
    }
}

/// Преобразование обычного Model в DTO (для get_by_id)
fn model_to_dto_simple(model: repository::Model) -> OzonFinanceRealizationDto {
    use chrono::DateTime;

    let loaded_at_formatted = DateTime::parse_from_rfc3339(&model.loaded_at_utc)
        .ok()
        .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
        .unwrap_or_else(|| model.loaded_at_utc.clone());

    OzonFinanceRealizationDto {
        posting_number: model.posting_number,
        sku: model.sku,
        document_type: model.document_type,
        registrator_ref: model.registrator_ref,
        connection_mp_ref: model.connection_mp_ref,
        organization_ref: model.organization_ref,
        posting_ref: model.posting_ref,
        accrual_date: model.accrual_date,
        sale_date: None, // Нет sale_date для простого Model
        operation_date: model.operation_date,
        delivery_date: model.delivery_date,
        delivery_schema: model.delivery_schema,
        delivery_region: model.delivery_region,
        delivery_city: model.delivery_city,
        quantity: model.quantity,
        price: model.price,
        amount: model.amount,
        commission_amount: model.commission_amount,
        commission_percent: model.commission_percent,
        services_amount: model.services_amount,
        payout_amount: model.payout_amount,
        operation_type: model.operation_type,
        operation_type_name: model.operation_type_name,
        is_return: model.is_return,
        currency_code: model.currency_code,
        loaded_at_utc: loaded_at_formatted,
        payload_version: model.payload_version,
        extra: model.extra,
    }
}
