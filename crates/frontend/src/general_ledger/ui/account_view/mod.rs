use crate::general_ledger::api::{
    create_gl_drilldown_session, fetch_gl_account_view, fetch_gl_dimensions,
};
use crate::general_ledger::ui::dimension_chip::is_system_dim_id;
use crate::layout::global_context::AppGlobalContext;
use crate::shared::components::date_range_picker::DateRangePicker;
use crate::shared::icons::icon;
use crate::shared::modal_frame::ModalFrame;
use crate::shared::page_frame::PageFrame;
use crate::shared::page_standard::PAGE_CAT_LIST;
use chrono::{Datelike, Utc};
use contracts::domain::a006_connection_mp::aggregate::ConnectionMP;
use contracts::domain::common::AggregateId;
use contracts::general_ledger::{
    GlAccountViewQuery, GlAccountViewResponse, GlAccountViewRow, GlDimensionDef, GlDrilldownQuery,
    GlDrilldownSessionCreate,
};
use gloo_net::http::Request;
use leptos::prelude::*;
use leptos::task::spawn_local;
use thaw::*;
use wasm_bindgen::JsCast;

use crate::shared::api_utils::api_base;

// ─────────────────────────────────────────────────────────────────────────────
// Вспомогательные типы
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
struct CabinetOption {
    id: String,
    label: String,
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
    let mut opts: Vec<CabinetOption> = data
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
    opts.sort_by(|a, b| a.label.cmp(&b.label));
    opts
}

static LAYER_OPTIONS: &[(&str, &str)] = &[
    ("prod", "Производственный"),
    ("", "Все слои"),
    ("oper", "Операционный"),
    ("fact", "Фактический"),
    ("plan", "Плановый"),
];

fn fmt_money(v: f64) -> String {
    let abs = v.abs();
    let sign = if v < 0.0 { "−" } else { "" };
    let int_part = abs as u64;
    let frac = ((abs - int_part as f64) * 100.0).round() as u64;
    let int_str = int_part.to_string();
    let mut groups: Vec<&str> = Vec::new();
    let mut i = int_str.len();
    while i > 3 {
        i -= 3;
        groups.push(&int_str[i..i + 3]);
    }
    groups.push(&int_str[..i]);
    groups.reverse();
    format!("{}{},{:02}", sign, groups.join("\u{00a0}"), frac)
}

fn fmt_csv_money(v: f64) -> String {
    format!("{:.2}", v).replace('.', ",")
}

fn download_csv(data: &GlAccountViewResponse) {
    let mut csv = String::from(
        "Блок;Вид оборота;Код оборота;Корр.счёт;Слой;Дт оборот;Кт оборот;Сальдо;Кол-во\n",
    );
    for row in &data.main_rows {
        csv.push_str(&format!(
            "Основной;{};{};{};{};{};{};{};{}\n",
            row.turnover_name,
            row.turnover_code,
            row.corr_account,
            row.layer,
            fmt_csv_money(row.debit_amount),
            fmt_csv_money(row.credit_amount),
            fmt_csv_money(row.balance),
            row.entry_count,
        ));
    }
    csv.push_str(&format!(
        "ИТОГО;;;;;\"{}\";\"{}\";\"{}\";{}\n",
        fmt_csv_money(data.total_debit),
        fmt_csv_money(data.total_credit),
        fmt_csv_money(data.total_balance),
        data.main_rows.iter().map(|r| r.entry_count).sum::<i64>(),
    ));
    for row in &data.info_rows {
        csv.push_str(&format!(
            "Доп.;{};{};{};{};{};{};{};{}\n",
            row.turnover_name,
            row.turnover_code,
            row.corr_account,
            row.layer,
            fmt_csv_money(row.debit_amount),
            fmt_csv_money(row.credit_amount),
            fmt_csv_money(row.balance),
            row.entry_count,
        ));
    }

    let content = format!("\u{FEFF}{}", csv);
    let Some(window) = web_sys::window() else {
        return;
    };
    let Some(document) = window.document() else {
        return;
    };
    let array = js_sys::Array::new();
    array.push(&wasm_bindgen::JsValue::from_str(&content));
    let opts = web_sys::BlobPropertyBag::new();
    opts.set_type("text/csv;charset=utf-8;");
    let Ok(blob) = web_sys::Blob::new_with_str_sequence_and_options(&array, &opts) else {
        return;
    };
    let Ok(url) = web_sys::Url::create_object_url_with_blob(&blob) else {
        return;
    };
    let Ok(el) = document.create_element("a") else {
        return;
    };
    let Ok(anchor) = el.dyn_into::<web_sys::HtmlAnchorElement>() else {
        return;
    };
    anchor.set_href(&url);
    anchor.set_download("gl_account_view_7609.csv");
    let Some(body) = document.body() else { return };
    let _ = body.append_child(&anchor);
    anchor.click();
    let _ = body.remove_child(&anchor);
    let _ = web_sys::Url::revoke_object_url(&url);
}

fn default_month_range() -> (String, String) {
    let now = Utc::now().date_naive();
    let year = now.year();
    let month = now.month();
    let start = chrono::NaiveDate::from_ymd_opt(year, month, 1).expect("start");
    let end = if month == 12 {
        chrono::NaiveDate::from_ymd_opt(year + 1, 1, 1).map(|d| d - chrono::Duration::days(1))
    } else {
        chrono::NaiveDate::from_ymd_opt(year, month + 1, 1).map(|d| d - chrono::Duration::days(1))
    }
    .expect("end");
    (
        start.format("%Y-%m-%d").to_string(),
        end.format("%Y-%m-%d").to_string(),
    )
}

// ─────────────────────────────────────────────────────────────────────────────
// Основной компонент
// ─────────────────────────────────────────────────────────────────────────────

/// Страница GL-ведомости по счёту 7609 — «Расчёты с маркетплейсом».
///
/// Две секции:
///   - Основные обороты (main_rows) — суммируются, отображаются первыми.
///   - Дополнительно (info_rows)   — для сверки, без суммирования.
#[component]
pub fn GlAccountViewPage() -> impl IntoView {
    let tabs_store = use_context::<AppGlobalContext>().expect("AppGlobalContext not found");

    const ACCOUNT: &str = "7609";

    let (default_from, default_to) = default_month_range();

    // ── Фильтры (черновик) ───────────────────────────────────────────────────
    let date_from = RwSignal::new(default_from.clone());
    let date_to = RwSignal::new(default_to.clone());
    let cabinet_sig = RwSignal::new(String::new());
    let layer_sig = RwSignal::new(String::new());

    // ── Зафиксированные фильтры (на момент последнего запроса) ──────────────
    let applied_date_from = RwSignal::new(default_from);
    let applied_date_to = RwSignal::new(default_to);
    let applied_cabinet = RwSignal::new(String::new());
    let applied_layer = RwSignal::new(String::new());

    // ── Данные ───────────────────────────────────────────────────────────────
    let cabinet_options = RwSignal::new(Vec::<CabinetOption>::new());
    let view_data = RwSignal::new(Option::<GlAccountViewResponse>::None);
    let loading = RwSignal::new(false);
    let error_msg = RwSignal::new(Option::<String>::None);
    let loaded = RwSignal::new(false);

    // ── Состояние модального окна детализации ────────────────────────────────
    let modal_row: RwSignal<Option<GlAccountViewRow>> = RwSignal::new(None);
    let dimensions = RwSignal::new(Vec::<GlDimensionDef>::new());
    let dims_loading = RwSignal::new(false);

    // ── Загрузка кабинетов ───────────────────────────────────────────────────
    Effect::new(move |_| {
        spawn_local(async move {
            cabinet_options.set(load_cabinet_options().await);
        });
    });

    // ── Загрузка данных ───────────────────────────────────────────────────────
    let load_view = move || {
        let df = date_from.get_untracked();
        let dt = date_to.get_untracked();
        let cab = cabinet_sig.get_untracked();
        let lay = layer_sig.get_untracked();

        applied_date_from.set(df.clone());
        applied_date_to.set(dt.clone());
        applied_cabinet.set(cab.clone());
        applied_layer.set(lay.clone());

        spawn_local(async move {
            loading.set(true);
            error_msg.set(None);

            let query = GlAccountViewQuery {
                account: ACCOUNT.to_string(),
                date_from: df,
                date_to: dt,
                connection_mp_ref: if cab.is_empty() { None } else { Some(cab) },
                layer: if lay.is_empty() { None } else { Some(lay) },
            };

            match fetch_gl_account_view(&query).await {
                Ok(data) => {
                    view_data.set(Some(data));
                    loaded.set(true);
                }
                Err(err) => error_msg.set(Some(err)),
            }
            loading.set(false);
        });
    };

    // ── Открыть модалку с измерениями для строки ────────────────────────────
    let open_drilldown_modal = move |row: GlAccountViewRow| {
        let tc = row.turnover_code.clone();
        modal_row.set(Some(row));
        dimensions.set(vec![]);
        spawn_local(async move {
            dims_loading.set(true);
            match fetch_gl_dimensions(&tc).await {
                Ok(resp) => dimensions.set(resp.dimensions),
                Err(_) => dimensions.set(vec![]),
            }
            dims_loading.set(false);
        });
    };

    // ── Открыть drilldown-вкладку (после выбора измерения) ──────────────────
    let open_drilldown_tab = move |row: GlAccountViewRow, dim: GlDimensionDef| {
        let tc = row.turnover_code.clone();
        let gb = dim.id.clone();
        let df = applied_date_from.get_untracked();
        let dt = applied_date_to.get_untracked();
        let cab = applied_cabinet.get_untracked();
        let row_layer = row.layer.trim().to_string();
        let lay = if row_layer.is_empty() {
            applied_layer.get_untracked()
        } else {
            row_layer
        };
        let tab_title = format!("{} / {}", row.turnover_name, dim.label);
        let query = GlDrilldownQuery {
            turnover_code: tc,
            group_by: gb,
            date_from: df,
            date_to: dt,
            connection_mp_ref: if cab.is_empty() { None } else { Some(cab) },
            connection_mp_refs: vec![],
            account: Some(ACCOUNT.to_string()),
            layer: if lay.is_empty() { None } else { Some(lay) },
            entity: None,
            corr_account: if row.corr_account.trim().is_empty() {
                None
            } else {
                Some(row.corr_account.clone())
            },
        };
        let body = GlDrilldownSessionCreate {
            title: Some(tab_title.clone()),
            query,
        };
        let tabs_store = tabs_store.clone();
        modal_row.set(None);
        spawn_local(async move {
            if let Ok(session) = create_gl_drilldown_session(&body).await {
                let tab_key = format!("gl_drilldown__{}", session.session_id);
                tabs_store.open_tab(&tab_key, &tab_title);
            }
        });
    };

    view! {
        <PageFrame page_id="gl_account_view--7609" category=PAGE_CAT_LIST>
            // ── Заголовок ────────────────────────────────────────────────────
            <div class="page__header" style="display: flex; align-items: center; justify-content: space-between; gap: 12px;">
                <div class="page__header-left">
                    <h1 class="page__title">"Ведомость по счёту 7609 — Расчёты с МП"</h1>
                </div>
                <div class="page__header-right">
                    <Button
                        appearance=ButtonAppearance::Secondary
                        disabled=Signal::derive(move || view_data.get().is_none())
                        on_click=move |_| {
                            if let Some(data) = view_data.get_untracked() {
                                download_csv(&data);
                            }
                        }
                    >
                        {icon("download")}
                        "Excel (csv)"
                    </Button>
                </div>
            </div>

            <div class="page__content">

            // ── Панель фильтров ───────────────────────────────────────────────
            <div class="gl-filter-bar" style="margin-top: 8px;">
                <DateRangePicker
                    date_from=date_from
                    date_to=date_to
                    on_change=Callback::new(move |(f, t): (String, String)| {
                        date_from.set(f);
                        date_to.set(t);
                    })
                />
                <Select value=cabinet_sig>
                    <option value="">"Все кабинеты"</option>
                    {move || cabinet_options.get().into_iter().map(|opt| {
                        view! { <option value=opt.id.clone()>{opt.label.clone()}</option> }
                    }).collect_view()}
                </Select>
                <Select value=layer_sig>
                    {LAYER_OPTIONS.iter().map(|(val, label)| {
                        view! { <option value=*val>{*label}</option> }
                    }).collect_view()}
                </Select>
                <Button
                    appearance=ButtonAppearance::Primary
                    on_click=move |_| load_view()
                    disabled=Signal::derive(move || loading.get())
                >
                    {move || if loading.get() { "Загрузка..." } else { "Применить" }}
                </Button>
            </div>

            // ── Статус ────────────────────────────────────────────────────────
            {move || {
                if loading.get() {
                    view! {
                        <div class="page__placeholder"><Spinner /> " Загрузка..."</div>
                    }.into_any()
                } else if let Some(err) = error_msg.get() {
                    view! {
                        <div class="alert alert--error">{format!("Ошибка: {err}")}</div>
                    }.into_any()
                } else if !loaded.get() {
                    view! {
                        <div class="page__placeholder">"Задайте фильтры и нажмите «Применить»"</div>
                    }.into_any()
                } else {
                    view! { <></> }.into_any()
                }
            }}

            // ── Таблицы ───────────────────────────────────────────────────────
            {move || {
                let Some(data) = view_data.get() else {
                    return view! { <></> }.into_any();
                };

                let open_dd = open_drilldown_modal.clone();
                let open_dd2 = open_drilldown_modal.clone();

                view! {
                    <div>
                        // ══ Блок 1: основные обороты ════════════════════════
                        <div style="margin-top: 16px;">
                            <div class="gl-dim-section" style="font-size: 13px; font-weight: 600; padding: 6px 0; margin-bottom: 4px;">
                                "Основные обороты"
                            </div>
                            {view_table(data.main_rows.clone(), true, data.total_debit, data.total_credit, data.total_balance, open_dd)}
                        </div>

                        // ══ Блок 2: информационный ═══════════════════════════
                        {if !data.info_rows.is_empty() {
                            let total_dt: f64 = data.info_rows.iter().map(|r| r.debit_amount).sum();
                            let total_kt: f64 = data.info_rows.iter().map(|r| r.credit_amount).sum();
                            let total_bl = total_dt - total_kt;
                            view! {
                                <div style="margin-top: 24px;">
                                    <div class="gl-dim-section" style="font-size: 13px; font-weight: 600; padding: 6px 0; margin-bottom: 4px; color: var(--color-text-secondary);">
                                        "Дополнительно (без суммирования)"
                                    </div>
                                    {view_table(data.info_rows.clone(), false, total_dt, total_kt, total_bl, open_dd2)}
                                </div>
                            }.into_any()
                        } else {
                            view! { <></> }.into_any()
                        }}
                    </div>
                }.into_any()
            }}

            // ── Модальное окно детализации ────────────────────────────────────
            {move || {
                let Some(row) = modal_row.get() else {
                    return view! { <></> }.into_any();
                };

                let row_for_tabs = row.clone();
                let dims = dimensions.get();
                // Классификация по содержимому (не по индексу): номенклатурные —
                // по code_main == "Nom"; системные — оборот/счета; остальное общее.
                let nomen_dims = dims
                    .iter()
                    .filter(|dim| dim.code_main == "Nom")
                    .cloned()
                    .collect::<Vec<_>>();
                let system_dims = dims
                    .iter()
                    .filter(|dim| is_system_dim_id(&dim.id))
                    .cloned()
                    .collect::<Vec<_>>();
                let common_dims = dims
                    .iter()
                    .filter(|dim| dim.code_main != "Nom" && !is_system_dim_id(&dim.id))
                    .cloned()
                    .collect::<Vec<_>>();

                view! {
                    <ModalFrame
                        on_close=Callback::new(move |_| modal_row.set(None))
                        modal_style="width: 340px; padding: 0; overflow: hidden;".to_string()
                    >
                        <div style="padding: 14px 18px 12px; border-bottom: 1px solid var(--color-border);">
                            <div style="font-size: 14px; font-weight: 600; margin-bottom: 3px;">
                                "Детализация оборота"
                            </div>
                            <div style="font-size: 12px; color: var(--color-text-secondary);">
                                {format!("{} · {}", row.turnover_name, row.turnover_code)}
                            </div>
                        </div>

                        {if dims_loading.get() {
                            view! {
                                <div style="padding: 24px; display:flex; align-items:center; gap:8px; color:var(--color-text-secondary);">
                                    <Spinner /> " Загрузка..."
                                </div>
                            }.into_any()
                        } else if dims.is_empty() {
                            view! {
                                <div style="padding: 20px 18px; font-size:13px; color:var(--color-text-secondary);">
                                    "Детализация недоступна для данного вида оборота"
                                </div>
                            }.into_any()
                        } else {
                            let render_section = {
                                let row_base = row_for_tabs.clone();
                                move |title: &'static str, items: Vec<GlDimensionDef>| {
                                    if items.is_empty() {
                                        return view! { <></> }.into_any();
                                    }
                                    let row_base = row_base.clone();
                                    view! {
                                        <div>
                                            <div class="gl-dim-section" style="margin-top: 2px;">{title}</div>
                                            {items.into_iter().map(|dim| {
                                                let row_d = row_base.clone();
                                                let dim_c = dim.clone();
                                                view! {
                                                    <div
                                                        class="gl-dim-item"
                                                        on:click=move |_| open_drilldown_tab(row_d.clone(), dim_c.clone())
                                                    >
                                                        <span class="gl-dim-item__label">{dim.label.clone()}</span>
                                                        <span class="gl-dim-item__arrow">"›"</span>
                                                    </div>
                                                }
                                            }).collect_view()}
                                        </div>
                                    }.into_any()
                                }
                            };
                            view! {
                                <div>
                                    {render_section("Общие", common_dims)}
                                    {render_section("Системные", system_dims)}
                                    {render_section("По номенклатуре", nomen_dims)}
                                </div>
                            }.into_any()
                        }}
                    </ModalFrame>
                }.into_any()
            }}

            </div>
        </PageFrame>
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Таблица строк
// ─────────────────────────────────────────────────────────────────────────────

fn view_table(
    rows: Vec<GlAccountViewRow>,
    show_totals: bool,
    total_debit: f64,
    total_credit: f64,
    total_balance: f64,
    open_dd: impl Fn(GlAccountViewRow) + Clone + 'static,
) -> impl IntoView {
    let row_count: i64 = rows.iter().map(|r| r.entry_count).sum();

    // Compact cell style shared across all data cells
    const CELL: &str = "padding-top: 3px; padding-bottom: 3px;";
    const CELL_R: &str = "padding-top: 3px; padding-bottom: 3px; text-align: right;";
    const CELL_C: &str = "padding-top: 3px; padding-bottom: 3px; text-align: center;";
    // Balance column gets bold + subtle tinted background
    const BALANCE_BASE: &str = "padding-top: 3px; padding-bottom: 3px; text-align: right; font-weight: 600; background: rgba(99,102,241,0.06);";

    view! {
        <div class="table-wrap" style="width: 100%;">
            <table class="table" style="width: 100%; table-layout: fixed;">
                <thead>
                    <tr class="table__header-row">
                        <th class="table__header-cell" style="width: 28%;">"Вид оборота"</th>
                        <th class="table__header-cell table__header-cell--center" style="width: 10%;">"Корр.счёт"</th>
                        <th class="table__header-cell" style="width: 8%;">"Слой"</th>
                        <th class="table__header-cell" style="width: 14%;">"Дт оборот"</th>
                        <th class="table__header-cell" style="width: 14%;">"Кт оборот"</th>
                        <th class="table__header-cell" style="width: 12%; font-weight: 700; background: rgba(99,102,241,0.06);">"Сальдо"</th>
                        <th class="table__header-cell" style="width: 8%;">"Кол-во"</th>
                        <th class="table__header-cell table__header-cell--center" style="width: 6%;"></th>
                    </tr>
                </thead>
                <tbody>
                    {rows.into_iter().map(|row| {
                        let row_for_btn = row.clone();
                        let open_dd = open_dd.clone();
                        let balance_color = if row.balance < 0.0 { "color: var(--color-error-700);" } else { "" };
                        let balance_style = format!("{BALANCE_BASE}{balance_color}");
                        let is_storno = row.turnover_code.ends_with("_storno");
                        let code_style = if is_storno {
                            "font-size: 11px; color: #c2185b;"
                        } else {
                            "font-size: 11px; color: var(--color-text-tertiary);"
                        };
                        view! {
                            <tr class="table__row">
                                <td class="table__cell" style=format!("{CELL} overflow-wrap: anywhere;")>
                                    {row.turnover_name.clone()}
                                    <br />
                                    <code class="text-code" style=code_style>
                                        {row.turnover_code.clone()}
                                    </code>
                                </td>
                                <td class="table__cell table__cell--center" style=CELL_C>
                                    <code class="text-code">{row.corr_account.clone()}</code>
                                </td>
                                <td class="table__cell" style=CELL>
                                    <code class="text-code">{row.layer.clone()}</code>
                                </td>
                                <td class="table__cell table__cell--right" style=CELL_R>
                                    {if row.debit_amount.abs() > f64::EPSILON {
                                        fmt_money(row.debit_amount)
                                    } else {
                                        String::new()
                                    }}
                                </td>
                                <td class="table__cell table__cell--right" style=CELL_R>
                                    {if row.credit_amount.abs() > f64::EPSILON {
                                        fmt_money(row.credit_amount)
                                    } else {
                                        String::new()
                                    }}
                                </td>
                                <td class="table__cell table__cell--right" style=balance_style>
                                    {fmt_money(row.balance)}
                                </td>
                                <td class="table__cell table__cell--right" style=CELL_R>
                                    {row.entry_count}
                                </td>
                                <td class="table__cell table__cell--center" style=CELL_C>
                                    <button
                                        class="gl-link-btn"
                                        style="padding: 2px; line-height: 1; display: inline-flex; align-items: center; justify-content: center;"
                                        on:click=move |_| open_dd(row_for_btn.clone())
                                    >
                                        // Lucide «zoom-in» icon
                                        <svg
                                            xmlns="http://www.w3.org/2000/svg"
                                            width="14" height="14"
                                            viewBox="0 0 24 24"
                                            fill="none"
                                            stroke="currentColor"
                                            stroke-width="2"
                                            stroke-linecap="round"
                                            stroke-linejoin="round"
                                            aria-hidden="true"
                                        >
                                            <circle cx="11" cy="11" r="8"/>
                                            <line x1="21" y1="21" x2="16.65" y2="16.65"/>
                                            <line x1="11" y1="8" x2="11" y2="14"/>
                                            <line x1="8" y1="11" x2="14" y2="11"/>
                                        </svg>
                                    </button>
                                </td>
                            </tr>
                        }
                    }).collect_view()}
                </tbody>
                {if show_totals {
                    let total_bal_style = if total_balance < 0.0 {
                        format!("{BALANCE_BASE}color: var(--color-error-700);")
                    } else {
                        BALANCE_BASE.to_string()
                    };
                    view! {
                        <tfoot>
                            <tr class="table__totals-row">
                                <td class="table__cell" colspan="3" style=CELL>
                                    <strong>"ИТОГО"</strong>
                                </td>
                                <td class="table__cell table__cell--right" style=CELL_R>
                                    <strong>{fmt_money(total_debit)}</strong>
                                </td>
                                <td class="table__cell table__cell--right" style=CELL_R>
                                    <strong>{fmt_money(total_credit)}</strong>
                                </td>
                                <td class="table__cell table__cell--right" style=total_bal_style>
                                    {fmt_money(total_balance)}
                                </td>
                                <td class="table__cell table__cell--right" style=CELL_R>
                                    <strong>{row_count}</strong>
                                </td>
                                <td class="table__cell" style=CELL></td>
                            </tr>
                        </tfoot>
                    }.into_any()
                } else {
                    view! { <></> }.into_any()
                }}
            </table>
        </div>
    }
}
