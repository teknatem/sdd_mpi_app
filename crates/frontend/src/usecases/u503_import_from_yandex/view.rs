use super::api;
use crate::shared::page_frame::PageFrame;
use chrono::{Duration, NaiveDate, Utc};
use contracts::domain::a006_connection_mp::aggregate::ConnectionMP;
use contracts::domain::common::AggregateId;
use contracts::enums::marketplace_type::MarketplaceType;
use contracts::usecases::u503_import_from_yandex::{
    progress::{ImportProgress, ImportStatus},
    request::{ImportMode, ImportRequest},
};
use leptos::prelude::*;
use leptos::task::spawn_local;
use std::collections::HashMap;
use thaw::*;

fn storage() -> Option<web_sys::Storage> {
    web_sys::window().and_then(|w| w.local_storage().ok().flatten())
}

fn row_session_key(row_id: &str) -> String {
    format!("u503_row_{}_session_id", row_id)
}

fn row_progress_key(row_id: &str) -> String {
    format!("u503_row_{}_progress", row_id)
}

fn save_row_session_id(row_id: &str, session_id: &str) {
    if let Some(s) = storage() {
        let _ = s.set_item(&row_session_key(row_id), session_id);
    }
}

fn load_row_session_id(row_id: &str) -> Option<String> {
    storage().and_then(|s| s.get_item(&row_session_key(row_id)).ok().flatten())
}

fn save_row_progress_snapshot(row_id: &str, progress: &ImportProgress) {
    if let Ok(json) = serde_json::to_string(progress) {
        if let Some(s) = storage() {
            let _ = s.set_item(&row_progress_key(row_id), &json);
        }
    }
}

fn load_row_progress_snapshot(row_id: &str) -> Option<ImportProgress> {
    storage()
        .and_then(|s| s.get_item(&row_progress_key(row_id)).ok().flatten())
        .and_then(|json| serde_json::from_str(&json).ok())
}

fn clear_row_storage(row_id: &str) {
    if let Some(s) = storage() {
        let _ = s.remove_item(&row_session_key(row_id));
        let _ = s.remove_item(&row_progress_key(row_id));
    }
}

/// Завершённая сессия больше не опрашивается, но её итоговый snapshot должен
/// остаться после перезагрузки страницы, особенно если импорт завершился ошибкой.
fn clear_row_session(row_id: &str) {
    if let Some(s) = storage() {
        let _ = s.remove_item(&row_session_key(row_id));
    }
}

fn is_finished(progress: &ImportProgress) -> bool {
    matches!(
        progress.status,
        ImportStatus::Completed
            | ImportStatus::CompletedWithErrors
            | ImportStatus::Failed
            | ImportStatus::Cancelled
    )
}

#[component]
fn ServiceRow(
    row_id: &'static str,
    title: &'static str,
    description: &'static str,
    aggregate: &'static str,
    #[prop(default = false)] needs_period: bool,
    #[prop(into)] selected_connection: Signal<String>,
) -> impl IntoView {
    let default_date_from = Utc::now().naive_utc().date() - Duration::days(3);
    let default_date_to = Utc::now().naive_utc().date();

    let (date_from, set_date_from) = signal(default_date_from.format("%Y-%m-%d").to_string());
    let (date_to, set_date_to) = signal(default_date_to.format("%Y-%m-%d").to_string());
    let (session_id, set_session_id) = signal(None::<String>);
    let (progress, set_progress) = signal(None::<ImportProgress>);
    let (error_msg, set_error_msg) = signal(String::new());
    let (is_starting, set_is_starting) = signal(false);

    let prev_connection = StoredValue::new(selected_connection.get_untracked());
    Effect::new(move || {
        let current = selected_connection.get();
        if current != prev_connection.get_value() {
            prev_connection.set_value(current);
            set_session_id.set(None);
            set_progress.set(None);
            set_error_msg.set(String::new());
            set_date_from.set(default_date_from.format("%Y-%m-%d").to_string());
            set_date_to.set(default_date_to.format("%Y-%m-%d").to_string());
            set_is_starting.set(false);
            clear_row_storage(row_id);
        }
    });

    Effect::new(move || {
        if session_id.get().is_none() {
            if let Some(saved_id) = load_row_session_id(row_id) {
                set_session_id.set(Some(saved_id));
            }
            if let Some(snapshot) = load_row_progress_snapshot(row_id) {
                set_progress.set(Some(snapshot));
            }
        }
    });

    Effect::new(move || {
        if let Some(sid) = session_id.get() {
            let sid_clone = sid.clone();
            spawn_local(async move {
                loop {
                    match api::get_progress(&sid_clone).await {
                        Ok(prog) => {
                            save_row_progress_snapshot(row_id, &prog);
                            let finished = is_finished(&prog);
                            set_progress.set(Some(prog));
                            if finished {
                                clear_row_session(row_id);
                                set_session_id.set(None);
                                break;
                            }
                        }
                        Err(e) => {
                            if e.contains("404") {
                                clear_row_storage(row_id);
                                set_session_id.set(None);
                                set_progress.set(None);
                            } else {
                                set_error_msg.set(format!("Ошибка получения прогресса: {}", e));
                            }
                            break;
                        }
                    }
                    gloo_timers::future::TimeoutFuture::new(2000).await;
                }
            });
        }
    });

    let on_start = move |_| {
        let connection_id = selected_connection.get();
        if connection_id.is_empty() {
            set_error_msg.set("Сначала выберите подключение к Yandex Market".to_string());
            return;
        }

        set_is_starting.set(true);
        set_error_msg.set(String::new());
        set_progress.set(None);
        clear_row_storage(row_id);

        let date_from_text = date_from.get();
        let date_to_text = date_to.get();
        spawn_local(async move {
            let parsed_date_from = if needs_period {
                match NaiveDate::parse_from_str(&date_from_text, "%Y-%m-%d") {
                    Ok(v) => v,
                    Err(_) => {
                        set_error_msg.set("Неверный формат date_from".to_string());
                        set_is_starting.set(false);
                        return;
                    }
                }
            } else {
                default_date_from
            };

            let parsed_date_to = if needs_period {
                match NaiveDate::parse_from_str(&date_to_text, "%Y-%m-%d") {
                    Ok(v) => v,
                    Err(_) => {
                        set_error_msg.set("Неверный формат date_to".to_string());
                        set_is_starting.set(false);
                        return;
                    }
                }
            } else {
                default_date_to
            };

            let request = ImportRequest {
                connection_id,
                target_aggregates: vec![aggregate.to_string()],
                date_from: parsed_date_from,
                date_to: parsed_date_to,
                mode: ImportMode::Interactive,
                // Ручной импорт = полный бэкфилл за период. Перебор магазинов бизнеса
                // (если задан БизнесАккаунтID) выполняется автоматически на бэкенде.
                incremental_by_update: false,
            };

            match api::start_import(request).await {
                Ok(response) => {
                    save_row_session_id(row_id, &response.session_id);
                    set_session_id.set(Some(response.session_id));
                    set_is_starting.set(false);
                }
                Err(e) => {
                    set_error_msg.set(format!("Ошибка запуска: {}", e));
                    set_is_starting.set(false);
                }
            }
        });
    };

    let row_agg = move || {
        progress.get().and_then(|p| {
            let status = p.status;
            p.aggregates
                .into_iter()
                .find(|agg| agg.aggregate_index == aggregate)
                .map(|agg| (status, agg))
        })
    };

    view! {
        <Card>
            <div class="doc-filters__row">
                <Button
                    appearance=ButtonAppearance::Primary
                    on_click=on_start
                    disabled=move || {
                        selected_connection.get().is_empty() || is_starting.get() || session_id.get().is_some()
                    }
                >
                    {move || if is_starting.get() {
                        "Запуск..."
                    } else if session_id.get().is_some() {
                        "В работе"
                    } else {
                        "Запустить"
                    }}
                </Button>

                <div class="doc-filter" style="flex-direction: column; align-items: flex-start; gap: 2px; min-width: 220px;">
                    <span style="font-size: var(--font-size-base); font-weight: 600;">{title}</span>
                    <span style="font-size: var(--font-size-sm); color: var(--color-text-secondary);">{description}</span>
                    <div style="font-size: var(--font-size-sm); color: var(--color-text-tertiary); font-family: monospace;">
                        {aggregate}
                    </div>
                </div>

                <Flex vertical=true gap=FlexGap::Small>
                {move || if needs_period {
                    view! {
                        <div class="doc-filter">
                            <label class="doc-filter__label">"Период:"</label>
                            <input
                                type="date"
                                class="doc-filter__input"
                                prop:value=move || date_from.get()
                                on:change=move |ev| set_date_from.set(event_target_value(&ev))
                            />
                            <span>"—"</span>
                            <input
                                type="date"
                                class="doc-filter__input"
                                prop:value=move || date_to.get()
                                on:change=move |ev| set_date_to.set(event_target_value(&ev))
                            />
                        </div>
                    }.into_any()
                } else {
                    view! { <></> }.into_any()
                }}

                {move || {
                    if let Some((status, agg)) = row_agg() {
                        let total = agg.total.unwrap_or(0);
                        let percent = if total > 0 {
                            ((agg.processed as f64 / total as f64) * 100.0).clamp(0.0, 100.0) as i32
                        } else if agg.processed > 0 {
                            100
                        } else {
                            0
                        };
                        let current_info = agg.current_item.unwrap_or_else(|| "-".to_string());

                        view! {
                            <div style="display: flex; align-items: center; gap: 10px; flex: 1; min-width: 0;">
                                <span style="font-size: var(--font-size-sm); color: var(--color-text-secondary); min-width: 90px;">
                                    {format!("{:?}", status)}
                                </span>
                                <div style="height: 16px; border-radius: var(--radius-sm); overflow: hidden; background: var(--color-border); flex: 1; min-width: 200px;">
                                    <div style={format!("width: {}%; height: 100%; background: var(--colorBrandForeground1); transition: width 0.2s;", percent)}></div>
                                </div>
                                <span style="font-size: var(--font-size-sm); color: var(--color-text-secondary); min-width: 85px; text-align: right;">
                                    {if total > 0 { format!("{} / {}", agg.processed, total) } else { format!("{}", agg.processed) }}
                                </span>
                                <span style="font-size: var(--font-size-sm); color: var(--color-text-secondary); min-width: 165px;">
                                    {format!("ins: {}  upd: {}  err:{}", agg.inserted, agg.updated, agg.errors)}
                                </span>
                                <span style="font-size: var(--font-size-sm); color: var(--color-text-secondary); min-width: 220px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;">
                                    {format!("item: {}", current_info)}
                                </span>
                            </div>
                        }.into_any()
                    } else if let Some(p) = progress.get() {
                        view! {
                            <span style="font-size: var(--font-size-sm); color: var(--color-text-secondary);">
                                {format!("status: {:?}, processed: {}, errors: {}", p.status, p.total_processed, p.total_errors)}
                            </span>
                        }.into_any()
                    } else {
                        view! {
                            <span style="font-size: var(--font-size-sm); color: var(--color-text-secondary);">
                                "Готово к запуску"
                            </span>
                        }.into_any()
                    }
                }}
                </Flex>
            </div>

            {move || {
                let mut messages = Vec::new();
                let request_error = error_msg.get();
                if !request_error.is_empty() {
                    messages.push(request_error);
                }
                if let Some(snapshot) = progress.get() {
                    messages.extend(snapshot.errors.into_iter().map(|error| error.message));
                }
                messages.sort();
                messages.dedup();
                let err = messages.join("\n");
                if !err.is_empty() {
                    view! {
                        <div style="white-space: pre-wrap; margin-top: 8px; padding: 8px 12px; border-radius: var(--radius-md); border-left: 3px solid var(--color-error); background: var(--color-error-50); font-size: var(--font-size-sm);">
                            {err}
                        </div>
                    }.into_any()
                } else {
                    view! { <></> }.into_any()
                }
            }}
        </Card>
    }
}

#[component]
pub fn ImportWidget() -> impl IntoView {
    let (connections, set_connections) = signal(Vec::<ConnectionMP>::new());
    let (selected_connection, set_selected_connection) = signal(String::new());
    let (error_msg, set_error_msg) = signal(String::new());

    Effect::new(move || {
        spawn_local(async move {
            match api::get_marketplaces().await {
                Ok(marketplaces) => {
                    let marketplace_type_map: HashMap<String, Option<MarketplaceType>> =
                        marketplaces
                            .into_iter()
                            .map(|mp| (mp.base.id.as_string(), mp.marketplace_type))
                            .collect();

                    match api::get_connections().await {
                        Ok(conns) => {
                            let filtered: Vec<ConnectionMP> = conns
                                .into_iter()
                                .filter(|conn| {
                                    marketplace_type_map
                                        .get(&conn.marketplace_id)
                                        .and_then(|t| t.as_ref())
                                        .map(|t| *t == MarketplaceType::YandexMarket)
                                        .unwrap_or(false)
                                })
                                .collect();

                            if let Some(first) = filtered.first() {
                                set_selected_connection.set(first.to_string_id());
                            }
                            set_connections.set(filtered);
                        }
                        Err(e) => {
                            set_error_msg.set(format!("Ошибка загрузки подключений: {}", e));
                        }
                    }
                }
                Err(e) => {
                    set_error_msg.set(format!("Ошибка загрузки маркетплейсов: {}", e));
                }
            }
        });
    });

    view! {
        <PageFrame page_id="u503_import_from_yandex--usecase" category="usecase">
            <div class="page__header">
                <div class="page__header-left">
                    <h1 class="page__title">{"u503: Импорт Yandex Market"}</h1>
                </div>
            </div>

            {move || {
                let err = error_msg.get();
                if !err.is_empty() {
                    view! {
                        <div style="padding: 12px 16px; border-radius: var(--radius-md); border-left: 3px solid var(--color-error); background: var(--color-error-50); margin-bottom: 16px; font-size: var(--font-size-base);">
                            {err}
                        </div>
                    }.into_any()
                } else {
                    view! { <></> }.into_any()
                }
            }}

            <div style="display: flex; flex-direction: column; gap: 16px; margin-top: 16px;">
                <Flex vertical=false gap=FlexGap::Large justify=FlexJustify::Center align=FlexAlign::Center>
                    <label class="form__label">"Подключение Yandex Market"</label>
                    <select
                        class="doc-filter__select"
                        style="width: 100%; max-width: 500px;"
                        on:change=move |ev| set_selected_connection.set(event_target_value(&ev))
                    >
                        <option value="">"— выберите подключение —"</option>
                        {move || connections.get().into_iter().map(|conn| {
                            let id = conn.to_string_id();
                            let selected = id == selected_connection.get();
                            let caption = conn.base.description.clone();
                            view! { <option selected=selected value={id}>{caption}</option> }
                        }).collect_view()}
                    </select>
                </Flex>

                <div style="display: flex; flex-direction: column; gap: 8px; margin-left:16px; margin-right:16px;">
                    <ServiceRow
                        row_id="a007"
                        title="Товары маркетплейса"
                        description="a007_marketplace_product"
                        aggregate="a007_marketplace_product"
                        selected_connection=selected_connection
                    />
                    <ServiceRow
                        row_id="a013"
                        title="Заказы Yandex Market"
                        description="a013_ym_order"
                        aggregate="a013_ym_order"
                        needs_period=true
                        selected_connection=selected_connection
                    />
                    <ServiceRow
                        row_id="a016"
                        title="Возвраты и невыкупы"
                        description="a016_ym_returns"
                        aggregate="a016_ym_returns"
                        needs_period=true
                        selected_connection=selected_connection
                    />
                    <ServiceRow
                        row_id="p907"
                        title="Отчёт по платежам"
                        description="p907_ym_payment_report"
                        aggregate="p907_ym_payment_report"
                        needs_period=true
                        selected_connection=selected_connection
                    />
                    <ServiceRow
                        row_id="a034"
                        title="Отчёт о реализации (слой ybuh)"
                        description="a034_ym_realization"
                        aggregate="a034_ym_realization"
                        needs_period=true
                        selected_connection=selected_connection
                    />
                    <ServiceRow
                        row_id="a041"
                        title="Воронка продаж (Аналитика продаж)"
                        description="Показы, клики, корзины и заказы по товарам"
                        aggregate="a041_ym_shows_sales_daily"
                        needs_period=true
                        selected_connection=selected_connection
                    />
                </div>
            </div>
        </PageFrame>
    }
}
