use crate::domain::common::{AggregateId, AggregateRoot, BaseAggregate, EntityMetadata, Origin};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct WbDocumentId(pub Uuid);

impl WbDocumentId {
    pub fn new(value: Uuid) -> Self {
        Self(value)
    }

    pub fn new_v4() -> Self {
        Self(Uuid::new_v4())
    }

    pub fn value(&self) -> Uuid {
        self.0
    }
}

impl AggregateId for WbDocumentId {
    fn as_string(&self) -> String {
        self.0.to_string()
    }

    fn from_string(s: &str) -> Result<Self, String> {
        Uuid::parse_str(s)
            .map(WbDocumentId::new)
            .map_err(|e| format!("Invalid UUID: {}", e))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WbDocumentHeader {
    pub service_name: String,
    pub name: String,
    pub category: String,
    pub extensions: Vec<String>,
    pub creation_time: String,
    pub viewed: bool,
    pub connection_id: String,
    pub organization_id: String,
    pub marketplace_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WbDocumentSourceMeta {
    pub fetched_at: String,
    pub locale: String,
    pub document_version: i32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WbWeeklyReportManualData {
    #[serde(default)]
    pub realized_goods_total: Option<f64>,
    #[serde(default)]
    pub wb_reward_with_vat: Option<f64>,
    #[serde(default)]
    pub seller_transfer_total: Option<f64>,
    #[serde(default)]
    pub other_deductions: Option<f64>,
    #[serde(default)]
    pub logistics: Option<f64>,
    #[serde(default)]
    pub acquiring: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WbDocument {
    #[serde(flatten)]
    pub base: BaseAggregate<WbDocumentId>,
    pub header: WbDocumentHeader,
    pub is_weekly_report: bool,
    pub report_period_from: Option<String>,
    pub report_period_to: Option<String>,
    pub weekly_report_data: WbWeeklyReportManualData,
    /// Maximum reconciliation deviation (by absolute amount) across the 6 indicators,
    /// computed and stored at posting time.
    #[serde(default)]
    pub max_deviation: Option<f64>,
    pub source_meta: WbDocumentSourceMeta,
}

impl WbDocument {
    pub fn new_for_insert(header: WbDocumentHeader, source_meta: WbDocumentSourceMeta) -> Self {
        let description = format!("WB Document {} ({})", header.service_name, header.category);
        let base = BaseAggregate::new(
            WbDocumentId::new_v4(),
            header.service_name.clone(),
            description,
        );

        Self {
            base,
            header,
            is_weekly_report: false,
            report_period_from: None,
            report_period_to: None,
            weekly_report_data: WbWeeklyReportManualData::default(),
            max_deviation: None,
            source_meta,
        }
    }

    pub fn touch_updated(&mut self) {
        self.base.touch();
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.header.service_name.trim().is_empty() {
            return Err("service_name обязателен".into());
        }
        if self.header.name.trim().is_empty() {
            return Err("name обязателен".into());
        }
        if self.header.category.trim().is_empty() {
            return Err("category обязателен".into());
        }
        if self.header.creation_time.trim().is_empty() {
            return Err("creation_time обязателен".into());
        }
        if self.header.connection_id.trim().is_empty() {
            return Err("connection_id обязателен".into());
        }
        Ok(())
    }

    pub fn before_write(&mut self) {
        self.base.code = self.header.service_name.clone();
        self.base.description = format!(
            "WB Document {} ({})",
            self.header.service_name, self.header.category
        );
        self.touch_updated();
    }
}

impl AggregateRoot for WbDocument {
    type Id = WbDocumentId;

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

    fn aggregate_index() -> &'static str {
        "a027"
    }

    fn collection_name() -> &'static str {
        "wb_documents"
    }

    fn element_name() -> &'static str {
        "Документ WB"
    }

    fn list_name() -> &'static str {
        "Документы WB"
    }

    fn origin() -> Origin {
        Origin::Marketplace
    }
}
