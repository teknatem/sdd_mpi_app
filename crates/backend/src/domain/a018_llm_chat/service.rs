use super::job_store;
use super::repository;
// Чат работает через «виртуального сотрудника» a017 (персона: специализация + обязанности),
// который ссылается на техническое подключение a038 (провайдер/креды/тюнинг). Связь хранится
// как UUID (chat.agent_id) → сотрудник a017. Существующие чаты валидны: у a017 и a038 совпадают id.
use crate::domain::a017_llm_agent::repository as employee_repository;
use crate::domain::a038_llm_connection::repository as connection_repository;
use crate::shared::llm::cost::{cost_micro, Pricing, UsageTotals};
use crate::shared::llm::tool_guards::{GuardCapabilities, GuardDecision, GuardState};
use crate::shared::llm::types::{ChatMessage, ChatRole as LlmChatRole, ToolCaller};
use crate::shared::llm::{create_provider, execute_tool_call};
use crate::system::s3::service::{self as s3_service, UploadedFile};
use axum::extract::Multipart;
use base64::Engine;
use bytes::Bytes;
use contracts::domain::a017_llm_agent::aggregate::LlmAgentId;
use contracts::domain::a018_llm_chat::aggregate::{
    ChatRole, LlmChat, LlmChatAttachment, LlmChatAttachmentSummary, LlmChatDetail, LlmChatId,
    LlmChatListItem, LlmChatMessage,
};
use contracts::domain::a018_llm_chat::context::ContextPackageSummary;
use contracts::domain::a019_llm_artifact::aggregate::LlmArtifactId;
use contracts::domain::a038_llm_connection::aggregate::{
    AgentType, LlmConnection, LlmProviderType,
};
use contracts::domain::common::AggregateId;
use contracts::system::s3::S3FileCategory;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use uuid::Uuid;

/// Обрезать строку до `max_bytes` байт, не разрывая UTF-8 символы.
fn utf8_truncate(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    let mut boundary = max_bytes;
    while boundary > 0 && !s.is_char_boundary(boundary) {
        boundary -= 1;
    }
    &s[..boundary]
}

/// Максимальное число итераций tool calling в одном запросе.
/// При исчерпании лимит обрабатывается мягко: делается финальный запрос без
/// инструментов, чтобы модель подытожила проделанную работу (см. ниже).
// Аналитические навыки нередко требуют: активация → resource → schema → несколько SQL →
// детерминированный task → итог. Десять итераций обрывали WB-воронку сразу после последнего
// успешного SQL, не оставляя модели хода для текстового ответа.
// Рабочий каталог чата (план + журнал шагов) удерживает нить на длинных задачах, но требует
// собственных ходов: завести активность, заполнить анкету, сохранить шаг, отметить пункт плана.
// На четырнадцати итерациях многошаговая сверка обрывалась до текстового ответа.
const MAX_TOOL_ITERATIONS: usize = 40;
const MAX_TOOL_FAILURES: usize = 10;

/// Системные промпты вынесены в реестр навыков (см. shared/llm/skills.rs):
/// базовый core-промпт + промпт-фрагменты активных навыков.

/// Токен-бюджет истории диалога (грубая оценка ~3 символа на токен для RU/EN-микса).
/// При превышении старая часть истории СУММИРУЕТСЯ (компакция) и заменяется сводкой,
/// которая сохраняется в чате (a018_llm_chat.summary_text/summary_upto).
/// Инструменты, чей `artifact_id` показывается пользователю карточкой под ответом.
/// Всё остальное, что возвращает `artifact_id` (напр. `execute_query`), — служебные
/// записи для воспроизводимости, а не результат для чтения.
const USER_FACING_ARTIFACT_TOOLS: &[&str] = &[
    "create_drilldown_report",
    "build_chart",
    "build_table",
    "plugin_upsert",
];

/// Бюджет истории и размер живого хвоста — производные от окна контекста подключения (a038).
///
/// Раньше это были константы 120_000/60_000, верные для облачных моделей и разрушительные
/// для локальных: у Ollama с `num_ctx = 32k` история просто не влезает, и сервер молча
/// обрезает промпт. Коэффициент 3/4 оставляет место под системный промпт, схемы инструментов
/// и ответ, а также страхует неточность `estimate_tokens` на чужих токенизаторах.
///
/// Дефолт колонки `context_window` = 160_000 даёт ровно прежние 120_000/60_000.
fn history_budget(context_window: i32) -> (usize, usize) {
    let window = if context_window <= 0 {
        contracts::domain::a038_llm_connection::aggregate::default_context_window()
    } else {
        context_window
    } as usize;
    let budget = window * 3 / 4;
    (budget, budget / 2)
}

/// Грубая оценка числа токенов в строке (~3 символа/токен).
fn estimate_tokens(s: &str) -> usize {
    s.chars().count() / 3 + 1
}

/// Оценка «веса» сообщения в контексте: контент + tool trace (он дописывается
/// к assistant-сообщениям при сборке контекста, cap 12KB — см. MAX_HISTORY_TRACE_BYTES).
fn message_tokens(m: &LlmChatMessage) -> usize {
    estimate_tokens(&m.content)
        + m.tool_trace
            .as_deref()
            .map(|t| estimate_tokens(t).min(4_000))
            .unwrap_or(0)
}

/// Индекс разреза для компакции: history[..idx] уходит в сводку, history[idx..]
/// остаётся живым хвостом (≈ `keep_recent` по оценке; system-сообщения не
/// считаются — они сохраняются отдельно). Последнее сообщение (текущий вопрос
/// пользователя) не компактится никогда. 0 — резать нечего.
fn compaction_cut_index(
    history: &[LlmChatMessage],
    total_tokens: usize,
    keep_recent: usize,
) -> usize {
    if history.len() < 2 {
        return 0;
    }
    let mut running = 0usize;
    for (i, m) in history.iter().enumerate().take(history.len() - 1) {
        if m.role == ChatRole::System {
            continue;
        }
        running += message_tokens(m);
        if total_tokens - running <= keep_recent {
            return i + 1;
        }
    }
    // Даже один последний хвост больше KEEP — компактим всё, кроме текущего сообщения.
    history.len() - 1
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmChatDto {
    pub id: Option<String>,
    pub code: Option<String>,
    pub description: String,
    pub comment: Option<String>,
    pub agent_id: String,
    pub model_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendMessageRequest {
    pub content: String,
    pub model_name: Option<String>,
    #[serde(default)]
    pub attachment_ids: Vec<String>,
    /// Client-generated idempotency key for background execution retries.
    #[serde(default)]
    pub request_id: Option<String>,
}

fn ensure_model_is_allowed(model: &str, allowed_models: &[String]) -> anyhow::Result<()> {
    let model = model.trim();
    if model.is_empty() {
        anyhow::bail!("Модель LLM не выбрана");
    }
    // Пустой allowed_models означает, что подключение не ограничивает список моделей.
    if allowed_models.is_empty() || allowed_models.iter().any(|allowed| allowed == model) {
        return Ok(());
    }
    anyhow::bail!(
        "Модель '{}' недоступна сотруднику. Доступные модели: {}. Выберите другую модель в заголовке чата.",
        model,
        allowed_models.join(", ")
    )
}

fn ensure_connection_model_is_allowed(
    connection: &LlmConnection,
    model: &str,
) -> anyhow::Result<()> {
    ensure_model_is_allowed(model, &connection.allowed_models_list())
}

/// «Эффективный агент» чата: персона (специализация + обязанности) из сотрудника a017 +
/// техническое подключение a038, из которого берётся провайдер/креды/дефолт-модель.
pub struct EffectiveAgent {
    /// Техническое подключение — для `create_provider` и дефолтной модели.
    pub connection: LlmConnection,
    /// Специализация сотрудника (определяет навыки/инструменты).
    pub agent_type: AgentType,
    /// Должностные обязанности / системный промпт сотрудника.
    pub system_prompt: Option<String>,
    /// Отображаемое имя (сотрудник; для legacy — подключение).
    pub display_name: String,
    /// Закреплённая сотрудником модель (пусто → None).
    pub pinned_model: Option<String>,
}

/// Нужен ли отдельный LLM-вызов классификатора интента для этого подключения.
///
/// Для локальной модели — нет: классификатор занимает ту же карту, что и основной цикл,
/// поэтому «конкурентный и потому бесплатный» вызов становится серийным удвоением времени
/// ответа. Метку интента в сообщение всё равно пишет правиловый `quick_result`, так что
/// теряется только диагностика расхождения.
fn llm_router_enabled(connection: &LlmConnection) -> bool {
    connection.provider_type != LlmProviderType::Ollama
}

/// Разрешить техническое подключение сотрудника: явная привязка → близнец с тем же UUID
/// (миграция 0165) → основное подключение.
async fn resolve_connection(
    conn_id: Option<&str>,
    fallback_id: &str,
) -> anyhow::Result<LlmConnection> {
    if let Some(cid) = conn_id.filter(|s| !s.trim().is_empty()) {
        if let Some(c) = connection_repository::find_by_id(cid).await? {
            return Ok(c);
        }
    }
    if let Some(c) = connection_repository::find_by_id(fallback_id).await? {
        return Ok(c);
    }
    if let Some(c) = connection_repository::find_primary().await? {
        return Ok(c);
    }
    Err(anyhow::anyhow!(
        "У сотрудника нет доступного технического подключения LLM (a038)"
    ))
}

/// Собрать «эффективного агента» по `chat.agent_id`: сначала как сотрудника a017 (персона),
/// с техникой из его подключения a038. Fallback — legacy-чаты, указывающие прямо на a038
/// без a017-близнеца (напр. авто-подключения старого почтового конвейера): персона дефолтная.
async fn resolve_effective_agent(agent_id: &str) -> anyhow::Result<EffectiveAgent> {
    if let Some(emp) = employee_repository::find_by_id(agent_id).await? {
        let connection = resolve_connection(emp.connection_id.as_deref(), agent_id).await?;
        let pinned_model = Some(emp.model_name.trim().to_string()).filter(|s| !s.is_empty());
        return Ok(EffectiveAgent {
            connection,
            agent_type: emp.agent_type,
            system_prompt: emp.system_prompt,
            display_name: emp.base.description,
            pinned_model,
        });
    }
    if let Some(connection) = connection_repository::find_by_id(agent_id).await? {
        let display_name = connection.base.description.clone();
        return Ok(EffectiveAgent {
            connection,
            agent_type: AgentType::default(),
            system_prompt: None,
            display_name,
            pinned_model: None,
        });
    }
    Err(anyhow::anyhow!("Agent not found: {}", agent_id))
}

/// Создание нового чата. `owner_user_id` — текущий пользователь (владелец чата);
/// `None` для системных чатов (планировщик/автоматика), которые не привязаны к пользователю.
pub async fn create(dto: LlmChatDto, owner_user_id: Option<String>) -> anyhow::Result<Uuid> {
    let code = dto
        .code
        .clone()
        .unwrap_or_else(|| format!("CHAT-{}", Uuid::new_v4()));

    let agent_uuid =
        Uuid::parse_str(&dto.agent_id).map_err(|e| anyhow::anyhow!("Invalid agent_id: {}", e))?;
    let agent_id = LlmAgentId::new(agent_uuid);

    // Проверяем что сотрудник существует и резолвим его подключение для дефолт-модели.
    let effective = resolve_effective_agent(&agent_id.as_string()).await?;

    let db = crate::shared::data::db::get_connection();

    // Модель: из DTO → закреплённая у сотрудника → дефолтная у подключения.
    let model_name = dto
        .model_name
        .filter(|s| !s.trim().is_empty())
        .or_else(|| effective.pinned_model.clone())
        .unwrap_or_else(|| effective.connection.model_name.clone());

    ensure_connection_model_is_allowed(&effective.connection, &model_name)?;

    let mut aggregate =
        LlmChat::new_for_insert(code, dto.description, agent_id, model_name, owner_user_id);

    // Валидация
    aggregate
        .validate()
        .map_err(|e| anyhow::anyhow!("Validation failed: {}", e))?;

    // Before write
    aggregate.before_write();

    let id = aggregate.base.id.0;

    // Сохранение через repository
    repository::insert(&db, &aggregate).await?;

    Ok(id)
}

/// Обновление чата
pub async fn update(dto: LlmChatDto) -> anyhow::Result<()> {
    let id_str = dto
        .id
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("ID is required"))?;

    let chat_uuid =
        Uuid::parse_str(id_str).map_err(|e| anyhow::anyhow!("Invalid chat ID: {}", e))?;
    let chat_id = LlmChatId::new(chat_uuid);

    let db = crate::shared::data::db::get_connection();
    let mut aggregate = repository::find_by_id(&db, &chat_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Chat not found"))?;

    // Обновление полей
    if let Some(code) = dto.code {
        aggregate.base.code = code;
    }
    aggregate.base.description = dto.description;
    aggregate.base.comment = dto.comment;

    if let Some(new_agent_id_str) = Some(dto.agent_id) {
        let agent_uuid = Uuid::parse_str(&new_agent_id_str)
            .map_err(|e| anyhow::anyhow!("Invalid agent_id: {}", e))?;
        aggregate.agent_id = LlmAgentId::new(agent_uuid);
    }

    if let Some(model_name) = dto.model_name {
        aggregate.model_name = model_name;
    }

    let effective = resolve_effective_agent(&aggregate.agent_id.as_string()).await?;
    ensure_connection_model_is_allowed(&effective.connection, &aggregate.model_name)?;

    // Валидация
    aggregate
        .validate()
        .map_err(|e| anyhow::anyhow!("Validation failed: {}", e))?;

    // Before write
    aggregate.before_write();

    // Сохранение
    repository::update(&db, &aggregate).await?;

    Ok(())
}

/// Изменить модель существующего чата, проверив её по текущему подключению сотрудника.
pub async fn set_model(id: &str, model_name: String) -> anyhow::Result<()> {
    let chat_uuid = Uuid::parse_str(id).map_err(|e| anyhow::anyhow!("Invalid chat ID: {}", e))?;
    let chat_id = LlmChatId::new(chat_uuid);
    let db = crate::shared::data::db::get_connection();
    let mut chat = repository::find_by_id(&db, &chat_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Chat not found"))?;

    let effective = resolve_effective_agent(&chat.agent_id.as_string()).await?;
    let model_name = model_name.trim().to_string();
    ensure_connection_model_is_allowed(&effective.connection, &model_name)?;

    chat.model_name = model_name;
    chat.before_write();
    repository::update(&db, &chat).await?;
    Ok(())
}

/// Установить пользовательскую оценку чата (1..5; None — снять оценку).
pub async fn set_rating(id: &str, rating: Option<i32>) -> anyhow::Result<()> {
    if let Some(r) = rating {
        if !(1..=5).contains(&r) {
            return Err(anyhow::anyhow!("Rating must be between 1 and 5"));
        }
    }

    let chat_uuid = Uuid::parse_str(id).map_err(|e| anyhow::anyhow!("Invalid chat ID: {}", e))?;
    let chat_id = LlmChatId::new(chat_uuid);

    let db = crate::shared::data::db::get_connection();
    let mut aggregate = repository::find_by_id(&db, &chat_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Chat not found"))?;

    aggregate.rating = rating;
    aggregate.before_write();
    repository::update(&db, &aggregate).await?;

    Ok(())
}

/// Удаление чата (soft delete)
pub async fn delete(id: &str) -> anyhow::Result<()> {
    let chat_uuid = Uuid::parse_str(id).map_err(|e| anyhow::anyhow!("Invalid chat ID: {}", e))?;
    let chat_id = LlmChatId::new(chat_uuid);

    let db = crate::shared::data::db::get_connection();
    repository::soft_delete(&db, &chat_id).await?;

    Ok(())
}

/// Получить чат по ID с именем агента
pub async fn get_by_id(id: &str) -> anyhow::Result<Option<LlmChatDetail>> {
    let chat_uuid = Uuid::parse_str(id).map_err(|e| anyhow::anyhow!("Invalid chat ID: {}", e))?;
    let chat_id = LlmChatId::new(chat_uuid);

    let db = crate::shared::data::db::get_connection();
    let chat = match repository::find_by_id(&db, &chat_id).await? {
        Some(c) => c,
        None => return Ok(None),
    };

    let agent_name = resolve_effective_agent(&chat.agent_id.as_string())
        .await
        .ok()
        .map(|e| e.display_name);

    Ok(Some(LlmChatDetail { chat, agent_name }))
}

/// Получить список всех чатов
pub async fn list_all() -> anyhow::Result<Vec<LlmChat>> {
    let db = crate::shared::data::db::get_connection();
    let chats = repository::list_all(&db).await?;
    Ok(chats)
}

/// Получить список чатов с пагинацией
pub async fn list_paginated(page: u64, page_size: u64) -> anyhow::Result<(Vec<LlmChat>, u64)> {
    let db = crate::shared::data::db::get_connection();
    let (chats, total) = repository::list_paginated(&db, page, page_size).await?;
    Ok((chats, total))
}

/// Получить список чатов с подсчетом сообщений и временем последнего сообщения.
/// Фильтрация по доступу: `is_admin` видит все, обычный пользователь — свои + общие.
pub async fn list_with_stats(
    viewer_id: &str,
    is_admin: bool,
) -> anyhow::Result<Vec<LlmChatListItem>> {
    let db = crate::shared::data::db::get_connection();
    let chats = repository::list_with_stats(&db, viewer_id, is_admin).await?;
    Ok(chats)
}

/// Установить признак «Общий доступ» у чата.
/// Менять статус может только владелец чата или superadmin (`is_admin`).
pub async fn set_shared(
    id: &str,
    is_shared: bool,
    requester_id: &str,
    is_admin: bool,
) -> anyhow::Result<()> {
    let chat_uuid = Uuid::parse_str(id).map_err(|e| anyhow::anyhow!("Invalid chat ID: {}", e))?;
    let chat_id = LlmChatId::new(chat_uuid);

    let db = crate::shared::data::db::get_connection();
    let mut aggregate = repository::find_by_id(&db, &chat_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Chat not found"))?;

    // Ownership-guard: только владелец или админ.
    if !is_admin && aggregate.owner_user_id.as_deref() != Some(requester_id) {
        return Err(anyhow::anyhow!("Forbidden: not the owner of this chat"));
    }

    aggregate.is_shared = is_shared;
    aggregate.before_write();
    repository::update(&db, &aggregate).await?;

    Ok(())
}

/// Получить все сообщения чата
pub async fn get_messages(chat_id: &str) -> anyhow::Result<Vec<LlmChatMessage>> {
    let chat_uuid =
        Uuid::parse_str(chat_id).map_err(|e| anyhow::anyhow!("Invalid chat ID: {}", e))?;
    let chat_id_obj = LlmChatId::new(chat_uuid);

    let db = crate::shared::data::db::get_connection();
    let mut messages = repository::find_messages_by_chat_id(&db, &chat_id_obj).await?;
    for message in &mut messages {
        message.attachments = repository::find_attachments_by_message_id(&db, &message.id)
            .await?
            .iter()
            .map(LlmChatAttachmentSummary::from)
            .collect();
    }

    Ok(messages)
}

pub async fn ensure_chat_access(
    chat_id: &str,
    user_id: &str,
    is_admin: bool,
) -> anyhow::Result<LlmChat> {
    let id = LlmChatId::new(Uuid::parse_str(chat_id)?);
    let db = crate::shared::data::db::get_connection();
    let chat = repository::find_by_id(&db, &id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Chat not found"))?;
    if is_admin || chat.owner_user_id.as_deref() == Some(user_id) || chat.is_shared {
        Ok(chat)
    } else {
        anyhow::bail!("Forbidden")
    }
}

/// Полный журнал вызовов инструментов для сообщения ассистента (sys_tool_trace).
pub async fn get_tool_trace(
    message_id: &str,
) -> anyhow::Result<Vec<contracts::domain::a018_llm_chat::aggregate::ToolTraceEntry>> {
    let db = crate::shared::data::db::get_connection();
    let entries = repository::find_tool_trace_by_message(&db, message_id).await?;
    Ok(entries)
}

/// Инструменты, повтор которых с теми же аргументами в рамках одного ответа гарантированно
/// даёт тот же результат: только чтение, без записи и без побочных эффектов.
/// Метаданные (`get_entity_schema`, `list_entities`, …) кэшируются глубже — в `tool_executor`.
fn is_memoizable_tool(name: &str) -> bool {
    matches!(
        name,
        "query_data_schema" | "run_data_view_scalar" | "run_data_view_drilldown" | "execute_query"
    )
}

/// Человекочитаемая подпись инструмента для индикатора прогресса (показывается
/// пользователю, не модели). Неизвестные имена показываем как есть.
fn human_tool_label(name: &str) -> String {
    let label = match name {
        "find_data_sources" => "Поиск источника данных",
        "preview_data" => "Проверка данных",
        "build_chart" => "Создание графика",
        "build_table" => "Создание таблицы",
        "list_data_sources" => "Просмотр источников данных",
        "query_data_schema" => "Запрос данных по схеме",
        "run_data_view_scalar" | "run_data_view_drilldown" => "Расчёт по DataView",
        "execute_query" => "SQL-запрос",
        "list_entities" => "Просмотр каталога объектов",
        "get_entity_schema" => "Чтение схемы объекта",
        "get_join_hint" => "Поиск связи таблиц",
        "get_architecture_overview" => "Обзор архитектуры",
        "search_knowledge" => "Поиск в базе знаний",
        "get_knowledge" => "Чтение базы знаний",
        "kb_propose_article" => "Черновик статьи в базу знаний",
        "kb_report_issue" => "Замечание к статье базы знаний",
        "list_kb_vocabulary" => "Словарь тегов базы знаний",
        "chart_template" | "chart_examples" | "get_chart_ui_contract" => "Подготовка графика",
        "plugin_validate" => "Проверка плагина",
        "plugin_smoke_test" => "Проверка графика/таблицы",
        "plugin_upsert" => "Сохранение плагина",
        "plugin_invoke" => "Запуск плагина",
        "plugin_runs" => "История запусков плагина",
        "list_skills" | "use_skill" => "Подбор навыка",
        "check_system_health" => "Проверка состояния системы",
        "get_performance_stats" => "Метрики производительности",
        "list_background_jobs" => "Фоновые задачи",
        "get_data_integrity_report" => "Проверка целостности данных",
        "list_scheduled_tasks" => "Просмотр регламентных заданий",
        "describe_task_types" => "Справка по типам заданий",
        "find_page_help" => "Поиск инструкции по разделу",
        "get_user_recent_pages" => "Просмотр истории страниц",
        "ticket_search" => "Поиск похожих обращений",
        "ticket_validate" => "Проверка черновика тикета",
        "ticket_create" => "Оформление тикета",
        other => return format!("Инструмент: {other}"),
    };
    label.to_string()
}

/// Стабильный этап workflow для диагностики и UI. Имя инструмента остается техническим
/// идентификатором, а stage описывает его место в конвейере.
fn tool_stage(name: &str) -> &'static str {
    match name {
        "find_data_sources" => "discovery",
        "preview_data" => "preview",
        "build_chart" | "build_table" | "kb_propose_article" => "publish",
        _ => "tool",
    }
}

fn bounded_trace_value(value: serde_json::Value, max_bytes: usize) -> serde_json::Value {
    let encoded = serde_json::to_string(&value).unwrap_or_default();
    if encoded.len() <= max_bytes {
        value
    } else {
        serde_json::json!({
            "truncated": true,
            "preview": utf8_truncate(&encoded, max_bytes),
        })
    }
}

/// Вход этапа сохраняется отдельно от выхода. Это делает trace пригодным не только
/// для таймингов, но и для проверки фактического контракта между этапами.
fn trace_input(arguments: &str) -> serde_json::Value {
    let value = serde_json::from_str(arguments)
        .unwrap_or_else(|_| serde_json::json!({ "raw": utf8_truncate(arguments, 2_000) }));
    bounded_trace_value(value, 6_000)
}

/// Выход trace намеренно компактный: строки preview могут занимать сотни килобайт,
/// а для диагностики контракта нужны колонки, готовность, IDs и структурированная ошибка.
fn trace_output(value: Option<&serde_json::Value>) -> serde_json::Value {
    let Some(value) = value else {
        return serde_json::json!({ "error": "Tool returned non-JSON output" });
    };
    let mut out = serde_json::Map::new();
    for key in [
        "ok",
        "stage",
        "error_code",
        "error",
        "recommended_fix",
        "row_count",
        "total",
        "build_ready",
        "truncated",
        "source",
        "columns",
        "effective_context",
        "next_step",
        "artifact_id",
        "plugin_id",
        "revision_id",
        "session_id",
        "status",
        "data_modes",
        "presentation",
        "_activate_skill",
        "_skill_access_level",
        "_skill_inefficiency",
        "_skill_digest",
        "_registry_generation",
        "task_id",
        "mode",
        "digest",
        "registry_generation",
        "activated",
        // Тикеты: `complete` + `payload_hash` — опора гейта ticket_create, они ОБЯЗАНЫ
        // переживать усечение трассы, иначе создание тикета перестанет проходить.
        "complete",
        "payload_hash",
        "missing",
        "ticket_id",
        "ticket_code",
        // База знаний: без этих ключей трасса KB-инструментов схлопывается до {ok}
        // и разобрать плохую выдачу поиска становится нечем.
        "returned",
        "total_matched",
        "kb_edit_id",
        "issue_id",
        "open_issues",
        "unknown_tags",
        "warnings",
        "path",
    ] {
        if let Some(item) = value.get(key) {
            out.insert(key.to_string(), item.clone());
        }
    }
    if let Some(sources) = value.get("sources").and_then(serde_json::Value::as_array) {
        let compact = sources
            .iter()
            .take(5)
            .map(|source| {
                serde_json::json!({
                    "id": source.get("id"),
                    "kind": source.get("kind"),
                    "name": source.get("name"),
                    "table": source.get("table"),
                    "source_template": source.get("source_template"),
                })
            })
            .collect::<Vec<_>>();
        out.insert("sources".to_string(), serde_json::Value::Array(compact));
    }
    if value.get("task_id").is_some() {
        if let Some(result) = value.get("result") {
            out.insert(
                "result".to_string(),
                bounded_trace_value(result.clone(), 3_000),
            );
        }
    }
    if out.is_empty() {
        out.insert("result".to_string(), value.clone());
    }
    bounded_trace_value(serde_json::Value::Object(out), 8_000)
}

/// Записать текущий этап выполнения в job_store (если задача отслеживается).
async fn report_progress(job_id: Option<&str>, step: u32, stage: impl Into<String>) {
    if let Some(id) = job_id {
        job_store::set_progress(id, job_store::JobProgress::new(step, stage)).await;
    }
}

/// Отправить сообщение пользователя и получить ответ от LLM.
/// `job_id` — идентификатор фоновой задачи для отчёта о прогрессе (None для
/// синхронных вызовов из фоновых тасков, где прогресс не нужен).
/// `actor` — пользователь, от чьего имени идёт диалог. `None` у фоновых сценариев
/// (планировщик, почтовый конвейер): у них нет человека-собеседника, и инструменты,
/// действующие от лица пользователя, для них закрыты.
pub async fn send_message(
    chat_id: &str,
    request: SendMessageRequest,
    job_id: Option<&str>,
    actor: Option<ToolCaller>,
) -> anyhow::Result<LlmChatMessage> {
    let chat_uuid =
        Uuid::parse_str(chat_id).map_err(|e| anyhow::anyhow!("Invalid chat ID: {}", e))?;
    let chat_id_obj = LlmChatId::new(chat_uuid);

    let db = crate::shared::data::db::get_connection();

    // 1. Получить чат
    let chat = repository::find_by_id(&db, &chat_id_obj)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Chat not found"))?;

    // 2. Резолвим «эффективного агента»: персона (a017) + техническое подключение (a038).
    let effective = resolve_effective_agent(&chat.agent_id.as_string()).await?;

    // Выбор модели: из запроса -> из чата -> закреплённая у сотрудника -> дефолт подключения.
    let model_to_use = request
        .model_name
        .clone()
        .filter(|s| !s.trim().is_empty())
        .or_else(|| Some(chat.model_name.clone()).filter(|s| !s.trim().is_empty()))
        .or_else(|| effective.pinned_model.clone())
        .unwrap_or_else(|| effective.connection.model_name.clone());

    // Не даём модели старого чата или клиентскому override уйти в API другого провайдера.
    ensure_connection_model_is_allowed(&effective.connection, &model_to_use)?;

    // Провайдер создаётся сразу: нужен и для компакции истории (ниже), и для цикла.
    // Технику берём из подключения a038.
    let provider = create_provider(&effective.connection, Some(&model_to_use))
        .map_err(|e| anyhow::anyhow!("LLM provider error: {}", e))?;

    // 3. Обработать вложения если есть
    let mut content_with_attachments = request.content.clone();
    let mut has_image_attachment = false;

    if !request.attachment_ids.is_empty() {
        let mut attachment_contents = Vec::new();

        for att_id_str in &request.attachment_ids {
            let att_uuid = Uuid::parse_str(att_id_str)
                .map_err(|e| anyhow::anyhow!("Invalid attachment ID: {}", e))?;

            // Найти вложение по его id (а не сканировать все nil-вложения глобально)
            if let Some(attachment) = repository::find_attachment_by_id(&db, &att_uuid).await? {
                if attachment.chat_id != chat_id_obj {
                    anyhow::bail!("Attachment does not belong to this chat");
                }
                if attachment.message_id.is_some() {
                    anyhow::bail!("Attachment has already been sent");
                }
                if is_image_content_type(&attachment.content_type) {
                    has_image_attachment = true;
                    continue;
                }
                // Прочитать содержимое файла
                match read_attachment_text(&attachment).await {
                    Ok(file_content) => {
                        attachment_contents
                            .push(format!("--- {} ---\n{}", attachment.filename, file_content));
                    }
                    Err(e) => {
                        tracing::warn!("Failed to read attachment {}: {}", attachment.filename, e);
                    }
                }
            } else {
                anyhow::bail!("Attachment not found");
            }
        }

        if !attachment_contents.is_empty() {
            content_with_attachments.push_str("\n\nПрикрепленные файлы:\n");
            content_with_attachments.push_str(&attachment_contents.join("\n\n"));
        }
    }

    if has_image_attachment && !effective.connection.supports_image_input(&model_to_use) {
        anyhow::bail!(
            "Модель '{}' не отмечена как поддерживающая изображения. Выберите vision-модель.",
            model_to_use
        );
    }

    // 4. Сохранить сообщение пользователя
    let user_msg = LlmChatMessage::user(chat_id_obj, request.content);
    repository::insert_message(&db, &user_msg).await?;

    // 5. Привязать вложения к сохранённому сообщению пользователя точечным UPDATE
    // по id вложения (без удаления чужих незавершённых загрузок).
    for att_id_str in &request.attachment_ids {
        if let Ok(att_uuid) = Uuid::parse_str(att_id_str) {
            if let Err(e) =
                repository::bind_attachment_to_message(&db, &att_uuid, &user_msg.id).await
            {
                tracing::warn!("Failed to bind attachment {}: {}", att_id_str, e);
            }
        }
    }

    // 6. Получить историю сообщений для контекста
    let mut history = repository::find_messages_by_chat_id(&db, &chat_id_obj).await?;

    // === Управление контекстом: персистентная сводка + токен-бюджет ===
    // 6.1 Ранее скомпактированная часть уже заменена сводкой — исключаем её из истории.
    let mut chat_summary: Option<String> = None;
    match repository::get_chat_summary(&db, &chat_id_obj).await {
        Ok(Some((text, upto))) => {
            if let Ok(upto_ts) = chrono::DateTime::parse_from_rfc3339(&upto) {
                let upto_utc = upto_ts.with_timezone(&chrono::Utc);
                history.retain(|m| m.role == ChatRole::System || m.created_at > upto_utc);
            }
            chat_summary = Some(text);
        }
        Ok(None) => {}
        Err(e) => tracing::warn!("get_chat_summary failed for chat {}: {}", chat_id, e),
    }

    // 6.2 Токен-бюджет: при превышении старая часть суммируется LLM-вызовом,
    // сводка сохраняется в чате, в контексте остаётся сводка + свежий хвост.
    let total_tokens: usize = history
        .iter()
        .filter(|m| m.role != ChatRole::System)
        .map(message_tokens)
        .sum();
    let (history_budget_tokens, keep_recent_tokens) =
        history_budget(effective.connection.context_window);
    if total_tokens > history_budget_tokens {
        report_progress(job_id, 0, "Сжимаю контекст диалога…").await;

        // Точка разреза: свежий хвост ≤ keep_recent_tokens остаётся живым.
        let cut = compaction_cut_index(&history, total_tokens, keep_recent_tokens);

        if cut > 0 {
            let recent = history.split_off(cut);
            let old_part = history;
            let old_system: Vec<_> = old_part
                .iter()
                .filter(|m| m.role == ChatRole::System)
                .cloned()
                .collect();
            let to_compact: Vec<_> = old_part
                .into_iter()
                .filter(|m| m.role != ChatRole::System)
                .collect();

            if let Some(last_old) = to_compact.last() {
                let upto_rfc = last_old.created_at.to_rfc3339();
                let mut payload = String::new();
                if let Some(prev) = &chat_summary {
                    payload.push_str("Предыдущая сводка (объедини её с новой информацией):\n");
                    payload.push_str(prev);
                    payload.push_str("\n\n");
                }
                payload.push_str("Сообщения для сжатия:\n");
                for m in &to_compact {
                    let role = match m.role {
                        ChatRole::User => "Пользователь",
                        ChatRole::Assistant => "Ассистент",
                        ChatRole::System => "Система",
                    };
                    payload.push_str(&format!(
                        "[{}] {}\n\n",
                        role,
                        utf8_truncate(&m.content, 4_000)
                    ));
                }
                let payload = utf8_truncate(&payload, 120_000).to_string();
                let sum_messages = vec![
                    ChatMessage::system(
                        "Ты сжимаешь историю диалога, чтобы ассистент продолжил работу с меньшим \
                         контекстом. Составь компактную сводку (до 400 слов): цель пользователя; \
                         ключевые факты, данные и принятые решения; созданные/изменённые объекты и \
                         артефакты с их id; незакрытые задачи и договорённости. Пиши фактами, без \
                         воды. Ответ — только текст сводки.",
                    ),
                    ChatMessage::user(payload),
                ];
                match provider.chat_completion(&sum_messages).await {
                    Ok(resp) if !resp.content.trim().is_empty() => {
                        let new_summary = resp.content.trim().to_string();
                        if let Err(e) =
                            repository::set_chat_summary(&db, &chat_id_obj, &new_summary, &upto_rfc)
                                .await
                        {
                            tracing::warn!("set_chat_summary failed for chat {}: {}", chat_id, e);
                        }
                        tracing::info!(
                            "[compaction] chat='{}' ctx_window={} budget={} compacted {} msgs (~{} tok) -> summary {} chars",
                            chat_id,
                            effective.connection.context_window,
                            history_budget_tokens,
                            to_compact.len(),
                            total_tokens,
                            new_summary.len()
                        );
                        chat_summary = Some(new_summary);
                    }
                    Ok(_) => tracing::warn!(
                        "[compaction] пустая сводка — жёсткая обрезка без сводки на этот ход"
                    ),
                    Err(e) => tracing::warn!(
                        "[compaction] суммаризация не удалась ({:?}) — жёсткая обрезка на этот ход",
                        e
                    ),
                }
            }
            history = old_system.into_iter().chain(recent).collect();
        }
    }

    // Дописать содержимое вложений к user-сообщениям истории. В БД content хранит
    // только оригинальный ввод пользователя, поэтому текст файлов дочитывается с диска
    // при каждой сборке контекста — иначе модель теряла файлы на follow-up ходах.
    let mut image_inputs: HashMap<Uuid, Vec<String>> = HashMap::new();
    for msg in history.iter_mut() {
        if msg.role == ChatRole::User {
            let images = append_attachments_to_context(&db, msg).await;
            if !images.is_empty() {
                image_inputs.insert(msg.id, images);
            }
        }
    }

    // 7. Преобразовать историю в формат для LLM
    let mut llm_messages: Vec<ChatMessage> = Vec::new();

    // Системный промпт и набор инструментов собираются из АКТИВНЫХ навыков (skills).
    use crate::shared::llm::skills;
    let mut skill_session =
        crate::shared::llm::skill_session::SkillSession::load(effective.agent_type.clone()).await?;
    // Детерминированный селектор предактивирует один подходящий skill. Матрица определяет,
    // является ли активация штатной или расширенной; denied никогда не раскрывается модели.
    let quick_result = crate::shared::llm::router::quick_intent_result(
        utf8_truncate(&content_with_attachments, 2000),
        &effective.agent_type,
    );
    let quick = quick_result.intent.clone();
    let previous_skill_id = history
        .iter()
        .rev()
        .filter(|message| message.role == ChatRole::Assistant)
        .filter_map(|message| message.skill_trace.as_deref())
        .filter_map(|raw| serde_json::from_str::<Vec<serde_json::Value>>(raw).ok())
        .flat_map(|items| items.into_iter().rev())
        .filter_map(|item| {
            item.get("skill_id")
                .and_then(|v| v.as_str())
                .map(str::to_string)
        })
        .next();
    let selected_skill = skill_session.select_initial(
        &quick,
        quick_result.confidence,
        previous_skill_id.as_deref(),
    );
    let initial_skill_trace = selected_skill.as_ref().map(|(skill, trace)| {
        crate::shared::llm::skill_session::activation_tool_trace(trace, &skill.title)
    });

    // Базовый промпт: кастомный из агента, иначе role-agnostic core.
    // ВАЖНО для кэша префикса у провайдеров (OpenAI/DeepSeek кэшируют префикс
    // автоматически): системный промпт держим байт-стабильным — без даты и прочих
    // меняющихся значений. Дата добавляется ОТДЕЛЬНЫМ system-сообщением после истории.
    let mut system_text = effective
        .system_prompt
        .clone()
        .unwrap_or_else(|| skills::core_prompt().to_string());
    // Основной каталог доступен сразу, но полные prompts/tools остаются ленивыми.
    system_text.push_str(&skill_session.immediate_catalog_prompt());
    system_text.push_str(&skill_session.active_prompts());
    tracing::info!(
        "[skills] chat_id='{}' quick_intent='{}' selected={:?} artifact_publish={}",
        chat_id,
        quick,
        selected_skill.as_ref().map(|(skill, trace)| (
            skill.id.as_str(),
            trace.access_level.as_str(),
            trace.source.as_str()
        )),
        skill_session.artifact_publish_allowed()
    );
    llm_messages.push(ChatMessage::system(system_text));

    // Кто по ту сторону чата. Отдельным system-сообщением сразу после промпта: блок
    // неизменен в пределах чата, поэтому кэш префикса не страдает (в отличие от даты,
    // которая уходит в самый конец).
    if let Some(actor) = &actor {
        // Логин — не имя. Требование «обращайся по имени» при наличии одного лишь
        // логина заставляло модель выдумывать имя собеседника («Иван» для admin).
        llm_messages.push(ChatMessage::system(format!(
            "Собеседник — реальный пользователь системы, логин {} (роль «{}»{}). \
             Это ЛОГИН, а не имя: не придумывай имя и не обращайся по имени — его система \
             не знает. Всё, что ты делаешь инструментами от лица \
             пользователя (например, оформление тикета), выполняется от его имени.",
            actor.username,
            actor.primary_role,
            if actor.is_admin {
                ", администратор"
            } else {
                ""
            }
        )));
    }

    // Сводка ранней части диалога (компакция): заменяет сообщения старше summary_upto.
    if let Some(summary) = &chat_summary {
        llm_messages.push(ChatMessage::system(format!(
            "Сводка ранней части этого диалога (более старые сообщения заменены ею):\n\n{}",
            summary
        )));
    }

    // Контекст прикреплённых страниц (если есть) — отдельным system-сообщением.
    // Привязка хранится в БД (context_package.chat_id), поэтому доступна каждый вызов.
    if let Ok(ctxs) = repository::list_context_by_chat(&db, chat_id).await {
        if !ctxs.is_empty() {
            // Дедуп: один и тот же объект/страница мог прикрепляться несколько раз —
            // оставляем только новейший пакет на ключ (entity_id, иначе page_key).
            // Глобальный потолок суммарного объёма: не раздувать контекст на множестве
            // крупных пакетов (каждый rendered_text — до 24KB).
            const MAX_CONTEXT_BLOCK_BYTES: usize = 48_000;
            use std::collections::HashSet;
            let mut seen: HashSet<String> = HashSet::new();
            let mut chosen: Vec<&_> = Vec::new();
            // Список отсортирован старые→новые; идём с конца, чтобы оставить новейший.
            for c in ctxs.iter().rev() {
                let dedup_key = c
                    .entity_id
                    .clone()
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| c.page_key.clone());
                if seen.insert(dedup_key) {
                    chosen.push(c);
                }
            }
            chosen.reverse(); // восстановить хронологический порядок

            let mut block = String::from(
                "Контекст прикреплённых страниц приложения (данные текущих объектов/отчётов). \
                 Опирайся на него при ответе:\n\n",
            );
            let mut total = 0usize;
            let mut omitted = false;
            for c in &chosen {
                if total + c.rendered_text.len() > MAX_CONTEXT_BLOCK_BYTES {
                    omitted = true;
                    continue;
                }
                block.push_str(&c.rendered_text);
                block.push_str("\n---\n\n");
                total += c.rendered_text.len();
            }
            if omitted {
                block.push_str("…[часть прикреплённого контекста опущена из-за объёма]\n");
            }
            llm_messages.push(ChatMessage::system(block));
        }
    }

    // Добавить историю
    for msg in &history {
        let mut content = msg.content.clone();
        if msg.role == ChatRole::Assistant {
            if let Some(trace) = msg.tool_trace.as_deref().filter(|trace| !trace.is_empty()) {
                const MAX_HISTORY_TRACE_BYTES: usize = 12_000;
                content.push_str(
                    "\n\n[Служебная трассировка выполнения предыдущего ответа; используй её для точного продолжения и диагностики:]\n",
                );
                content.push_str(utf8_truncate(trace, MAX_HISTORY_TRACE_BYTES));
            }
        }
        let chat_msg = ChatMessage {
            role: match msg.role {
                ChatRole::System => LlmChatRole::System,
                ChatRole::User => LlmChatRole::User,
                ChatRole::Assistant => LlmChatRole::Assistant,
            },
            content: Some(content),
            image_urls: image_inputs.get(&msg.id).cloned().unwrap_or_default(),
            tool_calls: None,
            tool_call_id: None,
        };
        llm_messages.push(chat_msg);
    }

    // Текущая дата — в конце (после истории), чтобы смена дня не инвалидировала
    // кэш префикса (system prompt + история) у провайдера.
    let now = chrono::Local::now();
    llm_messages.push(ChatMessage::system(format!(
        "Сегодня: {}. Текущий месяц: {}. Текущий год: {}.",
        now.format("%Y-%m-%d"),
        now.format("%m.%Y"),
        now.format("%Y")
    )));

    // Рабочий каталог — самым последним, т.к. это самый волатильный блок (меняется
    // внутри хода). Файл, который модель должна не забыть прочитать, она забудет:
    // анкета и план работают только потому, что переподставляются каждый ход.
    // Пустой каталог блока не даёт — не засорять промпт на приветствиях.
    if let Some(ws) = crate::shared::llm::chat_workspace::ChatWorkspace::for_chat(chat_id) {
        if let Some(block) = ws.render_for_prompt().await {
            llm_messages.push(ChatMessage::system(block));
        }
    }

    // 8.5 Роутер интентов (Фаза 0): классифицируем запрос пользователя для
    // метаданных/аналитики. Поведение пайплайна (tools/промпт) пока НЕ меняем.
    // Запускаем КОНКУРЕНТНО с основным tool-циклом (tokio::join! ниже), чтобы
    // классификация не добавляла серийную задержку к каждому сообщению.
    //
    // На локальной модели второй полный проход ради одной метки не окупается: он занимает
    // ту же карту, что и основной цикл, т.е. перестаёт быть «бесплатным» конкурентным
    // вызовом. Фактическим селектором навыка и так является `quick_result` выше.
    let router_input = utf8_truncate(&content_with_attachments, 2000);
    let router_llm_enabled = llm_router_enabled(&effective.connection);
    let router_rules_fallback = {
        let mut fallback = quick_result.clone();
        fallback.source = "rules_local";
        fallback
    };
    let router_fut = async {
        if router_llm_enabled {
            crate::shared::llm::router::classify_intent(
                provider.as_ref(),
                router_input,
                "",
                &effective.agent_type,
            )
            .await
        } else {
            router_rules_fallback
        }
    };

    // 9. Tool calling цикл: набор инструментов = core ∪ активные навыки.
    // tool_defs/active_tools/active_skills мутабельны — модель может активировать навыки
    // через use_skill (progressive disclosure), и набор пересобирается на лету.
    let mut builder_workflow =
        crate::shared::llm::builder_workflow::BuilderWorkflow::from_active_skills(
            skill_session.active_skill_ids(),
            skill_session.artifact_publish_allowed(),
        );
    let mut tool_defs =
        skill_session.advertised_tools(builder_workflow.as_ref().and_then(|flow| flow.next_tool()));
    let mut active_tools = skill_session.executable_tools();
    let start = std::time::Instant::now();

    tracing::info!(
        "[llm_loop] chat_id='{}' model='{}' history_msgs={} tools={}",
        chat_id,
        model_to_use,
        llm_messages.len(),
        tool_defs.len()
    );

    // Основной цикл — отдельный future, чтобы выполняться конкурентно с роутером.
    // Блок захватывает `llm_messages` по &mut (только этот future его меняет),
    // а провайдер/инструменты — по общей ссылке (их же конкурентно читает роутер).
    let agent_id_str = chat.agent_id.as_string();
    let loop_fut = async {
        let mut final_response: Option<crate::shared::llm::types::LlmResponse> = None;
        let mut artifact_to_attach: Option<LlmArtifactId> = None;
        let mut tool_trace: Vec<serde_json::Value> = initial_skill_trace.into_iter().collect();
        let mut total_tokens_used: i64 = 0;
        // Разбивка копится параллельно сумме: стоимость ответа считается по ней,
        // а один ответ пользователю — это все итерации цикла вместе.
        let mut usage_totals = UsageTotals::default();
        let mut tool_failures = 0usize;
        // Memo одинаковых read-only вызовов в пределах ОДНОГО ответа: модель регулярно
        // повторяет тот же drilldown/запрос на соседних итерациях (в разобранных чатах —
        // до 18 секунд на повтор). Данные внутри ответа не меняются: путь только читающий.
        let mut tool_memo: std::collections::HashMap<(String, String), String> =
            std::collections::HashMap::new();
        // Гарды дополняют memo, а не заменяют его: memo экономит время на
        // повторе читающего вызова, гард прерывает петлю и говорит об этом модели.
        let mut guards = GuardState::new(GuardCapabilities {
            artifact_publish_allowed: skill_session.artifact_publish_allowed(),
            data_repair_execute_allowed: skill_session.data_repair_execute_allowed(),
        });

        for iteration in 0..MAX_TOOL_ITERATIONS {
            if let Some(id) = job_id {
                if job_store::is_cancelled(id).await {
                    return Err(anyhow::anyhow!("LLM job cancelled"));
                }
            }
            let step = (iteration + 1) as u32;
            report_progress(job_id, step, "Модель думает…").await;

            // Стриминг: дельты текста складываются в job_store (in-memory),
            // фронт показывает их через poll/SSE ещё до завершения ответа.
            let response = match job_id {
                Some(id) => {
                    // Разделитель между текстом разных итераций (narration → финальный ответ)
                    if iteration > 0 && job_store::get_partial(id).is_some() {
                        job_store::append_partial(id, "\n\n");
                    }
                    let id_owned = id.to_string();
                    let sink = move |delta: &str| job_store::append_partial(&id_owned, delta);
                    provider
                        .chat_completion_with_tools_streaming(&llm_messages, &tool_defs, &sink)
                        .await
                }
                None => {
                    provider
                        .chat_completion_with_tools(&llm_messages, &tool_defs)
                        .await
                }
            }
            .map_err(|e| anyhow::anyhow!("LLM error: {:?}", e))?;
            total_tokens_used += response.tokens_used.unwrap_or(0) as i64;
            usage_totals.add_response(&response);

            tracing::info!(
                "[llm_iter] iter={} has_tool_calls={} finish_reason={:?} tokens={:?}",
                iteration + 1,
                response.has_tool_calls(),
                response.finish_reason,
                response.tokens_used
            );

            if !response.has_tool_calls() {
                // Финальный ответ — LLM завершил работу
                report_progress(job_id, step, "Формирую ответ…").await;
                final_response = Some(response);
                break;
            }

            tracing::debug!(
                "Tool calling iteration {}: {} calls",
                iteration + 1,
                response.tool_calls.len()
            );

            // Добавить ответ ассистента с tool_calls в историю сообщений
            llm_messages.push(ChatMessage::assistant_with_tool_calls(
                response.tool_calls.clone(),
            ));

            // Навыки, активированные на этой итерации (применяем после всех tool-результатов,
            // чтобы не разрывать пару assistant(tool_calls) → tool_result).
            let mut newly_activated: Vec<skills::Skill> = Vec::new();
            // Детерминированные подсказки по классам отказов этой итерации.
            let mut failure_hints: Vec<String> = Vec::new();

            // Выполнить каждый tool call и добавить результаты
            let tool_count = response.tool_calls.len();
            for (tool_idx, tool_call) in response.tool_calls.iter().enumerate() {
                if let Some(id) = job_id {
                    if job_store::is_cancelled(id).await {
                        return Err(anyhow::anyhow!("LLM job cancelled"));
                    }
                }
                let stage = if tool_count > 1 {
                    format!(
                        "{} ({}/{})",
                        human_tool_label(&tool_call.name),
                        tool_idx + 1,
                        tool_count
                    )
                } else {
                    human_tool_label(&tool_call.name)
                };
                report_progress(job_id, step, stage).await;

                let call_start = std::time::Instant::now();
                tracing::info!(
                    "[tool_call] iter={} tool='{}' args={}",
                    iteration + 1,
                    tool_call.name,
                    utf8_truncate(&tool_call.arguments, 200)
                );
                let active_skill_ids = skill_session.active_skill_set();

                // Граница вызова: решение принимается ДО исполнения и до memo —
                // иначе повтор молча обслуживался бы кэшем, и петля осталась бы
                // невидимой и для модели, и для трассы.
                let guard_denial = match guards.before_tool(&tool_call.name, &tool_call.arguments) {
                    GuardDecision::Deny { code, message } => {
                        tracing::warn!(
                            "[tool_guard] iter={} tool='{}' отклонён гардом '{}'",
                            iteration + 1,
                            tool_call.name,
                            code
                        );
                        let payload = serde_json::json!({
                            "error": message,
                            "_tool": tool_call.name,
                            "_ok": false,
                            "_guard": code,
                        });
                        Some((code, payload.to_string()))
                    }
                    GuardDecision::Allow => None,
                };
                let guard_code = guard_denial.as_ref().map(|(code, _)| *code);

                let memo_key = is_memoizable_tool(&tool_call.name)
                    .then(|| (tool_call.name.clone(), tool_call.arguments.clone()));
                let memoized = memo_key
                    .as_ref()
                    .and_then(|key| tool_memo.get(key))
                    .cloned();
                let result = if let Some((_, denial)) = guard_denial {
                    denial
                } else {
                    match memoized {
                        Some(cached) => {
                            tracing::info!(
                            "[tool_call] iter={} tool='{}' — повтор с теми же аргументами, ответ из memo",
                            iteration + 1,
                            tool_call.name
                        );
                            cached
                        }
                        None => {
                            let fresh = execute_tool_call(
                                tool_call,
                                chat_id,
                                &agent_id_str,
                                &effective.agent_type,
                                &active_tools,
                                &active_skill_ids,
                                skill_session.snapshot(),
                                skill_session.access(),
                                skill_session.artifact_publish_allowed(),
                                skill_session.script_execute_allowed(),
                                skill_session.script_develop_allowed(),
                                skill_session.data_repair_execute_allowed(),
                                actor.as_ref(),
                            )
                            .await;
                            if let Some(key) = memo_key {
                                // Ошибки не кэшируем: у них своя причина исправиться (модель
                                // чинит SQL и осмысленно повторяет вызов с тем же текстом).
                                let ok = serde_json::from_str::<serde_json::Value>(&fresh)
                                    .ok()
                                    .and_then(|v| {
                                        v.get("_ok")
                                            .and_then(serde_json::Value::as_bool)
                                            .or_else(|| v.get("error").is_none().then_some(true))
                                    })
                                    .unwrap_or(true);
                                if ok {
                                    tool_memo.insert(key, fresh.clone());
                                }
                            }
                            fresh
                        }
                    }
                };
                let call_ms = call_start.elapsed().as_millis() as u64;

                // Разобрать результат: единственный источник истины об успехе — поле `_ok`
                // (его проставляет tool_executor каждому результату).
                let parsed = serde_json::from_str::<serde_json::Value>(&result).ok();
                let is_ok = parsed
                    .as_ref()
                    .map(|v| v.get("_ok").and_then(|b| b.as_bool()).unwrap_or(true))
                    .unwrap_or(true);
                if !is_ok {
                    tool_failures += 1;
                }

                tracing::info!(
                    "[tool_result] tool='{}' ms={} ok={} preview={}",
                    tool_call.name,
                    call_ms,
                    is_ok,
                    utf8_truncate(&result, 300)
                );

                // Детерминированная передача управления для builder-workflow. Модель выбирает
                // конкретный source/spec, но не может бесконечно возвращаться к discovery после
                // успешного preview или пропустить publish.
                if let Some(flow) = builder_workflow.as_mut() {
                    flow.observe(&tool_call.name, is_ok, parsed.as_ref());
                }

                // Артефакт прикрепляем к сообщению только от инструментов, чей артефакт
                // предназначен пользователю. `execute_query` тоже возвращает artifact_id,
                // но это служебная запись самого запроса: под аналитическим ответом она
                // всплывала карточкой «сырой выборки» — шум, да ещё и от произвольного
                // из нескольких запросов (побеждал последний).
                if USER_FACING_ARTIFACT_TOOLS.contains(&tool_call.name.as_str()) {
                    if let Some(ref v) = parsed {
                        if let Some(id_str) = v.get("artifact_id").and_then(|v| v.as_str()) {
                            if let Ok(uid) = Uuid::parse_str(id_str) {
                                artifact_to_attach = Some(LlmArtifactId::new(uid));
                                tracing::info!("Tool call produced artifact: {}", id_str);
                            }
                        }
                    }
                }

                // Активация навыка через use_skill (_activate_skill) — progressive disclosure.
                if let Some(ref v) = parsed {
                    if let Some(sid) = v.get("_activate_skill").and_then(|x| x.as_str()) {
                        let was_active =
                            skill_session.active_skill_ids().iter().any(|id| id == sid);
                        if !was_active && skill_session.activate(sid, "model").is_some() {
                            if let Some(skill) = skill_session.skill(sid) {
                                newly_activated.push(skill);
                            }
                        }
                    }
                }

                // Краткое описание результата для трассировки
                let summary = if !is_ok {
                    parsed
                        .as_ref()
                        .and_then(|v| v.get("error").and_then(|e| e.as_str()))
                        .unwrap_or("error")
                        .chars()
                        .take(120)
                        .collect::<String>()
                } else {
                    parsed
                        .as_ref()
                        .and_then(|v| {
                            // Подбираем лучшее краткое описание в зависимости от инструмента
                            v.get("row_count")
                                .and_then(|n| n.as_u64())
                                .map(|n| format!("{} rows", n))
                                .or_else(|| {
                                    v.get("total")
                                        .and_then(|n| n.as_u64())
                                        .map(|n| format!("{} items", n))
                                })
                                .or_else(|| {
                                    v.get("session_id")
                                        .and_then(|s| s.as_str())
                                        .map(|_| "artifact created".to_string())
                                })
                                .or_else(|| {
                                    v.get("artifact_id")
                                        .and_then(|s| s.as_str())
                                        .map(|_| "artifact created".to_string())
                                })
                        })
                        .unwrap_or_else(|| "ok".to_string())
                };

                // Срабатывание гарда попадает в трассу под своей стадией: иначе
                // «сколько раз система удержала агента» неотличимо от отказа
                // самого инструмента, и на d407 обе величины слипаются в одну.
                tool_trace.push(serde_json::json!({
                    "iteration": iteration + 1,
                    "call":     tool_idx + 1,
                    "stage":    guard_code.map_or_else(|| tool_stage(&tool_call.name), |_| "guard"),
                    "tool":    tool_call.name,
                    "ok":      is_ok,
                    "ms":      call_ms,
                    "summary": guard_code
                        .map(|code| format!("гард: {code}"))
                        .unwrap_or(summary),
                    "input":   trace_input(&tool_call.arguments),
                    "output":  trace_output(parsed.as_ref()),
                }));

                // Подсказка по классу отказа — после всех tool-результатов
                // (см. ниже), чтобы не разрывать пару assistant(tool_calls) → tool_result.
                if let Some(value) = parsed.as_ref() {
                    if let Some(hint) = guards.after_tool(&tool_call.name, value) {
                        failure_hints.push(hint);
                    }
                }

                llm_messages.push(ChatMessage::tool_result(tool_call.id.clone(), result));
            }

            for hint in failure_hints {
                llm_messages.push(ChatMessage::system(hint));
            }

            // Применить активированные навыки: пересобрать инструменты и дописать
            // их инструкции (после всех tool-результатов — пара assistant/tool не разорвана).
            if !newly_activated.is_empty() {
                if builder_workflow.is_none() {
                    builder_workflow =
                        crate::shared::llm::builder_workflow::BuilderWorkflow::from_active_skills(
                            skill_session.active_skill_ids(),
                            skill_session.artifact_publish_allowed(),
                        );
                }
                tool_defs = skill_session
                    .advertised_tools(builder_workflow.as_ref().and_then(|flow| flow.next_tool()));
                active_tools = skill_session.executable_tools();
                for sk in newly_activated {
                    tracing::info!("[skill] activated '{}' (chat='{}')", sk.id, chat_id);
                    llm_messages.push(ChatMessage::system(format!(
                        "Активирован навык «{}». Его инструменты и инструкции:\n\n{}",
                        sk.title,
                        skills::activation_prompt(&sk)
                    )));
                }
            }

            if let Some(next) = builder_workflow.as_ref().and_then(|flow| flow.next_tool()) {
                tool_defs = skill_session.advertised_tools(Some(next));
            }

            // Публикация является терминальным выходом builder-workflow. После получения
            // artifact_id больше не даём модели запускать discovery/preview повторно.
            if builder_workflow.is_some() && artifact_to_attach.is_some() {
                break;
            }

            if tool_failures >= MAX_TOOL_FAILURES {
                tracing::warn!(
                    "[llm_loop] stopped after {} failed tool calls (chat='{}')",
                    tool_failures,
                    chat_id
                );
                llm_messages.push(ChatMessage::system(
                    "Лимит технических ошибок инструментов исчерпан. Не вызывай инструменты снова; кратко сообщи пользователю, какой контракт аргументов не удалось выполнить."
                        .to_string(),
                ));
                break;
            }
        }

        // Сверка плана с журналом шагов. До этого «пункт выполнен» было
        // утверждением модели, которое никто не проверял; теперь ссылка на
        // несохранённый шаг становится наблюдаемым фактом, а не догадкой при
        // разборе диалога.
        match crate::shared::llm::chat_workspace::plan_state(chat_id).await {
            Ok((_, drift)) if !drift.is_empty() => {
                tracing::warn!(
                    "[plan_drift] chat='{}': закрытых пунктов со ссылкой на несохранённый шаг — {}",
                    chat_id,
                    drift.len()
                );
                tool_trace.push(serde_json::json!({
                    "iteration": MAX_TOOL_ITERATIONS + 1,
                    "call": 0,
                    "stage": "guard",
                    "tool": "plan",
                    "ok": false,
                    "ms": 0,
                    "summary": format!("гард: plan_drift ({} пункт(ов))", drift.len()),
                    "input": { "steps": drift.iter().map(|s| &s.id).collect::<Vec<_>>() },
                    "output": {
                        "_guard": "plan_drift",
                        "error": "Пункты плана отмечены выполненными, но шаги, на которые они \
                                  ссылаются, в журнале отсутствуют",
                        "steps": drift,
                    },
                }));
            }
            Ok(_) => {}
            Err(e) => tracing::debug!("[plan_drift] чтение плана не удалось: {e}"),
        }

        if let Some(flow) = builder_workflow
            .as_ref()
            .filter(|_| artifact_to_attach.is_none())
        {
            tool_trace.push(serde_json::json!({
                "iteration": MAX_TOOL_ITERATIONS + 1,
                "call": 0,
                "stage": "publish",
                "tool": "workflow",
                "ok": false,
                "ms": 0,
                "summary": flow.completion_label(false),
                "input": { "expected": flow.next_tool().unwrap_or("publish") },
                "output": {
                    "error_code": "workflow_incomplete",
                    "error": "Builder workflow завершился без artifact_id",
                    "next_step": flow.next_tool(),
                },
            }));
        }

        // Запрашиваем финальный ТЕКСТОВЫЙ ответ, если:
        //  - исчерпан лимит итераций (модель всё ещё звала инструменты), ИЛИ
        //  - модель завершила работу, но вернула ПУСТОЙ текст (наблюдается у DeepSeek: после
        //    серии tool-call'ов отдаёт пустой content — пользователь иначе получит пустое
        //    сообщение). В обоих случаях просим связный итог без инструментов.
        let final_is_blank = final_response
            .as_ref()
            .map(|r| r.content.trim().is_empty())
            .unwrap_or(true);
        if final_is_blank {
            tracing::warn!(
                "[llm_iter] финальный ответ пуст/не получен (лимит {} итераций?) — запрашиваю текстовый итог",
                MAX_TOOL_ITERATIONS
            );
            llm_messages.push(ChatMessage::system(
                "Заверши диалог: больше инструменты НЕ вызывай и НЕ возвращай пустой ответ. \
                 Подведи краткий итог пользователю на естественном языке: что уже сделано и его \
                 статус (полученные данные, созданные/обновлённые объекты и артефакты) и предложи \
                 следующий шаг."
                    .to_string(),
            ));
            let summary_result = match job_id {
                Some(id) => {
                    if job_store::get_partial(id).is_some() {
                        job_store::append_partial(id, "\n\n");
                    }
                    let id_owned = id.to_string();
                    let sink = move |delta: &str| job_store::append_partial(&id_owned, delta);
                    provider
                        .chat_completion_with_tools_streaming(&llm_messages, &[], &sink)
                        .await
                }
                None => provider.chat_completion(&llm_messages).await,
            };
            match summary_result {
                Ok(resp) => {
                    total_tokens_used += resp.tokens_used.unwrap_or(0) as i64;
                    usage_totals.add_response(&resp);
                    final_response = Some(resp);
                }
                Err(e) => tracing::warn!(
                    "[llm_iter] финальный ответ без инструментов не удался: {:?}",
                    e
                ),
            }
        }

        if let Some(response) = final_response.as_mut() {
            response.tokens_used = Some(total_tokens_used.min(i32::MAX as i64) as i32);
            response.prompt_tokens = usage_totals.prompt_i32();
            response.completion_tokens = usage_totals.completion_i32();
            response.cached_prompt_tokens = usage_totals.cached_prompt_i32();
        }
        let fallback_text = if artifact_to_attach.is_some() {
            "Результат создан, но модель не сформировала итоговый текст. Откройте карточку артефакта ниже."
        } else {
            "Не удалось завершить построение: модель исчерпала лимит технических попыток. Повторите запрос — диагностические детали сохранены в tool trace."
        };
        match final_response.as_mut() {
            Some(response) if response.content.trim().is_empty() => {
                response.content = fallback_text.to_string();
            }
            None => {
                final_response = Some(crate::shared::llm::types::LlmResponse {
                    content: fallback_text.to_string(),
                    tool_calls: vec![],
                    tokens_used: Some(total_tokens_used.min(i32::MAX as i64) as i32),
                    prompt_tokens: usage_totals.prompt_i32(),
                    completion_tokens: usage_totals.completion_i32(),
                    cached_prompt_tokens: usage_totals.cached_prompt_i32(),
                    model: model_to_use.clone(),
                    finish_reason: Some("local_fallback".to_string()),
                    confidence: None,
                });
            }
            _ => {}
        }
        Ok::<_, anyhow::Error>((final_response, artifact_to_attach, tool_trace))
    };

    // Конкурентный запуск: роутер не добавляет серийную задержку.
    let (intent_result, loop_result) = tokio::join!(router_fut, loop_fut);
    let (mut final_response, artifact_to_attach, tool_trace) = loop_result?;
    if let Some(response) = final_response.as_mut() {
        let combined =
            response.tokens_used.unwrap_or(0) as i64 + intent_result.tokens_used.max(0) as i64;
        response.tokens_used = Some(combined.min(i32::MAX as i64) as i32);
        // Классификатор — такой же платный вызов модели, как и итерации цикла.
        let mut totals = UsageTotals {
            prompt: response.prompt_tokens.unwrap_or(0) as i64,
            completion: response.completion_tokens.unwrap_or(0) as i64,
            cached_prompt: response.cached_prompt_tokens.unwrap_or(0) as i64,
        };
        totals.add(
            intent_result.prompt_tokens,
            intent_result.completion_tokens,
            intent_result.cached_prompt_tokens,
        );
        response.prompt_tokens = totals.prompt_i32();
        response.completion_tokens = totals.completion_i32();
        response.cached_prompt_tokens = totals.cached_prompt_i32();
    }

    tracing::info!(
        "[router] chat_id='{}' mode={} intent='{}' confidence={:.2} source={}",
        chat_id,
        if router_llm_enabled {
            "llm"
        } else {
            "local_rules"
        },
        intent_result.intent,
        intent_result.confidence,
        intent_result.source
    );

    let duration_ms = start.elapsed().as_millis() as i64;

    if let Some(flow) = &builder_workflow {
        let stage = flow.completion_label(artifact_to_attach.is_some());
        report_progress(job_id, MAX_TOOL_ITERATIONS as u32 + 1, stage).await;
    }

    let llm_response = final_response.ok_or_else(|| {
        anyhow::anyhow!(
            "LLM exceeded maximum tool calling iterations ({})",
            MAX_TOOL_ITERATIONS
        )
    })?;

    // Статья, процитированная в ответе, дошла до человека — это отдельный
    // от «прочитана моделью» факт и главный сигнал полезности статьи.
    crate::shared::llm::kb_tools::record_citations(&llm_response.content);

    // Стоимость считается ЗДЕСЬ и один раз, по прайсу подключения на момент прогона:
    // ставки со временем меняются, а исторические суммы должны остаться сопоставимыми.
    let pricing = Pricing::from_fields(
        effective.connection.price_in_per_mtok,
        effective.connection.price_out_per_mtok,
        effective.connection.price_cached_per_mtok,
    );
    let answer_usage = UsageTotals {
        prompt: llm_response.prompt_tokens.unwrap_or(0) as i64,
        completion: llm_response.completion_tokens.unwrap_or(0) as i64,
        cached_prompt: llm_response.cached_prompt_tokens.unwrap_or(0) as i64,
    };
    let answer_cost = cost_micro(&answer_usage, pricing);
    let answer_currency = answer_cost
        .and(effective.connection.currency.clone())
        .filter(|c| !c.trim().is_empty());

    // 10. Сохранить ответ ассистента с метаданными
    let mut assistant_msg = LlmChatMessage::new_with_metadata(
        chat_id_obj,
        ChatRole::Assistant,
        llm_response.content,
        llm_response.tokens_used,
        Some(model_to_use),
        llm_response.confidence,
        Some(duration_ms),
    )
    .with_usage(
        llm_response.prompt_tokens,
        llm_response.completion_tokens,
        llm_response.cached_prompt_tokens,
        answer_cost,
        answer_currency,
    );

    if let Some(artifact_id) = artifact_to_attach {
        assistant_msg.artifact_id = Some(artifact_id);
        assistant_msg.artifact_action =
            Some(contracts::domain::a018_llm_chat::aggregate::ArtifactAction::Created);
    }

    // Трасса вызовов инструментов теперь ведётся в двух местах с разной ролью:
    //  - tool_trace_json на сообщении — МИНИМУМ для пилюль (tool, ok, ms);
    //  - sys_tool_trace — полная запись на каждый вызов (вход/выход/summary),
    //    для детальной карточки в UI и кросс-чат аналитики.
    if !tool_trace.is_empty() {
        let pills: Vec<serde_json::Value> = tool_trace
            .iter()
            .map(|e| {
                serde_json::json!({
                    "tool": e.get("tool"),
                    "ok": e.get("ok"),
                    "ms": e.get("ms"),
                })
            })
            .collect();
        assistant_msg.tool_trace = serde_json::to_string(&pills).ok();
    }

    let skill_trace: Vec<contracts::domain::a018_llm_chat::aggregate::SkillTraceEntry> = tool_trace
        .iter()
        .filter_map(|entry| {
            let output = entry.get("output")?;
            let skill_id = output.get("_activate_skill")?.as_str()?;
            let access_level = output
                .get("_skill_access_level")
                .and_then(|v| v.as_str())
                .unwrap_or("immediate");
            let inefficient = output
                .get("_skill_inefficiency")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let source = output
                .get("source")
                .and_then(|v| v.as_str())
                .unwrap_or("model");
            let skill_digest = output
                .get("_skill_digest")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            let registry_generation = output
                .get("_registry_generation")
                .and_then(|v| v.as_u64())
                .unwrap_or_default();
            Some(
                contracts::domain::a018_llm_chat::aggregate::SkillTraceEntry {
                    skill_id: skill_id.to_string(),
                    skill_digest: skill_digest.to_string(),
                    registry_generation,
                    access_level: access_level.to_string(),
                    source: source.to_string(),
                    inefficient,
                },
            )
        })
        .collect();
    if !skill_trace.is_empty() {
        assistant_msg.skill_trace = serde_json::to_string(&skill_trace).ok();
    }

    // Метка интента: пишем ТОТ интент, который реально управлял подбором навыка (быстрый
    // rule-based селектор), а не мнение LLM-роутера. Раньше в сообщение попадал роутер, и в
    // разборе чатов было видно `intent=marketplace_funnel_analysis` при фактически
    // работавшем data-analytics — поле вводило в заблуждение. Расхождение остаётся в логах.
    if intent_result.intent != quick {
        tracing::warn!(
            "[router] chat_id='{}' intent divergence: acting='{}' router='{}' (confidence={:.2})",
            chat_id,
            quick,
            intent_result.intent,
            intent_result.confidence
        );
    }
    assistant_msg.intent = Some(quick.clone());

    repository::insert_message(&db, &assistant_msg).await?;

    // Полный журнал вызовов — отдельной пачкой, уже с известным message_id.
    if !tool_trace.is_empty() {
        let message_id = assistant_msg.id.to_string();
        let created_at = assistant_msg.created_at;
        let rows: Vec<contracts::domain::a018_llm_chat::aggregate::ToolTraceEntry> = tool_trace
            .iter()
            .map(
                |e| contracts::domain::a018_llm_chat::aggregate::ToolTraceEntry {
                    id: Uuid::new_v4().to_string(),
                    chat_id: chat_id.to_string(),
                    message_id: message_id.clone(),
                    iteration: e.get("iteration").and_then(|v| v.as_i64()).unwrap_or(0),
                    call_index: e.get("call").and_then(|v| v.as_i64()).unwrap_or(0),
                    stage: e
                        .get("stage")
                        .and_then(|v| v.as_str())
                        .unwrap_or("tool")
                        .to_string(),
                    tool: e
                        .get("tool")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    ok: e.get("ok").and_then(|v| v.as_bool()).unwrap_or(true),
                    ms: e.get("ms").and_then(|v| v.as_i64()).unwrap_or(0),
                    summary: e
                        .get("summary")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string()),
                    input: e.get("input").cloned(),
                    output: e.get("output").cloned(),
                    created_at,
                },
            )
            .collect();
        if let Err(err) = repository::insert_tool_trace_batch(&db, &rows).await {
            // Журнал вспомогательный — его сбой не должен рушить ответ ассистента.
            tracing::warn!("[tool_trace] failed to persist sys_tool_trace: {}", err);
        }
    }

    Ok(assistant_msg)
}

/// Собрать контекст текущей страницы и привязать его к чату.
/// `session_user_id` включает снимок навигации пользователя в пакет контекста
/// (см. `context::build_for_page_key_with_session`).
pub async fn add_chat_context(
    chat_id: &str,
    page_key: &str,
    label: Option<&str>,
    session_user_id: Option<&str>,
) -> anyhow::Result<ContextPackageSummary> {
    let db = crate::shared::data::db::get_connection();
    let built =
        super::context::build_for_page_key_with_session(page_key, label, session_user_id).await;

    let id = Uuid::new_v4().to_string();
    let created_at = chrono::Utc::now().to_rfc3339();

    repository::insert_context_package(
        &db,
        repository::NewContextPackage {
            id: id.clone(),
            chat_id: Some(chat_id.to_string()),
            page_key: page_key.to_string(),
            page_type: built.page_type.clone(),
            entity_index: built.entity_index.clone(),
            entity_id: built.entity_id.clone(),
            title: built.title.clone(),
            context_json: built.context_json.to_string(),
            rendered_text: built.rendered_text.clone(),
            created_at: created_at.clone(),
        },
    )
    .await?;

    Ok(ContextPackageSummary {
        id,
        chat_id: Some(chat_id.to_string()),
        page_key: page_key.to_string(),
        page_type: built.page_type,
        title: built.title,
        created_at,
        rendered_text: built.rendered_text,
    })
}

/// Получить один пакет контекста по id (для details-страницы контекста).
pub async fn get_context_by_id(id: &str) -> anyhow::Result<Option<ContextPackageSummary>> {
    let db = crate::shared::data::db::get_connection();
    let row = repository::find_context_by_id(&db, id).await?;
    Ok(row.map(|m| ContextPackageSummary {
        id: m.id,
        chat_id: m.chat_id,
        page_key: m.page_key,
        page_type: m.page_type,
        title: m.title,
        created_at: m.created_at,
        rendered_text: m.rendered_text,
    }))
}

/// Список пакетов контекста, привязанных к чату.
pub async fn list_chat_context(chat_id: &str) -> anyhow::Result<Vec<ContextPackageSummary>> {
    let db = crate::shared::data::db::get_connection();
    let rows = repository::list_context_by_chat(&db, chat_id).await?;
    Ok(rows
        .into_iter()
        .map(|m| ContextPackageSummary {
            id: m.id,
            chat_id: m.chat_id,
            page_key: m.page_key,
            page_type: m.page_type,
            title: m.title,
            created_at: m.created_at,
            rendered_text: m.rendered_text,
        })
        .collect())
}

/// Загрузить файл-вложение для чата
pub async fn upload_attachment(
    chat_id: &str,
    multipart: &mut Multipart,
    uploaded_by_user_id: Option<String>,
) -> anyhow::Result<LlmChatAttachment> {
    // Проверить что чат существует
    let chat_uuid =
        Uuid::parse_str(chat_id).map_err(|e| anyhow::anyhow!("Invalid chat ID: {}", e))?;
    let chat_id_obj = LlmChatId::new(chat_uuid);

    let db = crate::shared::data::db::get_connection();
    repository::find_by_id(&db, &chat_id_obj)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Chat not found"))?;

    // Получить файл из multipart
    let field = multipart
        .next_field()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to read multipart field: {}", e))?
        .ok_or_else(|| anyhow::anyhow!("No file in multipart"))?;

    let filename = field
        .file_name()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("screenshot")
        .to_string();

    let declared_content_type = field
        .content_type()
        .unwrap_or("application/octet-stream")
        .to_string();

    let data = field
        .bytes()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to read file bytes: {}", e))?;

    let file_size = data.len() as i64;
    const MAX_UPLOAD_BYTES: usize = 10 * 1024 * 1024;
    if data.len() > MAX_UPLOAD_BYTES {
        anyhow::bail!("Attachment exceeds 10 MiB");
    }
    let content_type = normalize_image_content_type(&data)
        .map(str::to_string)
        .unwrap_or(declared_content_type);
    if content_type.starts_with("image/") && !is_image_content_type(&content_type) {
        anyhow::bail!("Only PNG, JPEG and WebP images are supported");
    }
    if is_image_content_type(&content_type) && normalize_image_content_type(&data).is_none() {
        anyhow::bail!("Image contents do not match a supported image format");
    }

    // Изображения сразу становятся приватными S3-объектами. Текстовые вложения
    // пока сохраняют прежнее локальное хранение.
    let (filepath, s3_file_id) = if is_image_content_type(&content_type) {
        let uploaded = s3_service::upload(
            S3FileCategory::LlmChatImages,
            UploadedFile {
                filename: filename.clone(),
                content_type: Some(content_type.clone()),
                bytes: data.clone(),
            },
            uploaded_by_user_id,
        )
        .await?;
        (String::new(), Some(uploaded.id))
    } else {
        let upload_dir = attachments_root().join(chat_id);
        tokio::fs::create_dir_all(&upload_dir)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to create upload directory: {}", e))?;

        let file_id = Uuid::new_v4();
        let file_ext = std::path::Path::new(&filename)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");
        let stored_filename = if file_ext.is_empty() {
            file_id.to_string()
        } else {
            format!("{}.{}", file_id, file_ext)
        };
        tokio::fs::write(upload_dir.join(&stored_filename), &data)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to write file: {}", e))?;
        // В БД идёт ключ ОТНОСИТЕЛЬНО корня вложений. Абсолютный путь зависит от
        // конфига и может измениться при переносе данных, а старая форма записи
        // (относительно рабочего каталога процесса) ломалась от одного лишь
        // запуска бэкенда из другой директории.
        (format!("{chat_id}/{stored_filename}"), None)
    };

    // Создать запись о вложении (без привязки к сообщению пока)
    let attachment = LlmChatAttachment::new(
        chat_id_obj,
        None, // Привяжем к сообщению только при его отправке
        filename,
        filepath,
        s3_file_id.clone(),
        content_type,
        file_size,
    );

    // Сохранить временную запись (message_id = NULL)
    // Позже при отправке сообщения обновим message_id
    if let Err(error) = repository::insert_attachment(&db, &attachment).await {
        if let Some(s3_file_id) = s3_file_id {
            let _ = s3_service::delete(&s3_file_id).await;
        }
        return Err(error.into());
    }

    Ok(attachment)
}

pub async fn get_attachment(
    chat_id: &str,
    attachment_id: &str,
) -> anyhow::Result<LlmChatAttachment> {
    let chat_id = LlmChatId::new(Uuid::parse_str(chat_id)?);
    let attachment_id = Uuid::parse_str(attachment_id)?;
    let db = crate::shared::data::db::get_connection();
    let attachment = repository::find_attachment_by_id(&db, &attachment_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Attachment not found"))?;
    if attachment.chat_id != chat_id {
        anyhow::bail!("Attachment does not belong to this chat");
    }
    Ok(attachment)
}

pub async fn delete_pending_attachment(chat_id: &str, attachment_id: &str) -> anyhow::Result<()> {
    let attachment = get_attachment(chat_id, attachment_id).await?;
    if attachment.message_id.is_some() {
        anyhow::bail!("Sent attachments cannot be deleted");
    }
    if let Some(s3_file_id) = attachment.s3_file_id.as_deref() {
        s3_service::delete(s3_file_id).await?;
    } else {
        match tokio::fs::remove_file(attachment_abs_path(&attachment.filepath)).await {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    let db = crate::shared::data::db::get_connection();
    repository::delete_attachment_by_id(&db, &attachment.id).await?;
    Ok(())
}

fn is_image_content_type(content_type: &str) -> bool {
    matches!(content_type, "image/png" | "image/jpeg" | "image/webp")
}

fn normalize_image_content_type(data: &[u8]) -> Option<&'static str> {
    if data.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("image/png")
    } else if data.starts_with(&[0xff, 0xd8, 0xff]) {
        Some("image/jpeg")
    } else if data.len() >= 12 && &data[..4] == b"RIFF" && &data[8..12] == b"WEBP" {
        Some("image/webp")
    } else {
        None
    }
}

/// Загрузить содержимое текстового файла
pub async fn load_attachment_bytes(attachment: &LlmChatAttachment) -> anyhow::Result<Bytes> {
    if let Some(s3_file_id) = attachment.s3_file_id.as_deref() {
        let download = s3_service::download(s3_file_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("S3 object not found"))?;
        return Ok(download.bytes);
    }
    tokio::fs::read(attachment_abs_path(&attachment.filepath))
        .await
        .map(Bytes::from)
        .map_err(|e| anyhow::anyhow!("Failed to read file: {}", e))
}

/// Корень файловых вложений чатов из конфига.
fn attachments_root() -> PathBuf {
    match crate::shared::config::load_config() {
        Ok(config) => crate::shared::config::get_attachments_path(&config),
        Err(error) => {
            tracing::warn!("Не удалось прочитать конфиг для пути вложений: {error}");
            PathBuf::from("uploads").join("chat_attachments")
        }
    }
}

/// Абсолютный путь к файлу вложения по значению из БД.
///
/// В колонке `filepath` сосуществуют две формы: исторический путь относительно
/// РАБОЧЕГО КАТАЛОГА процесса (`uploads/chat_attachments/...`, на Windows — с
/// обратными слешами) и нынешний ключ относительно корня вложений
/// (`<chat_id>/<uuid>.<ext>`). Миграция 0210 переписывает первую форму во
/// вторую, но старые строки могут пережить откат миграции или прийти из копии
/// БД, поэтому обе распознаются здесь.
fn attachment_abs_path(filepath: &str) -> PathBuf {
    let path = std::path::Path::new(filepath);
    if path.is_absolute() || filepath.starts_with("uploads/") || filepath.starts_with("uploads\\") {
        return path.to_path_buf();
    }
    attachments_root().join(filepath.replace('\\', "/"))
}

async fn read_attachment_text(attachment: &LlmChatAttachment) -> anyhow::Result<String> {
    String::from_utf8(load_attachment_bytes(attachment).await?.to_vec())
        .map_err(|e| anyhow::anyhow!("Attachment is not valid UTF-8: {}", e))
}

/// Потолок на текст одного вложения в контексте LLM — чтобы один большой файл
/// не вытеснил остальную историю.
const MAX_ATTACHMENT_FILE_BYTES: usize = 64_000;

/// Дописать к тексту сообщения содержимое его вложений (для контекста LLM).
async fn append_attachments_to_context(
    db: &sea_orm::DatabaseConnection,
    msg: &mut LlmChatMessage,
) -> Vec<String> {
    let atts = match repository::find_attachments_by_message_id(db, &msg.id).await {
        Ok(atts) => atts,
        Err(e) => {
            tracing::warn!("Failed to load attachments for message {}: {}", msg.id, e);
            return Vec::new();
        }
    };
    if atts.is_empty() {
        return Vec::new();
    }
    let mut parts = Vec::new();
    let mut images = Vec::new();
    for att in &atts {
        if is_image_content_type(&att.content_type) {
            match load_attachment_bytes(att).await {
                Ok(bytes) => {
                    let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
                    images.push(format!("data:{};base64,{}", att.content_type, encoded));
                }
                Err(e) => tracing::warn!("Failed to read image {}: {}", att.filename, e),
            }
            continue;
        }
        match read_attachment_text(att).await {
            Ok(content) => {
                let truncated = utf8_truncate(&content, MAX_ATTACHMENT_FILE_BYTES);
                let suffix = if truncated.len() < content.len() {
                    "\n…[файл обрезан]"
                } else {
                    ""
                };
                parts.push(format!("--- {} ---\n{}{}", att.filename, truncated, suffix));
            }
            Err(e) => {
                tracing::warn!("Failed to read attachment {}: {}", att.filename, e);
            }
        }
    }
    if !parts.is_empty() {
        msg.content.push_str("\n\nПрикрепленные файлы:\n");
        msg.content.push_str(&parts.join("\n\n"));
    }
    images
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_allowlist_accepts_listed_model() {
        let allowed = vec!["deepseek-v4-pro".to_string()];
        assert!(ensure_model_is_allowed("deepseek-v4-pro", &allowed).is_ok());
    }

    #[test]
    fn model_allowlist_rejects_foreign_model_with_available_options() {
        let allowed = vec![
            "deepseek-v4-pro".to_string(),
            "deepseek-v4-flash".to_string(),
        ];
        let error = ensure_model_is_allowed("kimi-k3", &allowed)
            .expect_err("foreign model must be rejected")
            .to_string();
        assert!(error.contains("kimi-k3"));
        assert!(error.contains("deepseek-v4-pro, deepseek-v4-flash"));
    }

    #[test]
    fn empty_model_allowlist_remains_unrestricted() {
        assert!(ensure_model_is_allowed("custom-model", &[]).is_ok());
    }

    fn msg(chars: usize) -> LlmChatMessage {
        LlmChatMessage::user(LlmChatId::new(Uuid::nil()), "я".repeat(chars))
    }

    #[test]
    fn estimate_tokens_rough_thirds() {
        assert_eq!(estimate_tokens(""), 1);
        assert_eq!(estimate_tokens(&"a".repeat(300)), 101);
        // Кириллица считается по символам, не по байтам (UTF-8: 2 байта/символ).
        assert_eq!(estimate_tokens(&"я".repeat(300)), 101);
    }

    /// Окно локальной модели (Ollama, num_ctx=32k) → бюджет 24k, живой хвост 12k.
    const LOCAL_WINDOW: i32 = 32_768;

    #[test]
    fn history_budget_default_matches_legacy_constants() {
        // Дефолт колонки context_window обязан воспроизводить прежние захардкоженные
        // 120_000/60_000 — иначе миграция изменит поведение облачных подключений.
        let default_window =
            contracts::domain::a038_llm_connection::aggregate::default_context_window();
        assert_eq!(history_budget(default_window), (120_000, 60_000));
        // Кривое значение из БД не должно ронять бюджет в ноль.
        assert_eq!(history_budget(0), (120_000, 60_000));
    }

    #[test]
    fn history_budget_scales_with_local_window() {
        assert_eq!(history_budget(LOCAL_WINDOW), (24_576, 12_288));
    }

    #[test]
    fn compaction_keeps_recent_tail() {
        // 4 сообщения по ~9000 токенов: total ~36k > бюджета 24k.
        // Живым остаётся хвост ≤ 12k — последнее сообщение, cut = 3.
        let (budget, keep) = history_budget(LOCAL_WINDOW);
        let history: Vec<_> = (0..4).map(|_| msg(27_000)).collect();
        let total: usize = history.iter().map(message_tokens).sum();
        assert!(total > budget);
        assert_eq!(compaction_cut_index(&history, total, keep), 3);
    }

    #[test]
    fn compaction_never_cuts_last_message() {
        // Единственное гигантское сообщение (текущий вопрос) не компактится.
        let (_, keep) = history_budget(LOCAL_WINDOW);
        let history = vec![msg(120_000)];
        let total: usize = history.iter().map(message_tokens).sum();
        assert_eq!(compaction_cut_index(&history, total, keep), 0);
    }

    #[test]
    fn compaction_cuts_all_but_last_when_tail_is_huge() {
        // Оба сообщения больше keep: компактим всё, кроме текущего вопроса.
        let (_, keep) = history_budget(LOCAL_WINDOW);
        let history = vec![msg(60_000), msg(60_000)];
        let total: usize = history.iter().map(message_tokens).sum();
        assert_eq!(compaction_cut_index(&history, total, keep), 1);
    }
}
