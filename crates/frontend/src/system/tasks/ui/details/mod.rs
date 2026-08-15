use crate::layout::global_context::AppGlobalContext;
use crate::shared::components::card_animated::CardAnimated;
use crate::shared::components::page_tabs::{PageTabs, TabItem};
use crate::shared::date_utils::{format_duration_ms, format_http_traffic, format_utc_local};
use crate::shared::icons::icon;
use crate::shared::page_frame::PageFrame;
use crate::system::tasks::api;
use crate::system::tasks::ui::config_editor::{CronEditor, TaskConfigEditor};
use crate::system::tasks::ui::TaskProgressPanel;
use contracts::system::tasks::metadata::TaskMetadataDto;
use contracts::system::tasks::progress::TaskProgressResponse;
use contracts::system::tasks::request::{CreateScheduledTaskDto, UpdateScheduledTaskDto};
use contracts::system::tasks::runs::TaskRun;
use leptos::ev;
use leptos::logging::log;
use leptos::prelude::*;
use leptos::task::spawn_local;
use thaw::*;

fn run_history_http_calls(run: &TaskRun) -> String {
    run.http_request_count
        .filter(|&n| n > 0)
        .map(|n| n.to_string())
        .unwrap_or_else(|| "—".to_string())
}

fn run_history_http_traffic(run: &TaskRun) -> String {
    format_http_traffic(
        run.http_bytes_sent.unwrap_or(0),
        run.http_bytes_received.unwrap_or(0),
    )
    .unwrap_or_else(|| "—".to_string())
}

fn progress_panel_title(status: &str) -> &'static str {
    match status {
        "Completed" | "CompletedWithErrors" | "Failed" | "Cancelled" => "Результат запуска",
        _ => "Живой прогресс",
    }
}

fn progress_running_title(status: &str) -> &'static str {
    match status {
        "Completed" => "Завершено",
        "CompletedWithErrors" => "Завершено с ошибками",
        "Failed" => "Завершено с ошибкой",
        "Cancelled" => "Отменено",
        _ => "Выполняется…",
    }
}

#[component]
fn StatusBadge(status: String) -> impl IntoView {
    let (bg, fg, label) = match status.as_str() {
        "Completed" => (
            "var(--colorSuccessBackground2)",
            "var(--colorSuccessForeground1)",
            "Успешно",
        ),
        "CompletedWithErrors" => (
            "var(--colorPaletteYellowBackground2)",
            "var(--colorPaletteDarkOrangeForeground2)",
            "С ошибками",
        ),
        "Running" => (
            "var(--colorBrandBackground2)",
            "var(--colorBrandForeground1)",
            "В работе",
        ),
        "Failed" => (
            "var(--colorPaletteRedBackground2)",
            "var(--color-error)",
            "Ошибка",
        ),
        _ => (
            "var(--colorNeutralBackground3)",
            "var(--color-text-secondary)",
            "—",
        ),
    };
    view! {
        <span style=format!("display:inline-flex;align-items:center;padding:2px 8px;border-radius:999px;background:{bg};color:{fg};font-size:12px;font-weight:600;")>
            {label}
        </span>
    }
}

#[component]
fn TriggeredBadge(triggered_by: String) -> impl IntoView {
    let (bg, fg, label) = match triggered_by.as_str() {
        "Manual" => (
            "var(--colorPaletteYellowBackground2)",
            "var(--colorPaletteDarkOrangeForeground2)",
            "Вручную",
        ),
        _ => (
            "var(--colorNeutralBackground3)",
            "var(--color-text-secondary)",
            "Расписание",
        ),
    };
    view! {
        <span style=format!("display:inline-flex;align-items:center;padding:2px 8px;border-radius:999px;background:{bg};color:{fg};font-size:12px;font-weight:600;")>
            {label}
        </span>
    }
}

#[component]
fn RunHistoryRow(run: TaskRun) -> impl IntoView {
    let started = format_utc_local(&run.started_at, "%d.%m.%y %H:%M:%S");
    let duration = run
        .duration_ms
        .map(format_duration_ms)
        .unwrap_or_else(|| "—".to_string());
    let processed = run
        .total_processed
        .map(|v| v.to_string())
        .unwrap_or_else(|| "—".to_string());
    let inserted = run
        .total_inserted
        .map(|v| v.to_string())
        .unwrap_or_else(|| "—".to_string());
    let updated = run
        .total_updated
        .map(|v| v.to_string())
        .unwrap_or_else(|| "—".to_string());
    let errors = run
        .total_errors
        .map(|v| v.to_string())
        .unwrap_or_else(|| "—".to_string());
    let status = run.status.clone();
    let triggered = run.triggered_by.clone();
    let err_msg = run.error_message.clone().unwrap_or_default();
    let http_calls = run_history_http_calls(&run);
    let http_traffic = run_history_http_traffic(&run);

    view! {
        <TableRow>
            <TableCell><span style="font-size:12px;color:var(--color-text-secondary);">{started}</span></TableCell>
            <TableCell><TriggeredBadge triggered_by=triggered /></TableCell>
            <TableCell><StatusBadge status=status /></TableCell>
            <TableCell><span style="font-size:12px;">{duration}</span></TableCell>
            <TableCell><span style="font-size:12px;">{processed}</span></TableCell>
            <TableCell><span style="font-size:12px;color:var(--colorSuccessForeground1);">{inserted}</span></TableCell>
            <TableCell><span style="font-size:12px;color:var(--colorBrandForeground1);">{updated}</span></TableCell>
            <TableCell><span style="font-size:12px;color:var(--color-error);">{errors}</span></TableCell>
            <TableCell>
                <span style="font-size:12px;font-family:monospace;color:var(--color-text-secondary);">{http_calls}</span>
            </TableCell>
            <TableCell>
                <span
                    style="font-size:11px;font-family:monospace;line-height:1.35;color:var(--color-text-secondary);white-space:nowrap;"
                    title="Итоговый трафик HTTP за запуск: тела запросов и ответов"
                >
                    {http_traffic}
                </span>
            </TableCell>
            <TableCell>
                {if !err_msg.is_empty() {
                    let title = err_msg.clone();
                    view! {
                        <span style="font-size:11px;color:var(--color-error);max-width:200px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;display:block;" title=title>
                            {err_msg}
                        </span>
                    }.into_any()
                } else {
                    view! { <></> }.into_any()
                }}
            </TableCell>
        </TableRow>
    }
}

#[component]
pub fn ScheduledTaskDetails(id: String) -> impl IntoView {
    let is_new = id == "new";

    let tabs_store =
        leptos::context::use_context::<AppGlobalContext>().expect("AppGlobalContext not found");

    let tab_key = if is_new {
        "sys_task_details".to_string()
    } else {
        format!("sys_task_details_{}", id)
    };

    let close_tab = {
        let tab_key = tab_key.clone();
        move |_| tabs_store.close_tab(&tab_key)
    };

    let (saving, set_saving) = signal(false);
    let (saved, set_saved) = signal(false);
    let (refreshing, set_refreshing) = signal(false);
    let (error, set_error) = signal::<Option<String>>(None);
    let (task_loaded, set_task_loaded) = signal(false);
    let (is_running, set_is_running) = signal(false);
    let (watermark_saving, set_watermark_saving) = signal(false);
    let (watermark_saved, set_watermark_saved) = signal(false);
    let last_successful_run_at = RwSignal::new(None::<chrono::DateTime<chrono::Utc>>);
    let data_loaded_up_to = RwSignal::new(None::<chrono::NaiveDate>);
    let watermark_date_input = RwSignal::new(String::new());

    let code = RwSignal::new(String::new());
    let description = RwSignal::new(String::new());
    let task_type = RwSignal::new(String::new());
    let schedule_cron = RwSignal::new(String::new());
    let is_enabled = RwSignal::new(true);
    let config_json = RwSignal::new(String::new());
    let comment = RwSignal::new(String::new());

    let session_id = RwSignal::new(None::<String>);
    let run_progress = RwSignal::new(None::<TaskProgressResponse>);
    let log_content = RwSignal::new(String::new());

    let (runs, set_runs) = signal(Vec::<TaskRun>::new());
    let (runs_loading, set_runs_loading) = signal(false);
    let (metadata, set_metadata) = signal(None::<TaskMetadataDto>);
    let (active_tab, set_active_tab) = signal::<&'static str>("settings");
    let (task_types, set_task_types) = signal(Vec::<TaskMetadataDto>::new());
    let task_types_error = RwSignal::new(None::<String>);

    // ---- Load available task types once ----
    Effect::new(move |_| {
        spawn_local(async move {
            match api::get_task_types().await {
                Ok(types) => {
                    set_task_types.set(types);
                    task_types_error.set(None);
                }
                Err(error) => {
                    task_types_error.set(Some(error));
                }
            }
        });
    });

    // ---- loaders ----
    let task_id_str = id.clone();
    Effect::new(move |_| {
        if is_new {
            return;
        }
        let tid = task_id_str.clone();
        spawn_local(async move {
            match api::get_scheduled_task(&tid).await {
                Ok(t) => {
                    code.set(t.code.clone());
                    description.set(t.description.clone());
                    task_type.set(t.task_type.clone());
                    schedule_cron.set(t.schedule_cron.clone().unwrap_or_default());
                    is_enabled.set(t.is_enabled);
                    config_json.set(t.config_json.clone());
                    comment.set(t.comment.clone().unwrap_or_default());
                    last_successful_run_at.set(t.last_successful_run_at);
                    data_loaded_up_to.set(t.data_loaded_up_to);
                    // Pre-fill watermark input with the explicit data watermark.
                    let date_str = t
                        .data_loaded_up_to
                        .map(|date| date.to_string())
                        .unwrap_or_default();
                    watermark_date_input.set(date_str);
                    set_task_loaded.set(true);
                }
                Err(e) => set_error.set(Some(e)),
            }
        });
    });

    // Resolve metadata from the already loaded type registry. Reading both
    // signals makes the effect rerun regardless of which request finishes first.
    Effect::new(move |_| {
        let tt = task_type.get();
        let resolved = task_types
            .get()
            .into_iter()
            .find(|metadata| metadata.task_type == tt);
        set_metadata.set(resolved);
    });

    // Load runs when history tab is opened
    let runs_task_id = id.clone();
    Effect::new(move |_| {
        if active_tab.get() != "history" {
            return;
        }
        if is_new {
            return;
        }
        let tid = runs_task_id.clone();
        spawn_local(async move {
            set_runs_loading.set(true);
            match api::get_task_runs(&tid, Some(50)).await {
                Ok(resp) => set_runs.set(resp.runs),
                Err(e) => log!("Failed to load runs: {}", e),
            }
            set_runs_loading.set(false);
        });
    });

    // Load the latest run log when the Logs tab is opened. Polling fills this
    // only for the currently watched session, so existing completed runs need
    // an explicit load.
    let logs_task_id = id.clone();
    Effect::new(move |_| {
        if active_tab.get() != "logs" {
            return;
        }
        if is_new {
            return;
        }

        let tid = logs_task_id.clone();
        let sid = session_id.get();
        spawn_local(async move {
            let sid = match sid {
                Some(sid) => sid,
                None => match api::get_task_runs(&tid, Some(1)).await {
                    Ok(resp) => match resp.runs.first() {
                        Some(run) => run.session_id.clone(),
                        None => {
                            log_content.set("История запусков пуста.".to_string());
                            return;
                        }
                    },
                    Err(e) => {
                        log_content.set(format!("Не удалось загрузить историю запусков: {}", e));
                        return;
                    }
                },
            };

            match api::get_task_log(&tid, &sid).await {
                Ok(log) => log_content.set(log),
                Err(e) => log_content.set(format!("Не удалось загрузить лог: {}", e)),
            }
        });
    });

    // Poll progress when session_id is set
    let poll_task_id = id.clone();
    Effect::new(move |_| {
        let Some(sid) = session_id.get() else {
            return;
        };
        let tid = poll_task_id.clone();
        let sid2 = sid.clone();
        // Refresh runs after completion inline
        let runs_task_id2 = poll_task_id.clone();
        spawn_local(async move {
            loop {
                match api::get_task_progress(&tid, &sid2).await {
                    Ok(p) => {
                        let status = p.status.clone();
                        if let Some(log) = p.log_content.clone() {
                            if !log.is_empty() {
                                log_content.set(log);
                            }
                        }
                        run_progress.set(Some(p));
                        if status == "Completed"
                            || status == "CompletedWithErrors"
                            || status == "Failed"
                        {
                            set_is_running.set(false);
                            if let Ok(t) = api::get_scheduled_task(&tid).await {
                                last_successful_run_at.set(t.last_successful_run_at);
                                data_loaded_up_to.set(t.data_loaded_up_to);
                                watermark_date_input.set(
                                    t.data_loaded_up_to
                                        .map(|date| date.to_string())
                                        .unwrap_or_default(),
                                );
                            }
                            // Reload run history if on that tab
                            let t2 = runs_task_id2.clone();
                            spawn_local(async move {
                                if let Ok(resp) = api::get_task_runs(&t2, Some(50)).await {
                                    set_runs.set(resp.runs);
                                }
                            });
                            break;
                        }
                    }
                    Err(e) => {
                        log!("Progress poll error: {}", e);
                        set_is_running.set(false);
                        break;
                    }
                }
                gloo_timers::future::TimeoutFuture::new(2000).await;
            }
        });
    });

    // ---- refresh ----
    let refresh_id = id.clone();
    let refresh_sv = StoredValue::new(move || {
        if is_new {
            return;
        }
        set_refreshing.set(true);
        set_error.set(None);
        let tid = refresh_id.clone();
        spawn_local(async move {
            match api::get_scheduled_task(&tid).await {
                Ok(t) => {
                    code.set(t.code.clone());
                    description.set(t.description.clone());
                    task_type.set(t.task_type.clone());
                    schedule_cron.set(t.schedule_cron.clone().unwrap_or_default());
                    is_enabled.set(t.is_enabled);
                    config_json.set(t.config_json.clone());
                    comment.set(t.comment.clone().unwrap_or_default());
                    last_successful_run_at.set(t.last_successful_run_at);
                    data_loaded_up_to.set(t.data_loaded_up_to);
                    let date_str = t
                        .data_loaded_up_to
                        .map(|date| date.to_string())
                        .unwrap_or_default();
                    watermark_date_input.set(date_str);
                }
                Err(e) => set_error.set(Some(e)),
            }
            set_refreshing.set(false);
        });
    });

    // ---- save ----
    let save_id = id.clone();
    let save_task = move |_: ev::MouseEvent| {
        set_saving.set(true);
        set_saved.set(false);
        set_error.set(None);
        let tid = save_id.clone();
        spawn_local(async move {
            let res = if is_new {
                api::create_scheduled_task(CreateScheduledTaskDto {
                    code: code.get_untracked(),
                    description: description.get_untracked(),
                    comment: Some(comment.get_untracked()).filter(|s| !s.is_empty()),
                    task_type: task_type.get_untracked(),
                    schedule_cron: Some(schedule_cron.get_untracked()).filter(|s| !s.is_empty()),
                    is_enabled: is_enabled.get_untracked(),
                    config_json: config_json.get_untracked(),
                })
                .await
            } else {
                api::update_scheduled_task(
                    &tid,
                    UpdateScheduledTaskDto {
                        code: code.get_untracked(),
                        description: description.get_untracked(),
                        comment: Some(comment.get_untracked()).filter(|s| !s.is_empty()),
                        task_type: task_type.get_untracked(),
                        schedule_cron: Some(schedule_cron.get_untracked())
                            .filter(|s| !s.is_empty()),
                        is_enabled: is_enabled.get_untracked(),
                        config_json: config_json.get_untracked(),
                    },
                )
                .await
            };
            match res {
                Ok(t) => {
                    if is_new {
                        // For new tasks: populate signals from server response
                        // (server assigns the real ID which will be used for future saves)
                        code.set(t.code.clone());
                        description.set(t.description.clone());
                        task_type.set(t.task_type.clone());
                        schedule_cron.set(t.schedule_cron.clone().unwrap_or_default());
                        is_enabled.set(t.is_enabled);
                        config_json.set(t.config_json.clone());
                        comment.set(t.comment.clone().unwrap_or_default());
                        set_task_loaded.set(true);
                    }
                    // For UPDATE: signals already hold the user's correct values.
                    // Do NOT reset them from the server response —
                    // that would re-create TaskConfigEditor and reset the UI.
                    set_saved.set(true);
                    spawn_local(async move {
                        gloo_timers::future::TimeoutFuture::new(2000).await;
                        set_saved.set(false);
                    });
                }
                Err(e) => set_error.set(Some(e)),
            }
            set_saving.set(false);
        });
    };

    // ---- duplicate ----
    let duplicate_task = move |_: ev::MouseEvent| {
        if is_new {
            return;
        }
        set_error.set(None);
        let new_code = format!("{}_sts", code.get_untracked());
        spawn_local(async move {
            match api::create_scheduled_task(CreateScheduledTaskDto {
                code: new_code.clone(),
                description: description.get_untracked(),
                comment: Some(comment.get_untracked()).filter(|s| !s.is_empty()),
                task_type: task_type.get_untracked(),
                schedule_cron: Some(schedule_cron.get_untracked()).filter(|s| !s.is_empty()),
                is_enabled: false,
                config_json: config_json.get_untracked(),
            })
            .await
            {
                Ok(created) => {
                    tabs_store.open_tab(
                        &format!("sys_task_details_{}", created.id),
                        &format!("Задача: {}", created.code),
                    );
                }
                Err(e) => set_error.set(Some(format!("Ошибка дублирования: {}", e))),
            }
        });
    };

    // ---- run now (stored value so it can be called from multiple closures) ----
    let run_id = id.clone();
    let run_now_sv = StoredValue::new(move || {
        if is_new {
            return;
        }
        set_is_running.set(true);
        set_error.set(None);
        run_progress.set(None);
        log_content.set(String::new());
        let tid = run_id.clone();
        spawn_local(async move {
            match api::run_task_now(&tid).await {
                Ok(api::RunTaskNowOutcome::Started(resp)) => {
                    session_id.set(Some(resp.session_id));
                }
                Ok(api::RunTaskNowOutcome::AlreadyRunning(r)) => {
                    set_error.set(Some(format!(
                        "Задание уже выполняется. Запущено: {}",
                        format_utc_local(&r.started_at, "%d.%m.%Y %H:%M:%S")
                    )));
                    set_is_running.set(false);
                }
                Err(e) => {
                    set_error.set(Some(format!("Ошибка запуска: {}", e)));
                    set_is_running.set(false);
                }
            }
        });
    });

    view! {
        <PageFrame page_id="sys_tasks--detail" category="system" class="page--wide scheduled-task-details">

            // ---- Header ----
            <div class="page__header">
                <div class="page__header-left">
                    <h1 class="page__title">
                        {move || if is_new { "Новая задача".to_string() } else { format!("Задача: {}", code.get()) }}
                    </h1>
                </div>
                <div class="page__header-right">
                    {move || if !is_new {
                        view! {
                            <Button
                                appearance=ButtonAppearance::Primary
                                on_click=move |_| run_now_sv.with_value(|f| f())
                                disabled=move || is_running.get()
                            >
                                {icon("play")}
                                {move || if is_running.get() { " В работе..." } else { " Запустить сейчас" }}
                            </Button>
                            <Button
                                appearance=ButtonAppearance::Secondary
                                on_click=duplicate_task
                            >
                                {icon("copy")}
                                " Дублировать"
                            </Button>
                            <Button
                                appearance=ButtonAppearance::Secondary
                                on_click=move |_| refresh_sv.with_value(|f| f())
                                disabled=Signal::derive(move || refreshing.get())
                            >
                                {icon("refresh")}
                                {move || if refreshing.get() { " Обновление..." } else { " Обновить" }}
                            </Button>
                        }.into_any()
                    } else { view! { <></> }.into_any() }}
                    <Button
                        appearance=ButtonAppearance::Secondary
                        on_click=save_task
                        disabled=Signal::derive(move || saving.get())
                    >
                        {move || match (saving.get(), saved.get()) {
                            (true, _) => view! { <>{icon("save")} " Сохранение..."</> }.into_any(),
                            (_, true) => view! { <>"✓ Сохранено"</> }.into_any(),
                            _         => view! { <>{icon("save")} " Сохранить"</> }.into_any(),
                        }}
                    </Button>
                    <Button
                        appearance=ButtonAppearance::Secondary
                        on_click=close_tab
                    >
                        "Закрыть"
                    </Button>
                </div>
            </div>

            // ---- Error ----
            {move || error.get().map(|err| view! {
                <div class="warning-box warning-box--error scheduled-task-details__error">
                    <span class="warning-box__icon">"⚠"</span>
                    <span class="warning-box__text">{err}</span>
                </div>
            })}

            // ---- Live progress (when running) ----
            {move || {
                let running = is_running.get();
                let prog = run_progress.get();
                if !running && prog.is_none() { return view! { <></> }.into_any(); }

                let p = prog.unwrap_or_default();
                let section_title = progress_panel_title(&p.status).to_string();
                let running_title = progress_running_title(&p.status).to_string();

                view! {
                    <div style="padding:12px 16px;border-radius:var(--radius-md);background:var(--colorBrandBackground2);border:1px solid var(--colorBrandStroke2);margin-bottom:12px;">
                        <TaskProgressPanel
                            progress=p
                            section_title=section_title
                            running_title=running_title
                        />
                    </div>
                }.into_any()
            }}

            // ---- Tab bar ----
            <PageTabs
                tabs=vec![
                    TabItem::new("settings", "Настройки"),
                    TabItem::new("logs", "Логи"),
                    TabItem::new("history", "История запусков"),
                    TabItem::new("metadata", "Описание задачи"),
                ]
                active=active_tab.into()
                on_select=Callback::new(move |key: &'static str| set_active_tab.set(key))
            />

            // ---- Settings tab ----
            // Always mounted (display:none when inactive) so that Thaw <Select> is never
            // destroyed/recreated — recreation causes task_type signal to reset to "" because
            // the browser <select> cannot find the matching <option> before reactive options
            // are painted (timing race), Thaw then writes "" back into the signal.
            <div class="page__content" style=move || if active_tab.get() != "settings" { "display:none;" } else { "" }>
                // 2-column grid: left = Тип обработчика + Идентификация + Конфигурация,
                // right = Watermark + Параметры
                <div class="detail-grid">

                    // ── Left column: Тип обработчика + Идентификация + Конфигурация ──
                    <div class="detail-grid__col">

                        <CardAnimated delay_ms=0 nav_id="sys_task_details_handler_type">
                            <h4 class="details-section__title">"Тип обработчика"</h4>
                            <div class="form__group">
                                <select
                                    class="form__select"
                                    prop:value=move || {
                                        // Reapply the value after the async option list changes.
                                        let _ = task_types.get();
                                        task_type.get()
                                    }
                                    on:change=move |event| {
                                        task_type.set(event_target_value(&event));
                                    }
                                >
                                    <option
                                        value=""
                                        selected=move || task_type.get().is_empty()
                                    >
                                        "— Выберите тип —"
                                    </option>
                                    {move || {
                                        let current = task_type.get();
                                        let types = task_types.get();
                                        let current_is_missing = !current.is_empty()
                                            && !types.iter().any(|t| t.task_type == current);
                                        let missing_option = current_is_missing.then(|| {
                                            let value = current.clone();
                                            let label = format!("{current} (не зарегистрирован)");
                                            view! { <option value=value selected=true>{label}</option> }
                                        });
                                        let registered_options = types.into_iter().map(|t| {
                                            let selected = t.task_type == current;
                                            let value = t.task_type.clone();
                                            let label = format!("{}: {}", t.task_type, t.display_name);
                                            view! { <option value=value selected=selected>{label}</option> }
                                        }).collect_view();

                                        view! {
                                            {missing_option}
                                            {registered_options}
                                        }
                                    }}
                                </select>
                                {move || task_types_error.get().map(|error| view! {
                                    <div style="font-size:var(--font-size-sm);color:var(--color-error);margin-top:4px;">
                                        {format!("Не удалось загрузить список типов: {error}")}
                                    </div>
                                })}
                            </div>
                        </CardAnimated>

                        <CardAnimated delay_ms=40 nav_id="sys_task_details_identity">
                            <h4 class="details-section__title">"Идентификация"</h4>
                            <div style="display:flex;flex-direction:column;gap:var(--spacing-sm);">
                                <div class="form__group">
                                    <label class="form__label">"Код"</label>
                                    <Input value=code placeholder="u501_import_ut" />
                                </div>
                                <div class="form__group">
                                    <label class="form__label">"Описание"</label>
                                    <Input value=description placeholder="Импорт из WB" />
                                </div>
                                <div class="form__group">
                                    <label class="form__label">"Комментарий"</label>
                                    <Input value=comment placeholder="..." />
                                </div>
                            </div>
                        </CardAnimated>

                        <CardAnimated delay_ms=80 nav_id="sys_task_details_config">
                            <h4 class="details-section__title">"Конфигурация"</h4>
                            {move || {
                                let schema = metadata.get()
                                    .map(|m| m.config_fields)
                                    .unwrap_or_default();
                                let editor_ready = is_new || task_loaded.get();

                                if !schema.is_empty() && editor_ready {
                                    view! {
                                        <TaskConfigEditor
                                            config_json=config_json
                                            schema=schema
                                        />
                                    }.into_any()
                                } else {
                                    view! {
                                        <Textarea
                                            value=config_json
                                            placeholder="{ ... }"
                                            class="monospace-textarea"
                                            attr:rows=10
                                        />
                                        <div style="font-size:11px;color:var(--color-text-tertiary);margin-top:4px;">
                                            "JSON-конфигурация задачи. Структура зависит от типа обработчика."
                                        </div>
                                    }.into_any()
                                }
                            }}
                        </CardAnimated>

                    </div>

                    // ── Right column: Watermark + Параметры ───────────────────────────
                    <div class="detail-grid__col">

                        // Watermark (только для существующих задач)
                        {move || {
                            if is_new { return view! { <></> }.into_any(); }
                            let wm_task_id = id.clone();
                            let is_windowed_task = metadata
                                .get()
                                .map(|m| m.config_fields.iter().any(|f| f.key == "work_start_date"))
                                .unwrap_or(false);
                            let card_title = if is_windowed_task {
                                "Watermark загрузки"
                            } else {
                                "Последнее успешное обновление"
                            };
                            view! {
                                <CardAnimated delay_ms=80 nav_id="sys_task_details_watermark">
                                    <h4 class="details-section__title">{card_title}</h4>
                                    <div style="display:flex;flex-direction:column;gap:var(--spacing-sm);">
                                        {if is_windowed_task {
                                            view! {
                                                <div class="form__group">
                                                    <label class="form__label">"Данные загружены по"</label>
                                                    <span style="font-size:var(--font-size-base);color:var(--color-text);">
                                                        {move || data_loaded_up_to.get()
                                                            .map(|date| date.format("%d.%m.%Y").to_string())
                                                            .unwrap_or_else(|| "не установлен".to_string())}
                                                    </span>
                                                </div>
                                                <div class="form__group">
                                                    <label class="form__label">"Последний успешный запуск"</label>
                                                    <span style="font-size:var(--font-size-base);color:var(--color-text);">
                                                        {move || last_successful_run_at.get()
                                                            .map(|dt| format_utc_local(&dt, "%d.%m.%Y %H:%M"))
                                                            .unwrap_or_else(|| "не установлен".to_string())}
                                                    </span>
                                                </div>
                                            }.into_any()
                                        } else {
                                            view! {
                                                <div class="form__group">
                                                    <label class="form__label">"Последний успешный запуск"</label>
                                                    <span style="font-size:var(--font-size-base);color:var(--color-text);">
                                                        {move || last_successful_run_at.get()
                                                            .map(|dt| format_utc_local(&dt, "%d.%m.%Y %H:%M"))
                                                            .unwrap_or_else(|| "не установлен".to_string())}
                                                    </span>
                                                </div>
                                            }.into_any()
                                        }}
                                        {if is_windowed_task {
                                            view! {
                                                <div class="form__group">
                                                    <label class="form__label">"Установить watermark вручную"</label>
                                                    <div style="display:flex;align-items:center;gap:8px;flex-wrap:wrap;margin-top:2px;">
                                                        <input
                                                            type="date"
                                                            prop:value=move || watermark_date_input.get()
                                                            on:input=move |ev| { watermark_date_input.set(event_target_value(&ev)); }
                                                            style="padding:5px 8px;border:1px solid var(--color-border);border-radius:var(--radius-sm);background:var(--colorNeutralBackground1);color:var(--color-text);font-size:var(--font-size-base);"
                                                        />
                                                        <Button
                                                            appearance=ButtonAppearance::Primary
                                                            disabled=Signal::derive(move || watermark_saving.get())
                                                            on_click={
                                                                let wm_id = wm_task_id.clone();
                                                                move |_| {
                                                                    let date_val = watermark_date_input.get_untracked();
                                                                    if date_val.is_empty() { return; }
                                                                    set_watermark_saving.set(true);
                                                                    set_watermark_saved.set(false);
                                                                    let tid = wm_id.clone();
                                                                    let dv = date_val.clone();
                                                                    spawn_local(async move {
                                                                        match api::set_watermark(&tid, Some(dv)).await {
                                                                            Ok(_) => {
                                                                                if let Ok(t) = api::get_scheduled_task(&tid).await {
                                                                                    last_successful_run_at.set(t.last_successful_run_at);
                                                                                    data_loaded_up_to.set(t.data_loaded_up_to);
                                                                                    watermark_date_input.set(
                                                                                        t.data_loaded_up_to
                                                                                            .map(|date| date.to_string())
                                                                                            .unwrap_or_default(),
                                                                                    );
                                                                                }
                                                                                set_watermark_saved.set(true);
                                                                            }
                                                                            Err(e) => set_error.set(Some(format!("Ошибка: {e}"))),
                                                                        }
                                                                        set_watermark_saving.set(false);
                                                                    });
                                                                }
                                                            }
                                                        >
                                                            "Установить"
                                                        </Button>
                                                        <Button
                                                            appearance=ButtonAppearance::Secondary
                                                            disabled=Signal::derive(move || watermark_saving.get())
                                                            on_click={
                                                                let wm_id = wm_task_id.clone();
                                                                move |_| {
                                                                    set_watermark_saving.set(true);
                                                                    set_watermark_saved.set(false);
                                                                    let tid = wm_id.clone();
                                                                    spawn_local(async move {
                                                                        match api::set_watermark(&tid, None).await {
                                                                            Ok(_) => {
                                                                                last_successful_run_at.set(None);
                                                                                data_loaded_up_to.set(None);
                                                                                watermark_date_input.set(String::new());
                                                                                set_watermark_saved.set(true);
                                                                            }
                                                                            Err(e) => set_error.set(Some(format!("Ошибка: {e}"))),
                                                                        }
                                                                        set_watermark_saving.set(false);
                                                                    });
                                                                }
                                                            }
                                                        >
                                                            "Сброс (NULL)"
                                                        </Button>
                                                        {move || watermark_saved.get().then(|| view! {
                                                            <span style="color:var(--colorSuccessForeground1);font-size:var(--font-size-base);font-weight:600;">"✓ Сохранено"</span>
                                                        })}
                                                    </div>
                                                    <div style="font-size:var(--font-size-sm);color:var(--color-text-tertiary);margin-top:4px;">
                                                        "NULL — загрузка начнётся с work_start_date, по chunk_days дней за раз."
                                                    </div>
                                                </div>
                                            }.into_any()
                                        } else {
                                            view! {
                                                <div style="font-size:var(--font-size-sm);color:var(--color-text-tertiary);">
                                                    "Справочная задача: хронология по дням не используется, важна только дата последнего успешного обновления."
                                                </div>
                                            }.into_any()
                                        }}
                                    </div>
                                </CardAnimated>
                            }.into_any()
                        }}

                        // Параметры
                        <CardAnimated delay_ms=160 nav_id="sys_task_details_params">
                            <h4 class="details-section__title">"Параметры"</h4>
                            <div style="display:flex;flex-direction:column;gap:var(--spacing-sm);">
                                <div class="form__group">
                                    <label class="form__label">"Расписание (Cron)"</label>
                                    <CronEditor value=schedule_cron />
                                </div>
                                <Checkbox checked=is_enabled label="Автоматически запускать задачу" />
                            </div>
                        </CardAnimated>

                    </div>
                </div>
            </div>

            // ---- Logs tab ----
            <div class="page__content" style=move || if active_tab.get() != "logs" { "display:none;" } else { "" }>
                <div class="card">
                    <div class="card__header">
                        <h3 class="scheduled-task-details__card-title">"Лог последнего запуска"</h3>
                    </div>
                    <div class="card__body">
                        <div class="code-box scheduled-task-details__log-box"
                             style="min-height:200px;max-height:500px;overflow-y:auto;white-space:pre-wrap;font-size:12px;font-family:monospace;">
                            {move || {
                                let c = log_content.get();
                                if c.is_empty() { "Нет данных лога. Запустите задачу чтобы увидеть лог.".to_string() } else { c }
                            }}
                        </div>
                    </div>
                </div>
            </div>

            // ---- History tab ----
            <div class="page__content" style=move || if active_tab.get() != "history" { "display:none;" } else { "" }>
                <div class="card">
                    <div class="card__header">
                        <Flex justify=FlexJustify::SpaceBetween align=FlexAlign::Center>
                            <h3 class="scheduled-task-details__card-title">"История запусков"</h3>
                        </Flex>
                    </div>
                    <div class="card__body">
                        {move || if runs_loading.get() {
                            view! {
                                <Flex justify=FlexJustify::Center align=FlexAlign::Center style="padding:32px;">
                                    <Spinner />" Загрузка..."
                                </Flex>
                            }.into_any()
                        } else {
                            let run_list = runs.get();
                            if run_list.is_empty() {
                                view! {
                                    <div style="padding:32px;text-align:center;color:var(--color-text-secondary);">
                                        "История запусков пуста"
                                    </div>
                                }.into_any()
                            } else {
                                view! {
                                    <Table>
                                        <TableHeader>
                                            <TableRow>
                                                <TableHeaderCell attr:style="width:140px;">"Время"</TableHeaderCell>
                                                <TableHeaderCell attr:style="width:110px;">"Источник"</TableHeaderCell>
                                                <TableHeaderCell attr:style="width:90px;">"Статус"</TableHeaderCell>
                                                <TableHeaderCell attr:style="width:80px;">"Длит."</TableHeaderCell>
                                                <TableHeaderCell attr:style="width:70px;">"Обраб."</TableHeaderCell>
                                                <TableHeaderCell attr:style="width:55px;">"Ins"</TableHeaderCell>
                                                <TableHeaderCell attr:style="width:55px;">"Upd"</TableHeaderCell>
                                                <TableHeaderCell attr:style="width:55px;">"Err"</TableHeaderCell>
                                                <TableHeaderCell attr:style="width:56px;">"HTTP"</TableHeaderCell>
                                                <TableHeaderCell attr:style="width:120px;">"Трафик"</TableHeaderCell>
                                                <TableHeaderCell>"Ошибка"</TableHeaderCell>
                                            </TableRow>
                                        </TableHeader>
                                        <TableBody>
                                            {run_list.into_iter().map(|run| view! { <RunHistoryRow run=run /> }).collect_view()}
                                        </TableBody>
                                    </Table>
                                }.into_any()
                            }
                        }}
                    </div>
                </div>
            </div>

            // ---- Metadata tab ----
            <div class="page__content" style=move || if active_tab.get() != "metadata" { "display:none;" } else { "" }>
                <div style="display:grid;grid-template-columns:minmax(0,840px);justify-content:center;padding:var(--spacing-sm);gap:var(--spacing-md);">
                    {move || match metadata.get() {
                        None => view! {
                            <div style="padding:32px;text-align:center;color:var(--color-text-secondary);">
                                {move || {
                                    let tt = task_type.get();
                                    if tt.is_empty() {
                                        "Выберите тип задачи в настройках для просмотра описания".to_string()
                                    } else {
                                        format!("Метаданные для «{tt}» не найдены. Проверьте что воркер задач активен.")
                                    }
                                }}
                            </div>
                        }.into_any(),
                        Some(m) => view! {
                            <CardAnimated delay_ms=0 nav_id="sys_task_details_metadata_info">
                                <h4 class="details-section__title">{m.display_name.clone()}</h4>
                                <div style="display:flex;flex-direction:column;gap:var(--spacing-md);">
                                    <div>
                                        <div style="font-size:var(--font-size-sm);font-weight:700;letter-spacing:0.04em;text-transform:uppercase;color:var(--color-text-tertiary);margin-bottom:6px;">"Описание"</div>
                                        <div style="font-size:var(--font-size-base);color:var(--color-text);line-height:1.6;">{m.description.clone()}</div>
                                    </div>
                                    {if !m.write_tables.is_empty() { view! {
                                        <div>
                                            <div style="font-size:var(--font-size-sm);font-weight:700;letter-spacing:0.04em;text-transform:uppercase;color:var(--color-text-tertiary);margin-bottom:8px;">"Таблицы записи"</div>
                                            <div style="display:flex;flex-wrap:wrap;gap:6px;">
                                                {m.write_tables.iter().map(|table| {
                                                    let table = table.clone();
                                                    view! { <code style="font-size:var(--font-size-sm);padding:4px 8px;border-radius:var(--radius-sm);background:var(--colorNeutralBackground3);">{table}</code> }
                                                }).collect_view()}
                                            </div>
                                        </div>
                                    }.into_any() } else { view! { <></> }.into_any() }}
                                    {if !m.external_apis.is_empty() { view! {
                                        <div>
                                            <div style="font-size:var(--font-size-sm);font-weight:700;letter-spacing:0.04em;text-transform:uppercase;color:var(--color-text-tertiary);margin-bottom:8px;">"Внешние API"</div>
                                            <div style="display:flex;flex-direction:column;gap:8px;">
                                                {m.external_apis.iter().map(|api| {
                                                    let name = api.name.clone();
                                                    let url  = api.base_url.clone();
                                                    let rate = api.rate_limit_desc.clone();
                                                    view! {
                                                        <div style="padding:10px 14px;border-radius:var(--radius-md);background:var(--colorNeutralBackground3);border-left:3px solid var(--colorBrandStroke1);">
                                                            <div style="font-weight:600;font-size:var(--font-size-base);">{name}</div>
                                                            <div style="font-size:var(--font-size-sm);color:var(--color-text-secondary);margin-top:2px;font-family:monospace;">{url}</div>
                                                            <div style="font-size:var(--font-size-sm);color:var(--colorPaletteDarkOrangeForeground2);margin-top:4px;">"⏱ " {rate}</div>
                                                        </div>
                                                    }
                                                }).collect_view()}
                                            </div>
                                        </div>
                                    }.into_any() } else { view! { <></> }.into_any() }}
                                    {if !m.constraints.is_empty() { view! {
                                        <div>
                                            <div style="font-size:var(--font-size-sm);font-weight:700;letter-spacing:0.04em;text-transform:uppercase;color:var(--color-text-tertiary);margin-bottom:8px;">"Ограничения"</div>
                                            <ul style="margin:0;padding-left:20px;display:flex;flex-direction:column;gap:4px;">
                                                {m.constraints.iter().map(|c| {
                                                    let c = c.clone();
                                                    view! { <li style="font-size:var(--font-size-base);color:var(--color-text);line-height:1.5;">{c}</li> }
                                                }).collect_view()}
                                            </ul>
                                        </div>
                                    }.into_any() } else { view! { <></> }.into_any() }}
                                </div>
                            </CardAnimated>
                        }.into_any(),
                    }}
                </div>
            </div>

        </PageFrame>
    }
}
