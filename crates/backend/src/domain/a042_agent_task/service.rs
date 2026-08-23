//! Бизнес-логика очереди поручений между AI-сотрудниками.
//!
//! Два уровня контроля переходов, они закрывают разные поверхности:
//!   * условный `UPDATE` в репозитории (`try_claim`, `release_stale`,
//!     `fail_exhausted`) — межпроцессные гонки захвата;
//!   * `can_transition` в единственном приватном `set_status` — логические ошибки
//!     на ручных путях (отмена, перезапуск, завершение).
//!
//! Захват и развёртка НАМЕРЕННО не идут через `set_status`: load-modify-write
//! вернул бы ровно ту гонку, ради которой они и написаны условным UPDATE'ом.

use anyhow::{anyhow, Result};
use chrono::{Duration, Utc};
use contracts::domain::a017_llm_agent::aggregate::AgentType;
use contracts::domain::a042_agent_task::aggregate::{
    AgentTask, AgentTaskStatus, MAX_DELEGATION_DEPTH, MAX_GLOBAL_BACKLOG,
};
use uuid::Uuid;

use super::repository;

/// Потолок попыток по умолчанию: одна повторная после первого сбоя.
pub const DEFAULT_MAX_ATTEMPTS: i32 = 2;

/// Пауза перед повтором по номеру уже сделанной попытки (минуты).
const BACKOFF_MINUTES: &[i64] = &[5, 20, 60];

/// Параметры постановки поручения. Собираются в LLM-инструменте, но сам
/// `enqueue` от него не зависит — им может пользоваться любой backend-код.
#[derive(Debug, Clone)]
pub struct EnqueueRequest {
    pub title: String,
    pub request_text: String,
    pub target_agent_type: AgentType,
    pub payload_json: Option<String>,
    pub requested_by_agent_ref: Option<String>,
    pub requested_by_chat_ref: Option<String>,
    pub requested_by_user_ref: Option<String>,
    /// Родитель и глубина вычисляются вызывающим (см. `resolve_chain`), а не
    /// приходят от модели.
    pub parent_task_ref: Option<String>,
    pub depth: i32,
}

/// Положение чата в цепочке поручений.
pub struct ChainPosition {
    /// Глубина поручения, которое будет поставлено ИЗ этого чата.
    pub depth: i32,
    /// Поручение, исполнением которого является этот чат.
    pub parent_task_ref: Option<String>,
}

fn short_code() -> String {
    Uuid::new_v4()
        .to_string()
        .chars()
        .take(8)
        .collect::<String>()
        .to_uppercase()
}

fn truncate(value: &str, max: usize) -> String {
    if value.chars().count() <= max {
        return value.to_string();
    }
    value.chars().take(max).collect()
}

/// Где в цепочке находится чат: обычный диалог человека даёт глубину 1,
/// чат исполнения поручения — на единицу больше своего родителя.
pub async fn resolve_chain(chat_id: Option<&str>) -> Result<ChainPosition> {
    let Some(chat_id) = chat_id else {
        return Ok(ChainPosition {
            depth: 1,
            parent_task_ref: None,
        });
    };
    match repository::find_by_execution_chat(chat_id).await? {
        Some(parent) => Ok(ChainPosition {
            depth: parent.depth + 1,
            parent_task_ref: Some(parent.to_string_id()),
        }),
        None => Ok(ChainPosition {
            depth: 1,
            parent_task_ref: None,
        }),
    }
}

/// Поставить поручение в очередь.
///
/// Потолок очереди целиком проверяется **здесь**, а не у вызывающего: это
/// свойство очереди, а не заказчика. Пока он жил в инструменте чата, поручения
/// от Процесса обходили его — молча и в обход того самого правила, ради
/// которого он заведён (один зациклившийся источник не должен уморить всех).
/// Потолок на один чат остался у вызывающего: он про заказчика, а у Этапа
/// заказчика нет.
pub async fn enqueue(request: EnqueueRequest) -> Result<AgentTask> {
    if request.depth > MAX_DELEGATION_DEPTH {
        return Err(anyhow!(
            "Превышена глубина цепочки поручений ({} > {})",
            request.depth,
            MAX_DELEGATION_DEPTH
        ));
    }
    let open = repository::count_open().await?;
    if open >= MAX_GLOBAL_BACKLOG {
        return Err(anyhow!(
            "Очередь поручений переполнена: {open} незакрытых при пределе {MAX_GLOBAL_BACKLOG}"
        ));
    }

    let mut task = AgentTask::new_for_insert(
        format!("AT-{}", short_code()),
        truncate(request.title.trim(), 255),
        request.target_agent_type,
        request.request_text.trim().to_string(),
        DEFAULT_MAX_ATTEMPTS,
    );
    task.payload_json = request.payload_json;
    task.requested_by_agent_ref = request.requested_by_agent_ref;
    task.requested_by_chat_ref = request.requested_by_chat_ref;
    task.requested_by_user_ref = request.requested_by_user_ref;
    task.parent_task_ref = request.parent_task_ref;
    task.depth = request.depth;

    task.validate().map_err(|e| anyhow!(e))?;
    task.before_write();
    repository::insert(&task).await?;
    Ok(task)
}

// ─── Чтение ──────────────────────────────────────────────────────────────────

pub async fn get_by_id(id: &str) -> Result<Option<AgentTask>> {
    repository::find_by_id(id).await
}

pub async fn list_claimable(limit: u64) -> Result<Vec<AgentTask>> {
    repository::list_claimable(limit, &Utc::now().to_rfc3339()).await
}

pub async fn list_for_chat(
    chat_id: &str,
    status: Option<AgentTaskStatus>,
    limit: u64,
) -> Result<Vec<AgentTask>> {
    repository::list_by_requesting_chat(chat_id, status, limit).await
}

pub async fn count_outstanding_for_chat(chat_id: &str) -> Result<u64> {
    repository::count_outstanding_for_chat(chat_id).await
}

pub async fn count_open() -> Result<u64> {
    repository::count_open().await
}

pub async fn find_open_duplicate(
    chat_id: &str,
    target_agent_type: &AgentType,
    request_text: &str,
) -> Result<Option<AgentTask>> {
    repository::find_open_duplicate(chat_id, target_agent_type, request_text).await
}

pub async fn list_paginated(
    limit: u64,
    offset: u64,
    sort_by: &str,
    sort_desc: bool,
    status: Option<&str>,
    target_agent_type: Option<&str>,
    q: Option<&str>,
) -> Result<(Vec<AgentTask>, u64)> {
    repository::list_paginated(
        limit,
        offset,
        sort_by,
        sort_desc,
        status,
        target_agent_type,
        q,
    )
    .await
}

// ─── Переходы очереди ────────────────────────────────────────────────────────

/// Захватить запись прогоном. `false` — запись уже забрал кто-то другой.
pub async fn try_claim(id: &str, session_id: &str) -> Result<bool> {
    repository::try_claim(id, session_id, &Utc::now().to_rfc3339()).await
}

/// Развернуть брошенные прогоны. Возвращает число освобождённых записей.
pub async fn release_stale(stale_minutes: i64) -> Result<u64> {
    let now = Utc::now();
    let stale_before = (now - Duration::minutes(stale_minutes)).to_rfc3339();
    let retry_at = (now + Duration::minutes(BACKOFF_MINUTES[0])).to_rfc3339();
    repository::release_stale(&stale_before, &retry_at).await
}

/// Перевести исчерпавшие попытки в `failed`.
pub async fn fail_exhausted() -> Result<u64> {
    repository::fail_exhausted().await
}

/// Привязать чат исполнения к записи ДО прогона: если процесс умрёт в цикле
/// инструментов, диалог найдётся по самой записи, а не только по логу сессии.
pub async fn attach_execution_chat(
    id: &str,
    chat_id: &str,
    executor_agent_ref: &str,
) -> Result<()> {
    let mut task = load(id).await?;
    task.result_chat_ref = Some(chat_id.to_string());
    task.executor_agent_ref = Some(executor_agent_ref.to_string());
    task.before_write();
    repository::update(&task).await
}

/// Записать успешный результат.
pub async fn mark_done(
    id: &str,
    result_text: String,
    result_message_ref: Option<String>,
    result_artifact_ref: Option<String>,
) -> Result<()> {
    let mut task = load(id).await?;
    ensure_transition(&task, AgentTaskStatus::Done)?;
    task.status = AgentTaskStatus::Done;
    task.result_text = Some(result_text);
    task.result_message_ref = result_message_ref;
    task.result_artifact_ref = result_artifact_ref;
    task.error = None;
    task.finished_at = Some(Utc::now().to_rfc3339());
    task.before_write();
    repository::update(&task).await
}

/// Записать провал. Переповторяемая ошибка при незакончившихся попытках
/// возвращает запись в очередь с паузой, иначе — окончательный `failed`.
pub async fn mark_failed(id: &str, error: &str, retryable: bool) -> Result<()> {
    let mut task = load(id).await?;
    let requeue = retryable && task.attempts < task.max_attempts;
    let target = if requeue {
        AgentTaskStatus::Pending
    } else {
        AgentTaskStatus::Failed
    };
    ensure_transition(&task, target)?;

    task.status = target;
    task.error = Some(truncate(error, 2000));
    task.claim_session_id = None;
    if requeue {
        let idx = (task.attempts.max(1) - 1) as usize;
        let minutes = BACKOFF_MINUTES[idx.min(BACKOFF_MINUTES.len() - 1)];
        task.next_attempt_at = Some((Utc::now() + Duration::minutes(minutes)).to_rfc3339());
        task.finished_at = None;
    } else {
        task.finished_at = Some(Utc::now().to_rfc3339());
    }
    task.before_write();
    repository::update(&task).await
}

/// Снять поручение вручную.
pub async fn cancel(id: &str) -> Result<()> {
    let mut task = load(id).await?;
    ensure_transition(&task, AgentTaskStatus::Cancelled)?;
    task.status = AgentTaskStatus::Cancelled;
    task.claim_session_id = None;
    task.finished_at = Some(Utc::now().to_rfc3339());
    task.before_write();
    repository::update(&task).await
}

/// Вернуть провалившееся/снятое поручение в очередь: сбрасываем счётчик попыток
/// и паузу, иначе `list_claimable` его так и не увидит.
pub async fn requeue(id: &str) -> Result<()> {
    let mut task = load(id).await?;
    ensure_transition(&task, AgentTaskStatus::Pending)?;
    task.status = AgentTaskStatus::Pending;
    task.attempts = 0;
    task.next_attempt_at = None;
    task.claim_session_id = None;
    task.started_at = None;
    task.finished_at = None;
    task.error = None;
    task.before_write();
    repository::update(&task).await
}

pub async fn delete(id: &str) -> Result<()> {
    repository::soft_delete(id).await
}

async fn load(id: &str) -> Result<AgentTask> {
    repository::find_by_id(id)
        .await?
        .ok_or_else(|| anyhow!("Поручение не найдено: {}", id))
}

/// Единственная точка проверки легальности перехода на ручных путях.
fn ensure_transition(task: &AgentTask, to: AgentTaskStatus) -> Result<()> {
    if !task.status.can_transition(&to) {
        return Err(anyhow!(
            "Недопустимый переход поручения {}: {} → {}",
            task.base.code,
            task.status.display_name(),
            to.display_name()
        ));
    }
    Ok(())
}
