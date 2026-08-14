//! Страница «Наборы данных и перенос».
//!
//! Вкладки на одной странице, а не отдельные страницы: все они опираются на
//! один и тот же список наборов и одно и то же состояние экземпляра — разносить
//! значит дублировать и загрузку, и состояние.

mod catalog_tab;
mod diagnostics_tab;
mod diff_dialog;
mod export_tab;
mod job_progress;
mod journal_tab;

use gloo_timers::future::TimeoutFuture;
use leptos::prelude::*;
use leptos::task::spawn_local;

use contracts::system::datasets::{
    DatasetCatalogResponse, DatasetJobDto, DatasetJobStatus, DatasetsStatusDto, TransferLogEntryDto,
};

use crate::shared::icons::icon;
use crate::shared::page_frame::PageFrame;
use crate::shared::page_standard::PAGE_CAT_SYSTEM;
use crate::system::auth::guard::RequireAdmin;
use crate::system::datasets::api;

pub use catalog_tab::CatalogTab;
pub use diagnostics_tab::DiagnosticsTab;
pub use export_tab::ExportTab;
pub use job_progress::JobProgress;
pub use journal_tab::JournalTab;

/// Как часто опрашивать состояние фоновой операции. Секунда — компромисс:
/// полоса движется заметно, а на многочасовой выгрузке это несколько тысяч
/// дешёвых запросов к памяти процесса, не к БД.
const JOB_POLL_MS: u32 = 1000;

/// Единый формат отметок времени на странице. Все даты приезжают с бэкенда в
/// UTC (RFC3339) и показываются в местном поясе — см. `format_datetime_utc_local`.
pub const DATETIME_FMT: &str = "%d.%m.%Y %H:%M:%S";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActiveTab {
    Export,
    Restore,
    Diagnostics,
    Journal,
}

#[component]
pub fn DatasetsPage() -> impl IntoView {
    view! {
        <RequireAdmin>
            <DatasetsContent />
        </RequireAdmin>
    }
}

/// Общее состояние страницы, чтобы вкладки не тянули его каждая по-своему.
#[derive(Clone, Copy)]
pub struct DatasetsState {
    pub status: RwSignal<Option<DatasetsStatusDto>>,
    pub catalog: RwSignal<Option<DatasetCatalogResponse>>,
    pub history: RwSignal<Vec<TransferLogEntryDto>>,
    pub loading: RwSignal<bool>,
    /// Короткие синхронные операции (предпросмотр, удаление, скачивание) —
    /// текстовая плашка со спиннером. Длительные живут в `job`.
    pub busy_label: RwSignal<Option<String>>,
    pub error: RwSignal<Option<String>>,
    pub notice: RwSignal<Option<String>>,
    pub reload: Callback<()>,
    /// Состояние фоновой операции переноса, если она есть.
    pub job: RwSignal<Option<DatasetJobDto>>,
    /// Подключает опрос к задаче с этим идентификатором.
    pub watch_job: Callback<String>,
}

/// Итог завершённой операции одной строкой. Подробности (ключ объекта,
/// предупреждения, требование перезапуска) остаются во вкладках.
fn job_summary(job: &DatasetJobDto) -> String {
    if let Some(snapshot) = &job.snapshot_result {
        return format!(
            "Снапшот {} записан ({})",
            snapshot.snapshot.snapshot_id,
            crate::shared::date_utils::format_bytes_compact(snapshot.bundle_size_bytes)
        );
    }
    if let Some(restore) = &job.restore_result {
        let written: u64 = restore.sets.iter().map(|set| set.files_written).sum();
        let removed: u64 = restore.sets.iter().map(|set| set.files_deleted).sum();
        let restart = if restore.requires_restart {
            " Бэкенд автоматически перезапускается."
        } else {
            ""
        };
        return format!("Восстановлено: записано {written} файлов, удалено {removed}.{restart}");
    }
    "Операция завершена".to_string()
}

#[component]
fn DatasetsContent() -> impl IntoView {
    let status = RwSignal::<Option<DatasetsStatusDto>>::new(None);
    let catalog = RwSignal::<Option<DatasetCatalogResponse>>::new(None);
    let history = RwSignal::<Vec<TransferLogEntryDto>>::new(Vec::new());
    let loading = RwSignal::new(false);
    let busy_label = RwSignal::<Option<String>>::new(None);
    let error = RwSignal::<Option<String>>::new(None);
    let notice = RwSignal::<Option<String>>::new(None);
    let active_tab = RwSignal::new(ActiveTab::Export);
    // Ошибка каталога держится отдельно от ошибки страницы: ненастроенный S3 не
    // должен мешать смотреть локальное состояние наборов.
    let catalog_error = RwSignal::<Option<String>>::new(None);

    let reload = Callback::new(move |_| {
        loading.set(true);
        error.set(None);
        spawn_local(async move {
            match api::fetch_status().await {
                Ok(next) => status.set(Some(next)),
                Err(err) => error.set(Some(err)),
            }
            match api::fetch_catalog().await {
                Ok(next) => {
                    catalog.set(Some(next));
                    catalog_error.set(None);
                }
                Err(err) => catalog_error.set(Some(err)),
            }
            if let Ok(log) = api::fetch_history().await {
                history.set(log.items);
            }
            loading.set(false);
        });
    });

    let job = RwSignal::<Option<DatasetJobDto>>::new(None);

    // Опрос живёт на уровне страницы, а не вкладки: выгрузка стартует из одной
    // вкладки, восстановление из другой, а переключение вкладок не должно
    // обрывать наблюдение за операцией.
    let watch_job = Callback::new(move |job_id: String| {
        spawn_local(async move {
            loop {
                match api::fetch_job(&job_id).await {
                    Ok(dto) => {
                        let status = dto.status;
                        let finished = status.is_terminal();
                        job.set(Some(dto.clone()));
                        if finished {
                            match status {
                                DatasetJobStatus::Done => {
                                    notice.set(Some(job_summary(&dto)));
                                    error.set(None);
                                }
                                DatasetJobStatus::Cancelled => {
                                    notice.set(Some("Операция прервана.".to_string()));
                                }
                                _ => error.set(Some(dto.error.clone().unwrap_or_else(|| {
                                    "Операция завершилась с ошибкой".to_string()
                                }))),
                            }
                            // Результат остаётся на экране: он несёт ключ объекта,
                            // предупреждения и требование перезапуска.
                            reload.run(());
                            break;
                        }
                    }
                    // Задачу выгрузили из памяти либо связь пропала — прекращаем
                    // опрос, чтобы не крутить бесконечный цикл ошибок.
                    Err(message) => {
                        error.set(Some(message));
                        job.set(None);
                        break;
                    }
                }
                TimeoutFuture::new(JOB_POLL_MS).await;
            }
        });
    });

    Effect::new(move |_| {
        reload.run(());
        // Операция могла быть запущена до перезагрузки вкладки — подхватываем её.
        spawn_local(async move {
            if let Ok(Some(active)) = api::fetch_active_job().await {
                let job_id = active.job_id.clone();
                job.set(Some(active));
                watch_job.run(job_id);
            }
        });
    });

    let state = DatasetsState {
        status,
        catalog,
        history,
        loading,
        busy_label,
        error,
        notice,
        reload,
        job,
        watch_job,
    };

    // Число замечаний выносим на корешок вкладки: иначе диагностика видна
    // только тому, кто на неё специально зашёл.
    let anomaly_count = Signal::derive(move || {
        status
            .get()
            .map(|status| status.anomalies.len())
            .unwrap_or(0)
    });

    let tab_class = move |tab: ActiveTab| {
        if active_tab.get() == tab {
            "detail-tabs__item detail-tabs__item--active"
        } else {
            "detail-tabs__item"
        }
    };

    view! {
        <PageFrame page_id="sys_datasets--system" category=PAGE_CAT_SYSTEM class="page--wide">
            <div class="page__header">
                <div class="page__header-left">
                    <h1 class="page__title">"Наборы данных и перенос"</h1>
                    <p class="page__subtitle">
                        "Выборочная запись файловых данных в S3 и восстановление их на другом экземпляре."
                    </p>
                </div>
                <div class="page__header-right">
                    <button
                        class="button button--secondary"
                        disabled=move || loading.get()
                        on:click=move |_| reload.run(())
                    >
                        {icon("refresh-cw")}
                        {move || if loading.get() { "Обновление..." } else { "Обновить" }}
                    </button>
                </div>
            </div>

            <div class="page__content">
                <Show when=move || busy_label.get().is_some()>
                    <div class="datasets__busy-banner">
                        <span class="page-action-button__spinner"></span>
                        <span>{move || busy_label.get().unwrap_or_default()}</span>
                    </div>
                </Show>

                {move || job.get()
                    .filter(|dto| dto.status == DatasetJobStatus::Running)
                    .map(|dto| view! { <JobProgress job=dto /> })}

                {move || error.get().map(|err| view! {
                    <div class="alert alert--error">{err}</div>
                })}
                {move || notice.get().map(|msg| view! {
                    <div class="alert alert--success">{msg}</div>
                })}

                <div class="detail-tabs">
                    <button
                        class=move || tab_class(ActiveTab::Export)
                        on:click=move |_| active_tab.set(ActiveTab::Export)
                    >
                        {icon("upload-cloud")}
                        "Выгрузка"
                    </button>
                    <button
                        class=move || tab_class(ActiveTab::Restore)
                        on:click=move |_| active_tab.set(ActiveTab::Restore)
                    >
                        {icon("download-cloud")}
                        "Восстановление"
                    </button>
                    <button
                        class=move || tab_class(ActiveTab::Diagnostics)
                        on:click=move |_| active_tab.set(ActiveTab::Diagnostics)
                    >
                        {icon("activity")}
                        "Диагностика"
                        {move || (anomaly_count.get() > 0).then(|| view! {
                            <span class="badge badge--warning">{anomaly_count.get().to_string()}</span>
                        })}
                    </button>
                    <button
                        class=move || tab_class(ActiveTab::Journal)
                        on:click=move |_| active_tab.set(ActiveTab::Journal)
                    >
                        {icon("list")}
                        "Журнал операций"
                    </button>
                </div>

                <Show when=move || active_tab.get() == ActiveTab::Export>
                    <ExportTab state=state />
                </Show>
                <Show when=move || active_tab.get() == ActiveTab::Restore>
                    <CatalogTab state=state catalog_error=catalog_error />
                </Show>
                <Show when=move || active_tab.get() == ActiveTab::Diagnostics>
                    <DiagnosticsTab status=status />
                </Show>
                <Show when=move || active_tab.get() == ActiveTab::Journal>
                    <JournalTab history=history />
                </Show>
            </div>
        </PageFrame>
    }
}
