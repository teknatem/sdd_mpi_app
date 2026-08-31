pub mod aggregate_repost;
pub mod executor;
pub mod progress_tracker;
pub mod projection_repost;

pub use aggregate_repost::AggregateBulkRepost;
pub use executor::{repost_ids_concurrent, RepostExecutor};
pub use progress_tracker::ProgressTracker;
pub use projection_repost::{ProjectionOptionInfo, ProjectionRepost, RepostContext};

/// Общий исполнитель перепроведения процесса.
///
/// Один на приложение намеренно: прогресс сессий читается общим
/// `GET /api/u508/repost/progress/:session_id`, и второй исполнитель со своим
/// трекером сделал бы часть сессий невидимыми. Раньше он был `Lazy`-статиком
/// внутри хендлера — туда же за ним ходил и пересбор воронки, что заставляло
/// хендлеры ядра знать про p916.
pub fn shared() -> &'static std::sync::Arc<RepostExecutor> {
    static EXECUTOR: std::sync::OnceLock<std::sync::Arc<RepostExecutor>> =
        std::sync::OnceLock::new();
    EXECUTOR.get_or_init(|| {
        std::sync::Arc::new(RepostExecutor::new(std::sync::Arc::new(
            ProgressTracker::new(),
        )))
    })
}
