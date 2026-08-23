use crate::domain::a017_llm_agent::aggregate::LlmAgentId;
use crate::domain::a018_llm_chat::aggregate::LlmChatId;
use crate::domain::common::{
    AggregateId, AggregateRoot, BaseAggregate, EntityMetadata, Origin,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct KbEditId(pub Uuid);

impl KbEditId {
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

impl AggregateId for KbEditId {
    fn as_string(&self) -> String {
        self.0.to_string()
    }

    fn from_string(s: &str) -> Result<Self, String> {
        Uuid::parse_str(s)
            .map(KbEditId::new)
            .map_err(|e| format!("Invalid UUID: {}", e))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum KbEditType {
    Gap,
    Proposal,
    Contradiction,
    Question,
    AllGood,
}

impl KbEditType {
    pub fn from_str(value: &str) -> Self {
        match value {
            "gap" => Self::Gap,
            "contradiction" => Self::Contradiction,
            "question" => Self::Question,
            "all_good" => Self::AllGood,
            _ => Self::Proposal,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Gap => "gap",
            Self::Proposal => "proposal",
            Self::Contradiction => "contradiction",
            Self::Question => "question",
            Self::AllGood => "all_good",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Gap => "Пробел",
            Self::Proposal => "Предложение",
            Self::Contradiction => "Противоречие",
            Self::Question => "Вопрос",
            Self::AllGood => "Все в порядке",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum KbEditStatus {
    Pending,
    InDialog,
    Approved,
    Processing,
    Closed,
    Cancelled,
}

impl KbEditStatus {
    pub fn from_str(value: &str) -> Self {
        match value {
            "in_dialog" => Self::InDialog,
            "approved" => Self::Approved,
            "processing" => Self::Processing,
            "closed" => Self::Closed,
            "cancelled" => Self::Cancelled,
            _ => Self::Pending,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::InDialog => "in_dialog",
            Self::Approved => "approved",
            Self::Processing => "processing",
            Self::Closed => "closed",
            Self::Cancelled => "cancelled",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Pending => "Ожидание",
            Self::InDialog => "В диалоге",
            Self::Approved => "Утверждено",
            Self::Processing => "В обработке",
            Self::Closed => "Закрыто",
            Self::Cancelled => "Отменено",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KbEdit {
    #[serde(flatten)]
    pub base: BaseAggregate<KbEditId>,
    pub edit_type: KbEditType,
    pub status: KbEditStatus,
    pub title: String,
    pub agent_summary: String,
    #[serde(default)]
    pub target_articles: Vec<String>,
    #[serde(default)]
    pub applied_articles: Vec<String>,
    #[serde(default)]
    pub source_chat_ids: Vec<String>,
    pub agent_id: Option<LlmAgentId>,
    pub chat_id: Option<LlmChatId>,
    pub analyze_task_run_id: Option<String>,
    pub post_task_run_id: Option<String>,
}

impl KbEdit {
    pub fn new_for_insert(
        edit_type: KbEditType,
        title: String,
        agent_summary: String,
        target_articles: Vec<String>,
        source_chat_ids: Vec<String>,
        agent_id: Option<LlmAgentId>,
        chat_id: Option<LlmChatId>,
        analyze_task_run_id: Option<String>,
    ) -> Self {
        let code = format!(
            "KB-EDIT-{}",
            &Uuid::new_v4().to_string()[..8].to_uppercase()
        );
        let description = title.clone();
        Self {
            base: BaseAggregate::new(KbEditId::new_v4(), code, description),
            edit_type,
            status: KbEditStatus::Pending,
            title,
            agent_summary,
            target_articles,
            applied_articles: Vec::new(),
            source_chat_ids,
            agent_id,
            chat_id,
            analyze_task_run_id,
            post_task_run_id: None,
        }
    }

    pub fn touch_updated(&mut self) {
        self.base.touch();
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.title.trim().is_empty() {
            return Err("Заголовок обязателен".into());
        }
        if self.agent_summary.trim().is_empty() {
            return Err("Описание предложения обязательно".into());
        }
        Ok(())
    }

    pub fn before_write(&mut self) {
        self.base.description = self.title.clone();
        self.touch_updated();
    }
}

impl AggregateRoot for KbEdit {
    type Id = KbEditId;

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
        "a031"
    }

    fn collection_name() -> &'static str {
        "kb_edit"
    }

    fn element_name() -> &'static str {
        "Редактирование базы знаний"
    }

    fn list_name() -> &'static str {
        "Редактирования базы знаний"
    }

    fn origin() -> Origin {
        Origin::Self_
    }
}
