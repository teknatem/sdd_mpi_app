use anyhow::{Context, Result};
use async_trait::async_trait;
use contracts::system::tasks::aggregate::ScheduledTask;
use contracts::system::tasks::metadata::{
    ExternalApiInfo, TaskConfigField, TaskConfigFieldType, TaskMetadata,
};
use contracts::system::tasks::progress::TaskProgress;
use contracts::usecases::u503_import_from_yandex::progress::ImportStatus;
use contracts::usecases::u503_import_from_yandex::request::{ImportMode, ImportRequest};
use serde::Deserialize;
use std::sync::Arc;

use crate::system::tasks::logger::TaskLogger;
use crate::system::tasks::manager::{TaskManager, TaskRunOutcome};
use crate::usecases::u503_import_from_yandex::ImportExecutor;

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------

fn default_work_start_date() -> String {
    "2026-01-01".to_string()
}
fn default_overlap_days() -> i64 {
    3
}
fn default_chunk_days() -> i64 {
    30
}

#[derive(Deserialize)]
struct Config {
    connection_id: String,
    #[serde(default = "default_work_start_date")]
    work_start_date: String,
    /// YM досчитывает статистику воронки задним числом ещё несколько дней после даты,
    /// поэтому хвост окна всегда перезагружается.
    #[serde(default = "default_overlap_days")]
    overlap_days: i64,
    #[serde(default = "default_chunk_days")]
    chunk_days: i64,
}

// ---------------------------------------------------------------------------
// Metadata
// ---------------------------------------------------------------------------

static METADATA: TaskMetadata = TaskMetadata {
    task_type: "task026_ym_shows_sales_daily",
    write_tables: &[
        "a041_ym_shows_sales_daily",
        "p916_mp_sales_funnel_turnovers",
    ],
    display_name: "YM Воронка продаж (Аналитика продаж)",
    description: "Загружает отчёт «Аналитика продаж» Yandex Market \
        (POST /v2/reports/shows-sales/generate, grouping=OFFERS) — единственный источник \
        показов и кликов YM: в orders-API их нет. Асинхронный процесс: генерация → опрос \
        статуса → скачивание → разбор строк offer_id × день. Данные ложатся в \
        a041_ym_shows_sales_daily и в стадию marketing воронки p916 (показы, клики, \
        корзина, заказы, отмены и невыкупы). Окно управляется watermark: грузит порциями \
        chunk_days с перекрытием overlap_days и догоняет до сегодня. Период заменяется \
        целиком (replace_for_period), поэтому повторная загрузка безопасна и подхватывает \
        пересчёты YM задним числом.",
    external_apis: &[ExternalApiInfo {
        name: "Yandex Market Partner API (Reports)",
        base_url: "https://api.partner.market.yandex.ru/",
        rate_limit_desc: "Асинхронный отчёт; число одновременных генераций ограничено тарифом \
            (без подписки — 1). Опрос статуса до готовности, затем скачивание файла",
    }],
    constraints: &[
        "Требует подключение Yandex Market с API Key или OAuth 2.0",
        "Отчёт содержит данные всех магазинов кабинета; один campaignId используется только как контекст API-запроса",
        "ГЛУБИНА ИСТОРИИ ОГРАНИЧЕНА ТАРИФОМ YM: 90 дней без подписки и на «Лайт», 400 на «Медиум». \
         Пропущенный период за пределами этого окна восстановить нельзя",
        "overlap_days (по умолчанию 3) компенсирует пересчёт статистики задним числом",
        "chunk_days (по умолчанию 30) — максимальный диапазон за один запуск",
        "Счётчики отчёта — дневная статистика маркетплейса; они НЕ равны фактическим \
         заказам/отменам из a013_ym_order (те считаются по документам заказов)",
    ],
    config_fields: &[
        TaskConfigField {
            key: "connection_id",
            label: "Кабинет Яндекс Маркет",
            hint: "Подключение к Yandex Market Partner API из справочника «Подключения маркетплейсов»",
            field_type: TaskConfigFieldType::ConnectionMp,
            required: true,
            default_value: None,
            min_value: None,
            max_value: None,
        },
        TaskConfigField {
            key: "work_start_date",
            label: "Дата начала работы",
            hint: "Начиная с этой даты данные должны быть загружены полностью \
                   (глубже лимита тарифа YM отчёт всё равно пуст)",
            field_type: TaskConfigFieldType::Date,
            required: false,
            default_value: Some("2026-01-01"),
            min_value: None,
            max_value: None,
        },
        TaskConfigField {
            key: "overlap_days",
            label: "Перекрытие от watermark (дн)",
            hint: "Запас назад от watermark: YM досчитывает статистику воронки задним числом",
            field_type: TaskConfigFieldType::Integer,
            required: false,
            default_value: Some("3"),
            min_value: Some(0),
            max_value: Some(30),
        },
        TaskConfigField {
            key: "chunk_days",
            label: "Размер порции (дн)",
            hint: "Максимальный диапазон за один запуск при догоняющей загрузке",
            field_type: TaskConfigFieldType::Integer,
            required: false,
            default_value: Some("30"),
            min_value: Some(1),
            max_value: Some(90),
        },
    ],
    max_duration_seconds: 3600,
};

// ---------------------------------------------------------------------------
// Manager
// ---------------------------------------------------------------------------

/// Регламентное задание загрузки воронки продаж YM (task026). Watermark-стратегия
/// по образцу task019; запускает u503 только для a041_ym_shows_sales_daily.
pub struct Task026YmShowsSalesDailyManager {
    executor: Arc<ImportExecutor>,
}

impl Task026YmShowsSalesDailyManager {
    pub fn new(executor: Arc<ImportExecutor>) -> Self {
        Self { executor }
    }
}

#[async_trait]
impl TaskManager for Task026YmShowsSalesDailyManager {
    fn task_type(&self) -> &'static str {
        "task026_ym_shows_sales_daily"
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
            .context("Config parse failed — expected {\"connection_id\":\"<uuid>\",\"work_start_date\":\"2026-01-01\",\"overlap_days\":3,\"chunk_days\":30}")?;

        let connection_id =
            super::config_helpers::parse_connection_id(&cfg.connection_id, "Яндекс Маркет")?;
        let connection = crate::domain::a006_connection_mp::service::get_by_id(connection_id)
            .await?
            .ok_or_else(|| {
                anyhow::anyhow!("Marketplace connection not found: {}", connection_id)
            })?;

        let (date_from, date_to) = super::config_helpers::compute_date_window(
            task,
            &cfg.work_start_date,
            cfg.overlap_days,
            cfg.chunk_days,
        );

        logger.write_log(
            session_id,
            &format!(
                "task026 YM Shows-Sales: {} → {}; connection_id={}",
                date_from, date_to, cfg.connection_id
            ),
        )?;

        let req = ImportRequest {
            connection_id: cfg.connection_id,
            target_aggregates: vec!["a041_ym_shows_sales_daily".to_string()],
            date_from,
            date_to,
            mode: ImportMode::Background,
            incremental_by_update: false,
        };

        self.executor
            .execute_import(session_id, &req, &connection)
            .await?;

        let completed_with_errors = self
            .executor
            .get_progress(session_id)
            .map(|p| {
                p.total_errors > 0
                    || matches!(
                        p.status,
                        ImportStatus::CompletedWithErrors | ImportStatus::Failed
                    )
            })
            .unwrap_or(false);
        if completed_with_errors {
            logger.write_log(
                session_id,
                "task026 completed with errors; watermark NOT advanced — see progress/errors.",
            )?;
            return Ok(TaskRunOutcome::completed_with_errors());
        }

        logger.write_log(session_id, "task026: YM Shows-Sales completed")?;
        Ok(TaskRunOutcome::completed_loaded_to(date_to))
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
