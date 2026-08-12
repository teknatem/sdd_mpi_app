//! Фоновые задачи переноса и их прогресс.
//!
//! Выгрузка снапшота с базой данных — это VACUUM INTO нескольких гигабайт,
//! сжатие и многочастная загрузка: минуты работы. Держать на этом открытый
//! HTTP-запрос нельзя, поэтому операция уезжает в фон, а клиент опрашивает
//! состояние. Связка «увидел этот дифф — подтвердил этот дифф» при этом не
//! теряется: она держится на `expected_bundle_sha256` внутри запроса, а не на
//! синхронности вызова.
//!
//! Планировщик (`system/tasks`) намеренно не используется: он выключен
//! конфигом, живёт вокруг `ScheduledTask` и `sys_task_runs` и не рассчитан на
//! ручную админскую операцию, запускаемую из UI.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, RwLock};

use chrono::{DateTime, Duration, Utc};
use contracts::system::datasets::{
    CreateSnapshotResponse, DatasetJobDto, DatasetJobKind, DatasetJobStatus, RestoreResultDto,
};
use once_cell::sync::Lazy;
use uuid::Uuid;

/// Сколько держать завершённую задачу в памяти. Фронт опрашивает раз в секунду,
/// но вкладку могли закрыть и вернуться — терминальное состояние должно
/// пережить перезагрузку страницы.
const RETENTION_MINUTES: i64 = 30;

static JOBS: Lazy<RwLock<HashMap<String, JobHandle>>> = Lazy::new(Default::default);

/// Идентификатор выполняющейся задачи. Одновременно допускается ровно одна:
/// снапшот и восстановление трогают одни и те же каталоги, а VACUUM INTO двух
/// экземпляров разом просто удвоил бы нагрузку на диск.
static ACTIVE: Lazy<Mutex<Option<String>>> = Lazy::new(Default::default);

struct Terminal {
    status: DatasetJobStatus,
    finished_at: Option<DateTime<Utc>>,
    error: Option<String>,
    snapshot_result: Option<CreateSnapshotResponse>,
    restore_result: Option<RestoreResultDto>,
}

struct JobInner {
    job_id: String,
    kind: DatasetJobKind,
    stages: Vec<String>,
    stage_index: AtomicUsize,
    bytes_done: AtomicU64,
    bytes_total: AtomicU64,
    cancelled: AtomicBool,
    started_at: DateTime<Utc>,
    terminal: Mutex<Terminal>,
}

/// Ручка задачи: раздаётся исполняющему коду, чтобы двигать прогресс и
/// спрашивать про отмену. Клонируется свободно — внутри `Arc`.
#[derive(Clone)]
pub struct JobHandle(Arc<JobInner>);

impl JobHandle {
    pub fn job_id(&self) -> &str {
        &self.0.job_id
    }

    /// Переключает стадию и обнуляет счётчик байт: байты имеют смысл только
    /// внутри своей стадии (скачали архив → скачиваем базу → это разные шкалы).
    pub fn set_stage(&self, index: usize) {
        self.0.stage_index.store(index, Ordering::Relaxed);
        self.0.bytes_done.store(0, Ordering::Relaxed);
        self.0.bytes_total.store(0, Ordering::Relaxed);
    }

    pub fn set_bytes_total(&self, total: u64) {
        self.0.bytes_total.store(total, Ordering::Relaxed);
    }

    pub fn add_bytes(&self, delta: u64) {
        self.0.bytes_done.fetch_add(delta, Ordering::Relaxed);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.cancelled.load(Ordering::Relaxed)
    }

    /// Точка выхода по отмене. Вызывается между частями загрузки, чанками
    /// скачивания и между наборами — то есть там, где состояние на диске и в
    /// бакете ещё можно оставить чистым.
    pub fn check_cancelled(&self) -> anyhow::Result<()> {
        if self.is_cancelled() {
            anyhow::bail!("Операция прервана пользователем");
        }
        Ok(())
    }

    fn finish(&self, status: DatasetJobStatus, apply: impl FnOnce(&mut Terminal)) {
        if let Ok(mut terminal) = self.0.terminal.lock() {
            terminal.status = status;
            terminal.finished_at = Some(Utc::now());
            apply(&mut terminal);
        }
        release_active(&self.0.job_id);
    }

    pub fn finish_snapshot(&self, response: CreateSnapshotResponse) {
        self.finish(DatasetJobStatus::Done, |terminal| {
            terminal.snapshot_result = Some(response);
        });
    }

    pub fn finish_restore(&self, response: RestoreResultDto) {
        self.finish(DatasetJobStatus::Done, |terminal| {
            terminal.restore_result = Some(response);
        });
    }

    /// Прерванная задача — не сбой: показывать её красной ошибкой неверно.
    pub fn finish_error(&self, error: &anyhow::Error) {
        let status = if self.is_cancelled() {
            DatasetJobStatus::Cancelled
        } else {
            DatasetJobStatus::Failed
        };
        let message = error.to_string();
        self.finish(status, |terminal| terminal.error = Some(message));
    }

    fn to_dto(&self) -> DatasetJobDto {
        let inner = &self.0;
        let stage_index = inner.stage_index.load(Ordering::Relaxed);
        let terminal = inner.terminal.lock().ok();
        let (status, finished_at, error, snapshot_result, restore_result) = match terminal {
            Some(terminal) => (
                terminal.status,
                terminal.finished_at.map(|at| at.to_rfc3339()),
                terminal.error.clone(),
                terminal.snapshot_result.clone(),
                terminal.restore_result.clone(),
            ),
            None => (DatasetJobStatus::Running, None, None, None, None),
        };

        DatasetJobDto {
            job_id: inner.job_id.clone(),
            kind: inner.kind,
            status,
            stage_index,
            stage_label: inner.stages.get(stage_index).cloned().unwrap_or_default(),
            stages: inner.stages.clone(),
            bytes_done: inner.bytes_done.load(Ordering::Relaxed),
            bytes_total: inner.bytes_total.load(Ordering::Relaxed),
            started_at: inner.started_at.to_rfc3339(),
            finished_at,
            error,
            snapshot_result,
            restore_result,
        }
    }

    fn is_running(&self) -> bool {
        self.0
            .terminal
            .lock()
            .map(|terminal| terminal.status == DatasetJobStatus::Running)
            .unwrap_or(false)
    }
}

/// Страховка от задачи, которая завершилась, не отчитавшись, — паникой внутри
/// исполняющего кода. Без неё слот «идёт операция» остался бы занятым до
/// перезапуска процесса, и подсистема переноса выглядела бы намертво зависшей.
///
/// Владеет им сам исполнитель: при обычном завершении статус уже терминальный и
/// `drop` ничего не делает, при развороте стека — помечает задачу упавшей и
/// освобождает слот.
pub struct ActiveGuard(JobHandle);

impl ActiveGuard {
    pub fn new(job: JobHandle) -> Self {
        Self(job)
    }
}

impl Drop for ActiveGuard {
    fn drop(&mut self) {
        if self.0.is_running() {
            self.0.finish_error(&anyhow::anyhow!(
                "Операция прервана аварийно: исполняющая задача завершилась без результата. \
                 Подробности — в логах сервера."
            ));
        }
    }
}

/// Стадии выгрузки. Порядок совпадает с порядком работы `publish::create_snapshot`.
pub mod stage {
    pub const SNAPSHOT_STAGES: &[&str] = &[
        "Сканирование файлов",
        "Сборка архива",
        "Снимок БД (VACUUM INTO)",
        "Проверка целостности",
        "Выгрузка в S3",
        "Запись манифеста и каталога",
    ];
    pub const SNAPSHOT_SCAN: usize = 0;
    pub const SNAPSHOT_BUNDLE: usize = 1;
    pub const SNAPSHOT_DB_DUMP: usize = 2;
    pub const SNAPSHOT_DB_VERIFY: usize = 3;
    pub const SNAPSHOT_UPLOAD: usize = 4;
    pub const SNAPSHOT_CATALOG: usize = 5;

    pub const RESTORE_STAGES: &[&str] = &[
        "Чтение манифеста",
        "Скачивание архива",
        "Скачивание БД",
        "Распаковка и проверка",
        "Применение файловых наборов",
        "Подготовка подмены БД",
    ];
    pub const RESTORE_MANIFEST: usize = 0;
    pub const RESTORE_DOWNLOAD_FILES: usize = 1;
    pub const RESTORE_DOWNLOAD_DB: usize = 2;
    pub const RESTORE_VERIFY: usize = 3;
    pub const RESTORE_APPLY_FILES: usize = 4;
    pub const RESTORE_STAGE_DB: usize = 5;
}

/// Регистрирует новую задачу. Ошибка означает, что другая операция ещё идёт —
/// вызывающий отвечает 409, а не 500.
pub fn start(kind: DatasetJobKind, stages: &[&str]) -> anyhow::Result<JobHandle> {
    sweep();

    let job_id = Uuid::new_v4().to_string();
    {
        let mut active = ACTIVE
            .lock()
            .map_err(|_| anyhow::anyhow!("Реестр операций переноса повреждён"))?;
        if let Some(running) = active.as_ref() {
            anyhow::bail!(
                "Операция переноса уже выполняется ({running}). Дождитесь её завершения \
                 или прервите её."
            );
        }
        *active = Some(job_id.clone());
    }

    let handle = JobHandle(Arc::new(JobInner {
        job_id: job_id.clone(),
        kind,
        stages: stages.iter().map(ToString::to_string).collect(),
        stage_index: AtomicUsize::new(0),
        bytes_done: AtomicU64::new(0),
        bytes_total: AtomicU64::new(0),
        cancelled: AtomicBool::new(false),
        started_at: Utc::now(),
        terminal: Mutex::new(Terminal {
            status: DatasetJobStatus::Running,
            finished_at: None,
            error: None,
            snapshot_result: None,
            restore_result: None,
        }),
    }));

    if let Ok(mut jobs) = JOBS.write() {
        jobs.insert(job_id, handle.clone());
    }
    Ok(handle)
}

fn release_active(job_id: &str) {
    if let Ok(mut active) = ACTIVE.lock() {
        if active.as_deref() == Some(job_id) {
            *active = None;
        }
    }
}

pub fn get(job_id: &str) -> Option<DatasetJobDto> {
    JOBS.read().ok()?.get(job_id).map(|handle| handle.to_dto())
}

/// Текущая выполняющаяся задача — по ней страница переподключается к операции,
/// запущенной до перезагрузки вкладки.
pub fn active() -> Option<DatasetJobDto> {
    let job_id = ACTIVE.lock().ok()?.clone()?;
    get(&job_id)
}

/// Помечает задачу отменённой. Само прерывание происходит в исполняющем коде на
/// ближайшей проверке — незавершённая многочастная загрузка при этом
/// отменяется, а временные файлы удаляются.
pub fn cancel(job_id: &str) -> bool {
    let Ok(jobs) = JOBS.read() else {
        return false;
    };
    match jobs.get(job_id) {
        Some(handle) if handle.is_running() => {
            handle.0.cancelled.store(true, Ordering::Relaxed);
            true
        }
        _ => false,
    }
}

/// Выбрасывает давно завершённые задачи. Вызывается на старте новой операции —
/// отдельный таймер ради полудюжины записей в час избыточен.
fn sweep() {
    let threshold = Utc::now() - Duration::minutes(RETENTION_MINUTES);
    if let Ok(mut jobs) = JOBS.write() {
        jobs.retain(|_, handle| {
            handle
                .0
                .terminal
                .lock()
                .ok()
                .and_then(|terminal| terminal.finished_at)
                .is_none_or(|finished| finished > threshold)
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Тесты трогают общий на процесс реестр, поэтому идут под одним мьютексом
    /// и убирают за собой.
    static SERIAL: Mutex<()> = Mutex::new(());

    fn cleanup(handle: &JobHandle) {
        release_active(handle.job_id());
        if let Ok(mut jobs) = JOBS.write() {
            jobs.remove(handle.job_id());
        }
    }

    #[test]
    fn second_job_is_rejected_while_the_first_runs() {
        let _guard = SERIAL.lock().unwrap_or_else(|error| error.into_inner());
        let first = start(DatasetJobKind::Snapshot, stage::SNAPSHOT_STAGES).unwrap();

        let second = start(DatasetJobKind::Restore, stage::RESTORE_STAGES);
        assert!(second.is_err(), "две операции переноса разом недопустимы");

        first.finish_error(&anyhow::anyhow!("прервано"));
        // Слот освободился — следующая задача стартует.
        let third = start(DatasetJobKind::Restore, stage::RESTORE_STAGES).unwrap();
        cleanup(&third);
        cleanup(&first);
    }

    #[test]
    fn cancel_marks_running_job_and_finish_reports_cancelled() {
        let _guard = SERIAL.lock().unwrap_or_else(|error| error.into_inner());
        let handle = start(DatasetJobKind::Snapshot, stage::SNAPSHOT_STAGES).unwrap();

        assert!(cancel(handle.job_id()));
        assert!(handle.check_cancelled().is_err());

        handle.finish_error(&anyhow::anyhow!("Операция прервана пользователем"));
        let dto = get(handle.job_id()).unwrap();
        // Отменённая задача не должна выглядеть как сбой.
        assert_eq!(dto.status, DatasetJobStatus::Cancelled);
        cleanup(&handle);
    }

    #[test]
    fn switching_stage_resets_byte_counters() {
        let _guard = SERIAL.lock().unwrap_or_else(|error| error.into_inner());
        let handle = start(DatasetJobKind::Snapshot, stage::SNAPSHOT_STAGES).unwrap();

        handle.set_bytes_total(100);
        handle.add_bytes(70);
        handle.set_stage(stage::SNAPSHOT_UPLOAD);

        let dto = get(handle.job_id()).unwrap();
        assert_eq!(dto.stage_index, stage::SNAPSHOT_UPLOAD);
        assert_eq!(dto.stage_label, "Выгрузка в S3");
        assert_eq!((dto.bytes_done, dto.bytes_total), (0, 0));

        handle.finish_error(&anyhow::anyhow!("конец"));
        cleanup(&handle);
    }
}
