//! a041 — дневная воронка Яндекс.Маркета из отчёта «Аналитика продаж» (shows-sales).
//!
//! Единственный источник ПОКАЗОВ и кликов YM: в orders-API (a013) их нет. Документ —
//! кабинет × дата, строки — товары (`offer_id`, он же `shop_sku` строк заказа a013).
//!
//! Все метрики опциональны: YM не гарантирует полный набор колонок на всех тарифах,
//! а глубина истории ограничена подпиской (90 дней без неё, 400 на «Медиум»).
//! `None` — «данных нет» (N/A), и подменять его нулём нельзя: нулевые показы и
//! отсутствующие показы дают принципиально разные конверсии.
//!
//! Роль в воронке — стадия `marketing` проекции p916 (аналог a036 у Wildberries).
//! Фактические заказы/отмены/выкупы YM живут в стадии `fulfillment` из a013/a016 и
//! с этими счётчиками НЕ совпадают: тут дневной счётчик самого маркетплейса.

use crate::domain::common::{
    AggregateId, AggregateRoot, BaseAggregate, EntityMetadata, Origin,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct YmShowsSalesDailyId(pub Uuid);

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

impl YmShowsSalesDailyId {
    pub fn new(value: Uuid) -> Self {
        Self(value)
    }

    pub fn new_v4() -> Self {
        Self(Uuid::new_v4())
    }

    /// Детерминированный id по (кабинет, дата): повторный импорт того же периода
    /// не плодит новые UUID, а перезаписывает документ.
    pub fn stable_for_header(header: &YmShowsSalesDailyHeader) -> Self {
        let key = format!(
            "a041_ym_shows_sales_daily:{}:{}",
            header.connection_id, header.document_date,
        );
        Self(Uuid::from_bytes(stable_uuid_bytes(&key)))
    }

    pub fn value(&self) -> Uuid {
        self.0
    }
}

impl AggregateId for YmShowsSalesDailyId {
    fn as_string(&self) -> String {
        self.0.to_string()
    }

    fn from_string(s: &str) -> Result<Self, String> {
        Uuid::parse_str(s)
            .map(YmShowsSalesDailyId::new)
            .map_err(|e| format!("Invalid UUID: {}", e))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YmShowsSalesDailyHeader {
    pub document_no: String,
    /// Дата воронки, `YYYY-MM-DD`.
    pub document_date: String,
    pub connection_id: String,
    pub organization_id: String,
    pub marketplace_id: String,
    /// Кампания (магазин) YM, по которой запрошен отчёт.
    #[serde(default)]
    pub campaign_id: Option<String>,
}

/// Метрики воронки YM за день. `None` — счётчика в отчёте не было (N/A ≠ 0).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct YmShowsSalesDailyMetrics {
    /// Показы моих товаров, шт. Настоящие impressions.
    #[serde(default)]
    pub shows: Option<i64>,
    /// Показы товаров всех продавцов по тому же MSKU — объём рынка, не мои показы.
    /// В воронку не входит: делить свои клики на чужие показы бессмысленно.
    #[serde(default)]
    pub by_msku_shows: Option<i64>,
    /// Клики по товарам — переходы в карточку (аналог open_count у WB).
    #[serde(default)]
    pub clicks: Option<i64>,
    /// Добавления в корзину, шт.
    #[serde(default)]
    pub to_cart: Option<i64>,
    /// Заказанные товары, шт. (счётчик маркетплейса, ≠ заказы из a013).
    #[serde(default)]
    pub order_items: Option<i64>,
    /// Доставлено за период, шт.
    #[serde(default)]
    pub delivered_count: Option<i64>,
    /// Отмены и невыкупы за период, шт. — счётчик отказов YM.
    #[serde(default)]
    pub canceled_count: Option<i64>,
    /// Возвращённые товары за период, шт.
    #[serde(default)]
    pub returned_count: Option<i64>,
}

impl YmShowsSalesDailyMetrics {
    /// Есть ли хоть одна ненулевая метрика (разреженность документа/проекции).
    pub fn has_activity(&self) -> bool {
        [
            self.shows,
            self.clicks,
            self.to_cart,
            self.order_items,
            self.delivered_count,
            self.canceled_count,
            self.returned_count,
        ]
        .iter()
        .any(|v| v.unwrap_or(0) != 0)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YmShowsSalesDailyLine {
    /// «Ваш SKU» — ключ товара; совпадает с `shop_sku` строк заказа a013.
    pub offer_id: String,
    pub offer_name: String,
    /// Ссылка на a007_marketplace_product (резолвится при импорте по offer_id).
    #[serde(default)]
    pub marketplace_product_ref: Option<String>,
    /// Ссылка на a004_nomenclature (через a007).
    #[serde(default)]
    pub nomenclature_ref: Option<String>,
    pub metrics: YmShowsSalesDailyMetrics,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YmShowsSalesDailySourceMeta {
    pub source: String,
    pub fetched_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YmShowsSalesDaily {
    #[serde(flatten)]
    pub base: BaseAggregate<YmShowsSalesDailyId>,
    pub header: YmShowsSalesDailyHeader,
    pub totals: YmShowsSalesDailyMetrics,
    pub lines: Vec<YmShowsSalesDailyLine>,
    pub source_meta: YmShowsSalesDailySourceMeta,
}

impl YmShowsSalesDaily {
    pub fn new_for_insert(
        header: YmShowsSalesDailyHeader,
        totals: YmShowsSalesDailyMetrics,
        lines: Vec<YmShowsSalesDailyLine>,
        source_meta: YmShowsSalesDailySourceMeta,
    ) -> Self {
        let description = format!("Воронка продаж YM за {}", header.document_date);
        let base = BaseAggregate::new(
            YmShowsSalesDailyId::stable_for_header(&header),
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

impl AggregateRoot for YmShowsSalesDaily {
    type Id = YmShowsSalesDailyId;

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
        "a041"
    }

    fn collection_name() -> &'static str {
        "ym_shows_sales_daily"
    }

    fn element_name() -> &'static str {
        "Воронка продаж YM"
    }

    fn list_name() -> &'static str {
        "Воронка продаж YM"
    }

    fn origin() -> Origin {
        Origin::Marketplace
    }
}
