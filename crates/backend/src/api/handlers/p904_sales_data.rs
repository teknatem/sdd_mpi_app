use crate::shared::error::ApiError;
use axum::{extract::Query, Json};
use contracts::projections::p904_sales_data::dto::{SalesDataDto, SalesDataListResponse};
use serde::Deserialize;

use crate::projections::p904_sales_data::repository::ModelWithCabinet;
use crate::projections::p904_sales_data::service;

#[derive(Deserialize)]
pub struct ListParams {
    pub limit: Option<u64>,
    pub date_from: Option<String>,
    pub date_to: Option<String>,
    pub connection_mp_ref: Option<String>,
}

pub async fn list(
    Query(params): Query<ListParams>,
) -> Result<Json<SalesDataListResponse>, ApiError> {
    // Валидация лимита: минимум 100, максимум 100000, по умолчанию 1000
    let limit = match params.limit {
        Some(lim) if lim < 100 => {
            tracing::warn!(
                "P904: Invalid limit {} (too small), using default 1000",
                lim
            );
            Some(1000)
        }
        Some(lim) if lim > 100000 => {
            tracing::warn!("P904: Invalid limit {} (too large), using max 100000", lim);
            Some(100000)
        }
        Some(lim) => Some(lim),
        None => {
            tracing::info!("P904: No limit specified, using default 1000");
            Some(1000)
        }
    };

    tracing::info!(
        "P904 list request: limit={:?}, date_from={:?}, date_to={:?}, connection_mp_ref={:?}",
        limit,
        params.date_from,
        params.date_to,
        params.connection_mp_ref
    );

    match service::list_with_filters(
        params.date_from,
        params.date_to,
        params.connection_mp_ref,
        limit,
    )
    .await
    {
        Ok(items) => {
            tracing::info!("P904 list response: {} items returned", items.len());

            let dtos: Vec<SalesDataDto> = items.into_iter().map(model_to_dto).collect();
            let total_count = dtos.len() as i32;

            Ok(Json(SalesDataListResponse {
                items: dtos,
                total_count,
                has_more: false,
            }))
        }
        Err(e) => {
            tracing::error!("Failed to list sales data: {}", e);
            Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into())
        }
    }
}

/// Преобразование ModelWithCabinet в DTO
fn model_to_dto(model: ModelWithCabinet) -> SalesDataDto {
    SalesDataDto {
        id: model.base.id,
        registrator_ref: model.base.registrator_ref,
        registrator_type: model.base.registrator_type,
        date: model.base.date,
        connection_mp_ref: model.base.connection_mp_ref,
        nomenclature_ref: model.base.nomenclature_ref,
        marketplace_product_ref: model.base.marketplace_product_ref,
        customer_in: model.base.customer_in,
        customer_out: model.base.customer_out,
        coinvest_in: model.base.coinvest_in,
        commission_out: model.base.commission_out,
        acquiring_out: model.base.acquiring_out,
        penalty_out: model.base.penalty_out,
        logistics_out: model.base.logistics_out,
        seller_out: model.base.seller_out,
        price_full: model.base.price_full,
        price_list: model.base.price_list,
        price_return: model.base.price_return,
        commission_percent: model.base.commission_percent,
        coinvest_persent: model.base.coinvest_persent,
        total: model.base.total,
        cost: model.base.cost,
        document_no: model.base.document_no,
        article: model.base.article,
        posted_at: model.base.posted_at,
        connection_mp_name: model.connection_mp_name,
    }
}
