use crate::domain::common::{AggregateId, AggregateRoot, BaseAggregate, EntityMetadata, Origin};
use crate::enums::marketplace_type::MarketplaceType;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ============================================================================
// ID Type
// ============================================================================

/// Уникальный идентификатор маркетплейса
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MarketplaceId(pub Uuid);

impl MarketplaceId {
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

impl AggregateId for MarketplaceId {
    fn as_string(&self) -> String {
        self.0.to_string()
    }

    fn from_string(s: &str) -> Result<Self, String> {
        Uuid::parse_str(s)
            .map(MarketplaceId::new)
            .map_err(|e| format!("Invalid UUID: {}", e))
    }
}

// ============================================================================
// Aggregate Root
// ============================================================================

/// Маркетплейс (торговая площадка)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Marketplace {
    #[serde(flatten)]
    pub base: BaseAggregate<MarketplaceId>,

    // Специфичные поля агрегата
    pub url: String,

    #[serde(rename = "logoPath")]
    pub logo_path: Option<String>,

    #[serde(rename = "marketplaceType")]
    pub marketplace_type: Option<MarketplaceType>,

    #[serde(rename = "acquiringFeePro")]
    pub acquiring_fee_pro: f64,
}

impl Marketplace {
    /// Создать новый маркетплейс для вставки в БД
    pub fn new_for_insert(
        code: String,
        description: String,
        url: String,
        logo_path: Option<String>,
        marketplace_type: Option<MarketplaceType>,
        comment: Option<String>,
        acquiring_fee_pro: f64,
    ) -> Self {
        let mut base = BaseAggregate::new(MarketplaceId::new_v4(), code, description);
        base.comment = comment;

        Self {
            base,
            url,
            logo_path,
            marketplace_type,
            acquiring_fee_pro,
        }
    }

    /// Создать маркетплейс с заданным UUID
    pub fn new_with_id(
        id: MarketplaceId,
        code: String,
        description: String,
        url: String,
        logo_path: Option<String>,
        marketplace_type: Option<MarketplaceType>,
        comment: Option<String>,
        acquiring_fee_pro: f64,
    ) -> Self {
        let mut base = BaseAggregate::new(id, code, description);
        base.comment = comment;

        Self {
            base,
            url,
            logo_path,
            marketplace_type,
            acquiring_fee_pro,
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
    pub fn update(&mut self, dto: &MarketplaceDto) {
        self.base.code = dto.code.clone().unwrap_or_default();
        self.base.description = dto.description.clone();
        self.base.comment = dto.comment.clone();
        self.url = dto.url.clone();
        self.logo_path = dto.logo_path.clone();
        self.marketplace_type = dto.marketplace_type;
        self.acquiring_fee_pro = dto.acquiring_fee_pro;
    }

    /// Валидация данных
    pub fn validate(&self) -> Result<(), String> {
        if self.base.description.trim().is_empty() {
            return Err("Описание не может быть пустым".into());
        }
        if self.base.code.trim().is_empty() {
            return Err("Код не может быть пустым".into());
        }
        if self.url.trim().is_empty() {
            return Err("URL не может быть пустым".into());
        }

        // Базовая валидация URL
        if !self.url.starts_with("http://") && !self.url.starts_with("https://") {
            return Err("URL должен начинаться с http:// или https://".into());
        }

        Ok(())
    }

    /// Хук перед записью
    pub fn before_write(&mut self) {
        self.touch_updated();
    }
}

impl AggregateRoot for Marketplace {
    type Id = MarketplaceId;

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
        "a005"
    }

    fn collection_name() -> &'static str {
        "marketplace"
    }

    fn element_name() -> &'static str {
        "Маркетплейс"
    }

    fn list_name() -> &'static str {
        "Маркетплейсы"
    }

    fn origin() -> Origin {
        Origin::Self_
    }
}

// ============================================================================
// Forms / DTOs
// ============================================================================

/// DTO для создания/обновления маркетплейса
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MarketplaceDto {
    pub id: Option<String>,
    pub code: Option<String>,
    pub description: String,
    pub url: String,
    #[serde(rename = "logoPath")]
    pub logo_path: Option<String>,
    #[serde(rename = "marketplaceType")]
    pub marketplace_type: Option<MarketplaceType>,
    pub comment: Option<String>,
    #[serde(rename = "acquiringFeePro")]
    pub acquiring_fee_pro: f64,
}
