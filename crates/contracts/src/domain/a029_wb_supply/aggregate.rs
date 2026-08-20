use crate::domain::common::{
    AggregateId, AggregateRoot, BaseAggregate, EntityMetadata, EventStore, Origin,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// ID типа для документа WB Supply (поставка)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct WbSupplyId(pub Uuid);

impl WbSupplyId {
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

impl AggregateId for WbSupplyId {
    fn as_string(&self) -> String {
        self.0.to_string()
    }
    fn from_string(s: &str) -> Result<Self, String> {
        Uuid::parse_str(s)
            .map(WbSupplyId::new)
            .map_err(|e| format!("Invalid UUID: {}", e))
    }
}

/// Заголовочные поля документа поставки
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WbSupplyHeader {
    /// ID поставки из WB API (например "WB-GI-12345678")
    pub supply_id: String,
    /// ID подключения маркетплейса
    pub connection_id: String,
    /// ID организации
    pub organization_id: String,
    /// ID маркетплейса
    pub marketplace_id: String,
}

/// Информация о поставке из WB API
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WbSupplyInfo {
    /// Название поставки
    pub name: Option<String>,
    /// Флаг B2B-поставки
    pub is_b2b: bool,
    /// Поставка завершена
    pub is_done: bool,
    /// Дата/время создания поставки в WB
    pub created_at_wb: Option<DateTime<Utc>>,
    /// Дата/время закрытия поставки
    pub closed_at_wb: Option<DateTime<Utc>>,
    /// Дата/время сканирования
    pub scan_dt: Option<DateTime<Utc>>,
    /// Тип упаковки (0=виртуальная, 1=короб, 2=монопаллета, 5=суперсейф)
    pub cargo_type: Option<i32>,
    /// Тип кросс-бордер
    pub cross_border_type: Option<i32>,
    /// ID офиса назначения
    pub destination_office_id: Option<i64>,
}

/// Строка в поставке (заказ внутри поставки)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WbSupplyOrderRow {
    /// ID заказа WB
    pub order_id: i64,
    /// uid заказа
    pub order_uid: Option<String>,
    /// Артикул продавца
    pub article: Option<String>,
    /// nmId
    pub nm_id: Option<i64>,
    /// chrtId
    pub chrt_id: Option<i64>,
    /// Баркоды
    pub barcodes: Vec<String>,
    /// Цена
    pub price: Option<i64>,
    /// Дата создания
    pub created_at: Option<String>,
    /// ID склада
    pub warehouse_id: Option<i64>,
    /// Номер стикера (partA)
    pub part_a: Option<i64>,
    /// Номер стикера (partB)
    pub part_b: Option<i64>,
    /// UUID номенклатуры 1С (a004_nomenclature)
    pub nomenclature_ref: Option<String>,
    /// UUID базовой номенклатуры 1С (a004_nomenclature)
    pub base_nomenclature_ref: Option<String>,
    /// Код цвета
    pub color_code: Option<String>,
    /// Статус
    pub status: Option<String>,
}

/// Служебные метаданные источника
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WbSupplySourceMeta {
    /// Ссылка на сырой JSON (ID в document_raw_storage)
    pub raw_payload_ref: String,
    /// Дата/время получения из API
    pub fetched_at: DateTime<Utc>,
    /// Версия документа
    pub document_version: i32,
}

/// Документ WB Supply (поставка) — агрегат
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WbSupply {
    #[serde(flatten)]
    pub base: BaseAggregate<WbSupplyId>,

    /// Заголовок документа
    pub header: WbSupplyHeader,

    /// Информация о поставке
    pub info: WbSupplyInfo,

    /// Служебные метаданные
    pub source_meta: WbSupplySourceMeta,

    /// Флаг проведения документа
    pub is_posted: bool,

    /// Заказы внутри поставки (JSON-массив WbSupplyOrderRow)
    pub supply_orders: Vec<WbSupplyOrderRow>,

    /// Дата документа (дата создания поставки, для фильтрации)
    pub document_date: Option<String>,
}

impl WbSupply {
    #[allow(clippy::too_many_arguments)]
    pub fn new_for_insert(
        code: String,
        description: String,
        header: WbSupplyHeader,
        info: WbSupplyInfo,
        source_meta: WbSupplySourceMeta,
        is_posted: bool,
        supply_orders: Vec<WbSupplyOrderRow>,
        document_date: Option<String>,
    ) -> Self {
        let base = BaseAggregate::new(WbSupplyId::new_v4(), code, description);
        Self {
            base,
            header,
            info,
            source_meta,
            is_posted,
            supply_orders,
            document_date,
        }
    }

    pub fn to_string_id(&self) -> String {
        self.base.id.as_string()
    }

    pub fn touch_updated(&mut self) {
        self.base.touch();
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.base.description.trim().is_empty() {
            return Err("Описание не может быть пустым".into());
        }
        if self.base.code.trim().is_empty() {
            return Err("Код не может быть пустым".into());
        }
        if self.header.supply_id.trim().is_empty() {
            return Err("ID поставки обязателен".into());
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

impl AggregateRoot for WbSupply {
    type Id = WbSupplyId;

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
        "a029"
    }

    fn collection_name() -> &'static str {
        "wb_supply"
    }

    fn element_name() -> &'static str {
        "Поставка WB"
    }

    fn list_name() -> &'static str {
        "Поставки WB"
    }

    fn origin() -> Origin {
        Origin::Marketplace
    }
}
