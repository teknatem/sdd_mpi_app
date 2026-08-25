use crate::domain::common::{AggregateId, AggregateRoot, BaseAggregate, EntityMetadata, Origin};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ============================================================================
// ID Type
// ============================================================================
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CounterpartyId(pub Uuid);

impl CounterpartyId {
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

impl AggregateId for CounterpartyId {
    fn as_string(&self) -> String {
        self.0.to_string()
    }

    fn from_string(s: &str) -> Result<Self, String> {
        Uuid::parse_str(s)
            .map(CounterpartyId::new)
            .map_err(|e| format!("Invalid UUID: {}", e))
    }
}

// ============================================================================
// Aggregate Root
// ============================================================================
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Counterparty {
    #[serde(flatten)]
    pub base: BaseAggregate<CounterpartyId>,

    #[serde(rename = "isFolder", default)]
    pub is_folder: bool,

    #[serde(rename = "parentId")]
    pub parent_id: Option<String>,

    // Tax identifiers
    #[serde(default)]
    pub inn: String,
    #[serde(default)]
    pub kpp: String,
}

impl Counterparty {
    pub fn new_for_insert(
        code: String,
        description: String,
        is_folder: bool,
        parent_id: Option<String>,
        inn: String,
        kpp: String,
        comment: Option<String>,
    ) -> Self {
        let mut base = BaseAggregate::new(CounterpartyId::new_v4(), code, description);
        base.comment = comment;

        Self {
            base,
            is_folder,
            parent_id,
            inn,
            kpp,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new_with_id(
        id: CounterpartyId,
        code: String,
        description: String,
        is_folder: bool,
        parent_id: Option<String>,
        inn: String,
        kpp: String,
        comment: Option<String>,
    ) -> Self {
        let mut base = BaseAggregate::new(id, code, description);
        base.comment = comment;

        Self {
            base,
            is_folder,
            parent_id,
            inn,
            kpp,
        }
    }

    pub fn to_string_id(&self) -> String {
        self.base.id.as_string()
    }

    pub fn touch_updated(&mut self) {
        self.base.touch();
    }

    pub fn update(&mut self, dto: &CounterpartyDto) {
        self.base.code = dto.code.clone().unwrap_or_default();
        self.base.description = dto.description.clone();
        self.base.comment = dto.comment.clone();
        self.is_folder = dto.is_folder;
        self.parent_id = dto.parent_id.clone();
        self.inn = dto.inn.clone().unwrap_or_default();
        self.kpp = dto.kpp.clone().unwrap_or_default();
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.base.description.trim().is_empty() {
            return Err("Описание не может быть пустым".into());
        }
        if self.base.code.trim().is_empty() {
            return Err("Код не может быть пустым".into());
        }
        Ok(())
    }

    pub fn before_write(&mut self) {
        self.touch_updated();
    }
}

impl AggregateRoot for Counterparty {
    type Id = CounterpartyId;

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
        "a003"
    }

    fn collection_name() -> &'static str {
        "counterparty"
    }

    fn element_name() -> &'static str {
        "Контрагент"
    }

    fn list_name() -> &'static str {
        "Контрагенты"
    }

    fn origin() -> Origin {
        Origin::C1
    }
}

// ============================================================================
// DTO
// ============================================================================
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CounterpartyDto {
    pub id: Option<String>,
    pub code: Option<String>,
    pub description: String,
    #[serde(rename = "isFolder", default)]
    pub is_folder: bool,
    #[serde(rename = "parentId")]
    pub parent_id: Option<String>,
    pub comment: Option<String>,
    pub inn: Option<String>,
    pub kpp: Option<String>,
    #[serde(rename = "updatedAt")]
    pub updated_at: Option<chrono::DateTime<chrono::Utc>>,
}
