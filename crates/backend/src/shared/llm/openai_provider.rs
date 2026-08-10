use super::types::{
    ChatMessage, ChatRole, LlmError, LlmProvider, LlmResponse, ToolCall, ToolDefinition,
};
use async_openai::{
    config::OpenAIConfig,
    types::chat::{
        ChatChoice, ChatCompletionMessageToolCall, ChatCompletionMessageToolCalls,
        ChatCompletionRequestAssistantMessageArgs, ChatCompletionRequestMessage,
        ChatCompletionRequestMessageContentPartImage, ChatCompletionRequestMessageContentPartText,
        ChatCompletionRequestSystemMessageArgs, ChatCompletionRequestToolMessageArgs,
        ChatCompletionRequestUserMessageArgs, ChatCompletionRequestUserMessageContentPart,
        ChatCompletionTool, ChatCompletionTools, CompletionUsage, CreateChatCompletionRequest,
        CreateChatCompletionRequestArgs, CreateChatCompletionResponse, FunctionCall,
        FunctionObject, ImageUrl,
    },
    Client,
};
use async_trait::async_trait;

/// Разбивка токенов одного ответа провайдера.
///
/// Вход, выход и кэшированная часть входа тарифицируются по разным ставкам,
/// поэтому одной суммы `total_tokens` для расчёта стоимости не хватает.
/// Провайдер может не прислать `usage` вовсе (или прислать без разбивки) —
/// тогда поля остаются `None`, и стоимость честно не считается.
#[derive(Default, Clone, Copy)]
struct UsageSplit {
    total: Option<i32>,
    prompt: Option<i32>,
    completion: Option<i32>,
    cached_prompt: Option<i32>,
}

impl UsageSplit {
    fn from_usage(usage: Option<&CompletionUsage>) -> Self {
        match usage {
            Some(u) => Self {
                total: Some(u.total_tokens as i32),
                prompt: Some(u.prompt_tokens as i32),
                completion: Some(u.completion_tokens as i32),
                cached_prompt: u
                    .prompt_tokens_details
                    .as_ref()
                    .and_then(|details| details.cached_tokens)
                    .map(|tokens| tokens as i32),
            },
            None => Self::default(),
        }
    }
}

/// OpenAI провайдер
pub struct OpenAiProvider {
    client: Client<OpenAIConfig>,
    provider_name: &'static str,
    model: String,
    temperature: f32,
    max_tokens: u32,
    use_legacy_max_tokens: bool,
    request_logprobs: bool,
}

impl OpenAiProvider {
    /// Создать новый OpenAI провайдер
    pub fn new(api_key: String, model: String, temperature: f64, max_tokens: i32) -> Self {
        let config = OpenAIConfig::new().with_api_key(api_key);
        let client = Client::with_config(config);

        Self {
            client,
            provider_name: "OpenAI",
            model,
            temperature: temperature as f32,
            max_tokens: max_tokens as u32,
            use_legacy_max_tokens: false,
            request_logprobs: true,
        }
    }

    /// Создать с кастомным endpoint (для совместимых API)
    pub fn new_with_endpoint(
        api_endpoint: String,
        api_key: String,
        model: String,
        temperature: f64,
        max_tokens: i32,
    ) -> Self {
        let config = OpenAIConfig::new()
            .with_api_key(api_key)
            .with_api_base(api_endpoint);
        let client = Client::with_config(config);

        Self {
            client,
            provider_name: "OpenAI",
            model,
            temperature: temperature as f32,
            max_tokens: max_tokens as u32,
            use_legacy_max_tokens: false,
            request_logprobs: true,
        }
    }

    /// Create a provider for OpenAI-compatible APIs with custom request quirks.
    pub fn new_compatible(
        provider_name: &'static str,
        api_endpoint: String,
        api_key: String,
        model: String,
        temperature: f64,
        max_tokens: i32,
        use_legacy_max_tokens: bool,
        request_logprobs: bool,
    ) -> Self {
        let config = OpenAIConfig::new()
            .with_api_key(api_key)
            .with_api_base(api_endpoint);
        let client = Client::with_config(config);

        Self {
            client,
            provider_name,
            model,
            temperature: temperature as f32,
            max_tokens: max_tokens as u32,
            use_legacy_max_tokens,
            request_logprobs,
        }
    }

    /// Конвертировать наши сообщения в формат OpenAI
    fn convert_messages(
        &self,
        messages: &[ChatMessage],
    ) -> Result<Vec<ChatCompletionRequestMessage>, LlmError> {
        let mut openai_messages = Vec::new();

        for msg in messages {
            let openai_msg = match &msg.role {
                ChatRole::System => ChatCompletionRequestSystemMessageArgs::default()
                    .content(msg.content_str())
                    .build()
                    .map_err(|e| LlmError::InvalidRequest(e.to_string()))?
                    .into(),
                ChatRole::User => {
                    let mut builder = ChatCompletionRequestUserMessageArgs::default();
                    if msg.image_urls.is_empty() {
                        builder.content(msg.content_str());
                    } else {
                        let mut parts = Vec::new();
                        if !msg.content_str().trim().is_empty() {
                            parts.push(ChatCompletionRequestUserMessageContentPart::Text(
                                ChatCompletionRequestMessageContentPartText {
                                    text: msg.content_str().to_string(),
                                },
                            ));
                        }
                        parts.extend(msg.image_urls.iter().cloned().map(|url| {
                            ChatCompletionRequestUserMessageContentPart::ImageUrl(
                                ChatCompletionRequestMessageContentPartImage {
                                    image_url: ImageUrl { url, detail: None },
                                },
                            )
                        }));
                        builder.content(parts);
                    }
                    builder
                        .build()
                        .map_err(|e| LlmError::InvalidRequest(e.to_string()))?
                        .into()
                }
                ChatRole::Assistant => {
                    let mut builder = ChatCompletionRequestAssistantMessageArgs::default();
                    if let Some(content) = &msg.content {
                        builder.content(content.as_str());
                    }
                    if let Some(tool_calls) = &msg.tool_calls {
                        let openai_tool_calls: Vec<ChatCompletionMessageToolCalls> = tool_calls
                            .iter()
                            .map(|tc| {
                                ChatCompletionMessageToolCalls::Function(
                                    ChatCompletionMessageToolCall {
                                        id: tc.id.clone(),
                                        function: FunctionCall {
                                            name: tc.name.clone(),
                                            arguments: tc.arguments.clone(),
                                        },
                                    },
                                )
                            })
                            .collect();
                        builder.tool_calls(openai_tool_calls);
                    }
                    builder
                        .build()
                        .map_err(|e| LlmError::InvalidRequest(e.to_string()))?
                        .into()
                }
                ChatRole::Tool => {
                    let tool_call_id = msg.tool_call_id.as_deref().unwrap_or("").to_string();
                    ChatCompletionRequestToolMessageArgs::default()
                        .content(msg.content_str())
                        .tool_call_id(tool_call_id)
                        .build()
                        .map_err(|e| LlmError::InvalidRequest(e.to_string()))?
                        .into()
                }
            };
            openai_messages.push(openai_msg);
        }

        Ok(openai_messages)
    }

    /// Конвертировать определения инструментов в формат OpenAI
    fn convert_tools(&self, tools: &[ToolDefinition]) -> Vec<ChatCompletionTools> {
        tools
            .iter()
            .map(|t| {
                ChatCompletionTools::Function(ChatCompletionTool {
                    function: FunctionObject {
                        name: t.name.clone(),
                        description: Some(t.description.clone()),
                        parameters: Some(t.parameters.clone()),
                        strict: None,
                    },
                })
            })
            .collect()
    }

    /// Отправить запрос с повторами при ВРЕМЕННЫХ ошибках сети/сервиса.
    ///
    /// Транспортные сбои (`error sending request`, разрыв соединения, таймаут) и
    /// перегрузка апстрима (429/502/503/504) — преходящие: один сетевой «чих» не
    /// должен ронять весь ход чата. Невосстановимые ошибки (401/403/400) возвращаются
    /// сразу, без повторов.
    async fn create_with_retries(
        &self,
        request: CreateChatCompletionRequest,
    ) -> Result<CreateChatCompletionResponse, LlmError> {
        const MAX_ATTEMPTS: u32 = 3;
        let mut attempt = 0;
        loop {
            attempt += 1;
            match self.client.chat().create(request.clone()).await {
                Ok(response) => return Ok(response),
                Err(e) => {
                    let err_str = normalize_api_error_message(&e.to_string());
                    if is_transient_error(&err_str) && attempt < MAX_ATTEMPTS {
                        let backoff_ms = 400u64 * 2u64.pow(attempt - 1); // 400ms, 800ms
                        tracing::warn!(
                            "[llm] временная ошибка ({}) попытка {}/{}, повтор через {}ms: {}",
                            self.provider_name,
                            attempt,
                            MAX_ATTEMPTS,
                            backoff_ms,
                            err_str
                        );
                        tokio::time::sleep(std::time::Duration::from_millis(backoff_ms)).await;
                        continue;
                    }
                    return Err(classify_api_error(&err_str));
                }
            }
        }
    }

    /// Собрать CreateChatCompletionRequest для обычного или стримингового вызова.
    fn build_request(
        &self,
        messages: &[ChatMessage],
        tools: &[ToolDefinition],
        stream: bool,
    ) -> Result<CreateChatCompletionRequest, LlmError> {
        let openai_messages = self.convert_messages(messages)?;
        let has_tools = !tools.is_empty();

        let mut request_builder = CreateChatCompletionRequestArgs::default();
        request_builder.model(&self.model).messages(openai_messages);

        // Добавляем инструменты если есть
        if has_tools {
            use async_openai::types::chat::{ChatCompletionToolChoiceOption, ToolChoiceOptions};
            request_builder.tools(self.convert_tools(tools));
            // Явно указываем "auto" чтобы модель сама решала вызывать ли инструмент.
            // Без этого некоторые endpoint'ы могут игнорировать инструменты.
            request_builder.tool_choice(ChatCompletionToolChoiceOption::Mode(
                ToolChoiceOptions::Auto,
            ));
        }

        // Добавляем расширенные параметры только для поддерживающих моделей
        if Self::supports_advanced_params(&self.model) {
            request_builder.temperature(self.temperature);
            if self.use_legacy_max_tokens {
                #[allow(deprecated)]
                {
                    request_builder.max_tokens(self.max_tokens);
                }
            } else {
                request_builder.max_completion_tokens(self.max_tokens);
            }
            // logprobs несовместимы с tool calling и не нужны при стриминге
            if self.request_logprobs && !has_tools && !stream {
                request_builder.logprobs(true).top_logprobs(1);
            }
        }

        if stream {
            use async_openai::types::chat::ChatCompletionStreamOptions;
            request_builder.stream(true);
            // include_usage: последний чанк несёт usage всего запроса
            request_builder.stream_options(ChatCompletionStreamOptions {
                include_usage: Some(true),
                include_obfuscation: None,
            });
        }

        request_builder
            .build()
            .map_err(|e| LlmError::InvalidRequest(e.to_string()))
    }

    /// Извлечь tool calls из ответа OpenAI
    fn extract_tool_calls(&self, choice: &ChatChoice) -> Vec<ToolCall> {
        let Some(tool_calls) = &choice.message.tool_calls else {
            return vec![];
        };
        tool_calls
            .iter()
            .filter_map(|tc| match tc {
                ChatCompletionMessageToolCalls::Function(f) => Some(ToolCall {
                    id: f.id.clone(),
                    name: f.function.name.clone(),
                    arguments: f.function.arguments.clone(),
                }),
                ChatCompletionMessageToolCalls::Custom(_) => None,
            })
            .collect()
    }
}

#[async_trait]
impl LlmProvider for OpenAiProvider {
    async fn chat_completion(&self, messages: &[ChatMessage]) -> Result<LlmResponse, LlmError> {
        self.chat_completion_with_tools(messages, &[]).await
    }

    async fn chat_completion_with_tools(
        &self,
        messages: &[ChatMessage],
        tools: &[ToolDefinition],
    ) -> Result<LlmResponse, LlmError> {
        let request = self.build_request(messages, tools, false)?;
        let response = self.create_with_retries(request).await?;

        let choice = response
            .choices
            .first()
            .ok_or_else(|| LlmError::ApiError("No response from API".to_string()))?;

        let mut content = choice.message.content.clone().unwrap_or_default();
        let mut tool_calls = self.extract_tool_calls(choice);

        // Совместимость: некоторые модели (DeepSeek-v4 через OpenRouter) возвращают вызовы
        // инструментов ТЕКСТОМ в своём DSML-формате, а не в поле tool_calls. Если стандартных
        // вызовов нет, но в тексте есть такая разметка — распарсим её, чтобы цикл выполнил
        // инструменты, а не принял сырой markup за финальный ответ.
        if tool_calls.is_empty() {
            if let Some((parsed, cleaned)) =
                super::deepseek_tools::parse_inline_tool_calls(&content)
            {
                tracing::warn!(
                    "[provider] распознаны inline tool-calls в тексте ответа (DSML): {} шт.",
                    parsed.len()
                );
                tool_calls = parsed;
                content = cleaned;
            }
        }

        let usage = UsageSplit::from_usage(response.usage.as_ref());
        let finish_reason = choice.finish_reason.as_ref().map(|r| format!("{:?}", r));

        // Вычислить confidence из logprobs (только если нет tool calls)
        let confidence = if tool_calls.is_empty() {
            choice.logprobs.as_ref().and_then(|logprobs| {
                if let Some(content_logprobs) = &logprobs.content {
                    if content_logprobs.is_empty() {
                        return None;
                    }
                    let sum: f64 = content_logprobs
                        .iter()
                        .map(|token| (token.logprob as f64).exp())
                        .sum();
                    let count = content_logprobs.len();
                    if count > 0 {
                        Some(sum / count as f64)
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
        } else {
            None
        };

        Ok(LlmResponse {
            content,
            tool_calls,
            tokens_used: usage.total,
            prompt_tokens: usage.prompt,
            completion_tokens: usage.completion,
            cached_prompt_tokens: usage.cached_prompt,
            model: response.model.clone(),
            finish_reason,
            confidence,
        })
    }

    async fn chat_completion_with_tools_streaming(
        &self,
        messages: &[ChatMessage],
        tools: &[ToolDefinition],
        on_delta: super::types::DeltaCallback<'_>,
    ) -> Result<LlmResponse, LlmError> {
        use futures_util::StreamExt;

        let request = self.build_request(messages, tools, true)?;

        const MAX_ATTEMPTS: u32 = 3;
        let mut attempt = 0u32;
        'retry: loop {
            attempt += 1;

            let mut stream = match self.client.chat().create_stream(request.clone()).await {
                Ok(stream) => stream,
                Err(e) => {
                    let err_str = normalize_api_error_message(&e.to_string());
                    if is_transient_error(&err_str) && attempt < MAX_ATTEMPTS {
                        let backoff_ms = 400u64 * 2u64.pow(attempt - 1);
                        tracing::warn!(
                            "[llm-stream] временная ошибка ({}) попытка {}/{}, повтор через {}ms: {}",
                            self.provider_name, attempt, MAX_ATTEMPTS, backoff_ms, err_str
                        );
                        tokio::time::sleep(std::time::Duration::from_millis(backoff_ms)).await;
                        continue 'retry;
                    }
                    return Err(classify_api_error(&err_str));
                }
            };

            let mut content = String::new();
            let mut model = self.model.clone();
            let mut finish_reason: Option<String> = None;
            let mut usage = UsageSplit::default();
            // Аккумулятор чанков tool-call'ов: index → (id, name, arguments)
            let mut tool_acc: std::collections::BTreeMap<u32, (String, String, String)> =
                std::collections::BTreeMap::new();
            // Ошибка середины потока: ретраим только если пользователю ещё ничего не показано
            let mut midstream_error: Option<String> = None;

            while let Some(item) = stream.next().await {
                match item {
                    Ok(chunk) => {
                        if chunk.usage.is_some() {
                            usage = UsageSplit::from_usage(chunk.usage.as_ref());
                        }
                        if !chunk.model.is_empty() {
                            model = chunk.model.clone();
                        }
                        if let Some(choice) = chunk.choices.first() {
                            if let Some(delta) = &choice.delta.content {
                                if !delta.is_empty() {
                                    content.push_str(delta);
                                    on_delta(delta);
                                }
                            }
                            if let Some(tc_chunks) = &choice.delta.tool_calls {
                                for tc in tc_chunks {
                                    let entry = tool_acc.entry(tc.index).or_default();
                                    if let Some(id) = &tc.id {
                                        entry.0.push_str(id);
                                    }
                                    if let Some(func) = &tc.function {
                                        if let Some(name) = &func.name {
                                            entry.1.push_str(name);
                                        }
                                        if let Some(args) = &func.arguments {
                                            entry.2.push_str(args);
                                        }
                                    }
                                }
                            }
                            if let Some(reason) = &choice.finish_reason {
                                finish_reason = Some(format!("{:?}", reason));
                            }
                        }
                    }
                    Err(e) => {
                        midstream_error = Some(normalize_api_error_message(&e.to_string()));
                        break;
                    }
                }
            }

            if let Some(err_str) = midstream_error {
                let nothing_emitted = content.is_empty() && tool_acc.is_empty();
                if nothing_emitted && is_transient_error(&err_str) && attempt < MAX_ATTEMPTS {
                    let backoff_ms = 400u64 * 2u64.pow(attempt - 1);
                    tracing::warn!(
                        "[llm-stream] обрыв потока ({}) попытка {}/{}, повтор через {}ms: {}",
                        self.provider_name,
                        attempt,
                        MAX_ATTEMPTS,
                        backoff_ms,
                        err_str
                    );
                    tokio::time::sleep(std::time::Duration::from_millis(backoff_ms)).await;
                    continue 'retry;
                }
                return Err(classify_api_error(&err_str));
            }

            let mut tool_calls: Vec<ToolCall> = tool_acc
                .into_values()
                .map(|(id, name, arguments)| ToolCall {
                    id,
                    name,
                    arguments,
                })
                .collect();

            // Совместимость: inline DSML tool-calls в тексте (см. нестриминговый путь).
            if tool_calls.is_empty() {
                if let Some((parsed, cleaned)) =
                    super::deepseek_tools::parse_inline_tool_calls(&content)
                {
                    tracing::warn!(
                        "[provider] распознаны inline tool-calls в стрим-ответе (DSML): {} шт.",
                        parsed.len()
                    );
                    tool_calls = parsed;
                    content = cleaned;
                }
            }

            return Ok(LlmResponse {
                content,
                tool_calls,
                tokens_used: usage.total,
                prompt_tokens: usage.prompt,
                completion_tokens: usage.completion,
                cached_prompt_tokens: usage.cached_prompt,
                model,
                finish_reason,
                confidence: None,
            });
        }
    }

    async fn test_connection(&self) -> Result<(), LlmError> {
        let messages = vec![ChatMessage::user("Hello")];
        self.chat_completion(&messages).await?;
        Ok(())
    }

    fn provider_name(&self) -> &str {
        self.provider_name
    }
}

impl OpenAiProvider {
    /// Проверяет, поддерживает ли модель расширенные параметры (temperature, logprobs, max_tokens)
    ///
    /// GPT-5 и o1/o3 модели имеют ограниченный API:
    /// - Не поддерживают кастомный temperature (только дефолт 1.0)
    /// - Не поддерживают logprobs для расчета confidence
    /// - Не поддерживают max_completion_tokens
    fn supports_advanced_params(model_id: &str) -> bool {
        let normalized_model_id = model_id.rsplit('/').next().unwrap_or(model_id);
        let is_restricted = normalized_model_id.starts_with("gpt-5")
            || is_reasoning_model_family(normalized_model_id, "o1")
            || is_reasoning_model_family(normalized_model_id, "o3")
            || is_reasoning_model_family(normalized_model_id, "o4");

        !is_restricted
    }

    /// Проверяет, является ли модель подходящей для chat completion
    fn is_chat_model(model_id: &str) -> bool {
        // Включаем chat-модели
        let is_chat = model_id.starts_with("gpt-5")
            || model_id.starts_with("gpt-4")
            || model_id.starts_with("gpt-3.5")
            || model_id.starts_with("o1-")
            || model_id.starts_with("o3-")
            || model_id.starts_with("chatgpt-")
            || model_id.starts_with("deepseek"); // deepseek-chat / deepseek-reasoner

        // Исключаем специализированные модели
        let is_excluded = model_id.starts_with("text-embedding-")
            || model_id.starts_with("whisper-")
            || model_id.starts_with("tts-")
            || model_id.starts_with("dall-e-")
            || model_id.starts_with("text-moderation-")
            || model_id.starts_with("text-davinci-")
            || model_id.starts_with("text-curie-")
            || model_id.starts_with("text-babbage-")
            || model_id.starts_with("text-ada-")
            || model_id.starts_with("davinci-")
            || model_id.starts_with("curie-")
            || model_id.starts_with("babbage-")
            || model_id.starts_with("ada-")
            || model_id.contains("embedding")
            || model_id.contains("search")
            || model_id.contains("similarity")
            || model_id.contains("edit")
            || model_id.contains("insert")
            || model_id.contains(":ft-"); // fine-tuned модели

        is_chat && !is_excluded
    }

    /// Получить список доступных моделей для chat completion от OpenAI
    pub async fn list_models(&self) -> Result<Vec<serde_json::Value>, LlmError> {
        let response = self
            .client
            .models()
            .list()
            .await
            .map_err(|e| LlmError::ApiError(e.to_string()))?;

        let models: Vec<serde_json::Value> = response
            .data
            .into_iter()
            .filter(|m| Self::is_chat_model(&m.id))
            .map(|m| {
                serde_json::json!({
                    "id": m.id,
                    "created": m.created,
                    "owned_by": m.owned_by
                })
            })
            .collect();

        Ok(models)
    }
}

/// Сопоставить текст ошибки апстрима с типом [`LlmError`].
fn classify_api_error(err_str: &str) -> LlmError {
    let lower = err_str.to_lowercase();
    if err_str.contains("401") || err_str.contains("403") || lower.contains("authentication") {
        LlmError::AuthError(err_str.to_string())
    } else if err_str.contains("429") || lower.contains("rate limit") {
        LlmError::RateLimitExceeded
    } else if lower.contains("decoding response body") || lower.contains("error decoding") {
        // Тело ответа не распарсилось даже после повторов: провайдер вернул неполный/
        // некорректный ответ (частый временный сбой шлюза OpenRouter или несовместимый
        // формат ответа модели). Даём понятную подсказку вместо «error decoding response body».
        LlmError::ApiError(format!(
            "провайдер вернул неполный/некорректный ответ (после повторов). \
             Обычно это временный сбой — повторите запрос; если повторяется на этой модели, \
             смените модель/агента. Исходно: {err_str}"
        ))
    } else {
        LlmError::ApiError(err_str.to_string())
    }
}

/// Является ли ошибка временной (есть смысл повторить запрос).
fn is_transient_error(err_str: &str) -> bool {
    let s = err_str.to_lowercase();
    s.contains("error sending request")
        || s.contains("connection")
        || s.contains("connect error")
        || s.contains("timed out")
        || s.contains("timeout")
        || s.contains("reset")
        || s.contains("broken pipe")
        || s.contains("dns")
        || s.contains("429")
        || s.contains("rate limit")
        || s.contains("502")
        || s.contains("503")
        || s.contains("504")
        || s.contains("bad gateway")
        || s.contains("service unavailable")
        || s.contains("gateway timeout")
        // Неполный/непарсящийся ответ провайдера: оборванное тело, HTML-страница ошибки
        // от шлюза (Cloudflare 5xx), усечённый поток. У OpenRouter это, как правило, временно.
        || s.contains("error decoding response body")
        || s.contains("decoding response body")
        || s.contains("unexpected end of file")
        || s.contains("unexpected eof")
        || s.contains("incomplete")
}

fn normalize_api_error_message(err: &str) -> String {
    let Some((prefix, content)) = err.split_once(" content:") else {
        return err.to_string();
    };
    let Ok(payload) = serde_json::from_str::<serde_json::Value>(content.trim()) else {
        return err.to_string();
    };

    if let Some(raw_message) = payload
        .pointer("/error/metadata/raw")
        .and_then(|raw| raw.as_str())
        .and_then(extract_nested_error_message)
    {
        return format!("{}: {}", prefix.trim(), raw_message);
    }

    if let Some(message) = payload.pointer("/error/message").and_then(|m| m.as_str()) {
        return format!("{}: {}", prefix.trim(), message);
    }

    err.to_string()
}

fn extract_nested_error_message(raw: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(raw)
        .ok()
        .and_then(|payload| {
            payload
                .pointer("/error/message")
                .and_then(|message| message.as_str())
                .map(ToString::to_string)
        })
}

fn is_reasoning_model_family(model_id: &str, family: &str) -> bool {
    model_id == family || model_id.starts_with(&format!("{family}-"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_body_error_is_transient_and_retried() {
        // Сообщение reqwest для оборванного/непарсящегося тела (наблюдалось у OpenRouter).
        assert!(is_transient_error(
            "http error: error decoding response body"
        ));
        assert!(is_transient_error(
            "error decoding response body: unexpected EOF"
        ));
        // Регрессия: обычные сетевые/шлюзовые ошибки тоже остаются временными.
        assert!(is_transient_error("502 Bad Gateway"));
        assert!(is_transient_error("connection reset by peer"));
        // А вот доменные ошибки повторять не нужно.
        assert!(!is_transient_error("400 invalid request: bad model"));
    }

    #[test]
    fn decode_error_gets_friendly_message() {
        let err = classify_api_error("http error: error decoding response body");
        match err {
            LlmError::ApiError(msg) => {
                assert!(
                    msg.contains("неполный"),
                    "friendly hint expected, got: {msg}"
                );
            }
            other => panic!("expected ApiError, got {other:?}"),
        }
    }
}
