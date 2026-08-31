use super::aggregate_repost;
use super::progress_tracker::ProgressTracker;
use super::projection_repost;
use anyhow::{anyhow, Result};
use chrono::NaiveDate;
use contracts::usecases::u508_repost_documents::{
    aggregate::AggregateOption,
    aggregate_request::AggregateRepostRequest,
    progress::RepostStatus,
    projection::ProjectionOption,
    request::RepostRequest,
    response::{RepostResponse, RepostStartStatus},
};
use std::sync::{
    atomic::{AtomicI32, Ordering},
    Arc,
};
use uuid::Uuid;

pub struct RepostExecutor {
    pub progress_tracker: Arc<ProgressTracker>,
}

impl RepostExecutor {
    pub fn new(progress_tracker: Arc<ProgressTracker>) -> Self {
        Self { progress_tracker }
    }

    /// Проекции, которые умеют пересобираться за период.
    ///
    /// Список объявляют сами проекции через `ProjectionRepost::option`,
    /// порядок задаёт composition root.
    pub fn list_available_projections(&self) -> Vec<ProjectionOption> {
        projection_repost::all()
            .iter()
            .map(|projection| {
                let option = projection.option();
                ProjectionOption {
                    key: projection.key().to_string(),
                    label: option.label.to_string(),
                    description: option.description.to_string(),
                }
            })
            .collect()
    }

    /// Агрегаты, которые умеют перепроводиться оптом за период.
    ///
    /// Список не ведётся здесь: его объявляют сами срезы через
    /// `Registrator::repost_option`, а порядок задаёт composition root.
    pub fn list_available_aggregates(&self) -> Vec<AggregateOption> {
        crate::shared::registrators::all()
            .iter()
            .filter_map(|registrator| {
                let option = registrator.repost_option()?;
                Some(AggregateOption {
                    key: registrator.kind().to_string(),
                    label: option.label.to_string(),
                    description: option.description.to_string(),
                })
            })
            .collect()
    }

    pub async fn start_repost(&self, request: RepostRequest) -> Result<RepostResponse> {
        let database_activity = crate::system::maintenance::try_begin_database_activity()
            .ok_or_else(|| {
                anyhow!("Перепроведение недоступно во время обслуживания базы данных")
            })?;
        Self::validate_request(&request)?;

        let session_id = Uuid::new_v4().to_string();
        self.progress_tracker.create_session(session_id.clone());

        let executor = Arc::new(Self {
            progress_tracker: self.progress_tracker.clone(),
        });
        let sid = session_id.clone();
        let req = request.clone();

        tokio::spawn(async move {
            let _database_activity = database_activity;
            if let Err(error) = executor.execute_repost(&sid, &req).await {
                tracing::error!("Projection repost failed: {}", error);
                executor
                    .progress_tracker
                    .add_error(&sid, format!("Repost failed: {}", error));
                executor
                    .progress_tracker
                    .complete_session(&sid, RepostStatus::Failed);
            }
        });

        Ok(RepostResponse {
            session_id,
            status: RepostStartStatus::Started,
            message: "Repost started".to_string(),
        })
    }

    pub async fn start_aggregate_repost(
        &self,
        request: AggregateRepostRequest,
    ) -> Result<RepostResponse> {
        let database_activity = crate::system::maintenance::try_begin_database_activity()
            .ok_or_else(|| {
                anyhow!("Перепроведение недоступно во время обслуживания базы данных")
            })?;
        Self::validate_aggregate_request(&request)?;

        let session_id = Uuid::new_v4().to_string();
        self.progress_tracker.create_session(session_id.clone());

        let executor = Arc::new(Self {
            progress_tracker: self.progress_tracker.clone(),
        });
        let sid = session_id.clone();
        let req = request.clone();

        tokio::spawn(async move {
            let _database_activity = database_activity;
            if let Err(error) = executor.execute_aggregate_repost(&sid, &req).await {
                tracing::error!("Aggregate repost failed: {}", error);
                executor
                    .progress_tracker
                    .add_error(&sid, format!("Repost failed: {}", error));
                executor
                    .progress_tracker
                    .complete_session(&sid, RepostStatus::Failed);
            }
        });

        Ok(RepostResponse {
            session_id,
            status: RepostStartStatus::Started,
            message: "Aggregate repost started".to_string(),
        })
    }

    /// Перепровести агрегат **не отходя**: валидация плюс исполнение в текущей
    /// задаче, с настоящим исходом в `Result`.
    ///
    /// Отличается от `start_aggregate_repost` тем, что не спавнит фон и не
    /// возвращает `session_id`. HTTP-обработчику нужен ровно обратный контракт
    /// (ответить сразу, прогресс опросить потом), а Действию механизма
    /// Процессов — этот: журнал эффектов обязан записать, что произошло, а не
    /// что началось.
    ///
    /// Прогресс пишется в трекер **этого** экземпляра исполнителя. У HTTP-слоя
    /// трекер свой (`api/handlers/usecases.rs`), поэтому через `get_progress`
    /// такой прогон снаружи не виден: наблюдать его можно только по журналу
    /// эффектов, когда он закончится.
    pub async fn repost_aggregate_inline(
        &self,
        session_id: &str,
        request: &AggregateRepostRequest,
    ) -> Result<()> {
        let _database_activity = crate::system::maintenance::try_begin_database_activity()
            .ok_or_else(|| {
                anyhow!("Перепроведение недоступно во время обслуживания базы данных")
            })?;
        Self::validate_aggregate_request(request)?;
        self.progress_tracker.create_session(session_id.to_string());
        let outcome = self.execute_aggregate_repost(session_id, request).await;
        match &outcome {
            Ok(()) => self
                .progress_tracker
                .complete_session(session_id, RepostStatus::Completed),
            Err(error) => {
                self.progress_tracker
                    .add_error(session_id, format!("Repost failed: {error}"));
                self.progress_tracker
                    .complete_session(session_id, RepostStatus::Failed);
            }
        }
        outcome
    }

    pub fn get_progress(
        &self,
        session_id: &str,
    ) -> Option<contracts::usecases::u508_repost_documents::progress::RepostProgress> {
        self.progress_tracker.get_progress(session_id)
    }

    fn validate_request(request: &RepostRequest) -> Result<()> {
        // Допустимость ключа — это наличие проекции в реестре, то есть ровно
        // тот список, который пользователь видел на странице.
        if projection_repost::find(&request.projection_key).is_none() {
            return Err(anyhow!(
                "Unsupported projection_key: {}",
                request.projection_key
            ));
        }

        let date_from = NaiveDate::parse_from_str(&request.date_from, "%Y-%m-%d")
            .map_err(|_| anyhow!("Invalid date_from: {}", request.date_from))?;
        let date_to = NaiveDate::parse_from_str(&request.date_to, "%Y-%m-%d")
            .map_err(|_| anyhow!("Invalid date_to: {}", request.date_to))?;

        if date_from > date_to {
            return Err(anyhow!("date_from must be less than or equal to date_to"));
        }

        Ok(())
    }

    fn validate_aggregate_request(request: &AggregateRepostRequest) -> Result<()> {
        // Допустимость ключа — это ровно наличие `repost_option` у регистратора,
        // то есть тот же список, что видит пользователь на странице. Отдельное
        // перечисление здесь было девятым по счёту и разошлось бы первым.
        let supported = crate::shared::registrators::find(&request.aggregate_key)
            .is_some_and(|registrator| registrator.repost_option().is_some());
        if !supported {
            return Err(anyhow!(
                "Unsupported aggregate_key: {}",
                request.aggregate_key
            ));
        }

        let date_from = NaiveDate::parse_from_str(&request.date_from, "%Y-%m-%d")
            .map_err(|_| anyhow!("Invalid date_from: {}", request.date_from))?;
        let date_to = NaiveDate::parse_from_str(&request.date_to, "%Y-%m-%d")
            .map_err(|_| anyhow!("Invalid date_to: {}", request.date_to))?;

        if date_from > date_to {
            return Err(anyhow!("date_from must be less than or equal to date_to"));
        }

        Ok(())
    }

    async fn execute_repost(&self, session_id: &str, request: &RepostRequest) -> Result<()> {
        projection_repost::run(
            projection_repost::RepostContext {
                tracker: &self.progress_tracker,
                session_id,
                date_from: &request.date_from,
                date_to: &request.date_to,
            },
            &request.projection_key,
        )
        .await
    }

    async fn execute_aggregate_repost(
        &self,
        session_id: &str,
        request: &AggregateRepostRequest,
    ) -> Result<()> {
        // Срез мог объявить свою стратегию перепроведения оптом — например,
        // с прогревом кэша по дню. Если объявил, движок уступает ей целиком.
        if let Some(strategy) = aggregate_repost::find(&request.aggregate_key) {
            return strategy
                .repost(&self.progress_tracker, session_id, request)
                .await;
        }

        // Отбор документов за период принадлежит срезу: у a013 это дата
        // создания заказа, у a016 — когорта заказов того же периода, у
        // остальных — своя дата документа. Здесь достаточно ключа.
        let registrator = crate::shared::registrators::find(&request.aggregate_key)
            .ok_or_else(|| anyhow!("Unsupported aggregate_key: {}", request.aggregate_key))?;
        let document_ids = registrator
            .ids_in_period(&request.date_from, &request.date_to, request.only_posted)
            .await?;

        let total = document_ids.len() as i32;
        self.progress_tracker.set_total(session_id, total);

        let processed = Arc::new(AtomicI32::new(0));
        let reposted = Arc::new(AtomicI32::new(0));

        repost_ids_concurrent(
            &self.progress_tracker,
            session_id,
            &request.aggregate_key,
            document_ids,
            &processed,
            &reposted,
        )
        .await?;

        let final_reposted = reposted.load(Ordering::Relaxed);
        self.progress_tracker
            .update_progress(session_id, total, final_reposted, None);

        let final_status = if self
            .progress_tracker
            .get_progress(session_id)
            .map(|progress| progress.errors > 0)
            .unwrap_or(false)
        {
            RepostStatus::CompletedWithErrors
        } else {
            RepostStatus::Completed
        };

        self.progress_tracker
            .complete_session(session_id, final_status);

        Ok(())
    }
}

/// Конкурентный репост набора id одного агрегата с ограничением параллелизма.
///
/// Счётчики `processed`/`reposted` — общие атомики, передаются снаружи: одиночный
/// aggregate-repost создаёт их на прогон, а пересбор воронки прокидывает сквозные
/// счётчики через все шаги. SQLite (WAL) сериализует запись, но параллелизм ускоряет
/// CPU-bound вычисления и перекрытие read-фазы (get_by_id, lookups) с write-фазой;
/// `repost_one_with_retry` гасит конфликты снапшота при CONCURRENCY > 1.
/// Конкурентный репост набора id одного агрегата — публичная примитивная
/// операция движка: ею пользуется и перепроведение по периоду, и пересбор
/// воронки (`p916::service`), которому нужны сквозные счётчики.
pub async fn repost_ids_concurrent(
    tracker: &Arc<ProgressTracker>,
    session_id: &str,
    aggregate_key: &str,
    document_ids: Vec<String>,
    processed: &Arc<AtomicI32>,
    reposted: &Arc<AtomicI32>,
) -> Result<()> {
    const CONCURRENCY: usize = 4;
    let semaphore = Arc::new(tokio::sync::Semaphore::new(CONCURRENCY));
    let session_id_str = session_id.to_string();

    let mut join_set = tokio::task::JoinSet::new();

    for document_id_str in document_ids {
        let aggregate_id = match Uuid::parse_str(&document_id_str) {
            Ok(id) => id,
            Err(error) => {
                tracker.add_error(
                    &session_id_str,
                    format!("Invalid aggregate id {}: {}", document_id_str, error),
                );
                processed.fetch_add(1, Ordering::Relaxed);
                let p = processed.load(Ordering::Relaxed);
                let r = reposted.load(Ordering::Relaxed);
                tracker.update_progress(&session_id_str, p, r, None);
                continue;
            }
        };

        let permit = semaphore
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| anyhow!("Semaphore closed"))?;

        let tracker_task = tracker.clone();
        let sid = session_id_str.clone();
        let agg_key = aggregate_key.to_string();
        let processed_ref = processed.clone();
        let reposted_ref = reposted.clone();

        join_set.spawn(async move {
            let _permit = permit;
            let result = repost_one_with_retry(&agg_key, aggregate_id).await;
            let proc_count = processed_ref.fetch_add(1, Ordering::Relaxed) + 1;
            let repo_count = match result {
                Ok(()) => reposted_ref.fetch_add(1, Ordering::Relaxed) + 1,
                Err(error) => {
                    tracker_task.add_error(
                        &sid,
                        format!("Failed to repost {} {}: {}", agg_key, aggregate_id, error),
                    );
                    reposted_ref.load(Ordering::Relaxed)
                }
            };
            tracker_task.update_progress(&sid, proc_count, repo_count, None);
        });
    }

    while let Some(task_result) = join_set.join_next().await {
        task_result.map_err(|e| anyhow!("Task panicked: {}", e))?;
    }

    Ok(())
}

/// Провести один документ через реестр регистраторов.
///
/// Раньше здесь стояли два почти одинаковых `match`: `dispatch_repost` знал
/// исторические ключи `p904_sales_data` (`WB_Sales`, `OZON_FBS`…), а
/// `dispatch_aggregate_repost` — канонические (`a012_wb_sales`). Списки жили
/// врозь и успели разойтись. Реестр индексирует оба пространства сразу
/// (`Registrator::aliases`), поэтому функция нужна одна.
pub(super) async fn repost_one(registrator_type: &str, registrator_id: Uuid) -> Result<()> {
    let registrator = crate::shared::registrators::find(registrator_type)
        .ok_or_else(|| anyhow!("Unsupported registrator_type: {}", registrator_type))?;
    registrator.post_document(registrator_id).await
}

/// Признак ошибки блокировки SQLite (`SQLITE_BUSY` = 5, `SQLITE_BUSY_SNAPSHOT` = 517).
///
/// `busy_timeout` сериализует ожидание write-lock, но НЕ помогает при конфликте
/// снапшота: SeaORM открывает все транзакции как `BEGIN DEFERRED`, поэтому txn
/// становится читателем на первом SELECT и не может «дорасти» до писателя, если
/// другое соединение успело закоммититься. Единственное корректное лечение —
/// откатить и повторить транзакцию целиком.
fn is_database_locked(error: &anyhow::Error) -> bool {
    let message = format!("{:#}", error);
    message.contains("database is locked")
        || message.contains("(code: 5)")
        || message.contains("(code: 517)")
}

/// Перепроведение одного документа с повторами при блокировке SQLite.
///
/// При CONCURRENCY > 1 параллельные писатели гарантированно ловят конфликты
/// снапшота; джиттер в backoff декоррелирует воркеры, чтобы они не повторяли
/// попытку синхронно.
async fn repost_one_with_retry(aggregate_key: &str, aggregate_id: Uuid) -> Result<()> {
    const MAX_ATTEMPTS: u32 = 8;

    let mut attempt = 0u32;
    loop {
        attempt += 1;
        match repost_one(aggregate_key, aggregate_id).await {
            Ok(()) => return Ok(()),
            Err(error) if attempt < MAX_ATTEMPTS && is_database_locked(&error) => {
                // Backoff с джиттером: базовая задержка растёт с попыткой,
                // джиттер берём из младших бит UUID (без внешних зависимостей).
                let base_ms = 20u64 * attempt as u64;
                let jitter_ms = (aggregate_id.as_u128() as u64 % 40) + 1;
                tokio::time::sleep(std::time::Duration::from_millis(base_ms + jitter_ms)).await;
            }
            Err(error) => return Err(error),
        }
    }
}
