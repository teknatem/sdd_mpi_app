use crate::shared::error::ApiError;
use axum::{
    extract::{Path, Query},
    Json,
};
use contracts::projections::p915_mp_order_events::dto::{
    MpOrderEventDto, MpOrderEventListResponse,
};
use serde::Deserialize;

#[derive(Deserialize)]
pub struct ListParams {
    pub limit: Option<u64>,
    pub offset: Option<u64>,
    pub sort_by: Option<String>,
    pub sort_desc: Option<bool>,
    pub date_from: Option<String>,
    pub date_to: Option<String>,
    pub connection_mp_ref: Option<String>,
    pub order_id: Option<String>,
    pub event_type: Option<String>,
    pub registrator_type: Option<String>,
    pub layer: Option<String>,
}

pub async fn list(
    Query(params): Query<ListParams>,
) -> Result<Json<MpOrderEventListResponse>, ApiError> {
    let limit = params.limit.or(Some(1000));
    let offset = params.offset.or(Some(0));
    let sort_desc = params.sort_desc.unwrap_or(false);

    let total_count = crate::projections::p915_mp_order_events::repository::count_with_filters(
        params.date_from.clone(),
        params.date_to.clone(),
        params.connection_mp_ref.clone(),
        params.order_id.clone(),
        params.event_type.clone(),
        params.registrator_type.clone(),
        params.layer.clone(),
    )
    .await
    .map_err(|e| {
        tracing::error!("Failed to count p915 rows: {}", e);
        ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
    })?;

    let items = crate::projections::p915_mp_order_events::repository::list_with_filters(
        params.date_from,
        params.date_to,
        params.connection_mp_ref,
        params.order_id,
        params.event_type,
        params.registrator_type,
        params.layer,
        params.sort_by,
        sort_desc,
        offset,
        limit,
    )
    .await
    .map_err(|e| {
        tracing::error!("Failed to list p915 rows: {}", e);
        ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
    })?;

    let has_more = offset.unwrap_or(0) + (items.len() as u64) < total_count;

    let dtos = items.into_iter().map(model_to_dto).collect::<Vec<_>>();
    Ok(Json(MpOrderEventListResponse {
        total_count: total_count as i32,
        items: dtos,
        has_more,
    }))
}

/// Полный таймлайн событий одного заказа (упорядочен по дате/типу события).
pub async fn by_order(
    Path(order_id): Path<String>,
) -> Result<Json<Vec<MpOrderEventDto>>, ApiError> {
    let items = crate::projections::p915_mp_order_events::repository::list_by_order_id(&order_id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to list p915 rows by order_id: {}", e);
            ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        })?;
    Ok(Json(items.into_iter().map(model_to_dto).collect()))
}

pub(crate) fn model_to_dto(
    m: crate::projections::p915_mp_order_events::repository::Model,
) -> MpOrderEventDto {
    MpOrderEventDto {
        id: m.id,
        order_id: m.order_id,
        marketplace_product: m.marketplace_product,
        event_date: m.event_date,
        event_type: m.event_type,
        layer: m.layer,
        amount: m.amount,
        registrator_type: m.registrator_type,
        registrator_ref: m.registrator_ref,
        connection_mp_ref: m.connection_mp_ref,
        created_at_msk: m.created_at_msk,
        updated_at_msk: m.updated_at_msk,
    }
}
