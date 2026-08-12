use anyhow::Result;
use chrono::Utc;
use contracts::domain::a043_wb_finance_report::{
    WbFinanceReport, WbFinanceReportHeader, WbFinanceReportId, WbFinanceReportSourceMeta,
};
use contracts::domain::common::{BaseAggregate, EntityMetadata};
use sea_orm::entity::prelude::*;
use sea_orm::{ConnectionTrait, DatabaseBackend, Set, Statement, TransactionTrait};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::shared::data::db::get_connection;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "a043_wb_finance_report")]
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
    pub report_id: String,
    pub period: String,
    pub date_from: String,
    pub date_to: String,
    pub create_date: String,
    pub seller_finance_name: String,
    pub currency: String,
    pub report_type: Option<i64>,
    pub retail_amount_sum: Option<String>,
    pub for_pay_sum: Option<String>,
    pub delivery_service_sum: Option<String>,
    pub paid_storage_sum: Option<String>,
    pub paid_acceptance_sum: Option<String>,
    pub deduction_sum: Option<String>,
    pub penalty_sum: Option<String>,
    pub additional_payment_sum: Option<String>,
    pub cashback_amount_sum: Option<String>,
    pub cashback_discount_sum: Option<String>,
    pub cashback_commission_change_sum: Option<String>,
    pub payment_schedule: Option<String>,
    pub bank_payment_sum: Option<String>,
    pub lines_count: i32,
    pub pages_count: i32,
    pub last_rrd_id: Option<String>,
    pub header_json: String,
    pub lines_json: String,
    pub source_meta_json: String,
    pub fetched_at: String,
    pub is_deleted: bool,
    pub created_at: Option<DateTimeUtc>,
    pub updated_at: Option<DateTimeUtc>,
    pub version: i32,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}
impl ActiveModelBehavior for ActiveModel {}

impl From<Model> for WbFinanceReport {
    fn from(m: Model) -> Self {
        let raw_header: Value = serde_json::from_str(&m.header_json).unwrap_or(Value::Null);
        let header = WbFinanceReportHeader {
            document_no: m.document_no.clone(),
            document_date: m.document_date.clone(),
            connection_id: m.connection_id.clone(),
            organization_id: m.organization_id.clone(),
            marketplace_id: m.marketplace_id.clone(),
            report_id: m.report_id.clone(),
            period: m.period.clone(),
            date_from: m.date_from.clone(),
            date_to: m.date_to.clone(),
            create_date: m.create_date.clone(),
            seller_finance_name: m.seller_finance_name.clone(),
            currency: m.currency.clone(),
            report_type: m.report_type,
            retail_amount_sum: m.retail_amount_sum.clone(),
            for_pay_sum: m.for_pay_sum.clone(),
            avg_sale_percent: raw_header.get("avgSalePercent").cloned(),
            delivery_service_sum: m.delivery_service_sum.clone(),
            paid_storage_sum: m.paid_storage_sum.clone(),
            paid_acceptance_sum: m.paid_acceptance_sum.clone(),
            deduction_sum: m.deduction_sum.clone(),
            penalty_sum: m.penalty_sum.clone(),
            additional_payment_sum: m.additional_payment_sum.clone(),
            cashback_amount_sum: m.cashback_amount_sum.clone(),
            cashback_discount_sum: m.cashback_discount_sum.clone(),
            cashback_commission_change_sum: m.cashback_commission_change_sum.clone(),
            payment_schedule: m.payment_schedule.clone(),
            bank_payment_sum: m.bank_payment_sum.clone(),
            raw: raw_header,
        };
        let source_meta =
            serde_json::from_str(&m.source_meta_json).unwrap_or(WbFinanceReportSourceMeta {
                source: "wb_finance_api_v1".into(),
                fetched_at: m.fetched_at.clone(),
                pages_count: m.pages_count,
                last_rrd_id: m.last_rrd_id.clone(),
                ..Default::default()
            });
        Self {
            base: BaseAggregate::with_metadata(
                WbFinanceReportId::new(Uuid::parse_str(&m.id).unwrap_or_else(|_| Uuid::new_v4())),
                m.code,
                m.description,
                m.comment,
                EntityMetadata {
                    created_at: m.created_at.unwrap_or_else(Utc::now),
                    updated_at: m.updated_at.unwrap_or_else(Utc::now),
                    is_deleted: m.is_deleted,
                    is_posted: false,
                    version: m.version,
                },
            ),
            header,
            lines: serde_json::from_str(&m.lines_json).unwrap_or_default(),
            source_meta,
        }
    }
}

fn active(
    document: &WbFinanceReport,
    created_at: DateTimeUtc,
    version: i32,
) -> Result<ActiveModel> {
    let h = &document.header;
    Ok(ActiveModel {
        id: Set(document.base.id.value().to_string()),
        code: Set(document.base.code.clone()),
        description: Set(document.base.description.clone()),
        comment: Set(document.base.comment.clone()),
        document_no: Set(h.document_no.clone()),
        document_date: Set(h.document_date.clone()),
        connection_id: Set(h.connection_id.clone()),
        organization_id: Set(h.organization_id.clone()),
        marketplace_id: Set(h.marketplace_id.clone()),
        report_id: Set(h.report_id.clone()),
        period: Set(h.period.clone()),
        date_from: Set(h.date_from.clone()),
        date_to: Set(h.date_to.clone()),
        create_date: Set(h.create_date.clone()),
        seller_finance_name: Set(h.seller_finance_name.clone()),
        currency: Set(h.currency.clone()),
        report_type: Set(h.report_type),
        retail_amount_sum: Set(h.retail_amount_sum.clone()),
        for_pay_sum: Set(h.for_pay_sum.clone()),
        delivery_service_sum: Set(h.delivery_service_sum.clone()),
        paid_storage_sum: Set(h.paid_storage_sum.clone()),
        paid_acceptance_sum: Set(h.paid_acceptance_sum.clone()),
        deduction_sum: Set(h.deduction_sum.clone()),
        penalty_sum: Set(h.penalty_sum.clone()),
        additional_payment_sum: Set(h.additional_payment_sum.clone()),
        cashback_amount_sum: Set(h.cashback_amount_sum.clone()),
        cashback_discount_sum: Set(h.cashback_discount_sum.clone()),
        cashback_commission_change_sum: Set(h.cashback_commission_change_sum.clone()),
        payment_schedule: Set(h.payment_schedule.clone()),
        bank_payment_sum: Set(h.bank_payment_sum.clone()),
        lines_count: Set(document.lines.len() as i32),
        pages_count: Set(document.source_meta.pages_count),
        last_rrd_id: Set(document.source_meta.last_rrd_id.clone()),
        header_json: Set(serde_json::to_string(&h.raw)?),
        lines_json: Set(serde_json::to_string(&document.lines)?),
        source_meta_json: Set(serde_json::to_string(&document.source_meta)?),
        fetched_at: Set(document.source_meta.fetched_at.clone()),
        is_deleted: Set(false),
        created_at: Set(Some(created_at)),
        updated_at: Set(Some(Utc::now())),
        version: Set(version),
    })
}

/// Заменяет только один полностью загруженный документ в транзакции.
pub async fn upsert_complete(document: &WbFinanceReport) -> Result<()> {
    let db = get_connection();
    let txn = db.begin().await?;
    let existing = Entity::find_by_id(document.base.id.value().to_string())
        .one(&txn)
        .await?;
    let created_at = existing
        .as_ref()
        .and_then(|m| m.created_at)
        .unwrap_or_else(Utc::now);
    let version = existing.as_ref().map(|m| m.version + 1).unwrap_or(1);
    if existing.is_some() {
        Entity::delete_by_id(document.base.id.value().to_string())
            .exec(&txn)
            .await?;
    }
    Entity::insert(active(document, created_at, version)?)
        .exec(&txn)
        .await?;
    txn.commit().await?;
    Ok(())
}

pub async fn get_by_id(id: Uuid) -> Result<Option<WbFinanceReport>> {
    Ok(Entity::find_by_id(id.to_string())
        .one(get_connection())
        .await?
        .map(Into::into))
}

#[derive(Debug, Clone)]
pub struct FinanceReportListQuery {
    pub date_from: Option<String>,
    pub date_to: Option<String>,
    pub connection_id: Option<String>,
    pub period: Option<String>,
    pub search: Option<String>,
    pub sort_by: String,
    pub sort_desc: bool,
    pub limit: usize,
    pub offset: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct FinanceReportListRow {
    pub id: String,
    pub report_id: String,
    pub period: String,
    pub date_from: String,
    pub date_to: String,
    pub create_date: String,
    pub seller_finance_name: String,
    pub currency: String,
    pub for_pay_sum: Option<String>,
    pub bank_payment_sum: Option<String>,
    pub lines_count: i32,
    pub connection_id: String,
    pub connection_name: Option<String>,
    pub organization_name: Option<String>,
    pub fetched_at: String,
}

pub struct FinanceReportListResult {
    pub items: Vec<FinanceReportListRow>,
    pub total: usize,
}

fn esc(value: &str) -> String {
    value.replace('\'', "''")
}

pub async fn list(query: FinanceReportListQuery) -> Result<FinanceReportListResult> {
    let mut filters = vec!["d.is_deleted = 0".to_string()];
    if let Some(v) = query.date_from.filter(|v| !v.is_empty()) {
        filters.push(format!("d.date_from >= '{}'", esc(&v)));
    }
    if let Some(v) = query.date_to.filter(|v| !v.is_empty()) {
        filters.push(format!("d.date_to <= '{}'", esc(&v)));
    }
    if let Some(v) = query.connection_id.filter(|v| !v.is_empty()) {
        filters.push(format!("d.connection_id = '{}'", esc(&v)));
    }
    if let Some(v) = query.period.filter(|v| !v.is_empty()) {
        filters.push(format!("d.period = '{}'", esc(&v)));
    }
    if let Some(v) = query.search.filter(|v| !v.is_empty()) {
        let v = esc(&v);
        filters.push(format!(
            "(d.report_id LIKE '%{v}%' OR d.seller_finance_name LIKE '%{v}%')"
        ));
    }
    let where_sql = filters.join(" AND ");
    let sort = match query.sort_by.as_str() {
        "report_id" => "d.report_id",
        "date_from" => "d.date_from",
        "date_to" => "d.date_to",
        "seller_finance_name" => "d.seller_finance_name",
        "lines_count" => "d.lines_count",
        "for_pay_sum" => "CAST(d.for_pay_sum AS REAL)",
        "bank_payment_sum" => "CAST(d.bank_payment_sum AS REAL)",
        "fetched_at" => "d.fetched_at",
        _ => "d.create_date",
    };
    let direction = if query.sort_desc { "DESC" } else { "ASC" };
    let base = "FROM a043_wb_finance_report d LEFT JOIN a006_connection_mp c ON c.id=d.connection_id LEFT JOIN a002_organization o ON o.id=d.organization_id";
    let db = get_connection();
    let count = db
        .query_one(Statement::from_string(
            DatabaseBackend::Sqlite,
            format!("SELECT COUNT(*) cnt {base} WHERE {where_sql}"),
        ))
        .await?
        .and_then(|r| r.try_get::<i64>("", "cnt").ok())
        .unwrap_or(0) as usize;
    let sql = format!("SELECT d.id,d.report_id,d.period,d.date_from,d.date_to,d.create_date,d.seller_finance_name,d.currency,d.for_pay_sum,d.bank_payment_sum,d.lines_count,d.connection_id,c.description connection_name,o.description organization_name,d.fetched_at {base} WHERE {where_sql} ORDER BY {sort} {direction} LIMIT {} OFFSET {}", query.limit.min(500), query.offset);
    let rows = db
        .query_all(Statement::from_string(DatabaseBackend::Sqlite, sql))
        .await?;
    let items = rows
        .into_iter()
        .map(|r| FinanceReportListRow {
            id: r.try_get("", "id").unwrap_or_default(),
            report_id: r.try_get("", "report_id").unwrap_or_default(),
            period: r.try_get("", "period").unwrap_or_default(),
            date_from: r.try_get("", "date_from").unwrap_or_default(),
            date_to: r.try_get("", "date_to").unwrap_or_default(),
            create_date: r.try_get("", "create_date").unwrap_or_default(),
            seller_finance_name: r.try_get("", "seller_finance_name").unwrap_or_default(),
            currency: r.try_get("", "currency").unwrap_or_default(),
            for_pay_sum: r.try_get("", "for_pay_sum").ok(),
            bank_payment_sum: r.try_get("", "bank_payment_sum").ok(),
            lines_count: r.try_get("", "lines_count").unwrap_or(0),
            connection_id: r.try_get("", "connection_id").unwrap_or_default(),
            connection_name: r.try_get("", "connection_name").ok(),
            organization_name: r.try_get("", "organization_name").ok(),
            fetched_at: r.try_get("", "fetched_at").unwrap_or_default(),
        })
        .collect();
    Ok(FinanceReportListResult {
        items,
        total: count,
    })
}

#[derive(Debug, Serialize)]
pub struct LinesPage {
    pub items: Vec<Value>,
    pub total: usize,
    pub offset: usize,
    pub limit: usize,
}

pub async fn lines(id: Uuid, offset: usize, limit: usize) -> Result<Option<LinesPage>> {
    let Some(model) = Entity::find_by_id(id.to_string())
        .one(get_connection())
        .await?
    else {
        return Ok(None);
    };
    let all: Vec<Value> = serde_json::from_str(&model.lines_json)?;
    let total = all.len();
    let items = all.into_iter().skip(offset).take(limit).collect();
    Ok(Some(LinesPage {
        items,
        total,
        offset,
        limit,
    }))
}
