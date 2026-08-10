use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Ошибки LLM провайдера
#[derive(Debug, Error)]
pub enum LlmError {
    #[error("API error: {0}")]
    ApiError(String),

    #[error("Authentication error: {0}")]
    AuthError(String),

    #[error("Invalid request: {0}")]
    InvalidRequest(String),

    #[error("Rate limit exceeded")]
    RateLimitExceeded,

    #[error("Network error: {0}")]
    NetworkError(String),

    #[error("Provider not supported: {0}")]
    UnsupportedProvider(String),
}

/// Роль сообщения в чате
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChatRole {
    System,
    User,
    Assistant,
    /// Результат выполнения инструмента (tool call result)
    Tool,
}

/// Вызов инструмента от LLM (содержится в ответе ассистента)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    /// Уникальный ID вызова (нужен для связи с результатом)
    pub id: String,
    /// Имя функции/инструмента
    pub name: String,
    /// Аргументы в виде JSON-строки
    pub arguments: String,
}

/// Кто разговаривает с моделью — реальный пользователь приложения.
///
/// Нужен инструментам, которые действуют от его имени (оформление тикета заводится
/// на автора-человека, а не на LLM), и для обращения к собеседнику в диалоге.
/// `None` на месте вызывающего означает фоновый сценарий (планировщик, почтовый
/// конвейер) — такие инструменты для него недоступны.
#[derive(Debug, Clone)]
pub struct ToolCaller {
    pub user_id: String,
    pub username: String,
    pub is_admin: bool,
    pub primary_role: String,
}

/// Определение инструмента для передачи LLM
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    /// JSON Schema параметров функции
    pub parameters: serde_json::Value,
}

/// Сообщение чата с поддержкой tool calling
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: ChatRole,
    /// Текстовое содержимое (None для assistant-сообщений с tool_calls)
    pub content: Option<String>,
    /// Private image inputs encoded as data URLs. Used only for user messages.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub image_urls: Vec<String>,
    /// Вызовы инструментов (только для Assistant)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    /// ID вызова инструмента (только для Tool role — связь с ToolCall.id)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

impl ChatMessage {
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: ChatRole::System,
            content: Some(content.into()),
            image_urls: Vec::new(),
            tool_calls: None,
            tool_call_id: None,
        }
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: ChatRole::User,
            content: Some(content.into()),
            image_urls: Vec::new(),
            tool_calls: None,
            tool_call_id: None,
        }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: ChatRole::Assistant,
            content: Some(content.into()),
            image_urls: Vec::new(),
            tool_calls: None,
            tool_call_id: None,
        }
    }

    /// Сообщение ассистента с вызовами инструментов (без текстового контента)
    pub fn assistant_with_tool_calls(tool_calls: Vec<ToolCall>) -> Self {
        Self {
            role: ChatRole::Assistant,
            content: None,
            image_urls: Vec::new(),
            tool_calls: Some(tool_calls),
            tool_call_id: None,
        }
    }

    /// Результат выполнения инструмента
    pub fn tool_result(tool_call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: ChatRole::Tool,
            content: Some(content.into()),
            image_urls: Vec::new(),
            tool_calls: None,
            tool_call_id: Some(tool_call_id.into()),
        }
    }

    /// Получить текст сообщения (пустая строка если None)
    pub fn content_str(&self) -> &str {
        self.content.as_deref().unwrap_or("")
    }
}

/// Ответ от LLM
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmResponse {
    pub content: String,
    /// Вызовы инструментов (пусто если LLM ответил текстом)
    #[serde(default)]
    pub tool_calls: Vec<ToolCall>,
    pub tokens_used: Option<i32>,
    /// Разбивка `tokens_used`. Отдельными полями, а не только суммой: вход и выход
    /// стоят разных денег, и без разбивки стоимость прогона посчитать нечем.
    #[serde(default)]
    pub prompt_tokens: Option<i32>,
    #[serde(default)]
    pub completion_tokens: Option<i32>,
    /// Часть `prompt_tokens`, обслуженная кэшем провайдера (тарифицируется дешевле).
    #[serde(default)]
    pub cached_prompt_tokens: Option<i32>,
    pub model: String,
    pub finish_reason: Option<String>,
    pub confidence: Option<f64>,
}

impl LlmResponse {
    /// Проверить, вернул ли LLM вызовы инструментов
    pub fn has_tool_calls(&self) -> bool {
        !self.tool_calls.is_empty()
    }
}

/// Callback для дельт текстового контента при стриминге.
pub type DeltaCallback<'a> = &'a (dyn Fn(&str) + Send + Sync);

/// Трейт для LLM провайдеров
#[async_trait]
pub trait LlmProvider: Send + Sync {
    /// Отправка запроса к чату (без инструментов)
    async fn chat_completion(&self, messages: &[ChatMessage]) -> Result<LlmResponse, LlmError>;

    /// Отправка запроса к чату с поддержкой инструментов.
    ///
    /// Принимает срезы (а не `Vec`), чтобы вызывающий код (tool-цикл) не клонировал
    /// всю историю сообщений на каждой итерации.
    async fn chat_completion_with_tools(
        &self,
        messages: &[ChatMessage],
        tools: &[ToolDefinition],
    ) -> Result<LlmResponse, LlmError>;

    /// Стриминговый вариант `chat_completion_with_tools`: дельты текстового контента
    /// передаются в `on_delta` по мере генерации (для живого отображения в UI).
    /// Реализация по умолчанию — обычный нестриминговый вызов (on_delta не зовётся),
    /// чтобы провайдеры без поддержки стрима продолжали работать.
    async fn chat_completion_with_tools_streaming(
        &self,
        messages: &[ChatMessage],
        tools: &[ToolDefinition],
        on_delta: DeltaCallback<'_>,
    ) -> Result<LlmResponse, LlmError> {
        let _ = on_delta;
        self.chat_completion_with_tools(messages, tools).await
    }

    /// Тест подключения к провайдеру
    async fn test_connection(&self) -> Result<(), LlmError>;

    /// Получить название провайдера
    fn provider_name(&self) -> &str;
}
