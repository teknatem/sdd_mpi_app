use crate::domain::common::{AggregateId, AggregateRoot, BaseAggregate, EntityMetadata, Origin};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// ID типа для документа Yandex Market Return
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct YmReturnId(pub Uuid);

impl YmReturnId {
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

impl AggregateId for YmReturnId {
    fn as_string(&self) -> String {
        self.0.to_string()
    }
    fn from_string(s: &str) -> Result<Self, String> {
        Uuid::parse_str(s)
            .map(YmReturnId::new)
            .map_err(|e| format!("Invalid UUID: {}", e))
    }
}

/// Заголовочные поля документа возврата
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YmReturnHeader {
    /// ID возврата из YM API (returnId)
    pub return_id: i64,
    /// ID исходного заказа (orderId)
    pub order_id: i64,
    /// ID подключения маркетплейса
    pub connection_id: String,
    /// ID организации
    pub organization_id: String,
    /// ID маркетплейса
    pub marketplace_id: String,
    /// ID кампании в Yandex Market
    pub campaign_id: String,
    /// Тип операции: RETURN (возврат) или UNREDEEMED (невыкуп)
    pub return_type: String,
    /// Общая сумма возврата (amount)
    #[serde(default)]
    pub amount: Option<f64>,
    /// Валюта
    #[serde(default)]
    pub currency: Option<String>,
}

/// Решение по возврату товара
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YmReturnDecision {
    /// Тип решения: REFUND_MONEY, DECLINE_REFUND, и т.д.
    pub decision_type: String,
    /// Сумма возврата за данный товар
    #[serde(default)]
    pub amount: Option<f64>,
    /// Валюта
    #[serde(default)]
    pub currency: Option<String>,
    /// Компенсация за обратную доставку от Маркета
    #[serde(default)]
    pub partner_compensation_amount: Option<f64>,
    /// Комментарий к решению
    #[serde(default)]
    pub comment: Option<String>,
}

/// Строка документа возврата (товар)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YmReturnLine {
    /// ID товара в возврате
    pub item_id: i64,
    /// shopSku (артикул продавца)
    pub shop_sku: String,
    /// offerId (идентификатор товара)
    pub offer_id: String,
    /// Название товара
    pub name: String,
    /// Количество возвращаемых единиц
    pub count: i32,
    /// Цена товара
    #[serde(default)]
    pub price: Option<f64>,
    /// Причина возврата
    #[serde(default)]
    pub return_reason: Option<String>,
    /// Решения по возврату (может быть несколько)
    #[serde(default)]
    pub decisions: Vec<YmReturnDecision>,
    /// Фотографии дефектов (URLs)
    #[serde(default)]
    pub photos: Vec<String>,
}

/// Статусы и временные метки
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YmReturnState {
    /// Статус возврата денег: REFUNDED, REFUND_IN_PROGRESS, NOT_REFUNDED
    pub refund_status: String,
    /// Дата создания возврата
    #[serde(default)]
    pub created_at_source: Option<DateTime<Utc>>,
    /// Дата обновления возврата
    #[serde(default)]
    pub updated_at_source: Option<DateTime<Utc>>,
    /// Дата фактического возврата денег
    #[serde(default)]
    pub refund_date: Option<DateTime<Utc>>,
}

/// Служебные метаданные
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YmReturnSourceMeta {
    /// Ссылка на сырой JSON (ID в document_raw_storage)
    pub raw_payload_ref: String,
    /// Дата/время получения из API
    pub fetched_at: DateTime<Utc>,
    /// Версия документа (для отслеживания изменений)
    pub document_version: i32,
}

/// Документ Yandex Market Return (агрегат)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YmReturn {
    #[serde(flatten)]
    pub base: BaseAggregate<YmReturnId>,

    /// Заголовок документа
    pub header: YmReturnHeader,

    /// Строки документа (товары возврата)
    pub lines: Vec<YmReturnLine>,

    /// Статусы и временные метки
    pub state: YmReturnState,

    /// Служебные метаданные
    pub source_meta: YmReturnSourceMeta,

    /// Флаг проведения документа (для формирования проекций)
    pub is_posted: bool,
}

impl YmReturn {
    pub fn new_for_insert(
        code: String,
        description: String,
        header: YmReturnHeader,
        lines: Vec<YmReturnLine>,
        state: YmReturnState,
        source_meta: YmReturnSourceMeta,
        is_posted: bool,
    ) -> Self {
        let base = BaseAggregate::new(YmReturnId::new_v4(), code, description);
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
        id: YmReturnId,
        code: String,
        description: String,
        header: YmReturnHeader,
        lines: Vec<YmReturnLine>,
        state: YmReturnState,
        source_meta: YmReturnSourceMeta,
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

    /// Вычислить общую сумму возврата по всем решениям
    pub fn calculate_total_refund(&self) -> f64 {
        self.lines
            .iter()
            .flat_map(|line| line.decisions.iter())
            .filter(|d| d.decision_type == "REFUND_MONEY")
            .filter_map(|d| d.amount)
            .sum()
    }

    /// Вычислить общую компенсацию за доставку
    pub fn calculate_total_compensation(&self) -> f64 {
        self.lines
            .iter()
            .flat_map(|line| line.decisions.iter())
            .filter_map(|d| d.partner_compensation_amount)
            .sum()
    }

    /// Получить количество товаров в возврате
    pub fn total_items_count(&self) -> i32 {
        self.lines.iter().map(|l| l.count).sum()
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.base.description.trim().is_empty() {
            return Err("Описание не может быть пустым".into());
        }
        if self.base.code.trim().is_empty() {
            return Err("Код не может быть пустым".into());
        }
        if self.header.return_id == 0 {
            return Err("ID возврата обязателен".into());
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

impl AggregateRoot for YmReturn {
    type Id = YmReturnId;

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
        "a016"
    }

    fn collection_name() -> &'static str {
        "ym_returns"
    }

    fn element_name() -> &'static str {
        "Возврат Yandex Market"
    }

    fn list_name() -> &'static str {
        "Возвраты Yandex Market"
    }

    fn origin() -> Origin {
        Origin::Marketplace
    }
}

// =============================================================================
// List DTO for frontend (flat structure for list views)
// =============================================================================

/// DTO для списка возвратов (минимальные поля для list view)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YmReturnListItemDto {
    pub id: String,
    pub return_id: i64,
    pub order_id: i64,
    /// UUID подключения МП (a006_connection_mp)
    pub connection_id: String,
    /// Дата исходного заказа (a013_ym_order.creation_date по order_id); пусто если заказ не найден
    #[serde(default)]
    pub order_date: String,
    pub return_type: String,
    pub refund_status: String,
    pub total_items: i32,
    pub total_amount: f64,
    pub created_at_source: String,
    pub fetched_at: String,
    pub is_posted: bool,
}
