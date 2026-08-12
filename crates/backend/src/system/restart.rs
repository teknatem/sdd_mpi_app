//! Координация штатного перезапуска процесса после подготовки подмены БД.
//!
//! Приложение не запускает вторую копию самого себя: под Windows это даёт гонку
//! за порт и конфликтует с NSSM. Оно корректно завершает HTTP-сервер, а внешний
//! supervisor поднимает процесс снова. Для NSSM действие `Restart` является
//! стандартным и дополнительно закреплено в инструкции развёртывания.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use once_cell::sync::Lazy;
use tokio::sync::Notify;

static REQUESTED: AtomicBool = AtomicBool::new(false);
static SHUTDOWN: Lazy<Notify> = Lazy::new(Notify::new);
const FORCE_EXIT_AFTER: Duration = Duration::from_secs(15);
const RESTART_EXIT_CODE: i32 = 75;

/// Запланировать единственный перезапуск. Повторные вызовы безопасны: несколько
/// завершившихся веток не создадут несколько таймеров завершения процесса.
pub fn schedule(delay: Duration) {
    if REQUESTED.swap(true, Ordering::AcqRel) {
        return;
    }
    tracing::warn!(
        "server: automatic restart scheduled in {:.1}s",
        delay.as_secs_f64()
    );
    tokio::spawn(async move {
        tokio::time::sleep(delay).await;
        SHUTDOWN.notify_one();
        // `with_graceful_shutdown` ждёт активные соединения без собственного
        // дедлайна. Maintenance уже закрыл прикладные запросы, поэтому после
        // разумного окна безопаснее гарантировать рестарт, чем навсегда зависнуть
        // из-за одного keep-alive клиента.
        tokio::time::sleep(FORCE_EXIT_AFTER).await;
        tracing::error!(
            "server: graceful shutdown did not finish in {}s; forcing exit",
            FORCE_EXIT_AFTER.as_secs()
        );
        std::process::exit(RESTART_EXIT_CODE);
    });
}

/// Future для `axum::serve(...).with_graceful_shutdown(...)`.
pub async fn wait() {
    SHUTDOWN.notified().await;
    tracing::warn!("server: graceful shutdown started for automatic restart");
}

pub fn is_requested() -> bool {
    REQUESTED.load(Ordering::Acquire)
}
