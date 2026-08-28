use anyhow::{Context, Result};
use async_trait::async_trait;
use chrono::Utc;
use contracts::system::tasks::aggregate::ScheduledTask;
use contracts::system::tasks::metadata::{
    ExternalApiInfo, TaskConfigField, TaskConfigFieldType, TaskMetadata,
};
use contracts::system::tasks::progress::TaskProgress;
use contracts::usecases::u501_import_from_ut::request::{ImportMode, ImportRequest};
use serde::Deserialize;
use std::sync::Arc;

use crate::system::tasks::logger::TaskLogger;
use crate::system::tasks::manager::{TaskManager, TaskRunOutcome};
use crate::usecases::u501_import_from_ut::ImportExecutor;

#[derive(Deserialize)]
struct Config {
    /// UUID из таблицы a001_connection_1c
    connection_id: String,
}

static METADATA: TaskMetadata = TaskMetadata {
    task_type: "task031_ut_nomenclature_prices",
    write_tables: &["a004_nomenclature", "p906_nomenclature_prices"],
    display_name: "1С: номенклатура и дилерские цены",
    description: "Ежедневная загрузка справочника номенклатуры (a004) и дилерских цен УТ (p906) \
        из 1С. Номенклатура — OData `Catalog_Номенклатура` (код, наименование, артикул, \
        «ТребуетсяСборка»). Цены — HTTP `/hs/mpi_api/prices_dealer`: таблица p906 очищается и \
        перезаписывается целиком (начальные + история). Измерения (категория, линейка, цвет) \
        из 1С не приходят и при обновлении не затираются. Не тянет организации, контрагентов, \
        закупки и штрихкоды — для полного импорта есть `u501_import_ut`.",
    external_apis: &[
        ExternalApiInfo {
            name: "1С:УТ11 OData API",
            base_url: "http://<server>/UT11/odata/standard.odata/",
            rate_limit_desc: "Страницы по 100, пауза 1 с между батчами",
        },
        ExternalApiInfo {
            name: "1С HTTP mpi_api/prices_dealer",
            base_url: "http://<server>/UT11/hs/mpi_api/prices_dealer",
            rate_limit_desc: "Один GET, полный снимок цен",
        },
    ],
    constraints: &[
        "Требует активного подключения к базе 1С (connection_id → a001_connection_1c)",
        "OData-сессия привязана к учётным данным пользователя 1С",
        "Цены: delete-all + insert, период из конфига не используется",
        "Планировщик должен быть включён в config.toml ([scheduled_tasks].enabled)",
    ],
    config_fields: &[TaskConfigField {
        key: "connection_id",
        label: "Подключение к 1С",
        hint: "UUID подключения из справочника «Подключения 1С» (a001)",
        field_type: TaskConfigFieldType::Text,
        required: true,
        default_value: None,
        min_value: None,
        max_value: None,
    }],
    max_duration_seconds: 7200,
};

pub struct Task031UtNomenclaturePricesManager {
    executor: Arc<ImportExecutor>,
}

impl Task031UtNomenclaturePricesManager {
    pub fn new(executor: Arc<ImportExecutor>) -> Self {
        Self { executor }
    }
}

#[async_trait]
impl TaskManager for Task031UtNomenclaturePricesManager {
    fn task_type(&self) -> &'static str {
        "task031_ut_nomenclature_prices"
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
        logger.write_log(
            session_id,
            "task031: nomenclature + dealer prices from 1C started",
        )?;

        let cfg: Config = serde_json::from_str(&task.config_json)
            .context("Config parse failed — expected {\"connection_id\":\"<uuid>\"}")?;

        let connection_id = super::config_helpers::parse_connection_id(&cfg.connection_id, "1С")?;
        let connection = crate::domain::a001_connection_1c::service::get_by_id(connection_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Connection 1C not found: {}", connection_id))?;

        let today = Utc::now().naive_utc().date().to_string();
        let req = ImportRequest {
            connection_id: cfg.connection_id,
            target_aggregates: vec![
                "a004_nomenclature".to_string(),
                "p906_prices".to_string(),
            ],
            mode: ImportMode::Background,
            delete_obsolete: false,
            period_from: Some(today.clone()),
            period_to: Some(today),
        };

        self.executor
            .execute_import(session_id, &req, &connection)
            .await?;

        logger.write_log(
            session_id,
            "task031: nomenclature + dealer prices from 1C completed",
        )?;
        Ok(TaskRunOutcome::completed())
    }

    fn get_progress(&self, session_id: &str) -> Option<TaskProgress> {
        self.executor
            .progress_tracker
            .get_progress(session_id)
            .map(|p| p.into())
    }

    fn list_live_progress_sessions(&self) -> Vec<TaskProgress> {
        self.executor.list_live_task_progress()
    }
}
