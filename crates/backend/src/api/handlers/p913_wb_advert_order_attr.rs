use crate::shared::error::ApiError;
use axum::{extract::Query, Json};
use contracts::projections::p913_wb_advert_order_attr::dto::{
    WbAdvertOrderAttrDto, WbAdvertOrderAttrListResponse,
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
    pub turnover_code: Option<String>,
    pub order_key: Option<String>,
    pub wb_advert_campaign_code: Option<String>,
}

pub async fn list(
    Query(params): Query<ListParams>,
) -> Result<Json<WbAdvertOrderAttrListResponse>, ApiError> {
    let limit = params.limit.or(Some(500));
    let offset = params.offset.or(Some(0));
    let sort_desc = params.sort_desc.unwrap_or(true);

    let total_count =
        crate::projections::p913_wb_advert_order_attr::repository::count_with_filters(
            params.date_from.clone(),
            params.date_to.clone(),
            params.connection_mp_ref.clone(),
            params.turnover_code.clone(),
            params.order_key.clone(),
            params.wb_advert_campaign_code.clone(),
        )
        .await
        .map_err(|e| {
            tracing::error!("Failed to count p913 rows: {}", e);
            ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        })?;

    let items = crate::projections::p913_wb_advert_order_attr::repository::list_with_filters(
        params.date_from,
        params.date_to,
        params.connection_mp_ref,
        params.turnover_code,
        params.order_key,
        params.wb_advert_campaign_code,
        params.sort_by,
        sort_desc,
        offset,
        limit,
    )
    .await
    .map_err(|e| {
        tracing::error!("Failed to list p913 rows: {}", e);
        ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
    })?;

    let dtos = items.into_iter().map(model_to_dto).collect::<Vec<_>>();
    Ok(Json(WbAdvertOrderAttrListResponse {
        total_count: total_count as i32,
        items: dtos,
    }))
}

fn model_to_dto(
    m: crate::projections::p913_wb_advert_order_attr::repository::Model,
) -> WbAdvertOrderAttrDto {
    WbAdvertOrderAttrDto {
        id: m.id,
        connection_mp_ref: m.connection_mp_ref,
        entry_date: m.entry_date,
        turnover_code: m.turnover_code,
        amount: m.amount,
        nomenclature_ref: m.nomenclature_ref,
        wb_advert_campaign_code: m.wb_advert_campaign_code,
        order_key: m.order_key,
        registrator_type: m.registrator_type,
        registrator_ref: m.registrator_ref,
        general_ledger_ref: m.general_ledger_ref,
        is_problem: m.is_problem,
        created_at: m.created_at,
        updated_at: m.updated_at,
        sale_amount: m.sale_amount,
    }
}
