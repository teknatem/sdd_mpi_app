use crate::domain::common::{
    AggregateId, AggregateRoot, BaseAggregate, EntityMetadata, EventStore, Origin,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// Провайдер-enum переиспользуем из a017 (владелец). Персона (AgentType) больше НЕ живёт
// на подключении: a038 — чисто техническая сущность (провайдер+креды+модели). Роль/персона
// перенесена на «виртуального сотрудника» a017. `AgentType` всё ещё ре-экспортируется здесь
// для обратной совместимости импортов.
pub use crate::domain::a017_llm_agent::aggregate::{AgentType, LlmProviderType};

/// ID типа для агрегата LLM Connection
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct LlmConnectionId(pub Uuid);

impl LlmConnectionId {
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

impl AggregateId for LlmConnectionId {
    fn as_string(&self) -> String {
        self.0.to_string()
    }
    fn from_string(s: &str) -> Result<Self, String> {
        Uuid::parse_str(s)
            .map(LlmConnectionId::new)
            .map_err(|e| format!("Invalid UUID: {}", e))
    }
}

/// Агрегат LLM Connection — «Подключение LLM».
///
/// Чисто техническая сущность «провайдер + креды + модели». Персона (роль, промпт) вынесена
/// на «виртуального сотрудника» a017, который ссылается на подключение. Отличается от старого
/// a017 наличием `allowed_models` — курируемого короткого списка технически совместимых моделей,
/// из которых можно выбирать в рамках чата.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConnection {
    #[serde(flatten)]
    pub base: BaseAggregate<LlmConnectionId>,

    /// Тип провайдера
    pub provider_type: LlmProviderType,

    /// API Endpoint
    pub api_endpoint: String,

    /// API ключ (зашифрованный)
    pub api_key: String,

    /// Название модели (по умолчанию)
    pub model_name: String,

    /// Temperature (0.0-2.0)
    pub temperature: f64,

    /// Max tokens
    pub max_tokens: i32,

    /// Размер окна контекста модели в токенах. Из него считается бюджет истории чата
    /// (компакция в a018). Для локальных моделей должен совпадать с фактическим `num_ctx`
    /// сервера Ollama — приложение это не проверяет.
    #[serde(default = "default_context_window")]
    pub context_window: i32,

    /// Флаг основного подключения
    pub is_primary: bool,

    /// Полный список моделей из API провайдера (JSON, кэш fetch-models)
    pub available_models: Option<String>,

    /// Курируемое подмножество разрешённых моделей (JSON-массив model_id).
    /// Именно из него можно выбирать модель в чате. Подмножество `available_models`.
    pub allowed_models: Option<String>,

    /// JSON array: curated subset of allowed_models that accepts image input.
    #[serde(default)]
    pub image_input_models: Option<String>,

    /// Ставка за миллион входных токенов, в валюте `currency`. Пусто = прайс не
    /// задан, стоимость прогонов по этому подключению не считается (а не равна нулю).
    #[serde(default)]
    pub price_in_per_mtok: Option<f64>,

    /// Ставка за миллион выходных токенов.
    #[serde(default)]
    pub price_out_per_mtok: Option<f64>,

    /// Ставка за миллион кэшированных входных токенов. Пусто = скидки нет,
    /// считаем по входной ставке: незаполненное поле не должно выглядеть
    /// как бесплатный кэш и занижать стоимость.
    #[serde(default)]
    pub price_cached_per_mtok: Option<f64>,

    /// Валюта прайса (RUB, USD, …). Хранится строкой: конвертацией курсов
    /// подсистема стоимости не занимается, разные валюты просто не смешиваются.
    #[serde(default)]
    pub currency: Option<String>,
}

/// Дефолт окна контекста. 160 000 подобрано так, чтобы формула бюджета компакции
/// (`context_window * 3 / 4`) дала исторические 120 000 токенов — существующие облачные
/// подключения после миграции ведут себя ровно как раньше.
pub fn default_context_window() -> i32 {
    160_000
}

impl LlmConnection {
    #[allow(clippy::too_many_arguments)]
    pub fn new_for_insert(
        code: String,
        description: String,
        provider_type: LlmProviderType,
        api_endpoint: String,
        api_key: String,
        model_name: String,
        temperature: f64,
        max_tokens: i32,
        context_window: i32,
        is_primary: bool,
        available_models: Option<String>,
        allowed_models: Option<String>,
        image_input_models: Option<String>,
    ) -> Self {
        let base = BaseAggregate::new(LlmConnectionId::new_v4(), code, description);
        Self {
            base,
            provider_type,
            api_endpoint,
            api_key,
            model_name,
            temperature,
            max_tokens,
            context_window,
            is_primary,
            available_models,
            allowed_models,
            image_input_models,
            // Прайс задаётся отдельно (см. `LlmConnectionDto`): у конструктора уже
            // тринадцать позиционных аргументов, четырнадцатым ставку не добавляют.
            price_in_per_mtok: None,
            price_out_per_mtok: None,
            price_cached_per_mtok: None,
            currency: None,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new_with_id(
        id: LlmConnectionId,
        code: String,
        description: String,
        provider_type: LlmProviderType,
        api_endpoint: String,
        api_key: String,
        model_name: String,
        temperature: f64,
        max_tokens: i32,
        context_window: i32,
        is_primary: bool,
        available_models: Option<String>,
        allowed_models: Option<String>,
        image_input_models: Option<String>,
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
            context_window,
            is_primary,
            available_models,
            allowed_models,
            image_input_models,
            // Прайс задаётся отдельно (см. `LlmConnectionDto`): у конструктора уже
            // тринадцать позиционных аргументов, четырнадцатым ставку не добавляют.
            price_in_per_mtok: None,
            price_out_per_mtok: None,
            price_cached_per_mtok: None,
            currency: None,
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
        if self.api_endpoint.trim().is_empty() {
            return Err("API Endpoint обязателен".into());
        }
        // У локального провайдера (Ollama) ключа не существует — требовать строку-заглушку
        // значило бы класть в поле секрета фейковое значение, которое потом всплывёт в
        // `masked_api_key()` и логах как настоящий ключ.
        if self.api_key.trim().is_empty() && self.provider_type != LlmProviderType::Ollama {
            return Err("API ключ обязателен".into());
        }
        if self.model_name.trim().is_empty() {
            return Err("Название модели обязательно".into());
        }
        if !(0.0..=2.0).contains(&self.temperature) {
            return Err("Temperature должна быть в диапазоне 0.0-2.0".into());
        }
        if self.max_tokens < 256 || self.max_tokens > 128000 {
            return Err("Max tokens должен быть в диапазоне 256-128000".into());
        }
        if self.context_window < 4096 || self.context_window > 2_000_000 {
            return Err("Размер контекста должен быть в диапазоне 4096-2000000".into());
        }
        if self.max_tokens >= self.context_window {
            return Err("Max tokens должен быть меньше размера контекста".into());
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

    /// Разобрать `allowed_models` (JSON-массив строк) в вектор model_id.
    /// Пустой/невалидный JSON → пустой вектор (модель не ограничена курированием на бэке).
    pub fn allowed_models_list(&self) -> Vec<String> {
        self.allowed_models
            .as_deref()
            .filter(|s| !s.trim().is_empty())
            .and_then(|s| serde_json::from_str::<Vec<String>>(s).ok())
            .unwrap_or_default()
    }

    pub fn image_input_models_list(&self) -> Vec<String> {
        self.image_input_models
            .as_deref()
            .and_then(|raw| serde_json::from_str::<Vec<String>>(raw).ok())
            .unwrap_or_default()
    }

    pub fn supports_image_input(&self, model: &str) -> bool {
        self.image_input_models_list()
            .iter()
            .any(|item| item == model)
    }
}

impl AggregateRoot for LlmConnection {
    type Id = LlmConnectionId;

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
        "a038"
    }

    fn collection_name() -> &'static str {
        "llm_connection"
    }

    fn element_name() -> &'static str {
        "Подключение LLM"
    }

    fn list_name() -> &'static str {
        "Подключения LLM"
    }

    fn origin() -> Origin {
        Origin::Self_
    }
}
