//! Репозиторий a041 — дневная воронка YM из отчёта «Аналитика продаж».
//!
//! По образцу `a036_wb_sales_funnel_daily::repository`: документ = кабинет × дата,
//! импорт заменяет период целиком (`replace_for_period`) и в той же транзакции
//! пересобирает маркетинговые движения p916.

use anyhow::Result;
use chrono::Utc;
use contracts::domain::a041_ym_shows_sales_daily::aggregate::{
    YmShowsSalesDaily, YmShowsSalesDailyHeader, YmShowsSalesDailyId, YmShowsSalesDailyLine,
    YmShowsSalesDailySourceMeta,
};
use contracts::domain::common::{BaseAggregate, EntityMetadata};
use sea_orm::entity::prelude::*;
use sea_orm::{ConnectionTrait, QueryOrder, Set, Statement, TransactionTrait};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

use crate::shared::data::db::get_connection;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "a041_ym_shows_sales_daily")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub code: String,
    pub description: String,
    pub comment: Option<String>,
    pub document_no: String,
    pub document_date: String,
    pub connection_id: String,
    pub organization_id: String,
    pub marketplace_id: String,
    #[sea_orm(nullable)]
    pub campaign_id: Option<String>,
    pub lines_count: i32,
    // Денормализованные итоги дня. NULL — метрики не было в отчёте (N/A ≠ 0).
    #[sea_orm(nullable)]
    pub total_shows: Option<i64>,
    #[sea_orm(nullable)]
    pub total_clicks: Option<i64>,
    #[sea_orm(nullable)]
    pub total_to_cart: Option<i64>,
    #[sea_orm(nullable)]
    pub total_order_items: Option<i64>,
    #[sea_orm(nullable)]
    pub total_delivered_count: Option<i64>,
    #[sea_orm(nullable)]
    pub total_canceled_count: Option<i64>,
    #[sea_orm(nullable)]
    pub total_returned_count: Option<i64>,
    pub header_json: String,
    pub totals_json: String,
    pub lines_json: String,
    pub source_meta_json: String,
    pub fetched_at: String,
    pub is_deleted: bool,
    pub created_at: Option<chrono::DateTime<chrono::Utc>>,
    pub updated_at: Option<chrono::DateTime<chrono::Utc>>,
    pub version: i32,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}

fn conn() -> &'static DatabaseConnection {
    get_connection()
}

impl From<Model> for YmShowsSalesDaily {
    fn from(m: Model) -> Self {
        let metadata = EntityMetadata {
            created_at: m.created_at.unwrap_or_else(Utc::now),
            updated_at: m.updated_at.unwrap_or_else(Utc::now),
            is_deleted: m.is_deleted,
            is_posted: false,
            version: m.version,
        };
        let uuid = Uuid::parse_str(&m.id).unwrap_or_else(|_| Uuid::new_v4());
        let header: YmShowsSalesDailyHeader =
            serde_json::from_str(&m.header_json).unwrap_or(YmShowsSalesDailyHeader {
                document_no: m.document_no.clone(),
                document_date: m.document_date.clone(),
                connection_id: m.connection_id.clone(),
                organization_id: m.organization_id.clone(),
                marketplace_id: m.marketplace_id.clone(),
                campaign_id: m.campaign_id.clone(),
            });
        let totals = serde_json::from_str(&m.totals_json).unwrap_or_default();
        let lines = serde_json::from_str(&m.lines_json).unwrap_or_default();
        let source_meta =
            serde_json::from_str(&m.source_meta_json).unwrap_or(YmShowsSalesDailySourceMeta {
                source: "ym_shows_sales".to_string(),
                fetched_at: m.fetched_at.clone(),
            });

        YmShowsSalesDaily {
            base: BaseAggregate::with_metadata(
                YmShowsSalesDailyId::new(uuid),
                m.code,
                m.description,
                m.comment,
                metadata,
            ),
            header,
            totals,
            lines,
            source_meta,
        }
    }
}

pub async fn get_by_id(id: Uuid) -> Result<Option<YmShowsSalesDaily>> {
    let result = Entity::find_by_id(id.to_string()).one(conn()).await?;
    Ok(result.map(Into::into))
}

#[derive(Debug, Clone)]
pub struct YmShowsSalesListQuery {
    pub date_from: Option<String>,
    pub date_to: Option<String>,
    pub connection_id: Option<String>,
    pub search_query: Option<String>,
    pub sort_by: String,
    pub sort_desc: bool,
    pub limit: usize,
    pub offset: usize,
}

#[derive(Debug, Clone)]
pub struct YmShowsSalesListRow {
    pub id: String,
    pub document_no: String,
    pub document_date: String,
    pub campaign_id: Option<String>,
    pub lines_count: i32,
    pub total_shows: Option<i64>,
    pub total_clicks: Option<i64>,
    pub total_to_cart: Option<i64>,
    pub total_order_items: Option<i64>,
    pub connection_id: String,
    pub connection_name: Option<String>,
    pub organization_name: Option<String>,
    pub fetched_at: String,
}

#[derive(Debug, Clone)]
pub struct YmShowsSalesListResult {
    pub items: Vec<YmShowsSalesListRow>,
    pub total: usize,
}

/// Список дневных документов для UI. Названия справочников подмешиваются одним SQL,
/// чтобы список не делал N+1 запросов.
pub async fn list_paginated(query: YmShowsSalesListQuery) -> Result<YmShowsSalesListResult> {
    let db = conn();
    let mut conditions = vec!["d.is_deleted = 0".to_string()];
    let quote = |value: &str| value.replace('\'', "''");

    if let Some(value) = query.date_from.filter(|value| !value.is_empty()) {
        conditions.push(format!("d.document_date >= '{}'", quote(&value)));
    }
    if let Some(value) = query.date_to.filter(|value| !value.is_empty()) {
        conditions.push(format!("d.document_date <= '{}'", quote(&value)));
    }
    if let Some(value) = query.connection_id.filter(|value| !value.is_empty()) {
        conditions.push(format!("d.connection_id = '{}'", quote(&value)));
    }
    if let Some(value) = query.search_query.filter(|value| !value.is_empty()) {
        let value = quote(&value);
        conditions.push(format!(
            "(d.document_no LIKE '%{0}%' OR d.campaign_id LIKE '%{0}%' OR c.description LIKE '%{0}%' OR o.description LIKE '%{0}%')",
            value
        ));
    }

    let where_clause = conditions.join(" AND ");
    let sort_column = match query.sort_by.as_str() {
        "document_no" => "d.document_no",
        "lines_count" => "d.lines_count",
        "total_shows" => "d.total_shows",
        "total_clicks" => "d.total_clicks",
        "total_to_cart" => "d.total_to_cart",
        "total_order_items" => "d.total_order_items",
        "connection_name" => "c.description",
        "organization_name" => "o.description",
        "fetched_at" => "d.fetched_at",
        _ => "d.document_date",
    };
    let sort_dir = if query.sort_desc { "DESC" } else { "ASC" };
    let backend = db.get_database_backend();

    let count_sql = format!(
        "SELECT COUNT(*) AS cnt FROM a041_ym_shows_sales_daily d \
         LEFT JOIN a006_connection_mp c ON c.id=d.connection_id \
         LEFT JOIN a002_organization o ON o.id=d.organization_id WHERE {where_clause}"
    );
    let total = db
        .query_one(Statement::from_string(backend, count_sql))
        .await?
        .and_then(|row| row.try_get::<i64>("", "cnt").ok())
        .unwrap_or(0) as usize;

    let list_sql = format!(
        "SELECT d.id,d.document_no,d.document_date,d.campaign_id,d.lines_count, \
         d.total_shows,d.total_clicks,d.total_to_cart,d.total_order_items, \
         d.connection_id,c.description AS connection_name,o.description AS organization_name,d.fetched_at \
         FROM a041_ym_shows_sales_daily d \
         LEFT JOIN a006_connection_mp c ON c.id=d.connection_id \
         LEFT JOIN a002_organization o ON o.id=d.organization_id \
         WHERE {where_clause} ORDER BY {sort_column} {sort_dir}, d.id ASC LIMIT {} OFFSET {}",
        query.limit, query.offset
    );
    let items = db
        .query_all(Statement::from_string(backend, list_sql))
        .await?
        .into_iter()
        .map(|row| YmShowsSalesListRow {
            id: row.try_get("", "id").unwrap_or_default(),
            document_no: row.try_get("", "document_no").unwrap_or_default(),
            document_date: row.try_get("", "document_date").unwrap_or_default(),
            campaign_id: row.try_get("", "campaign_id").ok(),
            lines_count: row.try_get("", "lines_count").unwrap_or_default(),
            total_shows: row.try_get("", "total_shows").ok(),
            total_clicks: row.try_get("", "total_clicks").ok(),
            total_to_cart: row.try_get("", "total_to_cart").ok(),
            total_order_items: row.try_get("", "total_order_items").ok(),
            connection_id: row.try_get("", "connection_id").unwrap_or_default(),
            connection_name: row.try_get("", "connection_name").ok(),
            organization_name: row.try_get("", "organization_name").ok(),
            fetched_at: row.try_get("", "fetched_at").unwrap_or_default(),
        })
        .collect();

    Ok(YmShowsSalesListResult { items, total })
}

/// Плоская строка воронки `offer_id × дата` для внешних BI-потребителей.
#[derive(Debug, Clone)]
pub struct FunnelProductRow {
    pub date: String,
    pub connection_id: String,
    pub connection_name: Option<String>,
    pub organization_name: Option<String>,
    pub campaign_id: Option<String>,
    pub offer_id: String,
    pub offer_name: String,
    pub marketplace_product_ref: Option<String>,
    pub nomenclature_ref: Option<String>,
    pub brand_name: Option<String>,
    pub category_id: Option<String>,
    pub category_name: Option<String>,
    pub shows: Option<i64>,
    pub clicks: Option<i64>,
    pub cart_count: Option<i64>,
    pub order_count: Option<i64>,
    pub order_sum: Option<i64>,
    pub delivered_count: Option<i64>,
    pub delivered_sum: Option<i64>,
    pub cancel_count: Option<i64>,
    pub return_count: Option<i64>,
    pub click_through_conversion: Option<f64>,
    pub add_to_cart_conversion: Option<f64>,
    pub cart_to_order_conversion: Option<f64>,
}

pub struct FunnelProductRowsResult {
    pub rows: Vec<FunnelProductRow>,
    pub total: usize,
}

#[derive(Debug, Clone)]
struct ProductDimensions {
    brand_name: Option<String>,
    category_id: Option<String>,
    category_name: Option<String>,
}

/// Загружает карту id → description для справочной таблицы (a006/a002).
async fn load_name_map<C: ConnectionTrait>(db: &C, table: &str) -> HashMap<String, String> {
    let sql = format!("SELECT id, description FROM {table}");
    let rows = match db
        .query_all(Statement::from_string(db.get_database_backend(), sql))
        .await
    {
        Ok(rows) => rows,
        Err(error) => {
            tracing::warn!(
                "a041 funnel: failed to load names from {}: {}",
                table,
                error
            );
            return HashMap::new();
        }
    };
    rows.into_iter()
        .filter_map(|row| {
            let id: String = row.try_get("", "id").ok()?;
            let description: String = row.try_get("", "description").ok()?;
            Some((id, description))
        })
        .collect()
}

async fn load_product_dimensions(
    connection_ids: &HashSet<String>,
    product_refs: &HashSet<String>,
) -> Result<HashMap<String, ProductDimensions>> {
    if connection_ids.is_empty() || product_refs.is_empty() {
        return Ok(HashMap::new());
    }

    use crate::domain::a007_marketplace_product::repository as products;

    let models = products::Entity::find()
        .filter(products::Column::IsDeleted.eq(false))
        .filter(products::Column::ConnectionMpRef.is_in(connection_ids.iter().cloned()))
        .all(conn())
        .await?;

    Ok(models
        .into_iter()
        .filter(|model| product_refs.contains(&model.id))
        .map(|model| {
            (
                model.id,
                ProductDimensions {
                    brand_name: model.brand,
                    category_id: model.category_id,
                    category_name: model.category_name,
                },
            )
        })
        .collect())
}

/// Процентная конверсия. Отсутствующие данные и нулевой знаменатель — N/A.
fn conversion_percent(numerator: Option<i64>, denominator: Option<i64>) -> Option<f64> {
    match (numerator, denominator) {
        (Some(numerator), Some(denominator)) if denominator > 0 => {
            Some(numerator as f64 / denominator as f64 * 100.0)
        }
        _ => None,
    }
}

fn sort_and_paginate(
    mut rows: Vec<FunnelProductRow>,
    limit: usize,
    offset: usize,
) -> FunnelProductRowsResult {
    rows.sort_by(|a, b| {
        a.date
            .cmp(&b.date)
            .then_with(|| a.connection_id.cmp(&b.connection_id))
            .then_with(|| a.offer_id.cmp(&b.offer_id))
    });
    let total = rows.len();
    let rows = rows.into_iter().skip(offset).take(limit).collect();
    FunnelProductRowsResult { rows, total }
}

/// Плоские строки воронки за включительный период. Пагинация применяется после
/// разворачивания `lines_json`; `total` — число строк до пагинации.
pub async fn product_rows_for_period(
    date_from: &str,
    date_to: &str,
    connection_id: Option<&str>,
    limit: usize,
    offset: usize,
) -> Result<FunnelProductRowsResult> {
    let db = conn();
    let mut find = Entity::find()
        .filter(Column::IsDeleted.eq(false))
        .filter(Column::DocumentDate.gte(date_from))
        .filter(Column::DocumentDate.lte(date_to));
    if let Some(connection_id) = connection_id.filter(|value| !value.is_empty()) {
        find = find.filter(Column::ConnectionId.eq(connection_id));
    }
    let models = find.order_by_asc(Column::DocumentDate).all(db).await?;

    let parsed: Vec<(Model, Vec<YmShowsSalesDailyLine>)> = models
        .into_iter()
        .map(|model| {
            let lines = serde_json::from_str(&model.lines_json).unwrap_or_default();
            (model, lines)
        })
        .collect();
    let product_refs: HashSet<String> = parsed
        .iter()
        .flat_map(|(_, lines)| lines.iter())
        .filter_map(|line| line.marketplace_product_ref.clone())
        .collect();
    let connection_ids: HashSet<String> = parsed
        .iter()
        .map(|(model, _)| model.connection_id.clone())
        .collect();

    let connection_names = load_name_map(db, "a006_connection_mp").await;
    let organization_names = load_name_map(db, "a002_organization").await;
    let product_dimensions = load_product_dimensions(&connection_ids, &product_refs).await?;

    let mut rows = Vec::new();
    for (model, lines) in parsed {
        let connection_name = connection_names.get(&model.connection_id).cloned();
        let organization_name = organization_names.get(&model.organization_id).cloned();
        for line in lines {
            let dimensions = line
                .marketplace_product_ref
                .as_ref()
                .and_then(|id| product_dimensions.get(id));
            let metrics = &line.metrics;
            rows.push(FunnelProductRow {
                date: model.document_date.clone(),
                connection_id: model.connection_id.clone(),
                connection_name: connection_name.clone(),
                organization_name: organization_name.clone(),
                campaign_id: model.campaign_id.clone(),
                offer_id: line.offer_id,
                offer_name: line.offer_name,
                marketplace_product_ref: line.marketplace_product_ref,
                nomenclature_ref: line.nomenclature_ref,
                brand_name: dimensions.and_then(|value| value.brand_name.clone()),
                category_id: dimensions.and_then(|value| value.category_id.clone()),
                category_name: dimensions.and_then(|value| value.category_name.clone()),
                shows: metrics.shows,
                clicks: metrics.clicks,
                cart_count: metrics.to_cart,
                order_count: metrics.order_items,
                order_sum: metrics.order_sum,
                delivered_count: metrics.delivered_count,
                delivered_sum: metrics.delivered_sum,
                cancel_count: metrics.canceled_count,
                return_count: metrics.returned_count,
                click_through_conversion: conversion_percent(metrics.clicks, metrics.shows),
                add_to_cart_conversion: conversion_percent(metrics.to_cart, metrics.clicks),
                cart_to_order_conversion: conversion_percent(metrics.order_items, metrics.to_cart),
            });
        }
    }

    Ok(sort_and_paginate(rows, limit, offset))
}

/// `id` документов за период — для пересбора воронки (u508).
pub async fn list_ids_by_period(
    date_from: &str,
    date_to: &str,
    connection_mp_refs: &[String],
) -> Result<Vec<String>> {
    let mut query = Entity::find()
        .filter(Column::IsDeleted.eq(false))
        .filter(Column::DocumentDate.gte(date_from))
        .filter(Column::DocumentDate.lte(date_to));
    if !connection_mp_refs.is_empty() {
        query = query.filter(Column::ConnectionId.is_in(connection_mp_refs.to_vec()));
    }
    Ok(query
        .order_by_asc(Column::DocumentDate)
        .all(conn())
        .await?
        .into_iter()
        .map(|item| item.id)
        .collect())
}

/// Точечное перепроведение одного документа: пересобирает его движения p916.
/// Идемпотентно (delete-by-registrator + insert в одной транзакции).
pub async fn post_document(id: Uuid) -> Result<()> {
    let document = get_by_id(id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Document not found: {}", id))?;

    use crate::projections::p916_mp_sales_funnel_turnovers::{
        builder as funnel_builder, repository as funnel_repo,
    };
    let registrator_ref = id.to_string();
    let rows = funnel_builder::from_ym_shows_sales_daily(&document, &registrator_ref);

    let db = get_connection();
    let txn = db.begin().await?;
    funnel_repo::delete_by_registrator_with_conn(&txn, funnel_builder::REG_A041, &registrator_ref)
        .await?;
    funnel_repo::insert_many_with_conn(&txn, &rows).await?;
    txn.commit().await?;
    Ok(())
}

/// Замена периода целиком: удаляем документы кабинета за `[date_from, date_to]`,
/// вставляем новые и в той же транзакции пересобираем маркетинговые движения p916.
/// Отчёт YM пересчитывается задним числом, поэтому окно всегда перезаливается, а не
/// дополняется.
pub async fn replace_for_period(
    connection_id: &str,
    date_from: &str,
    date_to: &str,
    documents: &[YmShowsSalesDaily],
) -> Result<usize> {
    let db = get_connection();
    let started_at = std::time::Instant::now();
    tracing::info!(
        "a041_ym_shows_sales_daily replace_for_period: connection={}, period={}..{}, documents={}",
        connection_id,
        date_from,
        date_to,
        documents.len()
    );
    let txn = db.begin().await?;

    Entity::delete_many()
        .filter(Column::ConnectionId.eq(connection_id))
        .filter(Column::DocumentDate.gte(date_from))
        .filter(Column::DocumentDate.lte(date_to))
        .exec(&txn)
        .await?;

    for document in documents {
        insert_with_conn(&txn, document).await?;
    }

    use crate::projections::p916_mp_sales_funnel_turnovers::{
        builder as funnel_builder, repository as funnel_repo,
    };
    funnel_repo::delete_marketing_for_period_with_conn(
        &txn,
        funnel_builder::REG_A041,
        connection_id,
        date_from,
        date_to,
    )
    .await?;
    for document in documents {
        let registrator_ref = document.base.id.value().to_string();
        let rows = funnel_builder::from_ym_shows_sales_daily(document, &registrator_ref);
        funnel_repo::insert_many_with_conn(&txn, &rows).await?;
    }

    txn.commit().await?;
    tracing::info!(
        "a041_ym_shows_sales_daily replace_for_period: committed connection={}, inserted={}, elapsed_ms={}",
        connection_id,
        documents.len(),
        started_at.elapsed().as_millis()
    );
    Ok(documents.len())
}

async fn insert_with_conn<C: ConnectionTrait>(db: &C, document: &YmShowsSalesDaily) -> Result<()> {
    let header_json = serde_json::to_string(&document.header)?;
    let totals_json = serde_json::to_string(&document.totals)?;
    let lines_json = serde_json::to_string(&document.lines)?;
    let source_meta_json = serde_json::to_string(&document.source_meta)?;

    let active_model = ActiveModel {
        id: Set(document.base.id.value().to_string()),
        code: Set(document.base.code.clone()),
        description: Set(document.base.description.clone()),
        comment: Set(document.base.comment.clone()),
        document_no: Set(document.header.document_no.clone()),
        document_date: Set(document.header.document_date.clone()),
        connection_id: Set(document.header.connection_id.clone()),
        organization_id: Set(document.header.organization_id.clone()),
        marketplace_id: Set(document.header.marketplace_id.clone()),
        campaign_id: Set(document.header.campaign_id.clone()),
        lines_count: Set(document.lines.len() as i32),
        total_shows: Set(document.totals.shows),
        total_clicks: Set(document.totals.clicks),
        total_to_cart: Set(document.totals.to_cart),
        total_order_items: Set(document.totals.order_items),
        total_delivered_count: Set(document.totals.delivered_count),
        total_canceled_count: Set(document.totals.canceled_count),
        total_returned_count: Set(document.totals.returned_count),
        header_json: Set(header_json),
        totals_json: Set(totals_json),
        lines_json: Set(lines_json),
        source_meta_json: Set(source_meta_json),
        fetched_at: Set(document.source_meta.fetched_at.clone()),
        is_deleted: Set(false),
        created_at: Set(Some(Utc::now())),
        updated_at: Set(Some(Utc::now())),
        version: Set(1),
    };

    Entity::insert(active_model).exec(db).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(date: &str, connection_id: &str, offer_id: &str) -> FunnelProductRow {
        FunnelProductRow {
            date: date.into(),
            connection_id: connection_id.into(),
            connection_name: None,
            organization_name: None,
            campaign_id: None,
            offer_id: offer_id.into(),
            offer_name: String::new(),
            marketplace_product_ref: None,
            nomenclature_ref: None,
            brand_name: None,
            category_id: None,
            category_name: None,
            shows: None,
            clicks: None,
            cart_count: None,
            order_count: None,
            order_sum: None,
            delivered_count: None,
            delivered_sum: None,
            cancel_count: None,
            return_count: None,
            click_through_conversion: None,
            add_to_cart_conversion: None,
            cart_to_order_conversion: None,
        }
    }

    #[test]
    fn conversion_preserves_na_and_rejects_zero_denominator() {
        assert_eq!(conversion_percent(Some(25), Some(100)), Some(25.0));
        assert_eq!(conversion_percent(None, Some(100)), None);
        assert_eq!(conversion_percent(Some(10), None), None);
        assert_eq!(conversion_percent(Some(10), Some(0)), None);
    }

    #[test]
    fn pagination_uses_stable_funnel_order_and_keeps_full_total() {
        let result = sort_and_paginate(
            vec![
                row("2026-08-02", "b", "2"),
                row("2026-08-01", "b", "1"),
                row("2026-08-01", "a", "2"),
                row("2026-08-01", "a", "1"),
            ],
            2,
            1,
        );

        assert_eq!(result.total, 4);
        assert_eq!(result.rows.len(), 2);
        assert_eq!(result.rows[0].offer_id, "2");
        assert_eq!(result.rows[1].connection_id, "b");
    }
}
