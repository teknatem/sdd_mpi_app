//! Состояние режима технического обслуживания.
//!
//! Флаг живёт **только в памяти процесса** и намеренно не сохраняется ни в БД,
//! ни в файле. Причины:
//!
//! * Хранить его в базе бессмысленно там, где он нужнее всего: при
//!   восстановлении сама база и подменяется, вместе с флагом.
//! * Перезапуск снимает режим. Для автоматического включения это ровно нужное
//!   поведение (операция перезапуск не переживает), а для ручного — страховка:
//!   администратор, запершийся снаружи, чинит это перезапуском бэкенда.
//!
//! Быстрая проверка вынесена в `AtomicBool`: она выполняется на каждом запросе,
//! и брать ради неё блокировку не стоит. Подробности (причина, время, кто
//! включил) читаются только когда режим активен — то есть при сборке ответа 503.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use chrono::Utc;
use contracts::system::maintenance::{
    MaintenanceStatusDto, MaintenanceTrigger, MaintenanceUnavailableDto,
};
use once_cell::sync::Lazy;

static ACTIVE: AtomicBool = AtomicBool::new(false);
static DETAILS: Lazy<RwLock<Option<Details>>> = Lazy::new(Default::default);
static DATABASE_ACTIVITY: Lazy<Arc<tokio::sync::RwLock<()>>> =
    Lazy::new(|| Arc::new(tokio::sync::RwLock::new(())));

/// Разрешение фоновой/регламентной задаче работать с основной БД.
/// Удерживается весь срок выполнения задачи, включая запись её финального
/// статуса. При переносе БД эксклюзивная сторона барьера ждёт освобождения всех
/// таких разрешений и не пускает новые задачи.
pub struct DatabaseActivityPermit {
    _guard: tokio::sync::OwnedRwLockReadGuard<()>,
}

/// Эксклюзивная пауза фоновой активности БД на время выгрузки или загрузки.
pub struct DatabaseActivityPause {
    _guard: tokio::sync::OwnedRwLockWriteGuard<()>,
}

/// Начать фоновую работу с БД, только если обслуживание не включено и операция
/// переноса ещё не забрала эксклюзивный барьер.
pub fn try_begin_database_activity() -> Option<DatabaseActivityPermit> {
    if is_active() {
        return None;
    }
    let guard = Arc::clone(&DATABASE_ACTIVITY).try_read_owned().ok()?;
    // Закрывает гонку: maintenance мог включиться между первой проверкой и
    // получением read-lock. В этом случае разрешение сразу освобождается.
    if is_active() {
        return None;
    }
    Some(DatabaseActivityPermit { _guard: guard })
}

/// Дождаться завершения уже запущенных фоновых/регламентных задач и запретить
/// новые до уничтожения возвращённого guard.
pub async fn pause_database_activity() -> DatabaseActivityPause {
    DatabaseActivityPause {
        _guard: Arc::clone(&DATABASE_ACTIVITY).write_owned().await,
    }
}

#[derive(Clone)]
struct Details {
    reason: String,
    trigger: MaintenanceTrigger,
    since: String,
    started_by: String,
    requires_restart: bool,
}

/// Горячая проверка для middleware.
pub fn is_active() -> bool {
    ACTIVE.load(Ordering::Acquire)
}

pub fn status() -> MaintenanceStatusDto {
    if !is_active() {
        return MaintenanceStatusDto::default();
    }
    let details = DETAILS.read().ok().and_then(|guard| guard.clone());
    match details {
        Some(details) => MaintenanceStatusDto {
            active: true,
            reason: Some(details.reason),
            trigger: Some(details.trigger),
            since: Some(details.since),
            started_by: Some(details.started_by),
            requires_restart: details.requires_restart,
        },
        None => MaintenanceStatusDto {
            active: true,
            ..Default::default()
        },
    }
}

/// Единственная форма отказа «закрыто на технические работы»: её отдаёт и гейт,
/// и обработчик входа. Тело обязательно — по голому 503 человек на экране входа
/// не отличит работы от упавшего сервера, а `Retry-After` даёт интеграциям
/// повторить попытку позже.
pub fn unavailable_response() -> Response {
    let body: MaintenanceUnavailableDto = status().into();
    (
        StatusCode::SERVICE_UNAVAILABLE,
        [("Retry-After", "120")],
        Json(body),
    )
        .into_response()
}

/// Включает режим. Повторный вызов перезаписывает описание — так автоматическая
/// операция может уточнить причину поверх ранее включённого ручного режима, не
/// сбрасывая его.
pub fn enable(reason: String, trigger: MaintenanceTrigger, started_by: String) {
    if let Ok(mut details) = DETAILS.write() {
        *details = Some(Details {
            reason,
            trigger,
            since: Utc::now().to_rfc3339(),
            started_by,
            requires_restart: false,
        });
    }
    ACTIVE.store(true, Ordering::Release);
    tracing::warn!("maintenance: режим обслуживания ВКЛЮЧЁН");
}

pub fn disable() {
    ACTIVE.store(false, Ordering::Release);
    if let Ok(mut details) = DETAILS.write() {
        *details = None;
    }
    tracing::info!("maintenance: режим обслуживания снят");
}

/// Отмечает, что до продолжения работы нужен перезапуск процесса. Ставится,
/// когда подмена базы уже подготовлена: снимать режим в этом состоянии нельзя,
/// пока файл не встал на место.
pub fn mark_requires_restart() {
    if let Ok(mut guard) = DETAILS.write() {
        if let Some(details) = guard.as_mut() {
            details.requires_restart = true;
        }
    }
}

/// Аренда режима на время операции: снимает его в `Drop`, что бы ни случилось —
/// ошибка, отмена или паника внутри задачи.
///
/// `keep()` разоружает аренду. Нужен ровно один раз: восстановление базы должно
/// оставить приложение закрытым до перезапуска, иначе пользователи войдут и
/// начнут писать в базу, которую вот-вот заменят.
pub struct MaintenanceLease {
    /// `false`, если режим уже был включён до нас: чужой ручной режим снимать
    /// по окончании своей операции нельзя.
    owned: bool,
    released: bool,
}

impl MaintenanceLease {
    pub fn acquire(reason: impl Into<String>, started_by: impl Into<String>) -> Self {
        let already_active = is_active();
        if !already_active {
            enable(
                reason.into(),
                MaintenanceTrigger::Automatic,
                started_by.into(),
            );
        }
        Self {
            owned: !already_active,
            released: false,
        }
    }

    /// Оставить режим включённым после завершения операции.
    pub fn keep(mut self) {
        self.released = true;
        mark_requires_restart();
    }
}

impl Drop for MaintenanceLease {
    fn drop(&mut self) {
        if self.owned && !self.released {
            disable();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// Состояние общее на процесс — тесты идут по одному и убирают за собой.
    static SERIAL: Mutex<()> = Mutex::new(());

    #[test]
    fn lease_releases_the_mode_on_drop() {
        let _guard = SERIAL.lock().unwrap_or_else(|error| error.into_inner());
        assert!(!is_active());
        {
            let _lease = MaintenanceLease::acquire("выгрузка", "auto:test");
            assert!(is_active());
        }
        assert!(
            !is_active(),
            "аренда обязана снять режим при выходе из области"
        );
    }

    #[test]
    fn kept_lease_leaves_the_mode_on_and_demands_restart() {
        let _guard = SERIAL.lock().unwrap_or_else(|error| error.into_inner());
        {
            let lease = MaintenanceLease::acquire("восстановление", "auto:test");
            lease.keep();
        }
        assert!(is_active());
        assert!(status().requires_restart);
        disable();
    }

    /// Ручной режим админа не должен сниматься автоматической операцией,
    /// которая просто закончилась внутри него.
    #[test]
    fn lease_does_not_release_a_mode_it_did_not_enable() {
        let _guard = SERIAL.lock().unwrap_or_else(|error| error.into_inner());
        enable(
            "ручные работы".to_string(),
            MaintenanceTrigger::Manual,
            "user:1".to_string(),
        );
        {
            let _lease = MaintenanceLease::acquire("выгрузка", "auto:test");
        }
        assert!(is_active());
        assert_eq!(status().trigger, Some(MaintenanceTrigger::Manual));
        disable();
    }

    #[tokio::test]
    async fn database_pause_blocks_new_background_activity() {
        let pause = pause_database_activity().await;
        assert!(try_begin_database_activity().is_none());

        drop(pause);
        assert!(try_begin_database_activity().is_some());
    }
}
