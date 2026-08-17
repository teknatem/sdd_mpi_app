//! External API handlers for WB Supply — used by 1C and other external integrations.
//! Authentication is handled by `check_api_key` middleware (X-Api-Key header).

use crate::shared::error::ApiError;
use axum::{extract::Query, Json};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::domain::a029_wb_supply;

// ─────────────────────────────────────────────
// DTOs
// ─────────────────────────────────────────────

/// Flat supply item returned by the list endpoint.
#[derive(Debug, Serialize)]
pub struct ExtSupplyListItem {
    /// Internal system UUID.
    pub id: String,
    /// WB supply ID (e.g. "WB-GI-32319994").
    pub supply_id: String,
    /// Human-readable supply name from WB.
    pub name: Option<String>,
    pub is_done: bool,
    pub is_b2b: bool,
    pub is_posted: bool,
    /// ISO-8601 timestamp of supply creation in WB, or null.
    pub created_at_wb: Option<String>,
    /// ISO-8601 timestamp of supply closure in WB, or null.
    pub closed_at_wb: Option<String>,
    /// Name of the associated organisation.
    pub organization_name: Option<String>,
    /// Number of orders in the supply.
    pub orders_count: i64,
}

/// Paginated list response.
#[derive(Debug, Serialize)]
pub struct ExtSupplyListResponse {
    pub items: Vec<ExtSupplyListItem>,
    pub total: usize,
}

/// Single order row within a supply detail response.
#[derive(Debug, Serialize)]
pub struct ExtOrderRow {
    pub order_id: i64,
    pub order_uid: Option<String>,
    pub article: Option<String>,
    pub nm_id: Option<i64>,
    pub chrt_id: Option<i64>,
    pub barcodes: Vec<String>,
    /// Price in kopecks (multiply by 0.01 to get roubles).
    pub price: Option<i64>,
    pub created_at: Option<String>,
    /// Sticker part A (upper number on label).
    pub part_a: Option<i64>,
    /// Sticker part B (lower number on label).
    pub part_b: Option<i64>,
    /// Reference to 1C nomenclature UUID (a004_nomenclature), if linked.
    pub nomenclature_ref: Option<String>,
    /// Order status: null = active, "cancel" = cancelled.
    pub status: Option<String>,
}

/// Full supply detail including header fields and order list.
#[derive(Debug, Serialize)]
pub struct ExtSupplyDetailResponse {
    pub id: String,
    pub supply_id: String,
    pub name: Option<String>,
    pub is_done: bool,
    pub is_b2b: bool,
    pub is_posted: bool,
    pub created_at_wb: Option<String>,
    pub closed_at_wb: Option<String>,
    pub connection_id: String,
    pub organization_id: String,
    /// Cargo type: 0=virtual, 1=box, 2=mono-pallet, 5=supersafe.
    pub cargo_type: Option<i32>,
    pub orders: Vec<ExtOrderRow>,
}

// ─────────────────────────────────────────────
// Query params
// ─────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct ListSuppliesQuery {
    pub date_from: Option<String>,
    pub date_to: Option<String>,
    pub connection_id: Option<String>,
    pub organization_id: Option<String>,
    #[serde(default = "default_limit")]
    pub limit: usize,
    #[serde(default)]
    pub offset: usize,
    /// When false, only open (not done) supplies are returned. Defaults to true.
    #[serde(default = "default_true")]
    pub show_done: bool,
}

fn default_limit() -> usize {
    200
}

fn default_true() -> bool {
    true
}

// ─────────────────────────────────────────────
// Handlers
// ─────────────────────────────────────────────

/// List WB supplies filtered by period and optional connection/organisation.
///
/// GET /api/ext/v1/wb-supplies
///   ?date_from=2024-01-01&date_to=2024-12-31
///   &connection_id=<uuid>      (optional)
///   &organization_id=<uuid>    (optional)
///   &limit=200&offset=0        (optional)
///   &show_done=true            (optional, default true)
pub async fn list_supplies(
    Query(q): Query<ListSuppliesQuery>,
) -> Result<Json<ExtSupplyListResponse>, ApiError> {
    use a029_wb_supply::repository::{list_sql, WbSupplyListQuery};

    let list_query = WbSupplyListQuery {
        date_from: q.date_from,
        date_to: q.date_to,
        connection_id: q.connection_id,
        organization_id: q.organization_id,
        search_query: None,
        sort_by: "created_at_wb".to_string(),
        sort_desc: true,
        limit: q.limit,
        offset: q.offset,
        show_done: q.show_done,
    };

    let result = list_sql(list_query).await.map_err(|e| {
        tracing::error!("[ext-api] list_supplies error: {}", e);
        ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
    })?;

    let items = result
        .items
        .into_iter()
        .map(|row| ExtSupplyListItem {
            id: row.id,
            supply_id: row.supply_id,
            name: row.supply_name,
            is_done: row.is_done,
            is_b2b: row.is_b2b,
            is_posted: row.is_posted,
            created_at_wb: row.created_at_wb,
            closed_at_wb: row.closed_at_wb,
            organization_name: row.organization_name,
            orders_count: row.orders_count,
        })
        .collect();

    Ok(Json(ExtSupplyListResponse {
        items,
        total: result.total,
    }))
}

/// Get full supply details including order list.
///
/// GET /api/ext/v1/wb-supplies/:id
///   :id may be an internal UUID or the WB supply ID string (e.g. "WB-GI-32319994")
pub async fn get_supply_detail(
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<Json<ExtSupplyDetailResponse>, ApiError> {
    let supply = if id.starts_with("WB-") {
        a029_wb_supply::service::get_by_supply_id(&id)
            .await
            .map_err(|e| {
                tracing::error!("[ext-api] get_supply_detail by supply_id {}: {}", id, e);
                ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
            })?
            .ok_or(ApiError::from(axum::http::StatusCode::NOT_FOUND))?
    } else {
        let uuid = Uuid::parse_str(&id).map_err(|_| axum::http::StatusCode::BAD_REQUEST)?;
        a029_wb_supply::service::get_by_id(uuid)
            .await
            .map_err(|e| {
                tracing::error!("[ext-api] get_supply_detail by uuid {}: {}", id, e);
                ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
            })?
            .ok_or(ApiError::from(axum::http::StatusCode::NOT_FOUND))?
    };

    let orders = supply
        .supply_orders
        .iter()
        .map(|o| ExtOrderRow {
            order_id: o.order_id,
            order_uid: o.order_uid.clone(),
            article: o.article.clone(),
            nm_id: o.nm_id,
            chrt_id: o.chrt_id,
            barcodes: o.barcodes.clone(),
            price: o.price,
            created_at: o.created_at.clone(),
            part_a: o.part_a,
            part_b: o.part_b,
            nomenclature_ref: o.nomenclature_ref.clone(),
            status: o.status.clone(),
        })
        .collect();

    let h = &supply.header;
    let i = &supply.info;

    Ok(Json(ExtSupplyDetailResponse {
        id: supply.base.id.value().to_string(),
        supply_id: h.supply_id.clone(),
        name: i.name.clone(),
        is_done: i.is_done,
        is_b2b: i.is_b2b,
        is_posted: supply.is_posted,
        created_at_wb: i.created_at_wb.map(|dt| dt.to_rfc3339()),
        closed_at_wb: i.closed_at_wb.map(|dt| dt.to_rfc3339()),
        connection_id: h.connection_id.clone(),
        organization_id: h.organization_id.clone(),
        cargo_type: i.cargo_type,
        orders,
    }))
}
