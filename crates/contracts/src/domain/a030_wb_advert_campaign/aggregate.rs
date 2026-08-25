use crate::domain::common::{AggregateId, AggregateRoot, BaseAggregate, EntityMetadata, Origin};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct WbAdvertCampaignId(pub Uuid);

impl WbAdvertCampaignId {
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

impl AggregateId for WbAdvertCampaignId {
    fn as_string(&self) -> String {
        self.0.to_string()
    }

    fn from_string(s: &str) -> Result<Self, String> {
        Uuid::parse_str(s)
            .map(WbAdvertCampaignId::new)
            .map_err(|e| format!("Invalid UUID: {}", e))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WbAdvertCampaignHeader {
    pub advert_id: i64,
    pub connection_id: String,
    pub organization_id: String,
    pub marketplace_id: String,
    #[serde(default)]
    pub campaign_type: Option<i32>,
    #[serde(default)]
    pub status: Option<i32>,
    #[serde(default)]
    pub change_time: Option<String>,
    /// Number of nm positions (from nm_settings, params[].nms, etc.)
    #[serde(default)]
    pub nm_count: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WbAdvertCampaignSourceMeta {
    pub source: String,
    pub fetched_at: String,
    /// Raw response item from `/api/advert/v2/adverts`; keeps new WB fields without schema churn.
    #[serde(default)]
    pub info_json: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WbAdvertCampaign {
    #[serde(flatten)]
    pub base: BaseAggregate<WbAdvertCampaignId>,
    pub header: WbAdvertCampaignHeader,
    pub source_meta: WbAdvertCampaignSourceMeta,
}

impl WbAdvertCampaign {
    pub fn new_for_insert(
        header: WbAdvertCampaignHeader,
        source_meta: WbAdvertCampaignSourceMeta,
    ) -> Self {
        let code = format!("WB-ADVERT-{}", header.advert_id);
        let description = format!("WB advert campaign {}", header.advert_id);
        Self {
            base: BaseAggregate::new(WbAdvertCampaignId::new_v4(), code, description),
            header,
            source_meta,
        }
    }

    pub fn touch_updated(&mut self) {
        self.base.touch();
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.header.advert_id <= 0 {
            return Err("advert_id должен быть положительным".into());
        }
        if self.header.connection_id.trim().is_empty() {
            return Err("connection_id обязателен".into());
        }
        Ok(())
    }

    pub fn before_write(&mut self) {
        self.base.code = format!("WB-ADVERT-{}", self.header.advert_id);

        // Extract campaign name from WB API response (settings.name or top-level name)
        let name = self
            .source_meta
            .info_json
            .get("settings")
            .and_then(|s| s.get("name"))
            .and_then(|n| n.as_str())
            .filter(|s| !s.trim().is_empty())
            .or_else(|| {
                self.source_meta
                    .info_json
                    .get("name")
                    .and_then(|n| n.as_str())
                    .filter(|s| !s.trim().is_empty())
            });
        self.base.description = name
            .map(|n| n.to_string())
            .unwrap_or_else(|| format!("WB advert campaign {}", self.header.advert_id));

        // Count nm positions across all known WB API response formats
        self.header.nm_count = count_nm_positions(&self.source_meta.info_json);

        self.touch_updated();
    }
}

/// Count nm positions from the raw WB API info_json.
/// Supports all known response formats across campaign types.
pub fn count_nm_positions(info: &serde_json::Value) -> i32 {
    // nm_settings[].nm_id (snake_case, current format)
    if let Some(arr) = info.get("nm_settings").and_then(|v| v.as_array()) {
        if !arr.is_empty() {
            return arr.len() as i32;
        }
    }
    // unitedParams[].nms[]
    if let Some(params) = info.get("unitedParams").and_then(|v| v.as_array()) {
        let n: usize = params
            .iter()
            .filter_map(|p| p.get("nms").and_then(|v| v.as_array()))
            .map(|a| a.len())
            .sum();
        if n > 0 {
            return n as i32;
        }
    }
    // params[].nms[]
    if let Some(params) = info.get("params").and_then(|v| v.as_array()) {
        let n: usize = params
            .iter()
            .filter_map(|p| p.get("nms").and_then(|v| v.as_array()))
            .map(|a| a.len())
            .sum();
        if n > 0 {
            return n as i32;
        }
    }
    // nm[] (top-level)
    if let Some(arr) = info.get("nm").and_then(|v| v.as_array()) {
        if !arr.is_empty() {
            return arr.len() as i32;
        }
    }
    // nmIds[]
    if let Some(arr) = info.get("nmIds").and_then(|v| v.as_array()) {
        if !arr.is_empty() {
            return arr.len() as i32;
        }
    }
    0
}

impl AggregateRoot for WbAdvertCampaign {
    type Id = WbAdvertCampaignId;

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
        "a030"
    }

    fn collection_name() -> &'static str {
        "wb_advert_campaign"
    }

    fn element_name() -> &'static str {
        "Рекламная кампания WB"
    }

    fn list_name() -> &'static str {
        "Рекламные кампании WB"
    }

    fn origin() -> Origin {
        Origin::Marketplace
    }
}
