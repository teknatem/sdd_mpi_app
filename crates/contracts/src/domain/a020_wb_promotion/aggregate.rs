use crate::domain::common::{AggregateId, AggregateRoot, BaseAggregate, EntityMetadata, Origin};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// ID типа для документа WB Promotion
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct WbPromotionId(pub Uuid);

impl WbPromotionId {
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

impl AggregateId for WbPromotionId {
    fn as_string(&self) -> String {
        self.0.to_string()
    }
    fn from_string(s: &str) -> Result<Self, String> {
        Uuid::parse_str(s)
            .map(WbPromotionId::new)
            .map_err(|e| format!("Invalid UUID: {}", e))
    }
}

/// Заголовочные поля документа
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WbPromotionHeader {
    /// Номер документа (PROMO-{promotionID})
    pub document_no: String,
    /// ID подключения маркетплейса
    pub connection_id: String,
    /// ID организации
    pub organization_id: String,
    /// ID маркетплейса
    pub marketplace_id: String,
}

/// Рейтинговое условие акции (из /details)
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WbPromotionRanging {
    pub condition: Option<String>,
    pub participation_rate: Option<f64>,
    pub boost: Option<f64>,
}

/// Данные акции из WB Calendar API
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WbPromotionData {
    /// ID акции в WB (promotionID) — ключ дедупликации
    pub promotion_id: i64,
    /// Название акции
    pub name: String,
    /// Описание акции
    pub description: Option<String>,
    /// Преимущества участия (из /details)
    #[serde(default)]
    pub advantages: Vec<String>,
    /// Дата начала (ISO 8601)
    pub start_date_time: String,
    /// Дата окончания (ISO 8601)
    pub end_date_time: String,
    /// Тип акции
    pub promotion_type: Option<String>,
    /// Количество товаров-исключений
    pub exception_products_count: Option<i32>,
    /// Всего товаров уже в акции
    pub in_promo_action_total: Option<i32>,
    /// Остатки товаров в акции
    pub in_promo_action_leftovers: Option<i32>,
    /// Товаров, не участвующих в акции (остатки)
    pub not_in_promo_action_leftovers: Option<i32>,
    /// Всего товаров, не участвующих в акции
    pub not_in_promo_action_total: Option<i32>,
    /// Процент участия
    pub participation_percentage: Option<f64>,
    /// Условия рейтингового буста
    #[serde(default)]
    pub ranging: Vec<WbPromotionRanging>,
}

/// Позиция (товар) участвующий в акции
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WbPromotionNomenclature {
    /// nmId товара WB
    pub nm_id: i64,
}

/// Служебные метаданные источника
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WbPromotionSourceMeta {
    /// Ссылка на сырой JSON (ID в document_raw_storage)
    pub raw_payload_ref: String,
    /// Дата/время получения из API
    pub fetched_at: String,
}

/// Документ WB Акция (агрегат)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WbPromotion {
    #[serde(flatten)]
    pub base: BaseAggregate<WbPromotionId>,

    /// Заголовок документа
    pub header: WbPromotionHeader,

    /// Данные акции
    pub data: WbPromotionData,

    /// Товары-участники акции
    pub nomenclatures: Vec<WbPromotionNomenclature>,

    /// Служебные метаданные
    pub source_meta: WbPromotionSourceMeta,

    /// Флаг проведения
    pub is_posted: bool,
}

impl WbPromotion {
    pub fn new_for_insert(
        header: WbPromotionHeader,
        data: WbPromotionData,
        nomenclatures: Vec<WbPromotionNomenclature>,
        source_meta: WbPromotionSourceMeta,
    ) -> Self {
        let document_no = header.document_no.clone();
        let name = data.name.clone();
        let base = BaseAggregate::new(WbPromotionId::new_v4(), document_no, name);
        Self {
            base,
            header,
            data,
            nomenclatures,
            source_meta,
            is_posted: false,
        }
    }

    pub fn new_with_id(
        id: WbPromotionId,
        header: WbPromotionHeader,
        data: WbPromotionData,
        nomenclatures: Vec<WbPromotionNomenclature>,
        source_meta: WbPromotionSourceMeta,
        is_posted: bool,
    ) -> Self {
        let document_no = header.document_no.clone();
        let name = data.name.clone();
        let base = BaseAggregate::new(id, document_no, name);
        Self {
            base,
            header,
            data,
            nomenclatures,
            source_meta,
            is_posted,
        }
    }

    pub fn to_string_id(&self) -> String {
        self.base.id.as_string()
    }

    pub fn touch_updated(&mut self) {
        self.base.touch();
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.header.document_no.trim().is_empty() {
            return Err("Номер документа обязателен".into());
        }
        if self.header.connection_id.trim().is_empty() {
            return Err("Подключение обязательно".into());
        }
        if self.data.name.trim().is_empty() {
            return Err("Название акции обязательно".into());
        }
        Ok(())
    }

    pub fn before_write(&mut self) {
        self.touch_updated();
    }
}

impl AggregateRoot for WbPromotion {
    type Id = WbPromotionId;

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
        "a020"
    }

    fn collection_name() -> &'static str {
        "wb_promotion"
    }

    fn element_name() -> &'static str {
        "Акция WB"
    }

    fn list_name() -> &'static str {
        "Акции WB (Календарь)"
    }

    fn origin() -> Origin {
        Origin::Marketplace
    }
}
