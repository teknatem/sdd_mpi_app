//! Фронтенд агрегата a035_ym_settlement_recon (Сверка перечислений YM).
//! Список документов-ордеров с командой «Сформировать ордера» и карточка с
//! таблицей оборотов, где теоретическая сумма сверяется с фактом YM (bank_sum).

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
use contracts::domain::a035_ym_settlement_recon::aggregate::ReconLine;
use contracts::domain::common::AggregateId;
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
    bank_order_id: i64,
    bank_order_date: String,
    period_from: String,
    period_to: String,
    connection_id: String,
    connection_name: Option<String>,
    bank_sum: f64,
    theoretical_sum: f64,
    deviation: f64,
    #[serde(default)]
    rows_count: i64,
    #[serde(default)]
    model: String,
    #[serde(default)]
    is_posted: bool,
}

#[derive(Debug, Clone, Deserialize)]
struct ListResponse {
    items: Vec<ListItem>,
    total: usize,
}

#[derive(Debug, Clone, Deserialize)]
struct GenerateResponse {
    created: usize,
    updated: usize,
    total: usize,
}

#[derive(Debug, Clone, Deserialize)]
struct DetailsDto {
    bank_order_id: i64,
    bank_order_date: String,
    period_from: String,
    period_to: String,
    connection_name: Option<String>,
    organization_name: Option<String>,
    bank_sum: f64,
    theoretical_sum: f64,
    deviation: f64,
    #[serde(default)]
    model: String,
    #[serde(default)]
    is_posted: bool,
    #[serde(default)]
    created_at: String,
    #[serde(default)]
    updated_at: String,
    #[serde(default)]
    lines: Vec<ReconLine>,
    #[serde(default)]
    orders: Vec<OrderItem>,
}

/// Заказ, попавший в банковский ордер: дата заказа, статус и сумма перечисления.
#[derive(Debug, Clone, Deserialize)]
struct OrderItem {
    order_id: i64,
    /// UUID агрегата a013 для гиперссылки (пусто, если заказ не загружен).
    #[serde(default)]
    ym_order_id: Option<String>,
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    order_date: String,
    amount: f64,
    #[serde(default)]
    rows_count: i64,
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

/// Стиль ячейки расхождения: значимое отклонение — красным, иначе приглушённо.
fn deviation_cell_style(value: f64) -> &'static str {
    if value.abs() > 0.5 {
        "width:130px; text-align:right; color: var(--color-error-700); font-weight:600;"
    } else {
        "width:130px; text-align:right; color: var(--color-text-secondary);"
    }
}

#[component]
pub fn YmSettlementReconList() -> impl IntoView {
    let tabs_store = use_context::<AppGlobalContext>().expect("AppGlobalContext not found");
    let items = RwSignal::new(Vec::<ListItem>::new());
    let total = RwSignal::new(0usize);
    let loading = RwSignal::new(false);
    let error_msg = RwSignal::new(Option::<String>::None);
    let gen_msg = RwSignal::new(Option::<String>::None);
    let generating = RwSignal::new(false);
    let cabinets = RwSignal::new(Vec::<(String, String)>::new());
    let (default_from, default_to) = default_month_range();
    let date_from = RwSignal::new(default_from);
    let date_to = RwSignal::new(default_to);
    let connection_id = RwSignal::new(String::new());
    let sort_field = RwSignal::new("bank_order_date".to_string());
    let sort_ascending = RwSignal::new(false);
    let selected_ids = RwSignal::new(HashSet::<String>::new());
    let posting = RwSignal::new(false);

    let load = move || {
        spawn_local(async move {
            loading.set(true);
            error_msg.set(None);
            let df = date_from.get_untracked();
            let dt = date_to.get_untracked();
            let cab = connection_id.get_untracked();
            let sf = sort_field.get_untracked();
            let asc = sort_ascending.get_untracked();
            let mut url = format!(
                "{}/api/a035/ym-settlement-recon/list?limit=1000&date_from={}&date_to={}&sort_by={}&sort_desc={}",
                api_base(),
                df,
                dt,
                sf,
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

    Effect::new(move |_| {
        spawn_local(async move {
            cabinets.set(load_cabinet_options().await);
        });
    });
    Effect::new(move |_| load());

    let cabinet_inited = StoredValue::new(false);
    Effect::new(move |_| {
        let _ = connection_id.get();
        if cabinet_inited.get_value() {
            load();
        } else {
            cabinet_inited.set_value(true);
        }
    });

    // Команда «Сформировать ордера»: находит все банковские ордера за период и
    // создаёт/обновляет документы. После — перезагрузка списка.
    let generate = move |_: leptos::ev::MouseEvent| {
        if generating.get_untracked() {
            return;
        }
        generating.set(true);
        gen_msg.set(None);
        let df = date_from.get_untracked();
        let dt = date_to.get_untracked();
        spawn_local(async move {
            let url = format!("{}/api/a035/ym-settlement-recon/generate", api_base());
            let body = serde_json::json!({ "date_from": df, "date_to": dt });
            match Request::post(&url).json(&body).map(|r| r.send()) {
                Ok(fut) => match fut.await {
                    Ok(resp) if resp.ok() => match resp.json::<GenerateResponse>().await {
                        Ok(r) => gen_msg.set(Some(format!(
                            "Ордеров: {} (создано {}, обновлено {})",
                            r.total, r.created, r.updated
                        ))),
                        Err(e) => gen_msg.set(Some(format!("Ошибка разбора: {e}"))),
                    },
                    Ok(resp) => gen_msg.set(Some(format!("HTTP {}", resp.status()))),
                    Err(e) => gen_msg.set(Some(format!("Ошибка запроса: {e}"))),
                },
                Err(e) => gen_msg.set(Some(format!("Ошибка запроса: {e}"))),
            }
            generating.set(false);
            load();
        });
    };

    // Выбор строк для массового проведения.
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

    // Массовое проведение выбранных ордеров: пишет события «Дата оплаты поставщику».
    let post_selected = move |_: leptos::ev::MouseEvent| {
        let ids: Vec<String> = selected_ids.get_untracked().into_iter().collect();
        if ids.is_empty() {
            return;
        }
        posting.set(true);
        spawn_local(async move {
            for id in &ids {
                let url = format!("{}/api/a035/ym-settlement-recon/{}/post", api_base(), id);
                let _ = Request::post(&url).send().await;
            }
            posting.set(false);
            selected_ids.update(|s| s.clear());
            load();
        });
    };

    let toggle_sort = move |field: &'static str| {
        let same = sort_field.get_untracked() == field;
        if same {
            sort_ascending.update(|a| *a = !*a);
        } else {
            sort_field.set(field.to_string());
            sort_ascending.set(true);
        }
        load();
    };

    let sort_header = move |label: &'static str, field: &'static str, numeric: bool| {
        let base = if numeric {
            "width:130px; cursor:pointer; user-select:none; text-align:right;"
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
        <PageFrame page_id="a035_ym_settlement_recon--list" category=PAGE_CAT_LIST>
            <div class="page__header">
                <div class="page__header-left">
                    <h1 class="page__title">"Сверка перечислений YM"</h1>
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
                    <Button
                        appearance=ButtonAppearance::Secondary
                        disabled=Signal::derive(move || generating.get())
                        on_click=generate
                    >
                        {icon("refresh")}
                        {move || if generating.get() { "Формирование..." } else { "Сформировать ордера" }}
                    </Button>
                    <Button appearance=ButtonAppearance::Subtle on_click=move |_| load()>
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
                                    label="Период ордера:".to_string()
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

                {move || gen_msg.get().map(|m| view! { <div class="alert alert--success">{m}</div> })}
                {move || error_msg.get().map(|err| view! { <div class="alert alert--error">{err}</div> })}
                {move || {
                    if loading.get() {
                        view! { <div class="page__placeholder"><Spinner /> " Загрузка..."</div> }.into_any()
                    } else if items.get().is_empty() {
                        view! { <div class="page__placeholder">"Нет документов. Нажмите «Сформировать ордера» для создания из p907."</div> }.into_any()
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
                                            {sort_header("Ордер", "bank_order_id", true)}
                                            {sort_header("Дата ордера", "bank_order_date", true)}
                                            <th class="table__header-cell">"Период операций"</th>
                                            <th class="table__header-cell">"Дней"</th>
                                            <th class="table__header-cell">"Записей"</th>
                                            <th class="table__header-cell">"Схема"</th>
                                            <th class="table__header-cell">"Проведён"</th>
                                            {sort_header("Кабинет", "connection_name", false)}
                                            {sort_header("Факт YM", "bank_sum", true)}
                                            {sort_header("Теор. сумма", "theoretical_sum", true)}
                                            {sort_header("Расхождение", "deviation", true)}
                                        </tr>
                                    </thead>
                                    <tbody>
                                        {move || {
                                            let tabs_store = tabs_store.clone();
                                            items.get().into_iter().map(move |item| {
                                                let tabs_store = tabs_store.clone();
                                                let id = item.id.clone();
                                                let ord = item.bank_order_id;
                                                let open = move |_| {
                                                    tabs_store.open_tab(
                                                        &format!("a035_ym_settlement_recon_details_{}", id),
                                                        &format!("Сверка YM ордер {}", ord),
                                                    );
                                                };
                                                let period = if item.period_from.is_empty() && item.period_to.is_empty() {
                                                    String::new()
                                                } else {
                                                    format!("{} … {}", item.period_from, item.period_to)
                                                };
                                                let days = period_days(&item.period_from, &item.period_to)
                                                    .map(|d| d.to_string())
                                                    .unwrap_or_default();
                                                let id_for_cb = item.id.clone();
                                                let id_for_change = item.id.clone();
                                                let is_posted = item.is_posted;
                                                view! {
                                                    <tr class="table__row">
                                                        <td class="table__cell" style="width:36px;" on:click=|e| e.stop_propagation()>
                                                            <input
                                                                type="checkbox"
                                                                class="table__checkbox"
                                                                prop:checked=move || selected_ids.get().contains(&id_for_cb)
                                                                on:change=move |ev| {
                                                                    let checked = event_target_checked(&ev);
                                                                    let id_cb = id_for_change.clone();
                                                                    selected_ids.update(|s| { if checked { s.insert(id_cb); } else { s.remove(&id_cb); } });
                                                                }
                                                            />
                                                        </td>
                                                        <td class="table__cell table__cell--right" style="width:130px;">
                                                            <a href="#" class="table__link" on:click=move |ev| { ev.prevent_default(); open(()); }>
                                                                {item.bank_order_id.to_string()}
                                                            </a>
                                                        </td>
                                                        <td class="table__cell table__cell--right" style="width:130px;">{item.bank_order_date.clone()}</td>
                                                        <td class="table__cell">{period}</td>
                                                        <td class="table__cell table__cell--right">{days}</td>
                                                        <td class="table__cell table__cell--right">{item.rows_count.to_string()}</td>
                                                        <td class="table__cell">{item.model.clone()}</td>
                                                        <td class="table__cell">{if is_posted { "✓" } else { "" }}</td>
                                                        <td class="table__cell">{item.connection_name.clone().unwrap_or(item.connection_id.clone())}</td>
                                                        <td class="table__cell table__cell--right" style="width:130px;">{format_money(item.bank_sum)}</td>
                                                        <td class="table__cell table__cell--right" style="width:130px;">{format_money(item.theoretical_sum)}</td>
                                                        <td class="table__cell" style=deviation_cell_style(item.deviation)>{format_money(item.deviation)}</td>
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
pub fn YmSettlementReconDetail(id: String, #[prop(into)] on_close: Callback<()>) -> impl IntoView {
    let doc = RwSignal::new(Option::<DetailsDto>::None);
    let error_msg = RwSignal::new(Option::<String>::None);
    let busy = RwSignal::new(false);

    let id_for_load = id.clone();
    let load = move || {
        let id = id_for_load.clone();
        spawn_local(async move {
            error_msg.set(None);
            let url = format!("{}/api/a035/ym-settlement-recon/{}", api_base(), id);
            match Request::get(&url).send().await {
                Ok(resp) if resp.ok() => match resp.json::<DetailsDto>().await {
                    Ok(data) => doc.set(Some(data)),
                    Err(e) => error_msg.set(Some(format!("Ошибка разбора: {e}"))),
                },
                Ok(resp) => error_msg.set(Some(format!("HTTP {}", resp.status()))),
                Err(e) => error_msg.set(Some(format!("Ошибка запроса: {e}"))),
            }
        });
    };

    {
        let load = load.clone();
        Effect::new(move |_| load());
    }

    let id_for_recompute = id.clone();
    let recompute = move |_| {
        let id = id_for_recompute.clone();
        spawn_local(async move {
            busy.set(true);
            let url = format!(
                "{}/api/a035/ym-settlement-recon/{}/recompute",
                api_base(),
                id
            );
            if let Ok(resp) = Request::post(&url).send().await {
                if resp.ok() {
                    if let Ok(data) = resp.json::<DetailsDto>().await {
                        doc.set(Some(data));
                    }
                }
            }
            busy.set(false);
        });
    };

    let is_posted = Signal::derive(move || doc.get().map(|d| d.is_posted).unwrap_or(false));

    // Одна кнопка-переключатель: проводит или отменяет проведение по текущему статусу.
    let id_for_toggle = id.clone();
    let load_for_toggle = load.clone();
    let toggle_post = move |_| {
        let posted = is_posted.get_untracked();
        let id = id_for_toggle.clone();
        let load = load_for_toggle.clone();
        spawn_local(async move {
            busy.set(true);
            let action = if posted { "unpost" } else { "post" };
            let url = format!(
                "{}/api/a035/ym-settlement-recon/{}/{}",
                api_base(),
                id,
                action
            );
            let _ = Request::post(&url).send().await;
            busy.set(false);
            load();
        });
    };

    let favorite_tab_key = format!("a035_ym_settlement_recon_details_{}", id);
    let favorite_title = Signal::derive(move || {
        doc.get()
            .map(|d| format!("Сверка YM ордер {}", d.bank_order_id))
            .unwrap_or_else(|| "Сверка перечислений YM".to_string())
    });
    let fav_key_for_btn = favorite_tab_key.clone();

    view! {
        <PageFrame page_id="a035_ym_settlement_recon--details" category=PAGE_CAT_LIST>
            <div class="page__header">
                <div class="page__header-left">
                    <FavoriteButton
                        target_kind="a035_ym_settlement_recon_details".to_string()
                        target_id=fav_key_for_btn.clone()
                        target_title=favorite_title
                        tab_key=fav_key_for_btn
                    />
                    <h1 class="page__title">{move || favorite_title.get()}</h1>
                    {move || {
                        if is_posted.get() {
                            view! { <Badge appearance=BadgeAppearance::Filled color=BadgeColor::Success>"Проведён"</Badge> }.into_any()
                        } else {
                            view! { <Badge appearance=BadgeAppearance::Outline color=BadgeColor::Subtle>"Не проведён"</Badge> }.into_any()
                        }
                    }}
                </div>
                <div class="page__header-right">
                    <Button
                        appearance=ButtonAppearance::Subtle
                        size=ButtonSize::Medium
                        disabled=Signal::derive(move || busy.get())
                        on_click=toggle_post
                    >
                        <span class="page-action-button__content">
                            <span class="page-action-button__icon">
                                {move || if is_posted.get() { icon("x") } else { icon("zap") }}
                            </span>
                            <span class="page-action-button__text">
                                {move || if is_posted.get() { "Отменить проведение" } else { "Провести" }}
                            </span>
                        </span>
                    </Button>
                    <Button
                        appearance=ButtonAppearance::Subtle
                        size=ButtonSize::Medium
                        disabled=Signal::derive(move || busy.get())
                        on_click=recompute
                    >
                        <span class="page-action-button__content">
                            <span class="page-action-button__icon">{icon("refresh")}</span>
                            <span class="page-action-button__text">"Обновить"</span>
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
                {move || {
                    let Some(d) = doc.get() else {
                        return view! { <div class="page__placeholder"><Spinner /> " Загрузка..."</div> }.into_any();
                    };
                    view! { <ReconResultView doc=d /> }.into_any()
                }}
            </div>
        </PageFrame>
    }
}

/// Число календарных дней в периоде операций (включительно): period_to − period_from + 1.
fn period_days(from: &str, to: &str) -> Option<i64> {
    let f = chrono::NaiveDate::parse_from_str(from.trim(), "%Y-%m-%d").ok()?;
    let t = chrono::NaiveDate::parse_from_str(to.trim(), "%Y-%m-%d").ok()?;
    Some((t - f).num_days() + 1)
}

/// Лаг выплаты: дата ордера − последняя операция периода (дней). Отрицательный,
/// если в ордер попали операции позже даты самой выплаты.
fn payout_lag_days(order_date: &str, period_to: &str) -> Option<i64> {
    let o = chrono::NaiveDate::parse_from_str(order_date.trim(), "%Y-%m-%d").ok()?;
    let t = chrono::NaiveDate::parse_from_str(period_to.trim(), "%Y-%m-%d").ok()?;
    Some((o - t).num_days())
}

/// RFC3339 → "YYYY-MM-DD HH:MM" (или исходная строка, если короче).
fn fmt_dt(s: &str) -> String {
    let s = s.trim();
    if s.len() < 16 {
        return s.to_string();
    }
    s[..16].replace('T', " ")
}

/// Поясняющая подпись под заголовком блока (приглушённый курсив).
fn block_hint(text: &'static str) -> AnyView {
    view! {
        <div style="margin: 0 0 var(--spacing-sm); color: var(--color-text-secondary); font-size: 0.84em; font-style: italic; line-height: 1.35;">
            {text}
        </div>
    }
    .into_any()
}

/// Карточка результата: реквизиты ордера и таблица оборотов с итоговой сверкой
/// теоретической суммы против факта YM (bank_sum).
#[component]
fn ReconResultView(doc: DetailsDto) -> impl IntoView {
    let tabs_store = use_context::<AppGlobalContext>().expect("AppGlobalContext not found");
    let period = if doc.period_from.is_empty() && doc.period_to.is_empty() {
        String::new()
    } else {
        format!("{} … {}", doc.period_from, doc.period_to)
    };
    // Период операций охватывает несколько дней и может заканчиваться позже даты
    // самого ордера (поздние/отложенные операции). Показываем длину периода в днях.
    let days_label = match period_days(&doc.period_from, &doc.period_to) {
        Some(d) => format!("{} дн.", d),
        None => String::new(),
    };
    // Лаг выплаты: насколько дата ордера позже последней операции периода.
    let lag_label = match payout_lag_days(&doc.bank_order_date, &doc.period_to) {
        Some(d) => format!("{} дн.", d),
        None => String::new(),
    };
    let created = fmt_dt(&doc.created_at);
    let updated = fmt_dt(&doc.updated_at);

    // Обороты — по убыванию числа строк (самые массовые операции сверху).
    let mut lines = doc.lines.clone();
    lines.sort_by(|a, b| b.rows_count.cmp(&a.rows_count));
    let total_ops: i32 = lines.iter().map(|l| l.rows_count).sum();
    let turnovers_count = lines.len();
    let theoretical = doc.theoretical_sum;

    // Заказы ордера: перечень с датой заказа и суммой перечисления (Σ строк p907).
    let orders = doc.orders.clone();
    let orders_count = orders.len();
    let orders_total: f64 = orders.iter().map(|o| o.amount).sum();

    // Расхождение в процентах от факта (если факт ненулевой).
    let dev_pct = if doc.bank_sum.abs() > 0.005 {
        format!(" ({:+.2}%)", doc.deviation / doc.bank_sum * 100.0)
    } else {
        String::new()
    };
    let deviation_style = if doc.deviation.abs() > 0.5 {
        "text-align:right; color: var(--color-error-700); font-weight:600;"
    } else {
        "text-align:right; color: var(--color-success); font-weight:600;"
    };

    view! {
        <div style="padding: var(--spacing-md); display:flex; flex-direction:column; gap: var(--spacing-md); max-width:820px;">
            <CardAnimated delay_ms=0 nav_id="a035_recon_info">
                <h4 class="details-section__title">"Банковский ордер"</h4>
                {block_hint("Один документ = один банковский ордер YM (фактическая выплата на расчётный счёт). YM делит выплаты по схеме фасилитации (FBS/FBY/DBS), поэтому ордер обычно однороден по модели, а на одну дату приходится по ордеру на схему. Период операций — диапазон дат транзакций p907, попавших в выплату; лаг выплаты — на сколько дней дата ордера позже последней операции (обычно YM платит спустя ~4 недели).")}
                <table class="data-table" style="width:100%;">
                    <tbody>
                        <tr><td>"Ордер (bank_order_id)"</td><td>{doc.bank_order_id.to_string()}</td></tr>
                        <tr><td>"Дата ордера (выплата)"</td><td>{doc.bank_order_date.clone()}</td></tr>
                        <tr><td>"Период операций"</td><td>{period}</td></tr>
                        <tr><td>"Дней в периоде"</td><td>{days_label}</td></tr>
                        <tr><td>"Схема (модель)"</td><td>{doc.model.clone()}</td></tr>
                        <tr><td>"Лаг выплаты (ордер − период)"</td><td>{lag_label}</td></tr>
                        <tr><td>"Кабинет"</td><td>{doc.connection_name.clone().unwrap_or_default()}</td></tr>
                        <tr><td>"Организация"</td><td>{doc.organization_name.clone().unwrap_or_default()}</td></tr>
                        <tr><td>"Сформирован"</td><td>{created}</td></tr>
                        <tr><td>"Обновлён"</td><td>{updated}</td></tr>
                    </tbody>
                </table>
            </CardAnimated>

            <CardAnimated delay_ms=40 nav_id="a035_recon_summary">
                <h4 class="details-section__title">"Итоги сверки"</h4>
                {block_hint("Теоретическая сумма — Σ наших оборотов по операциям ордера («как должно быть по нашим представлениям»). Факт YM — итог банковского ордера (bank_sum). Расхождение = теория − факт; обычно это неитемизированное удержание YM (эквайринг/холдбэк) и близко к нулю при полном совпадении.")}
                <table class="data-table" style="width:100%;">
                    <tbody>
                        <tr>
                            <td>"Факт YM (bank_sum)"</td>
                            <td style="text-align:right;">{format_money(doc.bank_sum)}</td>
                        </tr>
                        <tr>
                            <td>"Теоретическая сумма (Σ оборотов)"</td>
                            <td style="text-align:right;">{format_money(doc.theoretical_sum)}</td>
                        </tr>
                        <tr style="font-weight:600; border-top:2px solid var(--color-border);">
                            <td>"Расхождение (теория − факт)"</td>
                            <td style=deviation_style>{format!("{}{}", format_money(doc.deviation), dev_pct)}</td>
                        </tr>
                        <tr>
                            <td>"Число оборотов"</td>
                            <td style="text-align:right;">{turnovers_count.to_string()}</td>
                        </tr>
                        <tr>
                            <td>"Всего операций (строк p907)"</td>
                            <td style="text-align:right;">{total_ops.to_string()}</td>
                        </tr>
                    </tbody>
                </table>
            </CardAnimated>

            <CardAnimated delay_ms=80 nav_id="a035_recon_turnovers">
                <h4 class="details-section__title">"Обороты"</h4>
                {block_hint("Строки p907 ордера, сгруппированные по нашим оборотам (turnover_code). «Строк» — число операций p907 в обороте (отсортировано по убыванию). «Доля» — вклад оборота в теоретическую сумму; у удержаний доля отрицательная, у выручки может превышать 100%.")}
                <table class="table" style="width:100%;">
                    <thead>
                        <tr class="table__header-row">
                            <th class="table__header-cell">"Оборот"</th>
                            <th class="table__header-cell">"Код"</th>
                            <th class="table__header-cell">"Строк"</th>
                            <th class="table__header-cell">"Сумма"</th>
                            <th class="table__header-cell">"Доля"</th>
                        </tr>
                    </thead>
                    <tbody>
                        {lines.into_iter().map(|l: ReconLine| {
                            let share = if theoretical.abs() > 0.005 {
                                format!("{:.1}%", l.amount / theoretical * 100.0)
                            } else {
                                String::new()
                            };
                            view! {
                                <tr class="table__row">
                                    <td class="table__cell">{l.turnover_name}</td>
                                    <td class="table__cell">{l.turnover_code}</td>
                                    <td class="table__cell table__cell--right">{l.rows_count}</td>
                                    <td class="table__cell table__cell--right">{format_money(l.amount)}</td>
                                    <td class="table__cell table__cell--right">{share}</td>
                                </tr>
                            }
                        }).collect_view()}
                    </tbody>
                    <tfoot>
                        <tr class="table__row" style="border-top:2px solid var(--color-border); font-weight:600;">
                            <td class="table__cell" colspan="3">"Теоретическая сумма (Σ оборотов)"</td>
                            <td class="table__cell table__cell--right">{format_money(doc.theoretical_sum)}</td>
                            <td class="table__cell"></td>
                        </tr>
                        <tr class="table__row">
                            <td class="table__cell" colspan="3">"Факт YM (bank_sum)"</td>
                            <td class="table__cell table__cell--right">{format_money(doc.bank_sum)}</td>
                            <td class="table__cell"></td>
                        </tr>
                        <tr class="table__row" style="font-weight:600;">
                            <td class="table__cell" colspan="3">"Расхождение (теория − факт)"</td>
                            <td class="table__cell" style=deviation_style>{format_money(doc.deviation)}</td>
                            <td class="table__cell"></td>
                        </tr>
                    </tfoot>
                </table>
            </CardAnimated>

            <CardAnimated delay_ms=120 nav_id="a035_recon_orders">
                <h4 class="details-section__title">
                    "Заказы в ордере"
                    <span style="margin-left: var(--spacing-sm); color: var(--color-text-secondary); font-weight:400;">
                        {format!("({})", orders_count)}
                    </span>
                </h4>
                {block_hint("Заказы, попавшие в этот банковский ордер: строки p907 с непустым order_id, сгруппированные по заказу. Номер заказа ведёт в карточку заказа (a013). «Статус» — текущий статус заказа; «Дата заказа» — order_creation_date; «Сумма перечисления» — Σ transaction_sum строк заказа в ордере (нетто с учётом возвратов). Часть суммы ордера (услуги, премии) не привязана к заказу и здесь не отражена.")}
                {if orders.is_empty() {
                    view! { <div class="page__placeholder">"Нет заказов с привязкой к этому ордеру."</div> }.into_any()
                } else {
                    let tabs_store = tabs_store.clone();
                    view! {
                        <table class="table" style="width:100%;">
                            <thead>
                                <tr class="table__header-row">
                                    <th class="table__header-cell">"Заказ"</th>
                                    <th class="table__header-cell">"Статус"</th>
                                    <th class="table__header-cell">"Дата заказа"</th>
                                    <th class="table__header-cell">"Строк"</th>
                                    <th class="table__header-cell">"Сумма перечисления"</th>
                                </tr>
                            </thead>
                            <tbody>
                                {orders.into_iter().map(|o: OrderItem| {
                                    let tabs_store = tabs_store.clone();
                                    let order_no = o.order_id.to_string();
                                    let order_cell = match o.ym_order_id.clone() {
                                        Some(uuid) if !uuid.is_empty() => {
                                            let order_no_cb = order_no.clone();
                                            let open = move |ev: leptos::ev::MouseEvent| {
                                                ev.prevent_default();
                                                tabs_store.open_tab(
                                                    &format!("a013_ym_order_details_{}", uuid),
                                                    &format!("YM Order {}", order_no_cb),
                                                );
                                            };
                                            view! {
                                                <a href="#" class="table__link" on:click=open>{order_no.clone()}</a>
                                            }.into_any()
                                        }
                                        _ => order_no.clone().into_any(),
                                    };
                                    view! {
                                        <tr class="table__row">
                                            <td class="table__cell">{order_cell}</td>
                                            <td class="table__cell">{o.status.unwrap_or_default()}</td>
                                            <td class="table__cell">{o.order_date}</td>
                                            <td class="table__cell table__cell--right">{o.rows_count.to_string()}</td>
                                            <td class="table__cell table__cell--right">{format_money(o.amount)}</td>
                                        </tr>
                                    }
                                }).collect_view()}
                            </tbody>
                            <tfoot>
                                <tr class="table__row" style="border-top:2px solid var(--color-border); font-weight:600;">
                                    <td class="table__cell" colspan="4">{format!("Итого по {} заказам", orders_count)}</td>
                                    <td class="table__cell table__cell--right">{format_money(orders_total)}</td>
                                </tr>
                            </tfoot>
                        </table>
                    }.into_any()
                }}
            </CardAnimated>
        </div>
    }
}
