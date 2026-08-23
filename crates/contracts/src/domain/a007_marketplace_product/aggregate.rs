use crate::domain::common::{
    AggregateId, AggregateRoot, BaseAggregate, EntityMetadata, Origin,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ============================================================================
// ID Type
// ============================================================================

/// Уникальный идентификатор товара маркетплейса
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MarketplaceProductId(pub Uuid);

impl MarketplaceProductId {
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

impl AggregateId for MarketplaceProductId {
    fn as_string(&self) -> String {
        self.0.to_string()
    }

    fn from_string(s: &str) -> Result<Self, String> {
        Uuid::parse_str(s)
            .map(MarketplaceProductId::new)
            .map_err(|e| format!("Invalid UUID: {}", e))
    }
}

// ============================================================================
// Aggregate Root
// ============================================================================

/// Товар маркетплейса (консолидация номенклатуры)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketplaceProduct {
    #[serde(flatten)]
    pub base: BaseAggregate<MarketplaceProductId>,

    /// ID маркетплейса (ссылка на a005_marketplace)
    #[serde(rename = "marketplaceRef")]
    pub marketplace_ref: String,

    /// ID подключения к маркетплейсу (ссылка на a006_connection_mp)
    #[serde(rename = "connectionMpRef")]
    pub connection_mp_ref: String,

    /// Внутренний ID товара в маркетплейсе
    #[serde(rename = "marketplaceSku")]
    pub marketplace_sku: String,

    /// Штрихкод товара
    pub barcode: Option<String>,

    /// Артикул на маркетплейсе
    pub article: String,

    /// Бренд товара
    pub brand: Option<String>,

    /// ID категории товара на маркетплейсе
    #[serde(rename = "categoryId")]
    pub category_id: Option<String>,

    /// Текстовое название категории
    #[serde(rename = "categoryName")]
    pub category_name: Option<String>,

    /// Дата последнего обновления информации с маркетплейса
    #[serde(rename = "lastUpdate")]
    pub last_update: Option<chrono::DateTime<chrono::Utc>>,

    /// Уникальный ID товара в собственной базе (ссылка на a004_nomenclature)
    #[serde(rename = "nomenclatureRef")]
    pub nomenclature_ref: Option<String>,
}

impl MarketplaceProduct {
    /// Создать новый товар маркетплейса для вставки в БД
    #[allow(clippy::too_many_arguments)]
    pub fn new_for_insert(
        code: String,
        description: String,
        marketplace_ref: String,
        connection_mp_ref: String,
        marketplace_sku: String,
        barcode: Option<String>,
        article: String,
        brand: Option<String>,
        category_id: Option<String>,
        category_name: Option<String>,
        last_update: Option<chrono::DateTime<chrono::Utc>>,
        nomenclature_ref: Option<String>,
        comment: Option<String>,
    ) -> Self {
        let mut base = BaseAggregate::new(MarketplaceProductId::new_v4(), code, description);
        base.comment = comment;

        Self {
            base,
            marketplace_ref,
            connection_mp_ref,
            marketplace_sku,
            barcode,
            article,
            brand,
            category_id,
            category_name,
            last_update,
            nomenclature_ref,
        }
    }

    /// Создать товар с заданным UUID
    #[allow(clippy::too_many_arguments)]
    pub fn new_with_id(
        id: MarketplaceProductId,
        code: String,
        description: String,
        marketplace_ref: String,
        connection_mp_ref: String,
        marketplace_sku: String,
        barcode: Option<String>,
        article: String,
        brand: Option<String>,
        category_id: Option<String>,
        category_name: Option<String>,
        last_update: Option<chrono::DateTime<chrono::Utc>>,
        nomenclature_ref: Option<String>,
        comment: Option<String>,
    ) -> Self {
        let mut base = BaseAggregate::new(id, code, description);
        base.comment = comment;

        Self {
            base,
            marketplace_ref,
            connection_mp_ref,
            marketplace_sku,
            barcode,
            article,
            brand,
            category_id,
            category_name,
            last_update,
            nomenclature_ref,
        }
    }

    /// Получить ID как строку
    pub fn to_string_id(&self) -> String {
        self.base.id.as_string()
    }

    /// Обновить timestamp
    pub fn touch_updated(&mut self) {
        self.base.touch();
    }

    /// Обновить данные из DTO
    pub fn update(&mut self, dto: &MarketplaceProductDto) {
        self.base.code = dto.code.clone().unwrap_or_default();
        self.base.description = dto.description.clone();
        self.base.comment = dto.comment.clone();
        self.marketplace_ref = dto.marketplace_ref.clone();
        self.connection_mp_ref = dto.connection_mp_ref.clone();
        self.marketplace_sku = dto.marketplace_sku.clone();
        self.barcode = dto.barcode.clone();
        self.article = dto.article.clone();
        self.brand = dto.brand.clone();
        self.category_id = dto.category_id.clone();
        self.category_name = dto.category_name.clone();
        self.last_update = dto.last_update;
        self.nomenclature_ref = dto.nomenclature_ref.clone();
    }

    /// Валидация данных
    pub fn validate(&self) -> Result<(), String> {
        if self.base.description.trim().is_empty() {
            return Err("Описание не может быть пустым".into());
        }
        if self.base.code.trim().is_empty() {
            return Err("Код не может быть пустым".into());
        }
        if self.marketplace_ref.trim().is_empty() {
            return Err("ID маркетплейса не может быть пустым".into());
        }
        if self.marketplace_sku.trim().is_empty() {
            return Err("SKU маркетплейса не может быть пустым".into());
        }
        if self.article.trim().is_empty() {
            return Err("Артикул не может быть пустым".into());
        }

        Ok(())
    }

    /// Хук перед записью
    pub fn before_write(&mut self) {
        self.touch_updated();
    }
}

impl AggregateRoot for MarketplaceProduct {
    type Id = MarketplaceProductId;

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
        "a007"
    }

    fn collection_name() -> &'static str {
        "marketplace_product"
    }

    fn element_name() -> &'static str {
        "Товар маркетплейса"
    }

    fn list_name() -> &'static str {
        "Товары маркетплейсов"
    }

    fn origin() -> Origin {
        Origin::Marketplace
    }
}

// ============================================================================
// Forms / DTOs
// ============================================================================

/// DTO для создания/обновления товара маркетплейса
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MarketplaceProductDto {
    pub id: Option<String>,
    pub code: Option<String>,
    pub description: String,
    #[serde(rename = "marketplaceRef")]
    pub marketplace_ref: String,
    #[serde(rename = "connectionMpRef")]
    pub connection_mp_ref: String,
    #[serde(rename = "marketplaceSku")]
    pub marketplace_sku: String,
    pub barcode: Option<String>,
    pub article: String,
    pub brand: Option<String>,
    #[serde(rename = "categoryId")]
    pub category_id: Option<String>,
    #[serde(rename = "categoryName")]
    pub category_name: Option<String>,
    #[serde(rename = "lastUpdate")]
    pub last_update: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(rename = "nomenclatureRef")]
    pub nomenclature_ref: Option<String>,
    pub comment: Option<String>,
}

// =============================================================================
// List DTO for frontend (flat structure for list views)
// =============================================================================

/// DTO для списка товаров маркетплейса (минимальные поля для list view)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketplaceProductListItemDto {
    pub id: String,
    pub code: String,
    pub description: String,
    pub marketplace_ref: String,
    pub connection_mp_ref: String,
    pub marketplace_sku: String,
    pub barcode: Option<String>,
    pub article: String,
    pub nomenclature_ref: Option<String>,
    pub is_posted: bool,
    pub created_at: String,
}
