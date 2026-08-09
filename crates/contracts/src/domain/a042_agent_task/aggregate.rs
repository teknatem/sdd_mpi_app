use crate::domain::a017_llm_agent::aggregate::AgentType;
use crate::domain::common::{
    AggregateId, AggregateRoot, BaseAggregate, EntityMetadata, EventStore, Origin,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Предел длины цепочки поручений.
///
/// Живёт в contracts, а не в конфиге задачи: гард стоит в LLM-инструменте, который
/// работает внутри чат-пайплайна и `sys_tasks.config_json` не читает. Общая
/// константа — единственный способ, чтобы инструмент и воркер знали одно и то же число.
///
/// При значении 1 исполнитель поручения не может делегировать дальше вообще: цепочка
/// A→B заканчивается на B. Это осознанный потолок — канал между агентами приглашает
/// петлю A→B→A, а без потолка она стоит реальных денег на каждом витке.
pub const MAX_DELEGATION_DEPTH: i32 = 1;

/// Максимум незакрытых поручений на один чат-заказчик.
pub const MAX_OUTSTANDING_PER_CHAT: u64 = 3;

/// Максимум записей в очереди целиком: один зациклившийся чат не должен уморить всех.
pub const MAX_GLOBAL_BACKLOG: u64 = 50;

/// ID типа для агрегата Agent Task
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AgentTaskId(pub Uuid);

impl AgentTaskId {
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

impl AggregateId for AgentTaskId {
    fn as_string(&self) -> String {
        self.0.to_string()
    }
    fn from_string(s: &str) -> Result<Self, String> {
        Uuid::parse_str(s)
            .map(AgentTaskId::new)
            .map_err(|e| format!("Invalid UUID: {}", e))
    }
}

/// Статус поручения.
///
/// ```text
/// pending    → processing | cancelled
/// processing → done | failed | pending (повтор) | cancelled
/// failed     → pending (ручной перезапуск) | cancelled
/// done       → ∅            cancelled → ∅
/// ```
///
/// Отдельного `retrying` нет: запись, ждущая повтора, — это `pending` с
/// `next_attempt_at` в будущем и `attempts > 0`. Отдельного `rejected` нет:
/// инструмент отказывает синхронно, строка не создаётся вовсе.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentTaskStatus {
    /// В очереди (в т.ч. ожидает повтора).
    Pending,
    /// Захвачено прогоном воркера и исполняется.
    Processing,
    /// Исполнено, `result_text` заполнен.
    Done,
    /// Провалено окончательно (попытки исчерпаны или ошибка непереповторяемая).
    Failed,
    /// Снято вручную.
    Cancelled,
}

impl AgentTaskStatus {
    /// Тотальный разбор: неизвестное значение из БД деградирует в начальный статус,
    /// а не роняет десериализацию всей строки.
    pub fn from_str(value: &str) -> Self {
        match value {
            "processing" => Self::Processing,
            "done" => Self::Done,
            "failed" => Self::Failed,
            "cancelled" => Self::Cancelled,
            _ => Self::Pending,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Processing => "processing",
            Self::Done => "done",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Pending => "Ожидание",
            Self::Processing => "Исполняется",
            Self::Done => "Готово",
            Self::Failed => "Ошибка",
            Self::Cancelled => "Отменено",
        }
    }

    /// Терминальный статус — из него переходов нет.
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Done | Self::Cancelled)
    }

    /// Легален ли переход. Проверяется в единственном `set_status` сервиса:
    /// у очереди четыре независимых писателя (инструмент постановки, захват,
    /// завершение, развёртка зависших), и UI-only контроль как у a031 здесь
    /// уже не удерживает.
    ///
    /// NB: `done → pending` запрещён намеренно — это и есть формальная причина,
    /// по которой развёртка зависших не может воскресить завершённое поручение.
    pub fn can_transition(&self, to: &AgentTaskStatus) -> bool {
        use AgentTaskStatus::*;
        match (self, to) {
            (Pending, Processing) | (Pending, Cancelled) => true,
            (Processing, Done)
            | (Processing, Failed)
            | (Processing, Pending)
            | (Processing, Cancelled) => true,
            (Failed, Pending) | (Failed, Cancelled) => true,
            _ => false,
        }
    }

    /// Статусы, по которым поручение считается незакрытым.
    pub const OPEN: &'static [AgentTaskStatus] = &[Self::Pending, Self::Processing];
}

impl Default for AgentTaskStatus {
    fn default() -> Self {
        Self::Pending
    }
}

/// Агрегат Agent Task — «Поручение AI-сотруднику».
///
/// Рабочий элемент очереди, через которую один агент передаёт задачу другому.
/// Постановка — LLM-инструментом `create_agent_task`; исполнение — регламентным
/// заданием `task029_agent_task_runner`, которое гоняет поручение через служебный
/// чат a018 от лица сотрудника нужной специализации.
///
/// Тяжёлое содержимое прогона (переписка, трасса инструментов) живёт в чате a018;
/// здесь — постановка, статус, учёт попыток и итоговый ответ со ссылками.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentTask {
    #[serde(flatten)]
    pub base: BaseAggregate<AgentTaskId>,

    /// Статус обработки.
    pub status: AgentTaskStatus,

    /// Специализация исполнителя (что заказывали).
    pub target_agent_type: AgentType,

    /// Постановка задачи — уходит исполнителю как есть.
    pub request_text: String,

    /// Структурный контекст, который нельзя терять при пересказе (JSON-объект строкой).
    pub payload_json: Option<String>,

    /// a017 агента-заказчика.
    pub requested_by_agent_ref: Option<String>,

    /// a018 чата-заказчика: якорь и для выдачи результата, и для расчёта глубины.
    pub requested_by_chat_ref: Option<String>,

    /// `sys_users.id` человека, чей вопрос породил поручение — владелец чата исполнения.
    pub requested_by_user_ref: Option<String>,

    /// Родительское поручение (если заказчик сам исполнял поручение).
    pub parent_task_ref: Option<String>,

    /// Глубина цепочки; вычисляется при постановке, не приходит от модели.
    pub depth: i32,

    /// Число захватов. Растёт при захвате, а не при провале.
    pub attempts: i32,

    /// Потолок попыток.
    pub max_attempts: i32,

    /// Не брать в работу раньше этого момента (RFC3339) — гейт бэкоффа.
    pub next_attempt_at: Option<String>,

    /// `session_id` владеющего прогона — ключ join'а в `sys_task_runs` и в файл лога.
    pub claim_session_id: Option<String>,

    /// Момент захвата (RFC3339) — база отсчёта для развёртки зависших.
    pub started_at: Option<String>,

    /// Момент завершения (RFC3339).
    pub finished_at: Option<String>,

    /// a017 сотрудника, который реально исполнял (может отличаться от заказанного типа).
    pub executor_agent_ref: Option<String>,

    /// a018 чата исполнения; пишется ДО прогона, чтобы диалог нашёлся и после падения.
    pub result_chat_ref: Option<String>,

    /// id итогового сообщения ассистента.
    pub result_message_ref: Option<String>,

    /// a019 артефакта: если ответ — график или таблица, артефакт и есть результат.
    pub result_artifact_ref: Option<String>,

    /// Итоговый ответ исполнителя.
    pub result_text: Option<String>,

    /// Диагностика провала.
    pub error: Option<String>,
}

impl AgentTask {
    /// Создать поручение для вставки в очередь.
    pub fn new_for_insert(
        code: String,
        description: String,
        target_agent_type: AgentType,
        request_text: String,
        max_attempts: i32,
    ) -> Self {
        let base = BaseAggregate::new(AgentTaskId::new_v4(), code, description);
        Self {
            base,
            status: AgentTaskStatus::Pending,
            target_agent_type,
            request_text,
            payload_json: None,
            requested_by_agent_ref: None,
            requested_by_chat_ref: None,
            requested_by_user_ref: None,
            parent_task_ref: None,
            depth: 0,
            attempts: 0,
            max_attempts,
            next_attempt_at: None,
            claim_session_id: None,
            started_at: None,
            finished_at: None,
            executor_agent_ref: None,
            result_chat_ref: None,
            result_message_ref: None,
            result_artifact_ref: None,
            result_text: None,
            error: None,
        }
    }

    pub fn to_string_id(&self) -> String {
        self.base.id.as_string()
    }

    pub fn touch_updated(&mut self) {
        self.base.touch();
    }

    /// Незакрытое поручение — ещё может дойти до исполнения.
    pub fn is_open(&self) -> bool {
        matches!(
            self.status,
            AgentTaskStatus::Pending | AgentTaskStatus::Processing
        )
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.base.code.trim().is_empty() {
            return Err("Код не может быть пустым".into());
        }
        if self.base.description.trim().is_empty() {
            return Err("Заголовок поручения не может быть пустым".into());
        }
        if self.request_text.trim().is_empty() {
            return Err("Постановка задачи не может быть пустой".into());
        }
        if self.depth > MAX_DELEGATION_DEPTH {
            return Err(format!(
                "Превышена глубина цепочки поручений: {} > {}",
                self.depth, MAX_DELEGATION_DEPTH
            ));
        }
        if self.max_attempts < 1 {
            return Err("max_attempts должен быть не меньше 1".into());
        }
        Ok(())
    }

    pub fn before_write(&mut self) {
        self.touch_updated();
    }
}

impl AggregateRoot for AgentTask {
    type Id = AgentTaskId;

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

    fn events(&self) -> &EventStore {
        &self.base.events
    }

    fn events_mut(&mut self) -> &mut EventStore {
        &mut self.base.events
    }

    fn aggregate_index() -> &'static str {
        "a042"
    }

    fn collection_name() -> &'static str {
        "agent_task"
    }

    fn element_name() -> &'static str {
        "Поручение AI-сотруднику"
    }

    fn list_name() -> &'static str {
        "Поручения AI-сотрудникам"
    }

    fn origin() -> Origin {
        Origin::Self_
    }
}

#[cfg(test)]
mod tests {
    use super::AgentTaskStatus::*;
    use super::*;

    #[test]
    fn status_round_trips_through_string() {
        for s in [Pending, Processing, Done, Failed, Cancelled] {
            assert_eq!(AgentTaskStatus::from_str(s.as_str()), s);
        }
        // Мусор из БД деградирует в начальный статус, а не роняет чтение строки.
        assert_eq!(AgentTaskStatus::from_str("что-то новое"), Pending);
    }

    /// Полная матрица переходов. Главное здесь — запрещённый `done → pending`:
    /// именно он документирует, почему развёртка зависших не воскрешает
    /// завершённое поручение и не оплачивает второй прогон.
    #[test]
    fn transition_matrix_is_exact() {
        let legal = [
            (Pending, Processing),
            (Pending, Cancelled),
            (Processing, Done),
            (Processing, Failed),
            (Processing, Pending),
            (Processing, Cancelled),
            (Failed, Pending),
            (Failed, Cancelled),
        ];
        let all = [Pending, Processing, Done, Failed, Cancelled];
        for from in all {
            for to in all {
                let expected = legal.contains(&(from, to));
                assert_eq!(
                    from.can_transition(&to),
                    expected,
                    "переход {:?} → {:?}",
                    from,
                    to
                );
            }
        }
        assert!(!Done.can_transition(&Pending));
        assert!(Done.is_terminal() && Cancelled.is_terminal());
    }

    #[test]
    fn validate_rejects_overdeep_chain() {
        let mut task = AgentTask::new_for_insert(
            "AT-TEST".into(),
            "Проверка".into(),
            AgentType::SalesAnalyst,
            "Посчитай выручку за июль".into(),
            2,
        );
        assert!(task.validate().is_ok());
        task.depth = MAX_DELEGATION_DEPTH + 1;
        assert!(task.validate().is_err());
    }
}
