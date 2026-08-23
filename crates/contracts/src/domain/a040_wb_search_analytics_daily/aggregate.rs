use crate::domain::common::{
    AggregateId, AggregateRoot, BaseAggregate, EntityMetadata, Origin,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct WbSearchAnalyticsDailyId(pub Uuid);

fn fnv1a64(input: &str) -> u64 {
    const OFFSET: u64 = 0xcbf29ce484222325;
    const PRIME: u64 = 0x100000001b3;
    let mut hash = OFFSET;
    for byte in input.bytes() {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(PRIME);
    }
    hash
}

fn stable_uuid_bytes(key: &str) -> [u8; 16] {
    let h1 = fnv1a64(key);
    let h2 = fnv1a64(&format!("{key}\0salt"));
    let mut bytes = [0u8; 16];
    bytes[..8].copy_from_slice(&h1.to_le_bytes());
    bytes[8..].copy_from_slice(&h2.to_le_bytes());
    bytes
}

impl WbSearchAnalyticsDailyId {
    pub fn new(value: Uuid) -> Self {
        Self(value)
    }

    pub fn new_v4() -> Self {
        Self(Uuid::new_v4())
    }

    /// Детерминированный id снимка по (connection, snapshot_date).
    pub fn stable_for_header(header: &WbSearchAnalyticsDailyHeader) -> Self {
        let key = format!(
            "a040_wb_search_analytics_daily:{}:{}",
            header.connection_id, header.snapshot_date,
        );
        Self(Uuid::from_bytes(stable_uuid_bytes(&key)))
    }

    pub fn value(&self) -> Uuid {
        self.0
    }
}

impl AggregateId for WbSearchAnalyticsDailyId {
    fn as_string(&self) -> String {
        self.0.to_string()
    }

    fn from_string(s: &str) -> Result<Self, String> {
        Uuid::parse_str(s)
            .map(WbSearchAnalyticsDailyId::new)
            .map_err(|e| format!("Invalid UUID: {}", e))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WbSearchAnalyticsDailyHeader {
    pub document_no: String,
    /// Дата снимка (yyyy-mm-dd); также хранится в колонке document_date.
    pub snapshot_date: String,
    pub connection_id: String,
    pub organization_id: String,
    pub marketplace_id: String,
}

/// Метрики поисковой аналитики товара за день (из WB search-report).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WbSearchMetrics {
    /// Показы в поиске/каталоге (органика).
    pub impressions: i64,
    /// Переходы в карточку из поиска (openCard).
    pub open_card: i64,
    /// CTR из выдачи, %.
    pub ctr: f64,
    /// Добавления в корзину.
    pub add_to_cart: i64,
    /// Заказы (атрибуция от поиска).
    pub orders: i64,
    /// Средняя позиция в выдаче.
    pub avg_position: f64,
    /// Видимость (доля/индекс показов из WB).
    pub visibility: f64,
    /// Конверсия переход→корзина, %.
    pub open_to_cart_conv: f64,
    /// Конверсия корзина→заказ, %.
    pub cart_to_order_conv: f64,
}

/// Статистика по одному поисковому запросу для товара (топ-запросы).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WbSearchQueryStat {
    /// Текст поискового запроса.
    pub text: String,
    /// Частотность запроса на WB (сколько раз искали всего).
    pub frequency: i64,
    /// Показы карточки по этому запросу.
    pub impressions: i64,
    /// Клики/переходы по запросу.
    pub clicks: i64,
    /// Заказы по запросу.
    pub orders: i64,
    /// Средняя позиция карточки по запросу.
    pub avg_position: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WbSearchAnalyticsDailyLine {
    pub nm_id: i64,
    pub title: String,
    pub vendor_code: String,
    pub brand_name: String,
    pub subject_id: i64,
    pub subject_name: String,
    pub nomenclature_ref: Option<String>,
    pub metrics: WbSearchMetrics,
    #[serde(default)]
    pub top_queries: Vec<WbSearchQueryStat>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WbSearchAnalyticsDailyTotals {
    pub total_impressions: i64,
    pub total_open_card: i64,
    pub total_orders: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WbSearchAnalyticsDailySourceMeta {
    pub source: String,
    pub fetched_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WbSearchAnalyticsDaily {
    #[serde(flatten)]
    pub base: BaseAggregate<WbSearchAnalyticsDailyId>,
    pub header: WbSearchAnalyticsDailyHeader,
    pub totals: WbSearchAnalyticsDailyTotals,
    pub lines: Vec<WbSearchAnalyticsDailyLine>,
    pub source_meta: WbSearchAnalyticsDailySourceMeta,
}

impl WbSearchAnalyticsDaily {
    pub fn new_for_insert(
        header: WbSearchAnalyticsDailyHeader,
        totals: WbSearchAnalyticsDailyTotals,
        lines: Vec<WbSearchAnalyticsDailyLine>,
        source_meta: WbSearchAnalyticsDailySourceMeta,
    ) -> Self {
        let description = format!("Поисковая аналитика WB за {}", header.snapshot_date);
        let base = BaseAggregate::new(
            WbSearchAnalyticsDailyId::stable_for_header(&header),
            header.document_no.clone(),
            description,
        );

        Self {
            base,
            header,
            totals,
            lines,
            source_meta,
        }
    }

    pub fn touch_updated(&mut self) {
        self.base.touch();
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.header.document_no.trim().is_empty() {
            return Err("Номер документа обязателен".into());
        }
        if self.header.snapshot_date.trim().is_empty() {
            return Err("Дата снимка обязательна".into());
        }
        if self.header.connection_id.trim().is_empty() {
            return Err("Подключение обязательно".into());
        }
        Ok(())
    }

    pub fn before_write(&mut self) {
        self.touch_updated();
    }
}

impl AggregateRoot for WbSearchAnalyticsDaily {
    type Id = WbSearchAnalyticsDailyId;

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
        "a040"
    }

    fn collection_name() -> &'static str {
        "wb_search_analytics_daily"
    }

    fn element_name() -> &'static str {
        "Поисковая аналитика WB"
    }

    fn list_name() -> &'static str {
        "Поисковая аналитика WB"
    }

    fn origin() -> Origin {
        Origin::Marketplace
    }
}
