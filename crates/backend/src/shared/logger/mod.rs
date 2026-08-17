pub mod repository;

use repository::log_event_internal;

/// Логирование события на сервере
///
/// # Примеры
///
/// `no_run`: запись идёт в базу, а у доктеста её нет.
/// ```no_run
/// use backend::shared::logger;
///
/// logger::log("startup", "Сервер запущен");
/// logger::log("api", "Получен запрос к /api/marketplace");
/// ```
pub fn log(category: &str, message: &str) {
    log_event_internal("server", category, message);
}
