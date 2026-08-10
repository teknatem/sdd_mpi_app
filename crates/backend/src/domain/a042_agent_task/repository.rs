//! Доступ к очереди поручений a042.
//!
//! Ключевое отличие от репозиториев-образцов (a031/a039): переходы очереди —
//! `try_claim`, `release_stale`, `fail_exhausted` — выполняются одним условным
//! `UPDATE` с проверкой ожидаемого статуса в `WHERE`, а не парой «прочитать →
//! записать». Read-then-write отдаёт одну и ту же `pending`-строку и регламентному,
//! и ручному прогону, а гонка «воркер дописывает `done` ровно тогда, когда развёртка
//! решила, что запись зависла» стоит второго реального прогона модели.

use chrono::Utc;
use contracts::domain::a017_llm_agent::aggregate::AgentType;
use contracts::domain::a042_agent_task::aggregate::{AgentTask, AgentTaskId, AgentTaskStatus};
use contracts::domain::common::{BaseAggregate, EntityMetadata};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::shared::data::db::get_connection;
use sea_orm::entity::prelude::*;
use sea_orm::{
    ColumnTrait, Condition, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder, QuerySelect, Set,
};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "a042_agent_task")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub code: String,
    pub description: String,
    pub comment: Option<String>,
    pub status: String,
    pub target_agent_type: String,
    pub request_text: String,
    pub payload_json: Option<String>,
    pub requested_by_agent_ref: Option<String>,
    pub requested_by_chat_ref: Option<String>,
    pub requested_by_user_ref: Option<String>,
    pub parent_task_ref: Option<String>,
    pub depth: i32,
    pub attempts: i32,
    pub max_attempts: i32,
    pub next_attempt_at: Option<String>,
    pub claim_session_id: Option<String>,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub executor_agent_ref: Option<String>,
    pub result_chat_ref: Option<String>,
    pub result_message_ref: Option<String>,
    pub result_artifact_ref: Option<String>,
    pub result_text: Option<String>,
    pub error: Option<String>,
    pub is_deleted: bool,
    pub is_posted: bool,
    pub created_at: Option<chrono::DateTime<chrono::Utc>>,
    pub updated_at: Option<chrono::DateTime<chrono::Utc>>,
    pub version: i32,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}

impl From<Model> for AgentTask {
    fn from(m: Model) -> Self {
        let metadata = EntityMetadata {
            created_at: m.created_at.unwrap_or_else(Utc::now),
            updated_at: m.updated_at.unwrap_or_else(Utc::now),
            is_deleted: m.is_deleted,
            is_posted: m.is_posted,
            version: m.version,
        };
        let uuid = Uuid::parse_str(&m.id).unwrap_or_else(|_| Uuid::new_v4());

        AgentTask {
            base: BaseAggregate::with_metadata(
                AgentTaskId(uuid),
                m.code,
                m.description,
                m.comment.clone(),
                metadata,
            ),
            status: AgentTaskStatus::from_str(&m.status),
            target_agent_type: AgentType::from_str(&m.target_agent_type),
            request_text: m.request_text,
            payload_json: m.payload_json,
            requested_by_agent_ref: m.requested_by_agent_ref,
            requested_by_chat_ref: m.requested_by_chat_ref,
            requested_by_user_ref: m.requested_by_user_ref,
            parent_task_ref: m.parent_task_ref,
            depth: m.depth,
            attempts: m.attempts,
            max_attempts: m.max_attempts,
            next_attempt_at: m.next_attempt_at,
            claim_session_id: m.claim_session_id,
            started_at: m.started_at,
            finished_at: m.finished_at,
            executor_agent_ref: m.executor_agent_ref,
            result_chat_ref: m.result_chat_ref,
            result_message_ref: m.result_message_ref,
            result_artifact_ref: m.result_artifact_ref,
            result_text: m.result_text,
            error: m.error,
        }
    }
}

fn conn() -> &'static DatabaseConnection {
    get_connection()
}

fn to_active(item: &AgentTask, is_insert: bool) -> ActiveModel {
    let now = Utc::now();
    ActiveModel {
        id: Set(item.to_string_id()),
        code: Set(item.base.code.clone()),
        description: Set(item.base.description.clone()),
        comment: Set(item.base.comment.clone()),
        status: Set(item.status.as_str().to_string()),
        target_agent_type: Set(item.target_agent_type.as_str().to_string()),
        request_text: Set(item.request_text.clone()),
        payload_json: Set(item.payload_json.clone()),
        requested_by_agent_ref: Set(item.requested_by_agent_ref.clone()),
        requested_by_chat_ref: Set(item.requested_by_chat_ref.clone()),
        requested_by_user_ref: Set(item.requested_by_user_ref.clone()),
        parent_task_ref: Set(item.parent_task_ref.clone()),
        depth: Set(item.depth),
        attempts: Set(item.attempts),
        max_attempts: Set(item.max_attempts),
        next_attempt_at: Set(item.next_attempt_at.clone()),
        claim_session_id: Set(item.claim_session_id.clone()),
        started_at: Set(item.started_at.clone()),
        finished_at: Set(item.finished_at.clone()),
        executor_agent_ref: Set(item.executor_agent_ref.clone()),
        result_chat_ref: Set(item.result_chat_ref.clone()),
        result_message_ref: Set(item.result_message_ref.clone()),
        result_artifact_ref: Set(item.result_artifact_ref.clone()),
        result_text: Set(item.result_text.clone()),
        error: Set(item.error.clone()),
        is_deleted: Set(item.base.metadata.is_deleted),
        is_posted: Set(false),
        created_at: Set(Some(if is_insert {
            now
        } else {
            item.base.metadata.created_at
        })),
        updated_at: Set(Some(now)),
        version: Set(if is_insert {
            1
        } else {
            item.base.metadata.version + 1
        }),
    }
}

// ─── Чтение ──────────────────────────────────────────────────────────────────

pub async fn find_by_id(id: &str) -> anyhow::Result<Option<AgentTask>> {
    let model = Entity::find_by_id(id.to_string())
        .filter(Column::IsDeleted.eq(false))
        .one(conn())
        .await?;
    Ok(model.map(Into::into))
}

/// Кандидаты на исполнение: FIFO по `created_at`, с учётом гейта бэкоффа и
/// исчерпанных попыток. Читаем-потом-захватываем безопасно: захват условный,
/// проигравший гонку просто получает `false` и пропускает запись.
pub async fn list_claimable(limit: u64, now: &str) -> anyhow::Result<Vec<AgentTask>> {
    let items: Vec<AgentTask> = Entity::find()
        .filter(Column::IsDeleted.eq(false))
        .filter(Column::Status.eq(AgentTaskStatus::Pending.as_str()))
        .filter(
            Condition::any()
                .add(Column::NextAttemptAt.is_null())
                .add(Column::NextAttemptAt.lte(now.to_string())),
        )
        .filter(Expr::col(Column::Attempts).lt(Expr::col(Column::MaxAttempts)))
        .order_by_asc(Column::CreatedAt)
        .limit(limit)
        .all(conn())
        .await?
        .into_iter()
        .map(Into::into)
        .collect();
    Ok(items)
}

/// Поручения, поставленные из указанного чата (заказчик забирает свой результат).
pub async fn list_by_requesting_chat(
    chat_id: &str,
    status: Option<AgentTaskStatus>,
    limit: u64,
) -> anyhow::Result<Vec<AgentTask>> {
    let mut query = Entity::find()
        .filter(Column::IsDeleted.eq(false))
        .filter(Column::RequestedByChatRef.eq(chat_id.to_string()));
    if let Some(status) = status {
        query = query.filter(Column::Status.eq(status.as_str()));
    }
    let items: Vec<AgentTask> = query
        .order_by_desc(Column::CreatedAt)
        .limit(limit)
        .all(conn())
        .await?
        .into_iter()
        .map(Into::into)
        .collect();
    Ok(items)
}

/// Поручение, исполнением которого является данный чат.
/// Обратный поиск для расчёта глубины цепочки при постановке нового поручения.
pub async fn find_by_execution_chat(chat_id: &str) -> anyhow::Result<Option<AgentTask>> {
    let model = Entity::find()
        .filter(Column::IsDeleted.eq(false))
        .filter(Column::ResultChatRef.eq(chat_id.to_string()))
        .one(conn())
        .await?;
    Ok(model.map(Into::into))
}

/// Сколько незакрытых поручений уже поставлено из этого чата.
pub async fn count_outstanding_for_chat(chat_id: &str) -> anyhow::Result<u64> {
    let open: Vec<String> = AgentTaskStatus::OPEN
        .iter()
        .map(|s| s.as_str().to_string())
        .collect();
    let count = Entity::find()
        .filter(Column::IsDeleted.eq(false))
        .filter(Column::RequestedByChatRef.eq(chat_id.to_string()))
        .filter(Column::Status.is_in(open))
        .count(conn())
        .await?;
    Ok(count)
}

/// Размер очереди целиком — защита от одного зациклившегося чата.
pub async fn count_open() -> anyhow::Result<u64> {
    let open: Vec<String> = AgentTaskStatus::OPEN
        .iter()
        .map(|s| s.as_str().to_string())
        .collect();
    let count = Entity::find()
        .filter(Column::IsDeleted.eq(false))
        .filter(Column::Status.is_in(open))
        .count(conn())
        .await?;
    Ok(count)
}

/// Незакрытый дубль того же поручения из того же чата.
/// Модель, зовущая инструмент дважды за один цикл, — классика; отвечать ей
/// ошибкой хуже, чем вернуть уже созданную запись.
pub async fn find_open_duplicate(
    chat_id: &str,
    target_agent_type: &AgentType,
    request_text: &str,
) -> anyhow::Result<Option<AgentTask>> {
    let open: Vec<String> = AgentTaskStatus::OPEN
        .iter()
        .map(|s| s.as_str().to_string())
        .collect();
    let model = Entity::find()
        .filter(Column::IsDeleted.eq(false))
        .filter(Column::RequestedByChatRef.eq(chat_id.to_string()))
        .filter(Column::TargetAgentType.eq(target_agent_type.as_str()))
        .filter(Column::RequestText.eq(request_text.to_string()))
        .filter(Column::Status.is_in(open))
        .one(conn())
        .await?;
    Ok(model.map(Into::into))
}

pub async fn list_paginated(
    limit: u64,
    offset: u64,
    sort_by: &str,
    sort_desc: bool,
    status: Option<&str>,
    target_agent_type: Option<&str>,
    q: Option<&str>,
) -> anyhow::Result<(Vec<AgentTask>, u64)> {
    let base = || {
        let mut query = Entity::find().filter(Column::IsDeleted.eq(false));
        if let Some(status) = status {
            query = query.filter(Column::Status.eq(status.to_string()));
        }
        if let Some(agent_type) = target_agent_type {
            query = query.filter(Column::TargetAgentType.eq(agent_type.to_string()));
        }
        if let Some(needle) = q {
            let pattern = format!("%{}%", needle);
            query = query.filter(
                Condition::any()
                    .add(Column::Code.like(&pattern))
                    .add(Column::Description.like(&pattern))
                    .add(Column::RequestText.like(&pattern)),
            );
        }
        query
    };

    let total = base().count(conn()).await?;

    let query = match (sort_by, sort_desc) {
        ("status", true) => base().order_by_desc(Column::Status),
        ("status", false) => base().order_by_asc(Column::Status),
        ("target_agent_type", true) => base().order_by_desc(Column::TargetAgentType),
        ("target_agent_type", false) => base().order_by_asc(Column::TargetAgentType),
        ("finished_at", true) => base().order_by_desc(Column::FinishedAt),
        ("finished_at", false) => base().order_by_asc(Column::FinishedAt),
        (_, false) => base().order_by_asc(Column::CreatedAt),
        (_, true) => base().order_by_desc(Column::CreatedAt),
    };

    let items: Vec<AgentTask> = query
        .offset(offset)
        .limit(limit)
        .all(conn())
        .await?
        .into_iter()
        .map(Into::into)
        .collect();

    Ok((items, total))
}

// ─── Запись ──────────────────────────────────────────────────────────────────

pub async fn insert(item: &AgentTask) -> anyhow::Result<()> {
    Entity::insert(to_active(item, true)).exec(conn()).await?;
    Ok(())
}

pub async fn update(item: &AgentTask) -> anyhow::Result<()> {
    Entity::update(to_active(item, false)).exec(conn()).await?;
    Ok(())
}

pub async fn soft_delete(id: &str) -> anyhow::Result<()> {
    Entity::update_many()
        .col_expr(Column::IsDeleted, Expr::value(true))
        .col_expr(Column::UpdatedAt, Expr::value(Some(Utc::now())))
        .filter(Column::Id.eq(id))
        .exec(conn())
        .await?;
    Ok(())
}

// ─── Переходы очереди (условные UPDATE) ──────────────────────────────────────

/// Атомарный захват записи прогоном. `true` — захват наш.
///
/// Счётчик попыток растёт ЗДЕСЬ, а не при провале: воркер, убитый в середине
/// прогона (рестарт, OOM), попытку уже потратил — иначе ядовитая запись,
/// роняющая процесс, крутится в очереди вечно.
pub async fn try_claim(id: &str, session_id: &str, now: &str) -> anyhow::Result<bool> {
    let res = Entity::update_many()
        .col_expr(
            Column::Status,
            Expr::value(AgentTaskStatus::Processing.as_str()),
        )
        .col_expr(
            Column::ClaimSessionId,
            Expr::value(Some(session_id.to_string())),
        )
        .col_expr(Column::StartedAt, Expr::value(Some(now.to_string())))
        .col_expr(Column::FinishedAt, Expr::value(Option::<String>::None))
        .col_expr(Column::Error, Expr::value(Option::<String>::None))
        .col_expr(Column::Attempts, Expr::col(Column::Attempts).add(1))
        .col_expr(Column::UpdatedAt, Expr::value(Some(Utc::now())))
        .filter(Column::Id.eq(id))
        // Гонка закрывается здесь: у проигравшего rows_affected = 0.
        .filter(Column::Status.eq(AgentTaskStatus::Pending.as_str()))
        .filter(Column::IsDeleted.eq(false))
        .exec(conn())
        .await?;
    Ok(res.rows_affected == 1)
}

/// Развернуть прогоны, брошенные упавшим воркером: `processing` со `started_at`
/// старше порога возвращается в очередь с отметкой причины.
///
/// Фильтр по `status = processing` обязателен: без него развёртка стаптывает
/// запись, которую воркер только что довёл до `done`, и поручение исполняется
/// второй раз за реальные деньги.
pub async fn release_stale(stale_before: &str, retry_at: &str) -> anyhow::Result<u64> {
    let res = Entity::update_many()
        .col_expr(
            Column::Status,
            Expr::value(AgentTaskStatus::Pending.as_str()),
        )
        .col_expr(Column::ClaimSessionId, Expr::value(Option::<String>::None))
        .col_expr(
            Column::NextAttemptAt,
            Expr::value(Some(retry_at.to_string())),
        )
        .col_expr(
            Column::Error,
            Expr::value(Some(
                "Прогон прерван: запись висела в статусе «исполняется» дольше допустимого"
                    .to_string(),
            )),
        )
        .col_expr(Column::UpdatedAt, Expr::value(Some(Utc::now())))
        .filter(Column::IsDeleted.eq(false))
        .filter(Column::Status.eq(AgentTaskStatus::Processing.as_str()))
        .filter(Column::StartedAt.lt(stale_before.to_string()))
        .exec(conn())
        .await?;
    Ok(res.rows_affected)
}

/// Добить записи, исчерпавшие попытки: из `pending` в `failed`.
///
/// Без этого они выпадают из `list_claimable` и превращаются в невидимый
/// «вечно ожидающий» хвост, которого нет ни в одном отчёте.
pub async fn fail_exhausted() -> anyhow::Result<u64> {
    let now = Utc::now();
    let res = Entity::update_many()
        .col_expr(
            Column::Status,
            Expr::value(AgentTaskStatus::Failed.as_str()),
        )
        .col_expr(Column::FinishedAt, Expr::value(Some(now.to_rfc3339())))
        .col_expr(
            Column::Error,
            Expr::cust("COALESCE(error, 'Исчерпаны попытки исполнения')"),
        )
        .col_expr(Column::UpdatedAt, Expr::value(Some(now)))
        .filter(Column::IsDeleted.eq(false))
        .filter(Column::Status.eq(AgentTaskStatus::Pending.as_str()))
        .filter(Expr::col(Column::Attempts).gte(Expr::col(Column::MaxAttempts)))
        .exec(conn())
        .await?;
    Ok(res.rows_affected)
}
