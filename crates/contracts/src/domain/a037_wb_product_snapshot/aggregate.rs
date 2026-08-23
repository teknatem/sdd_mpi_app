use crate::domain::common::{
    AggregateId, AggregateRoot, BaseAggregate, EntityMetadata, Origin,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct WbProductSnapshotId(pub Uuid);

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

impl WbProductSnapshotId {
    pub fn new(value: Uuid) -> Self {
        Self(value)
    }

    pub fn new_v4() -> Self {
        Self(Uuid::new_v4())
    }

    /// Детерминированный id снимка по (connection, snapshot_date).
    /// Один снимок = один кабинет + одна дата; повторный сбор за тот же день
    /// не плодит новые UUID.
    pub fn stable_for_header(header: &WbProductSnapshotHeader) -> Self {
        let key = format!(
            "a037_wb_product_snapshot:{}:{}",
            header.connection_id, header.snapshot_date,
        );
        Self(Uuid::from_bytes(stable_uuid_bytes(&key)))
    }

    pub fn value(&self) -> Uuid {
        self.0
    }
}

impl AggregateId for WbProductSnapshotId {
    fn as_string(&self) -> String {
        self.0.to_string()
    }

    fn from_string(s: &str) -> Result<Self, String> {
        Uuid::parse_str(s)
            .map(WbProductSnapshotId::new)
            .map_err(|e| format!("Invalid UUID: {}", e))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WbProductSnapshotHeader {
    pub document_no: String,
    /// Дата снятия снимка (yyyy-mm-dd); также хранится в колонке document_date.
    pub snapshot_date: String,
    pub connection_id: String,
    pub organization_id: String,
    pub marketplace_id: String,
}

/// Состояние товара на дату снимка — сырые значения из WB (`product.stocks` + рейтинги).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WbProductSnapshotState {
    /// Остаток на складах WB, шт.
    pub stock_wb: i64,
    /// Остаток на складах продавца, шт.
    pub stock_mp: i64,
    /// Сумма остатков.
    pub stock_balance_sum: f64,
    /// Рейтинг карточки товара.
    pub product_rating: f64,
    /// Оценка покупателей.
    pub feedback_rating: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WbProductSnapshotLine {
    pub nm_id: i64,
    pub title: String,
    pub vendor_code: String,
    pub brand_name: String,
    pub subject_id: i64,
    pub subject_name: String,
    pub nomenclature_ref: Option<String>,
    pub state: WbProductSnapshotState,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WbProductSnapshotTotals {
    pub total_stock_wb: i64,
    pub total_stock_mp: i64,
    pub total_balance_sum: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WbProductSnapshotSourceMeta {
    pub source: String,
    pub fetched_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WbProductSnapshot {
    #[serde(flatten)]
    pub base: BaseAggregate<WbProductSnapshotId>,
    pub header: WbProductSnapshotHeader,
    pub totals: WbProductSnapshotTotals,
    pub lines: Vec<WbProductSnapshotLine>,
    pub source_meta: WbProductSnapshotSourceMeta,
}

impl WbProductSnapshot {
    pub fn new_for_insert(
        header: WbProductSnapshotHeader,
        totals: WbProductSnapshotTotals,
        lines: Vec<WbProductSnapshotLine>,
        source_meta: WbProductSnapshotSourceMeta,
    ) -> Self {
        let description = format!("Данные по товарам WB за {}", header.snapshot_date);
        let base = BaseAggregate::new(
            WbProductSnapshotId::stable_for_header(&header),
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

impl AggregateRoot for WbProductSnapshot {
    type Id = WbProductSnapshotId;

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
        "a037"
    }

    fn collection_name() -> &'static str {
        "wb_product_snapshot"
    }

    fn element_name() -> &'static str {
        "Данные по товарам WB"
    }

    fn list_name() -> &'static str {
        "Данные по товарам WB"
    }

    fn origin() -> Origin {
        Origin::Marketplace
    }
}
