use crate::domain::common::{AggregateId, AggregateRoot, BaseAggregate, EntityMetadata, Origin};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ============================================================================
// ID Type
// ============================================================================

/// Уникальный идентификатор подключения к маркетплейсу
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ConnectionMPId(pub Uuid);

impl ConnectionMPId {
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

impl AggregateId for ConnectionMPId {
    fn as_string(&self) -> String {
        self.0.to_string()
    }

    fn from_string(s: &str) -> Result<Self, String> {
        Uuid::parse_str(s)
            .map(ConnectionMPId::new)
            .map_err(|e| format!("Invalid UUID: {}", e))
    }
}

// ============================================================================
// Enums
// ============================================================================

/// Типы маркетплейсов
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum MarketplaceType {
    #[serde(rename = "Озон")]
    Ozon,
    #[serde(rename = "Wildberries")]
    Wildberries,
    #[serde(rename = "Яндекс.Маркет")]
    YandexMarket,
}

impl Default for MarketplaceType {
    fn default() -> Self {
        Self::Ozon
    }
}

impl MarketplaceType {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Ozon => "Озон",
            Self::Wildberries => "Wildberries",
            Self::YandexMarket => "Яндекс.Маркет",
        }
    }
}

/// Типы авторизации
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AuthorizationType {
    #[serde(rename = "API Key")]
    ApiKey,
    #[serde(rename = "OAuth 2.0")]
    OAuth2,
    #[serde(rename = "Basic Auth")]
    BasicAuth,
}

impl Default for AuthorizationType {
    fn default() -> Self {
        Self::ApiKey
    }
}

impl AuthorizationType {
    pub fn as_str(&self) -> &str {
        match self {
            Self::ApiKey => "API Key",
            Self::OAuth2 => "OAuth 2.0",
            Self::BasicAuth => "Basic Auth",
        }
    }
}

// ============================================================================
// Aggregate Root
// ============================================================================

/// Подключение к маркетплейсу
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionMP {
    #[serde(flatten)]
    pub base: BaseAggregate<ConnectionMPId>,

    // Основные связи
    #[serde(rename = "Маркетплейс")]
    pub marketplace_id: String, // UUID маркетплейса из справочника a005_marketplace

    #[serde(rename = "ОрганизацияRef")]
    pub organization_ref: String, // UUID организации

    // Дополнительные поля
    #[serde(rename = "API_Key")]
    pub api_key: String,

    #[serde(rename = "ID_Поставщика")]
    pub supplier_id: Option<String>,

    #[serde(rename = "ID_Приложения")]
    pub application_id: Option<String>,

    #[serde(rename = "Используется")]
    pub is_used: bool,

    #[serde(rename = "БизнесАккаунтID")]
    pub business_account_id: Option<String>,

    #[serde(rename = "API_Key_Статистика")]
    pub api_key_stats: Option<String>,

    #[serde(rename = "ТестовыйРежим")]
    pub test_mode: bool,

    #[serde(rename = "ПлановыйПроцентКомиссии")]
    pub planned_commission_percent: Option<f64>,

    #[serde(rename = "ПлановыйПроцентЭквайринга")]
    pub planned_acquiring_percent: Option<f64>,

    #[serde(rename = "ТипАвторизации")]
    pub authorization_type: AuthorizationType,
}

impl ConnectionMP {
    /// Создать новое подключение для вставки в БД
    pub fn new_for_insert(
        code: String,
        description: String,
        marketplace_id: String,
        organization_ref: String,
        api_key: String,
        comment: Option<String>,
    ) -> Self {
        let mut base = BaseAggregate::new(ConnectionMPId::new_v4(), code, description);
        base.comment = comment;

        Self {
            base,
            marketplace_id,
            organization_ref,
            api_key,
            supplier_id: None,
            application_id: None,
            is_used: false,
            business_account_id: None,
            api_key_stats: None,
            test_mode: false,
            planned_commission_percent: None,
            planned_acquiring_percent: None,
            authorization_type: AuthorizationType::default(),
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
    pub fn update(&mut self, dto: &ConnectionMPDto) {
        self.base.code = dto.code.clone().unwrap_or_default();
        self.base.description = dto.description.clone();
        self.base.comment = dto.comment.clone();
        self.marketplace_id = dto.marketplace_id.clone();
        self.organization_ref = dto.organization_ref.clone();
        self.api_key = dto.api_key.clone();
        self.supplier_id = dto.supplier_id.clone();
        self.application_id = dto.application_id.clone();
        self.is_used = dto.is_used;
        self.business_account_id = dto.business_account_id.clone();
        self.api_key_stats = dto.api_key_stats.clone();
        self.test_mode = dto.test_mode;
        self.planned_commission_percent = dto.planned_commission_percent;
        self.planned_acquiring_percent = dto.planned_acquiring_percent;
        self.authorization_type = dto.authorization_type.clone();
    }

    /// Валидация данных
    pub fn validate(&self) -> Result<(), String> {
        if self.base.description.trim().is_empty() {
            return Err("Наименование не может быть пустым".into());
        }
        if self.base.code.trim().is_empty() {
            return Err("Код не может быть пустым".into());
        }
        if self.api_key.trim().is_empty() {
            return Err("API Key не может быть пустым".into());
        }
        if self.organization_ref.trim().is_empty() {
            return Err("Организация должна быть указана".into());
        }
        Ok(())
    }

    /// Хук перед записью
    pub fn before_write(&mut self) {
        self.touch_updated();
    }
}

impl AggregateRoot for ConnectionMP {
    type Id = ConnectionMPId;

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
        "a006"
    }

    fn collection_name() -> &'static str {
        "connection_mp"
    }

    fn element_name() -> &'static str {
        "Подключение маркетплейса"
    }

    fn list_name() -> &'static str {
        "Подключения маркетплейсов"
    }

    fn origin() -> Origin {
        Origin::Self_
    }
}

// ============================================================================
// Forms / DTOs
// ============================================================================

/// DTO для создания/обновления подключения к маркетплейсу
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ConnectionMPDto {
    pub id: Option<String>,
    pub code: Option<String>,
    pub description: String,
    pub comment: Option<String>,

    #[serde(rename = "Маркетплейс")]
    pub marketplace_id: String,

    #[serde(rename = "ОрганизацияRef")]
    pub organization_ref: String,

    #[serde(rename = "API_Key")]
    pub api_key: String,

    #[serde(rename = "ID_Поставщика")]
    pub supplier_id: Option<String>,

    #[serde(rename = "ID_Приложения")]
    pub application_id: Option<String>,

    #[serde(rename = "Используется")]
    pub is_used: bool,

    #[serde(rename = "БизнесАккаунтID")]
    pub business_account_id: Option<String>,

    #[serde(rename = "API_Key_Статистика")]
    pub api_key_stats: Option<String>,

    #[serde(rename = "ТестовыйРежим")]
    pub test_mode: bool,

    #[serde(rename = "ПлановыйПроцентКомиссии")]
    pub planned_commission_percent: Option<f64>,

    #[serde(rename = "ПлановыйПроцентЭквайринга")]
    pub planned_acquiring_percent: Option<f64>,

    #[serde(rename = "ТипАвторизации")]
    pub authorization_type: AuthorizationType,
}

/// Результат тестирования подключения к маркетплейсу
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionTestResult {
    pub success: bool,
    pub message: String,
    pub duration_ms: u64,
    pub tested_at: chrono::DateTime<chrono::Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
}
