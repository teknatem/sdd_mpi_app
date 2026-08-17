use crate::shared::error::ApiError;
use axum::{extract::Query, Json};
use chrono::NaiveDate;
use contracts::domain::a016_ym_returns::aggregate::{YmReturn, YmReturnListItemDto};
use contracts::domain::common::AggregateId;
use sea_orm::Statement;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::domain::a016_ym_returns;
use crate::shared::data::db::get_connection;
use crate::shared::data::raw_storage;

/// Серверные итоги по датасету
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YmReturnsTotals {
    pub total_records: usize,
    pub sum_items: i32,
    pub sum_amount: f64,
    pub returns_count: usize,
    pub unredeemed_count: usize,
}

/// Ответ с пагинацией
#[derive(Debug, Clone, Serialize)]
pub struct PaginatedYmReturnsResponse {
    pub items: Vec<YmReturnListItemDto>,
    pub total: usize,
    pub page: usize,
    pub page_size: usize,
    pub total_pages: usize,
    /// Серверные итоги по всему датасету (с учётом фильтров)
    pub totals: Option<YmReturnsTotals>,
}

/// Параметры запроса списка
#[derive(Debug, Deserialize)]
pub struct ListReturnsQuery {
    pub date_from: Option<String>,
    pub date_to: Option<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
    pub return_type: Option<String>,
    pub connection_mp_ref: Option<String>,
    pub sort_by: Option<String>,
    pub sort_desc: Option<bool>,
    pub search_return_id: Option<String>,
    pub search_order_id: Option<String>,
}

/// Handler для получения списка с пагинацией
pub async fn list_returns(
    Query(query): Query<ListReturnsQuery>,
) -> Result<Json<PaginatedYmReturnsResponse>, ApiError> {
    use a016_ym_returns::repository::{list_sql, YmReturnsListQuery};

    let page_size = query.limit.unwrap_or(100);
    let offset = query.offset.unwrap_or(0);
    let page = if page_size > 0 { offset / page_size } else { 0 };
    let sort_by = query
        .sort_by
        .clone()
        .unwrap_or_else(|| "created_at_source".to_string());
    let sort_desc = query.sort_desc.unwrap_or(true);

    let list_query = YmReturnsListQuery {
        date_from: query.date_from.clone(),
        date_to: query.date_to.clone(),
        return_type: query.return_type.clone(),
        connection_mp_ref: query.connection_mp_ref.clone(),
        search_return_id: query.search_return_id.clone(),
        search_order_id: query.search_order_id.clone(),
        sort_by: sort_by.clone(),
        sort_desc,
        limit: page_size,
        offset,
    };

    let result = list_sql(list_query.clone()).await.map_err(|e| {
        tracing::error!("Failed to list Yandex Market returns: {}", e);
        ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
    })?;

    let total = result.total;
    let total_pages = if page_size > 0 {
        (total + page_size - 1) / page_size
    } else {
        0
    };

    // Рассчитать итоги по всему датасету (с учётом фильтров)
    let totals = calculate_totals(&list_query).await.ok();

    Ok(Json(PaginatedYmReturnsResponse {
        items: result.items,
        total,
        page,
        page_size,
        total_pages,
        totals,
    }))
}

/// Рассчитать итоги по всему датасету (с учётом фильтров)
async fn calculate_totals(
    query: &a016_ym_returns::repository::YmReturnsListQuery,
) -> Result<YmReturnsTotals, anyhow::Error> {
    use sea_orm::ConnectionTrait;

    let db = get_connection();

    // Build WHERE clause (такой же как в list_sql)
    let mut conditions = vec!["is_deleted = 0".to_string()];

    if let Some(ref date_from) = query.date_from {
        conditions.push(format!(
            "json_extract(state_json, '$.created_at_source') >= '{}'",
            date_from
        ));
    }
    if let Some(ref date_to) = query.date_to {
        conditions.push(format!(
            "json_extract(state_json, '$.created_at_source') <= '{}T23:59:59'",
            date_to
        ));
    }
    if let Some(ref return_type) = query.return_type {
        conditions.push(format!(
            "json_extract(header_json, '$.return_type') = '{}'",
            return_type
        ));
    }
    if let Some(ref connection_mp_ref) = query.connection_mp_ref {
        if !connection_mp_ref.is_empty() {
            conditions.push(format!(
                "json_extract(header_json, '$.connection_id') = '{}'",
                connection_mp_ref
            ));
        }
    }
    if let Some(ref search_return_id) = query.search_return_id {
        if !search_return_id.is_empty() {
            conditions.push(format!(
                "CAST(return_id AS TEXT) LIKE '%{}%'",
                search_return_id
            ));
        }
    }
    if let Some(ref search_order_id) = query.search_order_id {
        if !search_order_id.is_empty() {
            conditions.push(format!(
                "CAST(order_id AS TEXT) LIKE '%{}%'",
                search_order_id
            ));
        }
    }

    let where_clause = conditions.join(" AND ");

    // Запрос итогов.
    // sum_amount берём из header.amount (поля state_json.total_* не существуют),
    // sum_items — сумма count по всем строкам через json_each(lines_json).
    let totals_sql = format!(
        "SELECT
            COUNT(*) as total_records,
            COALESCE(SUM((SELECT COALESCE(SUM(json_extract(j.value, '$.count')), 0) FROM json_each(lines_json) j)), 0) as sum_items,
            COALESCE(SUM(json_extract(header_json, '$.amount')), 0.0) as sum_amount,
            SUM(CASE WHEN json_extract(header_json, '$.return_type') = 'RETURN' THEN 1 ELSE 0 END) as returns_count,
            SUM(CASE WHEN json_extract(header_json, '$.return_type') = 'UNREDEEMED' THEN 1 ELSE 0 END) as unredeemed_count
        FROM a016_ym_returns
        WHERE {}",
        where_clause
    );

    let stmt = Statement::from_string(sea_orm::DatabaseBackend::Sqlite, totals_sql);
    let result = db.query_one(stmt).await?;

    if let Some(row) = result {
        Ok(YmReturnsTotals {
            total_records: row.try_get::<i64>("", "total_records").unwrap_or(0) as usize,
            sum_items: row.try_get::<i64>("", "sum_items").unwrap_or(0) as i32,
            sum_amount: row.try_get::<f64>("", "sum_amount").unwrap_or(0.0),
            returns_count: row.try_get::<i64>("", "returns_count").unwrap_or(0) as usize,
            unredeemed_count: row.try_get::<i64>("", "unredeemed_count").unwrap_or(0) as usize,
        })
    } else {
        Ok(YmReturnsTotals {
            total_records: 0,
            sum_items: 0,
            sum_amount: 0.0,
            returns_count: 0,
            unredeemed_count: 0,
        })
    }
}

/// Ответ резолва исходного заказа по номеру (для гиперссылки в карточке возврата)
#[derive(Debug, Serialize)]
pub struct SourceOrderResponse {
    pub id: String,
}

/// Handler: найти внутренний id документа a013_ym_order по номеру заказа (order_id).
/// Возвращает 404, если исходный заказ не загружен.
pub async fn get_source_order(
    axum::extract::Path(order_no): axum::extract::Path<String>,
) -> Result<Json<SourceOrderResponse>, ApiError> {
    let order = crate::domain::a013_ym_order::repository::get_by_document_no(&order_no)
        .await
        .map_err(|e| {
            tracing::error!("Failed to resolve source YM order {}: {}", order_no, e);
            ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        })?
        .ok_or(ApiError::from(axum::http::StatusCode::NOT_FOUND))?;

    Ok(Json(SourceOrderResponse {
        id: order.base.id.as_string(),
    }))
}

/// Handler для получения всех возвратов (без пагинации, для обратной совместимости)
pub async fn list_returns_all() -> Result<Json<Vec<YmReturn>>, ApiError> {
    let items = a016_ym_returns::service::list_all().await.map_err(|e| {
        tracing::error!("Failed to list Yandex Market returns: {}", e);
        ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
    })?;

    Ok(Json(items))
}

/// Handler для получения детальной информации о Yandex Market Return
pub async fn get_return_detail(
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<Json<YmReturn>, ApiError> {
    let uuid = Uuid::parse_str(&id).map_err(|_| axum::http::StatusCode::BAD_REQUEST)?;

    let item = a016_ym_returns::service::get_by_id(uuid)
        .await
        .map_err(|e| {
            tracing::error!("Failed to get Yandex Market return detail: {}", e);
            ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        })?
        .ok_or(ApiError::from(axum::http::StatusCode::NOT_FOUND))?;

    Ok(Json(item))
}

/// Handler для получения raw JSON от Yandex Market API по raw_payload_ref
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

    a016_ym_returns::posting::post_document(uuid)
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

    a016_ym_returns::posting::unpost_document(uuid)
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

/// Handler для проведения документов за период
pub async fn post_period(
    Query(req): Query<PostPeriodRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let from = NaiveDate::parse_from_str(&req.from, "%Y-%m-%d")
        .map_err(|_| axum::http::StatusCode::BAD_REQUEST)?;
    let to = NaiveDate::parse_from_str(&req.to, "%Y-%m-%d")
        .map_err(|_| axum::http::StatusCode::BAD_REQUEST)?;

    let documents = a016_ym_returns::service::list_all().await.map_err(|e| {
        tracing::error!("Failed to list documents: {}", e);
        ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
    })?;

    let mut posted_count = 0;
    let mut failed_count = 0;

    for doc in documents {
        let doc_date = doc.source_meta.fetched_at.date_naive();
        if doc_date >= from && doc_date <= to {
            match a016_ym_returns::posting::post_document(doc.base.id.value()).await {
                Ok(_) => {
                    posted_count += 1;
                    tracing::info!("Posted document: {}", doc.base.id.as_string());
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

#[derive(Deserialize)]
pub struct BatchOperationRequest {
    pub ids: Vec<String>,
}

/// Handler для пакетного проведения документов
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

        match a016_ym_returns::posting::post_document(uuid).await {
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

/// Handler для пакетной отмены проведения документов
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

        match a016_ym_returns::posting::unpost_document(uuid).await {
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

/// Handler для получения проекций по registrator_ref
pub async fn get_projections(
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    // Получаем данные из проекции p904 (YM Returns использует только её)
    let p904_items = crate::projections::p904_sales_data::repository::get_by_registrator(&id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to get p904 projections: {}", e);
            ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        })?;

    // Возвращаем результат в формате совместимом с WB Sales
    let result = serde_json::json!({
        "p904_sales_data": p904_items,
    });

    Ok(Json(result))
}
