use crate::domain::common::{
    AggregateId, AggregateRoot, BaseAggregate, EntityMetadata, Origin,
};
use crate::shared::data_view::FilterRef;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// ID типа для агрегата BI Dashboard
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BiDashboardId(pub Uuid);

impl BiDashboardId {
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

impl AggregateId for BiDashboardId {
    fn as_string(&self) -> String {
        self.0.to_string()
    }
    fn from_string(s: &str) -> Result<Self, String> {
        Uuid::parse_str(s)
            .map(BiDashboardId::new)
            .map_err(|e| format!("Invalid UUID: {}", e))
    }
}

// ============================================================================
// DashboardLayout — дерево категорий с индикаторами
// ============================================================================

/// Элемент дашборда — ссылка на конкретный индикатор (a024) с переопределениями
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashboardItem {
    /// UUID-ссылка на BiIndicator (a024)
    pub indicator_id: String,
    /// Кэшированное отображаемое имя индикатора (code — description).
    /// Хранится для отображения без дополнительных запросов.
    #[serde(default)]
    pub indicator_name: String,
    /// Порядок сортировки внутри группы
    pub sort_order: i32,
    /// Класс колонки для grid-layout (например "1x1", "2x1", "1x2", "2x2")
    pub col_class: String,
    /// Переопределение значений параметров для этого конкретного экземпляра
    #[serde(default)]
    pub param_overrides: HashMap<String, String>,
}

impl DashboardItem {
    pub fn new(indicator_id: String) -> Self {
        Self {
            indicator_id,
            indicator_name: String::new(),
            sort_order: 0,
            col_class: "1x1".to_string(),
            param_overrides: HashMap::new(),
        }
    }
}

/// Группа (категория) в дереве дашборда — может содержать индикаторы и вложенные группы
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashboardGroup {
    /// Локальный UUID для идентификации группы в дереве
    pub id: String,
    /// Название категории
    pub title: String,
    /// Порядок сортировки среди групп одного уровня
    pub sort_order: i32,
    /// Индикаторы в этой группе
    pub items: Vec<DashboardItem>,
    /// Вложенные подгруппы (рекурсивно)
    pub subgroups: Vec<DashboardGroup>,
}

impl DashboardGroup {
    pub fn new(title: String) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            title,
            sort_order: 0,
            items: vec![],
            subgroups: vec![],
        }
    }
}

/// Раскладка дашборда — корневой список групп
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashboardLayout {
    pub groups: Vec<DashboardGroup>,
}

impl Default for DashboardLayout {
    fn default() -> Self {
        Self { groups: vec![] }
    }
}

// ============================================================================
// Status
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum BiDashboardStatus {
    Draft,
    Active,
    Archived,
}

impl BiDashboardStatus {
    pub fn from_str(s: &str) -> Result<Self, String> {
        match s {
            "draft" => Ok(BiDashboardStatus::Draft),
            "active" => Ok(BiDashboardStatus::Active),
            "archived" => Ok(BiDashboardStatus::Archived),
            _ => Err(format!("Unknown BI dashboard status: {}", s)),
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            BiDashboardStatus::Draft => "draft",
            BiDashboardStatus::Active => "active",
            BiDashboardStatus::Archived => "archived",
        }
    }
}

// ============================================================================
// Aggregate
// ============================================================================

/// Агрегат BI Dashboard
///
/// Один агрегат = один дашборд, содержащий набор BI-индикаторов (a024),
/// сгруппированных по категориям в дерево, с глобальными фильтрами
/// и оценкой пользователя 1-5 звёзд.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BiDashboard {
    #[serde(flatten)]
    pub base: BaseAggregate<BiDashboardId>,

    /// Раскладка — дерево групп с индикаторами
    pub layout: DashboardLayout,
    /// Ссылки на глобальный реестр фильтров для этого дашборда
    #[serde(default)]
    pub filters: Vec<FilterRef>,

    /// Статус: Draft | Active | Archived
    pub status: BiDashboardStatus,
    /// Владелец дашборда
    pub owner_user_id: String,
    /// Публичный ли (доступен другим пользователям)
    pub is_public: bool,
    /// Оценка пользователя (1-5 звёзд, None = не оценён)
    pub rating: Option<u8>,
    /// Кто создал
    pub created_by: Option<String>,
    /// Кто обновил
    pub updated_by: Option<String>,
}

impl BiDashboard {
    pub fn new_for_insert(code: String, description: String, owner_user_id: String) -> Self {
        let base = BaseAggregate::new(BiDashboardId::new_v4(), code, description);
        Self {
            base,
            layout: DashboardLayout::default(),
            filters: vec![],
            status: BiDashboardStatus::Draft,
            owner_user_id,
            is_public: false,
            rating: None,
            created_by: None,
            updated_by: None,
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
            return Err("Наименование не может быть пустым".into());
        }
        if self.base.code.trim().is_empty() {
            return Err("Код не может быть пустым".into());
        }
        if let Some(r) = self.rating {
            if r < 1 || r > 5 {
                return Err("Оценка должна быть от 1 до 5".into());
            }
        }
        Ok(())
    }

    pub fn before_write(&mut self) {
        self.touch_updated();
    }
}

impl AggregateRoot for BiDashboard {
    type Id = BiDashboardId;

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
        "a025"
    }

    fn collection_name() -> &'static str {
        "bi_dashboard"
    }

    fn element_name() -> &'static str {
        "BI Дашборд"
    }

    fn list_name() -> &'static str {
        "BI Дашборды"
    }

    fn origin() -> Origin {
        Origin::Self_
    }
}

// ============================================================================
// DTO для списка
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BiDashboardListItem {
    pub id: String,
    pub code: String,
    pub description: String,
    pub comment: Option<String>,
    pub status: String,
    pub owner_user_id: String,
    pub is_public: bool,
    pub rating: Option<u8>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<BiDashboard> for BiDashboardListItem {
    fn from(d: BiDashboard) -> Self {
        Self {
            id: d.base.id.as_string(),
            code: d.base.code,
            description: d.base.description,
            comment: d.base.comment,
            status: d.status.as_str().to_string(),
            owner_user_id: d.owner_user_id,
            is_public: d.is_public,
            rating: d.rating,
            created_at: d.base.metadata.created_at,
            updated_at: d.base.metadata.updated_at,
        }
    }
}
