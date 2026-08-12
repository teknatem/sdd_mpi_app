//! Индикация длительных операций переноса.
//!
//! Прогресс двухуровневый намеренно. Одних процентов мало: выгрузка базы идёт
//! через `VACUUM INTO`, проверку целостности и загрузку по частям, и «47%» без
//! указания стадии не отвечает на вопрос «оно вообще движется или встало».
//! Одних стадий тоже мало: внутри «Выгрузка в S3» можно провести минуты.

use contracts::system::datasets::{DatasetJobDto, DatasetJobStatus};
use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::shared::date_utils::format_bytes_compact;
use crate::system::datasets::api;

#[component]
pub fn JobProgress(job: DatasetJobDto) -> impl IntoView {
    let running = job.status == DatasetJobStatus::Running;
    let job_id = job.job_id.clone();
    let cancelling = RwSignal::new(false);

    // Полоса детерминирована только когда известен объём: до начала передачи
    // (VACUUM INTO, запись каталога) он неизвестен, и рисовать «0%» было бы
    // враньём — там показывается бегущая полоса.
    let percent = (job.bytes_total > 0)
        .then(|| (job.bytes_done as f64 / job.bytes_total as f64 * 100.0).clamp(0.0, 100.0));

    let bytes_label = if job.bytes_total > 0 {
        format!(
            "{} из {}",
            format_bytes_compact(job.bytes_done),
            format_bytes_compact(job.bytes_total)
        )
    } else if job.bytes_done > 0 {
        format_bytes_compact(job.bytes_done)
    } else {
        String::new()
    };

    let cancel = move |_| {
        let job_id = job_id.clone();
        cancelling.set(true);
        spawn_local(async move {
            let _ = api::cancel_job(&job_id).await;
        });
    };

    let stages = job.stages.clone();
    let stage_index = job.stage_index;

    view! {
        <div class="datasets__job">
            <div class="datasets__job-head">
                <span class="page-action-button__spinner"></span>
                <span class="datasets__job-stage">
                    {format!(
                        "{} · шаг {} из {}: {}",
                        job.kind.label_ru(),
                        stage_index + 1,
                        stages.len(),
                        job.stage_label,
                    )}
                </span>
                <span class="text-muted datasets__job-bytes">{bytes_label}</span>
                // Не `<Show>`: компонент пересоздаётся на каждый опрос, `running`
                // здесь обычный bool, а не сигнал — реактивная обёртка потребовала
                // бы `Fn`-замыкание там, где обработчик отмены отдаётся один раз.
                {running.then(|| view! {
                    <button
                        class="button button--secondary button--small"
                        disabled=move || cancelling.get()
                        on:click=cancel
                    >
                        {move || if cancelling.get() { "Прерывание..." } else { "Прервать" }}
                    </button>
                })}
            </div>

            <div class="datasets__job-bar" class:datasets__job-bar--pulsing=percent.is_none()>
                <div
                    class="datasets__job-bar-fill"
                    style=match percent {
                        Some(value) => format!("width: {value:.1}%;"),
                        None => "width: 100%;".to_string(),
                    }
                ></div>
            </div>

            <ol class="datasets__job-stages">
                {stages.into_iter().enumerate().map(|(index, label)| {
                    let class = if index < stage_index {
                        "datasets__job-step datasets__job-step--done"
                    } else if index == stage_index {
                        "datasets__job-step datasets__job-step--active"
                    } else {
                        "datasets__job-step"
                    };
                    view! { <li class=class>{label}</li> }
                }).collect_view()}
            </ol>
        </div>
    }
}
