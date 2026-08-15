//! Единая страница загрузок с маркетплейса.
//!
//! Одна страница на все три use-case (u502/u503/u504): различия — только каталог
//! операций и `ImportUseCase`. Структура повторяет список DataView:
//! `PageFrame` → `page__header` → `page__content` → `spec-list` + `table__data`.
//!
//! Состояние строк живёт на уровне страницы (`RowState` из `Copy`-сигналов), а не
//! внутри строки: поиск и фильтр по группам перемонтируют строки, а запущенная
//! загрузка и выбранный период при этом обязаны сохраниться. По той же причине
//! опрос прогресса — обычная фоновая задача, не привязанная к жизни компонента.

use chrono::{Duration, NaiveDate, Utc};
use contracts::domain::a006_connection_mp::aggregate::ConnectionMP;
use leptos::prelude::*;
use leptos::task::spawn_local;
use thaw::*;

use super::catalog::{ImportOp, OpGroup};
use super::client::{self, ImportUseCase};
use super::progress::RunProgress;
use crate::shared::components::date_range_picker_v2::DateRangePickerV2;
use crate::shared::icons::icon;
use crate::shared::page_frame::PageFrame;
use crate::shared::page_standard::PAGE_CAT_USECASE;

/// Порядок чипов-групп в тулбаре.
const GROUP_ORDER: [OpGroup; 5] = [
    OpGroup::Catalog,
    OpGroup::Orders,
    OpGroup::Finance,
    OpGroup::Documents,
    OpGroup::Analytics,
];

/// Период по умолчанию — последние трое суток.
///
/// Он же уходит на бэкенд для загрузок без периода: бэкенд эти даты игнорирует,
/// но поля запроса обязательные. Пикеру пустые даты не отдаём намеренно: без
/// значений он выставляет текущий месяц, и случайный клик по «Заказы WB:
/// история» превращался бы в месячный бэкфилл.
fn default_range() -> (NaiveDate, NaiveDate) {
    let today = Utc::now().date_naive();
    (today - Duration::days(3), today)
}

fn iso(date: NaiveDate) -> String {
    date.format("%Y-%m-%d").to_string()
}

// ── Состояние строки ──────────────────────────────────────────────────────────

#[derive(Clone, Copy)]
struct RowState {
    date_from: RwSignal<String>,
    date_to: RwSignal<String>,
    session_id: RwSignal<Option<String>>,
    progress: RwSignal<Option<RunProgress>>,
    error: RwSignal<Option<String>>,
    starting: RwSignal<bool>,
}

impl RowState {
    fn new() -> Self {
        let (from, to) = default_range();
        Self {
            date_from: RwSignal::new(iso(from)),
            date_to: RwSignal::new(iso(to)),
            session_id: RwSignal::new(None),
            progress: RwSignal::new(None),
            error: RwSignal::new(None),
            starting: RwSignal::new(false),
        }
    }

    fn reset(&self) {
        let (from, to) = default_range();
        self.date_from.set(iso(from));
        self.date_to.set(iso(to));
        self.session_id.set(None);
        self.progress.set(None);
        self.error.set(None);
        self.starting.set(false);
    }

    /// Разобранный период; при пустых или битых значениях — период по умолчанию.
    fn range(&self) -> (NaiveDate, NaiveDate) {
        let (default_from, default_to) = default_range();
        let parse = |value: String, default: NaiveDate| {
            NaiveDate::parse_from_str(&value, "%Y-%m-%d").unwrap_or(default)
        };
        (
            parse(self.date_from.get_untracked(), default_from),
            parse(self.date_to.get_untracked(), default_to),
        )
    }
}

// ── localStorage ──────────────────────────────────────────────────────────────

fn storage() -> Option<web_sys::Storage> {
    web_sys::window().and_then(|w| w.local_storage().ok().flatten())
}

fn session_key(prefix: &str, row_id: &str) -> String {
    format!("{}_row_{}_session_id", prefix, row_id)
}

fn progress_key(prefix: &str, row_id: &str) -> String {
    format!("{}_row_{}_progress", prefix, row_id)
}

fn save_session(prefix: &str, row_id: &str, session_id: &str) {
    if let Some(s) = storage() {
        let _ = s.set_item(&session_key(prefix, row_id), session_id);
    }
}

fn save_snapshot(prefix: &str, row_id: &str, progress: &RunProgress) {
    if let (Some(s), Ok(json)) = (storage(), serde_json::to_string(progress)) {
        let _ = s.set_item(&progress_key(prefix, row_id), &json);
    }
}

fn clear_all(prefix: &str, row_id: &str) {
    if let Some(s) = storage() {
        let _ = s.remove_item(&session_key(prefix, row_id));
        let _ = s.remove_item(&progress_key(prefix, row_id));
    }
}

/// Завершённая сессия больше не опрашивается, но её итог должен пережить
/// перезагрузку страницы — особенно если импорт упал с ошибкой.
fn clear_session(prefix: &str, row_id: &str) {
    if let Some(s) = storage() {
        let _ = s.remove_item(&session_key(prefix, row_id));
    }
}

// ── Опрос прогресса ───────────────────────────────────────────────────────────

/// Опрашивать сессию раз в 2 секунды, пока загрузка не завершится.
fn poll(
    use_case: ImportUseCase,
    storage_prefix: &'static str,
    op: &'static ImportOp,
    state: RowState,
    session_id: String,
) {
    spawn_local(async move {
        loop {
            match client::progress(use_case, &session_id, op.aggregate).await {
                Ok(progress) => {
                    save_snapshot(storage_prefix, op.row_id, &progress);
                    let finished = progress.status.is_finished();
                    state.progress.set(Some(progress));
                    if finished {
                        clear_session(storage_prefix, op.row_id);
                        state.session_id.set(None);
                        break;
                    }
                }
                Err(e) => {
                    if e.contains("404") {
                        // Сессии на бэкенде уже нет — забываем и её снапшот.
                        clear_all(storage_prefix, op.row_id);
                        state.session_id.set(None);
                        state.progress.set(None);
                    } else {
                        state
                            .error
                            .set(Some(format!("Ошибка получения прогресса: {}", e)));
                    }
                    break;
                }
            }
            gloo_timers::future::TimeoutFuture::new(2000).await;
        }
    });
}

// ── Страница ──────────────────────────────────────────────────────────────────

#[component]
pub fn ImportPage(
    /// `{entity}--usecase`, например `"u504_import_from_wildberries--usecase"`.
    page_id: &'static str,
    title: &'static str,
    use_case: ImportUseCase,
    /// Префикс ключей localStorage: `u502` / `u503` / `u504`.
    storage_prefix: &'static str,
    ops: &'static [ImportOp],
) -> impl IntoView {
    let connections = RwSignal::new(Vec::<ConnectionMP>::new());
    let selected_connection = RwSignal::new(String::new());
    let error = RwSignal::new(None::<String>);

    let query = RwSignal::new(String::new());
    let group = RwSignal::new(None::<OpGroup>);
    let compact = RwSignal::new(false);

    // Состояние строк создаётся один раз на всю жизнь страницы.
    let states = StoredValue::new(
        ops.iter()
            .map(|op| (op.row_id, RowState::new()))
            .collect::<Vec<_>>(),
    );
    let state_of = move |row_id: &'static str| -> RowState {
        states.with_value(|list| {
            list.iter()
                .find(|(id, _)| *id == row_id)
                .map(|(_, state)| *state)
                .expect("состояние строки создано при монтировании страницы")
        })
    };

    let reload_connections = move || {
        spawn_local(async move {
            match client::load_connections(use_case.marketplace()).await {
                Ok(list) => {
                    if selected_connection.get_untracked().is_empty() {
                        if let Some(first) = list.first() {
                            selected_connection.set(first.to_string_id());
                        }
                    }
                    connections.set(list);
                    error.set(None);
                }
                Err(e) => error.set(Some(format!("Ошибка загрузки подключений: {}", e))),
            }
        });
    };

    // Восстановление после перезагрузки: живые сессии продолжаем опрашивать,
    // завершённые показываем по снапшоту.
    Effect::new(move |_| {
        reload_connections();

        for op in ops {
            let state = state_of(op.row_id);
            if let Some(s) = storage() {
                if let Ok(Some(json)) = s.get_item(&progress_key(storage_prefix, op.row_id)) {
                    if let Ok(snapshot) = serde_json::from_str::<RunProgress>(&json) {
                        state.progress.set(Some(snapshot));
                    }
                }
                if let Ok(Some(session_id)) = s.get_item(&session_key(storage_prefix, op.row_id)) {
                    state.session_id.set(Some(session_id.clone()));
                    poll(use_case, storage_prefix, op, state, session_id);
                }
            }
        }
    });

    // Смена подключения обнуляет результаты: они относились к другому кабинету.
    // Первичный автовыбор подключения (пусто → первое) сбросом не считается,
    // иначе он затёр бы только что восстановленные сессии.
    let previous_connection = StoredValue::new(selected_connection.get_untracked());
    Effect::new(move |_| {
        let current = selected_connection.get();
        let previous = previous_connection.get_value();
        previous_connection.set_value(current.clone());
        if current != previous && !previous.is_empty() {
            for op in ops {
                state_of(op.row_id).reset();
                clear_all(storage_prefix, op.row_id);
            }
        }
    });

    let start_row = move |op: &'static ImportOp| {
        let state = state_of(op.row_id);
        let connection_id = selected_connection.get_untracked();
        if connection_id.is_empty() {
            state.error.set(Some("Сначала выберите подключение".to_string()));
            return;
        }

        state.starting.set(true);
        state.error.set(None);
        state.progress.set(None);
        clear_all(storage_prefix, op.row_id);

        let (date_from, date_to) = state.range();
        spawn_local(async move {
            match client::start(use_case, connection_id, op.aggregate, date_from, date_to).await {
                Ok(session_id) => {
                    save_session(storage_prefix, op.row_id, &session_id);
                    state.session_id.set(Some(session_id.clone()));
                    state.starting.set(false);
                    poll(use_case, storage_prefix, op, state, session_id);
                }
                Err(e) => {
                    state.error.set(Some(format!("Ошибка запуска: {}", e)));
                    state.starting.set(false);
                }
            }
        });
    };

    // Чипы групп показываем, только если групп в каталоге больше одной.
    let groups: Vec<OpGroup> = GROUP_ORDER
        .into_iter()
        .filter(|g| ops.iter().any(|op| op.group == *g))
        .collect();
    let has_groups = groups.len() > 1;

    // Ищем ровно по тому, что видно на экране: пояснение — только в подробном режиме.
    let visible = move || -> Vec<&'static ImportOp> {
        let needle = query.get().trim().to_lowercase();
        let selected = group.get();
        let is_compact = compact.get();
        ops.iter()
            .filter(|op| selected.is_none() || selected == Some(op.group))
            .filter(|op| {
                if needle.is_empty() {
                    return true;
                }
                let mut haystack = format!(
                    "{} {} {} {}",
                    op.title,
                    op.aggregate,
                    op.group.label(),
                    op.period.label()
                );
                if !is_compact {
                    haystack.push(' ');
                    haystack.push_str(op.period_note);
                    haystack.push(' ');
                    haystack.push_str(op.details);
                }
                haystack.to_lowercase().contains(&needle)
            })
            .collect()
    };

    view! {
        <PageFrame page_id=page_id category=PAGE_CAT_USECASE class="import-ops">
            <div class="page__header">
                <div class="page__header-left">
                    <h1 class="page__title">{icon("download-cloud")} " " {title}</h1>
                </div>
                <div class="page__header-right">
                    <Button
                        appearance=ButtonAppearance::Secondary
                        size=ButtonSize::Small
                        on_click=move |_| reload_connections()
                    >
                        {icon("refresh")}
                        " Обновить"
                    </Button>
                </div>
            </div>

            <div class="page__content">
                {move || error.get().map(|e| view! {
                    <div class="warning-box warning-box--error">
                        <span class="warning-box__icon">"⚠"</span>
                        <span class="warning-box__text">{e}</span>
                    </div>
                })}

                <div class="spec-list" class:spec-list--compact=move || compact.get()>
                    <div class="spec-list__toolbar">
                        <div class="spec-list__search">
                            <span class="spec-list__search-icon">{icon("search")}</span>
                            <input
                                class="form__input"
                                type="text"
                                placeholder="Поиск по видимым текстам"
                                prop:value=move || query.get()
                                on:input=move |ev| query.set(event_target_value(&ev))
                            />
                            <Show when=move || !query.get().is_empty()>
                                <button
                                    class="spec-list__search-clear"
                                    title="Очистить"
                                    on:click=move |_| query.set(String::new())
                                >
                                    {icon("x")}
                                </button>
                            </Show>
                        </div>

                        <select
                            class="doc-filter__select import-ops__connection"
                            on:change=move |ev| selected_connection.set(event_target_value(&ev))
                        >
                            <option value="">"— выберите подключение —"</option>
                            {move || connections.get().into_iter().map(|conn| {
                                let id = conn.to_string_id();
                                let selected = id == selected_connection.get();
                                view! {
                                    <option selected=selected value=id>
                                        {conn.base.description.clone()}
                                    </option>
                                }
                            }).collect_view()}
                        </select>

                        {has_groups.then(|| view! {
                            <div class="dpc-mode-tabs">
                                <button
                                    class="dpc-mode-tab"
                                    class:dpc-mode-tab--active=move || group.get().is_none()
                                    on:click=move |_| group.set(None)
                                >
                                    "Все"
                                </button>
                                {groups.into_iter().map(|g| view! {
                                    <button
                                        class="dpc-mode-tab"
                                        class:dpc-mode-tab--active=move || group.get() == Some(g)
                                        on:click=move |_| group.set(Some(g))
                                    >
                                        {g.label()}
                                    </button>
                                }).collect_view()}
                            </div>
                        })}

                        <div class="dpc-mode-tabs">
                            <button
                                class="dpc-mode-tab"
                                class:dpc-mode-tab--active=move || compact.get()
                                on:click=move |_| compact.set(true)
                            >
                                "Кратко"
                            </button>
                            <button
                                class="dpc-mode-tab"
                                class:dpc-mode-tab--active=move || !compact.get()
                                on:click=move |_| compact.set(false)
                            >
                                "Подробно"
                            </button>
                        </div>

                        <span class="spec-list__count">
                            {move || format!("Показано {} из {}", visible().len(), ops.len())}
                        </span>
                    </div>

                    <div class="table-wrapper">
                        <table class="table__data">
                            <thead class="table__head">
                                <tr>
                                    <th class="table__header-cell">"Загрузка"</th>
                                    <th class="table__header-cell">"Группа"</th>
                                    <th class="table__header-cell">"Тип периода"</th>
                                    <th class="table__header-cell">"Период"</th>
                                    <th class="table__header-cell">"Статус"</th>
                                    <th class="table__header-cell"></th>
                                </tr>
                            </thead>
                            <tbody>
                                {move || visible()
                                    .into_iter()
                                    .map(|op| import_op_row(op, state_of(op.row_id), start_row))
                                    .collect_view()}
                            </tbody>
                        </table>
                    </div>

                    <Show when=move || visible().is_empty()>
                        <div class="spec-list__empty">
                            "Ничего не найдено — измените запрос или снимите фильтр группы."
                        </div>
                    </Show>
                </div>
            </div>
        </PageFrame>
    }
}

// ── Строка ────────────────────────────────────────────────────────────────────

/// Одна операция загрузки: основная строка плюс строка сообщений под ней.
fn import_op_row(
    op: &'static ImportOp,
    state: RowState,
    start: impl Fn(&'static ImportOp) + Copy + Send + Sync + 'static,
) -> impl IntoView {
    let button_label = move || {
        if state.starting.get() {
            "Запуск..."
        } else if state.session_id.get().is_some() {
            "В работе"
        } else {
            "Запустить"
        }
    };

    let messages = move || {
        let mut lines = Vec::new();
        if let Some(error) = state.error.get() {
            lines.push(error);
        }
        if let Some(progress) = state.progress.get() {
            if let Some(item) = progress.current_item.filter(|i| !i.trim().is_empty()) {
                lines.push(format!("Текущий элемент: {}", item));
            }
            lines.extend(progress.messages);
        }
        lines
    };

    view! {
        <tr class="spec-list__row" style=format!("--spec-cat:{};", op.group.stripe())>
            <td class="table__cell">
                <div class="spec-list__name">{op.title}</div>
                <span class="spec-list__ident">{op.aggregate}</span>
                <div class="spec-list__note">{op.details}</div>
            </td>
            <td class="table__cell">
                <span class=op.group.badge_class()>{op.group.label()}</span>
            </td>
            <td class="table__cell">
                <span class=op.period.badge_class()>{op.period.label()}</span>
                <div class="spec-list__note">
                    {if op.period_note.is_empty() { op.period.hint() } else { op.period_note }}
                </div>
            </td>
            <td class="table__cell import-ops__period">
                {if op.period.needs_period() {
                    view! {
                        <DateRangePickerV2
                            date_from=state.date_from
                            date_to=state.date_to
                            on_change=Callback::new(move |(from, to): (String, String)| {
                                state.date_from.set(from);
                                state.date_to.set(to);
                            })
                        />
                    }.into_any()
                } else {
                    view! { <span class="text-muted">"—"</span> }.into_any()
                }}
            </td>
            <td class="table__cell">
                {move || match state.progress.get() {
                    Some(progress) => {
                        let percent = progress.percent();
                        view! {
                            <span class=progress.status.badge_class()>
                                {progress.status.label()}
                            </span>
                            <div class="import-ops__counters">{progress.counters()}</div>
                            {percent.map(|percent| view! {
                                <div class="import-ops__bar">
                                    <div
                                        class="import-ops__bar-fill"
                                        style=format!("width:{}%;", percent)
                                    ></div>
                                </div>
                            })}
                        }.into_any()
                    }
                    None => view! {
                        <span class="badge badge--neutral">"Не запускалось"</span>
                    }.into_any(),
                }}
            </td>
            <td class="table__cell">
                <Button
                    appearance=ButtonAppearance::Primary
                    size=ButtonSize::Small
                    on_click=move |_| start(op)
                    disabled=move || state.starting.get() || state.session_id.get().is_some()
                >
                    {icon("play")}
                    " "
                    {button_label}
                </Button>
            </td>
        </tr>
        {move || {
            let lines = messages();
            (!lines.is_empty()).then(|| view! {
                <tr class="spec-list__row" style=format!("--spec-cat:{};", op.group.stripe())>
                    <td class="table__cell" colspan="6">
                        <div class="import-ops__messages">{lines.join("\n")}</div>
                    </td>
                </tr>
            })
        }}
    }
}
