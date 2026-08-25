use crate::domain::common::{AggregateId, AggregateRoot, BaseAggregate, EntityMetadata, Origin};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct WbAdvertDailyId(pub Uuid);

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

impl WbAdvertDailyId {
    pub fn new(value: Uuid) -> Self {
        Self(value)
    }

    pub fn new_v4() -> Self {
        Self(Uuid::new_v4())
    }

    /// Детерминированный id документа по (connection, advert_id, date).
    /// Один и тот же суточный отчёт WB всегда получает один UUID — перепроведение
    /// и replace_for_period не плодят «осиротевшие» p913 от случайных id.
    /// FNV-1a → 16 байт, без feature `v5` у uuid (и без лишних зависимостей).
    pub fn stable_for_header(header: &WbAdvertDailyHeader) -> Self {
        let key = format!(
            "a026_wb_advert_daily:{}:{}:{}",
            header.connection_id, header.advert_id, header.document_date,
        );
        Self(Uuid::from_bytes(stable_uuid_bytes(&key)))
    }

    pub fn value(&self) -> Uuid {
        self.0
    }
}

impl AggregateId for WbAdvertDailyId {
    fn as_string(&self) -> String {
        self.0.to_string()
    }

    fn from_string(s: &str) -> Result<Self, String> {
        Uuid::parse_str(s)
            .map(WbAdvertDailyId::new)
            .map_err(|e| format!("Invalid UUID: {}", e))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WbAdvertDailyHeader {
    pub document_no: String,
    pub document_date: String,
    /// Идентификатор рекламной кампании WB; один документ = одна дата + один advert_id.
    #[serde(default)]
    pub advert_id: i64,
    pub connection_id: String,
    pub organization_id: String,
    pub marketplace_id: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WbAdvertDailyMetrics {
    pub views: i64,
    pub clicks: i64,
    pub ctr: f64,
    pub cpc: f64,
    pub atbs: i64,
    pub orders: i64,
    pub shks: i64,
    pub sum: f64,
    pub sum_price: f64,
    pub cr: f64,
    pub canceled: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WbAdvertDailyLine {
    pub nm_id: i64,
    pub nm_name: String,
    pub nomenclature_ref: Option<String>,
    pub advert_ids: Vec<i64>,
    #[serde(default)]
    pub app_types: Vec<i32>,
    #[serde(default)]
    pub placements: Vec<String>,
    pub metrics: WbAdvertDailyMetrics,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WbAdvertDailySourceMeta {
    pub source: String,
    pub fetched_at: String,
}

/// Связанный заказ a015, найденный по тому же nm_id, connection и дате.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WbAdvertFoundOrder {
    /// srid из a015 (header.document_no).
    pub order_key: String,
    /// UUID документа a015 (base.id). None для старых данных до миграции.
    #[serde(default)]
    pub order_id: Option<String>,
    /// Дата заказа (order_dt из a015.state), формат "yyyy-mm-dd".
    #[serde(default)]
    pub order_date: Option<String>,
    pub nomenclature_ref: Option<String>,
    pub finished_price: Option<f64>,
    /// Заказ отменён (a015.state.is_cancel = true). Отображаем как
    /// информацию; в распределении рекламного расхода участвует наравне с
    /// активными — последующая обработка отмен идёт отдельным процессом.
    #[serde(default)]
    pub is_cancel: bool,
    #[serde(default)]
    pub allocation_basis: f64,
    /// true — заказ попал в расчёт аллокации (первые N по хронологии, где
    /// N = wb_reported_orders). false — заказ найден в БД, но выходит за
    /// пределы N; показываем для информации, расход = 0.
    #[serde(default)]
    pub is_allocated: bool,
    /// Доля basis заказа в сумме basis выбранных N заказов (0..1).
    /// Для неаллоцированных = 0.
    pub allocation_ratio: f64,
    /// Распределённая сумма расхода (RUB) на этот заказ. Сумма по группе
    /// nm_id равна `wb_advert_sum` (с округлением до копейки через
    /// last-residual). Для неаллоцированных = 0.
    #[serde(default)]
    pub allocated_cost: f64,
}

/// Группа найденных заказов для одной позиции (nm_id) рекламного отчёта.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WbAdvertLinkedOrdersByNm {
    pub nm_id: i64,
    pub nm_name: String,
    /// Сколько заказов привязал WB по своей метрике (line.metrics.orders).
    pub wb_reported_orders: i64,
    /// Расход WB на эту позицию (line.metrics.sum), который распределяется
    /// между `found_orders`.
    #[serde(default)]
    pub wb_advert_sum: f64,
    pub found_orders: Vec<WbAdvertFoundOrder>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WbAdvertDaily {
    #[serde(flatten)]
    pub base: BaseAggregate<WbAdvertDailyId>,
    pub header: WbAdvertDailyHeader,
    pub totals: WbAdvertDailyMetrics,
    pub unattributed_totals: WbAdvertDailyMetrics,
    pub lines: Vec<WbAdvertDailyLine>,
    pub source_meta: WbAdvertDailySourceMeta,
    pub is_posted: bool,
    /// true, если найден хотя бы один связанный заказ при последнем проведении.
    #[serde(default)]
    pub has_linked_orders: bool,
    /// Суммарное количество найденных заказов по всем nm_id.
    #[serde(default)]
    pub linked_orders_count: i64,
    /// Сгруппированный список найденных заказов по позициям рекламного отчёта.
    #[serde(default)]
    pub linked_orders: Vec<WbAdvertLinkedOrdersByNm>,
}

impl WbAdvertDaily {
    pub fn new_for_insert(
        header: WbAdvertDailyHeader,
        totals: WbAdvertDailyMetrics,
        unattributed_totals: WbAdvertDailyMetrics,
        lines: Vec<WbAdvertDailyLine>,
        source_meta: WbAdvertDailySourceMeta,
    ) -> Self {
        let description = format!(
            "Статистика рекламы WB advert_id={} за {}",
            header.advert_id, header.document_date
        );
        let base = BaseAggregate::new(
            WbAdvertDailyId::stable_for_header(&header),
            header.document_no.clone(),
            description,
        );

        Self {
            base,
            header,
            totals,
            unattributed_totals,
            lines,
            source_meta,
            is_posted: false,
            has_linked_orders: false,
            linked_orders_count: 0,
            linked_orders: Vec::new(),
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
        if self.header.advert_id <= 0 {
            return Err("advert_id должен быть положительным".into());
        }
        Ok(())
    }

    pub fn before_write(&mut self) {
        self.touch_updated();
    }
}

impl AggregateRoot for WbAdvertDaily {
    type Id = WbAdvertDailyId;

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
        "a026"
    }

    fn collection_name() -> &'static str {
        "wb_advert_daily"
    }

    fn element_name() -> &'static str {
        "Статистика рекламы WB"
    }

    fn list_name() -> &'static str {
        "Статистика рекламы WB"
    }

    fn origin() -> Origin {
        Origin::Marketplace
    }
}
