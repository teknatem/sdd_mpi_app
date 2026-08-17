use crate::shared::error::ApiError;
use axum::{extract::Query, Json};
use serde::Deserialize;

use crate::projections::p912_nomenclature_costs::service;

#[derive(Debug, Deserialize)]
pub struct ListParams {
    pub period: Option<String>,
    pub nomenclature_ref: Option<String>,
    pub registrator_type: Option<String>,
    pub registrator_ref: Option<String>,
    pub q: Option<String>,
    pub limit: Option<u64>,
    pub offset: Option<u64>,
}

/// GET /api/p912/nomenclature-costs
pub async fn list(
    Query(params): Query<ListParams>,
) -> Result<
    Json<contracts::projections::p912_nomenclature_costs::dto::NomenclatureCostListResponse>,
    ApiError,
> {
    let limit = params
        .limit
        .map(|value| value.clamp(1, 10_000))
        .or(Some(1000));

    match service::list_with_filters(
        params.period,
        params.nomenclature_ref,
        params.registrator_type,
        params.registrator_ref,
        params.q,
        limit,
        params.offset,
    )
    .await
    {
        Ok((items, total_count)) => Ok(Json(
            contracts::projections::p912_nomenclature_costs::dto::NomenclatureCostListResponse {
                items,
                total_count,
            },
        )),
        Err(error) => {
            tracing::error!("Failed to list p912 nomenclature costs: {}", error);
            Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into())
        }
    }
}
