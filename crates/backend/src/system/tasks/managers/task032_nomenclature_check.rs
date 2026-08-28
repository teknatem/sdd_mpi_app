//! task032 — искра для Процесса «Проверка номенклатуры» (pr0002).
//!
//! Само задание ничего не импортирует и не сопоставляет: оно публикует факт
//! `process.due`, а граф Этапов ведёт воркер Процессов. Ручной запуск идёт
//! через уже существующую кнопку «Запустить сейчас» (планировщик не нужен).

use anyhow::{Context, Result};
use async_trait::async_trait;
use contracts::processes::{CorrelationKey, DomainEventKind};
use contracts::system::tasks::aggregate::ScheduledTask;
use contracts::system::tasks::metadata::TaskMetadata;
use contracts::system::tasks::progress::TaskProgress;
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;

use crate::processes::events;
use crate::system::tasks::logger::TaskLogger;
use crate::system::tasks::manager::{TaskManager, TaskRunOutcome};

#[derive(Deserialize)]
struct Config {
    #[serde(default = "default_process_code")]
    process_code: String,
}

fn default_process_code() -> String {
    "pr0002".to_string()
}

static METADATA: TaskMetadata = TaskMetadata {
    task_type: "task032_nomenclature_check",
    write_tables: &["sys_domain_event"],
    display_name: "Проверка номенклатуры (pr0002)",
    description: "Публикует факт process.due для Процесса «Проверка номенклатуры». \
        Импорт справочников, сопоставление и тикеты делает сам Процесс (pr0002), \
        а не это задание. Для разработки достаточно ручного «Запустить сейчас».",
    external_apis: &[],
    constraints: &[
        "Требует активной версии Процесса pr0002 — иначе факт уйдёт в журнал, \
         а экземпляр не стартует",
        "config_json: {\"process_code\":\"pr0002\"} (по умолчанию pr0002)",
        "Планировщик не обязателен: ручной запуск работает при \
         [scheduled_tasks].enabled = false",
    ],
    config_fields: &[],
    max_duration_seconds: 60,
};

pub struct Task032NomenclatureCheckManager;

impl Task032NomenclatureCheckManager {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl TaskManager for Task032NomenclatureCheckManager {
    fn task_type(&self) -> &'static str {
        "task032_nomenclature_check"
    }

    fn metadata(&self) -> &'static TaskMetadata {
        &METADATA
    }

    async fn run(
        &self,
        task: &ScheduledTask,
        session_id: &str,
        logger: Arc<TaskLogger>,
    ) -> Result<TaskRunOutcome> {
        logger.write_log(session_id, "task032: nomenclature check spark started")?;

        let cfg: Config = if task.config_json.trim().is_empty() {
            Config {
                process_code: default_process_code(),
            }
        } else {
            serde_json::from_str(&task.config_json)
                .context("Config parse failed — expected {\"process_code\":\"pr0002\"}")?
        };

        let process_code = cfg.process_code.trim();
        if process_code.is_empty() {
            anyhow::bail!("task032.process_code is required");
        }

        let db = crate::shared::data::db::get_connection();
        let event = events::publish(
            db,
            DomainEventKind::ProcessDue,
            CorrelationKey::new().with("process_code", process_code),
            json!({ "source_task": "task032_nomenclature_check", "session_id": session_id }),
            "task032",
        )
        .await
        .context("failed to publish process.due")?;

        logger.write_log(
            session_id,
            &format!(
                "task032: published process.due process_code={} seq={}",
                process_code, event.seq
            ),
        )?;
        Ok(TaskRunOutcome::completed())
    }

    fn get_progress(&self, _session_id: &str) -> Option<TaskProgress> {
        None
    }
}
