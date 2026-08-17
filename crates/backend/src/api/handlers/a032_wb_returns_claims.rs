use crate::shared::error::ApiError;
use axum::{extract::Path, extract::Query, Json};
use contracts::domain::common::AggregateId;
use serde::Deserialize;
use std::collections::HashMap;
use uuid::Uuid;

use crate::domain::a002_organization;
use crate::domain::a032_wb_returns_claims;

#[derive(Debug, Deserialize)]
pub struct ListReturnsClaimsQuery {
    pub connection_id: Option<String>,
    pub date_from: Option<String>,
    pub date_to: Option<String>,
    pub is_archive: Option<bool>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

/// GET /api/a032/wb-returns-claims
pub async fn list_returns_claims(
    Query(_query): Query<ListReturnsClaimsQuery>,
) -> Result<
    Json<Vec<contracts::domain::a032_wb_returns_claims::aggregate::WbReturnsClaimsListDto>>,
    ApiError,
> {
    let items = a032_wb_returns_claims::service::list_all()
        .await
        .map_err(|e| {
            tracing::error!("Failed to list wb returns claims: {}", e);
            axum::http::StatusCode::INTERNAL_SERVER_ERROR
        })?;

    // Resolve organization names: load all orgs once, build id → name map
    let org_name_map: HashMap<String, String> =
        match a002_organization::service::list_all(crate::shared::data::db::get_connection()).await
        {
            Ok(orgs) => orgs
                .into_iter()
                .map(|org| {
                    let name = if !org.full_name.trim().is_empty() {
                        org.full_name.clone()
                    } else {
                        org.base.description.clone()
                    };
                    (org.base.id.as_string(), name)
                })
                .collect(),
            Err(e) => {
                tracing::warn!("Failed to load organizations for claims list: {}", e);
                HashMap::new()
            }
        };

    let dtos: Vec<_> = items
        .into_iter()
        .map(|a| {
            let mut dto = a.to_list_dto();
            dto.org_name = org_name_map.get(&a.organization_id).cloned();
            dto
        })
        .collect();

    Ok(Json(dtos))
}

/// GET /api/a032/wb-returns-claims/:id
pub async fn get_returns_claim_detail(
    Path(id): Path<String>,
) -> Result<
    Json<contracts::domain::a032_wb_returns_claims::aggregate::WbReturnsClaimsDetailDto>,
    ApiError,
> {
    let uuid = Uuid::parse_str(&id).map_err(|_| axum::http::StatusCode::BAD_REQUEST)?;
    match a032_wb_returns_claims::service::get_by_id(uuid).await {
        Ok(Some(item)) => Ok(Json(item.to_detail_dto())),
        Ok(None) => Err(axum::http::StatusCode::NOT_FOUND.into()),
        Err(e) => {
            tracing::error!("Failed to get wb returns claim {}: {}", id, e);
            Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into())
        }
    }
}
