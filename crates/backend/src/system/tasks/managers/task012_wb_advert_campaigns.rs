use anyhow::{Context, Result};
use async_trait::async_trait;
use chrono::Utc;
use contracts::system::tasks::aggregate::ScheduledTask;
use contracts::system::tasks::metadata::{
    ExternalApiInfo, TaskConfigField, TaskConfigFieldType, TaskMetadata,
};
use contracts::system::tasks::progress::TaskProgress;
use contracts::usecases::u504_import_from_wildberries::request::{ImportMode, ImportRequest};
use serde::Deserialize;
use std::sync::Arc;

use crate::system::tasks::logger::TaskLogger;
use crate::system::tasks::manager::{TaskManager, TaskRunOutcome};
use crate::usecases::u504_import_from_wildberries::ImportExecutor;

#[derive(Deserialize)]
struct Config {
    connection_id: String,
}

static METADATA: TaskMetadata = TaskMetadata {
    task_type: "task012_wb_advert_campaigns",
    write_tables: &["a030_wb_advert_campaign"],
    display_name: "WB Реклама (кампании)",
    description: "Обновляет справочник рекламных кампаний WB: список advertId, тип, статус, \
        changeTime и сырые свойства кампаний из Advert API. Данные сохраняются в \
        a030_wb_advert_campaign и используются задачей статистики. \
        Инкрементальная стратегия: новые и изменённые кампании загружаются всеми пачками \
        /api/advert/v2/adverts (до 50 кампаний в пачке). Кампании без изменений (change_time \
        совпадает и info_json заполнен) пропускаются. Между пачками выдерживается 250 мс. \
        Счётчики в мониторинге: «Обработано» — все кампании из WB; «Новые» — физически \
        добавленные записи в БД впервые; «Изменено» — кампании, получившие свежий info_json \
        из API в этом запуске (новые кампании входят в оба счётчика).",
    external_apis: &[
        ExternalApiInfo {
            name: "WB Advert API — campaign list",
            base_url: "https://advert-api.wildberries.ru/adv/v1/promotion/count",
            rate_limit_desc: "Официально 5 запросов/с, интервал 200 мс, burst 5",
        },
        ExternalApiInfo {
            name: "WB Advert API — campaign info",
            base_url: "https://advert-api.wildberries.ru/api/advert/v2/adverts",
            rate_limit_desc: "Официально 5 запросов/с, интервал 200 мс, burst 5. \
                Задача использует последовательные пачки ≤50 advertId с интервалом 250 мс \
                и ограниченным retry по rate-limit headers.",
        },
    ],
    constraints: &[
        "Требует API-токена WB с правами на Advert API",
        "Все новые/изменённые кампании обрабатываются за один запуск пачками до 50 ID",
        "fullstats берёт advertId из a030_wb_advert_campaign",
        "Существующий info_json сохраняется даже если API вернул 429",
    ],
    config_fields: &[TaskConfigField {
        key: "connection_id",
        label: "WB Кабинет",
        hint: "Подключение к Wildberries из справочника «Подключения маркетплейсов»",
        field_type: TaskConfigFieldType::ConnectionMp,
        required: true,
        default_value: None,
        min_value: None,
        max_value: None,
    }],
    max_duration_seconds: 3600,
};

pub struct Task012WbAdvertCampaignsManager {
    executor: Arc<ImportExecutor>,
}

impl Task012WbAdvertCampaignsManager {
    pub fn new(executor: Arc<ImportExecutor>) -> Self {
        Self { executor }
    }
}

#[async_trait]
impl TaskManager for Task012WbAdvertCampaignsManager {
    fn task_type(&self) -> &'static str {
        "task012_wb_advert_campaigns"
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
        logger.write_log(session_id, "task012: WB Advert campaigns sync started")?;

        let cfg: Config = serde_json::from_str(&task.config_json)
            .context("Config parse failed — expected {\"connection_id\":\"<uuid>\"}")?;

        let connection_id =
            super::config_helpers::parse_connection_id(&cfg.connection_id, "Wildberries")?;
        let connection = crate::domain::a006_connection_mp::service::get_by_id(connection_id)
            .await?
            .ok_or_else(|| {
                anyhow::anyhow!("Marketplace connection not found: {}", connection_id)
            })?;

        let today = Utc::now().naive_utc().date();
        let req = ImportRequest {
            connection_id: cfg.connection_id,
            target_aggregates: vec!["a030_wb_advert_campaign".to_string()],
            date_from: today,
            date_to: today,
            mode: ImportMode::Background,
        };

        self.executor
            .execute_import(session_id, &req, &connection)
            .await?;

        logger.write_log(session_id, "task012: WB Advert campaigns sync completed")?;
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
