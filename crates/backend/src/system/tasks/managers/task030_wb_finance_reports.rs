use anyhow::{Context, Result};
use async_trait::async_trait;
use contracts::{
    system::tasks::{
        aggregate::ScheduledTask,
        metadata::{ExternalApiInfo, TaskConfigField, TaskConfigFieldType, TaskMetadata},
        progress::TaskProgress,
    },
    usecases::u504_import_from_wildberries::{request::ImportMode, ImportRequest},
};
use serde::Deserialize;
use std::sync::Arc;

use crate::{
    system::tasks::{
        logger::TaskLogger,
        manager::{TaskManager, TaskRunOutcome},
    },
    usecases::u504_import_from_wildberries::ImportExecutor,
};

fn default_lookback_days() -> i64 {
    35
}

#[derive(Deserialize)]
struct Config {
    connection_id: String,
    #[serde(default = "default_lookback_days")]
    lookback_days: i64,
}

static METADATA: TaskMetadata = TaskMetadata {
    task_type: "task030_wb_finance_reports",
    write_tables: &["a043_wb_finance_report"],
    display_name: "WB Финансовые отчёты (Finance API v1)",
    description: "Повторно загружает ежедневные отчёты WB за скользящее окно и сохраняет один reportId как один документ a043. Legacy p903/task006 не изменяется.",
    external_apis: &[
        ExternalApiInfo { name: "WB Finance API — список", base_url: "https://finance-api.wildberries.ru/api/finance/v1/sales-reports/list", rate_limit_desc: "1 запрос в минуту на кабинет" },
        ExternalApiInfo { name: "WB Finance API — детализация", base_url: "https://finance-api.wildberries.ru/api/finance/v1/sales-reports/detailed/{reportId}", rate_limit_desc: "1 запрос в минуту на кабинет, общий последовательный gate" },
    ],
    constraints: &["Токен категории Финансы, персональный или сервисный", "Данные доступны с 2025-01-01", "Только period=daily", "Проекций и проводок нет"],
    config_fields: &[
        TaskConfigField { key: "connection_id", label: "WB Кабинет", hint: "Подключение Wildberries", field_type: TaskConfigFieldType::ConnectionMp, required: true, default_value: None, min_value: None, max_value: None },
        TaskConfigField { key: "lookback_days", label: "Окно обновления (дн.)", hint: "Количество последних календарных дней, перечитываемых при каждом запуске", field_type: TaskConfigFieldType::Integer, required: false, default_value: Some("35"), min_value: Some(1), max_value: Some(365) },
    ],
    max_duration_seconds: 14400,
};

pub struct Task030WbFinanceReportsManager {
    executor: Arc<ImportExecutor>,
}
impl Task030WbFinanceReportsManager {
    pub fn new(executor: Arc<ImportExecutor>) -> Self {
        Self { executor }
    }
}

#[async_trait]
impl TaskManager for Task030WbFinanceReportsManager {
    fn task_type(&self) -> &'static str {
        "task030_wb_finance_reports"
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
        let cfg: Config = serde_json::from_str(&task.config_json)
            .context("Config parse failed — expected connection_id and optional lookback_days")?;
        if !(1..=365).contains(&cfg.lookback_days) {
            anyhow::bail!("lookback_days must be between 1 and 365");
        }
        let id = super::config_helpers::parse_connection_id(&cfg.connection_id, "Wildberries")?;
        let connection = crate::domain::a006_connection_mp::service::get_by_id(id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Marketplace connection not found: {id}"))?;
        let date_to = chrono::Utc::now().date_naive();
        let date_from = (date_to - chrono::Duration::days(cfg.lookback_days - 1))
            .max(chrono::NaiveDate::from_ymd_opt(2025, 1, 1).unwrap());
        logger.write_log(
            session_id,
            &format!("task030: WB Finance API v1 {date_from}..{date_to}"),
        )?;
        self.executor
            .execute_import(
                session_id,
                &ImportRequest {
                    connection_id: cfg.connection_id,
                    target_aggregates: vec!["a043_wb_finance_report".into()],
                    date_from,
                    date_to,
                    mode: ImportMode::Background,
                },
                &connection,
            )
            .await?;
        Ok(TaskRunOutcome::completed())
    }

    fn get_progress(&self, session_id: &str) -> Option<TaskProgress> {
        self.executor
            .progress_tracker
            .get_progress(session_id)
            .map(Into::into)
    }
    fn list_live_progress_sessions(&self) -> Vec<TaskProgress> {
        self.executor.list_live_task_progress()
    }
}
