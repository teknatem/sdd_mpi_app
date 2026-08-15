pub mod state;

use self::state::create_state;
use crate::layout::global_context::AppGlobalContext;
use crate::shared::change_tokens::ChangeTokenContext;
use crate::shared::components::date_range_picker_v3::DateRangePickerV3;
use crate::shared::components::pagination_controls::PaginationControls;
use crate::shared::components::table::{
    TableCellCheckbox, TableCellMoney, TableCrosshairHighlight, TableHeaderCheckbox,
};
use crate::shared::components::ui::badge::Badge as UiBadge;
use crate::shared::icons::icon;
use crate::shared::list_utils::{get_sort_class, get_sort_indicator, Sortable};
use crate::shared::page_frame::PageFrame;
use crate::shared::table_utils::init_column_resize;
use crate::system::tasks::ui::RunTaskButton;
use gloo_net::http::Request;
use leptos::logging::log;
use leptos::prelude::*;
use leptos::task::spawn_local;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use thaw::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures;
use web_sys::{
    Blob, BlobPropertyBag, HtmlAnchorElement, Request as WebRequest, RequestInit, RequestMode,
    Response, Url,
};

use crate::shared::api_utils::api_base;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Organization {
    pub id: String,
    pub code: String,
    pub description: String,
}

/// Форматирует ISO 8601 дату в dd.mm.yyyy
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

/// Форматирует ISO 8601 время в hh:mm:ss
fn format_time(iso_date: &str) -> String {
    if let Some((_, time_part)) = iso_date.split_once('T') {
        let time_clean = time_part
            .split('Z')
            .next()
            .unwrap_or(time_part)
            .split('+')
            .next()
            .unwrap_or(time_part);
        let time_clean = time_clean.split('-').next().unwrap_or(time_clean);
        if let Some(hms) = time_clean.split('.').next() {
            let mut parts = hms.split(':');
            if let (Some(h), Some(m), Some(s)) = (parts.next(), parts.next(), parts.next()) {
                return format!("{}:{}:{}", h, m, s);
            }
        }
    }
    "—".to_string()
}

/// Короткая метка валюты заказа (ISO 4217 numeric → код). Пусто для рублёвых
/// заказов (currency_code не сохраняется для RUB), код — для трансграничных СНГ.
fn currency_label(code: Option<i64>) -> &'static str {
    match code {
        Some(398) => "KZT",
        Some(933) => "BYN",
        Some(51) => "AMD",
        Some(643) => "RUB",
        Some(_) => "?",
        None => "",
    }
}

/// Разобрать один элемент ответа списка в DTO. Используется и при загрузке страницы,
/// и при полной выгрузке в CSV.
fn parse_order_item(item: &serde_json::Value) -> Option<WbOrdersDto> {
    let id = item.get("id")?.as_str()?.to_string();
    let document_no = item.get("document_no")?.as_str()?.to_string();
    let str_field = |key: &str| item.get(key).and_then(|v| v.as_str()).map(String::from);
    Some(WbOrdersDto {
        id,
        document_no,
        line_id: str_field("line_id"),
        order_date: item
            .get("document_date")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        supplier_article: item
            .get("supplier_article")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        brand: str_field("brand"),
        qty: item.get("qty")?.as_f64()?,
        margin_pro: item.get("margin_pro").and_then(|v| v.as_f64()),
        dealer_price_ut: item.get("dealer_price_ut").and_then(|v| v.as_f64()),
        finished_price: item.get("finished_price").and_then(|v| v.as_f64()),
        total_price: item.get("total_price").and_then(|v| v.as_f64()),
        is_cancel: item.get("is_cancel")?.as_bool().unwrap_or(false),
        is_supply: item.get("is_supply").and_then(|v| v.as_bool()),
        is_realization: item.get("is_realization").and_then(|v| v.as_bool()),
        warehouse_type: str_field("warehouse_type"),
        income_id: item.get("income_id").and_then(|v| v.as_i64()),
        currency_code: item.get("currency_code").and_then(|v| v.as_i64()),
        has_wb_sales: item.get("has_wb_sales").and_then(|v| v.as_bool()),
        organization_name: str_field("organization_name"),
        marketplace_article: str_field("marketplace_article"),
        nomenclature_code: str_field("nomenclature_code"),
        nomenclature_article: str_field("nomenclature_article"),
        base_nomenclature_article: str_field("base_nomenclature_article"),
        base_nomenclature_description: str_field("base_nomenclature_description"),
    })
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WbOrdersDto {
    pub id: String,
    pub document_no: String,
    pub line_id: Option<String>,
    pub order_date: String,
    pub supplier_article: String,
    pub brand: Option<String>,
    pub qty: f64,
    pub margin_pro: Option<f64>,
    pub dealer_price_ut: Option<f64>,
    pub finished_price: Option<f64>,
    pub total_price: Option<f64>,
    pub is_cancel: bool,
    pub is_supply: Option<bool>,
    pub is_realization: Option<bool>,
    pub warehouse_type: Option<String>,
    pub income_id: Option<i64>,
    pub currency_code: Option<i64>,
    pub has_wb_sales: Option<bool>,
    pub organization_name: Option<String>,
    pub marketplace_article: Option<String>,
    pub nomenclature_code: Option<String>,
    pub nomenclature_article: Option<String>,
    pub base_nomenclature_article: Option<String>,
    pub base_nomenclature_description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaginatedResponse {
    pub items: Vec<serde_json::Value>,
    pub total: usize,
    pub page: usize,
    pub page_size: usize,
    pub total_pages: usize,
}

impl Sortable for WbOrdersDto {
    fn compare_by_field(&self, other: &Self, field: &str) -> Ordering {
        match field {
            "document_no" => self
                .document_no
                .to_lowercase()
                .cmp(&other.document_no.to_lowercase()),
            "line_id" => self.line_id.cmp(&other.line_id),
            "order_date" => self.order_date.cmp(&other.order_date),
            "supplier_article" => self
                .supplier_article
                .to_lowercase()
                .cmp(&other.supplier_article.to_lowercase()),
            "brand" => match (&self.brand, &other.brand) {
                (Some(a), Some(b)) => a.to_lowercase().cmp(&b.to_lowercase()),
                (Some(_), None) => Ordering::Less,
                (None, Some(_)) => Ordering::Greater,
                (None, None) => Ordering::Equal,
            },
            "base_nomenclature_article" => {
                match (
                    &self.base_nomenclature_article,
                    &other.base_nomenclature_article,
                ) {
                    (Some(a), Some(b)) => a.to_lowercase().cmp(&b.to_lowercase()),
                    (Some(_), None) => Ordering::Less,
                    (None, Some(_)) => Ordering::Greater,
                    (None, None) => Ordering::Equal,
                }
            }
            "base_nomenclature_description" => {
                match (
                    &self.base_nomenclature_description,
                    &other.base_nomenclature_description,
                ) {
                    (Some(a), Some(b)) => a.to_lowercase().cmp(&b.to_lowercase()),
                    (Some(_), None) => Ordering::Less,
                    (None, Some(_)) => Ordering::Greater,
                    (None, None) => Ordering::Equal,
                }
            }
            "qty" => self.qty.partial_cmp(&other.qty).unwrap_or(Ordering::Equal),
            "margin_pro" => match (&self.margin_pro, &other.margin_pro) {
                (Some(a), Some(b)) => a.partial_cmp(b).unwrap_or(Ordering::Equal),
                (Some(_), None) => Ordering::Less,
                (None, Some(_)) => Ordering::Greater,
                (None, None) => Ordering::Equal,
            },
            "dealer_price_ut" => match (&self.dealer_price_ut, &other.dealer_price_ut) {
                (Some(a), Some(b)) => a.partial_cmp(b).unwrap_or(Ordering::Equal),
                (Some(_), None) => Ordering::Less,
                (None, Some(_)) => Ordering::Greater,
                (None, None) => Ordering::Equal,
            },
            "finished_price" => match (&self.finished_price, &other.finished_price) {
                (Some(a), Some(b)) => a.partial_cmp(b).unwrap_or(Ordering::Equal),
                (Some(_), None) => Ordering::Less,
                (None, Some(_)) => Ordering::Greater,
                (None, None) => Ordering::Equal,
            },
            "total_price" => match (&self.total_price, &other.total_price) {
                (Some(a), Some(b)) => a.partial_cmp(b).unwrap_or(Ordering::Equal),
                (Some(_), None) => Ordering::Less,
                (None, Some(_)) => Ordering::Greater,
                (None, None) => Ordering::Equal,
            },
            "organization_name" => match (&self.organization_name, &other.organization_name) {
                (Some(a), Some(b)) => a.to_lowercase().cmp(&b.to_lowercase()),
                (Some(_), None) => Ordering::Less,
                (None, Some(_)) => Ordering::Greater,
                (None, None) => Ordering::Equal,
            },
            "currency_code" => match (&self.currency_code, &other.currency_code) {
                (Some(a), Some(b)) => a.cmp(b),
                (Some(_), None) => Ordering::Less,
                (None, Some(_)) => Ordering::Greater,
                (None, None) => Ordering::Equal,
            },
            _ => Ordering::Equal,
        }
    }
}

const TABLE_ID: &str = "a015-wb-orders-table";
const COLUMN_WIDTHS_KEY: &str = "a015_wb_orders_column_widths";

#[component]
pub fn WbOrdersList() -> impl IntoView {
    let tabs_store =
        leptos::context::use_context::<AppGlobalContext>().expect("AppGlobalContext not found");
    let state = create_state();
    let (loading, set_loading) = signal(false);
    let (error, set_error) = signal::<Option<String>>(None);

    // Filter panel expansion state
    let (is_filter_expanded, set_is_filter_expanded) = signal(false);

    // Show cancelled checkbox
    let show_cancelled = RwSignal::new(state.get().show_cancelled);

    // Organizations
    let (organizations, set_organizations) = signal::<Vec<Organization>>(Vec::new());

    // Batch post/unpost state
    let (posting_in_progress, set_posting_in_progress) = signal(false);
    let (_, set_operation_notification) = signal(None::<String>);

    // CSV export (full dataset, not just current page)
    let (exporting, set_exporting) = signal(false);

    const FORM_KEY: &str = "a015_wb_orders";

    let tabs_store_for_detail = tabs_store.clone();
    let open_detail = move |id: String, document_no: String| {
        tabs_store_for_detail.open_tab(
            &format!("a015_wb_orders_details_{}", id),
            &format!("WB Order {}", document_no),
        );
    };
    let tabs_store_for_close = tabs_store.clone();
    let close_current_tab = move |_| {
        if let Some(key) = tabs_store_for_close.active.get_untracked() {
            tabs_store_for_close.close_tab(&key);
        }
    };

    let load_orders = move || {
        spawn_local(async move {
            set_loading.set(true);
            set_error.set(None);

            let date_from_val = state.with_untracked(|s| s.date_from.clone());
            let date_to_val = state.with_untracked(|s| s.date_to.clone());
            let org_id = state.with_untracked(|s| s.selected_organization_id.clone());
            let search_query_val = state.with_untracked(|s| s.search_query.clone());
            let page = state.with_untracked(|s| s.page);
            let page_size = state.with_untracked(|s| s.page_size);
            let sort_field = state.with_untracked(|s| s.sort_field.clone());
            let sort_ascending = state.with_untracked(|s| s.sort_ascending);
            let show_cancelled_val = show_cancelled.get_untracked();
            let offset = page * page_size;
            let cache_buster = js_sys::Date::now() as i64;

            // Build URL with pagination parameters
            let mut url = format!(
                "{}/api/a015/wb-orders?date_from={}&date_to={}&limit={}&offset={}&sort_by={}&sort_desc={}&show_cancelled={}&_ts={}",
                api_base(),
                date_from_val,
                date_to_val,
                page_size,
                offset,
                sort_field,
                !sort_ascending,
                show_cancelled_val,
                cache_buster
            );

            if let Some(org_id) = org_id {
                url.push_str(&format!("&organization_id={}", org_id));
            }
            if !search_query_val.is_empty() {
                url.push_str(&format!("&search_query={}", search_query_val));
            }

            log!("Loading WB orders with URL: {}", url);

            match Request::get(&url)
                .header("Cache-Control", "no-cache, no-store, must-revalidate")
                .header("Pragma", "no-cache")
                .send()
                .await
            {
                Ok(response) => {
                    if response.ok() {
                        match response.json::<PaginatedResponse>().await {
                            Ok(paginated) => {
                                log!("Parsed paginated response: total={}, page={}, page_size={}, total_pages={}", 
                                    paginated.total, paginated.page, paginated.page_size, paginated.total_pages);

                                let parsed_orders: Vec<WbOrdersDto> = paginated
                                    .items
                                    .iter()
                                    .filter_map(parse_order_item)
                                    .collect();

                                log!("Successfully parsed {} orders", parsed_orders.len());
                                state.update(|s| {
                                    s.orders = parsed_orders;
                                    s.total_count = paginated.total;
                                    s.total_pages = paginated.total_pages;
                                    s.page = paginated.page;
                                    s.page_size = paginated.page_size;
                                    s.is_loaded = true;
                                });
                                set_loading.set(false);
                            }
                            Err(e) => {
                                log!("Failed to parse response: {}", e);
                                set_error.set(Some(format!("Failed to parse response: {}", e)));
                                set_loading.set(false);
                            }
                        }
                    } else {
                        set_error.set(Some(format!("Server error: {}", response.status())));
                        set_loading.set(false);
                    }
                }
                Err(e) => {
                    log!("Failed to fetch orders: {:?}", e);
                    set_error.set(Some(format!("Failed to fetch orders: {}", e)));
                    set_loading.set(false);
                }
            }
        });
    };

    // Load saved settings on mount
    Effect::new(move |_| {
        if !state.with_untracked(|s| s.is_loaded) {
            // Load organizations first
            spawn_local(async move {
                match fetch_organizations().await {
                    Ok(orgs) => {
                        set_organizations.set(orgs);
                    }
                    Err(e) => {
                        log!("Failed to load organizations: {}", e);
                    }
                }
            });

            spawn_local(async move {
                match load_saved_settings(FORM_KEY).await {
                    Ok(Some(settings)) => {
                        state.update(|s| {
                            if let Some(date_from_val) =
                                settings.get("date_from").and_then(|v| v.as_str())
                            {
                                s.date_from = date_from_val.to_string();
                            }
                            if let Some(date_to_val) =
                                settings.get("date_to").and_then(|v| v.as_str())
                            {
                                s.date_to = date_to_val.to_string();
                            }
                            if let Some(org_id) = settings
                                .get("selected_organization_id")
                                .and_then(|v| v.as_str())
                            {
                                if !org_id.is_empty() {
                                    s.selected_organization_id = Some(org_id.to_string());
                                }
                            }
                        });
                        log!("Loaded saved settings for A015");
                        load_orders();
                    }
                    Ok(None) => {
                        log!("No saved settings found for A015");
                        load_orders();
                    }
                    Err(e) => {
                        log!("Failed to load saved settings: {}", e);
                        load_orders();
                    }
                }
            });
        } else {
            log!("Used cached data for A015");
        }
    });

    // Автообновление списка при изменении токена a015 на сервере (например, после
    // проведения/отмены заказа в другой вкладке). prev=None при первом вызове —
    // пропускаем, чтобы не дублировать первичную загрузку из эффекта монтирования.
    let ct = use_context::<ChangeTokenContext>().expect("ChangeTokenContext not found");
    Effect::new(move |prev: Option<u64>| {
        let token = ct.a015_wb_orders.get();
        if prev.is_some() {
            load_orders();
        }
        token
    });

    // Thaw inputs
    let search_query = RwSignal::new(state.get_untracked().search_query.clone());
    let selected_org_id = RwSignal::new(
        state
            .get_untracked()
            .selected_organization_id
            .clone()
            .unwrap_or_default(),
    );

    Effect::new(move || {
        let v = search_query.get();
        untrack(move || {
            state.update(|s| {
                s.search_query = v;
            });
        });
    });

    Effect::new(move || {
        let v = selected_org_id.get();
        untrack(move || {
            state.update(|s| {
                s.selected_organization_id = if v.is_empty() { None } else { Some(v.clone()) };
            });
        });
    });

    // Initialize column resize
    let resize_initialized = leptos::prelude::StoredValue::new(false);
    Effect::new(move |_| {
        if !resize_initialized.get_value() {
            resize_initialized.set_value(true);
            spawn_local(async move {
                gloo_timers::future::TimeoutFuture::new(100).await;
                init_column_resize(TABLE_ID, COLUMN_WIDTHS_KEY);
            });
        }
    });

    // Count active filters
    let active_filters_count = Signal::derive(move || {
        let s = state.get();
        let mut count = 0;
        if !s.date_from.is_empty() {
            count += 1;
        }
        if !s.date_to.is_empty() {
            count += 1;
        }
        if s.selected_organization_id.is_some() {
            count += 1;
        }
        if !s.search_query.is_empty() {
            count += 1;
        }
        count
    });

    // Sort handler
    let toggle_sort = move |field: &'static str| {
        state.update(|s| {
            if s.sort_field == field {
                s.sort_ascending = !s.sort_ascending;
            } else {
                s.sort_field = field.to_string();
                s.sort_ascending = true;
            }
            s.page = 0; // Reset to first page on sort change
        });
        load_orders();
    };

    // Pagination functions
    let go_to_page = move |new_page: usize| {
        state.update(|s| s.page = new_page);
        load_orders();
    };

    let change_page_size = move |new_size: usize| {
        state.update(|s| {
            s.page_size = new_size;
            s.page = 0; // Reset to first page
        });
        load_orders();
    };

    // Selection management
    let selected = RwSignal::new(state.with_untracked(|s| s.selected_ids.clone()));

    let toggle_selection = move |id: String, checked: bool| {
        selected.update(|s| {
            if checked {
                s.insert(id.clone());
            } else {
                s.remove(&id);
            }
        });
        state.update(|s| {
            if checked {
                s.selected_ids.insert(id);
            } else {
                s.selected_ids.remove(&id);
            }
        });
    };

    let toggle_all = move |check_all: bool| {
        if check_all {
            let items = state.get().orders;
            selected.update(|s| {
                s.clear();
                for item in items.iter() {
                    s.insert(item.id.clone());
                }
            });
            state.update(|s| {
                s.selected_ids.clear();
                for item in items.iter() {
                    s.selected_ids.insert(item.id.clone());
                }
            });
        } else {
            selected.update(|s| s.clear());
            state.update(|s| s.selected_ids.clear());
        }
    };

    let selected_count = Signal::derive(move || state.with(|s| s.selected_ids.len()));

    let post_selected = move |_: leptos::ev::MouseEvent| {
        let ids: Vec<String> = state.with(|s| s.selected_ids.iter().cloned().collect());
        if ids.is_empty() {
            return;
        }

        set_posting_in_progress.set(true);
        set_operation_notification.set(None);

        spawn_local(async move {
            let mut ok_count = 0usize;
            let mut fail_count = 0usize;

            for id in &ids {
                let url = format!("{}/api/a015/wb-orders/{}/post", api_base(), id);
                match Request::post(&url).send().await {
                    Ok(resp) if resp.ok() => ok_count += 1,
                    Ok(resp) => {
                        fail_count += 1;
                        log!("Post failed for {}: status {}", id, resp.status());
                    }
                    Err(e) => {
                        fail_count += 1;
                        log!("Post failed for {}: {}", id, e);
                    }
                }
            }

            let msg = if fail_count == 0 {
                format!("Post: успешно {}", ok_count)
            } else {
                format!("Post: успешно {}, ошибок {}", ok_count, fail_count)
            };
            set_operation_notification.set(Some(msg));
            selected.update(|s| s.clear());
            state.update(|s| s.selected_ids.clear());
            load_orders();
            set_posting_in_progress.set(false);
        });
    };

    let items_signal = Signal::derive(move || state.get().orders);
    let selected_signal = Signal::derive(move || selected.get());

    view! {
        <PageFrame page_id="a015_wb_orders--list" category="list">
            <div class="page__header">
                <div class="page__header-left">
                    <h1 class="page__title">"Заказы Wildberries"</h1>
                    <UiBadge variant="primary".to_string()>
                        {move || state.get().total_count.to_string()}
                    </UiBadge>
                </div>

                <div class="page__header-right">
                    <Space>
                        <Button
                            appearance=ButtonAppearance::Subtle
                            size=ButtonSize::Medium
                            on_click=post_selected
                            disabled=Signal::derive(move || {
                                selected_count.get() == 0 || posting_in_progress.get() || loading.get()
                            })
                        >
                            <span class="page-action-button__content">
                                <span class="page-action-button__icon">
                                    {move || {
                                        if posting_in_progress.get() {
                                            view! { <span class="page-action-button__spinner"></span> }.into_any()
                                        } else {
                                            view! { <>{icon("refresh-cw")}</> }.into_any()
                                        }
                                    }}
                                </span>
                                <span class="page-action-button__text page-action-button__text--post">
                                    "Post"
                                    {move || format!(" ({})", selected_count.get())}
                                </span>
                            </span>
                        </Button>
                        <Button
                            appearance=ButtonAppearance::Subtle
                            size=ButtonSize::Medium
                            on_click=move |_| {
                                // Полная выгрузка по текущим фильтрам/сортировке, а не только
                                // текущая страница: дёргаем сервер с большим лимитом.
                                let date_from = state.with_untracked(|s| s.date_from.clone());
                                let date_to = state.with_untracked(|s| s.date_to.clone());
                                let org_id = state.with_untracked(|s| s.selected_organization_id.clone());
                                let search_query_val = state.with_untracked(|s| s.search_query.clone());
                                let sort_field = state.with_untracked(|s| s.sort_field.clone());
                                let sort_ascending = state.with_untracked(|s| s.sort_ascending);
                                let show_cancelled_val = show_cancelled.get_untracked();
                                set_exporting.set(true);
                                spawn_local(async move {
                                    match fetch_all_orders_for_export(
                                        date_from,
                                        date_to,
                                        org_id,
                                        search_query_val,
                                        sort_field,
                                        sort_ascending,
                                        show_cancelled_val,
                                    )
                                    .await
                                    {
                                        Ok(data) => {
                                            if let Err(e) = export_to_csv(&data) {
                                                log!("Failed to export: {}", e);
                                            }
                                        }
                                        Err(e) => log!("Failed to fetch full export: {}", e),
                                    }
                                    set_exporting.set(false);
                                });
                            }
                            disabled=Signal::derive(move || loading.get() || exporting.get() || state.get().total_count == 0)
                        >
                            <span class="page-action-button__content">
                                <span class="page-action-button__icon">{icon("download")}</span>
                                <span class="page-action-button__text">
                                    {move || if exporting.get() { "Выгрузка..." } else { "Excel (csv)" }}
                                </span>
                            </span>
                        </Button>
                        <RunTaskButton
                            task_code="task001-wb-orders-fbs".to_string()
                            label="Получить заказы".to_string()
                            page_action_style=true
                        />
                        <Button
                            appearance=ButtonAppearance::Subtle
                            size=ButtonSize::Medium
                            on_click=close_current_tab
                        >
                            <span class="page-action-button__content">
                                <span class="page-action-button__icon page-action-button__icon--close">{icon("x")}</span>
                                <span class="page-action-button__text">"Закрыть"</span>
                            </span>
                        </Button>
                    </Space>
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
                                    view! {
                                        <span class="filter-panel__badge">{count}</span>
                                    }.into_any()
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
                            <Button
                                appearance=ButtonAppearance::Primary
                                on_click=move |_| load_orders()
                                disabled=Signal::derive(move || loading.get())
                            >
                                {move || if loading.get() { "Загрузка..." } else { "Обновить" }}
                            </Button>
                        </div>
                    </div>

                    <Show when=move || is_filter_expanded.get()>
                        <div class="filter-panel-content">
                            <Flex gap=FlexGap::Small align=FlexAlign::End>
                                <DateRangePickerV3
                                    date_from=Signal::derive(move || state.with(|s| s.date_from.clone()))
                                    date_to=Signal::derive(move || state.with(|s| s.date_to.clone()))
                                    on_change=Callback::new(move |(from, to)| {
                                        state.update(|s| {
                                            s.date_from = from;
                                            s.date_to = to;
                                            s.page = 0;
                                        });
                                        load_orders();
                                    })
                                    label="Период:".to_string()
                                />

                                <div style="width: 260px;">
                                    <Flex vertical=true gap=FlexGap::Small>
                                        <Label>"Организация:"</Label>
                                        <Select value=selected_org_id>
                                            <option value="">"Все организации"</option>
                                            {move || organizations.get().into_iter().map(|org| {
                                                let id = org.id.clone();
                                                view! {
                                                    <option value=id>{org.description}</option>
                                                }
                                            }).collect_view()}
                                        </Select>
                                    </Flex>
                                </div>

                                <div style="flex: 1; max-width: 300px;">
                                    <Flex vertical=true gap=FlexGap::Small>
                                        <Label>"Поиск:"</Label>
                                        <Input
                                            value=search_query
                                            placeholder="Артикул, номер, Line ID, наименование базы..."
                                        />
                                    </Flex>
                                </div>

                                <div style="width: 180px;">
                                    <Flex vertical=true gap=FlexGap::Small>
                                        <Label>" "</Label>
                                        <Checkbox
                                            checked=show_cancelled
                                            label="Показать отменённые"
                                        />
                                    </Flex>
                                </div>

                            </Flex>
                        </div>
                    </Show>
                </div>

                {move || {
                    error
                        .get()
                        .map(|err| {
                            view! {
                                <div class="alert alert--error">
                                    {err}
                                </div>
                            }
                        })
                }}

                <div class="table-wrapper">
                    <TableCrosshairHighlight table_id=TABLE_ID.to_string() />

                    <Table attr:id=TABLE_ID attr:style="width: 100%; min-width: 1272px;">
                        <TableHeader>
                            <TableRow>
                                <TableHeaderCheckbox
                                    items=items_signal
                                    selected=selected_signal
                                    get_id=Callback::new(|row: WbOrdersDto| row.id.clone())
                                    on_change=Callback::new(toggle_all)
                                />

                                <TableHeaderCell resizable=false min_width=84.0 class="resizable" attr:style="width: 10ch; max-width: 10ch;">
                                    <div class="table__sortable-header" style="cursor: pointer;" on:click=move |_| toggle_sort("line_id")>
                                        "Line ID"
                                        <span class=move || state.with(|s| get_sort_class(&s.sort_field, "line_id"))>
                                            {move || get_sort_indicator(&state.with(|s| s.sort_field.clone()), "line_id", state.with(|s| s.sort_ascending))}
                                        </span>
                                    </div>
                                </TableHeaderCell>

                                <TableHeaderCell resizable=false min_width=86.0 class="resizable">
                                    <div class="table__sortable-header" style="cursor: pointer;" on:click=move |_| toggle_sort("order_date")>
                                        "Дата"
                                        <span class=move || state.with(|s| get_sort_class(&s.sort_field, "order_date"))>
                                            {move || get_sort_indicator(&state.with(|s| s.sort_field.clone()), "order_date", state.with(|s| s.sort_ascending))}
                                        </span>
                                    </div>
                                </TableHeaderCell>

                                <TableHeaderCell resizable=false min_width=68.0 class="resizable">
                                    "Время"
                                </TableHeaderCell>

                                <TableHeaderCell resizable=false min_width=150.0 class="resizable">
                                    <div class="table__sortable-header" style="cursor: pointer;" on:click=move |_| toggle_sort("organization_name")>
                                        "Организация"
                                        <span class=move || state.with(|s| get_sort_class(&s.sort_field, "organization_name"))>
                                            {move || get_sort_indicator(&state.with(|s| s.sort_field.clone()), "organization_name", state.with(|s| s.sort_ascending))}
                                        </span>
                                    </div>
                                </TableHeaderCell>

                                <TableHeaderCell resizable=false min_width=120.0 class="resizable">
                                    <div class="table__sortable-header" style="cursor: pointer;" on:click=move |_| toggle_sort("supplier_article")>
                                        "Артикул"
                                        <span class=move || state.with(|s| get_sort_class(&s.sort_field, "supplier_article"))>
                                            {move || get_sort_indicator(&state.with(|s| s.sort_field.clone()), "supplier_article", state.with(|s| s.sort_ascending))}
                                        </span>
                                    </div>
                                </TableHeaderCell>

                                <TableHeaderCell resizable=false min_width=130.0 class="resizable">
                                    <div class="table__sortable-header" style="cursor: pointer;" on:click=move |_| toggle_sort("base_nomenclature_article")>
                                        "Артикул база"
                                        <span class=move || state.with(|s| get_sort_class(&s.sort_field, "base_nomenclature_article"))>
                                            {move || get_sort_indicator(&state.with(|s| s.sort_field.clone()), "base_nomenclature_article", state.with(|s| s.sort_ascending))}
                                        </span>
                                    </div>
                                </TableHeaderCell>

                                <TableHeaderCell resizable=false min_width=160.0 class="resizable">
                                    <div class="table__sortable-header" style="cursor: pointer;" on:click=move |_| toggle_sort("base_nomenclature_description")>
                                        "Наименование база"
                                        <span class=move || state.with(|s| get_sort_class(&s.sort_field, "base_nomenclature_description"))>
                                            {move || get_sort_indicator(&state.with(|s| s.sort_field.clone()), "base_nomenclature_description", state.with(|s| s.sort_ascending))}
                                        </span>
                                    </div>
                                </TableHeaderCell>

                                <TableHeaderCell resizable=false min_width=76.0 class="resizable">
                                    <div class="table__sortable-header" style="cursor: pointer;" on:click=move |_| toggle_sort("margin_pro")>
                                        "Маржа, %"
                                        <span class=move || state.with(|s| get_sort_class(&s.sort_field, "margin_pro"))>
                                            {move || get_sort_indicator(&state.with(|s| s.sort_field.clone()), "margin_pro", state.with(|s| s.sort_ascending))}
                                        </span>
                                    </div>
                                </TableHeaderCell>

                                <TableHeaderCell resizable=false min_width=86.0 class="resizable">
                                    <div class="table__sortable-header" style="cursor: pointer;" on:click=move |_| toggle_sort("dealer_price_ut")>
                                        "Дил. цена УТ"
                                        <span class=move || state.with(|s| get_sort_class(&s.sort_field, "dealer_price_ut"))>
                                            {move || get_sort_indicator(&state.with(|s| s.sort_field.clone()), "dealer_price_ut", state.with(|s| s.sort_ascending))}
                                        </span>
                                    </div>
                                </TableHeaderCell>

                                <TableHeaderCell resizable=false min_width=96.0 class="resizable">
                                    <div class="table__sortable-header" style="cursor: pointer;" on:click=move |_| toggle_sort("finished_price")>
                                        "Итоговая цена"
                                        <span class=move || state.with(|s| get_sort_class(&s.sort_field, "finished_price"))>
                                            {move || get_sort_indicator(&state.with(|s| s.sort_field.clone()), "finished_price", state.with(|s| s.sort_ascending))}
                                        </span>
                                    </div>
                                </TableHeaderCell>

                                <TableHeaderCell resizable=false min_width=52.0 class="resizable" attr:style="width: 6ch; max-width: 6ch;">
                                    <div class="table__sortable-header" style="cursor: pointer;" on:click=move |_| toggle_sort("currency_code")>
                                        "Вал."
                                        <span class=move || state.with(|s| get_sort_class(&s.sort_field, "currency_code"))>
                                            {move || get_sort_indicator(&state.with(|s| s.sort_field.clone()), "currency_code", state.with(|s| s.sort_ascending))}
                                        </span>
                                    </div>
                                </TableHeaderCell>

                                <TableHeaderCell resizable=false min_width=70.0 class="resizable">
                                    "Тип"
                                </TableHeaderCell>

                                <TableHeaderCell resizable=false min_width=150.0 class="resizable">
                                    "Поставка"
                                </TableHeaderCell>

                                <TableHeaderCell resizable=false min_width=90.0 class="resizable">
                                    "Отменён"
                                </TableHeaderCell>
                            </TableRow>
                        </TableHeader>

                        <TableBody>
                            {move || state.get().orders.into_iter().map(move |order| {
                                    let order_id = order.id.clone();
                                    let order_id_for_link = order_id.clone();
                                    let document_no_for_link = order.document_no.clone();
                                    let line_id_text = order
                                        .line_id
                                        .clone()
                                        .filter(|v| !v.trim().is_empty())
                                        .unwrap_or_else(|| order.document_no.clone());
                                    let formatted_date = format_date(&order.order_date);
                                    let formatted_time = format_time(&order.order_date);

                                    view! {
                                        <TableRow>
                                            <TableCellCheckbox
                                                item_id=order_id.clone()
                                                selected=selected_signal
                                                on_change=Callback::new(move |(id, checked)| toggle_selection(id, checked))
                                            />

                                            <TableCell attr:style="width: 10ch; max-width: 10ch;">
                                                <TableCellLayout truncate=true>
                                                    <a
                                                        href="#"
                                                        class="table__link"
                                                        style=if order.has_wb_sales.unwrap_or(false) {
                                                            "display: inline-block; max-width: 10ch; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: #1b5e20; text-decoration: underline; font-weight: 600;"
                                                        } else {
                                                            "display: inline-block; max-width: 10ch; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: #0f6cbd; text-decoration: underline;"
                                                        }
                                                        title=if order.has_wb_sales.unwrap_or(false) {
                                                            "Есть связанные WB Sales"
                                                        } else {
                                                            "Открыть документ"
                                                        }
                                                        on:click=move |e| {
                                                            e.prevent_default();
                                                            open_detail(order_id_for_link.clone(), document_no_for_link.clone());
                                                        }
                                                    >
                                                        {line_id_text}
                                                    </a>
                                                </TableCellLayout>
                                            </TableCell>

                                            <TableCell>
                                                <TableCellLayout>
                                                    {formatted_date}
                                                </TableCellLayout>
                                            </TableCell>

                                            <TableCell>
                                                <TableCellLayout>
                                                    {formatted_time}
                                                </TableCellLayout>
                                            </TableCell>

                                            <TableCell>
                                                <TableCellLayout truncate=true>
                                                    {order.organization_name.clone().unwrap_or_else(|| "—".to_string())}
                                                </TableCellLayout>
                                            </TableCell>

                                            <TableCell>
                                                <TableCellLayout truncate=true>
                                                    {order.supplier_article}
                                                </TableCellLayout>
                                            </TableCell>

                                            <TableCell>
                                                <TableCellLayout truncate=true>
                                                    {order.base_nomenclature_article.clone().unwrap_or_else(|| "—".to_string())}
                                                </TableCellLayout>
                                            </TableCell>

                                            <TableCell>
                                                <TableCellLayout truncate=true>
                                                    {order.base_nomenclature_description.clone().unwrap_or_else(|| "—".to_string())}
                                                </TableCellLayout>
                                            </TableCell>

                                            <TableCellMoney
                                                value=order.margin_pro
                                                show_currency=false
                                                color_by_sign=false
                                            />

                                            <TableCellMoney
                                                value=order.dealer_price_ut
                                                show_currency=false
                                                color_by_sign=false
                                            />

                                            <TableCellMoney
                                                value=order.finished_price.unwrap_or(0.0)
                                                show_currency=false
                                                color_by_sign=false
                                            />

                                            <TableCell attr:style="width: 6ch; max-width: 6ch;">
                                                <TableCellLayout>
                                                    {
                                                        let label = currency_label(order.currency_code);
                                                        if label.is_empty() {
                                                            view! { <span class="text-muted">"—"</span> }.into_any()
                                                        } else {
                                                            view! {
                                                                <Badge appearance=BadgeAppearance::Tint color=BadgeColor::Danger>
                                                                    {label}
                                                                </Badge>
                                                            }.into_any()
                                                        }
                                                    }
                                                </TableCellLayout>
                                            </TableCell>

                                            <TableCell>
                                                <TableCellLayout>
                                                    {
                                                        let is_fbw = order
                                                            .warehouse_type
                                                            .as_deref()
                                                            == Some("Склад WB");
                                                        if is_fbw {
                                                            view! {
                                                                <Badge appearance=BadgeAppearance::Tint color=BadgeColor::Warning>
                                                                    "FBW"
                                                                </Badge>
                                                            }.into_any()
                                                        } else if order.income_id.map_or(false, |v| v != 0) {
                                                            view! {
                                                                <Badge appearance=BadgeAppearance::Tint color=BadgeColor::Brand>
                                                                    "FBS"
                                                                </Badge>
                                                            }.into_any()
                                                        } else {
                                                            view! { <span class="text-muted">"—"</span> }.into_any()
                                                        }
                                                    }
                                                </TableCellLayout>
                                            </TableCell>

                                            <TableCell>
                                                <TableCellLayout>
                                                    {
                                                        if let Some(income_id) = order.income_id.filter(|&v| v != 0) {
                                                            let supply_id = format!("WB-GI-{}", income_id);
                                                            let supply_id_nav = supply_id.clone();
                                                            let supply_id_text = supply_id.clone();
                                                            view! {
                                                                <a
                                                                    href="#"
                                                                    class="table__link"
                                                                    on:click=move |e| {
                                                                        e.prevent_default();
                                                                        tabs_store.open_tab(
                                                                            &format!("a029_wb_supply_details_{}", supply_id_nav),
                                                                            &format!("Поставка {}", supply_id_nav),
                                                                        );
                                                                    }
                                                                >
                                                                    {supply_id_text}
                                                                </a>
                                                            }.into_any()
                                                        } else {
                                                            view! { <span>"—"</span> }.into_any()
                                                        }
                                                    }
                                                </TableCellLayout>
                                            </TableCell>

                                            <TableCell>
                                                <TableCellLayout>
                                                    {if order.is_cancel { "Да" } else { "" }}
                                                </TableCellLayout>
                                            </TableCell>
                                        </TableRow>
                                    }
                            }).collect_view()}
                        </TableBody>
                    </Table>
                </div>
            </div>
        </PageFrame>
    }
}

/// Полная выгрузка заказов по текущим фильтрам/сортировке (все страницы) для CSV.
async fn fetch_all_orders_for_export(
    date_from: String,
    date_to: String,
    org_id: Option<String>,
    search_query: String,
    sort_field: String,
    sort_ascending: bool,
    show_cancelled: bool,
) -> Result<Vec<WbOrdersDto>, String> {
    let cache_buster = js_sys::Date::now() as i64;
    let mut url = format!(
        "{}/api/a015/wb-orders?date_from={}&date_to={}&limit={}&offset=0&sort_by={}&sort_desc={}&show_cancelled={}&_ts={}",
        api_base(),
        date_from,
        date_to,
        10_000_000,
        sort_field,
        !sort_ascending,
        show_cancelled,
        cache_buster
    );
    if let Some(org_id) = org_id {
        if !org_id.is_empty() {
            url.push_str(&format!("&organization_id={}", org_id));
        }
    }
    if !search_query.is_empty() {
        url.push_str(&format!("&search_query={}", search_query));
    }

    let response = Request::get(&url)
        .header("Cache-Control", "no-cache, no-store, must-revalidate")
        .send()
        .await
        .map_err(|e| format!("Failed to fetch: {}", e))?;
    if !response.ok() {
        return Err(format!("Server error: {}", response.status()));
    }
    let paginated = response
        .json::<PaginatedResponse>()
        .await
        .map_err(|e| format!("Failed to parse: {}", e))?;
    Ok(paginated
        .items
        .iter()
        .filter_map(parse_order_item)
        .collect())
}

/// Загрузка списка организаций
async fn fetch_organizations() -> Result<Vec<Organization>, String> {
    let opts = RequestInit::new();
    opts.set_method("GET");
    opts.set_mode(RequestMode::Cors);

    let url = format!("{}/api/organization", api_base());
    let request = WebRequest::new_with_str_and_init(&url, &opts).map_err(|e| format!("{e:?}"))?;
    request
        .headers()
        .set("Accept", "application/json")
        .map_err(|e| format!("{e:?}"))?;

    let window = web_sys::window().ok_or_else(|| "no window".to_string())?;
    let resp_value = wasm_bindgen_futures::JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(|e| format!("{e:?}"))?;
    let resp: Response = resp_value.dyn_into().map_err(|e| format!("{e:?}"))?;

    if !resp.ok() {
        return Err(format!("HTTP {}", resp.status()));
    }

    let text = wasm_bindgen_futures::JsFuture::from(resp.text().map_err(|e| format!("{e:?}"))?)
        .await
        .map_err(|e| format!("{e:?}"))?;
    let text: String = text.as_string().ok_or_else(|| "bad text".to_string())?;
    let data: Vec<Organization> = serde_json::from_str(&text).map_err(|e| format!("{e}"))?;
    Ok(data)
}

/// Load saved settings from database
async fn load_saved_settings(form_key: &str) -> Result<Option<serde_json::Value>, String> {
    let url = format!("{}/api/user-form-settings/{}", api_base(), form_key);
    match Request::get(&url).send().await {
        Ok(response) => {
            if response.status() == 200 {
                match response.json::<serde_json::Value>().await {
                    Ok(settings) => Ok(Some(settings)),
                    Err(e) => {
                        log!("Failed to parse settings: {}", e);
                        Ok(None)
                    }
                }
            } else if response.status() == 404 {
                Ok(None)
            } else {
                Err(format!("HTTP {}", response.status()))
            }
        }
        Err(e) => {
            log!("Failed to fetch settings: {:?}", e);
            Err(format!("{:?}", e))
        }
    }
}

/// Экспорт WB Orders в CSV для Excel
fn export_to_csv(data: &[WbOrdersDto]) -> Result<(), String> {
    let mut csv = String::from("\u{FEFF}");

    csv.push_str("Номер заказа;Дата заказа;Организация;Артикул продавца;Артикул МП;Артикул 1С;Артикул база;Наименование база;Бренд;Маржа, %;Дил. цена УТ;Итоговая цена;Валюта;Отменён\n");

    for order in data {
        let order_date = format_date(&order.order_date);
        let org_name = order
            .organization_name
            .as_ref()
            .map(|s| s.as_str())
            .unwrap_or("—");
        let mp_article = order
            .marketplace_article
            .as_ref()
            .map(|s| s.as_str())
            .unwrap_or("—");
        let base_nom_article = order
            .base_nomenclature_article
            .as_ref()
            .map(|s| s.as_str())
            .unwrap_or("—");
        let base_nom_description = order
            .base_nomenclature_description
            .as_ref()
            .map(|s| s.as_str())
            .unwrap_or("—");
        let nom_article = order
            .nomenclature_article
            .as_ref()
            .map(|s| s.as_str())
            .unwrap_or("—");
        let brand = order.brand.as_ref().map(|s| s.as_str()).unwrap_or("—");

        let margin_str = order
            .margin_pro
            .map(|a| format!("{:.2}", a).replace(".", ","))
            .unwrap_or_else(|| "—".to_string());
        let dealer_price_str = order
            .dealer_price_ut
            .map(|a| format!("{:.2}", a).replace(".", ","))
            .unwrap_or_else(|| "—".to_string());
        let finished_price_str = order
            .finished_price
            .map(|a| format!("{:.2}", a).replace(".", ","))
            .unwrap_or_else(|| "—".to_string());
        let is_cancel_str = if order.is_cancel { "Да" } else { "Нет" };
        // Пусто для RUB → показываем явно "RUB", иначе код валюты (KZT/BYN/AMD).
        let currency_str = match currency_label(order.currency_code) {
            "" => "RUB",
            label => label,
        };

        csv.push_str(&format!(
            "\"{}\";\"{}\";\"{}\";\"{}\";\"{}\";\"{}\";\"{}\";\"{}\";\"{}\";{};{};{};\"{}\";\"{}\"\n",
            order.document_no.replace('\"', "\"\""),
            order_date,
            org_name.replace('\"', "\"\""),
            order.supplier_article.replace('\"', "\"\""),
            mp_article.replace('\"', "\"\""),
            nom_article.replace('\"', "\"\""),
            base_nom_article.replace('\"', "\"\""),
            base_nom_description.replace('\"', "\"\""),
            brand.replace('\"', "\"\""),
            margin_str,
            dealer_price_str,
            finished_price_str,
            currency_str,
            is_cancel_str
        ));
    }

    let blob_parts = js_sys::Array::new();
    blob_parts.push(&wasm_bindgen::JsValue::from_str(&csv));

    let blob_props = BlobPropertyBag::new();
    blob_props.set_type("text/csv;charset=utf-8;");

    let blob = Blob::new_with_str_sequence_and_options(&blob_parts, &blob_props)
        .map_err(|e| format!("Failed to create blob: {:?}", e))?;

    let url = Url::create_object_url_with_blob(&blob)
        .map_err(|e| format!("Failed to create URL: {:?}", e))?;

    let window = web_sys::window().ok_or_else(|| "no window".to_string())?;
    let document = window.document().ok_or_else(|| "no document".to_string())?;

    let a = document
        .create_element("a")
        .map_err(|e| format!("Failed to create element: {:?}", e))?
        .dyn_into::<HtmlAnchorElement>()
        .map_err(|e| format!("Failed to cast to anchor: {:?}", e))?;

    a.set_href(&url);
    let filename = format!(
        "wb_orders_{}.csv",
        chrono::Utc::now().format("%Y%m%d_%H%M%S")
    );
    a.set_download(&filename);
    a.click();

    Url::revoke_object_url(&url).map_err(|e| format!("Failed to revoke URL: {:?}", e))?;

    Ok(())
}
