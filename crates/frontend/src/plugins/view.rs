//! Страница просмотра плагина — рабочая версия (рантайм).
//!
//! Открывается из меню/сайдбара по ключу `plugin__<id>`. Вкладки: «Приложение»
//! (плагин в iframe на всю ширину/высоту) и «Журнал» (события жизненного цикла +
//! серверный журнал host.log.*). Редактирование кода — на странице разработки
//! (`plugin_dev__<id>`, [`crate::plugins::PluginHost`]).

use crate::plugins::api;
use crate::plugins::frame::PluginFrame;
use crate::shared::date_utils::format_utc_local;
use crate::system::favorites::ui::FavoriteButton;
use contracts::plugins::{PluginDataMode, PluginDefinition, PluginRunContext};
use leptos::prelude::*;
use leptos::task::spawn_local;

#[component]
pub fn PluginView(plugin_id: String) -> impl IntoView {
    // Идентификатор/ключ вкладки для «Избранного» (target_id и tab_key plugin__<id>).
    let fav_target_id = plugin_id.clone();
    let fav_tab_key = format!("plugin__{}", plugin_id);
    let plugin_id_for_rating = plugin_id.clone();
    // Изменение оценки — управляющее действие (эндпоинт admin-only). Обычный
    // пользователь видит звёзды только для чтения.
    let user_is_admin = crate::system::auth::context::is_admin();
    let (def, set_def) = signal(None::<PluginDefinition>);
    let (loading, set_loading) = signal(true);
    let (error, set_error) = signal(None::<String>);
    let client_src = RwSignal::new(String::new());
    let styles_src = RwSignal::new(String::new());
    let kits = RwSignal::new(Vec::<String>::new());
    let context = RwSignal::new(PluginRunContext::default());
    let restart = RwSignal::new(0u64);
    let data_mode = RwSignal::new(PluginDataMode::Live);
    // Журнал — выдвижная нижняя панель поверх iframe (тоггл).
    let show_log = RwSignal::new(false);
    // Плагин-редактор сообщает о несохранённых правках через host.setDirty.
    // Restart перестраивает документ iframe целиком, то есть молча выбрасывает
    // их, — поэтому спрашиваем подтверждение.
    let dirty = RwSignal::new(false);

    // Журналы плагина — наполняются PluginFrame, рендерятся в выдвижной панели.
    let console = RwSignal::new(Vec::<String>::new());
    let events = RwSignal::new(Vec::<String>::new());

    {
        let id = plugin_id.clone();
        spawn_local(async move {
            match api::get_by_id(&id).await {
                Ok(plugin) => {
                    client_src.set(plugin.bundle.client_script.clone().unwrap_or_default());
                    styles_src.set(plugin.bundle.styles.clone().unwrap_or_default());
                    kits.set(crate::plugins::frame::kits::kit_names(
                        &plugin.bundle.manifest,
                    ));
                    context.set(crate::plugins::host::model::default_run_context(
                        &plugin.bundle,
                    ));
                    set_def.set(Some(plugin));
                    restart.update(|value| *value += 1);
                }
                Err(message) => set_error.set(Some(message)),
            }
            set_loading.set(false);
        });
    }

    // Restart перечитывает плагин из БД и заново монтирует iframe. Так работает
    // сценарий «два монитора»: сохранил свежую версию на странице разработки —
    // нажал Restart здесь и получил её без перезагрузки вкладки.
    let restart_plugin = {
        let id = plugin_id.clone();
        move |_| {
            if !confirm_discard(dirty) {
                return;
            }
            console.set(Vec::new());
            events.set(Vec::new());
            let id = id.clone();
            spawn_local(async move {
                match api::get_by_id(&id).await {
                    Ok(plugin) => {
                        client_src.set(plugin.bundle.client_script.clone().unwrap_or_default());
                        styles_src.set(plugin.bundle.styles.clone().unwrap_or_default());
                        kits.set(crate::plugins::frame::kits::kit_names(
                            &plugin.bundle.manifest,
                        ));
                        context.set(crate::plugins::host::model::default_run_context(
                            &plugin.bundle,
                        ));
                        set_def.set(Some(plugin));
                    }
                    Err(message) => set_error.set(Some(message)),
                }
                restart.update(|value| *value += 1);
            });
        }
    };

    let select_live = move |_| {
        if data_mode.get_untracked() != PluginDataMode::Live {
            if !confirm_discard(dirty) {
                return;
            }
            data_mode.set(PluginDataMode::Live);
            restart.update(|value| *value += 1);
        }
    };
    let select_snapshot = move |_| {
        let available = def
            .get_untracked()
            .and_then(|plugin| plugin.snapshot)
            .is_some();
        if available && data_mode.get_untracked() != PluginDataMode::Snapshot {
            if !confirm_discard(dirty) {
                return;
            }
            data_mode.set(PluginDataMode::Snapshot);
            restart.update(|value| *value += 1);
        }
    };

    // Заголовок для карточки «Избранного»: имя плагина (или код/«Плагин», пока грузится).
    let fav_title = Signal::derive(move || {
        def.get()
            .map(|p| p.bundle.manifest.title)
            .filter(|t| !t.is_empty())
            .or_else(|| def.get().map(|p| p.bundle.manifest.code))
            .filter(|t| !t.is_empty())
            .unwrap_or_else(|| "Плагин".into())
    });

    view! {
        <div class="plugin-host plugin-host--view">
            {move || error.get().map(|message| view! {
                <div class="plugin-host__alert plugin-host__alert--error">{message}</div>
            })}

            // Узкий заголовок: ★ · Наименование · код · Restart · Журнал.
            <div class="plugin-host__bar">
                <FavoriteButton
                    target_kind="plugin".to_string()
                    target_id=fav_target_id
                    target_title=fav_title
                    tab_key=fav_tab_key
                />
                <h2
                    class="plugin-host__title"
                    title=move || def.get().and_then(|p| p.bundle.manifest.description).unwrap_or_default()
                >
                    {move || def.get().map(|p| p.bundle.manifest.title)
                        .filter(|t| !t.is_empty())
                        .unwrap_or_else(|| if loading.get() { "Загрузка…".into() } else { "Плагин".into() })}
                </h2>
                <span class="plugin-host__code">
                    {move || def.get().map(|p| p.bundle.manifest.code).unwrap_or_default()}
                </span>
                // Оценка плагина: 5 звёзд. Клик по текущей звезде снимает оценку (только админ).
                <div
                    title=if user_is_admin { "Оценить плагин" } else { "Оценка плагина" }
                    style="display: inline-flex; gap: 2px; font-size: 16px; line-height: 1;"
                >
                    {move || {
                        let pid = plugin_id_for_rating.clone();
                        let current = def.get().and_then(|p| p.rating).unwrap_or(0);
                        (1..=5)
                            .map(|n| {
                                let pid = pid.clone();
                                let filled = n <= current;
                                let color = if filled { "#f5a623" } else { "var(--color-text-secondary, #9ca3af)" };
                                let glyph = if filled { "★" } else { "☆" };
                                if !user_is_admin {
                                    // Пользователь — только просмотр, без обработчика клика.
                                    return view! {
                                        <span style=format!("padding:0 1px;line-height:1;color:{color};")>
                                            {glyph}
                                        </span>
                                    }.into_any();
                                }
                                view! {
                                    <button
                                        type="button"
                                        title=move || format!("Оценка: {}", n)
                                        style=move || format!(
                                            "background:none;border:none;cursor:pointer;padding:0 1px;line-height:1;color:{};",
                                            if filled { "#f5a623" } else { "var(--color-text-secondary, #9ca3af)" }
                                        )
                                        on:click=move |_| {
                                            let pid = pid.clone();
                                            let target = if current == n { None } else { Some(n) };
                                            wasm_bindgen_futures::spawn_local(async move {
                                                match api::set_rating(&pid, target).await {
                                                    Ok(()) => set_def.update(|opt| {
                                                        if let Some(p) = opt { p.rating = target; }
                                                    }),
                                                    Err(e) => set_error.set(Some(format!("Ошибка оценки: {}", e))),
                                                }
                                            });
                                        }
                                    >
                                        {if filled { "★" } else { "☆" }}
                                    </button>
                                }.into_any()
                            })
                            .collect_view()
                    }}
                </div>
                <div class="plugin-data-mode" role="group" aria-label="Режим данных">
                    <button
                        class="plugin-data-mode__button plugin-data-mode__button--live"
                        class:plugin-data-mode__button--active=move || data_mode.get() == PluginDataMode::Live
                        on:click=select_live
                        title="Текущие данные из источника"
                    >
                        <span class="plugin-data-mode__dot"></span>"LIVE"
                    </button>
                    <button
                        class="plugin-data-mode__button plugin-data-mode__button--snapshot"
                        class:plugin-data-mode__button--active=move || data_mode.get() == PluginDataMode::Snapshot
                        disabled=move || def.get().and_then(|plugin| plugin.snapshot).is_none()
                        on:click=select_snapshot
                        title=move || def.get().and_then(|plugin| plugin.snapshot).map(|snapshot| {
                            format!("Снимок: {} · {} строк · {} KiB", format_utc_local(&snapshot.created_at, "%d.%m.%Y %H:%M"), snapshot.row_count, snapshot.size_bytes / 1024)
                        }).unwrap_or_else(|| "Сохраненный снимок отсутствует".to_string())
                    >
                        <span class="plugin-data-mode__dot plugin-data-mode__dot--snapshot"></span>
                        {move || {
                            if data_mode.get() == PluginDataMode::Snapshot {
                                def.get().and_then(|plugin| plugin.snapshot).map(|snapshot| {
                                    format!("Снимок · {} · {} стр.", format_utc_local(&snapshot.created_at, "%d.%m %H:%M"), snapshot.row_count)
                                }).unwrap_or_else(|| "Снимок".to_string())
                            } else {
                                "Сохраненные данные".to_string()
                            }
                        }}
                    </button>
                </div>
                <span class="plugin-host__bar-spacer"></span>
                <button
                    class="plugin-host__run plugin-host__run--server"
                    on:click=restart_plugin
                    title="Перечитать свежую версию из БД и перезапустить плагин"
                >
                    "Restart"
                </button>
                <button
                    class="plugin-host__run plugin-host__run--server"
                    class:plugin-host__run--active=move || show_log.get()
                    on:click=move |_| show_log.update(|v| *v = !*v)
                >
                    "Журнал"
                </button>
            </div>

            // Сцена: iframe на всё место + выдвижной журнал поверх него.
            <div class="plugin-host__stage">
                <PluginFrame
                    plugin_id=plugin_id
                    client_src=client_src
                    styles_src=styles_src
                    kits=kits
                    dirty=dirty
                    context=context
                    data_mode=data_mode
                    restart=restart
                    console=console
                    events=events
                    dev=true
                    frameless=true
                />

                <div
                    class="plugin-host__drawer"
                    class:plugin-host__hidden=move || !show_log.get()
                >
                    <div class="plugin-host__drawer-head">
                        <span class="plugin-host__drawer-title">"Журнал"</span>
                        <button
                            class="plugin-host__console-clear"
                            on:click=move |_| { events.set(Vec::new()); console.set(Vec::new()); }
                        >
                            "Очистить"
                        </button>
                        <span class="plugin-host__bar-spacer"></span>
                        <button
                            class="plugin-host__drawer-close"
                            on:click=move |_| show_log.set(false)
                            aria-label="Закрыть журнал"
                        >
                            "×"
                        </button>
                    </div>
                    <div class="plugin-host__drawer-body">
                        {move || {
                            let event_lines = events.get();
                            let console_lines = console.get();
                            if event_lines.is_empty() && console_lines.is_empty() {
                                view! {
                                    <div class="plugin-host__state">
                                        "Журнал пуст. Взаимодействуйте с плагином — события появятся здесь."
                                    </div>
                                }.into_any()
                            } else {
                                view! {
                                    <div class="plugin-host__logs">
                                        {(!event_lines.is_empty()).then(|| view! {
                                            <div class="plugin-host__console plugin-host__events">
                                                <div class="plugin-host__console-head">
                                                    <span class="plugin-host__console-title">"Журнал событий"</span>
                                                </div>
                                                <div class="plugin-host__console-body">
                                                    {event_lines.into_iter().map(|line| view! {
                                                        <div class="plugin-host__console-line">{line}</div>
                                                    }).collect_view()}
                                                </div>
                                            </div>
                                        })}
                                        {(!console_lines.is_empty()).then(|| view! {
                                            <div class="plugin-host__console">
                                                <div class="plugin-host__console-head">
                                                    <span class="plugin-host__console-title">"Серверный журнал"</span>
                                                </div>
                                                <div class="plugin-host__console-body">
                                                    {console_lines.into_iter().map(|line| view! {
                                                        <div class="plugin-host__console-line">{line}</div>
                                                    }).collect_view()}
                                                </div>
                                            </div>
                                        })}
                                    </div>
                                }.into_any()
                            }
                        }}
                    </div>
                </div>
            </div>
        </div>
    }
}

/// Спросить подтверждение, если у плагина есть несохранённые правки.
///
/// Прикрывает только те пути, которыми управляет хост, — Restart и смену режима
/// данных. **Закрытие вкладки браузера так не перехватить**: из sandbox-iframe
/// без `allow-same-origin` до `beforeunload` родителя не дотянуться, а сам
/// хост о закрытии вкладки узнаёт уже после факта. Редактору остаётся сохранять
/// часто, а не рассчитывать на диалог.
fn confirm_discard(dirty: RwSignal<bool>) -> bool {
    if !dirty.get_untracked() {
        return true;
    }
    let confirmed = web_sys::window()
        .and_then(|window| {
            window
                .confirm_with_message(
                    "В плагине есть несохранённые изменения. Продолжить и потерять их?",
                )
                .ok()
        })
        .unwrap_or(true);
    if confirmed {
        dirty.set(false);
    }
    confirmed
}
