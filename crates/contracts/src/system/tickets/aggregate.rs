use crate::domain::common::{AggregateId, AggregateRoot, EntityMetadata, Origin};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::dto::{TicketPriority, TicketStatus, TicketType};

// ============================================================================
// ID Type
// ============================================================================

/// Уникальный идентификатор тикета
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TicketId(pub Uuid);

impl TicketId {
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

impl AggregateId for TicketId {
    fn as_string(&self) -> String {
        self.0.to_string()
    }

    fn from_string(s: &str) -> Result<Self, String> {
        Uuid::parse_str(s)
            .map(TicketId::new)
            .map_err(|e| format!("Invalid UUID: {}", e))
    }
}

// ============================================================================
// Aggregate Root
// ============================================================================

/// Тикет — обратная связь пользователя (вопрос, доработка, ошибка, идея).
///
/// Системный объект (таблица `sys_ticket`), но со структурой агрегата.
/// Поля унифицированы с задачей Битрикс24: title↔TITLE, description↔DESCRIPTION,
/// priority↔PRIORITY, deadline↔DEADLINE, author↔CREATED_BY, assignee↔RESPONSIBLE_ID.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ticket {
    pub id: TicketId,
    /// Номер тикета вида "T-000001"
    pub code: String,
    /// Заголовок (название записи)
    pub title: String,
    /// Полное описание (markdown)
    pub description: String,
    pub ticket_type: TicketType,
    pub status: TicketStatus,
    pub priority: TicketPriority,
    pub deadline: Option<String>,
    pub author_user_id: String,
    pub assignee_user_id: Option<String>,
    pub tags: Vec<String>,
    /// Источник: self | llm_chat | bitrix
    pub origin: String,
    /// Tab-key страницы, с которой создан тикет (задел под создание из LLM-чата)
    pub context_page_key: Option<String>,
    /// Снимок контекста страницы (JSON, задел под создание из LLM-чата)
    pub context_json: Option<String>,
    /// ID задачи в Битрикс24 (задел под синхронизацию)
    pub bitrix_task_id: Option<String>,
    pub bitrix_synced_at: Option<String>,
    pub bitrix_received_at: Option<String>,
    pub sync_status: Option<String>,
    /// Метаданные жизненного цикла (created/updated/is_deleted)
    #[serde(default)]
    pub metadata: EntityMetadata,
}

impl AggregateRoot for Ticket {
    type Id = TicketId;

    fn id(&self) -> Self::Id {
        self.id
    }

    fn code(&self) -> &str {
        &self.code
    }

    fn description(&self) -> &str {
        // Название записи для UI/представлений — заголовок тикета
        &self.title
    }

    fn metadata(&self) -> &EntityMetadata {
        &self.metadata
    }

    fn metadata_mut(&mut self) -> &mut EntityMetadata {
        &mut self.metadata
    }

    fn aggregate_index() -> &'static str {
        "sys_ticket"
    }

    fn collection_name() -> &'static str {
        "sys_tickets"
    }

    fn element_name() -> &'static str {
        "Тикет"
    }

    fn list_name() -> &'static str {
        "Тикеты"
    }

    fn origin() -> Origin {
        Origin::Self_
    }
}
