use crate::domain::common::{AggregateId, AggregateRoot, BaseAggregate, EntityMetadata, Origin};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// ID типа для агрегата LLM Agent
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct LlmAgentId(pub Uuid);

impl LlmAgentId {
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

impl AggregateId for LlmAgentId {
    fn as_string(&self) -> String {
        self.0.to_string()
    }
    fn from_string(s: &str) -> Result<Self, String> {
        Uuid::parse_str(s)
            .map(LlmAgentId::new)
            .map_err(|e| format!("Invalid UUID: {}", e))
    }
}

/// Тип/роль агента LLM — определяет набор доступных инструментов и специализацию
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, Default)]
#[serde(rename_all = "snake_case")]
pub enum AgentType {
    /// Бизнес-аналитик: работа с данными маркетплейсов, SQL, BI-отчёты
    #[default]
    BusinessAnalyst,
    /// Системный администратор: мониторинг, производительность, безопасность
    SystemAdmin,
    /// Координатор-администратор: получает новые навыки по умолчанию
    #[serde(rename = "coordinator_admin", alias = "general")]
    CoordinatorAdmin,
    /// Администратор базы знаний: анализирует пробелы и готовит обновления KB
    KbAdmin,
    /// Разработчик: сопровождение системы — консультации пользователей и оформление
    /// тикетов (навык `support`), а также создание/правка/тест JS-плагинов из чата.
    /// Строковый код остаётся `plugin_admin` (значения в БД, матрица доступа, трасса).
    PluginAdmin,
    /// Аналитик продаж: продажи, выручка, заказы, маржа/прибыль
    SalesAnalyst,
    /// Маркетолог: реклама, воронка продаж, поисковая аналитика, промо
    Marketer,
    /// Финансист: главная книга, сверка выручки, взаиморасчёты, комиссии
    Financier,
    /// Тестировщик: обкатка пайплайна на локальной модели — узкая матрица навыков,
    /// без публикации артефактов. Специализация отдельная именно затем, чтобы урезание
    /// доступа не задевало облачных сотрудников.
    Tester,
}

impl AgentType {
    pub fn from_str(s: &str) -> Self {
        match s {
            "system_admin" => AgentType::SystemAdmin,
            "general" | "coordinator_admin" => AgentType::CoordinatorAdmin,
            "kb_admin" => AgentType::KbAdmin,
            "plugin_admin" => AgentType::PluginAdmin,
            "sales_analyst" => AgentType::SalesAnalyst,
            "marketer" => AgentType::Marketer,
            "financier" => AgentType::Financier,
            "tester" => AgentType::Tester,
            _ => AgentType::BusinessAnalyst,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            AgentType::BusinessAnalyst => "business_analyst",
            AgentType::SystemAdmin => "system_admin",
            AgentType::CoordinatorAdmin => "coordinator_admin",
            AgentType::KbAdmin => "kb_admin",
            AgentType::PluginAdmin => "plugin_admin",
            AgentType::SalesAnalyst => "sales_analyst",
            AgentType::Marketer => "marketer",
            AgentType::Financier => "financier",
            AgentType::Tester => "tester",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            AgentType::BusinessAnalyst => "Бизнес-аналитик",
            AgentType::SystemAdmin => "Системный администратор",
            AgentType::CoordinatorAdmin => "Координатор-администратор",
            AgentType::KbAdmin => "Администратор базы знаний",
            AgentType::PluginAdmin => "Разработчик",
            AgentType::SalesAnalyst => "Аналитик продаж",
            AgentType::Marketer => "Маркетолог",
            AgentType::Financier => "Финансист",
            AgentType::Tester => "Тестировщик",
        }
    }
}

/// Тип провайдера LLM
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum LlmProviderType {
    OpenAI,
    OpenRouter,
    DeepSeek,
    Kimi,
    Anthropic,
    Ollama,
}

impl LlmProviderType {
    pub fn from_str(s: &str) -> Result<Self, String> {
        match s {
            "OpenAI" => Ok(LlmProviderType::OpenAI),
            "OpenRouter" => Ok(LlmProviderType::OpenRouter),
            "DeepSeek" => Ok(LlmProviderType::DeepSeek),
            "Kimi" => Ok(LlmProviderType::Kimi),
            "Anthropic" => Ok(LlmProviderType::Anthropic),
            "Ollama" => Ok(LlmProviderType::Ollama),
            _ => Err(format!("Unknown provider type: {}", s)),
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            LlmProviderType::OpenAI => "OpenAI",
            LlmProviderType::OpenRouter => "OpenRouter",
            LlmProviderType::DeepSeek => "DeepSeek",
            LlmProviderType::Kimi => "Kimi",
            LlmProviderType::Anthropic => "Anthropic",
            LlmProviderType::Ollama => "Ollama",
        }
    }
}

fn default_true() -> bool {
    true
}

/// Агрегат LLM Agent — «виртуальный сотрудник».
///
/// Персона (имя, аватар, почта, специализация `agent_type`, должностные обязанности
/// `system_prompt`, расписание) поверх технического подключения a038 (`connection_id`).
/// Техническую диспетчеризацию (провайдер/креды/тюнинг) даёт связанное подключение;
/// поля `provider_type/api_endpoint/api_key/temperature/max_tokens` здесь вестигиальны
/// (сохраняются для неразрушающей миграции, источник истины — подключение). `model_name`
/// переосмыслен как ОПЦИОНАЛЬНО закреплённая сотрудником модель (пусто → дефолт подключения).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmAgent {
    #[serde(flatten)]
    pub base: BaseAggregate<LlmAgentId>,

    /// Тип провайдера (вестигиально — источник истины в подключении a038)
    pub provider_type: LlmProviderType,

    /// API Endpoint (вестигиально — источник истины в подключении a038)
    pub api_endpoint: String,

    /// API ключ (вестигиально — источник истины в подключении a038)
    pub api_key: String,

    /// Закреплённая сотрудником модель (пусто → дефолтная модель подключения)
    pub model_name: String,

    /// Temperature (вестигиально — источник истины в подключении a038)
    pub temperature: f64,

    /// Max tokens (вестигиально — источник истины в подключении a038)
    pub max_tokens: i32,

    /// Должностные обязанности / системный промпт сотрудника
    pub system_prompt: Option<String>,

    /// Флаг основного сотрудника (по умолчанию в чате)
    pub is_primary: bool,

    /// Список доступных моделей (JSON) — вестигиально
    pub available_models: Option<String>,

    /// Специализация (роль/персона) — определяет набор навыков/инструментов
    pub agent_type: AgentType,

    /// Техническое подключение a038, через которое работает сотрудник (UUID).
    pub connection_id: Option<String>,

    /// Аватар: эмодзи / инициалы / URL (строка)
    pub avatar: Option<String>,

    /// Внутренний адрес почты сотрудника (задел для agent-to-agent)
    pub email: Option<String>,

    /// Расписание пробуждения (cron). Задел: исполнитель добавляется отдельной фазой.
    pub schedule_cron: Option<String>,

    /// Активность сотрудника (нанят / в отпуске)
    #[serde(default = "default_true")]
    pub is_active: bool,
}

impl LlmAgent {
    pub fn new_for_insert(
        code: String,
        description: String,
        provider_type: LlmProviderType,
        api_endpoint: String,
        api_key: String,
        model_name: String,
        temperature: f64,
        max_tokens: i32,
        system_prompt: Option<String>,
        is_primary: bool,
        available_models: Option<String>,
    ) -> Self {
        let base = BaseAggregate::new(LlmAgentId::new_v4(), code, description);
        Self {
            base,
            provider_type,
            api_endpoint,
            api_key,
            model_name,
            temperature,
            max_tokens,
            system_prompt,
            is_primary,
            available_models,
            agent_type: AgentType::default(),
            connection_id: None,
            avatar: None,
            email: None,
            schedule_cron: None,
            is_active: true,
        }
    }

    pub fn new_with_id(
        id: LlmAgentId,
        code: String,
        description: String,
        provider_type: LlmProviderType,
        api_endpoint: String,
        api_key: String,
        model_name: String,
        temperature: f64,
        max_tokens: i32,
        system_prompt: Option<String>,
        is_primary: bool,
        available_models: Option<String>,
    ) -> Self {
        let base = BaseAggregate::new(id, code, description);
        Self {
            base,
            provider_type,
            api_endpoint,
            api_key,
            model_name,
            temperature,
            max_tokens,
            system_prompt,
            is_primary,
            available_models,
            agent_type: AgentType::default(),
            connection_id: None,
            avatar: None,
            email: None,
            schedule_cron: None,
            is_active: true,
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
            return Err("Имя сотрудника не может быть пустым".into());
        }
        if self.base.code.trim().is_empty() {
            return Err("Код не может быть пустым".into());
        }
        // Технические поля (endpoint/key/model) больше НЕ обязательны у сотрудника —
        // источник истины перенесён в связанное подключение a038. Обязательна привязка
        // к подключению.
        if self
            .connection_id
            .as_deref()
            .map(|s| s.trim().is_empty())
            .unwrap_or(true)
        {
            return Err("Не выбрано техническое подключение (a038)".into());
        }
        Ok(())
    }

    pub fn before_write(&mut self) {
        self.touch_updated();
    }

    /// Маскирование API ключа для отображения
    pub fn masked_api_key(&self) -> String {
        let key = &self.api_key;
        if key.len() <= 8 {
            return "****".to_string();
        }
        format!("{}...{}", &key[..4], &key[key.len() - 4..])
    }
}

impl AggregateRoot for LlmAgent {
    type Id = LlmAgentId;

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
        "a017"
    }

    fn collection_name() -> &'static str {
        "llm_agent"
    }

    fn element_name() -> &'static str {
        "AI-сотрудник"
    }

    fn list_name() -> &'static str {
        "AI-сотрудники"
    }

    fn origin() -> Origin {
        Origin::Self_
    }
}

#[cfg(test)]
mod agent_type_tests {
    use super::AgentType;

    #[test]
    fn legacy_general_deserializes_as_coordinator_admin() {
        assert_eq!(
            serde_json::from_str::<AgentType>("\"general\"").unwrap(),
            AgentType::CoordinatorAdmin
        );
        assert_eq!(
            serde_json::to_string(&AgentType::CoordinatorAdmin).unwrap(),
            "\"coordinator_admin\""
        );
    }
}
