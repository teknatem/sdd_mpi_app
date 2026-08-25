use crate::domain::common::{AggregateId, AggregateRoot, BaseAggregate, EntityMetadata, Origin};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// ID типа для документа OZON Transactions
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct OzonTransactionsId(pub Uuid);

impl OzonTransactionsId {
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

impl AggregateId for OzonTransactionsId {
    fn as_string(&self) -> String {
        self.0.to_string()
    }
    fn from_string(s: &str) -> Result<Self, String> {
        Uuid::parse_str(s)
            .map(OzonTransactionsId::new)
            .map_err(|e| format!("Invalid UUID: {}", e))
    }
}

/// Заголовочные поля транзакции
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OzonTransactionsHeader {
    /// ID операции из OZON API (operation_id) - natural key
    pub operation_id: i64,
    /// Тип операции (operation_type)
    pub operation_type: String,
    /// Дата операции
    pub operation_date: String,
    /// Название типа операции
    pub operation_type_name: String,
    /// Стоимость доставки
    pub delivery_charge: f64,
    /// Стоимость обратной доставки
    pub return_delivery_charge: f64,
    /// Начисления за продажу
    pub accruals_for_sale: f64,
    /// Комиссия за продажу
    pub sale_commission: f64,
    /// Итоговая сумма
    pub amount: f64,
    /// Тип транзакции (orders, services, etc.)
    pub transaction_type: String,
    /// ID подключения маркетплейса
    pub connection_id: String,
    /// ID организации
    pub organization_id: String,
    /// ID маркетплейса
    pub marketplace_id: String,
}

/// Информация о постинге (posting)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OzonTransactionsPosting {
    /// Схема доставки (FBS, FBO)
    #[serde(default)]
    pub delivery_schema: String,
    /// Дата заказа
    pub order_date: String,
    /// Номер постинга (posting_number)
    pub posting_number: String,
    /// ID склада
    pub warehouse_id: i64,
}

/// Элемент товара из массива items
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OzonTransactionsItem {
    /// Название товара
    pub name: String,
    /// SKU товара
    pub sku: i64,
    /// Цена товара (из постинга FBO/FBS)
    #[serde(default)]
    pub price: Option<f64>,
    /// Пропорция стоимости (для распределения сумм)
    #[serde(default)]
    pub ratio: Option<f64>,
    /// Ссылка на продукт маркетплейса (a003)
    #[serde(default)]
    pub marketplace_product_ref: Option<String>,
    /// Ссылка на номенклатуру (a004)
    #[serde(default)]
    pub nomenclature_ref: Option<String>,
}

/// Элемент сервиса из массива services
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OzonTransactionsService {
    /// Название сервиса
    pub name: String,
    /// Цена сервиса
    pub price: f64,
}

/// Служебные метаданные
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OzonTransactionsSourceMeta {
    /// Ссылка на сырой JSON (ID в document_raw_storage)
    pub raw_payload_ref: String,
    /// Дата/время получения из API
    pub fetched_at: DateTime<Utc>,
    /// Версия документа (для отслеживания изменений)
    pub document_version: i32,
}

/// Документ OZON Transactions (агрегат)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OzonTransactions {
    #[serde(flatten)]
    pub base: BaseAggregate<OzonTransactionsId>,

    /// Заголовок транзакции
    pub header: OzonTransactionsHeader,

    /// Информация о постинге
    pub posting: OzonTransactionsPosting,

    /// Товары (items)
    pub items: Vec<OzonTransactionsItem>,

    /// Сервисы (services)
    pub services: Vec<OzonTransactionsService>,

    /// Служебные метаданные
    pub source_meta: OzonTransactionsSourceMeta,

    /// Флаг проведения документа (для будущего постинга)
    pub is_posted: bool,

    /// Ссылка на документ постинга (A010 или A011)
    pub posting_ref: Option<String>,

    /// Тип документа постинга ("A010" или "A011")
    pub posting_ref_type: Option<String>,
}

impl OzonTransactions {
    #[allow(clippy::too_many_arguments)]
    pub fn new_for_insert(
        code: String,
        description: String,
        header: OzonTransactionsHeader,
        posting: OzonTransactionsPosting,
        items: Vec<OzonTransactionsItem>,
        services: Vec<OzonTransactionsService>,
        source_meta: OzonTransactionsSourceMeta,
        is_posted: bool,
    ) -> Self {
        let base = BaseAggregate::new(OzonTransactionsId::new_v4(), code, description);
        Self {
            base,
            header,
            posting,
            items,
            services,
            source_meta,
            is_posted,
            posting_ref: None,
            posting_ref_type: None,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new_with_id(
        id: OzonTransactionsId,
        code: String,
        description: String,
        header: OzonTransactionsHeader,
        posting: OzonTransactionsPosting,
        items: Vec<OzonTransactionsItem>,
        services: Vec<OzonTransactionsService>,
        source_meta: OzonTransactionsSourceMeta,
        is_posted: bool,
    ) -> Self {
        let base = BaseAggregate::new(id, code, description);
        Self {
            base,
            header,
            posting,
            items,
            services,
            source_meta,
            is_posted,
            posting_ref: None,
            posting_ref_type: None,
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
        if self.header.operation_id == 0 {
            return Err("ID операции обязателен".into());
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

impl AggregateRoot for OzonTransactions {
    type Id = OzonTransactionsId;

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
        "a014"
    }

    fn collection_name() -> &'static str {
        "ozon_transactions"
    }

    fn element_name() -> &'static str {
        "Транзакция OZON"
    }

    fn list_name() -> &'static str {
        "Транзакции OZON"
    }

    fn origin() -> Origin {
        Origin::Marketplace
    }
}

/// DTO для списка (плоская структура для таблицы)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OzonTransactionsListDto {
    pub id: String,
    pub operation_id: i64,
    pub operation_type: String,
    pub operation_type_name: String,
    pub operation_date: String,
    pub posting_number: String,
    pub transaction_type: String,
    pub delivery_schema: String,
    pub amount: f64,
    pub accruals_for_sale: f64,
    pub sale_commission: f64,
    pub delivery_charge: f64,
    pub substatus: Option<String>,
    pub delivering_date: Option<String>,
    pub is_posted: bool,
    pub posting_ref: Option<String>,
    pub posting_ref_type: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// DTO для деталей (полная структура)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OzonTransactionsDetailDto {
    pub id: String,
    pub code: String,
    pub description: String,
    pub header: OzonTransactionsHeader,
    pub posting: OzonTransactionsPosting,
    pub items: Vec<OzonTransactionsItem>,
    pub services: Vec<OzonTransactionsService>,
    pub source_meta: OzonTransactionsSourceMeta,
    pub is_posted: bool,
    pub posting_ref: Option<String>,
    pub posting_ref_type: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub is_deleted: bool,
    pub version: i32,
}

/// DTO для создания/обновления
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OzonTransactionsDto {
    pub code: String,
    pub description: String,
    pub header: OzonTransactionsHeader,
    pub posting: OzonTransactionsPosting,
    pub items: Vec<OzonTransactionsItem>,
    pub services: Vec<OzonTransactionsService>,
    pub source_meta: OzonTransactionsSourceMeta,
    pub is_posted: bool,
    pub posting_ref: Option<String>,
    pub posting_ref_type: Option<String>,
}
