use crate::domain::a017_llm_agent::aggregate::LlmAgentId;
use crate::domain::a018_llm_chat::aggregate::LlmChatId;
use crate::domain::common::{AggregateId, AggregateRoot, BaseAggregate, EntityMetadata, Origin};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// ID типа для агрегата LLM Artifact
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct LlmArtifactId(pub Uuid);

impl LlmArtifactId {
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

impl AggregateId for LlmArtifactId {
    fn as_string(&self) -> String {
        self.0.to_string()
    }
    fn from_string(s: &str) -> Result<Self, String> {
        Uuid::parse_str(s)
            .map(LlmArtifactId::new)
            .map_err(|e| format!("Invalid UUID: {}", e))
    }
}

/// Тип артефакта
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ArtifactType {
    SqlQuery,
    /// Drilldown-отчёт, созданный LLM. session_id хранится в query_params JSON.
    DrilldownReport,
    /// Плагин, созданный/обновлённый агентом. plugin_id/code/title — в query_params JSON.
    Plugin,
}

impl ArtifactType {
    pub fn from_str(s: &str) -> Result<Self, String> {
        match s {
            "sql_query" => Ok(ArtifactType::SqlQuery),
            "drilldown_report" => Ok(ArtifactType::DrilldownReport),
            "plugin" => Ok(ArtifactType::Plugin),
            _ => Err(format!("Unknown artifact type: {}", s)),
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            ArtifactType::SqlQuery => "sql_query",
            ArtifactType::DrilldownReport => "drilldown_report",
            ArtifactType::Plugin => "plugin",
        }
    }
}

/// Статус артефакта
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ArtifactStatus {
    Draft,
    Active,
    Deprecated,
    Failed,
}

impl ArtifactStatus {
    pub fn from_str(s: &str) -> Result<Self, String> {
        match s {
            "draft" => Ok(ArtifactStatus::Draft),
            "active" => Ok(ArtifactStatus::Active),
            "deprecated" => Ok(ArtifactStatus::Deprecated),
            "failed" => Ok(ArtifactStatus::Failed),
            _ => Err(format!("Unknown artifact status: {}", s)),
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            ArtifactStatus::Draft => "draft",
            ArtifactStatus::Active => "active",
            ArtifactStatus::Deprecated => "deprecated",
            ArtifactStatus::Failed => "failed",
        }
    }
}

/// Агрегат LLM Artifact
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmArtifact {
    #[serde(flatten)]
    pub base: BaseAggregate<LlmArtifactId>,

    // Связи
    pub chat_id: LlmChatId,
    pub agent_id: LlmAgentId,

    // Метаданные
    pub artifact_type: ArtifactType,
    pub status: ArtifactStatus,

    // SQL контент
    pub sql_query: String,
    pub query_params: Option<String>,

    // UI конфигурация
    pub visualization_config: Option<String>,

    // Статистика выполнения
    pub last_executed_at: Option<DateTime<Utc>>,
    pub execution_count: i32,
}

impl LlmArtifact {
    /// Создать новый артефакт для вставки в БД
    pub fn new_for_insert(
        code: String,
        description: String,
        chat_id: LlmChatId,
        agent_id: LlmAgentId,
        sql_query: String,
    ) -> Self {
        let base = BaseAggregate::new(LlmArtifactId::new_v4(), code, description);
        Self {
            base,
            chat_id,
            agent_id,
            artifact_type: ArtifactType::SqlQuery,
            status: ArtifactStatus::Active,
            sql_query,
            query_params: None,
            visualization_config: None,
            last_executed_at: None,
            execution_count: 0,
        }
    }

    /// Создать артефакт с известным ID
    pub fn new_with_id(
        id: LlmArtifactId,
        code: String,
        description: String,
        chat_id: LlmChatId,
        agent_id: LlmAgentId,
        sql_query: String,
    ) -> Self {
        let base = BaseAggregate::new(id, code, description);
        Self {
            base,
            chat_id,
            agent_id,
            artifact_type: ArtifactType::SqlQuery,
            status: ArtifactStatus::Active,
            sql_query,
            query_params: None,
            visualization_config: None,
            last_executed_at: None,
            execution_count: 0,
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
        if self.artifact_type == ArtifactType::SqlQuery && self.sql_query.trim().is_empty() {
            return Err("SQL запрос не может быть пустым".into());
        }
        Ok(())
    }

    pub fn before_write(&mut self) {
        self.touch_updated();
    }
}

impl AggregateRoot for LlmArtifact {
    type Id = LlmArtifactId;

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
        "a019"
    }

    fn collection_name() -> &'static str {
        "llm_artifact"
    }

    fn element_name() -> &'static str {
        "Артефакт LLM"
    }

    fn list_name() -> &'static str {
        "Артефакты LLM"
    }

    fn origin() -> Origin {
        Origin::Self_
    }
}

/// DTO для элемента списка артефактов
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmArtifactListItem {
    pub id: String,
    pub code: String,
    pub description: String,
    pub comment: Option<String>,
    pub chat_id: String,
    pub agent_id: String,
    pub artifact_type: String,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub last_executed_at: Option<DateTime<Utc>>,
    pub execution_count: i32,
}

impl From<LlmArtifact> for LlmArtifactListItem {
    fn from(artifact: LlmArtifact) -> Self {
        Self {
            id: artifact.base.id.as_string(),
            code: artifact.base.code,
            description: artifact.base.description,
            comment: artifact.base.comment,
            chat_id: artifact.chat_id.as_string(),
            agent_id: artifact.agent_id.as_string(),
            artifact_type: artifact.artifact_type.as_str().to_string(),
            status: artifact.status.as_str().to_string(),
            created_at: artifact.base.metadata.created_at,
            last_executed_at: artifact.last_executed_at,
            execution_count: artifact.execution_count,
        }
    }
}
