use crate::domain::common::{AggregateId, AggregateRoot, BaseAggregate, EntityMetadata, Origin};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct WbSalesFunnelDailyId(pub Uuid);

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

impl WbSalesFunnelDailyId {
    pub fn new(value: Uuid) -> Self {
        Self(value)
    }

    pub fn new_v4() -> Self {
        Self(Uuid::new_v4())
    }

    /// Детерминированный id документа по (connection, date).
    /// Один документ = один кабинет + одна дата; повторный импорт
    /// того же периода не плодит новые UUID.
    pub fn stable_for_header(header: &WbSalesFunnelDailyHeader) -> Self {
        let key = format!(
            "a036_wb_sales_funnel_daily:{}:{}",
            header.connection_id, header.document_date,
        );
        Self(Uuid::from_bytes(stable_uuid_bytes(&key)))
    }

    pub fn value(&self) -> Uuid {
        self.0
    }
}

impl AggregateId for WbSalesFunnelDailyId {
    fn as_string(&self) -> String {
        self.0.to_string()
    }

    fn from_string(s: &str) -> Result<Self, String> {
        Uuid::parse_str(s)
            .map(WbSalesFunnelDailyId::new)
            .map_err(|e| format!("Invalid UUID: {}", e))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WbSalesFunnelDailyHeader {
    pub document_no: String,
    pub document_date: String,
    pub connection_id: String,
    pub organization_id: String,
    pub marketplace_id: String,
    /// Валюта отчёта из API воронки (обычно RUB).
    #[serde(default)]
    pub currency: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WbSalesFunnelDailyMetrics {
    /// Переходы в карточку товара.
    pub open_count: i64,
    /// Положили в корзину, шт.
    pub cart_count: i64,
    /// Заказали товаров, шт.
    pub order_count: i64,
    /// Заказали на сумму.
    pub order_sum: f64,
    /// Выкупили товаров, шт.
    pub buyout_count: i64,
    /// Выкупили на сумму.
    pub buyout_sum: f64,
    /// Отменили товаров, шт. `None` — источник счётчик не отдал (N/A ≠ 0):
    /// у документов, импортированных до подключения отмен, и у путей API,
    /// где поля нет. CSV-отчёт `DETAIL_HISTORY_REPORT` отдаёт его всегда.
    #[serde(default)]
    pub cancel_count: Option<i64>,
    /// Отменили на сумму. `None` — см. `cancel_count`.
    #[serde(default)]
    pub cancel_sum: Option<f64>,
    /// Процент выкупа.
    pub buyout_percent: f64,
    /// Конверсия в корзину, %.
    pub add_to_cart_conversion: f64,
    /// Конверсия в заказ, %.
    pub cart_to_order_conversion: f64,
    /// Добавления в «Отложенные».
    pub add_to_wishlist_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WbSalesFunnelDailyLine {
    pub nm_id: i64,
    pub title: String,
    pub vendor_code: String,
    pub brand_name: String,
    pub subject_id: i64,
    pub subject_name: String,
    pub nomenclature_ref: Option<String>,
    pub metrics: WbSalesFunnelDailyMetrics,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WbSalesFunnelDailySourceMeta {
    pub source: String,
    pub fetched_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WbSalesFunnelDaily {
    #[serde(flatten)]
    pub base: BaseAggregate<WbSalesFunnelDailyId>,
    pub header: WbSalesFunnelDailyHeader,
    pub totals: WbSalesFunnelDailyMetrics,
    pub lines: Vec<WbSalesFunnelDailyLine>,
    pub source_meta: WbSalesFunnelDailySourceMeta,
}

impl WbSalesFunnelDaily {
    pub fn new_for_insert(
        header: WbSalesFunnelDailyHeader,
        totals: WbSalesFunnelDailyMetrics,
        lines: Vec<WbSalesFunnelDailyLine>,
        source_meta: WbSalesFunnelDailySourceMeta,
    ) -> Self {
        let description = format!("Воронка продаж WB за {}", header.document_date);
        let base = BaseAggregate::new(
            WbSalesFunnelDailyId::stable_for_header(&header),
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
        if self.header.document_date.trim().is_empty() {
            return Err("Дата документа обязательна".into());
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

impl AggregateRoot for WbSalesFunnelDaily {
    type Id = WbSalesFunnelDailyId;

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
        "a036"
    }

    fn collection_name() -> &'static str {
        "wb_sales_funnel_daily"
    }

    fn element_name() -> &'static str {
        "Воронка продаж WB"
    }

    fn list_name() -> &'static str {
        "Воронка продаж WB"
    }

    fn origin() -> Origin {
        Origin::Marketplace
    }
}
