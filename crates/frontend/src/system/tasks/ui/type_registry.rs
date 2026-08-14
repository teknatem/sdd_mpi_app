//! Реестр типов регламентных заданий.
//!
//! Страница построена на общем шаблоне `spec-list` (см. themes/core/components.css):
//! список именованных пунктов с коротким названием, техническим кодом, описанием,
//! колонками-характеристиками и раскрываемыми подробностями.

use crate::shared::icons::icon;
use crate::shared::page_frame::PageFrame;
use crate::shared::page_standard::PAGE_CAT_LIST;
use crate::system::tasks::api;
use contracts::system::tasks::metadata::{TaskConfigFieldTypeDto, TaskMetadataDto};
use leptos::prelude::*;
use leptos::task::spawn_local;
use std::collections::{HashMap, HashSet};

// ============================================================================
// Helpers
// ============================================================================

fn field_type_label(t: &TaskConfigFieldTypeDto) -> &'static str {
    match t {
        TaskConfigFieldTypeDto::ConnectionMp => "Кабинет МП",
        TaskConfigFieldTypeDto::Integer => "Число",
        TaskConfigFieldTypeDto::Text => "Текст",
        TaskConfigFieldTypeDto::Date => "Дата",
    }
}

fn field_type_badge_class(t: &TaskConfigFieldTypeDto) -> &'static str {
    match t {
        TaskConfigFieldTypeDto::ConnectionMp => "badge badge--primary",
        TaskConfigFieldTypeDto::Integer => "badge badge--success",
        TaskConfigFieldTypeDto::Text => "badge badge--neutral",
        TaskConfigFieldTypeDto::Date => "badge badge--accent",
    }
}

fn plural_types(n: usize) -> String {
    let suffix = if n % 10 == 1 && n % 100 != 11 {
        ""
    } else if (2..=4).contains(&(n % 10)) && !(12..=14).contains(&(n % 100)) {
        "а"
    } else {
        "ов"
    };
    format!("{n} тип{suffix} заданий зарегистрировано")
}

/// Текст, по которому идёт быстрый поиск. Описание попадает в него только в
/// подробном режиме — ищем ровно по тому, что видно на экране.
fn haystack(meta: &TaskMetadataDto, compact: bool) -> String {
    let mut text = format!("{} {}", meta.display_name, meta.task_type);
    if !compact {
        text.push(' ');
        text.push_str(&meta.description);
    }
    text.to_lowercase()
}

// ============================================================================
// Раскрытые подробности типа задания
// ============================================================================

#[component]
fn TaskTypeDetails(meta: TaskMetadataDto) -> impl IntoView {
    let has_write_tables = !meta.write_tables.is_empty();
    let has_constraints = !meta.constraints.is_empty();
    let has_apis = !meta.external_apis.is_empty();
    let has_fields = !meta.config_fields.is_empty();

    view! {
        <div class="task-type-registry__details">
            <div>
                <div class="task-type-registry__section-title">"Описание"</div>
                <div class="task-type-registry__prose">{meta.description.clone()}</div>
            </div>

            <Show when=move || has_write_tables>
                <div>
                    <div class="task-type-registry__section-title">"Таблицы записи"</div>
                    <div class="task-type-registry__chips">
                        {meta.write_tables.clone().into_iter()
                            .map(|table| view! { <code class="spec-list__code">{table}</code> })
                            .collect_view()}
                    </div>
                </div>
            </Show>

            <Show when=move || has_constraints>
                <div>
                    <div class="task-type-registry__section-title">"Ограничения"</div>
                    <ul class="task-type-registry__constraints">
                        {meta.constraints.clone().into_iter()
                            .map(|item| view! { <li>{item}</li> })
                            .collect_view()}
                    </ul>
                </div>
            </Show>

            <Show when=move || has_apis>
                <div>
                    <div class="task-type-registry__section-title">"Внешние API"</div>
                    <div class="task-type-registry__apis">
                        {meta.external_apis.clone().into_iter().map(|api| {
                            let has_limit = !api.rate_limit_desc.is_empty();
                            view! {
                                <div class="task-type-registry__api">
                                    <div class="task-type-registry__api-main">
                                        <div class="spec-list__name">{api.name.clone()}</div>
                                        <code class="task-type-registry__api-url">{api.base_url.clone()}</code>
                                    </div>
                                    <Show when=move || has_limit>
                                        <span class="badge badge--warning">
                                            {icon("zap")}
                                            " "
                                            {api.rate_limit_desc.clone()}
                                        </span>
                                    </Show>
                                </div>
                            }
                        }).collect_view()}
                    </div>
                </div>
            </Show>

            <div>
                <div class="task-type-registry__section-title">"Параметры конфигурации"</div>
                {if has_fields {
                    view! {
                        <div class="table-wrapper">
                            <table class="table__data table--striped">
                                <thead class="table__head">
                                    <tr>
                                        <th class="table__header-cell">"Ключ"</th>
                                        <th class="table__header-cell">"Название"</th>
                                        <th class="table__header-cell">"Тип"</th>
                                        <th class="table__header-cell">"Обяз."</th>
                                        <th class="table__header-cell">"По умолч."</th>
                                        <th class="table__header-cell">"Диапазон"</th>
                                        <th class="table__header-cell">"Подсказка"</th>
                                    </tr>
                                </thead>
                                <tbody>
                                    {meta.config_fields.clone().into_iter().map(|field| {
                                        let range = match (field.min_value, field.max_value) {
                                            (Some(mn), Some(mx)) => format!("{mn} – {mx}"),
                                            (Some(mn), None) => format!("≥ {mn}"),
                                            (None, Some(mx)) => format!("≤ {mx}"),
                                            (None, None) => "—".to_string(),
                                        };
                                        let default_str = field
                                            .default_value
                                            .clone()
                                            .map(|value| format!("`{value}`"))
                                            .unwrap_or_else(|| "—".to_string());
                                        view! {
                                            <tr class="table__row">
                                                <td class="table__cell">
                                                    <code class="spec-list__code">{field.key.clone()}</code>
                                                </td>
                                                <td class="table__cell">{field.label.clone()}</td>
                                                <td class="table__cell">
                                                    <span class=field_type_badge_class(&field.field_type)>
                                                        {field_type_label(&field.field_type)}
                                                    </span>
                                                </td>
                                                <td class="table__cell">
                                                    {if field.required {
                                                        view! {
                                                            <span class="task-type-registry__required">"✓"</span>
                                                        }.into_any()
                                                    } else {
                                                        view! { <span class="text-muted">"—"</span> }.into_any()
                                                    }}
                                                </td>
                                                <td class="table__cell table__cell--muted">{default_str}</td>
                                                <td class="table__cell table__cell--muted">{range}</td>
                                                <td class="table__cell table__cell--muted">{field.hint.clone()}</td>
                                            </tr>
                                        }
                                    }).collect_view()}
                                </tbody>
                            </table>
                        </div>
                    }.into_any()
                } else {
                    view! {
                        <div class="task-type-registry__prose text-muted">
                            "Параметры не определены — используется произвольный JSON."
                        </div>
                    }.into_any()
                }}
            </div>
        </div>
    }
}

// ============================================================================
// Main page
// ============================================================================

#[component]
pub fn TaskTypeRegistryPage() -> impl IntoView {
    let loading = RwSignal::new(true);
    let error = RwSignal::new(None::<String>);
    let types = RwSignal::new(Vec::<TaskMetadataDto>::new());
    let auto_task_counts = RwSignal::new(HashMap::<String, usize>::new());

    // Тулбар списка: краткий/подробный режим и быстрый поиск.
    let compact = RwSignal::new(false);
    let query = RwSignal::new(String::new());
    let expanded = RwSignal::new(HashSet::<String>::new());

    Effect::new(move |_| {
        spawn_local(async move {
            match api::get_task_types().await {
                Ok(list) => {
                    types.set(list);
                }
                Err(e) => {
                    error.set(Some(e));
                }
            }

            match api::fetch_scheduled_tasks().await {
                Ok(tasks) => {
                    let mut counts = HashMap::new();
                    for task in tasks.into_iter().filter(|task| task.is_enabled) {
                        *counts.entry(task.task_type).or_insert(0) += 1;
                    }
                    auto_task_counts.set(counts);
                }
                Err(e) => {
                    error.update(|current| {
                        let message =
                            format!("Не удалось загрузить задания для колонки «Авто»: {e}");
                        *current = Some(match current.take() {
                            Some(existing) => format!("{existing}; {message}"),
                            None => message,
                        });
                    });
                }
            }

            loading.set(false);
        });
    });

    let visible = move || -> Vec<TaskMetadataDto> {
        let needle = query.get().trim().to_lowercase();
        let is_compact = compact.get();
        types
            .get()
            .into_iter()
            .filter(|meta| needle.is_empty() || haystack(meta, is_compact).contains(&needle))
            .collect()
    };

    view! {
        <PageFrame
            page_id="sys_task_type_registry--list"
            category=PAGE_CAT_LIST
            class="task-type-registry"
        >
            <div class="page__header">
                <div class="page__header-left">
                    <h1 class="page__title">"Реестр типов заданий"</h1>
                    <p class="page__subtitle">
                        "Обработчики регламентных заданий: параметры, внешние API и ограничения."
                    </p>
                </div>
                <div class="page__header-right">
                    <span class="badge badge--neutral">
                        {move || plural_types(types.get().len())}
                    </span>
                </div>
            </div>

            <div class="page__content">
                <Show when=move || loading.get()>
                    <div class="loading-state">{icon("refresh-cw")} " Загрузка..."</div>
                </Show>

                {move || error.get().map(|err| view! {
                    <div class="alert alert--error">{icon("alert-circle")} " " {err}</div>
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
                            {move || format!("Показано {} из {}", visible().len(), types.get().len())}
                        </span>
                    </div>

                    <div class="table-wrapper">
                        <table class="table__data">
                            <thead class="table__head">
                                <tr>
                                    <th class="table__header-cell">"Тип задания"</th>
                                    <th class="table__header-cell">"Параметры"</th>
                                    <th class="table__header-cell">"Внешние API"</th>
                                    <th class="table__header-cell">"Авто"</th>
                                </tr>
                            </thead>
                            <tbody>
                                <For
                                    each=visible
                                    key=|meta| meta.task_type.clone()
                                    children=move |meta| {
                                        let task_type = meta.task_type.clone();
                                        let key_for_toggle = task_type.clone();
                                        let key_for_chevron = task_type.clone();
                                        let key_for_body = task_type.clone();
                                        let chevron_open = move || expanded.get().contains(&key_for_chevron);
                                        let body_open = move || expanded.get().contains(&key_for_body);
                                        let field_count = meta.config_fields.len();
                                        let api_count = meta.external_apis.len();
                                        let auto_count_task_type = task_type.clone();
                                        let auto_count = Memo::new(move |_| {
                                            auto_task_counts
                                                .get()
                                                .get(&auto_count_task_type)
                                                .copied()
                                                .unwrap_or(0)
                                        });
                                        let meta_for_details = meta.clone();
                                        view! {
                                            <tr
                                                class="spec-list__row spec-list__row--clickable"
                                                on:click=move |_| expanded.update(|set| {
                                                    if !set.remove(&key_for_toggle) {
                                                        set.insert(key_for_toggle.clone());
                                                    }
                                                })
                                            >
                                                <td class="table__cell">
                                                    <div class="task-type-registry__title">
                                                        <span
                                                            class="task-type-registry__chevron"
                                                            class:task-type-registry__chevron--expanded=chevron_open
                                                        >
                                                            {icon("chevron-right")}
                                                        </span>
                                                        <span class="spec-list__name">{meta.display_name.clone()}</span>
                                                        <code class="spec-list__code">{task_type.clone()}</code>
                                                    </div>
                                                    <div class="spec-list__note">{meta.description.clone()}</div>
                                                </td>
                                                <td class="table__cell">
                                                    <span class="badge badge--neutral">{format!("{field_count} пар.")}</span>
                                                </td>
                                                <td class="table__cell">
                                                    <span class="badge badge--neutral">{format!("{api_count} API")}</span>
                                                </td>
                                                <td class="table__cell">
                                                    <span class=move || {
                                                        if auto_count.get() > 0 {
                                                            "badge badge--primary"
                                                        } else {
                                                            "badge badge--neutral"
                                                        }
                                                    }>
                                                        {move || auto_count.get()}
                                                    </span>
                                                </td>
                                            </tr>
                                            <Show when=body_open>
                                                <tr class="task-type-registry__details-row">
                                                    <td class="table__cell" colspan="4">
                                                        <TaskTypeDetails meta=meta_for_details.clone() />
                                                    </td>
                                                </tr>
                                            </Show>
                                        }
                                    }
                                />
                            </tbody>
                        </table>
                    </div>

                    <Show when=move || !loading.get() && visible().is_empty()>
                        <div class="spec-list__empty">
                            "Ничего не найдено — измените поисковый запрос."
                        </div>
                    </Show>
                </div>
            </div>
        </PageFrame>
    }
}
