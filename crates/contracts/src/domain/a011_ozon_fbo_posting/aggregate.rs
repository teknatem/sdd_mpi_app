use crate::domain::common::{AggregateId, AggregateRoot, BaseAggregate, EntityMetadata, Origin};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// ID типа для документа OZON FBO Posting
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct OzonFboPostingId(pub Uuid);

impl OzonFboPostingId {
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

impl AggregateId for OzonFboPostingId {
    fn as_string(&self) -> String {
        self.0.to_string()
    }
    fn from_string(s: &str) -> Result<Self, String> {
        Uuid::parse_str(s)
            .map(OzonFboPostingId::new)
            .map_err(|e| format!("Invalid UUID: {}", e))
    }
}

/// Заголовочные поля документа
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OzonFboPostingHeader {
    /// Номер документа (posting_number из OZON API)
    pub document_no: String,
    /// Схема (FBO)
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
pub struct OzonFboPostingLine {
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
pub struct OzonFboPostingState {
    /// Исходный статус из API
    pub status_raw: String,
    /// Нормализованный статус (DELIVERED и т.д.)
    pub status_norm: String,
    /// Подстатус из API (детальный статус)
    pub substatus_raw: Option<String>,
    /// Дата/время создания заказа в источнике
    pub created_at: Option<DateTime<Utc>>,
    /// Дата/время доставки (момент продажи)
    pub delivered_at: Option<DateTime<Utc>>,
    /// Дата/время обновления в источнике
    pub updated_at_source: Option<DateTime<Utc>>,
}

/// Служебные метаданные
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OzonFboPostingSourceMeta {
    /// Ссылка на сырой JSON (ID в document_raw_storage)
    pub raw_payload_ref: String,
    /// Дата/время получения из API
    pub fetched_at: DateTime<Utc>,
    /// Версия документа (для отслеживания изменений)
    pub document_version: i32,
}

/// Документ OZON FBO Posting (агрегат)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OzonFboPosting {
    #[serde(flatten)]
    pub base: BaseAggregate<OzonFboPostingId>,

    /// Заголовок документа
    pub header: OzonFboPostingHeader,

    /// Строки документа
    pub lines: Vec<OzonFboPostingLine>,

    /// Статусы и временные метки
    pub state: OzonFboPostingState,

    /// Служебные метаданные
    pub source_meta: OzonFboPostingSourceMeta,

    /// Флаг проведения документа (для формирования проекций)
    pub is_posted: bool,
}

impl OzonFboPosting {
    pub fn new_for_insert(
        code: String,
        description: String,
        header: OzonFboPostingHeader,
        lines: Vec<OzonFboPostingLine>,
        state: OzonFboPostingState,
        source_meta: OzonFboPostingSourceMeta,
        is_posted: bool,
    ) -> Self {
        let base = BaseAggregate::new(OzonFboPostingId::new_v4(), code, description);
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
        id: OzonFboPostingId,
        code: String,
        description: String,
        header: OzonFboPostingHeader,
        lines: Vec<OzonFboPostingLine>,
        state: OzonFboPostingState,
        source_meta: OzonFboPostingSourceMeta,
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

impl AggregateRoot for OzonFboPosting {
    type Id = OzonFboPostingId;

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
        "a011"
    }

    fn collection_name() -> &'static str {
        "ozon_fbo_posting"
    }

    fn element_name() -> &'static str {
        "Документ OZON FBO"
    }

    fn list_name() -> &'static str {
        "Документы OZON FBO"
    }

    fn origin() -> Origin {
        Origin::Marketplace
    }
}
