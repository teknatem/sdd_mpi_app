use crate::domain::common::{AggregateId, AggregateRoot, BaseAggregate, EntityMetadata, Origin};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// ID типа для документа Wildberries Orders
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct WbOrdersId(pub Uuid);

impl WbOrdersId {
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

impl AggregateId for WbOrdersId {
    fn as_string(&self) -> String {
        self.0.to_string()
    }
    fn from_string(s: &str) -> Result<Self, String> {
        Uuid::parse_str(s)
            .map(WbOrdersId::new)
            .map_err(|e| format!("Invalid UUID: {}", e))
    }
}

/// Заголовочные поля документа
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WbOrdersHeader {
    /// Номер документа (srid из WB API - уникальный ID строки заказа)
    pub document_no: String,
    /// ID подключения маркетплейса
    pub connection_id: String,
    /// ID организации
    pub organization_id: String,
    /// ID маркетплейса
    pub marketplace_id: String,
}

/// Строка документа (в WB один заказ = одна строка)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WbOrdersLine {
    /// ID строки (совпадает с srid в WB)
    pub line_id: String,
    /// Артикул продавца
    pub supplier_article: String,
    /// nmId (ID номенклатуры WB)
    pub nm_id: i64,
    /// Баркод
    pub barcode: String,
    /// Категория
    pub category: Option<String>,
    /// Предмет
    pub subject: Option<String>,
    /// Бренд
    pub brand: Option<String>,
    /// Размер
    pub tech_size: Option<String>,
    /// Количество (всегда 1 для заказов)
    pub qty: f64,
    /// Цена без скидки
    pub total_price: Option<f64>,
    /// Процент скидки
    pub discount_percent: Option<f64>,
    /// SPP (Согласованная скидка продавца)
    pub spp: Option<f64>,
    /// Итоговая цена для клиента
    pub finished_price: Option<f64>,
    /// Цена с учетом скидки
    pub price_with_disc: Option<f64>,
    /// Цена продавца (Statistics totalPrice или Marketplace API `price` в рублях)
    #[serde(default)]
    pub price: Option<f64>,
    /// Цена продажи из Marketplace API (`salePrice`, рубли). Хранится готовой на
    /// строке, чтобы расчёт margin_pro брал base_price одним запросом к a015 и не
    /// читал raw-payload из document_raw_storage на каждом проведении.
    #[serde(default)]
    pub sale_price: Option<f64>,
    /// Дилерская цена УТ
    pub dealer_price_ut: Option<f64>,
    /// Маржинальность, %
    pub margin_pro: Option<f64>,
    /// Код валюты продажи заказа (ISO 4217 numeric) из Marketplace API `currencyCode`.
    /// Заполняется только для трансграничных заказов СНГ (не-рубль: 398 KZT, 933 BYN,
    /// 51 AMD …). Отсутствие поля ⇒ заказ в рублях. Суммы `price`/`sale_price` уже
    /// пересчитаны в рубли, это поле сохраняет исходную валюту для прозрачности.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub currency_code: Option<i64>,
    /// Курс WB (рублей за единицу `currency_code`), применённый к WB-суммам при импорте.
    /// Вычисляется как `convertedPrice / price`. Заполняется вместе с `currency_code`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fx_rate: Option<f64>,
}

impl WbOrdersLine {
    /// База распределения рекламных расходов: price_with_disc → finished_price → price → total_price.
    pub fn allocation_basis(&self) -> f64 {
        [
            self.price_with_disc,
            self.finished_price,
            self.price,
            self.total_price,
        ]
        .into_iter()
        .flatten()
        .find(|v| *v > f64::EPSILON)
        .unwrap_or(0.0)
    }
}

/// Статусы и временные метки
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WbOrdersState {
    /// Дата/время заказа
    pub order_dt: DateTime<Utc>,
    /// Дата/время последнего изменения
    pub last_change_dt: Option<DateTime<Utc>>,
    /// Флаг отмены заказа
    pub is_cancel: bool,
    /// Дата отмены (если есть)
    pub cancel_dt: Option<DateTime<Utc>>,
    /// Флаг поставки
    pub is_supply: Option<bool>,
    /// Флаг реализации
    pub is_realization: Option<bool>,
}

/// Информация о складе
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WbOrdersWarehouse {
    /// Название склада
    pub warehouse_name: Option<String>,
    /// Тип склада
    pub warehouse_type: Option<String>,
}

/// Информация о географии
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WbOrdersGeography {
    /// Название страны
    pub country_name: Option<String>,
    /// Название области/округа
    pub oblast_okrug_name: Option<String>,
    /// Название региона
    pub region_name: Option<String>,
}

/// Служебные метаданные
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WbOrdersSourceMeta {
    /// Номер поставки
    pub income_id: Option<i64>,
    /// ID стикера
    pub sticker: Option<String>,
    /// G-номер
    pub g_number: Option<String>,
    /// Ссылка на сырой JSON (ID в document_raw_storage)
    pub raw_payload_ref: String,
    /// Ссылка на сырой JSON Marketplace API /api/v3/orders, если заказ приходил оттуда.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub marketplace_raw_payload_ref: Option<String>,
    /// Дата/время получения из API
    pub fetched_at: DateTime<Utc>,
    /// Версия документа (для отслеживания изменений)
    pub document_version: i32,
}

/// Документ Wildberries Orders (агрегат)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WbOrders {
    #[serde(flatten)]
    pub base: BaseAggregate<WbOrdersId>,

    /// Заголовок документа
    pub header: WbOrdersHeader,

    /// Строка документа (в WB всегда одна строка)
    pub line: WbOrdersLine,

    /// Статусы и временные метки
    pub state: WbOrdersState,

    /// Информация о складе
    pub warehouse: WbOrdersWarehouse,

    /// Информация о географии
    pub geography: WbOrdersGeography,

    /// Служебные метаданные
    pub source_meta: WbOrdersSourceMeta,

    /// Флаг проведения документа (для формирования проекций)
    pub is_posted: bool,

    /// Ссылка на товар маркетплейса (a007_marketplace_product)
    pub marketplace_product_ref: Option<String>,

    /// Ссылка на номенклатуру 1С (a004_nomenclature)
    pub nomenclature_ref: Option<String>,

    /// Ссылка на базовую номенклатуру
    pub base_nomenclature_ref: Option<String>,

    /// Дата документа из API (основная дата заказа для фильтрации)
    pub document_date: Option<String>,
}

impl WbOrders {
    #[allow(clippy::too_many_arguments)]
    pub fn new_for_insert(
        code: String,
        description: String,
        header: WbOrdersHeader,
        line: WbOrdersLine,
        state: WbOrdersState,
        warehouse: WbOrdersWarehouse,
        geography: WbOrdersGeography,
        source_meta: WbOrdersSourceMeta,
        is_posted: bool,
        document_date: Option<String>,
    ) -> Self {
        let base = BaseAggregate::new(WbOrdersId::new_v4(), code, description);
        Self {
            base,
            header,
            line,
            state,
            warehouse,
            geography,
            source_meta,
            is_posted,
            marketplace_product_ref: None,
            nomenclature_ref: None,
            base_nomenclature_ref: None,
            document_date,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new_with_id(
        id: WbOrdersId,
        code: String,
        description: String,
        header: WbOrdersHeader,
        line: WbOrdersLine,
        state: WbOrdersState,
        warehouse: WbOrdersWarehouse,
        geography: WbOrdersGeography,
        source_meta: WbOrdersSourceMeta,
        is_posted: bool,
        document_date: Option<String>,
    ) -> Self {
        let base = BaseAggregate::new(id, code, description);
        Self {
            base,
            header,
            line,
            state,
            warehouse,
            geography,
            source_meta,
            is_posted,
            marketplace_product_ref: None,
            nomenclature_ref: None,
            base_nomenclature_ref: None,
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
        if self.header.document_no.trim().is_empty() {
            return Err("Номер документа (srid) обязателен".into());
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

impl AggregateRoot for WbOrders {
    type Id = WbOrdersId;

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
        "a015"
    }

    fn collection_name() -> &'static str {
        "wb_orders"
    }

    fn element_name() -> &'static str {
        "Документ WB Заказы"
    }

    fn list_name() -> &'static str {
        "Документы WB Заказы"
    }

    fn origin() -> Origin {
        Origin::Marketplace
    }
}
