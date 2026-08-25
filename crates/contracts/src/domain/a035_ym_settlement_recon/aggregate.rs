use crate::domain::common::{AggregateId, AggregateRoot, BaseAggregate, EntityMetadata, Origin};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct YmSettlementReconId(pub Uuid);

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

impl YmSettlementReconId {
    pub fn new(value: Uuid) -> Self {
        Self(value)
    }

    pub fn new_v4() -> Self {
        Self(Uuid::new_v4())
    }

    /// Детерминированный id документа по (кабинет, банковский ордер). Повторный
    /// прогон команды «Сформировать ордера» всегда получает тот же UUID — upsert
    /// не плодит дубли и обновляет существующий документ.
    pub fn stable_for_order(connection_id: &str, bank_order_id: i64) -> Self {
        let key = format!("a035_ym_settlement_recon:{connection_id}:{bank_order_id}");
        Self(Uuid::from_bytes(stable_uuid_bytes(&key)))
    }

    pub fn value(&self) -> Uuid {
        self.0
    }
}

impl AggregateId for YmSettlementReconId {
    fn as_string(&self) -> String {
        self.0.to_string()
    }

    fn from_string(s: &str) -> Result<Self, String> {
        Uuid::parse_str(s)
            .map(YmSettlementReconId::new)
            .map_err(|e| format!("Invalid UUID: {}", e))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YmSettlementReconHeader {
    /// Номер банковского ордера YM (`p907.bank_order_id`). Один документ = один ордер.
    pub bank_order_id: i64,
    /// Дата банковского ордера (`p907.bank_order_date`, YYYY-MM-DD).
    pub bank_order_date: String,
    pub connection_id: String,
    pub organization_id: String,
    pub marketplace_id: String,
    /// Период покрытых ордером операций (min/max `p907.transaction_date`).
    pub period_from: String,
    pub period_to: String,
}

/// Строка таблицы сверки — один наш оборот (turnover_code). `amount` — сумма
/// `transaction_sum` строк p907 этого ордера, отнесённых к данному обороту.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReconLine {
    pub turnover_code: String,
    pub turnover_name: String,
    pub amount: f64,
    pub rows_count: i32,
}

/// Итоги сверки: «теоретическая» сумма по нашим оборотам vs факт YM (`bank_sum`).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct YmSettlementReconTotals {
    /// Σ amount по строкам-оборотам (как должно быть по нашим представлениям).
    pub theoretical_sum: f64,
    /// Факт YM — итог банковского ордера (`p907.bank_sum`).
    pub bank_sum: f64,
    /// theoretical_sum − bank_sum. ≈0 ⇒ ордер сходится.
    pub deviation: f64,
}

impl YmSettlementReconTotals {
    pub fn from_parts(lines: &[ReconLine], bank_sum: f64) -> Self {
        let theoretical_sum: f64 = lines.iter().map(|l| l.amount).sum();
        Self {
            theoretical_sum,
            bank_sum,
            deviation: theoretical_sum - bank_sum,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YmSettlementRecon {
    #[serde(flatten)]
    pub base: BaseAggregate<YmSettlementReconId>,
    pub header: YmSettlementReconHeader,
    pub totals: YmSettlementReconTotals,
    /// Таблица оборотов — по строке на turnover_code.
    #[serde(default)]
    pub lines: Vec<ReconLine>,
}

impl YmSettlementRecon {
    pub fn new_for_insert(
        header: YmSettlementReconHeader,
        lines: Vec<ReconLine>,
        bank_sum: f64,
    ) -> Self {
        let totals = YmSettlementReconTotals::from_parts(&lines, bank_sum);
        let description = format!(
            "Сверка перечисления YM: ордер {} от {} (кабинет {})",
            header.bank_order_id, header.bank_order_date, header.connection_id
        );
        let base = BaseAggregate::new(
            YmSettlementReconId::stable_for_order(&header.connection_id, header.bank_order_id),
            header.bank_order_id.to_string(),
            description,
        );

        Self {
            base,
            header,
            totals,
            lines,
        }
    }

    /// Абсолютное расхождение (для подсветки в списке).
    pub fn abs_deviation(&self) -> f64 {
        self.totals.deviation.abs()
    }

    pub fn touch_updated(&mut self) {
        self.base.touch();
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.header.bank_order_id == 0 {
            return Err("Номер банковского ордера обязателен".into());
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

impl AggregateRoot for YmSettlementRecon {
    type Id = YmSettlementReconId;

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
        "a035"
    }

    fn collection_name() -> &'static str {
        "ym_settlement_recon"
    }

    fn element_name() -> &'static str {
        "Сверка перечислений YM"
    }

    fn list_name() -> &'static str {
        "Сверка перечислений YM"
    }

    fn origin() -> Origin {
        Origin::Marketplace
    }
}
