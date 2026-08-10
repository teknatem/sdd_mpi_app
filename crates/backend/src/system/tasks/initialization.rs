use anyhow::Result;
use std::sync::Arc;

use crate::usecases::{
    u501_import_from_ut, u502_import_from_ozon, u503_import_from_yandex,
    u504_import_from_wildberries,
};

use super::{
    logger::{get_global_task_logger, set_global_task_logger, TaskLogger},
    managers::{
        QualityCheckRunManager, Task001WbOrdersFbsPollingManager,
        Task002WbOrdersStatsHourlyManager, Task003WbProductsManager, Task004WbSalesManager,
        Task005WbSuppliesManager, Task006WbFinanceManager, Task007WbCommissionsManager,
        Task008WbPricesManager, Task009WbPromotionsManager, Task010WbDocumentsManager,
        Task011WbAdvertManager, Task012WbAdvertCampaignsManager, Task013YmOrdersPollingManager,
        Task014KbAnalyzeManager, Task015KbPostManager, Task016KbIntakeManager,
        Task017WbReturnsClaimsManager, Task018YmReturnsManager, Task019YmPaymentReportManager,
        Task020WbProductSnapshotManager, Task021MailIntakeManager, Task022MailReplyManager,
        Task023WbSalesFunnelDailyManager, Task024WbSearchAnalyticsDailyManager,
        Task025BitrixTicketSyncManager, Task026YmShowsSalesDailyManager, Task027LlmJudgeManager,
        Task028LlmGoldenSetManager, Task029AgentTaskRunnerManager, U501ImportUtManager,
        U502ImportOzonManager, U503ImportYandexManager,
    },
    registry::{set_global_registry, TaskManagerRegistry},
    worker::ScheduledTaskWorker,
};

/// Создаёт пару (ProgressTracker, ImportExecutor) для u504 — каждая WB-задача
/// получает собственную пару, чтобы прогресс-трекеры не конфликтовали.
macro_rules! wb_executor {
    () => {{
        let tracker = Arc::new(u504_import_from_wildberries::ProgressTracker::new());
        Arc::new(u504_import_from_wildberries::ImportExecutor::new(tracker))
    }};
}

/// Создаёт пару (ProgressTracker, ImportExecutor) для атомарных u503/Yandex-задач.
/// Каждая задача получает собственный трекер, чтобы live progress не смешивался.
macro_rules! ym_executor {
    () => {{
        let tracker = Arc::new(u503_import_from_yandex::ProgressTracker::new());
        Arc::new(u503_import_from_yandex::ImportExecutor::new(tracker))
    }};
}

/// Инициализирует реестр задач и фоновый воркер.
pub async fn initialize_scheduled_tasks() -> Result<ScheduledTaskWorker> {
    let mut registry = TaskManagerRegistry::new();
    // Гарантируем, что глобальный логгер создан с конфигурацией воркера.
    // Если хендлеры уже вызвали get_global_task_logger() раньше — вызов игнорируется.
    set_global_task_logger(Arc::new(TaskLogger::new("./task_logs")));
    let logger = get_global_task_logger();

    // ---- Non-WB usecases ----

    let u501_tracker = Arc::new(u501_import_from_ut::ProgressTracker::new());
    let u501_executor = Arc::new(u501_import_from_ut::ImportExecutor::new(u501_tracker));
    registry.register(U501ImportUtManager::new(u501_executor));

    let u502_tracker = Arc::new(u502_import_from_ozon::ProgressTracker::new());
    let u502_executor = Arc::new(u502_import_from_ozon::ImportExecutor::new(u502_tracker));
    registry.register(U502ImportOzonManager::new(u502_executor));

    let u503_tracker = Arc::new(u503_import_from_yandex::ProgressTracker::new());
    let u503_executor = Arc::new(u503_import_from_yandex::ImportExecutor::new(u503_tracker));
    registry.register(U503ImportYandexManager::new(u503_executor));

    // ---- WB atomic task managers — each owns its own executor + progress tracker ----

    registry.register(Task001WbOrdersFbsPollingManager::new(wb_executor!()));
    registry.register(Task002WbOrdersStatsHourlyManager::new(wb_executor!()));
    registry.register(Task003WbProductsManager::new(wb_executor!()));
    registry.register(Task004WbSalesManager::new(wb_executor!()));
    registry.register(Task005WbSuppliesManager::new(wb_executor!()));
    registry.register(Task006WbFinanceManager::new(wb_executor!()));
    registry.register(Task007WbCommissionsManager::new(wb_executor!()));
    registry.register(Task008WbPricesManager::new(wb_executor!()));
    registry.register(Task009WbPromotionsManager::new(wb_executor!()));
    registry.register(Task010WbDocumentsManager::new(wb_executor!()));
    registry.register(Task011WbAdvertManager::new(wb_executor!()));
    registry.register(Task012WbAdvertCampaignsManager::new(wb_executor!()));
    registry.register(Task020WbProductSnapshotManager::new(wb_executor!()));
    registry.register(Task023WbSalesFunnelDailyManager::new(wb_executor!()));
    registry.register(Task024WbSearchAnalyticsDailyManager::new(wb_executor!()));

    // ---- Yandex atomic task managers ----

    registry.register(Task013YmOrdersPollingManager::new(ym_executor!()));
    registry.register(Task018YmReturnsManager::new(ym_executor!()));
    registry.register(Task019YmPaymentReportManager::new(ym_executor!()));
    registry.register(Task026YmShowsSalesDailyManager::new(ym_executor!()));

    // ---- Knowledge base task managers ----

    registry.register(Task014KbAnalyzeManager::new());
    registry.register(Task015KbPostManager::new());
    registry.register(Task016KbIntakeManager::new());

    // ---- LLM quality task managers ----

    registry.register(Task027LlmJudgeManager::new());
    registry.register(Task028LlmGoldenSetManager::new());

    // ---- Agent task queue ----
    registry.register(Task029AgentTaskRunnerManager::new());

    // ---- WB Returns Claims task manager ----
    registry.register(Task017WbReturnsClaimsManager::new(wb_executor!()));

    // ---- Mail intake / reply task managers ----
    registry.register(Task021MailIntakeManager::new());
    registry.register(Task022MailReplyManager::new());
    registry.register(Task025BitrixTicketSyncManager::new());
    registry.register(QualityCheckRunManager::new());

    let registry = Arc::new(registry);
    set_global_registry(Arc::clone(&registry));

    let worker = ScheduledTaskWorker::new(registry, logger, 60);
    Ok(worker)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Забыть `registry.register(...)` для нового менеджера — ошибка, которую компилятор
    /// не ловит: задание молча не запускается, а seed-строка в sys_tasks остаётся без
    /// исполнителя. Проверяем, что типы, на которые ссылаются seed-миграции, в реестре есть.
    #[tokio::test]
    async fn seeded_task_types_are_registered() {
        initialize_scheduled_tasks()
            .await
            .expect("registry init failed");
        let registry = super::super::registry::get_global_registry().expect("no global registry");

        for task_type in [
            "task020_wb_product_snapshot",
            "task023_wb_sales_funnel_daily",
            "task024_wb_search_analytics_daily",
            "task025_bitrix_ticket_sync",
            "task027_llm_judge",
            "task028_llm_golden_set",
            "task029_agent_task_runner",
            "quality_check_run",
        ] {
            let manager = registry
                .get(task_type)
                .unwrap_or_else(|| panic!("менеджер не зарегистрирован: {task_type}"));
            // Ключ реестра берётся из task_type(); metadata должна описывать тот же тип.
            assert_eq!(manager.metadata().task_type, task_type);
        }
    }

    #[tokio::test]
    async fn every_registered_task_declares_write_tables() {
        initialize_scheduled_tasks()
            .await
            .expect("registry init failed");
        let registry = super::super::registry::get_global_registry().expect("no global registry");

        for metadata in registry.list_metadata() {
            assert!(
                !metadata.write_tables.is_empty(),
                "task type '{}' must declare its business write tables",
                metadata.task_type
            );
        }
    }
}
