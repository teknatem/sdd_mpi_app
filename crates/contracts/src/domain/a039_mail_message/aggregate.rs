use crate::domain::common::{
    AggregateId, AggregateRoot, BaseAggregate, EntityMetadata, Origin,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// ID типа для агрегата Mail Message
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MailMessageId(pub Uuid);

impl MailMessageId {
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

impl AggregateId for MailMessageId {
    fn as_string(&self) -> String {
        self.0.to_string()
    }
    fn from_string(s: &str) -> Result<Self, String> {
        Uuid::parse_str(s)
            .map(MailMessageId::new)
            .map_err(|e| format!("Invalid UUID: {}", e))
    }
}

/// Направление письма.
pub mod direction {
    pub const INBOUND: &str = "inbound";
    pub const OUTBOUND: &str = "outbound";
}

/// Статусы обработки письма (статус-машина конвейера почтовых задач).
///
/// Входящее: `received` → (`prepared` | `rejected_unknown_sender` | `rejected_forbidden` | `failed`)
/// → `replied`. `overdue` — пометка просрочки (SLA) для регламентного задания ответов.
pub mod status {
    /// Письмо зафиксировано, ещё не обработано агентом.
    pub const RECEIVED: &str = "received";
    /// Агент подготовил ответ (готово к отправке).
    pub const PREPARED: &str = "prepared";
    /// Ответ отправлен пользователю.
    pub const REPLIED: &str = "replied";
    /// Отправитель не найден среди активных пользователей — без ответа.
    pub const REJECTED_UNKNOWN_SENDER: &str = "rejected_unknown_sender";
    /// У отправителя нет прав на нужного специалиста — будет вежливый отказ.
    pub const REJECTED_FORBIDDEN: &str = "rejected_forbidden";
    /// Ошибка прогона агента.
    pub const FAILED: &str = "failed";
    /// Просрочка обработки/ответа (SLA).
    pub const OVERDUE: &str = "overdue";

    /// Статусы входящих, ожидающих отправки ответа регламентным заданием.
    pub const PENDING_REPLY: &[&str] = &[PREPARED, REJECTED_FORBIDDEN, FAILED];
}

/// Агрегат Mail Message — «Письмо (журнал)».
///
/// Краткая запись по одному входящему/исходящему письму почтового конвейера.
/// Полное тело письма и переписки живёт в связанном чате a018; здесь — только сводка,
/// статус обработки и ссылки на чат/сообщение/артефакт.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MailMessage {
    #[serde(flatten)]
    pub base: BaseAggregate<MailMessageId>,

    /// Направление: `inbound` | `outbound` (см. [`direction`]).
    pub direction: String,

    /// IMAP UID (для входящих) — дедуп/повторная обработка.
    pub imap_uid: Option<i64>,

    /// RFC Message-ID заголовка письма (тред + идемпотентность).
    pub message_id_hdr: Option<String>,

    /// Для исходящего — id входящего a039, на которое это ответ.
    pub in_reply_to_ref: Option<String>,

    pub from_addr: String,
    pub to_addr: String,
    pub subject: String,

    /// Краткая выжимка тела (первые ~500 символов).
    pub body_excerpt: String,

    /// Связанный пользователь `sys_users.id` (отправитель для inbound / получатель для outbound).
    pub user_ref: Option<String>,

    /// Классифицированный интент запроса.
    pub intent: Option<String>,

    /// Тип агента-исполнителя (business_analyst | kb_admin | ...).
    pub agent_type: Option<String>,

    /// Ссылки на объекты прогона агента.
    pub chat_ref: Option<String>,
    pub message_ref: Option<String>,
    pub artifact_ref: Option<String>,

    /// Статус обработки (см. [`status`]).
    pub status: String,

    /// Текст ошибки/причины отказа.
    pub error: Option<String>,

    /// Крайний срок обработки/ответа (RFC3339), для SLA-контроля.
    pub due_at: Option<String>,
}

impl MailMessage {
    /// Создать запись журнала для вставки.
    pub fn new_for_insert(
        code: String,
        description: String,
        direction: String,
        status: String,
    ) -> Self {
        let base = BaseAggregate::new(MailMessageId::new_v4(), code, description);
        Self {
            base,
            direction,
            imap_uid: None,
            message_id_hdr: None,
            in_reply_to_ref: None,
            from_addr: String::new(),
            to_addr: String::new(),
            subject: String::new(),
            body_excerpt: String::new(),
            user_ref: None,
            intent: None,
            agent_type: None,
            chat_ref: None,
            message_ref: None,
            artifact_ref: None,
            status,
            error: None,
            due_at: None,
        }
    }

    pub fn to_string_id(&self) -> String {
        self.base.id.as_string()
    }

    pub fn touch_updated(&mut self) {
        self.base.touch();
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.base.code.trim().is_empty() {
            return Err("Код не может быть пустым".into());
        }
        if self.base.description.trim().is_empty() {
            return Err("Описание не может быть пустым".into());
        }
        if self.direction != direction::INBOUND && self.direction != direction::OUTBOUND {
            return Err("direction должно быть inbound|outbound".into());
        }
        Ok(())
    }

    pub fn before_write(&mut self) {
        self.touch_updated();
    }
}

impl AggregateRoot for MailMessage {
    type Id = MailMessageId;

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
        "a039"
    }

    fn collection_name() -> &'static str {
        "mail_message"
    }

    fn element_name() -> &'static str {
        "Письмо"
    }

    fn list_name() -> &'static str {
        "Письма (журнал)"
    }

    fn origin() -> Origin {
        Origin::Self_
    }
}
