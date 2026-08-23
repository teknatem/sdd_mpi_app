use crate::domain::common::{
    AggregateId, AggregateRoot, BaseAggregate, EntityMetadata, Origin,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ============================================================================
// ID Type
// ============================================================================

/// Уникальный идентификатор организации
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct OrganizationId(pub Uuid);

impl OrganizationId {
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

impl AggregateId for OrganizationId {
    fn as_string(&self) -> String {
        self.0.to_string()
    }

    fn from_string(s: &str) -> Result<Self, String> {
        Uuid::parse_str(s)
            .map(OrganizationId::new)
            .map_err(|e| format!("Invalid UUID: {}", e))
    }
}

// ============================================================================
// Aggregate Root
// ============================================================================

/// Организация (юридическое лицо или ИП)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Organization {
    #[serde(flatten)]
    pub base: BaseAggregate<OrganizationId>,

    // Специфичные поля агрегата
    #[serde(rename = "fullName")]
    pub full_name: String,

    pub inn: String,
    pub kpp: String,

    /// Субъект учёта GL (код из `GL_ENTITY_CLASSES`, kind=own: san/sts/upr).
    /// Локальное поле — НЕ перезаписывается импортом из 1С.
    #[serde(rename = "entityRef", default, skip_serializing_if = "Option::is_none")]
    pub entity_ref: Option<String>,
}

impl Organization {
    /// Создать новую организацию для вставки в БД
    pub fn new_for_insert(
        code: String,
        description: String,
        full_name: String,
        inn: String,
        kpp: String,
        comment: Option<String>,
    ) -> Self {
        let mut base = BaseAggregate::new(OrganizationId::new_v4(), code, description);
        base.comment = comment;

        Self {
            base,
            full_name,
            inn,
            kpp,
            entity_ref: None,
        }
    }

    /// Создать организацию с заданным UUID (для импорта из 1С)
    pub fn new_with_id(
        id: OrganizationId,
        code: String,
        description: String,
        full_name: String,
        inn: String,
        kpp: String,
        comment: Option<String>,
    ) -> Self {
        let mut base = BaseAggregate::new(id, code, description);
        base.comment = comment;

        Self {
            base,
            full_name,
            inn,
            kpp,
            entity_ref: None,
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
    pub fn update(&mut self, dto: &OrganizationDto) {
        self.base.code = dto.code.clone().unwrap_or_default();
        self.base.description = dto.description.clone();
        self.base.comment = dto.comment.clone();
        self.full_name = dto.full_name.clone();
        self.inn = dto.inn.clone();
        self.kpp = dto.kpp.clone();
        self.entity_ref = dto
            .entity_ref
            .clone()
            .filter(|value| !value.trim().is_empty());
    }

    /// Валидация данных
    pub fn validate(&self) -> Result<(), String> {
        if self.base.description.trim().is_empty() {
            return Err("Описание не может быть пустым".into());
        }
        if self.base.code.trim().is_empty() {
            return Err("Код не может быть пустым".into());
        }
        if self.full_name.trim().is_empty() {
            return Err("Полное наименование не может быть пустым".into());
        }

        // Валидация ИНН (разрешаем пустой для импорта из внешних систем)
        if !self.inn.trim().is_empty() {
            let inn_digits: String = self.inn.chars().filter(|c| c.is_ascii_digit()).collect();
            if inn_digits.len() != 10 && inn_digits.len() != 12 {
                return Err("ИНН должен содержать 10 цифр (для ЮЛ) или 12 цифр (для ИП)".into());
            }
        }

        // Валидация КПП (разрешаем пустой)
        if !self.kpp.trim().is_empty() {
            let kpp_digits: String = self.kpp.chars().filter(|c| c.is_ascii_digit()).collect();
            if kpp_digits.len() != 9 {
                return Err("КПП должен содержать 9 цифр или быть пустым (для ИП)".into());
            }
        }

        Ok(())
    }

    /// Хук перед записью
    pub fn before_write(&mut self) {
        self.touch_updated();
    }
}

impl AggregateRoot for Organization {
    type Id = OrganizationId;

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
        "a002"
    }

    fn collection_name() -> &'static str {
        "organization"
    }

    fn element_name() -> &'static str {
        "Организация"
    }

    fn list_name() -> &'static str {
        "Организации"
    }

    fn origin() -> Origin {
        Origin::Self_
    }
}

// ============================================================================
// Forms / DTOs
// ============================================================================

/// DTO для создания/обновления организации
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OrganizationDto {
    pub id: Option<String>,
    pub code: Option<String>,
    pub description: String,

    #[serde(rename = "fullName")]
    pub full_name: String,

    pub inn: String,
    pub kpp: String,
    pub comment: Option<String>,

    /// Субъект учёта GL (код kind=own: san/sts/upr). Локальное поле.
    #[serde(rename = "entityRef", default)]
    pub entity_ref: Option<String>,
}
