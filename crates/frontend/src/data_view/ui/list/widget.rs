//! DataView list page — каталог зарегистрированных DataView.
//!
//! Построена на общем шаблоне `spec-list` (см. themes/core/components.css):
//! короткое имя + технический id + описание, колонки-характеристики, категория
//! цветной полосой слева, быстрый поиск и режим «кратко/подробно».

use crate::data_view::api;
use crate::data_view::types::DataViewMeta;
use crate::layout::global_context::AppGlobalContext;
use crate::shared::icons::icon;
use crate::shared::page_frame::PageFrame;
use crate::shared::page_standard::PAGE_CAT_LIST;
use leptos::prelude::*;
use thaw::*;

/// Категория DataView: подпись, класс бейджа и цвет полосы слева.
/// Полоса берёт насыщенный токен темы — она единственная метка категории,
/// которую видно, не читая строку.
fn category_meta(category: &str) -> (String, &'static str, &'static str) {
    match category {
        "revenue" => (
            "Выручка".to_string(),
            "badge badge--success",
            "var(--color-success)",
        ),
        "orders" => (
            "Заказы".to_string(),
            "badge badge--primary",
            "var(--color-primary)",
        ),
        "costs" => (
            "Затраты".to_string(),
            "badge badge--warning",
            "var(--color-warning)",
        ),
        "returns" => (
            "Возвраты".to_string(),
            "badge badge--error",
            "var(--color-error)",
        ),
        other => (other.to_string(), "badge badge--neutral", "var(--color-border)"),
    }
}

const CATEGORY_ALL: &str = "";

/// Текст, по которому идёт быстрый поиск. Описание участвует только в подробном
/// режиме — ищем ровно по тому, что видно на экране.
fn haystack(meta: &DataViewMeta, compact: bool) -> String {
    let mut text = format!(
        "{} {} {} {}",
        meta.name,
        meta.id,
        category_meta(&meta.category).0,
        meta.data_sources.join(" "),
    );
    if !compact {
        text.push(' ');
        text.push_str(&meta.description);
    }
    text.to_lowercase()
}

#[component]
#[allow(non_snake_case)]
pub fn DataViewList() -> impl IntoView {
    let ctx = use_context::<AppGlobalContext>().expect("AppGlobalContext not found");

    let items = RwSignal::new(Vec::<DataViewMeta>::new());
    let loading = RwSignal::new(true);
    let error = RwSignal::new(None::<String>);

    // Тулбар списка: поиск, фильтр по категории, краткий/подробный режим.
    let query = RwSignal::new(String::new());
    let category = RwSignal::new(CATEGORY_ALL.to_string());
    let compact = RwSignal::new(false);

    let reload = move || {
        wasm_bindgen_futures::spawn_local(async move {
            loading.set(true);
            error.set(None);
            match api::fetch_list().await {
                Ok(data) => {
                    items.set(data);
                    loading.set(false);
                }
                Err(e) => {
                    error.set(Some(e));
                    loading.set(false);
                }
            }
        });
    };

    Effect::new(move |_| reload());

    // Категории берутся из данных: список DataView расширяется без правок UI.
    let categories = move || {
        let mut list: Vec<String> = items
            .get()
            .into_iter()
            .map(|meta| meta.category)
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect();
        list.insert(0, CATEGORY_ALL.to_string());
        list
    };

    let visible = move || -> Vec<DataViewMeta> {
        let needle = query.get().trim().to_lowercase();
        let selected = category.get();
        let is_compact = compact.get();
        items
            .get()
            .into_iter()
            .filter(|meta| selected == CATEGORY_ALL || meta.category == selected)
            .filter(|meta| needle.is_empty() || haystack(meta, is_compact).contains(&needle))
            .collect()
    };

    view! {
        <PageFrame page_id="data_view--list" category=PAGE_CAT_LIST>
            <div class="page__header">
                <div class="page__header-left">
                    <h1 class="page__title">{icon("layers")} " DataView — Источники данных"</h1>
                </div>
                <div class="page__header-right">
                    <Button
                        appearance=ButtonAppearance::Secondary
                        size=ButtonSize::Small
                        on_click=move |_| reload()
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

                <Show when=move || loading.get()>
                    <div class="loading-state">{icon("refresh-cw")} " Загрузка каталога DataView..."</div>
                </Show>

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
                            {move || categories().into_iter().map(|item| {
                                let label = if item == CATEGORY_ALL {
                                    "Все".to_string()
                                } else {
                                    category_meta(&item).0
                                };
                                let for_active = item.clone();
                                let for_click = item.clone();
                                view! {
                                    <button
                                        class="dpc-mode-tab"
                                        class:dpc-mode-tab--active=move || category.get() == for_active
                                        on:click=move |_| category.set(for_click.clone())
                                    >
                                        {label}
                                    </button>
                                }
                            }).collect_view()}
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
                            {move || format!("Показано {} из {}", visible().len(), items.get().len())}
                        </span>
                    </div>

                    <div class="table-wrapper">
                        <table class="table__data">
                            <thead class="table__head">
                                <tr>
                                    <th class="table__header-cell">"DataView"</th>
                                    <th class="table__header-cell">"Категория"</th>
                                    <th class="table__header-cell">"Идентификатор"</th>
                                    <th class="table__header-cell">"Источники"</th>
                                    <th class="table__header-cell">"Измерения"</th>
                                    <th class="table__header-cell">"Версия"</th>
                                </tr>
                            </thead>
                            <tbody>
                                {move || {
                                    let ctx = ctx.clone();
                                    visible().into_iter().map(move |meta| {
                                        let ctx = ctx.clone();
                                        let (label, badge_class, stripe) = category_meta(&meta.category);
                                        let tab_key = format!("data_view_details_{}", meta.id);
                                        let tab_title = format!("DataView · {}", meta.id);
                                        view! {
                                            <tr
                                                class="spec-list__row spec-list__row--clickable"
                                                style=format!("--spec-cat:{stripe};")
                                                on:click=move |_| ctx.open_tab(&tab_key, &tab_title)
                                            >
                                                <td class="table__cell">
                                                    <div class="spec-list__name">{meta.name.clone()}</div>
                                                    <div class="spec-list__note">{meta.description.clone()}</div>
                                                </td>
                                                <td class="table__cell">
                                                    <span class=badge_class>{label}</span>
                                                </td>
                                                <td class="table__cell">
                                                    <span class="spec-list__ident">{meta.id.clone()}</span>
                                                </td>
                                                <td class="table__cell table__cell--muted">
                                                    {meta.data_sources.join(", ")}
                                                </td>
                                                <td class="table__cell table__cell--muted">
                                                    {format!("{} измерений", meta.available_dimensions.len())}
                                                </td>
                                                <td class="table__cell table__cell--muted">
                                                    {format!("v{}", meta.version)}
                                                </td>
                                            </tr>
                                        }
                                    })
                                    .collect_view()
                                }}
                            </tbody>
                        </table>
                    </div>

                    <Show when=move || !loading.get() && visible().is_empty()>
                        <div class="spec-list__empty">
                            "Ничего не найдено — измените запрос или снимите фильтр категории."
                        </div>
                    </Show>
                </div>
            </div>
        </PageFrame>
    }
}
