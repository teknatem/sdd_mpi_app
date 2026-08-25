use crate::domain::common::{AggregateId, AggregateRoot, BaseAggregate, EntityMetadata, Origin};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// ID типа для документа Приобретение товаров и услуг
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PurchaseOfGoodsId(pub Uuid);

impl PurchaseOfGoodsId {
    pub fn new(value: Uuid) -> Self {
        Self(value)
    }
    pub fn value(&self) -> Uuid {
        self.0
    }
}

impl AggregateId for PurchaseOfGoodsId {
    fn as_string(&self) -> String {
        self.0.to_string()
    }
    fn from_string(s: &str) -> Result<Self, String> {
        Uuid::parse_str(s)
            .map(PurchaseOfGoodsId::new)
            .map_err(|e| format!("Invalid UUID: {}", e))
    }
}

/// Строка табличной части «Товары» документа ПриобретениеТоваровУслуг
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PurchaseOfGoodsLine {
    /// UUID номенклатуры из 1С (Номенклатура_Key)
    pub nomenclature_key: String,

    /// Количество
    pub quantity: f64,

    /// Цена
    pub price: f64,

    /// Сумма с НДС (СуммаСНДС)
    pub amount_with_vat: f64,

    /// Сумма НДС (СуммаНДС)
    pub vat_amount: f64,
}

/// Документ Приобретение товаров и услуг (агрегат a023)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PurchaseOfGoods {
    #[serde(flatten)]
    pub base: BaseAggregate<PurchaseOfGoodsId>,

    /// Номер документа (напр. "ПОСТ-000001")
    pub document_no: String,

    /// Дата документа (YYYY-MM-DD)
    pub document_date: String,

    /// UUID контрагента из 1С (ссылка на a003_counterparty)
    pub counterparty_key: String,

    /// JSON-массив строк табличной части Товары
    pub lines_json: Option<String>,

    /// ID подключения к 1С (a001_connection_1c)
    pub connection_id: String,

    /// Дата и время загрузки/обновления документа
    pub fetched_at: DateTime<Utc>,
}

impl PurchaseOfGoods {
    pub fn new_from_odata(
        id: Uuid,
        document_no: String,
        document_date: String,
        counterparty_key: String,
        lines: Vec<PurchaseOfGoodsLine>,
        connection_id: String,
    ) -> Self {
        let lines_json = if lines.is_empty() {
            None
        } else {
            serde_json::to_string(&lines).ok()
        };

        let description = format!("{} от {}", document_no, document_date);
        let base = BaseAggregate::new(PurchaseOfGoodsId::new(id), document_no.clone(), description);

        Self {
            base,
            document_no,
            document_date,
            counterparty_key,
            lines_json,
            connection_id,
            fetched_at: Utc::now(),
        }
    }

    pub fn to_string_id(&self) -> String {
        self.base.id.as_string()
    }

    /// Десериализовать lines_json в вектор строк
    pub fn parse_lines(&self) -> Vec<PurchaseOfGoodsLine> {
        self.lines_json
            .as_deref()
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or_default()
    }
}

impl AggregateRoot for PurchaseOfGoods {
    type Id = PurchaseOfGoodsId;

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
        "a023"
    }

    fn collection_name() -> &'static str {
        "purchase_of_goods"
    }

    fn element_name() -> &'static str {
        "Приобретение товаров"
    }

    fn list_name() -> &'static str {
        "Приобретение товаров"
    }

    fn origin() -> Origin {
        Origin::C1
    }
}
