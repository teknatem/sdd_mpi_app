//! Воркер экземпляров — то, что двигает Процессы.
//!
//! Один проход (`tick`) делает четыре вещи в этом порядке:
//!
//! 1. **снимает брошенные аренды** — воркер мог упасть, и его экземпляры иначе
//!    остались бы занятыми навсегда;
//! 2. **разбирает новые события**: стартует экземпляры по триггерам и будит
//!    ожидающих (ADR-0011 п.5, п.9);
//! 3. **исполняет очередной Этап** у готовых экземпляров;
//! 4. **разбирает просроченные ожидания**: публикует `process.instance.timeout`
//!    и уводит экземпляр либо по запасному ребру, либо к человеку.
//!
//! Воркер **независим от флага планировщика** (ADR-0011 п.12):
//! `[scheduled_tasks].enabled = false` остаётся выключенным и остаётся не
//! тронут — тридцать регламентных заданий не имеют к Процессам отношения. А вот
//! maintenance-гард общий: операция переноса БД должна получить тихое окно.
//!
//! Координатор ресурсов тоже общий, и это не формальность: Действие объявляет
//! таблицы, в которые пишет (`ActionInfo::write_tables`), и перепроведение из
//! Процесса не должно идти одновременно с импортом в те же таблицы.

use std::collections::{BTreeMap, HashMap};

use anyhow::Result;
use chrono::{DateTime, Duration, Utc};
use contracts::processes::{
    ActionMode, CorrelationKey, DomainEvent, DomainEventKind, EdgeTarget, InstanceWait,
    ProcessInstance, ProcessManifest, StageDefinition, StageRunContext, StageVerdict, WaitSpec,
};
use sea_orm::{ConnectionTrait, DatabaseBackend, DatabaseConnection, Statement};
use serde_json::{Map, Value};
use tracing::{info, warn};
use uuid::Uuid;

use crate::processes::{actions, definitions, events, instances, repository, stages, steps};
use crate::system::tasks::resource_coordinator::get_global_resource_coordinator;

/// Сколько раз повторяем Этап после временного сбоя, прежде чем звать человека.
pub const MAX_ATTEMPTS: i32 = 5;

/// Сколько экземпляров двигаем за один проход. Ограничение есть, чтобы проход
/// оставался коротким: длинный проход держит maintenance-окно закрытым.
const BATCH: u64 = 20;

/// Аренда, которую никто не обновил дольше этого, считается брошенной.
const LEASE_MINUTES: i64 = 30;

/// Ключ курсора по журналу событий в `sys_settings`.
const CURSOR_KEY: &str = "processes.event_cursor";

/// Что сделал один проход — для логов и тестов.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct TickReport {
    pub released: u64,
    pub started: usize,
    pub woken: usize,
    pub stages_run: usize,
    pub expired: usize,
    pub quarantined: usize,
}

impl TickReport {
    pub fn is_idle(&self) -> bool {
        self.released == 0
            && self.started == 0
            && self.woken == 0
            && self.stages_run == 0
            && self.expired == 0
    }
}

/// Фоновый воркер.
pub struct ProcessWorker {
    interval_seconds: u64,
}

impl ProcessWorker {
    pub fn new(interval_seconds: u64) -> Self {
        Self { interval_seconds }
    }

    pub async fn run_loop(&self, db: &'static DatabaseConnection) {
        info!(
            "Process worker started (interval {}s)",
            self.interval_seconds
        );
        let mut interval =
            tokio::time::interval(tokio::time::Duration::from_secs(self.interval_seconds));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            match tick(db).await {
                Ok(report) if report.is_idle() => {}
                Ok(report) => info!("Process worker pass: {report:?}"),
                Err(error) => tracing::error!("Process worker pass failed: {error:?}"),
            }
        }
    }
}

/// Один проход воркера. Отдельная функция, потому что её зовут и тесты: проход
/// обязан быть проверяемым без запуска цикла и ожидания таймера.
pub async fn tick(db: &'static DatabaseConnection) -> Result<TickReport> {
    let mut report = TickReport::default();

    // В maintenance не трогаем ничего: переносу БД нужно полностью тихое окно.
    let Some(_activity) = crate::system::maintenance::try_begin_database_activity() else {
        return Ok(report);
    };

    let now = Utc::now();
    report.released =
        instances::release_stale(db, &(now - Duration::minutes(LEASE_MINUTES)).to_rfc3339())
            .await?;

    dispatch_events(db, &mut report).await?;
    run_due_instances(db, &mut report, now).await?;
    expire_waits(db, &mut report, now).await?;

    Ok(report)
}

// ---------------------------------------------------------------------------
// События: старт и пробуждение
// ---------------------------------------------------------------------------

/// Разобрать события, до которых воркер ещё не дошёл.
///
/// Курсор двигается по номеру публикации и переживает перезапуск: иначе после
/// рестарта воркер либо перечитал бы весь журнал, либо потерял бы события,
/// пришедшие, пока его не было.
async fn dispatch_events(db: &'static DatabaseConnection, report: &mut TickReport) -> Result<()> {
    let cursor = read_cursor(db).await?;
    let events = events::list_since(db, cursor, BATCH).await?;
    if events.is_empty() {
        return Ok(());
    }

    let active = repository::list_active_processes(db).await?;
    let mut last_seq = cursor;
    for event in events {
        for process in &active {
            if process.definition.manifest.trigger.event == event.kind.as_str() {
                if start_instance(
                    db,
                    &process.code,
                    process.version,
                    &process.definition.manifest,
                    &event,
                )
                .await?
                {
                    report.started += 1;
                }
            }
        }
        report.woken += wake_waiting(db, &event).await?;
        last_seq = event.seq;
    }
    write_cursor(db, last_seq).await?;
    Ok(())
}

/// Завести экземпляр по факту.
async fn start_instance(
    db: &'static DatabaseConnection,
    process_code: &str,
    process_version: i32,
    manifest: &ProcessManifest,
    event: &DomainEvent,
) -> Result<bool> {
    let input = correlation_object(&event.correlation);
    let started = instances::start(
        db,
        process_code,
        process_version,
        &event.correlation,
        &event.correlation_token,
        &manifest.entry,
        &input,
    )
    .await?;
    if started.is_none() {
        // Живой экземпляр с таким ключом уже идёт — это штатно: тот же день
        // мог быть доимпортирован дважды.
        return Ok(false);
    }
    Ok(true)
}

/// Разбудить ожидающих этого факта.
async fn wake_waiting(db: &'static DatabaseConnection, event: &DomainEvent) -> Result<usize> {
    let waiting =
        instances::list_waiting_for(db, event.kind.as_str(), &event.correlation_token).await?;
    let mut woken = 0;
    for instance in waiting {
        // Событие, опубликованное до постановки в ожидание, не считается:
        // иначе экземпляр проснулся бы от собственного прошлого.
        if instance
            .wait
            .as_ref()
            .is_some_and(|wait| event.seq <= wait.since_seq)
        {
            continue;
        }
        if resume_instance(db, &instance, event).await? {
            woken += 1;
        }
    }
    Ok(woken)
}

/// Продолжить экземпляр после события: курсор идёт по тому же ребру, на котором
/// он встал в ожидание.
///
/// Цель ребра не хранится в состоянии — она читается из графа запиненной версии.
/// Так остаётся ровно один источник истины о маршруте: определение, на котором
/// экземпляр стартовал.
async fn resume_instance(
    db: &'static DatabaseConnection,
    instance: &ProcessInstance,
    event: &DomainEvent,
) -> Result<bool> {
    let Some(manifest) = process_manifest(db, instance).await? else {
        instances::quarantine(db, &instance.id, "версия Процесса не найдена").await?;
        return Ok(false);
    };
    let (Some(stage_code), Some(outcome)) = (&instance.stage_code, &instance.last_outcome) else {
        instances::quarantine(db, &instance.id, "ожидание без курсора и выхода").await?;
        return Ok(false);
    };
    let Some(edge) = manifest.edge(stage_code, outcome) else {
        instances::quarantine(
            db,
            &instance.id,
            &format!("ребро {stage_code} → '{outcome}' исчезло из графа"),
        )
        .await?;
        return Ok(false);
    };

    // Данные события дополняют подготовленный вход, а ключ корреляции
    // перекрывает их обоих: он и есть идентичность экземпляра.
    let mut input = merge(instance.input.clone(), &event.payload);
    input = merge(input, &correlation_object(&instance.correlation));

    match &edge.to {
        EdgeTarget::Done => {
            instances::finish(db, &instance.id, outcome).await?;
            Ok(true)
        }
        EdgeTarget::Stage { code } => instances::wake(db, &instance.id, code, &input).await,
    }
}

// ---------------------------------------------------------------------------
// Исполнение Этапов
// ---------------------------------------------------------------------------

async fn run_due_instances(
    db: &'static DatabaseConnection,
    report: &mut TickReport,
    now: DateTime<Utc>,
) -> Result<()> {
    for instance in instances::list_runnable(db, &now.to_rfc3339(), BATCH).await? {
        let session_id = Uuid::new_v4().to_string();
        if !instances::try_claim(db, &instance.id, &session_id).await? {
            // Экземпляр взял другой воркер — это не ошибка, а нормальная гонка.
            continue;
        }
        let outcome = run_claimed_instance(db, &instance, &session_id).await;
        instances::release(db, &instance.id).await.ok();
        match outcome {
            Ok(StepOutcome::Ran) => report.stages_run += 1,
            Ok(StepOutcome::Quarantined) => {
                report.stages_run += 1;
                report.quarantined += 1;
            }
            Ok(StepOutcome::Skipped) => {}
            Err(error) => {
                warn!("Экземпляр {} не отработал: {error:?}", instance.id);
                instances::schedule_retry(
                    db,
                    &instance.id,
                    instance.attempts + 1,
                    Utc::now() + backoff(instance.attempts + 1),
                    &error.to_string(),
                )
                .await
                .ok();
            }
        }
    }
    Ok(())
}

/// Чем закончился один шаг экземпляра.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepOutcome {
    Ran,
    Quarantined,
    /// Шага не было: нечего исполнять или занят ресурс.
    Skipped,
}

/// Исполнить очередной Этап арендованного экземпляра и применить ровно один
/// переход.
///
/// Публично, потому что этим же путём идут тесты: проверять решения воркера
/// через таймер бессмысленно.
pub async fn run_claimed_instance(
    db: &'static DatabaseConnection,
    instance: &ProcessInstance,
    session_id: &str,
) -> Result<StepOutcome> {
    let Some(manifest) = process_manifest(db, instance).await? else {
        instances::quarantine(db, &instance.id, "версия Процесса не найдена").await?;
        return Ok(StepOutcome::Quarantined);
    };
    let Some(stage_code) = instance.stage_code.clone() else {
        instances::quarantine(db, &instance.id, "экземпляр без курсора").await?;
        return Ok(StepOutcome::Quarantined);
    };
    let pinned =
        definitions::pinned_stages(db, &instance.process_code, instance.process_version).await?;
    let Some(stage) = pinned.get(&stage_code).cloned() else {
        instances::quarantine(
            db,
            &instance.id,
            &format!("Этап {stage_code} не запинен версией Процесса"),
        )
        .await?;
        return Ok(StepOutcome::Quarantined);
    };

    // Ресурсы берём на весь прогон Этапа: конфликт с импортом в те же таблицы
    // означает «сейчас нельзя», а не «сломалось».
    let write_tables = stage_write_tables(&stage);
    let borrowed: Vec<&str> = write_tables.iter().map(String::as_str).collect();
    let label = format!("Процесс {} / {stage_code}", instance.process_code);
    let _guard = match get_global_resource_coordinator().try_acquire(
        &instance.id,
        &label,
        session_id,
        &borrowed,
    ) {
        Ok(guard) => guard,
        Err(conflict) => {
            info!(
                "Экземпляр {} ждёт ресурс '{}' (занят '{}')",
                instance.id, conflict.resource, conflict.owner_task
            );
            return Ok(StepOutcome::Skipped);
        }
    };

    let context = StageRunContext::for_instance(&instance.id, instance.visit, ActionMode::Execute);
    let run = stages::run(db, &stage, instance.input.clone(), &context).await?;

    // Шаг записывается до применения перехода: разбирать прогон придётся и
    // тогда, когда переход не состоялся.
    steps::record(db, &instance.id, instance.visit, &run).await?;

    match run.verdict {
        StageVerdict::Outcome(outcome) => {
            apply_outcome(
                db,
                instance,
                &manifest,
                &stage_code,
                &outcome.outcome,
                &outcome.data,
            )
            .await
        }
        StageVerdict::TemporaryFailure { message } => {
            let attempts = instance.attempts + 1;
            if attempts >= MAX_ATTEMPTS {
                instances::quarantine(
                    db,
                    &instance.id,
                    &format!("временный сбой не прошёл за {MAX_ATTEMPTS} попыток: {message}"),
                )
                .await?;
                return Ok(StepOutcome::Quarantined);
            }
            instances::schedule_retry(
                db,
                &instance.id,
                attempts,
                Utc::now() + backoff(attempts),
                &message,
            )
            .await?;
            Ok(StepOutcome::Ran)
        }
        StageVerdict::Defect { message } => {
            // Дефект Этапа — не повод повторять: повтор бессмыслен до правки
            // кода (ADR-0011 п.10).
            instances::quarantine(db, &instance.id, &message).await?;
            Ok(StepOutcome::Quarantined)
        }
    }
}

/// Применить штатный выход Этапа: ребро, ожидание либо завершение.
async fn apply_outcome(
    db: &'static DatabaseConnection,
    instance: &ProcessInstance,
    manifest: &ProcessManifest,
    stage_code: &str,
    outcome: &str,
    data: &Value,
) -> Result<StepOutcome> {
    let Some(edge) = manifest.edge(stage_code, outcome) else {
        instances::quarantine(
            db,
            &instance.id,
            &format!("выход '{outcome}' Этапа {stage_code} никуда не ведёт"),
        )
        .await?;
        return Ok(StepOutcome::Quarantined);
    };

    // Вход следующего Этапа: данные выхода, поверх — ключ корреляции.
    let next_input = merge(data.clone(), &correlation_object(&instance.correlation));

    if let Some(spec) = &edge.wait {
        return match build_wait(db, instance, spec, data).await? {
            Ok(wait) => {
                // Подготовленный вход кладём в состояние: Этап уже отработал,
                // и его собственный вход больше не нужен.
                instances::begin_wait(db, &instance.id, &wait, outcome, &next_input).await?;
                Ok(StepOutcome::Ran)
            }
            Err(problem) => {
                instances::quarantine(db, &instance.id, &problem).await?;
                Ok(StepOutcome::Quarantined)
            }
        };
    }

    match &edge.to {
        EdgeTarget::Done => {
            instances::finish(db, &instance.id, outcome).await?;
            Ok(StepOutcome::Ran)
        }
        EdgeTarget::Stage { code } => {
            instances::advance(db, &instance.id, code, &next_input, outcome).await?;
            Ok(StepOutcome::Ran)
        }
    }
}

/// Собрать ожидание: событие, токен, дедлайн.
///
/// Токен строится из того, что экземпляр про себя знает: ключа корреляции,
/// собственного токена (`request_key` — им адресуется просьба к человеку),
/// собственного идентификатора и данных выхода. Не собравшийся токен — дефект:
/// ожидание, которое никто не сможет разбудить, хуже падения.
async fn build_wait(
    db: &'static DatabaseConnection,
    instance: &ProcessInstance,
    spec: &WaitSpec,
    data: &Value,
) -> Result<std::result::Result<InstanceWait, String>> {
    let Some(kind) = DomainEventKind::parse(&spec.event) else {
        return Ok(Err(format!("события '{}' нет в каталоге", spec.event)));
    };
    let available = wait_sources(instance, data);
    let mut key = CorrelationKey::new();
    for field in kind.correlation_fields() {
        let Some(value) = available.get(*field) else {
            return Ok(Err(format!(
                "ожидание события '{}': нечем заполнить поле ключа '{field}'",
                spec.event
            )));
        };
        key = key.with(*field, value.clone());
    }
    let token = match key.token(kind) {
        Ok(token) => token,
        Err(problem) => return Ok(Err(problem)),
    };

    Ok(Ok(InstanceWait {
        event: spec.event.clone(),
        token,
        since_seq: events::last_seq(db).await?,
        deadline_at: (Utc::now() + Duration::minutes(spec.deadline_minutes.max(1))).to_rfc3339(),
        on_timeout: spec.on_timeout.clone(),
    }))
}

/// Откуда берутся значения полей ключа ожидания.
pub fn wait_sources(instance: &ProcessInstance, data: &Value) -> BTreeMap<String, String> {
    let mut sources: BTreeMap<String, String> = BTreeMap::new();
    if let Some(object) = data.as_object() {
        for (key, value) in object {
            if let Some(text) = scalar_text(value) {
                sources.insert(key.clone(), text);
            }
        }
    }
    for (field, value) in instance.correlation.fields() {
        sources.insert(field.to_string(), value.to_string());
    }
    // Два поля даёт рантайм: ими адресуются просьба к человеку и сам экземпляр.
    sources.insert(
        "request_key".to_string(),
        instance.correlation_token.clone(),
    );
    sources.insert("instance_id".to_string(), instance.id.clone());
    sources
}

fn scalar_text(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.clone()),
        Value::Number(number) => Some(number.to_string()),
        Value::Bool(flag) => Some(flag.to_string()),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Дедлайны ожиданий
// ---------------------------------------------------------------------------

/// Разобрать ожидания, у которых вышел срок.
///
/// Дедлайн публикуется фактом (`process.instance.timeout`), а не только пишется
/// в строку: «истекло» — такое же событие домена, как остальные, и его может
/// ждать другой Процесс.
async fn expire_waits(
    db: &'static DatabaseConnection,
    report: &mut TickReport,
    now: DateTime<Utc>,
) -> Result<()> {
    for instance in instances::list_expired_waits(db, &now.to_rfc3339(), BATCH).await? {
        let Some(wait) = instance.wait.clone() else {
            continue;
        };
        events::publish(
            db,
            DomainEventKind::ProcessInstanceTimeout,
            CorrelationKey::new().with("instance_id", instance.id.clone()),
            serde_json::json!({
                "process_code": instance.process_code,
                "stage_code": instance.stage_code,
                "waited_for": wait.event,
            }),
            "process-worker",
        )
        .await?;
        report.expired += 1;

        match wait.on_timeout {
            Some(EdgeTarget::Stage { code }) => {
                let input = merge(
                    instance.input.clone(),
                    &correlation_object(&instance.correlation),
                );
                instances::wake(db, &instance.id, &code, &input).await?;
            }
            Some(EdgeTarget::Done) => {
                instances::finish(db, &instance.id, "дедлайн ожидания").await?;
            }
            // Эскалация: экземпляр остаётся человеку, а не идёт по графу сам.
            None => {
                instances::quarantine(
                    db,
                    &instance.id,
                    &format!("не дождались события '{}' к дедлайну", wait.event),
                )
                .await?;
                report.quarantined += 1;
            }
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Мелочи
// ---------------------------------------------------------------------------

/// Пауза перед повтором. Растёт, но упирается в потолок: смысл повтора — пережить
/// чужую недоступность, а не отложить работу на сутки.
fn backoff(attempts: i32) -> Duration {
    match attempts {
        0 | 1 => Duration::minutes(1),
        2 => Duration::minutes(5),
        3 => Duration::minutes(15),
        _ => Duration::minutes(30),
    }
}

async fn process_manifest(
    db: &DatabaseConnection,
    instance: &ProcessInstance,
) -> Result<Option<ProcessManifest>> {
    Ok(
        repository::find_process(db, &instance.process_code, instance.process_version)
            .await?
            .map(|record| record.definition.manifest),
    )
}

/// Таблицы, которые может тронуть Этап: объединение по его Действиям.
fn stage_write_tables(stage: &StageDefinition) -> Vec<String> {
    let mut tables: Vec<String> = Vec::new();
    let catalog: HashMap<&str, &'static [&'static str]> = actions::list()
        .into_iter()
        .map(|info| (info.name, info.write_tables))
        .collect();
    for capability in &stage.manifest.capabilities {
        let Some(name) = capability.trim().strip_prefix("action:") else {
            continue;
        };
        for table in catalog.get(name.trim()).copied().unwrap_or(&[]) {
            if !tables.iter().any(|known| known == table) {
                tables.push((*table).to_string());
            }
        }
    }
    tables
}

fn correlation_object(correlation: &CorrelationKey) -> Value {
    let mut object = Map::new();
    for (field, value) in correlation.fields() {
        object.insert(field.to_string(), Value::String(value.to_string()));
    }
    Value::Object(object)
}

/// Наложить `overlay` на `base`. Оба не объекты — побеждает `overlay`.
fn merge(base: Value, overlay: &Value) -> Value {
    match (base, overlay) {
        (Value::Object(mut base), Value::Object(overlay)) => {
            for (key, value) in overlay {
                base.insert(key.clone(), value.clone());
            }
            Value::Object(base)
        }
        (_, Value::Object(overlay)) => Value::Object(overlay.clone()),
        (base, Value::Null) => base,
        (_, overlay) => overlay.clone(),
    }
}

async fn read_cursor(db: &DatabaseConnection) -> Result<i64> {
    let row = db
        .query_one(Statement::from_sql_and_values(
            DatabaseBackend::Sqlite,
            "SELECT value FROM sys_settings WHERE key = ?",
            vec![CURSOR_KEY.into()],
        ))
        .await?;
    Ok(row
        .map(|row| row.try_get::<String>("", "value"))
        .transpose()?
        .and_then(|value| value.parse::<i64>().ok())
        .unwrap_or(0))
}

async fn write_cursor(db: &DatabaseConnection, value: i64) -> Result<()> {
    let now = Utc::now().to_rfc3339();
    db.execute(Statement::from_sql_and_values(
        DatabaseBackend::Sqlite,
        "INSERT INTO sys_settings (key, value, created_at, updated_at) VALUES (?, ?, ?, ?) \
         ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
        vec![
            CURSOR_KEY.into(),
            value.to_string().into(),
            now.clone().into(),
            now.into(),
        ],
    ))
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use contracts::processes::InstanceStatus;
    use serde_json::json;

    fn instance() -> ProcessInstance {
        ProcessInstance {
            id: "i-1".into(),
            process_code: "pr0001".into(),
            process_version: 1,
            correlation: CorrelationKey::new()
                .with("connection_id", "c-1")
                .with("business_date", "2026-08-21"),
            correlation_token: "connection_id=c-1;business_date=2026-08-21".into(),
            status: InstanceStatus::Running,
            stage_code: Some("st0004".into()),
            visit: 2,
            input: json!({}),
            attempts: 0,
            next_attempt_at: None,
            wait: None,
            last_outcome: None,
            last_error: None,
            claim_session_id: None,
            started_at: "2026-08-21T10:00:00Z".into(),
            updated_at: "2026-08-21T10:00:00Z".into(),
            finished_at: None,
        }
    }

    /// Ключ корреляции перекрывает данные выхода: он и есть идентичность
    /// экземпляра, и Этап не должен уметь её переписать.
    #[test]
    fn correlation_wins_over_outcome_data() {
        let data = json!({ "business_date": "1999-01-01", "amount": 12 });
        let merged = merge(data, &correlation_object(&instance().correlation));
        assert_eq!(merged["business_date"], "2026-08-21");
        assert_eq!(merged["amount"], 12);
    }

    /// Просьба к человеку адресуется токеном экземпляра, поэтому `request_key`
    /// обязан находиться без всяких данных выхода.
    #[test]
    fn request_key_is_always_available() {
        let sources = wait_sources(&instance(), &Value::Null);
        assert_eq!(
            sources.get("request_key").map(String::as_str),
            Some("connection_id=c-1;business_date=2026-08-21")
        );
        assert_eq!(sources.get("instance_id").map(String::as_str), Some("i-1"));
        assert_eq!(
            sources.get("connection_id").map(String::as_str),
            Some("c-1")
        );
    }

    #[test]
    fn backoff_grows_and_stops_growing() {
        assert!(backoff(1) < backoff(2));
        assert!(backoff(2) < backoff(3));
        assert_eq!(backoff(4), backoff(40));
    }
}
