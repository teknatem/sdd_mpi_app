use anyhow::Result;
use chrono::Utc;
use contracts::domain::a035_ym_settlement_recon::aggregate::{
    YmSettlementRecon, YmSettlementReconHeader, YmSettlementReconId, YmSettlementReconTotals,
};
use contracts::domain::common::{BaseAggregate, EntityMetadata};
use sea_orm::entity::prelude::*;
use sea_orm::{ConnectionTrait, EntityTrait, Set, Statement, Value};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::shared::data::db::get_connection;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "a035_ym_settlement_recon")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub code: String,
    pub description: String,
    pub comment: Option<String>,
    pub bank_order_id: i64,
    pub bank_order_date: String,
    pub connection_id: String,
    pub organization_id: String,
    pub marketplace_id: String,
    pub period_from: String,
    pub period_to: String,
    pub bank_sum: f64,
    pub theoretical_sum: f64,
    pub deviation: f64,
    pub abs_deviation: f64,
    pub header_json: String,
    pub totals_json: String,
    pub lines_json: String,
    pub is_deleted: bool,
    pub is_posted: bool,
    pub created_at: Option<chrono::DateTime<chrono::Utc>>,
    pub updated_at: Option<chrono::DateTime<chrono::Utc>>,
    pub version: i32,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}

impl From<Model> for YmSettlementRecon {
    fn from(m: Model) -> Self {
        let metadata = EntityMetadata {
            created_at: m.created_at.unwrap_or_else(Utc::now),
            updated_at: m.updated_at.unwrap_or_else(Utc::now),
            is_deleted: m.is_deleted,
            is_posted: m.is_posted,
            version: m.version,
        };
        let uuid = Uuid::parse_str(&m.id).unwrap_or_else(|_| Uuid::new_v4());
        let header: YmSettlementReconHeader =
            serde_json::from_str(&m.header_json).unwrap_or(YmSettlementReconHeader {
                bank_order_id: m.bank_order_id,
                bank_order_date: m.bank_order_date.clone(),
                connection_id: m.connection_id.clone(),
                organization_id: m.organization_id.clone(),
                marketplace_id: m.marketplace_id.clone(),
                period_from: m.period_from.clone(),
                period_to: m.period_to.clone(),
            });
        let totals = serde_json::from_str(&m.totals_json).unwrap_or(YmSettlementReconTotals {
            theoretical_sum: m.theoretical_sum,
            bank_sum: m.bank_sum,
            deviation: m.deviation,
        });
        let lines = serde_json::from_str(&m.lines_json).unwrap_or_default();

        YmSettlementRecon {
            base: BaseAggregate::with_metadata(
                YmSettlementReconId::new(uuid),
                m.code,
                m.description,
                m.comment,
                metadata,
            ),
            header,
            totals,
            lines,
        }
    }
}

fn to_active_model(
    document: &YmSettlementRecon,
    created_at: Option<chrono::DateTime<chrono::Utc>>,
) -> Result<ActiveModel> {
    let header_json = serde_json::to_string(&document.header)?;
    let totals_json = serde_json::to_string(&document.totals)?;
    let lines_json = serde_json::to_string(&document.lines)?;

    Ok(ActiveModel {
        id: Set(document.base.id.value().to_string()),
        code: Set(document.base.code.clone()),
        description: Set(document.base.description.clone()),
        comment: Set(document.base.comment.clone()),
        bank_order_id: Set(document.header.bank_order_id),
        bank_order_date: Set(document.header.bank_order_date.clone()),
        connection_id: Set(document.header.connection_id.clone()),
        organization_id: Set(document.header.organization_id.clone()),
        marketplace_id: Set(document.header.marketplace_id.clone()),
        period_from: Set(document.header.period_from.clone()),
        period_to: Set(document.header.period_to.clone()),
        bank_sum: Set(document.totals.bank_sum),
        theoretical_sum: Set(document.totals.theoretical_sum),
        deviation: Set(document.totals.deviation),
        abs_deviation: Set(document.totals.deviation.abs()),
        header_json: Set(header_json),
        totals_json: Set(totals_json),
        lines_json: Set(lines_json),
        is_deleted: Set(document.base.metadata.is_deleted),
        is_posted: Set(document.base.metadata.is_posted),
        created_at: Set(created_at.or(Some(document.base.metadata.created_at))),
        updated_at: Set(Some(Utc::now())),
        version: Set(document.base.metadata.version),
    })
}

/// Upsert по детерминированному id (стабильный по кабинету+ордеру). Возвращает
/// `true`, если документ был создан (новый), `false` — если обновлён.
pub async fn upsert_document(document: &YmSettlementRecon) -> Result<bool> {
    let db = get_connection();
    let existing = Entity::find_by_id(document.base.id.value().to_string())
        .one(db)
        .await?;
    let created_at = existing.as_ref().and_then(|item| item.created_at);
    let active_model = to_active_model(document, created_at)?;
    if existing.is_some() {
        active_model.update(db).await?;
        Ok(false)
    } else {
        active_model.insert(db).await?;
        Ok(true)
    }
}

/// Обновляет существующий документ в рамках переданного соединения/транзакции
/// (для проведения: запись is_posted атомарно с событиями p915). Не создаёт новый.
pub async fn update_with_conn<C: ConnectionTrait>(
    db: &C,
    document: &YmSettlementRecon,
) -> Result<()> {
    let id = document.base.id.value().to_string();
    let existing = Entity::find_by_id(id.clone())
        .one(db)
        .await?
        .ok_or_else(|| anyhow::anyhow!("a035 document not found for update: {}", id))?;
    let active_model = to_active_model(document, existing.created_at)?;
    active_model.update(db).await?;
    Ok(())
}

pub async fn get_by_id(id: Uuid) -> Result<Option<YmSettlementRecon>> {
    let db = get_connection();
    let model = Entity::find_by_id(id.to_string()).one(db).await?;
    Ok(model.map(Into::into))
}

pub async fn exists_with_conn<C: ConnectionTrait>(db: &C, id: &str) -> Result<bool> {
    Ok(Entity::find_by_id(id.to_string()).one(db).await?.is_some())
}

#[derive(Debug, Clone)]
pub struct ReconListQuery {
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
pub struct ReconListRow {
    pub id: String,
    pub bank_order_id: i64,
    pub bank_order_date: String,
    pub period_from: String,
    pub period_to: String,
    pub connection_id: String,
    pub connection_name: Option<String>,
    pub organization_name: Option<String>,
    pub bank_sum: f64,
    pub theoretical_sum: f64,
    pub deviation: f64,
    /// Σ строк p907, агрегированных в обороты этого ордера.
    pub rows_count: i64,
    /// Схема(ы) фасилитации строк ордера (`model` в p907): FBS / FBY / DBS.
    /// YM делит выплаты по модели, поэтому ордер обычно однороден.
    pub model: String,
    pub is_posted: bool,
}

#[derive(Debug, Clone)]
pub struct ReconListResult {
    pub items: Vec<ReconListRow>,
    pub total: usize,
}

pub async fn list_sql(query: ReconListQuery) -> Result<ReconListResult> {
    let db = get_connection();

    let mut conditions = vec!["d.is_deleted = 0".to_string()];
    if let Some(ref date_from) = query.date_from {
        if !date_from.is_empty() {
            conditions.push(format!(
                "d.bank_order_date >= '{}'",
                date_from.replace('\'', "''")
            ));
        }
    }
    if let Some(ref date_to) = query.date_to {
        if !date_to.is_empty() {
            conditions.push(format!(
                "d.bank_order_date <= '{}'",
                date_to.replace('\'', "''")
            ));
        }
    }
    if let Some(ref connection_id) = query.connection_id {
        if !connection_id.is_empty() {
            conditions.push(format!(
                "d.connection_id = '{}'",
                connection_id.replace('\'', "''")
            ));
        }
    }
    if let Some(ref search) = query.search_query {
        if !search.is_empty() {
            let escaped = search.replace('\'', "''");
            conditions.push(format!(
                "(d.code LIKE '%{0}%' OR d.description LIKE '%{0}%' OR c.description LIKE '%{0}%' OR o.description LIKE '%{0}%')",
                escaped
            ));
        }
    }

    let where_clause = conditions.join(" AND ");
    let sort_column = match query.sort_by.as_str() {
        "bank_order_id" => "d.bank_order_id",
        "bank_order_date" => "d.bank_order_date",
        "bank_sum" => "d.bank_sum",
        "theoretical_sum" => "d.theoretical_sum",
        "deviation" => "d.deviation",
        "abs_deviation" => "d.abs_deviation",
        "connection_name" => "c.description",
        _ => "d.bank_order_date",
    };
    let sort_dir = if query.sort_desc { "DESC" } else { "ASC" };

    let count_sql = format!(
        "SELECT COUNT(*) as cnt
         FROM a035_ym_settlement_recon d
         LEFT JOIN a006_connection_mp c ON c.id = d.connection_id
         LEFT JOIN a002_organization o ON o.id = d.organization_id
         WHERE {}",
        where_clause
    );

    let list_sql = format!(
        "SELECT
            d.id,
            d.bank_order_id,
            d.bank_order_date,
            d.period_from,
            d.period_to,
            d.connection_id,
            c.description as connection_name,
            o.description as organization_name,
            d.bank_sum,
            d.theoretical_sum,
            d.deviation,
            d.is_posted,
            (SELECT COALESCE(SUM(json_extract(li.value, '$.rows_count')), 0)
             FROM json_each(d.lines_json) li) AS rows_count,
            (SELECT GROUP_CONCAT(DISTINCT p.model)
             FROM p907_ym_payment_report p
             WHERE p.connection_mp_ref = d.connection_id
               AND p.bank_order_id = d.bank_order_id) AS model
         FROM a035_ym_settlement_recon d
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
        .map(|row| ReconListRow {
            id: row.try_get("", "id").unwrap_or_default(),
            bank_order_id: row.try_get("", "bank_order_id").unwrap_or(0),
            bank_order_date: row.try_get("", "bank_order_date").unwrap_or_default(),
            period_from: row.try_get("", "period_from").unwrap_or_default(),
            period_to: row.try_get("", "period_to").unwrap_or_default(),
            connection_id: row.try_get("", "connection_id").unwrap_or_default(),
            connection_name: row.try_get("", "connection_name").ok(),
            organization_name: row.try_get("", "organization_name").ok(),
            bank_sum: row.try_get("", "bank_sum").unwrap_or(0.0),
            theoretical_sum: row.try_get("", "theoretical_sum").unwrap_or(0.0),
            deviation: row.try_get("", "deviation").unwrap_or(0.0),
            rows_count: row.try_get("", "rows_count").unwrap_or(0),
            model: row
                .try_get::<Option<String>>("", "model")
                .unwrap_or(None)
                .unwrap_or_default(),
            is_posted: row.try_get("", "is_posted").unwrap_or(false),
        })
        .collect();

    Ok(ReconListResult { items, total })
}

/// Схема(ы) фасилитации ордера: distinct `model` строк p907 (для карточки).
pub async fn order_models(connection_id: &str, bank_order_id: i64) -> Result<String> {
    let db = get_connection();
    let sql = "SELECT GROUP_CONCAT(DISTINCT model) AS model
               FROM p907_ym_payment_report
               WHERE connection_mp_ref = ? AND bank_order_id = ?";
    let stmt = Statement::from_sql_and_values(
        db.get_database_backend(),
        sql,
        vec![
            Value::String(Some(Box::new(connection_id.to_string()))),
            Value::BigInt(Some(bank_order_id)),
        ],
    );
    let row = db.query_one(stmt).await?;
    Ok(row
        .and_then(|r| r.try_get::<Option<String>>("", "model").ok().flatten())
        .unwrap_or_default())
}

/// Заказ, попавший в банковский ордер: агрегат строк p907 по `order_id`.
/// Используется в карточке a035 для перечня оплаченных заказов.
#[derive(Debug, Clone)]
pub struct OrderBreakdownRow {
    pub order_id: i64,
    /// UUID агрегата a013 (`document_no = order_id`) — для гиперссылки. `None`,
    /// если заказ ещё не загружен в a013.
    pub ym_order_id: Option<String>,
    /// Нормализованный статус заказа (`a013.status_norm`).
    pub status: Option<String>,
    /// Дата заказа (`order_creation_date`, YYYY-MM-DD).
    pub order_date: String,
    /// Σ `transaction_sum` всех строк заказа в этом ордере (сумма перечисления).
    pub amount: f64,
    /// Число строк p907 заказа в этом ордере.
    pub rows_count: i64,
}

/// Заказы, попавшие в банковский ордер: все строки p907 с непустым `order_id`,
/// сгруппированные по заказу, с датой заказа, статусом и суммой перечисления.
/// LEFT JOIN к a013 (по `document_no`) даёт UUID для ссылки и статус заказа.
pub async fn order_breakdown(
    connection_id: &str,
    bank_order_id: i64,
) -> Result<Vec<OrderBreakdownRow>> {
    let db = get_connection();
    let sql = "SELECT p.order_id AS order_id,
                      MAX(a.id) AS ym_order_id,
                      MAX(a.status_norm) AS status,
                      substr(MIN(p.order_creation_date), 1, 10) AS order_date,
                      SUM(p.transaction_sum) AS amount,
                      COUNT(*) AS cnt
               FROM p907_ym_payment_report p
               LEFT JOIN a013_ym_order a ON a.document_no = CAST(p.order_id AS TEXT)
               WHERE p.connection_mp_ref = ? AND p.bank_order_id = ?
                 AND p.order_id IS NOT NULL
               GROUP BY p.order_id
               ORDER BY order_date DESC, p.order_id DESC";
    let stmt = Statement::from_sql_and_values(
        db.get_database_backend(),
        sql,
        vec![
            Value::String(Some(Box::new(connection_id.to_string()))),
            Value::BigInt(Some(bank_order_id)),
        ],
    );
    let rows = db.query_all(stmt).await?;
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let order_id: i64 = row.try_get("", "order_id").unwrap_or_default();
        if order_id == 0 {
            continue;
        }
        out.push(OrderBreakdownRow {
            order_id,
            ym_order_id: row
                .try_get::<Option<String>>("", "ym_order_id")
                .unwrap_or(None),
            status: row.try_get::<Option<String>>("", "status").unwrap_or(None),
            order_date: row.try_get("", "order_date").unwrap_or_default(),
            amount: row.try_get("", "amount").unwrap_or(0.0),
            rows_count: row.try_get("", "cnt").unwrap_or(0),
        });
    }
    Ok(out)
}

/// Один расчёт по заказу в рамках банковского ордера: оплата поставщику или
/// удержание (возврат) ранее перечисленной оплаты при возврате товара.
#[derive(Debug, Clone)]
pub struct SettledOrder {
    pub order_id: String,
    /// Сумма `transaction_sum` (для возврата — отрицательная).
    pub amount: f64,
    /// `true` — строка «Возврат платежа покупателя» (удержание у поставщика).
    pub is_return: bool,
}

/// Заказы, рассчитанные в банковском ордере: строки «Платёж покупателя»
/// (оплата поставщику) и «Возврат платежа покупателя» (удержание при возврате),
/// сгруппированные по `order_id` с суммой `transaction_sum`. Используется при
/// проведении a035 для записи событий «Дата оплаты поставщику» /
/// «Возврат оплаты поставщику» в p915.
pub async fn settled_orders(connection_id: &str, bank_order_id: i64) -> Result<Vec<SettledOrder>> {
    let db = get_connection();
    let sql = "SELECT order_id, transaction_source, SUM(transaction_sum) AS amount
               FROM p907_ym_payment_report
               WHERE connection_mp_ref = ? AND bank_order_id = ?
                 AND transaction_source IN ('Платёж покупателя', 'Возврат платежа покупателя')
                 AND order_id IS NOT NULL
               GROUP BY order_id, transaction_source";
    let stmt = Statement::from_sql_and_values(
        db.get_database_backend(),
        sql,
        vec![
            Value::String(Some(Box::new(connection_id.to_string()))),
            Value::BigInt(Some(bank_order_id)),
        ],
    );
    let rows = db.query_all(stmt).await?;
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let order_id: i64 = row.try_get("", "order_id").unwrap_or_default();
        let amount: f64 = row.try_get("", "amount").unwrap_or(0.0);
        let source: String = row.try_get("", "transaction_source").unwrap_or_default();
        if order_id != 0 {
            out.push(SettledOrder {
                order_id: order_id.to_string(),
                amount,
                is_return: source == "Возврат платежа покупателя",
            });
        }
    }
    Ok(out)
}
