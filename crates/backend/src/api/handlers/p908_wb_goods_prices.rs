use crate::shared::error::ApiError;
use axum::{
    extract::{Path, Query},
    Json,
};
use contracts::projections::p908_wb_goods_prices::dto::{
    WbGoodsPriceDto, WbGoodsPriceListRequest, WbGoodsPriceListResponse,
};

use crate::projections::p908_wb_goods_prices::repository::{self, WbGoodsPriceRow};

/// Handler для получения списка цен товаров WB
pub async fn list_goods_prices(
    Query(req): Query<WbGoodsPriceListRequest>,
) -> Result<Json<WbGoodsPriceListResponse>, ApiError> {
    let (items, total) = repository::list_with_filters(
        req.connection_mp_ref,
        req.vendor_code,
        req.search,
        &req.sort_by,
        req.sort_desc,
        req.limit,
        req.offset,
    )
    .await
    .map_err(|e| {
        tracing::error!("Failed to list WB goods prices: {}", e);
        ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
    })?;

    let dtos: Vec<WbGoodsPriceDto> = items.into_iter().map(row_to_dto).collect();
    let has_more = total > (req.offset + dtos.len() as i32);

    Ok(Json(WbGoodsPriceListResponse {
        items: dtos,
        total_count: total,
        has_more,
    }))
}

/// Handler для получения одной записи цены товара WB по nm_id
pub async fn get_goods_price(Path(nm_id): Path<i64>) -> Result<Json<WbGoodsPriceDto>, ApiError> {
    let item = repository::get_by_nm_id(nm_id).await.map_err(|e| {
        tracing::error!("Failed to get WB goods price: {}", e);
        ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
    })?;

    match item {
        Some(model) => Ok(Json(model_to_dto(model))),
        None => Err(axum::http::StatusCode::NOT_FOUND.into()),
    }
}

fn row_to_dto(row: WbGoodsPriceRow) -> WbGoodsPriceDto {
    WbGoodsPriceDto {
        nm_id: row.nm_id,
        connection_mp_ref: row.connection_mp_ref,
        vendor_code: row.vendor_code,
        discount: row.discount,
        editable_size_price: row.editable_size_price != 0,
        price: row.price,
        discounted_price: row.discounted_price,
        sizes_json: row.sizes_json,
        fetched_at: row.fetched_at,
        ext_nomenklature_ref: row.ext_nomenklature_ref,
        dealer_price_ut: row.dealer_price_ut,
        margin_pro: row.margin_pro,
        nomenclature_name: row.nomenclature_name,
        connection_name: row.connection_name,
    }
}

fn model_to_dto(model: repository::Model) -> WbGoodsPriceDto {
    WbGoodsPriceDto {
        nm_id: model.nm_id,
        connection_mp_ref: model.connection_mp_ref,
        vendor_code: model.vendor_code,
        discount: model.discount,
        editable_size_price: model.editable_size_price != 0,
        price: model.price,
        discounted_price: model.discounted_price,
        sizes_json: model.sizes_json,
        fetched_at: model.fetched_at,
        ext_nomenklature_ref: model.ext_nomenklature_ref,
        dealer_price_ut: model.dealer_price_ut,
        margin_pro: model.margin_pro,
        nomenclature_name: None,
        connection_name: None,
    }
}
