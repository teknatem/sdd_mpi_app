use anyhow::Result;
use chrono::{DateTime, Utc};
use contracts::domain::common::AggregateId;
use std::str::FromStr;
use std::sync::Arc;
use tracing::{info, warn};
use uuid::Uuid;

use super::task_session_runner::{spawn_task_session, TaskSessionParams};
use super::{
    logger::TaskLogger, registry::TaskManagerRegistry,
    resource_coordinator::get_global_resource_coordinator, runs_service, service,
};
use contracts::system::tasks::progress::TaskStatus;

/// Следующее время запуска по cron-расписанию.
fn next_run_from_cron(schedule_cron: &str, after: DateTime<Utc>) -> Option<DateTime<Utc>> {
    let schedule = cron::Schedule::from_str(schedule_cron).ok()?;
    schedule.after(&after).next()
}

/// Фоновый воркер: каждые `interval_seconds` проверяет все включённые задачи
/// и запускает просроченные.
pub struct ScheduledTaskWorker {
    registry: Arc<TaskManagerRegistry>,
    logger: Arc<TaskLogger>,
    interval_seconds: u64,
}

impl ScheduledTaskWorker {
    pub fn new(
        registry: Arc<TaskManagerRegistry>,
        logger: Arc<TaskLogger>,
        interval_seconds: u64,
    ) -> Self {
        Self {
            registry,
            logger,
            interval_seconds,
        }
    }

    pub async fn run_loop(&self) {
        info!(
            "Scheduled task worker started (interval {}s)",
            self.interval_seconds
        );
        let mut interval =
            tokio::time::interval(tokio::time::Duration::from_secs(self.interval_seconds));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            interval.tick().await;
            info!("Checking for due scheduled tasks…");
            if let Err(e) = self.process_due_tasks().await {
                tracing::error!("Error processing scheduled tasks: {:?}", e);
            }
        }
    }

    async fn process_due_tasks(&self) -> Result<()> {
        // В maintenance не трогаем даже таблицу настроек планировщика: операция
        // переноса БД должна получить полностью тихое окно.
        if crate::system::maintenance::is_active() {
            info!("Maintenance is active — skipping scheduled task check");
            return Ok(());
        }
        if !crate::system::settings::service::get_scheduler_enabled().await? {
            info!("Scheduler is disabled — skipping task check");
            return Ok(());
        }

        let now = Utc::now();
        let mut tasks = service::list_enabled_tasks().await?;
        tasks.sort_by(|a, b| {
            a.next_run_at
                .cmp(&b.next_run_at)
                .then_with(|| a.base.id.as_string().cmp(&b.base.id.as_string()))
        });
        let coordinator = get_global_resource_coordinator();

        for task in tasks {
            let Some(database_activity) = crate::system::maintenance::try_begin_database_activity()
            else {
                info!("Database maintenance started — stopping this scheduler pass");
                break;
            };
            let should_run = task.next_run_at.map_or(true, |t| t <= now);
            if !should_run {
                continue;
            }

            let task_id_str = task.base.id.as_string();
            if let Ok(Some(_)) = runs_service::get_running_for_task(&task_id_str).await {
                warn!(
                    "Task '{}' ({}) is due but already running; skipping",
                    task.base.description, task_id_str
                );
                continue;
            }

            info!(
                "Task '{}' ({}) is due. Running…",
                task.base.description, task_id_str
            );

            let session_id = Uuid::new_v4().to_string();
            let task_id = task.base.id;
            let task_label = format!("{} ({})", task.base.description, task_id_str);
            let write_tables = self
                .registry
                .get(&task.task_type)
                .map(|manager| manager.metadata().write_tables)
                .unwrap_or_default();
            let resource_guard =
                match coordinator.try_acquire(&task_id_str, &task_label, &session_id, write_tables)
                {
                    Ok(guard) => guard,
                    Err(conflict) => {
                        warn!(
                        "Task '{}' is due but resource '{}' is used by '{}'; retrying next tick",
                        task_label, conflict.resource, conflict.owner_task
                    );
                        continue;
                    }
                };

            let next_run = task
                .schedule_cron
                .as_deref()
                .and_then(|cron| next_run_from_cron(cron, now))
                .unwrap_or_else(|| now + chrono::Duration::hours(1));

            let log_file_path = self.logger.get_log_file_path(&session_id);

            if let Err(e) = runs_service::create_run_with_log(
                task_id,
                session_id.clone(),
                "Scheduled".to_string(),
                log_file_path.clone(),
            )
            .await
            {
                warn!("Failed to record run start for task {}: {}", task_id_str, e);
            }

            service::update_run_status(
                &task_id,
                Some(now),
                Some(next_run),
                Some(log_file_path),
                Some(TaskStatus::Running.to_string()),
                None,
                None,
            )
            .await?;

            spawn_task_session(TaskSessionParams {
                task,
                session_id,
                started_at: now,
                next_run_at: next_run,
                recompute_next_run_if_elapsed: true,
                logger: Arc::clone(&self.logger),
                registry: Arc::clone(&self.registry),
                resource_guard,
                database_activity,
            });
        }
        Ok(())
    }
}
