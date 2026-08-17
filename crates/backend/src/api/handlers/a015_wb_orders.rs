use crate::shared::error::ApiError;
use axum::{extract::Query, Json};
use contracts::domain::a015_wb_orders::aggregate::WbOrders;
use contracts::domain::common::AggregateId;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::domain::a015_wb_orders;
use crate::shared::data::raw_storage;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WbOrdersListItemDto {
    #[serde(flatten)]
    pub order: WbOrders,
    pub organization_name: Option<String>,
    pub marketplace_article: Option<String>,
    pub nomenclature_code: Option<String>,
    pub nomenclature_article: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ListOrdersQuery {
    pub date_from: Option<String>,
    pub date_to: Option<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
    pub organization_id: Option<String>,
    pub search_query: Option<String>,
    pub sort_by: Option<String>,
    pub sort_desc: Option<bool>,
    pub show_cancelled: Option<bool>,
}

/// Simplified DTO for list responses (from repository row data)
#[derive(Debug, Clone, Serialize)]
pub struct WbOrdersListItemSimpleDto {
    pub id: String,
    pub document_no: String,
    pub line_id: Option<String>,
    pub document_date: Option<String>,
    pub supplier_article: Option<String>,
    pub brand: Option<String>,
    pub qty: Option<f64>,
    pub margin_pro: Option<f64>,
    pub dealer_price_ut: Option<f64>,
    pub finished_price: Option<f64>,
    pub total_price: Option<f64>,
    pub is_cancel: Option<bool>,
    pub is_supply: Option<bool>,
    pub is_realization: Option<bool>,
    pub warehouse_type: Option<String>,
    pub income_id: Option<i64>,
    pub currency_code: Option<i64>,
    pub is_posted: bool,
    pub organization_name: Option<String>,
    pub marketplace_article: Option<String>,
    pub nomenclature_code: Option<String>,
    pub nomenclature_article: Option<String>,
    pub base_nomenclature_article: Option<String>,
    pub base_nomenclature_description: Option<String>,
    pub has_wb_sales: bool,
}

#[derive(Debug, Serialize)]
pub struct PaginatedWbOrdersResponse {
    pub items: Vec<WbOrdersListItemSimpleDto>,
    pub total: usize,
    pub page: usize,
    pub page_size: usize,
    pub total_pages: usize,
}

/// Handler для получения списка Wildberries Orders с серверной пагинацией
pub async fn list_orders(
    Query(query): Query<ListOrdersQuery>,
) -> Result<Json<PaginatedWbOrdersResponse>, ApiError> {
    use a015_wb_orders::repository::{list_sql, WbOrdersListQuery};

    let page_size = query.limit.unwrap_or(100);
    let offset = query.offset.unwrap_or(0);
    let page = if page_size > 0 { offset / page_size } else { 0 };
    let sort_by = query
        .sort_by
        .clone()
        .unwrap_or_else(|| "document_date".to_string());
    let sort_desc = query.sort_desc.unwrap_or(true);
    let show_cancelled = query.show_cancelled.unwrap_or(true);

    // Build query for SQL-based list
    let list_query = WbOrdersListQuery {
        date_from: query.date_from.clone(),
        date_to: query.date_to.clone(),
        organization_id: query.organization_id.clone(),
        search_query: query.search_query.clone(),
        sort_by: sort_by.clone(),
        sort_desc,
        limit: page_size,
        offset,
        show_cancelled,
    };

    // Execute SQL query
    let result = list_sql(list_query.clone()).await.map_err(|e| {
        tracing::error!("Failed to list Wildberries orders: {}", e);
        ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
    })?;

    let total = result.total;
    let total_pages = if page_size > 0 {
        (total + page_size - 1) / page_size
    } else {
        0
    };

    let marketplace_products = crate::domain::a007_marketplace_product::service::list_all()
        .await
        .map_err(|e| {
            tracing::error!("Failed to load marketplace products: {}", e);
            ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        })?;

    let mp_map: std::collections::HashMap<String, (String, Option<String>)> = marketplace_products
        .into_iter()
        .map(|mp| {
            (
                mp.base.id.as_string(),
                (mp.article.clone(), mp.nomenclature_ref.clone()),
            )
        })
        .collect();

    let nomenclature_items = crate::domain::a004_nomenclature::service::list_all()
        .await
        .map_err(|e| {
            tracing::error!("Failed to load nomenclature: {}", e);
            ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        })?;

    let nom_map: std::collections::HashMap<String, (String, String, String, Option<String>)> =
        nomenclature_items
            .into_iter()
            .map(|nom| {
                (
                    nom.base.id.as_string(),
                    (
                        nom.base.code.clone(),
                        nom.article.clone(),
                        nom.base.description.clone(),
                        nom.base_nomenclature_ref.clone(),
                    ),
                )
            })
            .collect();

    // Build response DTOs with reference data
    let items: Vec<WbOrdersListItemSimpleDto> = result
        .items
        .into_iter()
        .map(|row| {
            let organization_name = row.organization_name.clone();

            let (marketplace_article, nomenclature_ref_from_mp) = row
                .marketplace_product_ref
                .as_ref()
                .and_then(|mp_ref| mp_map.get(mp_ref).cloned())
                .unwrap_or((String::new(), None));

            let nom_ref = row
                .nomenclature_ref
                .as_ref()
                .or(nomenclature_ref_from_mp.as_ref());
            let (nomenclature_code, nomenclature_article) = nom_ref
                .and_then(|nr| {
                    nom_map
                        .get(nr)
                        .cloned()
                        .map(|(code, article, _, _)| (code, article))
                })
                .unwrap_or((String::new(), String::new()));

            let effective_base_ref = row.base_nomenclature_ref.clone().or_else(|| {
                nom_ref.and_then(|nr| {
                    nom_map.get(nr).and_then(|(_, _, _, base_ref)| {
                        base_ref
                            .clone()
                            .filter(|s| {
                                let v = s.trim();
                                !v.is_empty() && v != "00000000-0000-0000-0000-000000000000"
                            })
                            .or_else(|| Some(nr.to_string()))
                    })
                })
            });

            let (base_nomenclature_article, base_nomenclature_description) = effective_base_ref
                .as_ref()
                .and_then(|base_ref| {
                    nom_map
                        .get(base_ref)
                        .cloned()
                        .map(|(_, article, description, _)| (Some(article), Some(description)))
                })
                .or_else(|| {
                    row.base_nomenclature_article
                        .clone()
                        .map(|article| (Some(article), row.base_nomenclature_description.clone()))
                })
                .unwrap_or((None, None));

            WbOrdersListItemSimpleDto {
                id: row.id,
                document_no: row.document_no,
                line_id: row.line_id,
                document_date: row.document_date,
                supplier_article: row.supplier_article,
                brand: row.brand,
                qty: row.qty,
                margin_pro: row.margin_pro,
                dealer_price_ut: row.dealer_price_ut,
                finished_price: row.finished_price,
                total_price: row.total_price,
                is_cancel: row.is_cancel,
                is_supply: row.is_supply,
                is_realization: row.is_realization,
                warehouse_type: row.warehouse_type,
                income_id: row.income_id,
                currency_code: row.currency_code,
                is_posted: row.is_posted,
                organization_name,
                marketplace_article: Some(marketplace_article),
                nomenclature_code: Some(nomenclature_code),
                nomenclature_article: Some(nomenclature_article),
                base_nomenclature_article,
                base_nomenclature_description,
                has_wb_sales: row.has_wb_sales,
            }
        })
        .collect();

    Ok(Json(PaginatedWbOrdersResponse {
        items,
        total,
        page,
        page_size,
        total_pages,
    }))
}

/// Handler для получения детальной информации о Wildberries Order
pub async fn get_order_detail(
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<Json<WbOrders>, ApiError> {
    let uuid = Uuid::parse_str(&id).map_err(|_| axum::http::StatusCode::BAD_REQUEST)?;

    let item = a015_wb_orders::service::get_by_id(uuid)
        .await
        .map_err(|e| {
            tracing::error!("Failed to get Wildberries order detail: {}", e);
            ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        })?
        .ok_or(ApiError::from(axum::http::StatusCode::NOT_FOUND))?;

    Ok(Json(item))
}

/// Handler для поиска документов по srid (document_no)
#[derive(Debug, Deserialize)]
pub struct SearchBySridQuery {
    pub srid: String,
}

pub async fn search_by_srid(
    Query(query): Query<SearchBySridQuery>,
) -> Result<Json<Vec<WbOrders>>, ApiError> {
    let items = a015_wb_orders::repository::search_by_document_no(&query.srid)
        .await
        .map_err(|e| {
            tracing::error!("Failed to search by srid: {}", e);
            ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        })?;

    Ok(Json(items))
}

/// Handler для получения raw JSON от WB API по raw_payload_ref
pub async fn get_raw_json(
    axum::extract::Path(ref_id): axum::extract::Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let json_value = raw_storage::get_json_value_by_ref(&ref_id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to get raw JSON: {}", e);
            ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        })?;

    Ok(Json(json_value))
}

/// Handler для удаления документа
pub async fn delete_order(
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let uuid = Uuid::parse_str(&id).map_err(|_| axum::http::StatusCode::BAD_REQUEST)?;

    a015_wb_orders::service::delete(uuid).await.map_err(|e| {
        tracing::error!("Failed to delete order: {}", e);
        ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
    })?;

    Ok(Json(serde_json::json!({"success": true})))
}

/// Handler для проведения документа
pub async fn post_order(
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let uuid = Uuid::parse_str(&id).map_err(|_| axum::http::StatusCode::BAD_REQUEST)?;

    a015_wb_orders::posting::post_document(uuid)
        .await
        .map_err(|e| {
            tracing::error!("Failed to post order: {}", e);
            ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        })?;

    Ok(Json(
        serde_json::json!({"success": true, "message": "Document posted"}),
    ))
}

/// Handler для отмены проведения документа
pub async fn unpost_order(
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let uuid = Uuid::parse_str(&id).map_err(|_| axum::http::StatusCode::BAD_REQUEST)?;

    a015_wb_orders::posting::unpost_document(uuid)
        .await
        .map_err(|e| {
            tracing::error!("Failed to unpost order: {}", e);
            ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        })?;

    Ok(Json(
        serde_json::json!({"success": true, "message": "Document unposted"}),
    ))
}

/// Движения проекций, которые документ a015 порождает при проведении: p909 (обороты
/// строк заказа) и p916 (воронка: строка «заказ» + при отмене строка «отмена»).
/// Отдаёт JSON, соответствующий записанным данным (закладка «Проекции»).
///
/// Внимание: registrator_ref РАЗНЫЙ у двух проекций — p909 использует префиксный
/// `a015:{id}`, p916 — «сырой» `{id}` (см. `a015::posting::post_document`).
pub async fn get_projections(
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let uuid = Uuid::parse_str(&id).map_err(|_| axum::http::StatusCode::BAD_REQUEST)?;
    let raw_ref = uuid.to_string();
    let prefixed_ref = format!("a015:{raw_ref}");

    let p909_items =
        crate::projections::p909_mp_order_line_turnovers::repository::list_by_registrator_ref(
            &prefixed_ref,
        )
        .await
        .map_err(|e| {
            tracing::error!("Failed to get p909 projections for {}: {}", id, e);
            ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        })?;

    let p916_items =
        crate::projections::p916_mp_sales_funnel_turnovers::repository::list_by_registrator(
            "a015_wb_orders",
            &raw_ref,
        )
        .await
        .map_err(|e| {
            tracing::error!("Failed to get p916 projections for {}: {}", id, e);
            ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        })?;

    Ok(Json(serde_json::json!({
        "p909_mp_order_line_turnovers": p909_items,
        "p916_mp_sales_funnel_turnovers": p916_items
    })))
}
