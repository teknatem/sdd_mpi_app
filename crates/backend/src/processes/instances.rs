//! Хранилище экземпляров процессов.
//!
//! Здесь запросы и переходы состояния, но не решения: что делать после выхода
//! Этапа, решает `worker.rs`.
//!
//! Всё, что связано с гонкой, сделано **условным `UPDATE`**, а не чтением с
//! последующей записью. Причина простая: воркеров может быть больше одного —
//! после перезапуска, при двух процессах, при развёртке зависшей аренды. Читать
//! и потом писать значит проиграть эту гонку тихо, а условный `UPDATE`
//! возвращает число затронутых строк, и проигравший это видит.

use anyhow::Result;
use chrono::{DateTime, Utc};
use contracts::processes::{
    CorrelationKey, EdgeTarget, InstanceStatus, InstanceWait, ProcessInstance,
};
use sea_orm::entity::prelude::*;
use sea_orm::sea_query::Expr;
use sea_orm::{
    ColumnTrait, Condition, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder, QuerySelect,
    Set,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "sys_process_instance")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub process_code: String,
    pub process_version: i32,
    pub correlation_json: String,
    pub correlation_token: String,
    pub status: String,
    #[sea_orm(nullable)]
    pub stage_code: Option<String>,
    pub visit: i32,
    pub input_json: String,
    pub attempts: i32,
    #[sea_orm(nullable)]
    pub next_attempt_at: Option<String>,
    #[sea_orm(nullable)]
    pub wait_event: Option<String>,
    #[sea_orm(nullable)]
    pub wait_token: Option<String>,
    #[sea_orm(nullable)]
    pub wait_since_seq: Option<i64>,
    #[sea_orm(nullable)]
    pub wait_deadline_at: Option<String>,
    #[sea_orm(nullable)]
    pub wait_on_timeout_json: Option<String>,
    #[sea_orm(nullable)]
    pub last_outcome: Option<String>,
    #[sea_orm(nullable)]
    pub last_error: Option<String>,
    #[sea_orm(nullable)]
    pub claim_session_id: Option<String>,
    #[sea_orm(nullable)]
    pub claimed_at: Option<String>,
    pub started_at: String,
    pub updated_at: String,
    #[sea_orm(nullable)]
    pub finished_at: Option<String>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}

impl From<Model> for ProcessInstance {
    fn from(row: Model) -> Self {
        let wait = match (row.wait_event, row.wait_token, row.wait_deadline_at) {
            (Some(event), Some(token), Some(deadline_at)) => Some(InstanceWait {
                event,
                token,
                since_seq: row.wait_since_seq.unwrap_or(0),
                deadline_at,
                on_timeout: row
                    .wait_on_timeout_json
                    .as_deref()
                    .and_then(|raw| serde_json::from_str::<EdgeTarget>(raw).ok()),
            }),
            _ => None,
        };
        ProcessInstance {
            id: row.id,
            process_code: row.process_code,
            process_version: row.process_version,
            correlation: serde_json::from_str(&row.correlation_json).unwrap_or_default(),
            correlation_token: row.correlation_token,
            status: InstanceStatus::from_str(&row.status),
            stage_code: row.stage_code,
            visit: row.visit,
            input: serde_json::from_str(&row.input_json).unwrap_or(Value::Null),
            attempts: row.attempts,
            next_attempt_at: row.next_attempt_at,
            wait,
            last_outcome: row.last_outcome,
            last_error: row.last_error,
            claim_session_id: row.claim_session_id,
            started_at: row.started_at,
            updated_at: row.updated_at,
            finished_at: row.finished_at,
        }
    }
}

/// Завести экземпляр.
///
/// `Ok(None)` — живой экземпляр с таким ключом уже есть, и это **штатный**
/// исход, а не ошибка: событие про один и тот же день может прийти дважды, и
/// второй прогон был бы вторым набором эффектов. Разводит их уникальный индекс
/// БД, поэтому две одновременные попытки тоже разойдутся.
pub async fn start(
    db: &DatabaseConnection,
    process_code: &str,
    process_version: i32,
    correlation: &CorrelationKey,
    correlation_token: &str,
    entry_stage: &str,
    input: &Value,
) -> Result<Option<ProcessInstance>> {
    if find_live(db, process_code, correlation_token)
        .await?
        .is_some()
    {
        return Ok(None);
    }
    let now = Utc::now().to_rfc3339();
    let row = ActiveModel {
        id: Set(Uuid::new_v4().to_string()),
        process_code: Set(process_code.to_string()),
        process_version: Set(process_version),
        correlation_json: Set(serde_json::to_string(correlation)?),
        correlation_token: Set(correlation_token.to_string()),
        status: Set(InstanceStatus::Running.as_str().to_string()),
        stage_code: Set(Some(entry_stage.to_string())),
        visit: Set(1),
        input_json: Set(input.to_string()),
        attempts: Set(0),
        next_attempt_at: Set(None),
        wait_event: Set(None),
        wait_token: Set(None),
        wait_since_seq: Set(None),
        wait_deadline_at: Set(None),
        wait_on_timeout_json: Set(None),
        last_outcome: Set(None),
        last_error: Set(None),
        claim_session_id: Set(None),
        claimed_at: Set(None),
        started_at: Set(now.clone()),
        updated_at: Set(now),
        finished_at: Set(None),
    };
    match row.insert(db).await {
        Ok(model) => Ok(Some(model.into())),
        // Гонку на уникальном индексе проигравший читает как «уже есть».
        Err(error) if is_unique_violation(&error) => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn is_unique_violation(error: &DbErr) -> bool {
    let text = error.to_string().to_ascii_lowercase();
    text.contains("unique") || text.contains("constraint")
}

pub async fn find(db: &DatabaseConnection, id: &str) -> Result<Option<ProcessInstance>> {
    Ok(Entity::find_by_id(id.to_string())
        .one(db)
        .await?
        .map(ProcessInstance::from))
}

/// Живой экземпляр по паре «Процесс + ключ».
pub async fn find_live(
    db: &DatabaseConnection,
    process_code: &str,
    correlation_token: &str,
) -> Result<Option<ProcessInstance>> {
    Ok(Entity::find()
        .filter(Column::ProcessCode.eq(process_code))
        .filter(Column::CorrelationToken.eq(correlation_token))
        .filter(live_condition())
        .one(db)
        .await?
        .map(ProcessInstance::from))
}

fn live_condition() -> Condition {
    Condition::any()
        .add(Column::Status.eq(InstanceStatus::Running.as_str()))
        .add(Column::Status.eq(InstanceStatus::Waiting.as_str()))
}

/// Экземпляры, которые пора двигать: не арендованы и время повтора настало.
pub async fn list_runnable(
    db: &DatabaseConnection,
    now: &str,
    limit: u64,
) -> Result<Vec<ProcessInstance>> {
    Ok(Entity::find()
        .filter(Column::Status.eq(InstanceStatus::Running.as_str()))
        .filter(Column::ClaimSessionId.is_null())
        .filter(
            Condition::any()
                .add(Column::NextAttemptAt.is_null())
                .add(Column::NextAttemptAt.lte(now)),
        )
        .order_by_asc(Column::StartedAt)
        .limit(limit)
        .all(db)
        .await?
        .into_iter()
        .map(ProcessInstance::from)
        .collect())
}

/// Ожидающие экземпляры, у которых вышел дедлайн.
pub async fn list_expired_waits(
    db: &DatabaseConnection,
    now: &str,
    limit: u64,
) -> Result<Vec<ProcessInstance>> {
    Ok(Entity::find()
        .filter(Column::Status.eq(InstanceStatus::Waiting.as_str()))
        .filter(Column::WaitDeadlineAt.lte(now))
        .order_by_asc(Column::WaitDeadlineAt)
        .limit(limit)
        .all(db)
        .await?
        .into_iter()
        .map(ProcessInstance::from)
        .collect())
}

/// Ожидающие экземпляры — инбокс и экран механизма.
pub async fn list_waiting(db: &DatabaseConnection, limit: u64) -> Result<Vec<ProcessInstance>> {
    Ok(Entity::find()
        .filter(Column::Status.eq(InstanceStatus::Waiting.as_str()))
        .order_by_asc(Column::WaitDeadlineAt)
        .limit(limit)
        .all(db)
        .await?
        .into_iter()
        .map(ProcessInstance::from)
        .collect())
}

/// Экземпляры, ждущие конкретного события с конкретным токеном.
pub async fn list_waiting_for(
    db: &DatabaseConnection,
    event: &str,
    token: &str,
) -> Result<Vec<ProcessInstance>> {
    Ok(Entity::find()
        .filter(Column::Status.eq(InstanceStatus::Waiting.as_str()))
        .filter(Column::WaitEvent.eq(event))
        .filter(Column::WaitToken.eq(token))
        .all(db)
        .await?
        .into_iter()
        .map(ProcessInstance::from)
        .collect())
}

/// Список для экрана: свежие сверху.
pub async fn list_recent(db: &DatabaseConnection, limit: u64) -> Result<Vec<ProcessInstance>> {
    Ok(Entity::find()
        .order_by_desc(Column::StartedAt)
        .limit(limit)
        .all(db)
        .await?
        .into_iter()
        .map(ProcessInstance::from)
        .collect())
}

/// Взять экземпляр в работу.
///
/// Условный `UPDATE`: аренда ставится только на неарендованный `running`.
/// Проигравший гонку получает `false` и идёт дальше — это и есть то, что делает
/// незавершённую запись в журнале эффектов признаком смерти воркера, а не
/// признаком конкуренции.
pub async fn try_claim(db: &DatabaseConnection, id: &str, session_id: &str) -> Result<bool> {
    let now = Utc::now().to_rfc3339();
    let result = Entity::update_many()
        .col_expr(Column::ClaimSessionId, Expr::value(Some(session_id)))
        .col_expr(Column::ClaimedAt, Expr::value(Some(now.clone())))
        .col_expr(Column::UpdatedAt, Expr::value(now))
        .filter(Column::Id.eq(id))
        .filter(Column::Status.eq(InstanceStatus::Running.as_str()))
        .filter(Column::ClaimSessionId.is_null())
        .exec(db)
        .await?;
    Ok(result.rows_affected == 1)
}

/// Снять аренду, брошенную упавшим воркером.
///
/// Фильтр по `status = running` обязателен: без него развёртка стаптывает
/// экземпляр, который воркер только что довёл до ожидания или до конца.
pub async fn release_stale(db: &DatabaseConnection, claimed_before: &str) -> Result<u64> {
    let result = Entity::update_many()
        .col_expr(Column::ClaimSessionId, Expr::value(Option::<String>::None))
        .col_expr(Column::ClaimedAt, Expr::value(Option::<String>::None))
        .col_expr(Column::UpdatedAt, Expr::value(Utc::now().to_rfc3339()))
        .filter(Column::Status.eq(InstanceStatus::Running.as_str()))
        .filter(Column::ClaimSessionId.is_not_null())
        .filter(Column::ClaimedAt.lt(claimed_before))
        .exec(db)
        .await?;
    Ok(result.rows_affected)
}

/// Освободить аренду своего экземпляра.
pub async fn release(db: &DatabaseConnection, id: &str) -> Result<()> {
    Entity::update_many()
        .col_expr(Column::ClaimSessionId, Expr::value(Option::<String>::None))
        .col_expr(Column::ClaimedAt, Expr::value(Option::<String>::None))
        .col_expr(Column::UpdatedAt, Expr::value(Utc::now().to_rfc3339()))
        .filter(Column::Id.eq(id))
        .exec(db)
        .await?;
    Ok(())
}

/// Перевести курсор на следующий Этап.
///
/// Номер захода растёт всегда, даже когда Этап тот же: цикл в графе — это новый
/// заход, и ключи идемпотентности его эффектов обязаны отличаться.
pub async fn advance(
    db: &DatabaseConnection,
    id: &str,
    stage_code: &str,
    input: &Value,
    outcome: &str,
) -> Result<()> {
    let mut active = load_active(db, id).await?;
    // Номер захода читаем из уже загруженной строки: `ActiveModel` после
    // `into()` держит значения, но брать их через `unwrap` значит поставить
    // панику там, где хватает нуля.
    let visit = active.visit.clone().take().unwrap_or(0);
    active.stage_code = Set(Some(stage_code.to_string()));
    active.visit = Set(visit + 1);
    active.input_json = Set(input.to_string());
    active.status = Set(InstanceStatus::Running.as_str().to_string());
    active.attempts = Set(0);
    active.next_attempt_at = Set(None);
    active.last_outcome = Set(Some(outcome.to_string()));
    active.last_error = Set(None);
    clear_wait(&mut active);
    clear_claim(&mut active);
    active.updated_at = Set(Utc::now().to_rfc3339());
    active.update(db).await?;
    Ok(())
}

/// Поставить экземпляр в ожидание события.
///
/// `next_input` — уже подготовленный вход того Этапа, в который экземпляр
/// уйдёт после пробуждения. Он кладётся в `input_json` сразу: текущий Этап
/// отработал, его собственный вход больше не нужен, а хранить подготовленный
/// проще, чем восстанавливать данные выхода через сутки ожидания.
pub async fn begin_wait(
    db: &DatabaseConnection,
    id: &str,
    wait: &InstanceWait,
    outcome: &str,
    next_input: &Value,
) -> Result<()> {
    let mut active = load_active(db, id).await?;
    active.status = Set(InstanceStatus::Waiting.as_str().to_string());
    active.input_json = Set(next_input.to_string());
    active.wait_event = Set(Some(wait.event.clone()));
    active.wait_token = Set(Some(wait.token.clone()));
    active.wait_since_seq = Set(Some(wait.since_seq));
    active.wait_deadline_at = Set(Some(wait.deadline_at.clone()));
    active.wait_on_timeout_json = Set(wait
        .on_timeout
        .as_ref()
        .map(|target| serde_json::to_string(target).unwrap_or_default()));
    active.last_outcome = Set(Some(outcome.to_string()));
    active.attempts = Set(0);
    active.next_attempt_at = Set(None);
    clear_claim(&mut active);
    active.updated_at = Set(Utc::now().to_rfc3339());
    active.update(db).await?;
    Ok(())
}

/// Разбудить ожидающий экземпляр: курсор встаёт на Этап, ожидание снимается.
///
/// Условный `UPDATE` по статусу: два события с одним токеном могут прийти
/// одновременно, и разбудить экземпляр должно ровно одно.
pub async fn wake(
    db: &DatabaseConnection,
    id: &str,
    stage_code: &str,
    input: &Value,
) -> Result<bool> {
    let now = Utc::now().to_rfc3339();
    let result = Entity::update_many()
        .col_expr(
            Column::Status,
            Expr::value(InstanceStatus::Running.as_str()),
        )
        .col_expr(Column::StageCode, Expr::value(Some(stage_code)))
        .col_expr(Column::Visit, Expr::col(Column::Visit).add(1))
        .col_expr(Column::InputJson, Expr::value(input.to_string()))
        .col_expr(Column::WaitEvent, Expr::value(Option::<String>::None))
        .col_expr(Column::WaitToken, Expr::value(Option::<String>::None))
        .col_expr(Column::WaitSinceSeq, Expr::value(Option::<i64>::None))
        .col_expr(Column::WaitDeadlineAt, Expr::value(Option::<String>::None))
        .col_expr(
            Column::WaitOnTimeoutJson,
            Expr::value(Option::<String>::None),
        )
        .col_expr(Column::Attempts, Expr::value(0))
        .col_expr(Column::NextAttemptAt, Expr::value(Option::<String>::None))
        .col_expr(Column::UpdatedAt, Expr::value(now))
        .filter(Column::Id.eq(id))
        .filter(Column::Status.eq(InstanceStatus::Waiting.as_str()))
        .exec(db)
        .await?;
    Ok(result.rows_affected == 1)
}

/// Отложить повтор Этапа после временного сбоя.
pub async fn schedule_retry(
    db: &DatabaseConnection,
    id: &str,
    attempts: i32,
    next_attempt_at: DateTime<Utc>,
    message: &str,
) -> Result<()> {
    let mut active = load_active(db, id).await?;
    active.attempts = Set(attempts);
    active.next_attempt_at = Set(Some(next_attempt_at.to_rfc3339()));
    active.last_error = Set(Some(message.to_string()));
    clear_claim(&mut active);
    active.updated_at = Set(Utc::now().to_rfc3339());
    active.update(db).await?;
    Ok(())
}

/// Увести экземпляр в карантин: дальше нужен человек.
pub async fn quarantine(db: &DatabaseConnection, id: &str, message: &str) -> Result<()> {
    let mut active = load_active(db, id).await?;
    active.status = Set(InstanceStatus::Quarantined.as_str().to_string());
    active.last_error = Set(Some(message.to_string()));
    active.next_attempt_at = Set(None);
    clear_claim(&mut active);
    let now = Utc::now().to_rfc3339();
    active.updated_at = Set(now.clone());
    active.finished_at = Set(Some(now));
    active.update(db).await?;
    Ok(())
}

/// Завершить экземпляр штатно.
pub async fn finish(db: &DatabaseConnection, id: &str, outcome: &str) -> Result<()> {
    let mut active = load_active(db, id).await?;
    active.status = Set(InstanceStatus::Done.as_str().to_string());
    active.stage_code = Set(None);
    active.last_outcome = Set(Some(outcome.to_string()));
    active.next_attempt_at = Set(None);
    clear_wait(&mut active);
    clear_claim(&mut active);
    let now = Utc::now().to_rfc3339();
    active.updated_at = Set(now.clone());
    active.finished_at = Set(Some(now));
    active.update(db).await?;
    Ok(())
}

async fn load_active(db: &DatabaseConnection, id: &str) -> Result<ActiveModel> {
    let Some(row) = Entity::find_by_id(id.to_string()).one(db).await? else {
        anyhow::bail!("экземпляр процесса не найден: {id}");
    };
    Ok(row.into())
}

fn clear_wait(active: &mut ActiveModel) {
    active.wait_event = Set(None);
    active.wait_token = Set(None);
    active.wait_since_seq = Set(None);
    active.wait_deadline_at = Set(None);
    active.wait_on_timeout_json = Set(None);
}

fn clear_claim(active: &mut ActiveModel) {
    active.claim_session_id = Set(None);
    active.claimed_at = Set(None);
}
