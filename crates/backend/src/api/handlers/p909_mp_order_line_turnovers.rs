use crate::shared::error::ApiError;
use axum::{
    extract::{Path, Query},
    Json,
};
use contracts::projections::p909_mp_order_line_turnovers::dto::{
    MpOrderLineTurnoverDto, MpOrderLineTurnoverListResponse,
};
use contracts::shared::analytics::{AggKind, EventKind, TurnoverLayer, ValueKind};
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
    pub order_key: Option<String>,
    pub line_key: Option<String>,
    pub layer: Option<String>,
    pub turnover_code: Option<String>,
    pub link_status: Option<String>,
}

pub async fn list(
    Query(params): Query<ListParams>,
) -> Result<Json<MpOrderLineTurnoverListResponse>, ApiError> {
    let limit = params.limit.or(Some(1000));
    let offset = params.offset.or(Some(0));
    let sort_desc = params.sort_desc.unwrap_or(true);
    let total_count =
        crate::projections::p909_mp_order_line_turnovers::service::count_with_filters(
            params.date_from.clone(),
            params.date_to.clone(),
            params.connection_mp_ref.clone(),
            params.order_key.clone(),
            params.line_key.clone(),
            params.layer.clone(),
            params.turnover_code.clone(),
            params.link_status.clone(),
        )
        .await
        .map_err(|error| {
            tracing::error!("Failed to count p909 rows: {}", error);
            ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        })?;
    let items = crate::projections::p909_mp_order_line_turnovers::service::list_with_filters(
        params.date_from,
        params.date_to,
        params.connection_mp_ref,
        params.order_key,
        params.line_key,
        params.layer,
        params.turnover_code,
        params.link_status,
        params.sort_by,
        sort_desc,
        offset,
        limit,
    )
    .await
    .map_err(|error| {
        tracing::error!("Failed to list p909 rows: {}", error);
        ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
    })?;

    let dtos = items.into_iter().map(model_to_dto).collect::<Vec<_>>();
    let limit_value = limit.unwrap_or(1000);
    Ok(Json(MpOrderLineTurnoverListResponse {
        total_count: total_count as i32,
        has_more: (offset.unwrap_or(0) + dtos.len() as u64) < total_count
            && (dtos.len() as u64) >= limit_value,
        items: dtos,
    }))
}

pub async fn get_by_id(Path(id): Path<String>) -> Result<Json<MpOrderLineTurnoverDto>, ApiError> {
    let model = crate::projections::p909_mp_order_line_turnovers::service::get_by_id(&id)
        .await
        .map_err(|error| {
            tracing::error!("Failed to load p909 detail '{}': {}", id, error);
            ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        })?
        .ok_or(ApiError::from(axum::http::StatusCode::NOT_FOUND))?;

    Ok(Json(model_to_dto(model)))
}

pub(crate) fn model_to_dto(
    model: crate::projections::p909_mp_order_line_turnovers::repository::Model,
) -> MpOrderLineTurnoverDto {
    let class =
        crate::shared::analytics::turnover_registry::get_turnover_class(&model.turnover_code)
            .unwrap_or_else(|| panic!("Missing turnover class for {}", model.turnover_code));

    MpOrderLineTurnoverDto {
        id: model.id,
        connection_mp_ref: model.connection_mp_ref,
        order_key: model.order_key,
        line_key: model.line_key,
        line_event_key: model.line_event_key,
        event_kind: EventKind::from_str(&model.event_kind).unwrap_or(EventKind::Other),
        entry_date: model.entry_date,
        layer: TurnoverLayer::from_str(&model.layer).unwrap_or(TurnoverLayer::Oper),
        turnover_code: model.turnover_code,
        value_kind: ValueKind::from_str(&model.value_kind).unwrap_or(ValueKind::Money),
        agg_kind: AggKind::from_str(&model.agg_kind).unwrap_or(AggKind::Sum),
        amount: model.amount,
        nomenclature_ref: model.nomenclature_ref,
        marketplace_product_ref: model.marketplace_product_ref,
        registrator_type: model.registrator_type,
        registrator_ref: model.registrator_ref,
        link_status: model.link_status,
        general_ledger_ref: model.general_ledger_ref,
        created_at: model.created_at,
        updated_at: model.updated_at,
        turnover_name: class.name.to_string(),
        turnover_description: class.description.to_string(),
        turnover_llm_description: class.llm_description.to_string(),
        selection_rule: class.selection_rule,
        report_group: class.report_group,
    }
}
