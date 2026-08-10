use anyhow::Result;
use chrono::Utc;
use contracts::domain::a036_wb_sales_funnel_daily::aggregate::{
    WbSalesFunnelDaily, WbSalesFunnelDailyHeader, WbSalesFunnelDailyId, WbSalesFunnelDailyLine,
    WbSalesFunnelDailyMetrics, WbSalesFunnelDailySourceMeta,
};
use contracts::domain::common::{BaseAggregate, EntityMetadata};
use sea_orm::entity::prelude::*;
use sea_orm::{ConnectionTrait, EntityTrait, QueryFilter, Set, Statement, TransactionTrait};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

use crate::shared::data::db::get_connection;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "a036_wb_sales_funnel_daily")]
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
    pub currency: String,
    pub lines_count: i32,
    pub total_open_count: i64,
    pub total_cart_count: i64,
    pub total_order_count: i64,
    pub total_order_sum: f64,
    pub total_buyout_count: i64,
    pub total_buyout_sum: f64,
    /// Отмены по счётчику воронки за день. NULL — источник счётчик не отдал
    /// (документы до подключения отмен), это N/A, а не ноль.
    #[sea_orm(nullable)]
    pub total_cancel_count: Option<i64>,
    #[sea_orm(nullable)]
    pub total_cancel_sum: Option<f64>,
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

impl From<Model> for WbSalesFunnelDaily {
    fn from(m: Model) -> Self {
        let metadata = EntityMetadata {
            created_at: m.created_at.unwrap_or_else(Utc::now),
            updated_at: m.updated_at.unwrap_or_else(Utc::now),
            is_deleted: m.is_deleted,
            is_posted: false,
            version: m.version,
        };
        let uuid = Uuid::parse_str(&m.id).unwrap_or_else(|_| Uuid::new_v4());
        let header: WbSalesFunnelDailyHeader =
            serde_json::from_str(&m.header_json).unwrap_or(WbSalesFunnelDailyHeader {
                document_no: m.document_no.clone(),
                document_date: m.document_date.clone(),
                connection_id: m.connection_id.clone(),
                organization_id: m.organization_id.clone(),
                marketplace_id: m.marketplace_id.clone(),
                currency: m.currency.clone(),
            });
        let totals = serde_json::from_str(&m.totals_json).unwrap_or_default();
        let lines = serde_json::from_str(&m.lines_json).unwrap_or_default();
        let source_meta =
            serde_json::from_str(&m.source_meta_json).unwrap_or(WbSalesFunnelDailySourceMeta {
                source: "wb_sales_funnel".to_string(),
                fetched_at: m.fetched_at.clone(),
            });

        WbSalesFunnelDaily {
            base: BaseAggregate::with_metadata(
                WbSalesFunnelDailyId::new(uuid),
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

/// Проведение одного документа a036 = пересборка его маркетинговых движений p916
/// (стадия 1). Идемпотентно: delete-by-registrator + insert из текущего документа
/// в одной транзакции. registrator_ref = id документа. Импортный `replace_for_period`
/// продолжает заменять весь период; эта функция — точечное перепроведение из UI.
pub async fn post_document(id: Uuid) -> Result<()> {
    let document = get_by_id(id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Document not found: {}", id))?;

    use crate::projections::p916_mp_sales_funnel_turnovers::{
        builder as funnel_builder, repository as funnel_repo,
    };
    let registrator_ref = id.to_string();
    let rows = funnel_builder::from_wb_funnel_daily(&document, &registrator_ref);

    let db = get_connection();
    let txn = db.begin().await?;
    funnel_repo::delete_by_registrator_with_conn(&txn, funnel_builder::REG_A036, &registrator_ref)
        .await?;
    funnel_repo::insert_many_with_conn(&txn, &rows).await?;
    txn.commit().await?;
    Ok(())
}

pub async fn replace_for_period(
    connection_id: &str,
    date_from: &str,
    date_to: &str,
    documents: &[WbSalesFunnelDaily],
) -> Result<usize> {
    let db = get_connection();
    let started_at = std::time::Instant::now();
    tracing::info!(
        "a036_wb_sales_funnel_daily replace_for_period: connection={}, period={}..{}, documents={}",
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

    // Стадия 1 воронки p916: заменяем маркетинговые движения периода целиком
    // (delete-by-period + insert из документов), в той же транзакции.
    use crate::projections::p916_mp_sales_funnel_turnovers::{
        builder as funnel_builder, repository as funnel_repo,
    };
    funnel_repo::delete_marketing_for_period_with_conn(
        &txn,
        funnel_builder::REG_A036,
        connection_id,
        date_from,
        date_to,
    )
    .await?;
    for document in documents {
        let registrator_ref = document.base.id.value().to_string();
        let rows = funnel_builder::from_wb_funnel_daily(document, &registrator_ref);
        funnel_repo::insert_many_with_conn(&txn, &rows).await?;
    }

    txn.commit().await?;
    tracing::info!(
        "a036_wb_sales_funnel_daily replace_for_period: committed connection={}, inserted={}, elapsed_ms={}",
        connection_id,
        documents.len(),
        started_at.elapsed().as_millis()
    );
    Ok(documents.len())
}

async fn insert_with_conn<C: ConnectionTrait>(db: &C, document: &WbSalesFunnelDaily) -> Result<()> {
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
        currency: Set(document.header.currency.clone()),
        lines_count: Set(document.lines.len() as i32),
        total_open_count: Set(document.totals.open_count),
        total_cart_count: Set(document.totals.cart_count),
        total_order_count: Set(document.totals.order_count),
        total_order_sum: Set(document.totals.order_sum),
        total_buyout_count: Set(document.totals.buyout_count),
        total_buyout_sum: Set(document.totals.buyout_sum),
        total_cancel_count: Set(document.totals.cancel_count),
        total_cancel_sum: Set(document.totals.cancel_sum),
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

pub async fn get_by_id(id: Uuid) -> Result<Option<WbSalesFunnelDaily>> {
    let db = get_connection();
    let model = Entity::find_by_id(id.to_string()).one(db).await?;
    Ok(model.map(Into::into))
}

/// Разовый бэкфилл стадии 1 воронки p916 из всех сохранённых документов a036.
/// Полностью перестраивает a036-движения (wipe + rebuild) в одной транзакции —
/// полезно для истории, накопленной глубже текущего окна выдачи WB. Возвращает
/// число вставленных строк движений.
pub async fn backfill_stage1_funnel() -> Result<usize> {
    use crate::projections::p916_mp_sales_funnel_turnovers::{
        builder as funnel_builder, repository as funnel_repo,
    };

    let db = get_connection();
    let models = Entity::find()
        .filter(Column::IsDeleted.eq(false))
        .all(db)
        .await?;

    let txn = db.begin().await?;
    funnel_repo::delete_all_by_registrator_type_with_conn(&txn, funnel_builder::REG_A036).await?;

    let mut inserted = 0usize;
    for model in models {
        let document: WbSalesFunnelDaily = model.into();
        let registrator_ref = document.base.id.value().to_string();
        let rows = funnel_builder::from_wb_funnel_daily(&document, &registrator_ref);
        funnel_repo::insert_many_with_conn(&txn, &rows).await?;
        inserted += rows.len();
    }

    txn.commit().await?;
    tracing::info!(
        "a036 → p916 stage-1 backfill: inserted {} movements",
        inserted
    );
    Ok(inserted)
}

/// Пересобрать стадию 1 воронки p916 за период `[date_from, date_to]` из сохранённых
/// документов a036 (без обращения к API WB). Пустой `connection_ids` → все кабинеты,
/// встреченные в периоде. Идемпотентно: сначала удаляются маркетинговые движения периода
/// по каждому кабинету, затем вставляются заново. Возвращает число вставленных строк.
pub async fn rebuild_stage1_for_period(
    connection_ids: &[String],
    date_from: &str,
    date_to: &str,
) -> Result<usize> {
    use crate::projections::p916_mp_sales_funnel_turnovers::{
        builder as funnel_builder, repository as funnel_repo,
    };

    let db = get_connection();
    let mut query = Entity::find()
        .filter(Column::IsDeleted.eq(false))
        .filter(Column::DocumentDate.gte(date_from))
        .filter(Column::DocumentDate.lte(date_to));
    if !connection_ids.is_empty() {
        query = query.filter(Column::ConnectionId.is_in(connection_ids.iter().cloned()));
    }
    let models = query.all(db).await?;

    // Кабинеты для очистки: явный список либо различные из найденных документов.
    let connections: Vec<String> = if connection_ids.is_empty() {
        let mut set: Vec<String> = models.iter().map(|m| m.connection_id.clone()).collect();
        set.sort();
        set.dedup();
        set
    } else {
        connection_ids.to_vec()
    };

    let txn = db.begin().await?;
    for connection_id in &connections {
        funnel_repo::delete_marketing_for_period_with_conn(
            &txn,
            funnel_builder::REG_A036,
            connection_id,
            date_from,
            date_to,
        )
        .await?;
    }

    let mut inserted = 0usize;
    for model in models {
        let document: WbSalesFunnelDaily = model.into();
        let registrator_ref = document.base.id.value().to_string();
        let rows = funnel_builder::from_wb_funnel_daily(&document, &registrator_ref);
        funnel_repo::insert_many_with_conn(&txn, &rows).await?;
        inserted += rows.len();
    }

    txn.commit().await?;
    tracing::info!(
        "a036 → p916 stage-1 rebuild for period {}..{}: inserted {} movements",
        date_from,
        date_to,
        inserted
    );
    Ok(inserted)
}

/// Суммарные метрики воронки по товару (nm_id) за период.
#[derive(Debug, Clone)]
pub struct FunnelProductMetrics {
    pub nm_id: i64,
    pub open_count: i64,
    pub cart_count: i64,
    pub order_count: i64,
}

/// Сумма метрик воронки (переходы/в корзину/заказы) по каждому nm_id
/// за период [date_from, date_to] для указанного кабинета.
/// Метрики вложены в `metrics`, поэтому агрегируем в памяти через десериализацию
/// строк (json-путь `$.metrics.*` был бы хрупким).
pub async fn product_metrics_sum(
    connection_id: &str,
    date_from: &str,
    date_to: &str,
) -> Result<Vec<FunnelProductMetrics>> {
    let db = get_connection();
    let models = Entity::find()
        .filter(Column::ConnectionId.eq(connection_id))
        .filter(Column::IsDeleted.eq(false))
        .filter(Column::DocumentDate.gte(date_from))
        .filter(Column::DocumentDate.lte(date_to))
        .all(db)
        .await?;

    let mut acc: HashMap<i64, (i64, i64, i64)> = HashMap::new();
    for m in models {
        let lines: Vec<WbSalesFunnelDailyLine> =
            serde_json::from_str(&m.lines_json).unwrap_or_default();
        for line in lines {
            let entry = acc.entry(line.nm_id).or_insert((0, 0, 0));
            entry.0 += line.metrics.open_count;
            entry.1 += line.metrics.cart_count;
            entry.2 += line.metrics.order_count;
        }
    }

    Ok(acc
        .into_iter()
        .map(
            |(nm_id, (open_count, cart_count, order_count))| FunnelProductMetrics {
                nm_id,
                open_count,
                cart_count,
                order_count,
            },
        )
        .collect())
}

/// Плоская строка воронки на уровне `nm_id × дата` — для внешней выгрузки
/// (Power BI и прочие BI-потребители). Разворачивает `lines_json` документов,
/// добавляя дату и имена кабинета/организации.
#[derive(Debug, Clone)]
pub struct FunnelProductRow {
    pub date: String,
    pub connection_id: String,
    pub connection_name: Option<String>,
    pub organization_name: Option<String>,
    pub currency: String,
    pub nm_id: i64,
    pub vendor_code: String,
    pub brand_name: String,
    pub subject_id: i64,
    pub subject_name: String,
    pub title: String,
    pub open_count: i64,
    pub add_to_wishlist_count: i64,
    pub cart_count: i64,
    pub order_count: i64,
    pub order_sum: f64,
    pub buyout_count: i64,
    pub buyout_sum: f64,
    /// Отмены по счётчику воронки. `None` — счётчика в источнике не было (N/A ≠ 0).
    pub cancel_count: Option<i64>,
    pub cancel_sum: Option<f64>,
    pub buyout_percent: f64,
    pub add_to_cart_conversion: f64,
    pub cart_to_order_conversion: f64,
}

pub struct FunnelProductRowsResult {
    pub rows: Vec<FunnelProductRow>,
    pub total: usize,
}

/// Загружает карту id → description для справочной таблицы (a006/a002).
async fn load_name_map<C: ConnectionTrait>(db: &C, table: &str) -> HashMap<String, String> {
    let sql = format!("SELECT id, description FROM {table}");
    let rows = match db
        .query_all(Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            sql,
        ))
        .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!("load_name_map({table}) failed: {e}");
            return HashMap::new();
        }
    };
    rows.into_iter()
        .filter_map(|row| {
            let id: String = row.try_get("", "id").ok()?;
            let desc: String = row.try_get("", "description").unwrap_or_default();
            Some((id, desc))
        })
        .collect()
}

/// Плоские строки воронки (одна на `nm_id × дата`) за период `[date_from, date_to]`,
/// опционально по одному кабинету. Пагинация применяется к развёрнутым строкам:
/// `total` — их общее число, возвращается срез `[offset .. offset+limit]`.
pub async fn product_rows_for_period(
    date_from: &str,
    date_to: &str,
    connection_id: Option<&str>,
    limit: usize,
    offset: usize,
) -> Result<FunnelProductRowsResult> {
    let db = get_connection();

    let mut find = Entity::find()
        .filter(Column::IsDeleted.eq(false))
        .filter(Column::DocumentDate.gte(date_from))
        .filter(Column::DocumentDate.lte(date_to));
    if let Some(cid) = connection_id {
        if !cid.is_empty() {
            find = find.filter(Column::ConnectionId.eq(cid));
        }
    }
    let models = find.all(db).await?;

    let conn_names = load_name_map(db, "a006_connection_mp").await;
    let org_names = load_name_map(db, "a002_organization").await;

    let mut rows: Vec<FunnelProductRow> = Vec::new();
    for m in models {
        let lines: Vec<WbSalesFunnelDailyLine> =
            serde_json::from_str(&m.lines_json).unwrap_or_default();
        let connection_name = conn_names.get(&m.connection_id).cloned();
        let organization_name = org_names.get(&m.organization_id).cloned();
        for line in lines {
            let mt = &line.metrics;
            rows.push(FunnelProductRow {
                date: m.document_date.clone(),
                connection_id: m.connection_id.clone(),
                connection_name: connection_name.clone(),
                organization_name: organization_name.clone(),
                currency: m.currency.clone(),
                nm_id: line.nm_id,
                vendor_code: line.vendor_code.clone(),
                brand_name: line.brand_name.clone(),
                subject_id: line.subject_id,
                subject_name: line.subject_name.clone(),
                title: line.title.clone(),
                open_count: mt.open_count,
                add_to_wishlist_count: mt.add_to_wishlist_count,
                cart_count: mt.cart_count,
                order_count: mt.order_count,
                order_sum: mt.order_sum,
                buyout_count: mt.buyout_count,
                buyout_sum: mt.buyout_sum,
                cancel_count: mt.cancel_count,
                cancel_sum: mt.cancel_sum,
                buyout_percent: mt.buyout_percent,
                add_to_cart_conversion: mt.add_to_cart_conversion,
                cart_to_order_conversion: mt.cart_to_order_conversion,
            });
        }
    }

    // Детерминированный порядок, чтобы offset/limit давали стабильные страницы.
    rows.sort_by(|a, b| {
        a.date
            .cmp(&b.date)
            .then_with(|| a.connection_id.cmp(&b.connection_id))
            .then_with(|| a.nm_id.cmp(&b.nm_id))
    });

    let total = rows.len();
    let page = rows.into_iter().skip(offset).take(limit).collect();

    Ok(FunnelProductRowsResult { rows: page, total })
}

/// Строка выгрузки позиций (`nm_id × дата`) в формате вкладки «Позиции» карточки
/// документа плюс дата — для CSV-экспорта со страницы списка.
#[derive(Debug, Clone)]
pub struct WbSalesFunnelExportRow {
    pub document_date: String,
    pub nm_id: i64,
    pub title: String,
    pub vendor_code: String,
    pub brand_name: String,
    pub subject_name: String,
    pub nomenclature_article: Option<String>,
    pub metrics: WbSalesFunnelDailyMetrics,
}

/// Позиции всех документов, попадающих под фильтры списка (период/кабинет/поиск),
/// без пагинации. Артикул 1С резолвится одним запросом в a004.
pub async fn export_rows(
    query: WbSalesFunnelDailyListQuery,
) -> Result<Vec<WbSalesFunnelExportRow>> {
    let db = get_connection();
    let where_clause = build_list_where(&query);

    let sql = format!(
        "SELECT d.document_date, d.lines_json
         FROM a036_wb_sales_funnel_daily d
         LEFT JOIN a006_connection_mp c ON c.id = d.connection_id
         LEFT JOIN a002_organization o ON o.id = d.organization_id
         WHERE {}
         ORDER BY d.document_date ASC",
        where_clause
    );

    let models = db
        .query_all(Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            sql,
        ))
        .await?;

    let articles = load_name_map_col(db, "a004_nomenclature", "article").await;

    let mut rows: Vec<WbSalesFunnelExportRow> = Vec::new();
    for model in models {
        let document_date: String = model.try_get("", "document_date").unwrap_or_default();
        let lines_json: String = model.try_get("", "lines_json").unwrap_or_default();
        let lines: Vec<WbSalesFunnelDailyLine> =
            serde_json::from_str(&lines_json).unwrap_or_default();

        for line in lines {
            let nomenclature_article = line
                .nomenclature_ref
                .as_ref()
                .and_then(|nom_ref| articles.get(nom_ref).cloned());

            rows.push(WbSalesFunnelExportRow {
                document_date: document_date.clone(),
                nm_id: line.nm_id,
                title: line.title,
                vendor_code: line.vendor_code,
                brand_name: line.brand_name,
                subject_name: line.subject_name,
                nomenclature_article,
                metrics: line.metrics,
            });
        }
    }

    Ok(rows)
}

/// Карта id → произвольная текстовая колонка справочника.
async fn load_name_map_col<C: ConnectionTrait>(
    db: &C,
    table: &str,
    column: &str,
) -> HashMap<String, String> {
    let sql = format!("SELECT id, {column} AS value FROM {table}");
    let rows = match db
        .query_all(Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            sql,
        ))
        .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!("load_name_map_col({table}.{column}) failed: {e}");
            return HashMap::new();
        }
    };
    rows.into_iter()
        .filter_map(|row| {
            let id: String = row.try_get("", "id").ok()?;
            let value: String = row.try_get("", "value").unwrap_or_default();
            Some((id, value))
        })
        .collect()
}

#[derive(Debug, Clone)]
pub struct WbSalesFunnelDailyListQuery {
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
pub struct WbSalesFunnelDailyListRow {
    pub id: String,
    pub document_no: String,
    pub document_date: String,
    pub currency: String,
    pub lines_count: i32,
    pub total_open_count: i64,
    pub total_cart_count: i64,
    pub total_order_count: i64,
    pub total_order_sum: f64,
    pub total_buyout_count: i64,
    pub total_buyout_sum: f64,
    /// `None` — счётчика отмен в источнике не было (N/A ≠ 0).
    pub total_cancel_count: Option<i64>,
    pub total_cancel_sum: Option<f64>,
    pub connection_id: String,
    pub connection_name: Option<String>,
    pub organization_name: Option<String>,
    pub fetched_at: String,
}

#[derive(Debug, Clone)]
pub struct WbSalesFunnelDailyListResult {
    pub items: Vec<WbSalesFunnelDailyListRow>,
    pub total: usize,
}

/// WHERE для списка документов (алиасы `d`/`c`/`o`), общий для списка и выгрузки.
fn build_list_where(query: &WbSalesFunnelDailyListQuery) -> String {
    let mut conditions = vec!["d.is_deleted = 0".to_string()];

    if let Some(ref date_from) = query.date_from {
        if !date_from.is_empty() {
            conditions.push(format!("d.document_date >= '{}'", date_from));
        }
    }
    if let Some(ref date_to) = query.date_to {
        if !date_to.is_empty() {
            conditions.push(format!("d.document_date <= '{}'", date_to));
        }
    }
    if let Some(ref connection_id) = query.connection_id {
        if !connection_id.is_empty() {
            conditions.push(format!("d.connection_id = '{}'", connection_id));
        }
    }
    if let Some(ref search) = query.search_query {
        if !search.is_empty() {
            let escaped = search.replace('\'', "''");
            conditions.push(format!(
                "(d.document_no LIKE '%{0}%' OR c.description LIKE '%{0}%' OR o.description LIKE '%{0}%')",
                escaped
            ));
        }
    }

    conditions.join(" AND ")
}

pub async fn list_sql(query: WbSalesFunnelDailyListQuery) -> Result<WbSalesFunnelDailyListResult> {
    let db = get_connection();

    let where_clause = build_list_where(&query);
    let sort_column = match query.sort_by.as_str() {
        "document_no" => "d.document_no",
        "document_date" => "d.document_date",
        "lines_count" => "d.lines_count",
        "total_open_count" => "d.total_open_count",
        "total_cart_count" => "d.total_cart_count",
        "total_order_count" => "d.total_order_count",
        "total_order_sum" => "d.total_order_sum",
        "total_buyout_count" => "d.total_buyout_count",
        "total_buyout_sum" => "d.total_buyout_sum",
        "total_cancel_count" => "d.total_cancel_count",
        "total_cancel_sum" => "d.total_cancel_sum",
        "connection_name" => "c.description",
        "organization_name" => "o.description",
        "fetched_at" => "d.fetched_at",
        _ => "d.document_date",
    };
    let sort_dir = if query.sort_desc { "DESC" } else { "ASC" };

    let count_sql = format!(
        "SELECT COUNT(*) as cnt
         FROM a036_wb_sales_funnel_daily d
         LEFT JOIN a006_connection_mp c ON c.id = d.connection_id
         LEFT JOIN a002_organization o ON o.id = d.organization_id
         WHERE {}",
        where_clause
    );

    let list_sql = format!(
        "SELECT
            d.id,
            d.document_no,
            d.document_date,
            d.currency,
            d.lines_count,
            d.total_open_count,
            d.total_cart_count,
            d.total_order_count,
            d.total_order_sum,
            d.total_buyout_count,
            d.total_buyout_sum,
            d.total_cancel_count,
            d.total_cancel_sum,
            d.connection_id,
            c.description as connection_name,
            o.description as organization_name,
            d.fetched_at
         FROM a036_wb_sales_funnel_daily d
         LEFT JOIN a006_connection_mp c ON c.id = d.connection_id
         LEFT JOIN a002_organization o ON o.id = d.organization_id
         WHERE {}
         ORDER BY {} {}
         LIMIT {} OFFSET {}",
        where_clause, sort_column, sort_dir, query.limit, query.offset
    );

    let count_result = db
        .query_one(Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            count_sql,
        ))
        .await?;

    let total = count_result
        .and_then(|row| row.try_get::<i64>("", "cnt").ok())
        .unwrap_or(0) as usize;

    let rows = db
        .query_all(Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            list_sql,
        ))
        .await?;

    let items = rows
        .into_iter()
        .map(|row| WbSalesFunnelDailyListRow {
            id: row.try_get("", "id").unwrap_or_default(),
            document_no: row.try_get("", "document_no").unwrap_or_default(),
            document_date: row.try_get("", "document_date").unwrap_or_default(),
            currency: row.try_get("", "currency").unwrap_or_default(),
            lines_count: row.try_get("", "lines_count").unwrap_or(0),
            total_open_count: row.try_get("", "total_open_count").unwrap_or(0),
            total_cart_count: row.try_get("", "total_cart_count").unwrap_or(0),
            total_order_count: row.try_get("", "total_order_count").unwrap_or(0),
            total_order_sum: row.try_get("", "total_order_sum").unwrap_or(0.0),
            total_buyout_count: row.try_get("", "total_buyout_count").unwrap_or(0),
            total_buyout_sum: row.try_get("", "total_buyout_sum").unwrap_or(0.0),
            total_cancel_count: row.try_get("", "total_cancel_count").ok(),
            total_cancel_sum: row.try_get("", "total_cancel_sum").ok(),
            connection_id: row.try_get("", "connection_id").unwrap_or_default(),
            connection_name: row.try_get("", "connection_name").ok(),
            organization_name: row.try_get("", "organization_name").ok(),
            fetched_at: row.try_get("", "fetched_at").unwrap_or_default(),
        })
        .collect();

    Ok(WbSalesFunnelDailyListResult { items, total })
}
