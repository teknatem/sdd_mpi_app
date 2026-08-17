use crate::shared::error::ApiError;
use axum::{extract::Query, Json};
use contracts::projections::p914_mp_finance_turnovers::dto::{
    MpFinanceTurnoverDto, MpFinanceTurnoverListResponse,
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
    pub registrator_type: Option<String>,
    pub turnover_code: Option<String>,
    pub order_key: Option<String>,
    pub event_kind: Option<String>,
}

pub async fn list(
    Query(params): Query<ListParams>,
) -> Result<Json<MpFinanceTurnoverListResponse>, ApiError> {
    let limit = params.limit.or(Some(1000));
    let offset = params.offset.or(Some(0));
    let sort_desc = params.sort_desc.unwrap_or(true);

    let total_count =
        crate::projections::p914_mp_finance_turnovers::repository::count_with_filters(
            params.date_from.clone(),
            params.date_to.clone(),
            params.connection_mp_ref.clone(),
            params.registrator_type.clone(),
            params.turnover_code.clone(),
            params.order_key.clone(),
            params.event_kind.clone(),
        )
        .await
        .map_err(|e| {
            tracing::error!("Failed to count p914 rows: {}", e);
            ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        })?;

    let items = crate::projections::p914_mp_finance_turnovers::repository::list_with_filters(
        params.date_from,
        params.date_to,
        params.connection_mp_ref,
        params.registrator_type,
        params.turnover_code,
        params.order_key,
        params.event_kind,
        params.sort_by,
        sort_desc,
        offset,
        limit,
    )
    .await
    .map_err(|e| {
        tracing::error!("Failed to list p914 rows: {}", e);
        ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
    })?;

    let has_more = offset.unwrap_or(0) + (items.len() as u64) < total_count;

    let dtos = items.into_iter().map(model_to_dto).collect::<Vec<_>>();
    Ok(Json(MpFinanceTurnoverListResponse {
        total_count: total_count as i32,
        items: dtos,
        has_more,
    }))
}

pub(crate) fn model_to_dto(
    m: crate::projections::p914_mp_finance_turnovers::repository::Model,
) -> MpFinanceTurnoverDto {
    MpFinanceTurnoverDto {
        id: m.id,
        transaction_date: m.transaction_date,
        general_ledger_ref: m.general_ledger_ref,
        registrator_type: m.registrator_type,
        registrator_ref: m.registrator_ref,
        connection_mp_ref: m.connection_mp_ref,
        nomenclature_ref: m.nomenclature_ref,
        marketplace_product_ref: m.marketplace_product_ref,
        turnover_code: m.turnover_code,
        order_key: m.order_key,
        order_ref: m.order_ref,
        order_registrator_type: m.order_registrator_type,
        event_kind: m.event_kind,
        customer_kind: m.customer_kind,
        fulfillment_type: m.fulfillment_type,
        layer: m.layer,
        amount: m.amount,
        quantity: m.quantity,
        created_at_msk: m.created_at_msk,
        updated_at_msk: m.updated_at_msk,
    }
}
