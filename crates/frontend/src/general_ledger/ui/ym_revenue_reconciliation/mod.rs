use crate::general_ledger::api::fetch_ym_revenue_reconciliation;
use crate::layout::global_context::AppGlobalContext;
use crate::shared::api_utils::api_base;
use crate::shared::components::date_range_picker::DateRangePicker;
use crate::shared::components::table::format_money;
use crate::shared::page_frame::PageFrame;
use crate::shared::page_standard::PAGE_CAT_LIST;
use chrono::{Datelike, Utc};
use contracts::domain::a006_connection_mp::aggregate::ConnectionMP;
use contracts::domain::common::AggregateId;
use contracts::general_ledger::{YmRevenueReconGroup, YmRevenueReconQuery, YmRevenueReconRow};
use gloo_net::http::Request;
use leptos::prelude::*;
use leptos::task::spawn_local;
use thaw::*;

#[derive(Clone, Debug)]
struct CabinetOption {
    id: String,
    label: String,
}

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

async fn load_cabinet_options() -> Vec<CabinetOption> {
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
    let mut options: Vec<CabinetOption> = data
        .into_iter()
        .map(|c| {
            let label = if c.base.description.trim().is_empty() {
                c.base.code.clone()
            } else {
                c.base.description.clone()
            };
            CabinetOption {
                id: c.base.id.as_string(),
                label,
            }
        })
        .collect();
    options.sort_by(|a, b| a.label.cmp(&b.label));
    options
}

/// Подсветка дельты: значимое расхождение — красным.
fn delta_style(delta: f64) -> &'static str {
    if delta.abs() > 1.0 {
        "display:block; width:100%; text-align:right; color: var(--color-error-700); font-weight:600;"
    } else {
        "display:block; width:100%; text-align:right;"
    }
}

/// Шеврон-стрелка сворачивания: поворачивается вниз при раскрытии (общие классы
/// filter-panel__chevron из layout.css).
fn collapse_chevron(expanded: RwSignal<bool>) -> impl IntoView {
    view! {
        <svg
            width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor"
            stroke-width="2" stroke-linecap="round" stroke-linejoin="round"
            class=move || if expanded.get() {
                "filter-panel__chevron filter-panel__chevron--expanded"
            } else {
                "filter-panel__chevron"
            }
        >
            <polyline points="6 9 12 15 18 9"></polyline>
        </svg>
    }
}

#[component]
pub fn YmRevenueReconciliationPage() -> impl IntoView {
    let tabs_store = use_context::<AppGlobalContext>().expect("AppGlobalContext not found");
    let (default_from, default_to) = default_month_range();

    let date_from = RwSignal::new(default_from);
    let date_to = RwSignal::new(default_to);
    let cabinet_sig = RwSignal::new(String::new());
    let group_sig = RwSignal::new("day".to_string());
    // Группировка, с которой загружены текущие данные: ссылка на карточку a034 из
    // колонки «Период» активна только в режиме «по дням» (строка = один документ).
    let loaded_group = RwSignal::new("day".to_string());
    // Сворачивание фильтров и блока-пояснения (независимо друг от друга).
    let filters_expanded = RwSignal::new(true);
    let info_expanded = RwSignal::new(true);

    let rows = RwSignal::new(Vec::<YmRevenueReconRow>::new());
    let total_fina = RwSignal::new(0.0_f64);
    let total_ybuh = RwSignal::new(0.0_f64);
    let total_delta = RwSignal::new(0.0_f64);
    let total_fina_qty = RwSignal::new(0.0_f64);
    let total_ybuh_qty = RwSignal::new(0.0_f64);
    let total_qty_delta = RwSignal::new(0.0_f64);
    let cabinets = RwSignal::new(Vec::<CabinetOption>::new());
    let loading = RwSignal::new(false);
    let error_msg = RwSignal::new(Option::<String>::None);
    let loaded = RwSignal::new(false);

    Effect::new(move |_| {
        spawn_local(async move {
            cabinets.set(load_cabinet_options().await);
        });
    });

    let load_report = move || {
        let df = date_from.get_untracked();
        let dt = date_to.get_untracked();
        let cab = cabinet_sig.get_untracked();
        let group_str = group_sig.get_untracked();
        let group = if group_str == "month" {
            YmRevenueReconGroup::Month
        } else {
            YmRevenueReconGroup::Day
        };

        spawn_local(async move {
            loading.set(true);
            error_msg.set(None);
            let query = YmRevenueReconQuery {
                date_from: df,
                date_to: dt,
                connection_mp_ref: if cab.trim().is_empty() {
                    None
                } else {
                    Some(cab)
                },
                group,
            };
            match fetch_ym_revenue_reconciliation(&query).await {
                Ok(resp) => {
                    rows.set(resp.rows);
                    total_fina.set(resp.total_fina_net);
                    total_ybuh.set(resp.total_ybuh_net);
                    total_delta.set(resp.total_delta);
                    total_fina_qty.set(resp.total_fina_qty);
                    total_ybuh_qty.set(resp.total_ybuh_qty);
                    total_qty_delta.set(resp.total_qty_delta);
                    loaded_group.set(group_str.clone());
                    loaded.set(true);
                }
                Err(err) => error_msg.set(Some(err)),
            }
            loading.set(false);
        });
    };

    view! {
        <PageFrame page_id="ym_revenue_reconciliation--list" category=PAGE_CAT_LIST>
            <div class="page__header">
                <div class="page__header-left">
                    <h1 class="page__title">"Сверка выручки YM: fina (p907) vs ybuh (реализация)"</h1>
                    <Badge appearance=BadgeAppearance::Filled>
                        {move || rows.get().len().to_string()}
                    </Badge>
                </div>
            </div>

            <div style="margin: 0 var(--spacing-md) var(--spacing-sm); max-width: 1100px;">
                <div
                    on:click=move |_| info_expanded.update(|e| *e = !*e)
                    style="cursor:pointer; display:flex; align-items:center; gap:6px; color: var(--color-text-secondary); font-size: 0.82em; user-select:none;"
                >
                    {collapse_chevron(info_expanded)}
                    <span>"О сверке: что и с чем сверяется"</span>
                </div>
                {move || info_expanded.get().then(|| view! {
                    <p style="margin: var(--spacing-xs) 0 0; color: var(--color-text-secondary); font-size: 0.82em; line-height: 1.5;">
                        "Сверяются два независимых источника выручки Yandex Market за период (по дням/месяцам, кабинетам и модели FBS/FBY). "
                        "«fina» — оперативные взаиморасчёты из отчёта «united-netting» (p907): строки «Платёж покупателя» за вычетом «Возврат платежа покупателя», выручка признаётся по дате доставки. "
                        "«ybuh» — официальный «Отчёт о реализации» (a034): доставлено (delivered) за вычетом возвратов (returned), отчёт помесячный на кампанию. "
                        "Дельта = fina − ybuh. Небольшие расхождения по дням нормальны (разные даты признания и границы месяца отчёта); устойчивое одностороннее расхождение указывает на пропуск данных — например, не загружена кампания (FBS/FBY) или заказ отсутствует в одном из источников."
                    </p>
                })}
            </div>

            <div class="page__content">
                <div class="filter-panel">
                    <div class="filter-panel-header">
                        <div
                            class="filter-panel-header__left"
                            on:click=move |_| filters_expanded.update(|e| *e = !*e)
                            style="cursor:pointer;"
                        >
                            {collapse_chevron(filters_expanded)}
                            <span class="filter-panel__title">"Фильтры"</span>
                        </div>
                        <div class="filter-panel-header__right">
                            <Button
                                appearance=ButtonAppearance::Primary
                                on_click=move |_| load_report()
                                disabled=Signal::derive(move || loading.get())
                            >
                                {move || if loading.get() { "Загрузка..." } else { "Применить" }}
                            </Button>
                        </div>
                    </div>

                    {move || filters_expanded.get().then(|| view! {
                    <div class="filter-panel-content">
                        <Flex gap=FlexGap::Small align=FlexAlign::End>
                            <div style="min-width: 420px;">
                                <DateRangePicker
                                    date_from=Signal::derive(move || date_from.get())
                                    date_to=Signal::derive(move || date_to.get())
                                    on_change=Callback::new(move |(from, to): (String, String)| {
                                        date_from.set(from);
                                        date_to.set(to);
                                    })
                                    label="Период".to_string()
                                />
                            </div>
                            <div style="width: 280px;">
                                <Flex vertical=true gap=FlexGap::Small>
                                    <Label>"Кабинет"</Label>
                                    <Select value=cabinet_sig>
                                        <option value="">"Все кабинеты"</option>
                                        {move || cabinets.get().into_iter().map(|c| {
                                            view! { <option value=c.id.clone()>{c.label}</option> }
                                        }).collect_view()}
                                    </Select>
                                </Flex>
                            </div>
                            <div style="width: 180px;">
                                <Flex vertical=true gap=FlexGap::Small>
                                    <Label>"Группировка"</Label>
                                    <Select value=group_sig>
                                        <option value="day">"По дням"</option>
                                        <option value="month">"По месяцам"</option>
                                    </Select>
                                </Flex>
                            </div>
                        </Flex>
                    </div>
                    })}
                </div>

                {move || error_msg.get().map(|err| view! { <div class="alert alert--error">{err}</div> })}

                {move || {
                    if loading.get() {
                        view! { <div class="page__placeholder"><Spinner /> " Загрузка..."</div> }.into_any()
                    } else if !loaded.get() {
                        view! { <div class="page__placeholder">"Задайте фильтры и нажмите \"Применить\""</div> }.into_any()
                    } else if rows.get().is_empty() {
                        view! { <div class="page__placeholder">"Нет данных за период"</div> }.into_any()
                    } else {
                        view! {
                            <div class="table-wrap" style="width:100%;">
                                <table class="table" style="width:100%;">
                                    <thead>
                                        <tr class="table__header-row">
                                            <th class="table__header-cell" colspan="4"></th>
                                            <th class="table__header-cell" colspan="3" style="text-align:center; border-left:1px solid var(--color-border);">"Проверка суммы, ₽"</th>
                                            <th class="table__header-cell" colspan="3" style="text-align:center; border-left:1px solid var(--color-border);">"Проверка количества, шт"</th>
                                        </tr>
                                        <tr class="table__header-row">
                                            <th class="table__header-cell">"Период"</th>
                                            <th class="table__header-cell">"Кабинет"</th>
                                            <th class="table__header-cell">"Модель"</th>
                                            <th class="table__header-cell">"campaignId"</th>
                                            <th class="table__header-cell" style="border-left:1px solid var(--color-border);">"fina"</th>
                                            <th class="table__header-cell">"ybuh"</th>
                                            <th class="table__header-cell">"Дельта"</th>
                                            <th class="table__header-cell" style="border-left:1px solid var(--color-border);">"fina"</th>
                                            <th class="table__header-cell">"ybuh"</th>
                                            <th class="table__header-cell">"Дельта"</th>
                                        </tr>
                                        <tr class="table__totals-row">
                                            <td class="table__cell" colspan="4"><strong>"ИТОГО"</strong></td>
                                            <td class="table__cell table__cell--right" style="border-left:1px solid var(--color-border);"><strong>{move || format_money(total_fina.get())}</strong></td>
                                            <td class="table__cell table__cell--right"><strong>{move || format_money(total_ybuh.get())}</strong></td>
                                            <td class="table__cell table__cell--right"><strong>{move || format_money(total_delta.get())}</strong></td>
                                            <td class="table__cell table__cell--right" style="border-left:1px solid var(--color-border);"><strong>{move || format!("{:.0}", total_fina_qty.get())}</strong></td>
                                            <td class="table__cell table__cell--right"><strong>{move || format!("{:.0}", total_ybuh_qty.get())}</strong></td>
                                            <td class="table__cell table__cell--right"><strong>{move || format!("{:.0}", total_qty_delta.get())}</strong></td>
                                        </tr>
                                    </thead>
                                    <tbody>
                                        {move || {
                                            let tabs_store = tabs_store.clone();
                                            let day_mode = loaded_group.get() == "day";
                                            rows.get().into_iter().map(move |row| {
                                                let tabs_store = tabs_store.clone();
                                                let period_cell = match (day_mode, row.ybuh_doc_id.clone()) {
                                                    (true, Some(doc_id)) => {
                                                        let title = format!("Реализация YM {}", row.period);
                                                        let label = row.period.clone();
                                                        view! {
                                                            <a href="#" class="table__link"
                                                                on:click=move |ev| {
                                                                    ev.prevent_default();
                                                                    tabs_store.open_tab(
                                                                        &format!("a034_ym_realization_details_{}", doc_id),
                                                                        &title,
                                                                    );
                                                                }
                                                            >{label}</a>
                                                        }.into_any()
                                                    }
                                                    _ => row.period.clone().into_any(),
                                                };
                                                view! {
                                                    <tr class="table__row">
                                                        <td class="table__cell">{period_cell}</td>
                                                        <td class="table__cell">
                                                            {row.connection_name.clone()
                                                                .or_else(|| row.connection_mp_ref.clone())
                                                                .unwrap_or_default()}
                                                        </td>
                                                        <td class="table__cell">{row.model.clone().unwrap_or_else(|| "—".to_string())}</td>
                                                        <td class="table__cell">{row.campaign_id.clone().unwrap_or_else(|| "—".to_string())}</td>
                                                        <td class="table__cell table__cell--right" style="border-left:1px solid var(--color-border);">{format_money(row.fina_net)}</td>
                                                        <td class="table__cell table__cell--right">{format_money(row.ybuh_net)}</td>
                                                        <td class="table__cell">
                                                            <span style=delta_style(row.delta)>{format_money(row.delta)}</span>
                                                        </td>
                                                        <td class="table__cell table__cell--right" style="border-left:1px solid var(--color-border);">{format!("{:.0}", row.fina_qty)}</td>
                                                        <td class="table__cell table__cell--right">{format!("{:.0}", row.ybuh_qty)}</td>
                                                        <td class="table__cell">
                                                            <span style=delta_style(row.qty_delta)>{format!("{:.0}", row.qty_delta)}</span>
                                                        </td>
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
