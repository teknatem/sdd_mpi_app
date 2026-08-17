use crate::shared::error::ApiError;
use axum::{extract::Query, Json};
use chrono::NaiveDate;
use contracts::domain::a012_wb_sales::aggregate::WbSales;
use contracts::domain::common::AggregateId;
use contracts::shared::analytics::TurnoverLayer;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::domain::a002_organization;
use crate::domain::a012_wb_sales;
use crate::shared::data::db::get_connection;

/// Convert empty string to None
fn non_empty(s: String) -> Option<String> {
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}
use crate::shared::data::raw_storage;
use sea_orm::{ConnectionTrait, Statement};
use std::collections::HashMap;
use std::sync::OnceLock;
use tokio::sync::RwLock;

// Cache for reference data (refreshed periodically)
static ORG_CACHE: OnceLock<RwLock<(HashMap<String, String>, std::time::Instant)>> = OnceLock::new();
static MP_CACHE: OnceLock<
    RwLock<(
        HashMap<String, (String, Option<String>)>,
        std::time::Instant,
    )>,
> = OnceLock::new();
static NOM_CACHE: OnceLock<RwLock<(HashMap<String, (String, String)>, std::time::Instant)>> =
    OnceLock::new();

const CACHE_TTL_SECS: u64 = 300; // 5 minutes

fn normalize_id(s: &str) -> String {
    s.trim().trim_matches('"').to_ascii_lowercase()
}

async fn get_org_map() -> HashMap<String, String> {
    let cache = ORG_CACHE.get_or_init(|| {
        RwLock::new((
            HashMap::new(),
            std::time::Instant::now() - std::time::Duration::from_secs(CACHE_TTL_SECS + 1),
        ))
    });

    {
        let read = cache.read().await;
        if read.1.elapsed().as_secs() < CACHE_TTL_SECS {
            return read.0.clone();
        }
    }

    // Refresh cache
    let mut write = cache.write().await;
    if write.1.elapsed().as_secs() < CACHE_TTL_SECS {
        return write.0.clone();
    }

    if let Ok(orgs) =
        a002_organization::service::list_all(crate::shared::data::db::get_connection()).await
    {
        write.0 = orgs
            .into_iter()
            .map(|org| {
                (
                    normalize_id(&org.base.id.as_string()),
                    org.base.description.clone(),
                )
            })
            .collect();
        write.1 = std::time::Instant::now();
    }
    write.0.clone()
}

async fn get_mp_map() -> HashMap<String, (String, Option<String>)> {
    let cache = MP_CACHE.get_or_init(|| {
        RwLock::new((
            HashMap::new(),
            std::time::Instant::now() - std::time::Duration::from_secs(CACHE_TTL_SECS + 1),
        ))
    });

    {
        let read = cache.read().await;
        if read.1.elapsed().as_secs() < CACHE_TTL_SECS {
            return read.0.clone();
        }
    }

    let mut write = cache.write().await;
    if write.1.elapsed().as_secs() < CACHE_TTL_SECS {
        return write.0.clone();
    }

    if let Ok(mps) = crate::domain::a007_marketplace_product::service::list_all().await {
        write.0 = mps
            .into_iter()
            .map(|mp| {
                (
                    mp.base.id.as_string(),
                    (mp.article.clone(), mp.nomenclature_ref.clone()),
                )
            })
            .collect();
        write.1 = std::time::Instant::now();
    }
    write.0.clone()
}

async fn get_nom_map() -> HashMap<String, (String, String)> {
    let cache = NOM_CACHE.get_or_init(|| {
        RwLock::new((
            HashMap::new(),
            std::time::Instant::now() - std::time::Duration::from_secs(CACHE_TTL_SECS + 1),
        ))
    });

    {
        let read = cache.read().await;
        if read.1.elapsed().as_secs() < CACHE_TTL_SECS {
            return read.0.clone();
        }
    }

    let mut write = cache.write().await;
    if write.1.elapsed().as_secs() < CACHE_TTL_SECS {
        return write.0.clone();
    }

    if let Ok(noms) = crate::domain::a004_nomenclature::service::list_all().await {
        write.0 = noms
            .into_iter()
            .map(|nom| {
                (
                    nom.base.id.as_string(),
                    (nom.base.code.clone(), nom.article.clone()),
                )
            })
            .collect();
        write.1 = std::time::Instant::now();
    }
    write.0.clone()
}

/// Получить минимальные даты операций (rr_dt) из P903 по SRID
/// Возвращает две HashMap: (sales_dates, return_dates)
#[allow(dead_code)]
async fn get_operation_dates_from_p903(
) -> Result<(HashMap<String, String>, HashMap<String, String>), anyhow::Error> {
    let db = get_connection();

    // SQL запрос для операций "Продажа"
    let sql_sales = r#"
        SELECT srid, MIN(rr_dt) as min_rr_dt
        FROM p903_wb_finance_report
        WHERE srid IS NOT NULL AND srid != ''
          AND supplier_oper_name = 'Продажа'
        GROUP BY srid
    "#;

    // SQL запрос для операций "Возврат"
    let sql_returns = r#"
        SELECT srid, MIN(rr_dt) as min_rr_dt
        FROM p903_wb_finance_report
        WHERE srid IS NOT NULL AND srid != ''
          AND supplier_oper_name = 'Возврат'
        GROUP BY srid
    "#;

    // Выполняем запрос для продаж
    let stmt_sales =
        Statement::from_string(sea_orm::DatabaseBackend::Sqlite, sql_sales.to_string());
    let sales_result = db.query_all(stmt_sales).await?;

    let mut sales_dates = HashMap::new();
    for row in sales_result {
        if let (Ok(srid), Ok(rr_dt)) = (
            row.try_get::<String>("", "srid"),
            row.try_get::<String>("", "min_rr_dt"),
        ) {
            sales_dates.insert(srid, rr_dt);
        }
    }

    // Выполняем запрос для возвратов
    let stmt_returns =
        Statement::from_string(sea_orm::DatabaseBackend::Sqlite, sql_returns.to_string());
    let returns_result = db.query_all(stmt_returns).await?;

    let mut return_dates = HashMap::new();
    for row in returns_result {
        if let (Ok(srid), Ok(rr_dt)) = (
            row.try_get::<String>("", "srid"),
            row.try_get::<String>("", "min_rr_dt"),
        ) {
            return_dates.insert(srid, rr_dt);
        }
    }

    Ok((sales_dates, return_dates))
}

/// Compact DTO for list view (only essential fields)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WbSalesListItemDto {
    pub id: String,
    pub document_no: String,
    pub sale_id: Option<String>,
    pub sale_date: String,
    pub supplier_article: String,
    pub name: String,
    pub qty: f64,
    pub amount_line: Option<f64>,
    pub total_price: Option<f64>,
    pub finished_price: Option<f64>,
    pub event_type: String,
    pub organization_name: Option<String>,
    pub marketplace_article: Option<String>,
    pub nomenclature_code: Option<String>,
    pub nomenclature_article: Option<String>,
    pub operation_date: Option<String>,
    pub dealer_price_ut: Option<f64>,
    pub prod_cost_problem: bool,
    pub prod_cost_status: Option<String>,
    pub prod_cost_problem_message: Option<String>,
    pub prod_cost_resolved_total: Option<f64>,
}

/// Серверные итоги по датасету WB Sales
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WbSalesTotals {
    pub total_records: usize,
    pub sum_quantity: i32,
    pub sum_for_pay: f64,
    pub sum_retail_amount: f64,
}

/// Paginated response for WB Sales list
#[derive(Debug, Clone, Serialize)]
pub struct PaginatedWbSalesResponse {
    pub items: Vec<WbSalesListItemDto>,
    pub total: usize,
    pub page: usize,
    pub page_size: usize,
    pub total_pages: usize,
    /// Серверные итоги по всему датасету
    pub totals: Option<WbSalesTotals>,
}

#[derive(Debug, Deserialize)]
pub struct ListSalesQuery {
    pub date_from: Option<String>,
    pub date_to: Option<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
    pub organization_id: Option<String>,
    pub sort_by: Option<String>,
    pub sort_desc: Option<bool>,
    /// Search by sale_id (saleID from WB API)
    pub search_sale_id: Option<String>,
    /// Search by srid (document_no)
    pub search_srid: Option<String>,
    /// Search by supplier_article
    pub search_supplier_article: Option<String>,
}

/// Handler для получения списка Wildberries Sales с пагинацией
/// Использует прямой SQL запрос с денормализованными полями (без JSON парсинга)
pub async fn list_sales(
    Query(query): Query<ListSalesQuery>,
) -> Result<Json<PaginatedWbSalesResponse>, ApiError> {
    use a012_wb_sales::repository::{list_sql, WbSalesListQuery};

    let page_size = query.limit.unwrap_or(100);
    let offset = query.offset.unwrap_or(0);
    let page = if page_size > 0 { offset / page_size } else { 0 };
    let sort_by = query
        .sort_by
        .clone()
        .unwrap_or_else(|| "sale_date".to_string());
    let sort_desc = query.sort_desc.unwrap_or(true);

    // Build query for SQL-based list
    let list_query = WbSalesListQuery {
        date_from: query.date_from.clone(),
        date_to: query.date_to.clone(),
        organization_id: query.organization_id.clone(),
        search_sale_id: query.search_sale_id.clone(),
        search_srid: query.search_srid.clone(),
        search_supplier_article: query.search_supplier_article.clone(),
        sort_by: sort_by.clone(),
        sort_desc,
        limit: page_size,
        offset,
    };

    // Execute SQL query (no caching, direct DB query)
    let result = list_sql(list_query.clone()).await.map_err(|e| {
        tracing::error!("Failed to list Wildberries sales: {}", e);
        ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
    })?;

    let total = result.total;
    let total_pages = if page_size > 0 {
        (total + page_size - 1) / page_size
    } else {
        0
    };

    // Load reference data in parallel (still use caching for these)
    let (org_map, mp_map, nom_map) = tokio::join!(get_org_map(), get_mp_map(), get_nom_map());

    // Build response DTOs with reference data
    let items: Vec<WbSalesListItemDto> = result
        .items
        .into_iter()
        .map(|row| {
            // Get organization name from cache
            let organization_name = row.organization_name.clone().or_else(|| {
                row.organization_id
                    .as_ref()
                    .and_then(|org_id| org_map.get(&normalize_id(org_id)).cloned())
            });

            // Get marketplace article and nomenclature_ref from marketplace_product
            let (marketplace_article, nomenclature_ref_from_mp) = row
                .marketplace_product_ref
                .as_ref()
                .and_then(|mp_ref| mp_map.get(mp_ref).cloned())
                .unwrap_or((String::new(), None));

            // Get nomenclature data
            let nom_ref = row
                .nomenclature_ref
                .as_ref()
                .or(nomenclature_ref_from_mp.as_ref());
            let (nomenclature_code, nomenclature_article) = nom_ref
                .and_then(|nr| nom_map.get(nr).cloned())
                .unwrap_or((String::new(), String::new()));

            // Get operation date from P903
            let operation_date = None;

            WbSalesListItemDto {
                id: row.id,
                document_no: row.document_no,
                sale_id: row.sale_id,
                sale_date: row.sale_date.unwrap_or_default(),
                supplier_article: row.supplier_article.unwrap_or_default(),
                name: row.product_name.unwrap_or_default(),
                qty: row.qty.unwrap_or(0.0),
                amount_line: row.amount_line,
                total_price: row.total_price,
                finished_price: row.finished_price,
                event_type: row.event_type.unwrap_or_default(),
                organization_name,
                marketplace_article: non_empty(marketplace_article),
                nomenclature_code: non_empty(nomenclature_code),
                nomenclature_article: non_empty(nomenclature_article),
                operation_date,
                dealer_price_ut: row.dealer_price_ut,
                prod_cost_problem: row.prod_cost_problem,
                prod_cost_status: row.prod_cost_status,
                prod_cost_problem_message: row.prod_cost_problem_message,
                prod_cost_resolved_total: row.prod_cost_resolved_total,
            }
        })
        .collect();

    // Рассчитать итоги по всему датасету (с учётом фильтров)
    let totals = calculate_wb_sales_totals(&list_query).await.ok();

    Ok(Json(PaginatedWbSalesResponse {
        items,
        total,
        page,
        page_size,
        total_pages,
        totals,
    }))
}

/// Рассчитать итоги по всему датасету WB Sales (с учётом фильтров)
async fn calculate_wb_sales_totals(
    query: &a012_wb_sales::repository::WbSalesListQuery,
) -> Result<WbSalesTotals, anyhow::Error> {
    use sea_orm::ConnectionTrait;

    let db = get_connection();

    // Build WHERE clause (такой же как в list_sql)
    let mut conditions = vec!["is_deleted = 0".to_string()];

    if let Some(ref date_from) = query.date_from {
        conditions.push(format!("sale_date >= '{}'", date_from));
    }
    if let Some(ref date_to) = query.date_to {
        conditions.push(format!("sale_date <= '{}'", date_to));
    }
    if let Some(ref org_id) = query.organization_id {
        if !org_id.is_empty() {
            conditions.push(format!(
                "LOWER(TRIM(REPLACE(COALESCE(organization_id, ''), '\"', ''))) = LOWER(TRIM(REPLACE('{}', '\"', '')))",
                org_id
            ));
        }
    }
    if let Some(ref sale_id) = query.search_sale_id {
        if !sale_id.is_empty() {
            conditions.push(format!("sale_id LIKE '%{}%'", sale_id));
        }
    }
    if let Some(ref srid) = query.search_srid {
        if !srid.is_empty() {
            conditions.push(format!("srid LIKE '%{}%'", srid));
        }
    }

    let where_clause = conditions.join(" AND ");

    // Запрос итогов
    let totals_sql = format!(
        "SELECT 
            COUNT(*) as total_records,
            COALESCE(SUM(qty), 0) as sum_quantity,
            COALESCE(SUM(finished_price), 0.0) as sum_for_pay,
            COALESCE(SUM(total_price), 0.0) as sum_retail_amount
        FROM a012_wb_sales 
        WHERE {}",
        where_clause
    );

    let stmt = Statement::from_string(sea_orm::DatabaseBackend::Sqlite, totals_sql);
    let result = db.query_one(stmt).await?;

    if let Some(row) = result {
        Ok(WbSalesTotals {
            total_records: row.try_get::<i64>("", "total_records").unwrap_or(0) as usize,
            sum_quantity: row.try_get::<f64>("", "sum_quantity").unwrap_or(0.0) as i32,
            sum_for_pay: row.try_get::<f64>("", "sum_for_pay").unwrap_or(0.0),
            sum_retail_amount: row.try_get::<f64>("", "sum_retail_amount").unwrap_or(0.0),
        })
    } else {
        Ok(WbSalesTotals {
            total_records: 0,
            sum_quantity: 0,
            sum_for_pay: 0.0,
            sum_retail_amount: 0.0,
        })
    }
}

/// Handler для получения детальной информации о Wildberries Sale
pub async fn get_sale_detail(
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<Json<WbSales>, ApiError> {
    let uuid = Uuid::parse_str(&id).map_err(|_| axum::http::StatusCode::BAD_REQUEST)?;

    let item = a012_wb_sales::service::get_by_id(uuid)
        .await
        .map_err(|e| {
            tracing::error!("Failed to get Wildberries sale detail: {}", e);
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
) -> Result<Json<Vec<WbSales>>, ApiError> {
    let items = a012_wb_sales::repository::search_by_document_no(&query.srid)
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

/// Handler для проведения документа
pub async fn post_document(
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let uuid = Uuid::parse_str(&id).map_err(|_| axum::http::StatusCode::BAD_REQUEST)?;

    a012_wb_sales::posting::post_document(uuid)
        .await
        .map_err(|e| {
            tracing::error!("Failed to post document: {}", e);
            ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        })?;

    Ok(Json(serde_json::json!({"success": true})))
}

/// Handler для отмены проведения документа
pub async fn unpost_document(
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let uuid = Uuid::parse_str(&id).map_err(|_| axum::http::StatusCode::BAD_REQUEST)?;

    a012_wb_sales::posting::unpost_document(uuid)
        .await
        .map_err(|e| {
            tracing::error!("Failed to unpost document: {}", e);
            ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        })?;

    Ok(Json(serde_json::json!({"success": true})))
}

#[derive(Deserialize)]
pub struct PostPeriodRequest {
    pub from: String,
    pub to: String,
}

#[derive(Deserialize)]
pub struct BatchOperationRequest {
    pub ids: Vec<String>,
}

/// Handler для проведения документов за период
pub async fn post_period(
    Query(req): Query<PostPeriodRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let from = NaiveDate::parse_from_str(&req.from, "%Y-%m-%d")
        .map_err(|_| axum::http::StatusCode::BAD_REQUEST)?;
    let to = NaiveDate::parse_from_str(&req.to, "%Y-%m-%d")
        .map_err(|_| axum::http::StatusCode::BAD_REQUEST)?;

    let documents = a012_wb_sales::service::list_all().await.map_err(|e| {
        tracing::error!("Failed to list documents: {}", e);
        ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
    })?;

    let mut posted_count = 0;
    let mut failed_count = 0;

    for doc in documents {
        let doc_date = doc.source_meta.fetched_at.date_naive();
        if doc_date >= from && doc_date <= to {
            match a012_wb_sales::posting::post_document(doc.base.id.value()).await {
                Ok(_) => {
                    posted_count += 1;
                }
                Err(e) => {
                    failed_count += 1;
                    tracing::error!("Failed to post document {}: {}", doc.base.id.as_string(), e);
                }
            }
        }
    }

    Ok(Json(serde_json::json!({
        "success": true,
        "posted_count": posted_count,
        "failed_count": failed_count
    })))
}

/// Handler для пакетного проведения документов (до 100 документов за раз)
pub async fn batch_post_documents(
    Json(req): Json<BatchOperationRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let total = req.ids.len();
    let mut succeeded = 0;
    let mut failed = 0;

    for id_str in req.ids {
        let uuid = match Uuid::parse_str(&id_str) {
            Ok(uuid) => uuid,
            Err(_) => {
                failed += 1;
                continue;
            }
        };

        match a012_wb_sales::posting::post_document(uuid).await {
            Ok(_) => succeeded += 1,
            Err(_) => failed += 1,
        }
    }

    tracing::info!(
        "Batch posted {} documents (succeeded: {}, failed: {})",
        total,
        succeeded,
        failed
    );

    Ok(Json(serde_json::json!({
        "success": true,
        "succeeded": succeeded,
        "failed": failed,
        "total": total
    })))
}

/// Handler для пакетной отмены проведения документов (до 100 документов за раз)
pub async fn batch_unpost_documents(
    Json(req): Json<BatchOperationRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let total = req.ids.len();
    let mut succeeded = 0;
    let mut failed = 0;

    for id_str in req.ids {
        let uuid = match Uuid::parse_str(&id_str) {
            Ok(uuid) => uuid,
            Err(_) => {
                failed += 1;
                continue;
            }
        };

        match a012_wb_sales::posting::unpost_document(uuid).await {
            Ok(_) => succeeded += 1,
            Err(_) => failed += 1,
        }
    }

    tracing::info!(
        "Batch unposted {} documents (succeeded: {}, failed: {})",
        total,
        succeeded,
        failed
    );

    Ok(Json(serde_json::json!({
        "success": true,
        "succeeded": succeeded,
        "failed": failed,
        "total": total
    })))
}

/// Handler для миграции старых документов: денормализация всех полей из JSON
pub async fn migrate_fill_sale_id() -> Result<Json<serde_json::Value>, ApiError> {
    let updated = crate::shared::data::db::migrate_wb_sales_denormalize()
        .await
        .map_err(|e| {
            tracing::error!("Failed to migrate WB Sales: {}", e);
            ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        })?;

    Ok(Json(serde_json::json!({
        "success": true,
        "updated": updated,
        "message": format!("Denormalization completed: {} documents updated", updated)
    })))
}

/// Handler для получения проекций по registrator_ref
pub async fn get_projections(
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    // Load p900 and p904 projections for a012_wb_sales
    let p900_items = crate::projections::p900_mp_sales_register::service::get_by_registrator(&id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to get p900 projections: {}", e);
            ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        })?;

    let p904_items = crate::projections::p904_sales_data::repository::get_by_registrator(&id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to get p904 projections: {}", e);
            ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        })?;

    let p913_items =
        crate::projections::p913_wb_advert_order_attr::repository::list_by_registrator(
            "a012_wb_sales",
            &id,
        )
        .await
        .map_err(|e| {
            tracing::error!("Failed to get p913 projections for a012 {}: {}", id, e);
            ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        })?;
    let p913_expense: Vec<_> = p913_items
        .into_iter()
        .filter(|row| row.turnover_code == "advert_clicks_order_expense")
        .collect();

    // p916 (воронка): выкуп/возврат из a012. registrator_ref = «сырой» id (см. a012::posting).
    let p916_items =
        crate::projections::p916_mp_sales_funnel_turnovers::repository::list_by_registrator(
            "a012_wb_sales",
            &id,
        )
        .await
        .map_err(|e| {
            tracing::error!("Failed to get p916 projections for a012 {}: {}", id, e);
            ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        })?;

    // Объединяем результаты
    let result = serde_json::json!({
        "p900_sales_register": p900_items,
        "p904_sales_data": p904_items,
        "p913_wb_advert_order_attr": p913_expense,
        "p916_mp_sales_funnel_turnovers": p916_items,
    });

    Ok(Json(result))
}

/// Handler для получения записей журнала операций по registrator_ref
pub async fn get_general_ledger_entries(
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    use crate::general_ledger::turnover_registry::get_turnover_class;
    use contracts::general_ledger::GeneralLedgerEntryDto;

    let rows = crate::general_ledger::repository::list_by_registrator("a012_wb_sales", &id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to get journal entries: {}", e);
            ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        })?;

    // Обогащаем каждую запись комментарием из реестра оборотов.
    let entries: Vec<GeneralLedgerEntryDto> = rows
        .into_iter()
        .map(|r| {
            let turnover_name = get_turnover_class(&r.turnover_code)
                .map(|c| c.name.to_string())
                .unwrap_or_else(|| r.turnover_code.clone());
            let comment = get_turnover_class(&r.turnover_code)
                .map(|c| c.journal_comment.to_string())
                .unwrap_or_default();
            GeneralLedgerEntryDto {
                id: r.id,
                entry_date: r.entry_date,
                layer: TurnoverLayer::from_str(&r.layer).unwrap_or(TurnoverLayer::Oper),
                entity: r.entity,
                connection_mp_ref: r.connection_mp_ref,
                registrator_type: r.registrator_type,
                registrator_ref: r.registrator_ref,
                order_id: r.order_id,
                debit_account: r.debit_account,
                credit_account: r.credit_account,
                amount: r.amount,
                qty: r.qty,
                turnover_code: r.turnover_code,
                turnover_name,
                resource_table: r.resource_table,
                resource_field: r.resource_field,
                resource_sign: r.resource_sign,
                created_at: r.created_at,
                comment,
            }
        })
        .collect();

    Ok(Json(serde_json::json!({ "entries": entries })))
}

// Advert attribution (decode advert_clicks_order_expense GL entry into source p913 reserves)
// Р Р†РІР‚СњР вЂљР Р†РІР‚СњР вЂљР Р†РІР‚СњР вЂљР Р†РІР‚СњР вЂљР Р†РІР‚СњР вЂљР Р†РІР‚СњР вЂљР Р†РІР‚СњР вЂљР Р†РІР‚СњР вЂљР Р†РІР‚СњР вЂљР Р†РІР‚СњР вЂљР Р†РІР‚СњР вЂљР Р†РІР‚СњР вЂљР Р†РІР‚СњР вЂљР Р†РІР‚СњР вЂљР Р†РІР‚СњР вЂљР Р†РІР‚СњР вЂљР Р†РІР‚СњР вЂљР Р†РІР‚СњР вЂљР Р†РІР‚СњР вЂљР Р†РІР‚СњР вЂљР Р†РІР‚СњР вЂљР Р†РІР‚СњР вЂљР Р†РІР‚СњР вЂљР Р†РІР‚СњР вЂљР Р†РІР‚СњР вЂљР Р†РІР‚СњР вЂљР Р†РІР‚СњР вЂљР Р†РІР‚СњР вЂљР Р†РІР‚СњР вЂљР Р†РІР‚СњР вЂљР Р†РІР‚СњР вЂљР Р†РІР‚СњР вЂљР Р†РІР‚СњР вЂљР Р†РІР‚СњР вЂљР Р†РІР‚СњР вЂљР Р†РІР‚СњР вЂљР Р†РІР‚СњР вЂљР Р†РІР‚СњР вЂљР Р†РІР‚СњР вЂљР Р†РІР‚СњР вЂљР Р†РІР‚СњР вЂљР Р†РІР‚СњР вЂљР Р†РІР‚СњР вЂљР Р†РІР‚СњР вЂљР Р†РІР‚СњР вЂљР Р†РІР‚СњР вЂљР Р†РІР‚СњР вЂљР Р†РІР‚СњР вЂљР Р†РІР‚СњР вЂљР Р†РІР‚СњР вЂљР Р†РІР‚СњР вЂљР Р†РІР‚СњР вЂљР Р†РІР‚СњР вЂљР Р†РІР‚СњР вЂљР Р†РІР‚СњР вЂљР Р†РІР‚СњР вЂљР Р†РІР‚СњР вЂљР Р†РІР‚СњР вЂљР Р†РІР‚СњР вЂљР Р†РІР‚СњР вЂљР Р†РІР‚СњР вЂљР Р†РІР‚СњР вЂљР Р†РІР‚СњР вЂљР Р†РІР‚СњР вЂљР Р†РІР‚СњР вЂљР Р†РІР‚СњР вЂљР Р†РІР‚СњР вЂљР Р†РІР‚СњР вЂљР Р†РІР‚СњР вЂљР Р†РІР‚СњР вЂљР Р†РІР‚СњР вЂљР Р†РІР‚СњР вЂљР Р†РІР‚СњР вЂљР Р†РІР‚СњР вЂљР Р†РІР‚СњР вЂљР Р†РІР‚СњР вЂљР Р†РІР‚СњР вЂљ

/// Р В РЎвЂєР В РўвЂР В Р вЂ¦Р В Р’В° Р РЋР С“Р РЋРІР‚С™Р РЋР вЂљР В РЎвЂўР В РЎвЂќР В Р’В° Р РЋР вЂљР В Р’В°Р РЋР С“Р РЋРІвЂљВ¬Р В РЎвЂР РЋРІР‚С›Р РЋР вЂљР В РЎвЂўР В Р вЂ Р В РЎвЂќР В РЎвЂ Р РЋР вЂљР В Р’ВµР В РЎвЂќР В Р’В»Р В Р’В°Р В РЎВР В Р вЂ¦Р В РЎвЂўР В РІвЂћвЂ“ Р В Р’В°Р РЋРІР‚С™Р РЋР вЂљР В РЎвЂР В Р’В±Р РЋРЎвЂњР РЋРІР‚В Р В РЎвЂР В РЎвЂ Р В РЎвЂ”Р В РЎвЂў a012-Р В РўвЂР В РЎвЂўР В РЎвЂќР РЋРЎвЂњР В РЎВР В Р’ВµР В Р вЂ¦Р РЋРІР‚С™Р РЋРЎвЂњ.
///
/// Р В Р Р‹Р В РЎвЂўР В РЎвЂўР РЋРІР‚С™Р В Р вЂ Р В Р’ВµР РЋРІР‚С™Р РЋР С“Р РЋРІР‚С™Р В Р вЂ Р РЋРЎвЂњР В Р’ВµР РЋРІР‚С™ Р В РЎвЂўР В РўвЂР В Р вЂ¦Р В РЎвЂўР В РІвЂћвЂ“ reserve-Р В Р’В·Р В Р’В°Р В РЎвЂ”Р В РЎвЂР РЋР С“Р В РЎвЂ `p913_wb_advert_order_attr` (Р В РЎвЂўР РЋРІР‚С™ a026)
/// Р РЋР С“ Р РЋРІР‚С™Р В Р’ВµР В РЎВ Р В Р’В¶Р В Р’Вµ `order_key`, Р РЋРІР‚РЋР РЋРІР‚С™Р В РЎвЂў Р В РЎвЂ srid Р РЋР РЉР РЋРІР‚С™Р В РЎвЂўР В РЎвЂ“Р В РЎвЂў Р В РўвЂР В РЎвЂўР В РЎвЂќР РЋРЎвЂњР В РЎВР В Р’ВµР В Р вЂ¦Р РЋРІР‚С™Р В Р’В° a012.
#[derive(Debug, Clone, Serialize)]
pub struct AdvertAttributionRowDto {
    /// p913 row id.
    pub id: String,
    /// `entry_date` Р В Р’В·Р В Р’В°Р В РЎвЂ”Р В РЎвЂР РЋР С“Р В РЎвЂ p913 (YYYY-MM-DD).
    pub entry_date: String,
    /// `wb_advert_campaign_code` (advert_id Р В РЎвЂќР В Р’В°Р В РЎвЂќ Р РЋР С“Р РЋРІР‚С™Р РЋР вЂљР В РЎвЂўР В РЎвЂќР В Р’В°).
    pub advert_id: String,
    /// `nomenclature_ref` (UUID a004), Р В Р’ВµР РЋР С“Р В Р’В»Р В РЎвЂ Р В Р’В·Р В Р’В°Р В РўвЂР В Р’В°Р В Р вЂ¦.
    pub nomenclature_ref: Option<String>,
    /// Р В РЎвЂ™Р РЋР вЂљР РЋРІР‚С™Р В РЎвЂР В РЎвЂќР РЋРЎвЂњР В Р’В» Р В Р вЂ¦Р В РЎвЂўР В РЎВР В Р’ВµР В Р вЂ¦Р В РЎвЂќР В Р’В»Р В Р’В°Р РЋРІР‚С™Р РЋРЎвЂњР РЋР вЂљР РЋРІР‚в„– Р В РЎвЂР В Р’В· a004.
    pub nomenclature_article: Option<String>,
    /// Р В Р Р‹Р РЋРЎвЂњР В РЎВР В РЎВР В Р’В° Р В РЎвЂ”Р В РЎвЂў Р РЋР С“Р РЋРІР‚С™Р РЋР вЂљР В РЎвЂўР В РЎвЂќР В Р’Вµ.
    pub amount: f64,
    /// Р В РІР‚СњР В РЎвЂўР В Р’В»Р РЋР РЏ Р РЋР С“Р РЋРІР‚С™Р РЋР вЂљР В РЎвЂўР В РЎвЂќР В РЎвЂ Р В Р вЂ  Р В РЎвЂўР В Р’В±Р РЋРІР‚В°Р В Р’ВµР В РІвЂћвЂ“ Р РЋР С“Р РЋРЎвЂњР В РЎВР В РЎВР В Р’Вµ (Р В Р вЂ  Р В РЎвЂ”Р РЋР вЂљР В РЎвЂўР РЋРІР‚В Р В Р’ВµР В Р вЂ¦Р РЋРІР‚С™Р В Р’В°Р РЋРІР‚В¦).
    pub ratio_percent: f64,
    /// Р В РЎСџР РЋР вЂљР В РЎвЂР В Р’В·Р В Р вЂ¦Р В Р’В°Р В РЎвЂќ Р вЂ™Р’В«Р В РЎвЂ”Р РЋР вЂљР В РЎвЂўР В Р’В±Р В Р’В»Р В Р’ВµР В РЎВР В Р вЂ¦Р В Р’В°Р РЋР РЏР вЂ™Р’В» Р Р†Р вЂљРІР‚Сњ Р В Р’В°Р РЋРІР‚С™Р РЋР вЂљР В РЎвЂР В Р’В±Р РЋРЎвЂњР РЋРІР‚В Р В РЎвЂР РЋР РЏ Р В Р’В±Р РЋРІР‚в„–Р В Р’В»Р В Р’В° Р В Р’В±Р В Р’ВµР В Р’В· Р РЋР РЏР В Р вЂ Р В Р вЂ¦Р В РЎвЂўР В РЎвЂ“Р В РЎвЂў Р РЋР С“Р В Р вЂ Р РЋР РЏР В Р’В·Р В Р’В°Р В Р вЂ¦Р В Р вЂ¦Р В РЎвЂўР В РЎвЂ“Р В РЎвЂў Р В Р’В·Р В Р’В°Р В РЎвЂќР В Р’В°Р В Р’В·Р В Р’В°.
    pub is_problem: bool,
    /// UUID Р В РўвЂР В РЎвЂўР В РЎвЂќР РЋРЎвЂњР В РЎВР В Р’ВµР В Р вЂ¦Р РЋРІР‚С™Р В Р’В° a026, Р В РЎвЂР В Р’В· Р В РЎвЂќР В РЎвЂўР РЋРІР‚С™Р В РЎвЂўР РЋР вЂљР В РЎвЂўР В РЎвЂ“Р В РЎвЂў Р В РЎвЂ”Р РЋР вЂљР В РЎвЂР РЋРІвЂљВ¬Р В Р’В»Р В Р’В° reserve-Р РЋР С“Р РЋРІР‚С™Р РЋР вЂљР В РЎвЂўР В РЎвЂќР В Р’В°.
    pub a026_id: Option<String>,
    /// `document_no` Р В РЎвЂР РЋР С“Р РЋРІР‚В¦Р В РЎвЂўР В РўвЂР В Р вЂ¦Р В РЎвЂўР В РЎвЂ“Р В РЎвЂў a026.
    pub a026_document_no: Option<String>,
    /// `document_date` Р В РЎвЂР РЋР С“Р РЋРІР‚В¦Р В РЎвЂўР В РўвЂР В Р вЂ¦Р В РЎвЂўР В РЎвЂ“Р В РЎвЂў a026 (YYYY-MM-DD).
    pub a026_document_date: Option<String>,
}

/// Р В Р Р‹Р В Р вЂ Р В РЎвЂўР В РўвЂР В Р вЂ¦Р РЋРІР‚в„–Р В Р’Вµ Р В РЎвЂР РЋРІР‚С™Р В РЎвЂўР В РЎвЂ“Р В РЎвЂ Р В РЎвЂ”Р В РЎвЂў Р РЋР С“Р РЋРІР‚С™Р РЋР вЂљР В Р’В°Р В Р вЂ¦Р В РЎвЂР РЋРІР‚В Р В Р’Вµ Р В Р’В°Р РЋРІР‚С™Р РЋР вЂљР В РЎвЂР В Р’В±Р РЋРЎвЂњР РЋРІР‚В Р В РЎвЂР В РЎвЂ.
#[derive(Debug, Clone, Serialize)]
pub struct AdvertAttributionTotals {
    /// Р В РЎв„ўР В РЎвЂўР В Р’В»-Р В Р вЂ Р В РЎвЂў reserve-Р РЋР С“Р РЋРІР‚С™Р РЋР вЂљР В РЎвЂўР В РЎвЂќ.
    pub rows_count: usize,
    /// Р В РЎв„ўР В РЎвЂўР В Р’В»-Р В Р вЂ Р В РЎвЂў Р РЋРЎвЂњР В Р вЂ¦Р В РЎвЂР В РЎвЂќР В Р’В°Р В Р’В»Р РЋР Р‰Р В Р вЂ¦Р РЋРІР‚в„–Р РЋРІР‚В¦ Р В РЎвЂќР В Р’В°Р В РЎВР В РЎвЂ”Р В Р’В°Р В Р вЂ¦Р В РЎвЂР В РІвЂћвЂ“.
    pub campaigns_count: usize,
    /// Р С›Р в‚¬ Р В Р вЂ Р РЋР С“Р В Р’ВµР РЋРІР‚В¦ reserve-Р РЋР С“Р РЋРІР‚С™Р РЋР вЂљР В РЎвЂўР В РЎвЂќ (= Р В РЎвЂўР В Р’В¶Р В РЎвЂР В РўвЂР В Р’В°Р В Р’ВµР В РЎВР В Р’В°Р РЋР РЏ Р РЋР С“Р РЋРЎвЂњР В РЎВР В РЎВР В Р’В° advert_clicks_order_expense).
    pub sum: f64,
    /// Р В Р Р‹Р РЋРЎвЂњР В РЎВР В РЎВР В Р’В° Р В РЎвЂР В Р’В· Р В РЎвЂ”Р РЋР вЂљР В РЎвЂўР В Р вЂ Р В РЎвЂўР В РўвЂР В РЎвЂќР В РЎвЂ `advert_clicks_order_expense` (Р В Р’ВµР РЋР С“Р В Р’В»Р В РЎвЂ Р В РўвЂР В РЎвЂўР В РЎвЂќР РЋРЎвЂњР В РЎВР В Р’ВµР В Р вЂ¦Р РЋРІР‚С™ Р В РЎвЂ”Р РЋР вЂљР В РЎвЂўР В Р вЂ Р В Р’ВµР В РўвЂР РЋРІР‚ВР В Р вЂ¦).
    pub gl_advert_expense: Option<f64>,
    /// `true`, Р В Р’ВµР РЋР С“Р В Р’В»Р В РЎвЂ |sum Р Р†РІвЂљВ¬РІР‚в„ў gl_advert_expense| < 0.01.
    pub is_match: Option<bool>,
}

/// Р В РЎвЂєР РЋРІР‚С™Р В Р вЂ Р В Р’ВµР РЋРІР‚С™ Р РЋР РЉР В Р вЂ¦Р В РўвЂР В РЎвЂ”Р В РЎвЂўР В РЎвЂР В Р вЂ¦Р РЋРІР‚С™Р В Р’В° `GET /api/a012/wb-sales/:id/advert-attribution`.
#[derive(Debug, Clone, Serialize)]
pub struct AdvertAttributionResponse {
    pub srid: String,
    pub is_posted: bool,
    pub is_customer_return: bool,
    pub totals: AdvertAttributionTotals,
    pub rows: Vec<AdvertAttributionRowDto>,
}

/// Handler Р В РўвЂР В Р’В»Р РЋР РЏ Р РЋР вЂљР В Р’В°Р РЋР С“Р РЋРІвЂљВ¬Р В РЎвЂР РЋРІР‚С›Р РЋР вЂљР В РЎвЂўР В Р вЂ Р В РЎвЂќР В РЎвЂ Р РЋР вЂљР В Р’ВµР В РЎвЂќР В Р’В»Р В Р’В°Р В РЎВР В Р вЂ¦Р В РЎвЂўР В РІвЂћвЂ“ Р В Р’В°Р РЋРІР‚С™Р РЋР вЂљР В РЎвЂР В Р’В±Р РЋРЎвЂњР РЋРІР‚В Р В РЎвЂР В РЎвЂ Р В РЎвЂ”Р В РЎвЂў Р В РўвЂР В РЎвЂўР В РЎвЂќР РЋРЎвЂњР В РЎВР В Р’ВµР В Р вЂ¦Р РЋРІР‚С™Р РЋРЎвЂњ a012.
///
/// Р В РІР‚в„ўР В РЎвЂўР В Р’В·Р В Р вЂ Р РЋР вЂљР В Р’В°Р РЋРІР‚В°Р В Р’В°Р В Р’ВµР РЋРІР‚С™ Р В Р вЂ Р РЋР С“Р В Р’Вµ reserve-Р РЋР С“Р РЋРІР‚С™Р РЋР вЂљР В РЎвЂўР В РЎвЂќР В РЎвЂ `p913_wb_advert_order_attr` Р РЋР С“
/// `order_key=srid(document_no)`, Р В РЎвЂўР В Р’В±Р В РЎвЂўР В РЎвЂ“Р В Р’В°Р РЋРІР‚В°Р РЋРІР‚ВР В Р вЂ¦Р В Р вЂ¦Р РЋРІР‚в„–Р В Р’Вµ Р В РЎвЂР В Р вЂ¦Р РЋРІР‚С›Р В РЎвЂўР РЋР вЂљР В РЎВР В Р’В°Р РЋРІР‚В Р В РЎвЂР В Р’ВµР В РІвЂћвЂ“ Р В РЎвЂўР В Р’В± Р В РЎвЂР РЋР С“Р РЋРІР‚В¦Р В РЎвЂўР В РўвЂР В Р вЂ¦Р В РЎвЂўР В РЎВ a026,
/// Р В РЎвЂ”Р В Р’В»Р РЋР вЂ№Р РЋР С“ Р РЋР С“Р РЋР вЂљР В Р’В°Р В Р вЂ Р В Р вЂ¦Р В Р’ВµР В Р вЂ¦Р В РЎвЂР В Р’Вµ Р С›Р в‚¬(amount) Р РЋР С“ Р В РЎвЂ”Р РЋР вЂљР В РЎвЂўР В Р вЂ Р В РЎвЂўР В РўвЂР В РЎвЂќР В РЎвЂўР В РІвЂћвЂ“ `advert_clicks_order_expense` Р В Р вЂ  Р В Р’В¶Р РЋРЎвЂњР РЋР вЂљР В Р вЂ¦Р В Р’В°Р В Р’В»Р В Р’Вµ.
pub async fn get_advert_attribution(
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<Json<AdvertAttributionResponse>, ApiError> {
    use std::collections::{HashMap, HashSet};

    let uuid = Uuid::parse_str(&id).map_err(|_| axum::http::StatusCode::BAD_REQUEST)?;

    let sale = a012_wb_sales::service::get_by_id(uuid)
        .await
        .map_err(|e| {
            tracing::error!("Failed to load a012 {} for attribution: {}", id, e);
            ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        })?
        .ok_or(ApiError::from(axum::http::StatusCode::NOT_FOUND))?;

    let srid = sale.header.document_no.clone();

    let reserve_rows =
        crate::projections::p913_wb_advert_order_attr::repository::list_by_order_key_and_turnover(
            &srid,
            "advert_clicks_order_accrual",
        )
        .await
        .map_err(|e| {
            tracing::error!("Failed to list p913 reserves for srid {}: {}", srid, e);
            ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        })?;

    let nom_map = get_nom_map().await;

    let mut a026_ids: HashSet<String> = HashSet::new();
    for row in &reserve_rows {
        if row.registrator_type == "a026_wb_advert_daily" {
            a026_ids.insert(row.registrator_ref.clone());
        }
    }
    let mut a026_info: HashMap<String, (String, String)> = HashMap::new();
    for ref_id in a026_ids {
        let Ok(uuid) = Uuid::parse_str(&ref_id) else {
            continue;
        };
        match crate::domain::a026_wb_advert_daily::service::get_by_id(uuid).await {
            Ok(Some(doc)) => {
                a026_info.insert(
                    ref_id,
                    (
                        doc.header.document_no.clone(),
                        doc.header.document_date.clone(),
                    ),
                );
            }
            Ok(None) => {}
            Err(e) => {
                tracing::warn!("Failed to load a026 {} for attribution: {}", ref_id, e);
            }
        }
    }

    let total_sum: f64 = reserve_rows.iter().map(|r| r.amount).sum();

    let mut campaigns: HashSet<String> = HashSet::new();
    let mut rows_dto: Vec<AdvertAttributionRowDto> = Vec::with_capacity(reserve_rows.len());
    for row in &reserve_rows {
        campaigns.insert(row.wb_advert_campaign_code.clone());

        let nomenclature_article = row
            .nomenclature_ref
            .as_ref()
            .and_then(|nr| nom_map.get(nr).map(|(_, article)| article.clone()))
            .filter(|s| !s.is_empty());

        let (a026_doc_no, a026_doc_date) = if row.registrator_type == "a026_wb_advert_daily" {
            a026_info
                .get(&row.registrator_ref)
                .cloned()
                .map(|(no, date)| (Some(no), Some(date)))
                .unwrap_or((None, None))
        } else {
            (None, None)
        };
        let a026_id = if row.registrator_type == "a026_wb_advert_daily" {
            Some(row.registrator_ref.clone())
        } else {
            None
        };

        let ratio_percent = if total_sum.abs() > f64::EPSILON {
            row.amount / total_sum * 100.0
        } else {
            0.0
        };

        rows_dto.push(AdvertAttributionRowDto {
            id: row.id.clone(),
            entry_date: row.entry_date.clone(),
            advert_id: row.wb_advert_campaign_code.clone(),
            nomenclature_ref: row.nomenclature_ref.clone(),
            nomenclature_article,
            amount: row.amount,
            ratio_percent,
            is_problem: row.is_problem,
            a026_id,
            a026_document_no: a026_doc_no,
            a026_document_date: a026_doc_date,
        });
    }

    // GL advert_clicks_order_expense for this a012 (if posted).
    let gl_advert_expense =
        match crate::general_ledger::repository::list_by_registrator("a012_wb_sales", &id).await {
            Ok(rows) => {
                let sum: f64 = rows
                    .into_iter()
                    .filter(|r| r.turnover_code == "advert_clicks_order_expense")
                    .map(|r| r.amount)
                    .sum();
                if sum.abs() > f64::EPSILON {
                    Some(sum)
                } else {
                    None
                }
            }
            Err(e) => {
                tracing::warn!(
                    "Failed to load GL entries for a012 {} during attribution: {}",
                    id,
                    e
                );
                None
            }
        };
    let is_match = gl_advert_expense.map(|gl| (total_sum - gl).abs() < 0.01);

    Ok(Json(AdvertAttributionResponse {
        srid,
        is_posted: sale.is_posted || sale.base.metadata.is_posted,
        is_customer_return: sale.is_customer_return,
        totals: AdvertAttributionTotals {
            rows_count: rows_dto.len(),
            campaigns_count: campaigns.len(),
            sum: total_sum,
            gl_advert_expense,
            is_match,
        },
        rows: rows_dto,
    }))
}

/// Handler Р В РўвЂР В Р’В»Р РЋР РЏ Р В РЎвЂўР В Р’В±Р В Р вЂ¦Р В РЎвЂўР В Р вЂ Р В Р’В»Р В Р’ВµР В Р вЂ¦Р В РЎвЂР РЋР РЏ dealer_price_ut
pub async fn refresh_dealer_price(
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let uuid = Uuid::parse_str(&id).map_err(|_| axum::http::StatusCode::BAD_REQUEST)?;

    a012_wb_sales::service::refresh_dealer_price(uuid)
        .await
        .map_err(|e| {
            tracing::error!("Failed to refresh dealer price: {}", e);
            ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        })?;

    Ok(Json(serde_json::json!({"success": true})))
}
