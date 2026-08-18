pub mod state;

use self::state::create_state;
use crate::layout::global_context::AppGlobalContext;
use crate::shared::api_utils::api_base;
use crate::shared::components::close_page_button::ClosePageButton;
use crate::shared::components::date_range_picker::DateRangePicker;
use crate::shared::components::pagination_controls::PaginationControls;
use crate::shared::components::table::TableCrosshairHighlight;
use crate::shared::icons::icon;
use crate::shared::list_utils::{get_sort_class, get_sort_indicator};
use crate::shared::page_frame::PageFrame;
use crate::shared::table_utils::init_column_resize;
use crate::usecases::common::{client, ImportUseCase};
use contracts::domain::a006_connection_mp::aggregate::ConnectionMP;
use contracts::domain::common::AggregateId;
use gloo_net::http::Request;
use leptos::prelude::*;
use leptos::task::spawn_local;
use serde::{Deserialize, Serialize};
use thaw::*;

fn format_date(iso_date: &str) -> String {
    if let Some(date_part) = iso_date.split('T').next() {
        if let Some((year, rest)) = date_part.split_once('-') {
            if let Some((month, day)) = rest.split_once('-') {
                return format!("{}.{}.{}", day, month, year);
            }
        }
    }
    iso_date.to_string()
}

fn format_datetime(iso_date: &str) -> String {
    if let Some((date, time)) = iso_date.split_once('T') {
        let time_clean = time
            .split('Z')
            .next()
            .unwrap_or(time)
            .split('+')
            .next()
            .unwrap_or(time);
        let hms = time_clean.split('.').next().unwrap_or(time_clean);
        return format!("{} {}", format_date(date), hms);
    }
    format_date(iso_date)
}

fn format_money(value: f64) -> String {
    let formatted = format!("{:.2}", value);
    let parts: Vec<&str> = formatted.split('.').collect();
    let integer_part = parts[0];
    let decimal_part = parts.get(1).copied().unwrap_or("00");

    let mut result = String::new();
    let chars: Vec<char> = integer_part.chars().rev().collect();

    for (index, ch) in chars.iter().enumerate() {
        if index > 0 && index % 3 == 0 && *ch != '-' {
            result.push(' ');
        }
        result.push(*ch);
    }

    format!(
        "{}.{}",
        result.chars().rev().collect::<String>(),
        decimal_part
    )
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WbProductSnapshotListDto {
    pub id: String,
    pub document_no: String,
    pub document_date: String,
    pub lines_count: i32,
    pub total_stock_wb: i64,
    pub total_stock_mp: i64,
    pub total_balance_sum: f64,
    pub connection_id: String,
    pub connection_name: Option<String>,
    pub organization_name: Option<String>,
    pub fetched_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaginatedResponse {
    pub items: Vec<serde_json::Value>,
    pub total: usize,
    pub page: usize,
    pub page_size: usize,
    pub total_pages: usize,
}

const TABLE_ID: &str = "a037-wb-product-snapshot-table";
const COLUMN_WIDTHS_KEY: &str = "a037_wb_product_snapshot_column_widths";

#[component]
pub fn WbProductSnapshotList() -> impl IntoView {
    let tabs_store =
        leptos::context::use_context::<AppGlobalContext>().expect("AppGlobalContext not found");
    let state = create_state();
    let (loading, set_loading) = signal(false);
    let (error, set_error) = signal::<Option<String>>(None);
    let (is_filter_expanded, set_is_filter_expanded) = signal(false);
    let (connections, set_connections) = signal::<Vec<ConnectionMP>>(Vec::new());
    let (import_running, set_import_running) = signal(false);
    let (import_status, set_import_status) = signal::<Option<String>>(None);

    let open_detail = move |id: String, document_date: String| {
        let title = format!("Товары WB {}", document_date);
        tabs_store.open_tab(&format!("a037_wb_product_snapshot_details_{}", id), &title);
    };

    let load_items = move || {
        spawn_local(async move {
            set_loading.set(true);
            set_error.set(None);

            let date_from_val = state.with_untracked(|s| s.date_from.clone());
            let date_to_val = state.with_untracked(|s| s.date_to.clone());
            let connection_id_val = state.with_untracked(|s| s.selected_connection_id.clone());
            let search_query_val = state.with_untracked(|s| s.search_query.clone());
            let page = state.with_untracked(|s| s.page);
            let page_size = state.with_untracked(|s| s.page_size);
            let sort_field = state.with_untracked(|s| s.sort_field.clone());
            let sort_ascending = state.with_untracked(|s| s.sort_ascending);
            let offset = page * page_size;
            let cache_buster = js_sys::Date::now() as i64;

            let mut url = format!(
                "{}/api/a037/wb-product-snapshot/list?date_from={}&date_to={}&limit={}&offset={}&sort_by={}&sort_desc={}&_ts={}",
                api_base(),
                date_from_val,
                date_to_val,
                page_size,
                offset,
                sort_field,
                !sort_ascending,
                cache_buster
            );

            if !search_query_val.is_empty() {
                url.push_str(&format!(
                    "&search_query={}",
                    urlencoding::encode(&search_query_val)
                ));
            }
            if let Some(connection_id) = connection_id_val.filter(|value| !value.is_empty()) {
                url.push_str(&format!(
                    "&connection_id={}",
                    urlencoding::encode(&connection_id)
                ));
            }

            match Request::get(&url)
                .header("Cache-Control", "no-cache, no-store, must-revalidate")
                .header("Pragma", "no-cache")
                .send()
                .await
            {
                Ok(response) if response.ok() => match response.json::<PaginatedResponse>().await {
                    Ok(paginated) => {
                        let parsed: Vec<WbProductSnapshotListDto> = paginated
                            .items
                            .into_iter()
                            .filter_map(|v| {
                                Some(WbProductSnapshotListDto {
                                    id: v.get("id")?.as_str()?.to_string(),
                                    document_no: v.get("document_no")?.as_str()?.to_string(),
                                    document_date: v.get("document_date")?.as_str()?.to_string(),
                                    lines_count: v
                                        .get("lines_count")
                                        .and_then(|x| x.as_i64())
                                        .unwrap_or(0)
                                        as i32,
                                    total_stock_wb: v
                                        .get("total_stock_wb")
                                        .and_then(|x| x.as_i64())
                                        .unwrap_or(0),
                                    total_stock_mp: v
                                        .get("total_stock_mp")
                                        .and_then(|x| x.as_i64())
                                        .unwrap_or(0),
                                    total_balance_sum: v
                                        .get("total_balance_sum")
                                        .and_then(|x| x.as_f64())
                                        .unwrap_or(0.0),
                                    connection_id: v
                                        .get("connection_id")
                                        .and_then(|x| x.as_str())
                                        .unwrap_or("")
                                        .to_string(),
                                    connection_name: v
                                        .get("connection_name")
                                        .and_then(|x| x.as_str())
                                        .map(String::from),
                                    organization_name: v
                                        .get("organization_name")
                                        .and_then(|x| x.as_str())
                                        .map(String::from),
                                    fetched_at: v
                                        .get("fetched_at")
                                        .and_then(|x| x.as_str())
                                        .unwrap_or("")
                                        .to_string(),
                                })
                            })
                            .collect();

                        state.update(|s| {
                            s.items = parsed;
                            s.total_count = paginated.total;
                            s.total_pages = paginated.total_pages;
                            s.page = paginated.page;
                            s.page_size = paginated.page_size;
                            s.is_loaded = true;
                        });
                    }
                    Err(e) => set_error.set(Some(format!("Ошибка парсинга: {}", e))),
                },
                Ok(response) => {
                    set_error.set(Some(format!("Ошибка сервера: {}", response.status())));
                }
                Err(e) => {
                    set_error.set(Some(format!("Ошибка сети: {}", e)));
                }
            }

            set_loading.set(false);
        });
    };

    Effect::new(move |_| {
        if !state.with_untracked(|s| s.is_loaded) {
            load_items();
        }
    });

    Effect::new(move |_| {
        spawn_local(async move {
            match fetch_connections().await {
                Ok(mut items) => {
                    items.sort_by(|left, right| {
                        left.base
                            .description
                            .to_lowercase()
                            .cmp(&right.base.description.to_lowercase())
                    });
                    set_connections.set(items);
                }
                Err(err) => {
                    set_error.set(Some(err));
                }
            }
        });
    });

    let search_query = RwSignal::new(state.get_untracked().search_query.clone());
    Effect::new(move || {
        let value = search_query.get();
        untrack(move || {
            state.update(|s| s.search_query = value);
        });
    });

    let selected_connection_id = RwSignal::new(
        state
            .get_untracked()
            .selected_connection_id
            .clone()
            .unwrap_or_default(),
    );
    Effect::new(move || {
        let value = selected_connection_id.get();
        untrack(move || {
            state.update(|s| {
                s.selected_connection_id = if value.trim().is_empty() {
                    None
                } else {
                    Some(value)
                };
            });
        });
    });

    let resize_initialized = StoredValue::new(false);
    Effect::new(move |_| {
        if !resize_initialized.get_value() {
            resize_initialized.set_value(true);
            spawn_local(async move {
                gloo_timers::future::TimeoutFuture::new(100).await;
                init_column_resize(TABLE_ID, COLUMN_WIDTHS_KEY);
            });
        }
    });

    let active_filters_count = Signal::derive(move || {
        let s = state.get();
        let mut count = 0;
        if !s.date_from.is_empty() {
            count += 1;
        }
        if !s.date_to.is_empty() {
            count += 1;
        }
        if s.selected_connection_id
            .as_deref()
            .is_some_and(|value| !value.is_empty())
        {
            count += 1;
        }
        if !s.search_query.is_empty() {
            count += 1;
        }
        count
    });

    let toggle_sort = move |field: &'static str| {
        state.update(|s| {
            if s.sort_field == field {
                s.sort_ascending = !s.sort_ascending;
            } else {
                s.sort_field = field.to_string();
                s.sort_ascending = true;
            }
            s.page = 0;
        });
        load_items();
    };

    let go_to_page = move |new_page: usize| {
        state.update(|s| s.page = new_page);
        load_items();
    };

    let change_page_size = move |new_size: usize| {
        state.update(|s| {
            s.page_size = new_size;
            s.page = 0;
        });
        load_items();
    };

    // Импорт снимка через общий механизм u504.
    let start_import = move || {
        let connection_id = state
            .with_untracked(|s| s.selected_connection_id.clone())
            .filter(|value| !value.is_empty());
        let Some(connection_id) = connection_id else {
            set_error.set(Some("Для импорта выберите кабинет в фильтрах".to_string()));
            return;
        };
        // Окно активности для снимка — последние 7 дней; snapshot_date проставит бэкенд (сегодня).
        let today = chrono::Utc::now().date_naive();
        let date_from = today - chrono::Duration::days(6);

        set_error.set(None);
        set_import_running.set(true);
        set_import_status.set(Some("Запуск сбора…".to_string()));

        spawn_local(async move {
            let outcome = client::run_to_completion(
                ImportUseCase::Wildberries,
                connection_id,
                "a037_wb_product_snapshot",
                date_from,
                today,
                |progress| {
                    let current = progress.current_item.clone().unwrap_or_default();
                    set_import_status.set(Some(if current.is_empty() {
                        "Сбор выполняется…".to_string()
                    } else {
                        format!("Сбор: {}", current)
                    }));
                },
            )
            .await;

            match outcome {
                Ok(()) => set_import_status.set(Some("Сбор завершён".to_string())),
                Err(e) => {
                    set_import_status.set(None);
                    set_error.set(Some(format!("Сбор завершился с ошибками: {}", e)));
                }
            }

            set_import_running.set(false);
            load_items();
        });
    };

    view! {
        <PageFrame page_id="a037_wb_product_snapshot--list" category="list">
            <div class="page__header">
                <div class="page__header-left">
                    <h1 class="page__title">"Данные по товарам WB"</h1>
                    <Badge appearance=BadgeAppearance::Filled>
                        {move || state.get().total_count.to_string()}
                    </Badge>
                </div>
                <div class="page__header-right">
                    <ClosePageButton />
                </div>
            </div>

            <div class="page__content">
                <div class="filter-panel">
                    <div class="filter-panel-header">
                        <div
                            class="filter-panel-header__left"
                            on:click=move |_| set_is_filter_expanded.update(|e| *e = !*e)
                        >
                            <svg
                                width="16"
                                height="16"
                                viewBox="0 0 24 24"
                                fill="none"
                                stroke="currentColor"
                                stroke-width="2"
                                stroke-linecap="round"
                                stroke-linejoin="round"
                                class=move || {
                                    if is_filter_expanded.get() {
                                        "filter-panel__chevron filter-panel__chevron--expanded"
                                    } else {
                                        "filter-panel__chevron"
                                    }
                                }
                            >
                                <polyline points="6 9 12 15 18 9"></polyline>
                            </svg>
                            {icon("filter")}
                            <span class="filter-panel__title">"Фильтры"</span>
                            {move || {
                                let count = active_filters_count.get();
                                if count > 0 {
                                    view! { <span class="filter-panel__badge">{count}</span> }.into_any()
                                } else {
                                    view! { <></> }.into_any()
                                }
                            }}
                        </div>

                        <div class="filter-panel-header__center">
                            <PaginationControls
                                current_page=Signal::derive(move || state.get().page)
                                total_pages=Signal::derive(move || state.get().total_pages)
                                total_count=Signal::derive(move || state.get().total_count)
                                page_size=Signal::derive(move || state.get().page_size)
                                on_page_change=Callback::new(go_to_page)
                                on_page_size_change=Callback::new(change_page_size)
                                page_size_options=vec![50, 100, 200, 500]
                            />
                        </div>

                        <div class="filter-panel-header__right">
                            {move || import_status.get().map(|status| view! {
                                <span class="text-muted" style="white-space: nowrap;">{status}</span>
                            })}
                            <Button
                                appearance=ButtonAppearance::Secondary
                                on_click=move |_| start_import()
                                disabled=Signal::derive(move || import_running.get())
                            >
                                {icon("download")}
                                {move || {
                                    if import_running.get() {
                                        "Сбор…"
                                    } else {
                                        "Собрать сейчас"
                                    }
                                }}
                            </Button>
                            <Button
                                appearance=ButtonAppearance::Primary
                                on_click=move |_| load_items()
                                disabled=Signal::derive(move || loading.get())
                            >
                                {move || if loading.get() { "Загрузка..." } else { "Обновить" }}
                            </Button>
                        </div>
                    </div>

                    <Show when=move || is_filter_expanded.get()>
                        <div class="filter-panel-content">
                            <Flex gap=FlexGap::Small align=FlexAlign::End>
                                <div style="min-width: 420px;">
                                    <DateRangePicker
                                        date_from=Signal::derive(move || state.with(|s| s.date_from.clone()))
                                        date_to=Signal::derive(move || state.with(|s| s.date_to.clone()))
                                        on_change=Callback::new(move |(from, to)| {
                                            state.update(|s| {
                                                s.date_from = from;
                                                s.date_to = to;
                                                s.page = 0;
                                            });
                                            load_items();
                                        })
                                        label="Период:".to_string()
                                    />
                                </div>

                                <div style="width: 280px;">
                                    <Flex vertical=true gap=FlexGap::Small>
                                        <Label>"Кабинет"</Label>
                                        <Select value=selected_connection_id>
                                            <option value="">"Все кабинеты"</option>
                                            {move || {
                                                connections.get().into_iter().map(|connection| {
                                                    let id = connection.base.id.as_string();
                                                    let label = connection.base.description;
                                                    view! {
                                                        <option value=id>{label}</option>
                                                    }
                                                }).collect_view()
                                            }}
                                        </Select>
                                    </Flex>
                                </div>

                                <div style="min-width: 280px;">
                                    <Flex vertical=true gap=FlexGap::Small>
                                        <Label>"Поиск"</Label>
                                        <Input
                                            value=search_query
                                            placeholder="Документ, кабинет, организация"
                                        />
                                    </Flex>
                                </div>
                            </Flex>
                        </div>
                    </Show>
                </div>

                {move || error.get().map(|err| view! {
                    <div class="alert alert--error">{err}</div>
                })}

                <div class="table-wrapper">
                    <TableCrosshairHighlight table_id=TABLE_ID.to_string() />

                    <Table attr:id=TABLE_ID attr:style="width: 100%; min-width: 1150px;">
                        <TableHeader>
                            <TableRow>
                                <TableHeaderCell resizable=false min_width=110.0 class="resizable">
                                    <div class="table__sortable-header" style="cursor: pointer;" on:click=move |_| toggle_sort("document_date")>
                                        "Дата"
                                        <span class=move || state.with(|s| get_sort_class(&s.sort_field, "document_date"))>
                                            {move || get_sort_indicator(&state.with(|s| s.sort_field.clone()), "document_date", state.with(|s| s.sort_ascending))}
                                        </span>
                                    </div>
                                </TableHeaderCell>

                                <TableHeaderCell resizable=false min_width=170.0 class="resizable">
                                    <div class="table__sortable-header" style="cursor: pointer;" on:click=move |_| toggle_sort("document_no")>
                                        "Документ"
                                        <span class=move || state.with(|s| get_sort_class(&s.sort_field, "document_no"))>
                                            {move || get_sort_indicator(&state.with(|s| s.sort_field.clone()), "document_no", state.with(|s| s.sort_ascending))}
                                        </span>
                                    </div>
                                </TableHeaderCell>

                                <TableHeaderCell resizable=false min_width=200.0 class="resizable">
                                    "Кабинет"
                                </TableHeaderCell>

                                <TableHeaderCell resizable=false min_width=180.0 class="resizable">
                                    "Организация"
                                </TableHeaderCell>

                                <TableHeaderCell resizable=false min_width=90.0 class="resizable">
                                    <div class="table__sortable-header" style="cursor: pointer; text-align: right; justify-content: flex-end;" on:click=move |_| toggle_sort("lines_count")>
                                        "Позиций"
                                        <span class=move || state.with(|s| get_sort_class(&s.sort_field, "lines_count"))>
                                            {move || get_sort_indicator(&state.with(|s| s.sort_field.clone()), "lines_count", state.with(|s| s.sort_ascending))}
                                        </span>
                                    </div>
                                </TableHeaderCell>

                                <TableHeaderCell resizable=false min_width=110.0 class="resizable">
                                    <div class="table__sortable-header" style="cursor: pointer; text-align: right; justify-content: flex-end;" on:click=move |_| toggle_sort("total_stock_wb")>
                                        "Остаток WB"
                                        <span class=move || state.with(|s| get_sort_class(&s.sort_field, "total_stock_wb"))>
                                            {move || get_sort_indicator(&state.with(|s| s.sort_field.clone()), "total_stock_wb", state.with(|s| s.sort_ascending))}
                                        </span>
                                    </div>
                                </TableHeaderCell>

                                <TableHeaderCell resizable=false min_width=110.0 class="resizable">
                                    <div class="table__sortable-header" style="cursor: pointer; text-align: right; justify-content: flex-end;" on:click=move |_| toggle_sort("total_stock_mp")>
                                        "Остаток МП"
                                        <span class=move || state.with(|s| get_sort_class(&s.sort_field, "total_stock_mp"))>
                                            {move || get_sort_indicator(&state.with(|s| s.sort_field.clone()), "total_stock_mp", state.with(|s| s.sort_ascending))}
                                        </span>
                                    </div>
                                </TableHeaderCell>

                                <TableHeaderCell resizable=false min_width=130.0 class="resizable">
                                    <div class="table__sortable-header" style="cursor: pointer; text-align: right; justify-content: flex-end;" on:click=move |_| toggle_sort("total_balance_sum")>
                                        "Сумма остатков"
                                        <span class=move || state.with(|s| get_sort_class(&s.sort_field, "total_balance_sum"))>
                                            {move || get_sort_indicator(&state.with(|s| s.sort_field.clone()), "total_balance_sum", state.with(|s| s.sort_ascending))}
                                        </span>
                                    </div>
                                </TableHeaderCell>

                                <TableHeaderCell resizable=false min_width=170.0 class="resizable">
                                    <div class="table__sortable-header" style="cursor: pointer;" on:click=move |_| toggle_sort("fetched_at")>
                                        "Загружено"
                                        <span class=move || state.with(|s| get_sort_class(&s.sort_field, "fetched_at"))>
                                            {move || get_sort_indicator(&state.with(|s| s.sort_field.clone()), "fetched_at", state.with(|s| s.sort_ascending))}
                                        </span>
                                    </div>
                                </TableHeaderCell>
                            </TableRow>
                        </TableHeader>

                        <TableBody>
                            <For
                                each=move || state.get().items
                                key=|item| item.id.clone()
                                children=move |item| {
                                    let row_id = item.id.clone();
                                    let doc_date = item.document_date.clone();
                                    let document_no = item.document_no.clone();
                                    let connection_name = item
                                        .connection_name
                                        .clone()
                                        .unwrap_or_else(|| item.connection_id.clone());
                                    let organization_name = item
                                        .organization_name
                                        .clone()
                                        .unwrap_or_else(|| "—".to_string());
                                    let fetched_at = format_datetime(&item.fetched_at);
                                    let balance_sum = format_money(item.total_balance_sum);

                                    view! {
                                        <TableRow>
                                            <TableCell>
                                                <TableCellLayout>
                                                    {format_date(&item.document_date)}
                                                </TableCellLayout>
                                            </TableCell>

                                            <TableCell>
                                                <TableCellLayout truncate=true>
                                                    <a
                                                        href="#"
                                                        class="table__link"
                                                        on:click=move |e| {
                                                            e.prevent_default();
                                                            open_detail(row_id.clone(), doc_date.clone());
                                                        }
                                                    >
                                                        {document_no}
                                                    </a>
                                                </TableCellLayout>
                                            </TableCell>

                                            <TableCell>
                                                <TableCellLayout truncate=true>
                                                    {connection_name}
                                                </TableCellLayout>
                                            </TableCell>

                                            <TableCell>
                                                <TableCellLayout truncate=true>
                                                    {organization_name}
                                                </TableCellLayout>
                                            </TableCell>

                                            <TableCell>
                                                <TableCellLayout attr:style="justify-content: flex-end; text-align: right;">
                                                    {item.lines_count}
                                                </TableCellLayout>
                                            </TableCell>

                                            <TableCell>
                                                <TableCellLayout attr:style="justify-content: flex-end; text-align: right;">
                                                    {item.total_stock_wb}
                                                </TableCellLayout>
                                            </TableCell>

                                            <TableCell>
                                                <TableCellLayout attr:style="justify-content: flex-end; text-align: right;">
                                                    {item.total_stock_mp}
                                                </TableCellLayout>
                                            </TableCell>

                                            <TableCell>
                                                <TableCellLayout attr:style="justify-content: flex-end; text-align: right;">
                                                    {balance_sum}
                                                </TableCellLayout>
                                            </TableCell>

                                            <TableCell>
                                                <TableCellLayout>
                                                    {fetched_at}
                                                </TableCellLayout>
                                            </TableCell>
                                        </TableRow>
                                    }
                                }
                            />
                        </TableBody>
                    </Table>
                </div>
            </div>
        </PageFrame>
    }
}

async fn fetch_connections() -> Result<Vec<ConnectionMP>, String> {
    let url = format!("{}/api/connection_mp", api_base());
    let response = Request::get(&url)
        .send()
        .await
        .map_err(|err| format!("Ошибка загрузки кабинетов: {}", err))?;

    if !response.ok() {
        return Err(format!(
            "Ошибка загрузки кабинетов: HTTP {}",
            response.status()
        ));
    }

    response
        .json::<Vec<ConnectionMP>>()
        .await
        .map_err(|err| format!("Ошибка разбора кабинетов: {}", err))
}
