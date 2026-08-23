use crate::domain::common::{
    AggregateId, AggregateRoot, BaseAggregate, EntityMetadata, Origin,
};
use crate::shared::metadata::{EntityMetadataInfo, FieldMetadata};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ============================================================================
// ID Type
// ============================================================================

/// Уникальный идентификатор подключения к базе 1С
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Connection1CDatabaseId(pub Uuid);

impl Connection1CDatabaseId {
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

impl AggregateId for Connection1CDatabaseId {
    fn as_string(&self) -> String {
        self.0.to_string()
    }

    fn from_string(s: &str) -> Result<Self, String> {
        Uuid::parse_str(s)
            .map(Connection1CDatabaseId::new)
            .map_err(|e| format!("Invalid UUID: {}", e))
    }
}

// ============================================================================
// Aggregate Root
// ============================================================================

/// Подключение к базе данных 1С:Enterprise
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Connection1CDatabase {
    #[serde(flatten)]
    pub base: BaseAggregate<Connection1CDatabaseId>,

    // Специфичные поля агрегата
    pub url: String,
    pub login: String,
    pub password: String,

    #[serde(rename = "isPrimary", default)]
    pub is_primary: bool,
}

impl Connection1CDatabase {
    /// Создать новое подключение для вставки в БД
    pub fn new_for_insert(
        code: String,
        description: String,
        url: String,
        comment: Option<String>,
        login: String,
        password: String,
        is_primary: bool,
    ) -> Self {
        let mut base = BaseAggregate::new(Connection1CDatabaseId::new_v4(), code, description);
        base.comment = comment;

        Self {
            base,
            url,
            login,
            password,
            is_primary,
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
    pub fn update(&mut self, dto: &Connection1CDatabaseDto) {
        self.base.code = dto.code.clone().unwrap_or_default();
        self.base.description = dto.description.clone();
        self.base.comment = dto.comment.clone();
        self.url = dto.url.clone();
        self.login = dto.login.clone();
        self.password = dto.password.clone();
        self.is_primary = dto.is_primary;
    }

    /// Валидация данных
    pub fn validate(&self) -> Result<(), String> {
        if self.url.trim().is_empty() {
            return Err("URL не может быть пустым".into());
        }
        if !self.url.starts_with("http://") && !self.url.starts_with("https://") {
            return Err("URL должен начинаться с http:// или https://".into());
        }
        if self.base.description.trim().is_empty() {
            return Err("Описание не может быть пустым".into());
        }
        if self.base.code.trim().is_empty() {
            return Err("Код не может быть пустым".into());
        }
        Ok(())
    }

    /// Хук перед записью
    pub fn before_write(&mut self) {
        self.touch_updated();
    }
}

impl AggregateRoot for Connection1CDatabase {
    type Id = Connection1CDatabaseId;

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
        "a001"
    }

    fn collection_name() -> &'static str {
        "connection_1c"
    }

    fn element_name() -> &'static str {
        "Подключение 1С"
    }

    fn list_name() -> &'static str {
        "Подключения 1С"
    }

    fn origin() -> Origin {
        Origin::Self_
    }

    fn entity_metadata_info() -> Option<&'static EntityMetadataInfo> {
        Some(&super::ENTITY_METADATA)
    }

    fn field_metadata() -> Option<&'static [FieldMetadata]> {
        Some(super::FIELDS)
    }
}

// ============================================================================
// Forms / DTOs
// ============================================================================

/// DTO для создания/обновления подключения к 1С
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Connection1CDatabaseDto {
    pub id: Option<String>,
    pub code: Option<String>,
    pub description: String,
    pub url: String,
    pub comment: Option<String>,
    pub login: String,
    pub password: String,

    #[serde(rename = "isPrimary", default)]
    pub is_primary: bool,
}

/// Результат тестирования подключения к 1С
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionTestResult {
    pub success: bool,
    pub message: String,
    pub duration_ms: u64,
    pub tested_at: chrono::DateTime<chrono::Utc>,
}
