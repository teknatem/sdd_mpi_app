use super::api;
use crate::shared::page_frame::PageFrame;
use crate::system::tasks::ui::TaskProgressPanel;
use chrono::{Duration, NaiveDate, Utc};
use contracts::domain::a006_connection_mp::aggregate::ConnectionMP;
use contracts::domain::common::AggregateId;
use contracts::enums::marketplace_type::MarketplaceType;
use contracts::system::tasks::import_progress_map::task_progress_response_from_u504;
use contracts::usecases::u504_import_from_wildberries::{
    progress::{AggregateImportStatus, ImportProgress, ImportStatus},
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
    format!("u504_row_{}_session_id", row_id)
}

fn row_progress_key(row_id: &str) -> String {
    format!("u504_row_{}_progress", row_id)
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

fn is_finished(progress: &ImportProgress) -> bool {
    matches!(
        progress.status,
        ImportStatus::Completed
            | ImportStatus::CompletedWithErrors
            | ImportStatus::Failed
            | ImportStatus::Cancelled
    )
}

fn find_aggregate_progress(
    progress: &ImportProgress,
    aggregate: &str,
) -> Option<contracts::usecases::u504_import_from_wildberries::progress::AggregateProgress> {
    progress
        .aggregates
        .iter()
        .find(|agg| agg.aggregate_index == aggregate)
        .cloned()
}

#[derive(Clone, Copy)]
enum RowDisplayStatus {
    Idle,
    Starting,
    Pending,
    Running,
    Completed,
    CompletedWithErrors,
    Failed,
    Cancelled,
}

fn derive_row_status(
    progress: Option<&ImportProgress>,
    aggregate: &str,
    is_starting: bool,
) -> RowDisplayStatus {
    if is_starting {
        return RowDisplayStatus::Starting;
    }

    let Some(progress) = progress else {
        return RowDisplayStatus::Idle;
    };

    if let Some(agg) = find_aggregate_progress(progress, aggregate) {
        return match agg.status {
            AggregateImportStatus::Pending => RowDisplayStatus::Pending,
            AggregateImportStatus::Running => RowDisplayStatus::Running,
            AggregateImportStatus::Completed => {
                if agg.errors > 0 {
                    RowDisplayStatus::CompletedWithErrors
                } else {
                    RowDisplayStatus::Completed
                }
            }
            AggregateImportStatus::Failed => RowDisplayStatus::Failed,
        };
    }

    match progress.status {
        ImportStatus::Running => RowDisplayStatus::Running,
        ImportStatus::Completed => RowDisplayStatus::Completed,
        ImportStatus::CompletedWithErrors => RowDisplayStatus::CompletedWithErrors,
        ImportStatus::Failed => RowDisplayStatus::Failed,
        ImportStatus::Cancelled => RowDisplayStatus::Cancelled,
    }
}

fn row_status_label(status: RowDisplayStatus) -> &'static str {
    match status {
        RowDisplayStatus::Idle => "Не запущено",
        RowDisplayStatus::Starting => "Запуск...",
        RowDisplayStatus::Pending => "Ожидает",
        RowDisplayStatus::Running => "В работе",
        RowDisplayStatus::Completed => "Завершено",
        RowDisplayStatus::CompletedWithErrors => "Есть ошибки",
        RowDisplayStatus::Failed => "Ошибка",
        RowDisplayStatus::Cancelled => "Отменено",
    }
}

fn row_status_style(status: RowDisplayStatus) -> &'static str {
    match status {
        RowDisplayStatus::Idle => {
            "display: inline-flex; align-items: center; min-height: 28px; padding: 0 10px; border-radius: 999px; background: var(--colorNeutralBackground3); color: var(--color-text-secondary); font-size: var(--font-size-sm); font-weight: 600;"
        }
        RowDisplayStatus::Starting | RowDisplayStatus::Pending | RowDisplayStatus::Running => {
            "display: inline-flex; align-items: center; min-height: 28px; padding: 0 10px; border-radius: 999px; background: var(--colorBrandBackground2); color: var(--colorBrandForeground1); font-size: var(--font-size-sm); font-weight: 600;"
        }
        RowDisplayStatus::Completed => {
            "display: inline-flex; align-items: center; min-height: 28px; padding: 0 10px; border-radius: 999px; background: var(--colorSuccessBackground2); color: var(--colorSuccessForeground1); font-size: var(--font-size-sm); font-weight: 600;"
        }
        RowDisplayStatus::CompletedWithErrors => {
            "display: inline-flex; align-items: center; min-height: 28px; padding: 0 10px; border-radius: 999px; background: var(--colorPaletteYellowBackground2); color: var(--colorPaletteDarkOrangeForeground2); font-size: var(--font-size-sm); font-weight: 600;"
        }
        RowDisplayStatus::Failed | RowDisplayStatus::Cancelled => {
            "display: inline-flex; align-items: center; min-height: 28px; padding: 0 10px; border-radius: 999px; background: var(--colorPaletteRedBackground2); color: var(--color-error); font-size: var(--font-size-sm); font-weight: 600;"
        }
    }
}

#[component]
fn TaskSection(title: &'static str, subtitle: &'static str, children: Children) -> impl IntoView {
    view! {
        <section style="display: flex; flex-direction: column; gap: 10px;">
            <div style="display: flex; flex-direction: column; gap: 2px; padding: 0 4px;">
                <h2 style="margin: 0; font-size: 18px; font-weight: 700; color: var(--color-text);">
                    {title}
                </h2>
                <div style="font-size: var(--font-size-sm); color: var(--color-text-secondary);">
                    {subtitle}
                </div>
            </div>
            <div style="display: flex; flex-direction: column; gap: 10px;">
                {children()}
            </div>
        </section>
    }
}

#[component]
fn ServiceRow(
    row_id: &'static str,
    title: &'static str,
    details_text: &'static str,
    aggregate: &'static str,
    #[prop(default = false)] needs_period: bool,
    #[prop(default = false)] show_backfill_note: bool,
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
                                clear_row_storage(row_id);
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
            set_error_msg.set("Сначала выберите подключение к Wildberries".to_string());
            return;
        }

        set_is_starting.set(true);
        set_error_msg.set(String::new());
        set_progress.set(None);

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

    view! {
        <Card>
            <div style="display: flex; flex-direction: column; gap: 10px;">
                <div style="display: flex; align-items: flex-start; justify-content: space-between; gap: 14px; padding: 4px 0 0 0; background: linear-gradient(90deg, var(--color-subtle-background) 0%, transparent 100%);">
                    <div style="display: flex; align-items: flex-start; gap: 10px; min-width: 0;">
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
                        <div style="display: flex; flex-direction: column; gap: 1px; min-width: 0;">
                            <div style="font-size: var(--font-size-base); font-weight: 700; color: var(--color-text);">
                                {title}
                            </div>
                            <div style="font-size: 12px; color: var(--color-text-secondary);">
                                {if needs_period {
                                    "Запуск по выбранному периоду"
                                } else {
                                    "Разовый запуск без выбора периода"
                                }}
                            </div>
                        </div>
                    </div>
                    <div style="display: flex; align-items: center; gap: 8px; flex-wrap: wrap; justify-content: flex-end;">
                        {move || {
                            let current_progress = progress.get();
                            let status = derive_row_status(
                                current_progress.as_ref(),
                                aggregate,
                                is_starting.get(),
                            );
                            view! {
                                <span style={row_status_style(status)}>
                                    {row_status_label(status)}
                                </span>
                            }
                        }}
                    </div>
                </div>

                {move || {
                    let has_progress = progress.get().is_some();
                    if needs_period || has_progress {
                        view! {
                            <div style="display: grid; grid-template-columns: repeat(auto-fit, minmax(280px, 1fr)); gap: 10px;">
                                {move || if needs_period {
                                    view! {
                                        <div style="display: flex; flex-direction: column; gap: 6px; padding: 2px 0;">
                                            <div style="font-size: 12px; font-weight: 700; letter-spacing: 0.04em; text-transform: uppercase; color: var(--color-text-tertiary);">
                                                "Период импорта"
                                            </div>
                                            <div style="display: flex; align-items: center; gap: 10px; white-space: nowrap;">
                                                <input
                                                    type="date"
                                                    class="doc-filter__input"
                                                    style="width: 132px;"
                                                    prop:value=move || date_from.get()
                                                    on:change=move |ev| set_date_from.set(event_target_value(&ev))
                                                />
                                                <span style="color: var(--color-text-secondary);">"—"</span>
                                                <input
                                                    type="date"
                                                    class="doc-filter__input"
                                                    style="width: 132px;"
                                                    prop:value=move || date_to.get()
                                                    on:change=move |ev| set_date_to.set(event_target_value(&ev))
                                                />
                                            </div>
                                        </div>
                                    }.into_any()
                                } else {
                                    view! { <></> }.into_any()
                                }}
                                {move || if let Some(prog) = progress.get() {
                                    let filter = find_aggregate_progress(&prog, aggregate)
                                        .map(|_| aggregate);
                                    let resp = task_progress_response_from_u504(&prog, filter);
                                    view! {
                                        <TaskProgressPanel
                                            progress=resp
                                            section_title="Прогресс".to_string()
                                            running_title="Синхронизация".to_string()
                                        />
                                    }.into_any()
                                } else {
                                    view! { <></> }.into_any()
                                }}
                            </div>
                        }.into_any()
                    } else {
                        view! { <></> }.into_any()
                    }
                }}

                {move || {
                    let mut has_messages = false;
                    if !error_msg.get().is_empty() || show_backfill_note {
                        has_messages = true;
                    }
                    if let Some(prog) = progress.get() {
                        if let Some(agg) = find_aggregate_progress(&prog, aggregate) {
                            if agg.current_item.as_ref().map(|item| !item.trim().is_empty()).unwrap_or(false) {
                                has_messages = true;
                            }
                        }
                    }

                    if has_messages {
                        view! {
                            <div style="display: flex; flex-direction: column; gap: 4px;">
                                {move || {
                                    if let Some(prog) = progress.get() {
                                        if let Some(agg) = find_aggregate_progress(&prog, aggregate) {
                                            if let Some(current_item) = agg.current_item {
                                                if !current_item.trim().is_empty() {
                                                    return view! {
                                                        <div style="padding: 2px 0; font-size: var(--font-size-sm); color: var(--color-text-secondary); overflow: hidden; text-overflow: ellipsis; white-space: nowrap;">
                                                            {format!("Текущий элемент: {}", current_item)}
                                                        </div>
                                                    }.into_any();
                                                }
                                            }
                                        }
                                    }
                                    view! { <></> }.into_any()
                                }}
                                {move || {
                                    let err = error_msg.get();
                                    if !err.is_empty() {
                                        view! {
                                            <div style="padding: 2px 0; color: var(--color-error); font-size: var(--font-size-sm);">
                                                {err}
                                            </div>
                                        }.into_any()
                                    } else {
                                        view! { <></> }.into_any()
                                    }
                                }}
                                {move || if show_backfill_note {
                                    view! {
                                        <div style="padding: 2px 0; font-size: var(--font-size-sm); color: var(--color-text-tertiary); font-style: italic;">
                                            "Backfill: cursor lastChangeDate (flag=0), date_to как soft-stop."
                                        </div>
                                    }.into_any()
                                } else {
                                    view! { <></> }.into_any()
                                }}
                            </div>
                        }.into_any()
                    } else {
                        view! { <></> }.into_any()
                    }
                }}

                <details>
                    <summary style="cursor: pointer; user-select: none; color: var(--colorBrandForeground1); font-weight: 600;">
                        "Технические детали"
                    </summary>
                    <div style="margin-top: 4px; display: flex; flex-direction: column; gap: 4px; font-size: var(--font-size-sm); color: var(--color-text-secondary);">
                        <div style="line-height: 1.4;">
                            {details_text}
                        </div>
                        <div style="font-family: monospace; color: var(--color-text-tertiary); font-size: 12px;">
                            {format!("aggregate: {}", aggregate)}
                        </div>
                    </div>
                </details>
            </div>
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
                                        .map(|t| *t == MarketplaceType::Wildberries)
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
        <PageFrame page_id="u504_import_from_wildberries--usecase" category="usecase">
            <div class="page__header">
                <div class="page__header-left">
                    <h1 class="page__title">{"Загрузка данных WB"}</h1>
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

            <div style="display: flex; flex-direction: column; gap: 18px; margin-top: 16px;">
                <Card>
                    <div style="display: flex; flex-direction: column; gap: 10px;">
                        <div style="font-size: var(--font-size-base); font-weight: 700; color: var(--color-text);">
                            "Подключение Wildberries"
                        </div>
                        <div style="font-size: var(--font-size-sm); color: var(--color-text-secondary);">
                            "Сначала выберите кабинет, затем запускайте нужные операции по секциям ниже."
                        </div>
                        <select
                            class="doc-filter__select"
                            style="width: 100%; max-width: 520px;"
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
                    </div>
                </Card>

                <div style="display: flex; flex-direction: column; gap: 18px; margin-left:16px; margin-right:16px;">
                    <TaskSection
                        title="Каталог и цены"
                        subtitle="Товары, цены и акции по ассортименту Wildberries."
                    >
                        <ServiceRow
                            row_id="a007"
                            title="Товары WB: каталог"
                            details_text="Загружает карточки товаров продавца из кабинета Wildberries: номенклатурные идентификаторы WB, артикулы, баркоды и другие данные, которые нужны для сопоставления ассортимента внутри системы."
                            aggregate="a007_marketplace_product"
                            selected_connection=selected_connection
                        />
                        <ServiceRow
                            row_id="p908"
                            title="Товары WB: цены"
                            details_text="Загружает актуальные цены и скидки по товарам Wildberries, чтобы в системе были свежие значения по каждой позиции ассортимента и можно было анализировать текущие условия продаж."
                            aggregate="p908_wb_goods_prices"
                            selected_connection=selected_connection
                        />
                        <ServiceRow
                            row_id="a020"
                            title="Товары WB: акции"
                            details_text="Загружает список активных и запланированных акций Wildberries вместе с привязанными товарами, чтобы видеть, какие позиции участвуют в промо и на какой период запланированы изменения."
                            aggregate="a020_wb_promotion"
                            needs_period=true
                            selected_connection=selected_connection
                        />
                    </TaskSection>

                    <TaskSection
                        title="Заказы и логистика"
                        subtitle="Новые заказы, историческая загрузка и FBS-поставки."
                    >
                        <ServiceRow
                            row_id="a015_new"
                            title="Заказы WB: новые"
                            details_text="Загружает новые заказы вскоре после оформления в кабинете Wildberries. Этот режим нужен для оперативной работы с заказами, когда важна скорость появления данных, даже если часть финансовых полей еще недоступна."
                            aggregate="a015_wb_orders_new"
                            needs_period=true
                            selected_connection=selected_connection
                        />
                        <ServiceRow
                            row_id="a015"
                            title="Заказы WB: история"
                            details_text="Загружает исторические заказы Wildberries с более полным набором данных: ценами, скидками и дополнительными реквизитами. Этот режим подходит для выравнивания и дозагрузки данных за период."
                            aggregate="a015_wb_orders"
                            needs_period=true
                            show_backfill_note=true
                            selected_connection=selected_connection
                        />
                        <ServiceRow
                            row_id="a029"
                            title="Поставки WB (FBS)"
                            details_text="Загружает поставки FBS за выбранный период, актуализирует связь уже известных заказов с этими поставками и сохраняет текущий состав поставки без затирания существующих данных. Во время загрузки дополнительно запрашиваются номера стикеров по заказам поставки и сохраняются в составе поставки для колонки «Стикер A-B»."
                            aggregate="a029_wb_supply"
                            needs_period=true
                            selected_connection=selected_connection
                        />
                    </TaskSection>

                    <TaskSection
                        title="Финансы и возвраты"
                        subtitle="Продажи, заявки на возврат, финансовый отчет и актуальные комиссии."
                    >
                        <ServiceRow
                            row_id="a012"
                            title="Продажи WB"
                            details_text="Загружает продажи и возвраты Wildberries с финансовыми полями, чтобы в системе появились данные для выручки, скидок, комиссий и последующего расчета финансового результата."
                            aggregate="a012_wb_sales"
                            needs_period=true
                            selected_connection=selected_connection
                        />
                        <ServiceRow
                            row_id="a032"
                            title="Заявки на возврат WB"
                            details_text="Загружает заявки покупателей на возврат товара из returns-api.wildberries.ru/api/v1/claims. API возвращает данные только за последние 14 дней — без выбора периода. Опрашиваются активные и архивные заявки. Upsert по (claim_id, connection_id). Требует токена с категорией «Buyers Returns»."
                            aggregate="a032_wb_returns_claims"
                            selected_connection=selected_connection
                        />
                        <ServiceRow
                            row_id="p903"
                            title="Финансы WB: отчет"
                            details_text="Загружает детализированный финансовый отчет Wildberries по выбранному периоду: строки реализации, удержания и прочие финансовые показатели, которые нужны для глубокой сверки расчетов."
                            aggregate="p903_wb_finance_report"
                            needs_period=true
                            selected_connection=selected_connection
                        />
                        <ServiceRow
                            row_id="a043"
                            title="Финансы WB: новый Finance API v1"
                            details_text="Загружает список ежедневных отчётов реализации и полную детализацию каждого reportId в независимый агрегат a043. Не изменяет legacy p903, проводки и сверки. API доступен с 2025-01-01 и ограничен одним запросом в минуту."
                            aggregate="a043_wb_finance_report"
                            needs_period=true
                            selected_connection=selected_connection
                        />
                        <ServiceRow
                            row_id="p905"
                            title="Финансы WB: комиссии"
                            details_text="Загружает актуальные ставки комиссий Wildberries по категориям, чтобы система могла использовать свежие правила расчета комиссионной нагрузки по ассортименту."
                            aggregate="p905_wb_commission_history"
                            selected_connection=selected_connection
                        />
                    </TaskSection>

                    <TaskSection
                        title="Документы и маркетинг"
                        subtitle="Входящие документы и статистика рекламных кампаний."
                    >
                        <ServiceRow
                            row_id="a027"
                            title="Документы WB"
                            details_text="Загружает входящие документы Wildberries, например акты и накладные, чтобы в системе был единый список доступных документов по кабинету за выбранный период."
                            aggregate="a027_wb_documents"
                            needs_period=true
                            selected_connection=selected_connection
                        />
                        <ServiceRow
                            row_id="wb_advert_csv"
                            title="Реклама WB: статистика"
                            details_text="Загружает дневную статистику рекламных кампаний Wildberries в документы a026 (по дате и advert_id), с проводками и проекциями. Промежуточный CSV в пайплайне не создаётся."
                            aggregate="wb_advert_stats"
                            needs_period=true
                            selected_connection=selected_connection
                        />
                        <ServiceRow
                            row_id="a036"
                            title="Воронка продаж WB"
                            details_text="Загружает дневную воронку продаж Wildberries в разрезе номенклатуры (переходы, корзины, заказы, выкупы, конверсии) в документы a036. Данные доступны примерно за последнюю неделю; один документ на день и кабинет."
                            aggregate="a036_wb_sales_funnel_daily"
                            needs_period=true
                            selected_connection=selected_connection
                        />
                        <ServiceRow
                            row_id="a036_history"
                            title="Воронка продаж WB: история CSV"
                            details_text="Создаёт асинхронный отчёт DETAIL_HISTORY_REPORT, ожидает готовность и загружает дневную историю в документы a036 за выбранный период. Требуется подписка «Джем». Существующая оперативная загрузка воронки не изменяется."
                            aggregate="a036_wb_sales_funnel_daily_history"
                            needs_period=true
                            selected_connection=selected_connection
                        />
                        <ServiceRow
                            row_id="a037"
                            title="Данные по товарам WB"
                            details_text="Собирает текущие остатки (склады WB и продавца) и рейтинги товаров Wildberries в документ a037 за сегодняшнюю дату. Данные фиксируются только вперёд — для динамики по дням запускайте ежедневно (или включите задачу task020)."
                            aggregate="a037_wb_product_snapshot"
                            needs_period=true
                            selected_connection=selected_connection
                        />
                        <ServiceRow
                            row_id="a040"
                            title="Поисковая аналитика WB (видимость)"
                            details_text="Собирает поисковую аналитику Wildberries в разрезе номенклатуры (видимость товара в выдаче в %, переходы из поиска, средняя позиция в выдаче + топ поисковых запросов) в документ a040 за дату. WB отдаёт только видимость (%), а не счётчик показов — a040 НЕ питает воронку p916 (органические показы там остаются N/A). Требует подписки «Джем» — при отсутствии доступа сбор завершится без данных. Данные фиксируются только вперёд."
                            aggregate="a040_wb_search_analytics_daily"
                            needs_period=true
                            selected_connection=selected_connection
                        />
                    </TaskSection>
                </div>
            </div>
        </PageFrame>
    }
}
