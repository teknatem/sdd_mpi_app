use crate::domain::common::{
    AggregateId, AggregateRoot, BaseAggregate, EntityMetadata, Origin,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// ID типа для документа OZON FBS Posting
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct OzonFbsPostingId(pub Uuid);

impl OzonFbsPostingId {
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

impl AggregateId for OzonFbsPostingId {
    fn as_string(&self) -> String {
        self.0.to_string()
    }
    fn from_string(s: &str) -> Result<Self, String> {
        Uuid::parse_str(s)
            .map(OzonFbsPostingId::new)
            .map_err(|e| format!("Invalid UUID: {}", e))
    }
}

/// Заголовочные поля документа
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OzonFbsPostingHeader {
    /// Номер документа (posting_number из OZON API)
    pub document_no: String,
    /// Схема (FBS)
    pub scheme: String,
    /// ID подключения маркетплейса
    pub connection_id: String,
    /// ID организации
    pub organization_id: String,
    /// ID маркетплейса
    pub marketplace_id: String,
}

/// Строка документа (позиция)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OzonFbsPostingLine {
    /// ID строки (детерминированный ключ)
    pub line_id: String,
    /// ID товара в OZON (product_id)
    pub product_id: String,
    /// Код продавца (offer_id)
    pub offer_id: String,
    /// Наименование товара
    pub name: String,
    /// Баркод
    pub barcode: Option<String>,
    /// Количество
    pub qty: f64,
    /// Цена до скидок
    pub price_list: Option<f64>,
    /// Сумма скидок
    pub discount_total: Option<f64>,
    /// Цена после скидок (за единицу)
    pub price_effective: Option<f64>,
    /// Сумма за строку
    pub amount_line: Option<f64>,
    /// Код валюты
    pub currency_code: Option<String>,
}

/// Статусы и временные метки
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OzonFbsPostingState {
    /// Исходный статус из API
    pub status_raw: String,
    /// Нормализованный статус (DELIVERED и т.д.)
    pub status_norm: String,
    /// Подстатус из API (детальный статус)
    pub substatus_raw: Option<String>,
    /// Дата/время доставки (момент продажи)
    pub delivered_at: Option<DateTime<Utc>>,
    /// Дата/время обновления в источнике
    pub updated_at_source: Option<DateTime<Utc>>,
}

/// Служебные метаданные
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OzonFbsPostingSourceMeta {
    /// Ссылка на сырой JSON (ID в document_raw_storage)
    pub raw_payload_ref: String,
    /// Дата/время получения из API
    pub fetched_at: DateTime<Utc>,
    /// Версия документа (для отслеживания изменений)
    pub document_version: i32,
}

/// Документ OZON FBS Posting (агрегат)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OzonFbsPosting {
    #[serde(flatten)]
    pub base: BaseAggregate<OzonFbsPostingId>,

    /// Заголовок документа
    pub header: OzonFbsPostingHeader,

    /// Строки документа
    pub lines: Vec<OzonFbsPostingLine>,

    /// Статусы и временные метки
    pub state: OzonFbsPostingState,

    /// Служебные метаданные
    pub source_meta: OzonFbsPostingSourceMeta,

    /// Флаг проведения документа (для формирования проекций)
    pub is_posted: bool,
}

impl OzonFbsPosting {
    pub fn new_for_insert(
        code: String,
        description: String,
        header: OzonFbsPostingHeader,
        lines: Vec<OzonFbsPostingLine>,
        state: OzonFbsPostingState,
        source_meta: OzonFbsPostingSourceMeta,
        is_posted: bool,
    ) -> Self {
        let base = BaseAggregate::new(OzonFbsPostingId::new_v4(), code, description);
        Self {
            base,
            header,
            lines,
            state,
            source_meta,
            is_posted,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new_with_id(
        id: OzonFbsPostingId,
        code: String,
        description: String,
        header: OzonFbsPostingHeader,
        lines: Vec<OzonFbsPostingLine>,
        state: OzonFbsPostingState,
        source_meta: OzonFbsPostingSourceMeta,
        is_posted: bool,
    ) -> Self {
        let base = BaseAggregate::new(id, code, description);
        Self {
            base,
            header,
            lines,
            state,
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
        if self.base.description.trim().is_empty() {
            return Err("Описание не может быть пустым".into());
        }
        if self.base.code.trim().is_empty() {
            return Err("Код не может быть пустым".into());
        }
        if self.header.document_no.trim().is_empty() {
            return Err("Номер документа обязателен".into());
        }
        if self.header.connection_id.trim().is_empty() {
            return Err("Подключение обязательно".into());
        }
        if self.lines.is_empty() {
            return Err("Документ должен содержать хотя бы одну строку".into());
        }
        Ok(())
    }

    pub fn before_write(&mut self) {
        self.touch_updated();
    }
}

impl AggregateRoot for OzonFbsPosting {
    type Id = OzonFbsPostingId;

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
        "a010"
    }

    fn collection_name() -> &'static str {
        "ozon_fbs_posting"
    }

    fn element_name() -> &'static str {
        "Документ OZON FBS"
    }

    fn list_name() -> &'static str {
        "Документы OZON FBS"
    }

    fn origin() -> Origin {
        Origin::Marketplace
    }
}
