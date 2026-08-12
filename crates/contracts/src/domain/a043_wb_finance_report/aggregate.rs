//! a043 — финансовый отчёт реализации WB из нового Finance API.
//!
//! Один агрегат соответствует одному ежедневному `reportId`. Детализация хранится
//! внутри документа без проекций и без влияния на legacy p903/Главную книгу.

use crate::domain::common::{
    AggregateId, AggregateRoot, BaseAggregate, EntityMetadata, EventStore, Origin,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct WbFinanceReportId(pub Uuid);

impl WbFinanceReportId {
    pub fn new(value: Uuid) -> Self {
        Self(value)
    }

    pub fn stable(connection_id: &str, period: &str, report_id: &str) -> Self {
        let key = format!("a043_wb_finance_report:{connection_id}:{period}:{report_id}");
        Self(Uuid::new_v5(&Uuid::NAMESPACE_OID, key.as_bytes()))
    }

    pub fn value(&self) -> Uuid {
        self.0
    }
}

impl AggregateId for WbFinanceReportId {
    fn as_string(&self) -> String {
        self.0.to_string()
    }

    fn from_string(s: &str) -> Result<Self, String> {
        Uuid::parse_str(s)
            .map(Self)
            .map_err(|e| format!("Invalid UUID: {e}"))
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WbFinanceReportHeader {
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
    #[serde(default)]
    pub seller_finance_name: String,
    #[serde(default)]
    pub currency: String,
    #[serde(default)]
    pub report_type: Option<i64>,
    #[serde(default)]
    pub retail_amount_sum: Option<String>,
    #[serde(default)]
    pub for_pay_sum: Option<String>,
    #[serde(default)]
    pub avg_sale_percent: Option<Value>,
    #[serde(default)]
    pub delivery_service_sum: Option<String>,
    #[serde(default)]
    pub paid_storage_sum: Option<String>,
    #[serde(default)]
    pub paid_acceptance_sum: Option<String>,
    #[serde(default)]
    pub deduction_sum: Option<String>,
    #[serde(default)]
    pub penalty_sum: Option<String>,
    #[serde(default)]
    pub additional_payment_sum: Option<String>,
    #[serde(default)]
    pub cashback_amount_sum: Option<String>,
    #[serde(default)]
    pub cashback_discount_sum: Option<String>,
    #[serde(default)]
    pub cashback_commission_change_sum: Option<String>,
    #[serde(default)]
    pub payment_schedule: Option<String>,
    #[serde(default)]
    pub bank_payment_sum: Option<String>,
    /// Полный объект из `/sales-reports/list`, включая будущие поля WB.
    #[serde(default)]
    pub raw: Value,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WbFinanceReportSourceMeta {
    pub source: String,
    pub list_endpoint: String,
    pub detail_endpoint: String,
    pub fetched_at: String,
    pub pages_count: i32,
    pub last_rrd_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WbFinanceReport {
    #[serde(flatten)]
    pub base: BaseAggregate<WbFinanceReportId>,
    pub header: WbFinanceReportHeader,
    pub lines: Vec<Value>,
    pub source_meta: WbFinanceReportSourceMeta,
}

impl WbFinanceReport {
    pub fn new_for_insert(
        header: WbFinanceReportHeader,
        lines: Vec<Value>,
        source_meta: WbFinanceReportSourceMeta,
    ) -> Self {
        let id =
            WbFinanceReportId::stable(&header.connection_id, &header.period, &header.report_id);
        let description = format!(
            "WB финансовый отчёт {} за {}–{}",
            header.report_id, header.date_from, header.date_to
        );
        let base = BaseAggregate::new(id, header.document_no.clone(), description);
        Self {
            base,
            header,
            lines,
            source_meta,
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.header.connection_id.trim().is_empty() {
            return Err("connection_id is required".into());
        }
        if self.header.report_id.trim().is_empty() {
            return Err("report_id is required".into());
        }
        if self.header.period != "daily" {
            return Err("a043 supports only daily reports".into());
        }
        Ok(())
    }
}

impl AggregateRoot for WbFinanceReport {
    type Id = WbFinanceReportId;

    fn id(&self) -> Self::Id {
        self.base.id
    }
    fn code(&self) -> &str {
        &self.base.code
    }
    fn description(&self) -> &str {
        &self.base.description
    }
    fn metadata(&self) -> &EntityMetadata {
        &self.base.metadata
    }
    fn metadata_mut(&mut self) -> &mut EntityMetadata {
        &mut self.base.metadata
    }
    fn events(&self) -> &EventStore {
        &self.base.events
    }
    fn events_mut(&mut self) -> &mut EventStore {
        &mut self.base.events
    }
    fn aggregate_index() -> &'static str {
        "a043"
    }
    fn collection_name() -> &'static str {
        "wb_finance_report"
    }
    fn element_name() -> &'static str {
        "Финансовый отчёт WB"
    }
    fn list_name() -> &'static str {
        "Финансовые отчёты WB (новый API)"
    }
    fn origin() -> Origin {
        Origin::Marketplace
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stable_id_is_scoped_by_connection_period_and_report() {
        let a = WbFinanceReportId::stable("c1", "daily", "90071992547409930");
        let b = WbFinanceReportId::stable("c1", "daily", "90071992547409930");
        let c = WbFinanceReportId::stable("c2", "daily", "90071992547409930");
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn raw_money_and_unknown_fields_round_trip_without_f64() {
        let raw = serde_json::json!({
            "reportId": "90071992547409931",
            "forPaySum": "1234567890.123456789",
            "futureWbField": { "nested": true }
        });
        let header = WbFinanceReportHeader {
            connection_id: "connection".into(),
            report_id: "90071992547409931".into(),
            period: "daily".into(),
            for_pay_sum: Some("1234567890.123456789".into()),
            raw: raw.clone(),
            ..Default::default()
        };
        let encoded = serde_json::to_value(&header).unwrap();
        assert_eq!(encoded["forPaySum"], "1234567890.123456789");
        assert_eq!(encoded["raw"]["futureWbField"], raw["futureWbField"]);
    }
}
