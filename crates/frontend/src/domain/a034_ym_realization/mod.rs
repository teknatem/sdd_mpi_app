//! Фронтенд агрегата a034_ym_realization (Отчёт о реализации YM, слой ybuh).
//! Лёгкие компоненты: список документов и страница деталей с журналом GL.

use crate::general_ledger::ui::{
    document_general_ledger_entries_nav_id, DocumentGeneralLedgerEntries,
};
use crate::layout::global_context::AppGlobalContext;
use crate::shared::api_utils::api_base;
use crate::shared::components::card_animated::CardAnimated;
use crate::shared::components::date_range_picker::DateRangePicker;
use crate::shared::components::table::format_money;
use crate::shared::icons::icon;
use crate::shared::list_utils::{get_sort_class, get_sort_indicator};
use crate::shared::page_frame::PageFrame;
use crate::shared::page_standard::PAGE_CAT_LIST;
use crate::system::favorites::ui::FavoriteButton;
use chrono::{Datelike, Utc};
use contracts::domain::a006_connection_mp::aggregate::ConnectionMP;
use contracts::domain::a034_ym_realization::aggregate::YmRealizationLine;
use contracts::domain::common::AggregateId;
use contracts::general_ledger::GeneralLedgerEntryDto;
use gloo_net::http::Request;
use leptos::prelude::event_target_checked;
use leptos::prelude::*;
use leptos::task::spawn_local;
use serde::Deserialize;
use std::collections::HashSet;
use thaw::*;

#[derive(Debug, Clone, Deserialize)]
struct ListItem {
    id: String,
    document_date: String,
    lines_count: i32,
    total_sales_revenue: f64,
    total_return_revenue: f64,
    net_revenue: f64,
    connection_name: Option<String>,
    connection_id: String,
    #[allow(dead_code)]
    is_posted: bool,
    #[serde(default)]
    placement_type: Option<String>,
    #[serde(default)]
    delivery_discrepancy: f64,
    #[serde(default)]
    returns_discrepancy: f64,
}

#[derive(Debug, Clone, Deserialize)]
struct ListResponse {
    items: Vec<ListItem>,
    total: usize,
}

#[derive(Debug, Clone, Deserialize)]
struct DetailsDto {
    document_no: String,
    document_date: String,
    connection_name: Option<String>,
    organization_name: Option<String>,
    #[serde(default)]
    placement_type: Option<String>,
    #[serde(default)]
    campaign_id: Option<String>,
    total_sales_revenue: f64,
    total_return_revenue: f64,
    net_revenue: f64,
    source: String,
    fetched_at: String,
    is_posted: bool,
    #[serde(default)]
    sales_lines: Vec<YmRealizationLine>,
    #[serde(default)]
    return_lines: Vec<YmRealizationLine>,
    #[serde(default)]
    product_names: std::collections::HashMap<String, String>,
}

#[derive(Debug, Clone, Deserialize)]
struct JournalResponse {
    general_ledger_entries: Vec<GeneralLedgerEntryDto>,
}

#[derive(Debug, Clone, Deserialize)]
struct PaymentDetailRow {
    kind: String,
    order_id: Option<String>,
    #[serde(default)]
    marketplace_product_ref: Option<String>,
    #[serde(default)]
    product_name: Option<String>,
    nomenclature: String,
    payment_date: Option<String>,
    return_amount: f64,
    revenue_amount: f64,
    return_qty: f64,
    revenue_qty: f64,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct PaymentDetailTotals {
    return_amount: f64,
    revenue_amount: f64,
    return_qty: f64,
    revenue_qty: f64,
}

#[derive(Debug, Clone, Deserialize)]
struct PaymentDetailResponse {
    rows: Vec<PaymentDetailRow>,
    totals: PaymentDetailTotals,
}

#[derive(Debug, Clone, Deserialize)]
struct ReconRow {
    order_id: String,
    #[serde(default)]
    marketplace_product_ref: Option<String>,
    #[serde(default)]
    product_name: Option<String>,
    shop_sku: String,
    nomenclature: String,
    #[serde(default)]
    order_status: Option<String>,
    #[serde(default)]
    order_delivery_date: Option<String>,
    ybuh_amount: f64,
    order_amount: f64,
    amount_delta: f64,
    ybuh_qty: f64,
    order_qty: f64,
    qty_delta: f64,
}

#[derive(Debug, Clone, Deserialize)]
struct ReconGroup {
    category: String,
    rows: Vec<ReconRow>,
    count: i64,
    ybuh_total: f64,
    order_total: f64,
    delta_total: f64,
}

#[derive(Debug, Clone, Deserialize)]
struct ReconResponse {
    groups: Vec<ReconGroup>,
}

#[derive(Debug, Clone, Deserialize)]
struct FetchMissingOrdersResult {
    total_missing: usize,
    fetched: usize,
    failed: usize,
    #[serde(default)]
    errors: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct ReconSummaryRow {
    category: String,
    report_qty: f64,
    report_sum: f64,
    orders_qty: f64,
    orders_sum: f64,
}

#[derive(Debug, Clone, Deserialize)]
struct ReconSummaryResponse {
    deliveries: Vec<ReconSummaryRow>,
    returns: Vec<ReconSummaryRow>,
}

#[derive(Debug, Clone, Deserialize)]
struct DeliveryOrderRow {
    order_no: String,
    #[serde(default)]
    status_norm: Option<String>,
    shop_sku: String,
    nomenclature: String,
    #[serde(default)]
    marketplace_product_ref: Option<String>,
    #[serde(default)]
    product_name: Option<String>,
    qty: f64,
    buyer_price: f64,
    amount: f64,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct DeliveryOrdersTotals {
    qty: f64,
    amount: f64,
}

#[derive(Debug, Clone, Deserialize)]
struct DeliveryOrdersResponse {
    rows: Vec<DeliveryOrderRow>,
    totals: DeliveryOrdersTotals,
}

/// Открыть документ заказа (a013_ym_order) по номеру: резолвим id через список и
/// открываем вкладку. Используется ссылками «Заказ» во всех таблицах деталей a034.
fn open_order(tabs_store: AppGlobalContext, order_no: String) {
    if order_no.trim().is_empty() {
        return;
    }
    spawn_local(async move {
        let url = format!(
            "{}/api/a013/ym-order/list?search_document_no={}&limit=1",
            api_base(),
            order_no
        );
        if let Ok(resp) = Request::get(&url).send().await {
            if resp.ok() {
                if let Ok(data) = resp.json::<A013ListResponse>().await {
                    if let Some(item) = data.items.into_iter().next() {
                        tabs_store.open_tab(
                            &format!("a013_ym_order_details_{}", item.id),
                            &format!("YM Order {}", order_no),
                        );
                    }
                }
            }
        }
    });
}

#[derive(Debug, Clone, Deserialize)]
struct A013ListItem {
    id: String,
}

#[derive(Debug, Clone, Deserialize)]
struct A013ListResponse {
    items: Vec<A013ListItem>,
}

/// Ячейка «Товар»: гиперссылка на карточку a007 по marketplace_product_ref.
/// Нет ref — показываем подпись (или прочерк) текстом.
fn product_link(tabs_store: AppGlobalContext, mp_ref: Option<String>, label: String) -> AnyView {
    let text = if label.trim().is_empty() {
        "—".to_string()
    } else {
        label
    };
    match mp_ref
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
    {
        Some(id) => {
            let label = text.clone();
            view! {
                <a
                    href="#"
                    class="table__link"
                    on:click=move |ev| {
                        ev.prevent_default();
                        tabs_store.open_tab(
                            &format!("a007_marketplace_product_details_{}", id),
                            &format!("Товар {}", label),
                        );
                    }
                >{text}</a>
            }
            .into_any()
        }
        None => text.into_any(),
    }
}

/// Ячейка «Заказ»: гиперссылка, открывающая документ заказа; пустой номер — текст.
fn order_link(tabs_store: AppGlobalContext, order_no: String) -> AnyView {
    if order_no.trim().is_empty() {
        return ().into_any();
    }
    let label = order_no.clone();
    view! {
        <a
            href="#"
            class="table__link"
            on:click=move |ev| {
                ev.prevent_default();
                open_order(tabs_store.clone(), order_no.clone());
            }
        >{label}</a>
    }
    .into_any()
}

/// Кабинеты МП (id, подпись) для фильтра.
async fn load_cabinet_options() -> Vec<(String, String)> {
    let url = format!("{}/api/connection_mp", api_base());
    let Ok(resp) = Request::get(&url).send().await else {
        return vec![];
    };
    if !resp.ok() {
        return vec![];
    }
    let Ok(data) = resp.json::<Vec<ConnectionMP>>().await else {
        return vec![];
    };
    let mut opts: Vec<(String, String)> = data
        .into_iter()
        .map(|c| {
            let label = if c.base.description.trim().is_empty() {
                c.base.code.clone()
            } else {
                c.base.description.clone()
            };
            (c.base.id.as_string(), label)
        })
        .collect();
    opts.sort_by(|a, b| a.1.cmp(&b.1));
    opts
}

/// Период по умолчанию — текущий месяц (первое-последнее число).
fn default_month_range() -> (String, String) {
    let now = Utc::now().date_naive();
    let (year, month) = (now.year(), now.month());
    let start = chrono::NaiveDate::from_ymd_opt(year, month, 1).expect("start");
    let end = if month == 12 {
        chrono::NaiveDate::from_ymd_opt(year + 1, 1, 1)
    } else {
        chrono::NaiveDate::from_ymd_opt(year, month + 1, 1)
    }
    .map(|d| d - chrono::Duration::days(1))
    .expect("end");
    (
        start.format("%Y-%m-%d").to_string(),
        end.format("%Y-%m-%d").to_string(),
    )
}

/// Стиль ячейки расхождения: значимое отклонение — красным, иначе обычное.
fn discrepancy_cell_style(value: f64) -> &'static str {
    if value.abs() > 0.5 {
        "width:100px; text-align:right; color: var(--color-error-700); font-weight:600;"
    } else {
        "width:100px; text-align:right; color: var(--color-text-secondary);"
    }
}

#[component]
pub fn YmRealizationList() -> impl IntoView {
    let tabs_store = use_context::<AppGlobalContext>().expect("AppGlobalContext not found");
    let items = RwSignal::new(Vec::<ListItem>::new());
    let total = RwSignal::new(0usize);
    let loading = RwSignal::new(false);
    let error_msg = RwSignal::new(Option::<String>::None);
    let cabinets = RwSignal::new(Vec::<(String, String)>::new());
    let (default_from, default_to) = default_month_range();
    let date_from = RwSignal::new(default_from);
    let date_to = RwSignal::new(default_to);
    let connection_id = RwSignal::new(String::new());
    let sort_field = RwSignal::new("document_date".to_string());
    let sort_ascending = RwSignal::new(false);
    let selected_ids = RwSignal::new(HashSet::<String>::new());
    let posting = RwSignal::new(false);

    // Поля сводных расхождений сортируются на клиенте (считаются построчно на сервере),
    // остальные колонки — на сервере (через sort_by/sort_desc).
    let is_client_sort = |f: &str| f == "delivery_discrepancy" || f == "returns_discrepancy";

    let load = move || {
        spawn_local(async move {
            loading.set(true);
            error_msg.set(None);
            let df = date_from.get_untracked();
            let dt = date_to.get_untracked();
            let cab = connection_id.get_untracked();
            let sf = sort_field.get_untracked();
            let asc = sort_ascending.get_untracked();
            // Серверная сортировка только для колонок БД; для клиентских полей
            // отправляем дефолт (document_date) — порядок задаст display_items.
            let server_sort = if is_client_sort(&sf) {
                "document_date".to_string()
            } else {
                sf
            };
            let mut url = format!(
                "{}/api/a034/ym-realization/list?limit=500&date_from={}&date_to={}&sort_by={}&sort_desc={}",
                api_base(),
                df,
                dt,
                server_sort,
                !asc
            );
            if !cab.is_empty() {
                url.push_str(&format!("&connection_id={}", cab));
            }
            match Request::get(&url).send().await {
                Ok(resp) if resp.ok() => match resp.json::<ListResponse>().await {
                    Ok(data) => {
                        total.set(data.total);
                        items.set(data.items);
                    }
                    Err(e) => error_msg.set(Some(format!("Ошибка разбора: {e}"))),
                },
                Ok(resp) => error_msg.set(Some(format!("HTTP {}", resp.status()))),
                Err(e) => error_msg.set(Some(format!("Ошибка запроса: {e}"))),
            }
            loading.set(false);
        });
    };

    // Загрузка кабинетов + первичная загрузка данных.
    Effect::new(move |_| {
        spawn_local(async move {
            cabinets.set(load_cabinet_options().await);
        });
    });
    Effect::new(move |_| load());

    // Перезагрузка при смене кабинета (после первичного монтирования).
    let cabinet_inited = StoredValue::new(false);
    Effect::new(move |_| {
        let _ = connection_id.get();
        if cabinet_inited.get_value() {
            load();
        } else {
            cabinet_inited.set_value(true);
        }
    });

    // Элементы для отображения: для клиентских полей сортируем на месте.
    let display_items = Signal::derive(move || {
        let mut v = items.get();
        let f = sort_field.get();
        let asc = sort_ascending.get();
        match f.as_str() {
            "delivery_discrepancy" => v.sort_by(|a, b| {
                a.delivery_discrepancy
                    .partial_cmp(&b.delivery_discrepancy)
                    .unwrap_or(std::cmp::Ordering::Equal)
            }),
            "returns_discrepancy" => v.sort_by(|a, b| {
                a.returns_discrepancy
                    .partial_cmp(&b.returns_discrepancy)
                    .unwrap_or(std::cmp::Ordering::Equal)
            }),
            _ => {}
        }
        if (f == "delivery_discrepancy" || f == "returns_discrepancy") && !asc {
            v.reverse();
        }
        v
    });

    let toggle_sort = move |field: &'static str| {
        let same = sort_field.get_untracked() == field;
        if same {
            sort_ascending.update(|a| *a = !*a);
        } else {
            sort_field.set(field.to_string());
            sort_ascending.set(true);
        }
        if !is_client_sort(field) {
            load();
        }
    };

    let all_selected = Signal::derive(move || {
        let its = items.get();
        let sel = selected_ids.get();
        !its.is_empty() && its.iter().all(|i| sel.contains(&i.id))
    });
    let toggle_all = move |check: bool| {
        if check {
            let all = items.get_untracked();
            selected_ids.update(|s| {
                for it in all.iter() {
                    s.insert(it.id.clone());
                }
            });
        } else {
            selected_ids.update(|s| s.clear());
        }
    };

    let selected_count = Signal::derive(move || selected_ids.get().len());

    let post_selected = move |_: leptos::ev::MouseEvent| {
        let ids: Vec<String> = selected_ids.get_untracked().into_iter().collect();
        if ids.is_empty() {
            return;
        }
        posting.set(true);
        spawn_local(async move {
            for id in &ids {
                let url = format!("{}/api/a034/ym-realization/{}/post", api_base(), id);
                let _ = Request::post(&url).send().await;
            }
            posting.set(false);
            selected_ids.update(|s| s.clear());
            load();
        });
    };

    // Заголовок сортируемой колонки.
    let sort_header = move |label: &'static str, field: &'static str, numeric: bool| {
        let base = if numeric {
            "width:100px; cursor:pointer; user-select:none; text-align:right;"
        } else {
            "cursor:pointer; user-select:none;"
        };
        view! {
            <th class="table__header-cell" style=base on:click=move |_| toggle_sort(field)>
                {label}
                <span class=move || get_sort_class(&sort_field.get(), field)>
                    {move || get_sort_indicator(&sort_field.get(), field, sort_ascending.get())}
                </span>
            </th>
        }
    };

    view! {
        <PageFrame page_id="a034_ym_realization--list" category=PAGE_CAT_LIST>
            <div class="page__header">
                <div class="page__header-left">
                    <h1 class="page__title">"Реализация YM (Отчёт о реализации)"</h1>
                    <Badge appearance=BadgeAppearance::Filled>{move || total.get().to_string()}</Badge>
                </div>
                <div class="page__header-right">
                    <Button
                        appearance=ButtonAppearance::Primary
                        disabled=Signal::derive(move || selected_count.get() == 0 || posting.get())
                        on_click=post_selected
                    >
                        {icon("zap")}
                        {move || format!("Провести ({})", selected_count.get())}
                    </Button>
                    <Button appearance=ButtonAppearance::Secondary on_click=move |_| load()>
                        {icon("refresh")}
                        "Обновить"
                    </Button>
                </div>
            </div>
            <div class="page__content">
                <div class="filter-panel">
                    <div class="filter-panel-header">
                        <div class="filter-panel-header__left">"Фильтры"</div>
                    </div>
                    <div class="filter-panel-content">
                        <Flex gap=FlexGap::Small align=FlexAlign::End>
                            <div style="min-width: 420px;">
                                <DateRangePicker
                                    date_from=Signal::derive(move || date_from.get())
                                    date_to=Signal::derive(move || date_to.get())
                                    on_change=Callback::new(move |(from, to): (String, String)| {
                                        date_from.set(from);
                                        date_to.set(to);
                                        load();
                                    })
                                    label="Период:".to_string()
                                />
                            </div>
                            <div style="width: 280px;">
                                <Flex vertical=true gap=FlexGap::Small>
                                    <Label>"Кабинет:"</Label>
                                    <Select value=connection_id>
                                        <option value="">"Все кабинеты"</option>
                                        {move || cabinets.get().into_iter().map(|(id, label)| {
                                            view! { <option value=id>{label}</option> }
                                        }).collect_view()}
                                    </Select>
                                </Flex>
                            </div>
                        </Flex>
                    </div>
                </div>

                {move || error_msg.get().map(|err| view! { <div class="alert alert--error">{err}</div> })}
                {move || {
                    if loading.get() {
                        view! { <div class="page__placeholder"><Spinner /> " Загрузка..."</div> }.into_any()
                    } else if items.get().is_empty() {
                        view! { <div class="page__placeholder">"Нет документов. Импортируйте отчёт о реализации (u503)."</div> }.into_any()
                    } else {
                        let tabs_store = tabs_store.clone();
                        view! {
                            <div class="table-wrap" style="width:100%;">
                                <table class="table" style="width:100%;">
                                    <thead>
                                        <tr class="table__header-row">
                                            <th class="table__header-cell" style="width:36px;">
                                                <input
                                                    type="checkbox"
                                                    class="table__checkbox"
                                                    prop:checked=move || all_selected.get()
                                                    on:change=move |ev| toggle_all(event_target_checked(&ev))
                                                />
                                            </th>
                                            {sort_header("Дата", "document_date", true)}
                                            {sort_header("Кабинет", "connection_name", false)}
                                            <th class="table__header-cell">"Модель"</th>
                                            {sort_header("Строк", "lines_count", true)}
                                            {sort_header("Продажи", "total_sales_revenue", true)}
                                            {sort_header("Возвраты", "total_return_revenue", true)}
                                            {sort_header("Нетто", "net_revenue", true)}
                                            {sort_header("Расх. доставки", "delivery_discrepancy", true)}
                                            {sort_header("Расх. возвраты", "returns_discrepancy", true)}
                                        </tr>
                                    </thead>
                                    <tbody>
                                        {move || {
                                            let tabs_store = tabs_store.clone();
                                            display_items.get().into_iter().map(move |item| {
                                                let tabs_store = tabs_store.clone();
                                                let id = item.id.clone();
                                                let id_for_cb = item.id.clone();
                                                let date = item.document_date.clone();
                                                let open = move |_| {
                                                    tabs_store.open_tab(
                                                        &format!("a034_ym_realization_details_{}", id),
                                                        &format!("Реализация YM {}", date),
                                                    );
                                                };
                                                view! {
                                                    <tr class="table__row">
                                                        <td class="table__cell" style="width:36px;" on:click=|e| e.stop_propagation()>
                                                            <input
                                                                type="checkbox"
                                                                class="table__checkbox"
                                                                prop:checked=move || selected_ids.get().contains(&id_for_cb)
                                                                on:change={
                                                                    let id_cb = item.id.clone();
                                                                    move |ev| {
                                                                        let checked = event_target_checked(&ev);
                                                                        let id_cb = id_cb.clone();
                                                                        selected_ids.update(|s| { if checked { s.insert(id_cb); } else { s.remove(&id_cb); } });
                                                                    }
                                                                }
                                                            />
                                                        </td>
                                                        <td class="table__cell" style="width:100px;">
                                                            <a href="#" class="table__link" on:click=move |ev| { ev.prevent_default(); open(()); }>
                                                                {item.document_date.clone()}
                                                            </a>
                                                        </td>
                                                        <td class="table__cell">{item.connection_name.clone().unwrap_or(item.connection_id.clone())}</td>
                                                        <td class="table__cell">{item.placement_type.clone().unwrap_or_else(|| "—".to_string())}</td>
                                                        <td class="table__cell table__cell--right" style="width:100px;">{item.lines_count}</td>
                                                        <td class="table__cell table__cell--right" style="width:100px;">{format_money(item.total_sales_revenue)}</td>
                                                        <td class="table__cell table__cell--right" style="width:100px;">{format_money(item.total_return_revenue)}</td>
                                                        <td class="table__cell table__cell--right" style="width:100px;">{format_money(item.net_revenue)}</td>
                                                        <td class="table__cell" style=discrepancy_cell_style(item.delivery_discrepancy)>{format_money(item.delivery_discrepancy)}</td>
                                                        <td class="table__cell" style=discrepancy_cell_style(item.returns_discrepancy)>{format_money(item.returns_discrepancy)}</td>
                                                    </tr>
                                                }
                                            }).collect_view()
                                        }}
                                    </tbody>
                                </table>
                            </div>
                        }.into_any()
                    }
                }}
            </div>
        </PageFrame>
    }
}

#[component]
pub fn YmRealizationDetail(id: String, #[prop(into)] on_close: Callback<()>) -> impl IntoView {
    let tabs_store = use_context::<AppGlobalContext>().expect("AppGlobalContext not found");
    let doc = RwSignal::new(Option::<DetailsDto>::None);
    let journal = RwSignal::new(Vec::<GeneralLedgerEntryDto>::new());
    let error_msg = RwSignal::new(Option::<String>::None);
    let busy = RwSignal::new(false);
    let active_tab = RwSignal::new("result".to_string());
    let payments = RwSignal::new(Option::<PaymentDetailResponse>::None);
    let payments_loaded = RwSignal::new(false);
    let recon_sales = RwSignal::new(Option::<ReconResponse>::None);
    let recon_sales_loaded = RwSignal::new(false);
    let recon_returns = RwSignal::new(Option::<ReconResponse>::None);
    let recon_returns_loaded = RwSignal::new(false);
    let deliveries = RwSignal::new(Option::<DeliveryOrdersResponse>::None);
    let deliveries_loaded = RwSignal::new(false);
    let summary = RwSignal::new(Option::<ReconSummaryResponse>::None);
    let summary_loaded = RwSignal::new(false);
    // Документ id для команды «Догрузить отсутствующие заказы» (вкладка «Сверка реализации»).
    let id_for_missing = id.clone();

    // Ленивая загрузка мини-отчёта «Платежи YM» при первом открытии вкладки.
    let id_for_payments = id.clone();
    let load_payments = move || {
        if payments_loaded.get() {
            return;
        }
        payments_loaded.set(true);
        let id = id_for_payments.clone();
        spawn_local(async move {
            let url = format!(
                "{}/api/a034/ym-realization/{}/payment-detail",
                api_base(),
                id
            );
            match Request::get(&url).send().await {
                Ok(resp) if resp.ok() => {
                    if let Ok(data) = resp.json::<PaymentDetailResponse>().await {
                        payments.set(Some(data));
                    }
                }
                _ => {}
            }
        });
    };
    // Ленивая загрузка «Сверки реализации» при первом открытии вкладки.
    let id_for_recon_sales = id.clone();
    let load_recon_sales = move || {
        if recon_sales_loaded.get() {
            return;
        }
        recon_sales_loaded.set(true);
        let id = id_for_recon_sales.clone();
        spawn_local(async move {
            let url = format!(
                "{}/api/a034/ym-realization/{}/reconciliation-sales",
                api_base(),
                id
            );
            match Request::get(&url).send().await {
                Ok(resp) if resp.ok() => {
                    if let Ok(data) = resp.json::<ReconResponse>().await {
                        recon_sales.set(Some(data));
                    }
                }
                _ => {}
            }
        });
    };
    // Ленивая загрузка «Сверки возвратов» при первом открытии вкладки.
    let id_for_recon_returns = id.clone();
    let load_recon_returns = move || {
        if recon_returns_loaded.get() {
            return;
        }
        recon_returns_loaded.set(true);
        let id = id_for_recon_returns.clone();
        spawn_local(async move {
            let url = format!(
                "{}/api/a034/ym-realization/{}/reconciliation-returns",
                api_base(),
                id
            );
            match Request::get(&url).send().await {
                Ok(resp) if resp.ok() => {
                    if let Ok(data) = resp.json::<ReconResponse>().await {
                        recon_returns.set(Some(data));
                    }
                }
                _ => {}
            }
        });
    };
    // Ленивая загрузка отчёта «Заказы (дата доставки)» при первом открытии вкладки.
    let id_for_deliveries = id.clone();
    let load_deliveries = move || {
        if deliveries_loaded.get() {
            return;
        }
        deliveries_loaded.set(true);
        let id = id_for_deliveries.clone();
        spawn_local(async move {
            let url = format!(
                "{}/api/a034/ym-realization/{}/delivery-orders",
                api_base(),
                id
            );
            match Request::get(&url).send().await {
                Ok(resp) if resp.ok() => {
                    if let Ok(data) = resp.json::<DeliveryOrdersResponse>().await {
                        deliveries.set(Some(data));
                    }
                }
                _ => {}
            }
        });
    };
    // Ленивая загрузка сводных таблиц сравнения (вкладка «Итоги»).
    let id_for_summary = id.clone();
    let load_summary = move || {
        if summary_loaded.get() {
            return;
        }
        summary_loaded.set(true);
        let id = id_for_summary.clone();
        spawn_local(async move {
            let url = format!(
                "{}/api/a034/ym-realization/{}/reconciliation-summary",
                api_base(),
                id
            );
            match Request::get(&url).send().await {
                Ok(resp) if resp.ok() => {
                    if let Ok(data) = resp.json::<ReconSummaryResponse>().await {
                        summary.set(Some(data));
                    }
                }
                _ => {}
            }
        });
    };
    {
        let load_payments = load_payments.clone();
        let load_recon_sales = load_recon_sales.clone();
        let load_recon_returns = load_recon_returns.clone();
        let load_deliveries = load_deliveries.clone();
        let load_summary = load_summary.clone();
        Effect::new(move |_| match active_tab.get().as_str() {
            "payments" => load_payments(),
            "deliveries" => load_deliveries(),
            "compare_sales" => load_recon_sales(),
            "compare_returns" => load_recon_returns(),
            // Сводные таблицы сравнения теперь на вкладке «Результат».
            "result" => load_summary(),
            _ => {}
        });
    }

    let id_for_load = id.clone();
    let load = move || {
        let id = id_for_load.clone();
        spawn_local(async move {
            error_msg.set(None);
            let url = format!("{}/api/a034/ym-realization/{}", api_base(), id);
            match Request::get(&url).send().await {
                Ok(resp) if resp.ok() => match resp.json::<DetailsDto>().await {
                    Ok(data) => doc.set(Some(data)),
                    Err(e) => error_msg.set(Some(format!("Ошибка разбора: {e}"))),
                },
                Ok(resp) => error_msg.set(Some(format!("HTTP {}", resp.status()))),
                Err(e) => error_msg.set(Some(format!("Ошибка запроса: {e}"))),
            }
            let jurl = format!("{}/api/a034/ym-realization/{}/journal", api_base(), id);
            if let Ok(resp) = Request::get(&jurl).send().await {
                if resp.ok() {
                    if let Ok(jr) = resp.json::<JournalResponse>().await {
                        journal.set(jr.general_ledger_entries);
                    }
                }
            }
        });
    };

    {
        let load = load.clone();
        Effect::new(move |_| load());
    }

    let id_for_post = id.clone();
    let post = {
        let load_summary = load_summary.clone();
        move |_| {
            let id = id_for_post.clone();
            let load2 = load.clone();
            let load_summary = load_summary.clone();
            spawn_local(async move {
                busy.set(true);
                let url = format!("{}/api/a034/ym-realization/{}/post", api_base(), id);
                let _ = Request::post(&url).send().await;
                busy.set(false);
                // Перезагрузить документ + журнал: на проведении бэкенд пересчитывает
                // итоги (totals) из строк и переписывает GL-проводки.
                load2();
                // Пересчитать сводки расхождений (доставки/возвраты) на вкладке
                // «Результат» и сбросить кэш вкладок сверки, чтобы при их открытии
                // данные перезапросились с актуальными a013/a016.
                summary.set(None);
                summary_loaded.set(false);
                load_summary();
                recon_sales.set(None);
                recon_sales_loaded.set(false);
                recon_returns.set(None);
                recon_returns_loaded.set(false);
                deliveries.set(None);
                deliveries_loaded.set(false);
                payments.set(None);
                payments_loaded.set(false);
            });
        }
    };

    let favorite_tab_key = format!("a034_ym_realization_details_{}", id);
    let favorite_title = Signal::derive(move || {
        doc.get()
            .map(|d| format!("Реализация YM {}", d.document_date))
            .unwrap_or_else(|| "Реализация YM".to_string())
    });
    let fav_key_for_btn = favorite_tab_key.clone();

    view! {
        <PageFrame page_id="a034_ym_realization--details" category=PAGE_CAT_LIST>
            <div class="page__header">
                <div class="page__header-left">
                    <FavoriteButton
                        target_kind="a034_ym_realization_details".to_string()
                        target_id=fav_key_for_btn.clone()
                        target_title=favorite_title
                        tab_key=fav_key_for_btn
                    />
                    <h1 class="page__title">{move || favorite_title.get()}</h1>
                    {move || doc.get().map(|d| {
                        if d.is_posted {
                            view! {
                                <Badge appearance=BadgeAppearance::Filled color=BadgeColor::Success>"Проведён"</Badge>
                            }.into_any()
                        } else {
                            view! {
                                <Badge appearance=BadgeAppearance::Outline color=BadgeColor::Subtle>"Не проведён"</Badge>
                            }.into_any()
                        }
                    })}
                </div>
                <div class="page__header-right">
                    <Button
                        appearance=ButtonAppearance::Subtle
                        size=ButtonSize::Medium
                        disabled=Signal::derive(move || busy.get())
                        on_click=post
                    >
                        <span class="page-action-button__content">
                            <span class="page-action-button__icon">{icon("zap")}</span>
                            <span class="page-action-button__text">"Провести"</span>
                        </span>
                    </Button>
                    <Button
                        appearance=ButtonAppearance::Subtle
                        size=ButtonSize::Medium
                        on_click=move |_| on_close.run(())
                    >
                        <span class="page-action-button__content">
                            <span class="page-action-button__icon page-action-button__icon--close">{icon("x")}</span>
                            <span class="page-action-button__text">"Закрыть"</span>
                        </span>
                    </Button>
                </div>
            </div>

            {move || error_msg.get().map(|err| view! { <div class="alert alert--error">{err}</div> })}

            <div class="page__content">
                <div style="border-bottom: 1px solid var(--color-border);">
                    <TabList selected_value=active_tab>
                        <Tab value="result".to_string()>"Результат"</Tab>
                        <Tab value="general_ledger".to_string()>"Журнал операций"</Tab>
                        <Tab value="sales".to_string()>"Реализации"</Tab>
                        <Tab value="returns".to_string()>"Возвраты"</Tab>
                        <Tab value="payments".to_string()>"Платежи YM (p907)"</Tab>
                        <Tab value="deliveries".to_string()>"Заказы (дата доставки)"</Tab>
                        <Tab value="compare_sales".to_string()>"Сверка реализации"</Tab>
                        <Tab value="compare_returns".to_string()>"Сверка возвратов"</Tab>
                    </TabList>
                </div>

                {move || {
                    let Some(d) = doc.get() else {
                        return view! { <div class="page__placeholder"><Spinner /> " Загрузка..."</div> }.into_any();
                    };
                    let id_for_missing = id_for_missing.clone();
                    match active_tab.get().as_str() {
                        "general_ledger" => {
                            let entries = Signal::derive(move || journal.get());
                            view! {
                                {tab_hint("GL-проводки документа (слой ybuh), сформированные при проведении.")}
                                <DocumentGeneralLedgerEntries
                                    entries=entries
                                    loading=Signal::derive(|| false)
                                    error=Signal::derive(|| None::<String>)
                                    nav_id=document_general_ledger_entries_nav_id("a034_ym_realization")
                                    title="Журнал операций"
                                    empty_message="Нет проводок. Проведите документ для формирования GL."
                                />
                            }.into_any()
                        }
                        "payments" => view! {
                            {tab_hint("Справочно: платежи покупателей и возвраты из p907 (слой fina) за дату доставки документа.")}
                            <PaymentsReport data=payments tabs_store=tabs_store.clone() />
                        }.into_any(),
                        "deliveries" => view! {
                            {tab_hint("Позиции заказов a013, доставленных в дату документа (строится на лету). Сумма = цена покупателя × количество.")}
                            <DeliveryOrdersReport data=deliveries tabs_store=tabs_store.clone() />
                        }.into_any(),
                        "compare_sales" => view! {
                            {tab_hint("Построчная сверка реализаций (a034) с заказами a013, доставленными в дату документа. Кнопкой можно догрузить отсутствующие в системе заказы.")}
                            <MissingOrdersButton id=id_for_missing.clone() recon_sales=recon_sales summary=summary />
                            <ReconReport data=recon_sales tabs_store=tabs_store.clone() cmp_label="заказы".to_string() status_label="Статус заказа".to_string() />
                        }.into_any(),
                        "compare_returns" => view! {
                            {tab_hint("Построчная сверка возвратов из отчёта (a034) с возвратами a016 по тем же заказам.")}
                            <ReconReport data=recon_returns tabs_store=tabs_store.clone() cmp_label="a016".to_string() status_label="Статус возврата".to_string() />
                        }.into_any(),
                        "sales" => view! {
                            {tab_hint("Строки продаж из отчёта о реализации YM (слой ybuh): заказ, товар, SKU, количество и выручка.")}
                            <RealizationLinesTable
                                lines=d.sales_lines.clone()
                                product_names=d.product_names.clone()
                                tabs_store=tabs_store.clone()
                                is_return=false
                            />
                        }.into_any(),
                        "returns" => view! {
                            {tab_hint("Строки возвратов из отчёта о реализации YM: заказ, товар, SKU, количество и сумма возврата.")}
                            <RealizationLinesTable
                                lines=d.return_lines.clone()
                                product_names=d.product_names.clone()
                                tabs_store=tabs_store.clone()
                                is_return=true
                            />
                        }.into_any(),
                        _ => view! { <ResultTab doc=d.clone() summary=summary /> }.into_any(),
                    }
                }}
            </div>
        </PageFrame>
    }
}

/// Короткий поясняющий комментарий о содержимом вкладки (над таблицами).
fn tab_hint(text: &'static str) -> AnyView {
    view! {
        <div style="padding: var(--spacing-xs) var(--spacing-md); color: var(--color-text-secondary); font-size: 0.86em; font-style: italic;">
            {text}
        </div>
    }
    .into_any()
}

/// Вкладка «Результат»: реквизиты и итоги документа (одним блоком) и две сводные
/// таблицы сравнения за день (Доставки и Возвраты). Журнал GL — на отдельной
/// вкладке. Блоки-карточки в стиле a033_wb_day_close_details, ширина ≤ 900px.
#[component]
fn ResultTab(doc: DetailsDto, summary: RwSignal<Option<ReconSummaryResponse>>) -> impl IntoView {
    view! {
        <div style="padding: var(--spacing-md); display:flex; flex-direction:column; gap: var(--spacing-md); max-width:900px;">
            <CardAnimated delay_ms=0 nav_id="a034_result_info">
                <h4 class="details-section__title">"Документ"</h4>
                <table class="data-table" style="width:100%;">
                    <tbody>
                        <tr><td>"Документ"</td><td>{doc.document_no.clone()}</td></tr>
                        <tr><td>"Кабинет"</td><td>{doc.connection_name.clone().unwrap_or_default()}</td></tr>
                        <tr><td>"Модель"</td><td>{doc.placement_type.clone().unwrap_or_else(|| "—".to_string())}</td></tr>
                        <tr><td>"campaignId"</td><td>{doc.campaign_id.clone().unwrap_or_else(|| "—".to_string())}</td></tr>
                        <tr><td>"Организация"</td><td>{doc.organization_name.clone().unwrap_or_default()}</td></tr>
                        <tr><td>"Источник"</td><td>{format!("{} ({})", doc.source, doc.fetched_at)}</td></tr>
                        <tr><td>"Проведён"</td><td>{if doc.is_posted { "да" } else { "нет" }}</td></tr>
                        <tr style="border-top:2px solid var(--color-border);">
                            <td>"Продажи"</td>
                            <td style="text-align:right;">{format_money(doc.total_sales_revenue)}</td>
                        </tr>
                        <tr>
                            <td>"Возвраты"</td>
                            <td style="text-align:right;">{format_money(doc.total_return_revenue)}</td>
                        </tr>
                        <tr style="font-weight:600;">
                            <td>"Нетто-выручка"</td>
                            <td style="text-align:right;">{format_money(doc.net_revenue)}</td>
                        </tr>
                    </tbody>
                </table>
            </CardAnimated>

            <CardAnimated delay_ms=40 nav_id="a034_result_deliveries">
                <h4 class="details-section__title">"Сводка сравнения: Доставки"</h4>
                {move || match summary.get() {
                    Some(s) => render_summary_table(s.deliveries),
                    None => view! { <div class="page__placeholder"><Spinner /> " Загрузка..."</div> }.into_any(),
                }}
            </CardAnimated>

            <CardAnimated delay_ms=80 nav_id="a034_result_returns">
                <h4 class="details-section__title">"Сводка сравнения: Возвраты"</h4>
                {move || match summary.get() {
                    Some(s) => render_summary_table(s.returns),
                    None => view! { <div class="page__placeholder"><Spinner /> " Загрузка..."</div> }.into_any(),
                }}
            </CardAnimated>
        </div>
    }
}

/// Вертикальный отступ ячеек сводных таблиц: строки на 2px выше (по 2px сверху/снизу).
const SUMMARY_CELL_PAD: &str = "padding:2px 8px;";

/// Подсветка чередующихся строк — лёгкий серый, читаемый в светлой и тёмной темах.
fn summary_row_style(idx: usize) -> &'static str {
    if idx % 2 == 1 {
        "background: rgba(128, 128, 128, 0.08);"
    } else {
        ""
    }
}

/// Подсветка колонок разницы: значимое отклонение — оранжевым, иначе зелёным.
/// Включает вертикальный отступ ячейки сводной таблицы.
fn diff_style(value: f64) -> &'static str {
    if value.abs() > 0.5 {
        "text-align:right; padding:2px 8px; color: var(--color-warning); font-weight:600;"
    } else {
        "text-align:right; padding:2px 8px; color: var(--color-success);"
    }
}

/// Количество без «нулевого шума»: ~0 (включая -0) → пусто.
fn fmt_qty_blank(value: f64) -> String {
    if value.abs() < 0.5 {
        String::new()
    } else {
        format!("{:.0}", value)
    }
}

/// Сумма без «нулевого шума»: ~0 (включая -0.00) → пусто.
fn fmt_money_blank(value: f64) -> String {
    if value.abs() < 0.005 {
        String::new()
    } else {
        format_money(value)
    }
}

/// Сводная таблица сравнения отчёт/заказы. Строки = группы детальной сверки
/// (те же, что на вкладке) + Итоги; колонки = кол-во/сумма отчёта, кол-во/сумма
/// заказов, кол-во/сумма разницы.
fn render_summary_table(rows: Vec<ReconSummaryRow>) -> AnyView {
    let mut t_rq = 0.0;
    let mut t_rs = 0.0;
    let mut t_oq = 0.0;
    let mut t_os = 0.0;
    for r in &rows {
        t_rq += r.report_qty;
        t_rs += r.report_sum;
        t_oq += r.orders_qty;
        t_os += r.orders_sum;
    }
    let body = rows
        .into_iter()
        .enumerate()
        .map(|(idx, r)| {
            let qd = r.report_qty - r.orders_qty;
            let sd = r.report_sum - r.orders_sum;
            let num_cell = format!("text-align:right; {}", SUMMARY_CELL_PAD);
            view! {
                <tr style=summary_row_style(idx)>
                    <td style=SUMMARY_CELL_PAD>{r.category.clone()}</td>
                    <td style=num_cell.clone()>{fmt_qty_blank(r.report_qty)}</td>
                    <td style=num_cell.clone()>{fmt_money_blank(r.report_sum)}</td>
                    <td style=num_cell.clone()>{fmt_qty_blank(r.orders_qty)}</td>
                    <td style=num_cell.clone()>{fmt_money_blank(r.orders_sum)}</td>
                    <td style=diff_style(qd)>{fmt_qty_blank(qd)}</td>
                    <td style=diff_style(sd)>{fmt_money_blank(sd)}</td>
                </tr>
            }
        })
        .collect_view();
    let tqd = t_rq - t_oq;
    let tsd = t_rs - t_os;
    view! {
        <div class="table-wrap" style="width:100%;">
            <table class="data-table" style="width:100%;">
                <thead>
                    <tr>
                        <th style=SUMMARY_CELL_PAD></th>
                        <th style="text-align:right; padding:2px 8px;">"Кол-во отчёт"</th>
                        <th style="text-align:right; padding:2px 8px;">"Сумма отчёт"</th>
                        <th style="text-align:right; padding:2px 8px;">"Кол-во заказы"</th>
                        <th style="text-align:right; padding:2px 8px;">"Сумма заказы"</th>
                        <th style="text-align:right; padding:2px 8px;">"Кол-во разница"</th>
                        <th style="text-align:right; padding:2px 8px;">"Сумма разница"</th>
                    </tr>
                </thead>
                <tbody>
                    {body}
                    <tr style="font-weight:600; border-top:2px solid var(--color-border);">
                        <td style=SUMMARY_CELL_PAD>"Итоги"</td>
                        <td style="text-align:right; padding:2px 8px;">{fmt_qty_blank(t_rq)}</td>
                        <td style="text-align:right; padding:2px 8px;">{fmt_money_blank(t_rs)}</td>
                        <td style="text-align:right; padding:2px 8px;">{fmt_qty_blank(t_oq)}</td>
                        <td style="text-align:right; padding:2px 8px;">{fmt_money_blank(t_os)}</td>
                        <td style=diff_style(tqd)>{fmt_qty_blank(tqd)}</td>
                        <td style=diff_style(tsd)>{fmt_money_blank(tsd)}</td>
                    </tr>
                </tbody>
            </table>
        </div>
    }
    .into_any()
}

/// Таблица строк документа a034. Продажи и возвраты приходят уже разделёнными
/// коллекциями (`sales_lines` / `return_lines`); `is_return` влияет только на
/// подписи. Колонки: заказ, товар (a007), SKU, наименование, кол-во, сумма.
#[component]
fn RealizationLinesTable(
    lines: Vec<YmRealizationLine>,
    product_names: std::collections::HashMap<String, String>,
    tabs_store: AppGlobalContext,
    is_return: bool,
) -> impl IntoView {
    let mut lines = lines;
    // «По заказам и номенклатуре»: группируем визуально сортировкой.
    lines.sort_by(|a, b| {
        a.order_id
            .cmp(&b.order_id)
            .then_with(|| a.shop_sku.cmp(&b.shop_sku))
    });
    let amount_header = if is_return {
        "Сумма возврата"
    } else {
        "Выручка"
    };
    let total: f64 = lines.iter().map(|l| l.revenue_amount).sum();
    let total_qty: f64 = lines.iter().map(|l| l.quantity).sum();
    if lines.is_empty() {
        let label = if is_return {
            "возвратов"
        } else {
            "реализаций"
        };
        return view! { <div class="page__placeholder">{format!("Нет строк {} в документе.", label)}</div> }.into_any();
    }
    view! {
        <div class="table-wrap" style="width:100%;">
            <table class="table" style="width:100%;">
                <thead>
                    <tr class="table__header-row">
                        <th class="table__header-cell">"Заказ"</th>
                        <th class="table__header-cell">"Товар (a007)"</th>
                        <th class="table__header-cell">"SKU"</th>
                        <th class="table__header-cell">"Наименование"</th>
                        <th class="table__header-cell">"Кол-во"</th>
                        <th class="table__header-cell">{amount_header}</th>
                    </tr>
                </thead>
                <tbody>
                    {lines.into_iter().map(|l| {
                        let tabs_store = tabs_store.clone();
                        let mp_ref = l.marketplace_product_ref.clone();
                        let label = mp_ref
                            .as_deref()
                            .and_then(|r| product_names.get(r).cloned())
                            .unwrap_or_else(|| l.your_sku.clone().unwrap_or_default());
                        view! {
                            <tr class="table__row">
                                <td class="table__cell">{order_link(tabs_store.clone(), l.order_id.clone().unwrap_or_default())}</td>
                                <td class="table__cell">{product_link(tabs_store, mp_ref, label)}</td>
                                <td class="table__cell">{l.shop_sku}</td>
                                <td class="table__cell">{l.offer_name}</td>
                                <td class="table__cell table__cell--right">{format!("{:.0}", l.quantity)}</td>
                                <td class="table__cell table__cell--right">{format_money(l.revenue_amount)}</td>
                            </tr>
                        }
                    }).collect_view()}
                </tbody>
                <tfoot>
                    <tr class="table__row">
                        <td class="table__cell" colspan="4"><b>"Итого"</b></td>
                        <td class="table__cell table__cell--right"><b>{format!("{:.0}", total_qty)}</b></td>
                        <td class="table__cell table__cell--right"><b>{format_money(total)}</b></td>
                    </tr>
                </tfoot>
            </table>
        </div>
    }.into_any()
}

/// Мини-отчёт «Платежи YM (p907)»: доставленные заказы и возвраты за день
/// документа (fina-сторона). Колонки: тип, заказ, номенклатура, дата оплаты,
/// сумма/кол-во возврата и выручки; внизу — итоги.
#[component]
fn PaymentsReport(
    data: RwSignal<Option<PaymentDetailResponse>>,
    tabs_store: AppGlobalContext,
) -> impl IntoView {
    view! {
        {move || {
            let tabs_store = tabs_store.clone();
            let Some(resp) = data.get() else {
                return view! { <div class="page__placeholder"><Spinner /> " Загрузка..."</div> }.into_any();
            };
            if resp.rows.is_empty() {
                return view! { <div class="page__placeholder">"Нет строк p907 за этот кабинет/день доставки."</div> }.into_any();
            }
            let totals = resp.totals.clone();
            view! {
                <div class="table-wrap" style="width:100%;">
                    <table class="table" style="width:100%;">
                        <thead>
                            <tr class="table__header-row">
                                <th class="table__header-cell">"Тип"</th>
                                <th class="table__header-cell">"Заказ"</th>
                                <th class="table__header-cell">"Товар (a007)"</th>
                                <th class="table__header-cell">"Номенклатура"</th>
                                <th class="table__header-cell">"Дата оплаты"</th>
                                <th class="table__header-cell">"Сумма возврат"</th>
                                <th class="table__header-cell">"Сумма выручка"</th>
                                <th class="table__header-cell">"Кол-во возврат"</th>
                                <th class="table__header-cell">"Кол-во выручка"</th>
                            </tr>
                        </thead>
                        <tbody>
                            {resp.rows.clone().into_iter().map(|r| {
                                let tabs_store = tabs_store.clone();
                                let prod_label = r.product_name.clone().unwrap_or_else(|| r.nomenclature.clone());
                                view! {
                                    <tr class="table__row">
                                        <td class="table__cell">{r.kind}</td>
                                        <td class="table__cell">{order_link(tabs_store.clone(), r.order_id.unwrap_or_default())}</td>
                                        <td class="table__cell">{product_link(tabs_store, r.marketplace_product_ref.clone(), prod_label)}</td>
                                        <td class="table__cell">{r.nomenclature}</td>
                                        <td class="table__cell">{r.payment_date.unwrap_or_default()}</td>
                                        <td class="table__cell table__cell--right">{if r.return_amount != 0.0 { format_money(r.return_amount) } else { String::new() }}</td>
                                        <td class="table__cell table__cell--right">{if r.revenue_amount != 0.0 { format_money(r.revenue_amount) } else { String::new() }}</td>
                                        <td class="table__cell table__cell--right">{if r.return_qty != 0.0 { format!("{:.0}", r.return_qty) } else { String::new() }}</td>
                                        <td class="table__cell table__cell--right">{if r.revenue_qty != 0.0 { format!("{:.0}", r.revenue_qty) } else { String::new() }}</td>
                                    </tr>
                                }
                            }).collect_view()}
                        </tbody>
                        <tfoot>
                            <tr class="table__row">
                                <td class="table__cell" colspan="5"><b>"Итого"</b></td>
                                <td class="table__cell table__cell--right"><b>{format_money(totals.return_amount)}</b></td>
                                <td class="table__cell table__cell--right"><b>{format_money(totals.revenue_amount)}</b></td>
                                <td class="table__cell table__cell--right"><b>{format!("{:.0}", totals.return_qty)}</b></td>
                                <td class="table__cell table__cell--right"><b>{format!("{:.0}", totals.revenue_qty)}</b></td>
                            </tr>
                        </tfoot>
                    </table>
                </div>
            }.into_any()
        }}
    }
}

/// Отчёт «Сверка»: реализация (ybuh, a034) против заказов (a013), доставленных в
/// дату документа. Блоки: Совпадение / Расходятся / Частичная (неполная) доставка /
/// Нет среди заказов / Нет в реализации / Не распознано. 1-я колонка строки-заголовка
/// блока = название блока + итоги; далее строки ключей (заказ+SKU) с суммами/кол-вом.
#[component]
fn ReconReport(
    data: RwSignal<Option<ReconResponse>>,
    tabs_store: AppGlobalContext,
    /// Подпись правой стороны сравнения: "заказы" (сверка реализации) или "a016"
    /// (сверка возвратов). Влияет на заголовки "Сумма {}" / "Кол-во {}".
    cmp_label: String,
    /// Заголовок колонки статуса: "Статус заказа" или "Статус возврата".
    status_label: String,
) -> impl IntoView {
    view! {
        {move || {
            let tabs_store = tabs_store.clone();
            let cmp_label = cmp_label.clone();
            let status_label = status_label.clone();
            let Some(resp) = data.get() else {
                return view! { <div class="page__placeholder"><Spinner /> " Загрузка..."</div> }.into_any();
            };
            view! {
                <div class="table-wrap" style="width:100%;">
                    <table class="table" style="width:100%;">
                        <thead>
                            <tr class="table__header-row">
                                <th class="table__header-cell">"Блок"</th>
                                <th class="table__header-cell">"Заказ"</th>
                                <th class="table__header-cell">"Товар (a007)"</th>
                                <th class="table__header-cell">"SKU"</th>
                                <th class="table__header-cell">"Дата доставки"</th>
                                <th class="table__header-cell">{status_label}</th>
                                <th class="table__header-cell">"Сумма реализ."</th>
                                <th class="table__header-cell">{format!("Сумма {}", cmp_label)}</th>
                                <th class="table__header-cell">"Δ сумма"</th>
                                <th class="table__header-cell">"Кол-во реализ."</th>
                                <th class="table__header-cell">{format!("Кол-во {}", cmp_label)}</th>
                                <th class="table__header-cell">"Δ кол-во"</th>
                            </tr>
                        </thead>
                        <tbody>
                            {resp.groups.clone().into_iter().map(|g| {
                                let tabs_store = tabs_store.clone();
                                let header = view! {
                                    <tr class="table__row" style="background:var(--colorNeutralBackground3, #f0f0f0);">
                                        <td class="table__cell" colspan="6"><b>{format!("{} ({})", g.category, g.count)}</b></td>
                                        <td class="table__cell table__cell--right"><b>{format_money(g.ybuh_total)}</b></td>
                                        <td class="table__cell table__cell--right"><b>{format_money(g.order_total)}</b></td>
                                        <td class="table__cell table__cell--right"><b>{format_money(g.delta_total)}</b></td>
                                        <td class="table__cell"></td>
                                        <td class="table__cell"></td>
                                        <td class="table__cell"></td>
                                    </tr>
                                };
                                let rows = g.rows.into_iter().map(move |r| {
                                    let tabs_store = tabs_store.clone();
                                    let prod_label = r.product_name.clone().unwrap_or_else(|| r.nomenclature.clone());
                                    view! {
                                        <tr class="table__row">
                                            <td class="table__cell"></td>
                                            <td class="table__cell">{order_link(tabs_store.clone(), r.order_id)}</td>
                                            <td class="table__cell">{product_link(tabs_store, r.marketplace_product_ref.clone(), prod_label)}</td>
                                            <td class="table__cell">{r.shop_sku}</td>
                                            <td class="table__cell">{r.order_delivery_date.map(|d| d.chars().take(10).collect::<String>()).unwrap_or_default()}</td>
                                            <td class="table__cell">{r.order_status.unwrap_or_default()}</td>
                                            <td class="table__cell table__cell--right">{format_money(r.ybuh_amount)}</td>
                                            <td class="table__cell table__cell--right">{format_money(r.order_amount)}</td>
                                            <td class="table__cell table__cell--right">{format_money(r.amount_delta)}</td>
                                            <td class="table__cell table__cell--right">{format!("{:.0}", r.ybuh_qty)}</td>
                                            <td class="table__cell table__cell--right">{format!("{:.0}", r.order_qty)}</td>
                                            <td class="table__cell table__cell--right">{format!("{:.0}", r.qty_delta)}</td>
                                        </tr>
                                    }
                                }).collect_view();
                                view! { {header} {rows} }
                            }).collect_view()}
                        </tbody>
                    </table>
                </div>
            }.into_any()
        }}
    }
}

/// Кнопка «Догрузить отсутствующие заказы» для вкладки «Сверка реализации».
/// Берёт документ реализации, на сервере определяет упомянутые в нём заказы,
/// отсутствующие в системе (a013), и догружает их через YM API. После успеха
/// перезапрашивает сверку (блок «Нет среди заказов») и сводку доставок/возвратов
/// на вкладке «Результат» — т.к. появление заказов меняет расхождения.
#[component]
fn MissingOrdersButton(
    id: String,
    recon_sales: RwSignal<Option<ReconResponse>>,
    summary: RwSignal<Option<ReconSummaryResponse>>,
) -> impl IntoView {
    let busy = RwSignal::new(false);
    let msg = RwSignal::new(Option::<String>::None);
    let is_err = RwSignal::new(false);

    let on_click = move |_| {
        let id = id.clone();
        spawn_local(async move {
            busy.set(true);
            msg.set(None);
            is_err.set(false);
            let url = format!(
                "{}/api/a034/ym-realization/{}/fetch-missing-orders",
                api_base(),
                id
            );
            match Request::post(&url).send().await {
                Ok(resp) if resp.ok() => match resp.json::<FetchMissingOrdersResult>().await {
                    Ok(r) => {
                        is_err.set(r.failed > 0);
                        let mut text = format!(
                            "Отсутствовало: {}, догружено: {}, ошибок: {}",
                            r.total_missing, r.fetched, r.failed
                        );
                        if !r.errors.is_empty() {
                            text.push_str(" — ");
                            text.push_str(&r.errors.join("; "));
                        }
                        msg.set(Some(text));
                        // Перезагрузить сверку реализации (блок «Нет среди заказов» сократится).
                        let rurl = format!(
                            "{}/api/a034/ym-realization/{}/reconciliation-sales",
                            api_base(),
                            id
                        );
                        if let Ok(rr) = Request::get(&rurl).send().await {
                            if rr.ok() {
                                if let Ok(data) = rr.json::<ReconResponse>().await {
                                    recon_sales.set(Some(data));
                                }
                            }
                        }
                        // Пересчитать сводку доставок/возвратов на вкладке «Результат».
                        let surl = format!(
                            "{}/api/a034/ym-realization/{}/reconciliation-summary",
                            api_base(),
                            id
                        );
                        if let Ok(sr) = Request::get(&surl).send().await {
                            if sr.ok() {
                                if let Ok(data) = sr.json::<ReconSummaryResponse>().await {
                                    summary.set(Some(data));
                                }
                            }
                        }
                    }
                    Err(e) => {
                        is_err.set(true);
                        msg.set(Some(format!("Ошибка разбора ответа: {e}")));
                    }
                },
                Ok(resp) => {
                    is_err.set(true);
                    msg.set(Some(format!("HTTP {}", resp.status())));
                }
                Err(e) => {
                    is_err.set(true);
                    msg.set(Some(format!("Ошибка запроса: {e}")));
                }
            }
            busy.set(false);
        });
    };

    view! {
        <div style="display:flex; align-items:center; gap:12px; margin-bottom:12px;">
            <Button
                appearance=ButtonAppearance::Primary
                disabled=Signal::derive(move || busy.get())
                on_click=on_click
            >
                {move || if busy.get() { "Загрузка..." } else { "Догрузить отсутствующие заказы" }}
            </Button>
            {move || msg.get().map(|m| {
                let cls = if is_err.get() { "alert alert--error" } else { "alert alert--success" };
                view! { <span class=cls>{m}</span> }
            })}
        </div>
    }
}

/// Отчёт «Заказы (дата доставки)»: все позиции заказов кабинета с датой доставки =
/// дате документа. Сумма = buyer_price * qty. Строится на лету, не хранится.
#[component]
fn DeliveryOrdersReport(
    data: RwSignal<Option<DeliveryOrdersResponse>>,
    tabs_store: AppGlobalContext,
) -> impl IntoView {
    view! {
        {move || {
            let tabs_store = tabs_store.clone();
            let Some(resp) = data.get() else {
                return view! { <div class="page__placeholder"><Spinner /> " Загрузка..."</div> }.into_any();
            };
            if resp.rows.is_empty() {
                return view! { <div class="page__placeholder">"Нет заказов с датой доставки = дате документа."</div> }.into_any();
            }
            let totals = resp.totals.clone();
            view! {
                <div class="table-wrap" style="width:100%;">
                    <table class="table" style="width:100%;">
                        <thead>
                            <tr class="table__header-row">
                                <th class="table__header-cell">"Заказ"</th>
                                <th class="table__header-cell">"Статус"</th>
                                <th class="table__header-cell">"Товар (a007)"</th>
                                <th class="table__header-cell">"SKU"</th>
                                <th class="table__header-cell">"Наименование"</th>
                                <th class="table__header-cell">"Кол-во"</th>
                                <th class="table__header-cell">"Цена покупателя"</th>
                                <th class="table__header-cell">"Сумма"</th>
                            </tr>
                        </thead>
                        <tbody>
                            {resp.rows.clone().into_iter().map(|r| {
                                let tabs_store = tabs_store.clone();
                                let prod_label = r.product_name.clone().unwrap_or_else(|| r.nomenclature.clone());
                                view! {
                                    <tr class="table__row">
                                        <td class="table__cell">{order_link(tabs_store.clone(), r.order_no)}</td>
                                        <td class="table__cell">{r.status_norm.unwrap_or_default()}</td>
                                        <td class="table__cell">{product_link(tabs_store, r.marketplace_product_ref.clone(), prod_label)}</td>
                                        <td class="table__cell">{r.shop_sku}</td>
                                        <td class="table__cell">{r.nomenclature}</td>
                                        <td class="table__cell table__cell--right">{format!("{:.0}", r.qty)}</td>
                                        <td class="table__cell table__cell--right">{format_money(r.buyer_price)}</td>
                                        <td class="table__cell table__cell--right">{format_money(r.amount)}</td>
                                    </tr>
                                }
                            }).collect_view()}
                        </tbody>
                        <tfoot>
                            <tr class="table__row">
                                <td class="table__cell" colspan="5"><b>"Итого"</b></td>
                                <td class="table__cell table__cell--right"><b>{format!("{:.0}", totals.qty)}</b></td>
                                <td class="table__cell"></td>
                                <td class="table__cell table__cell--right"><b>{format_money(totals.amount)}</b></td>
                            </tr>
                        </tfoot>
                    </table>
                </div>
            }.into_any()
        }}
    }
}
