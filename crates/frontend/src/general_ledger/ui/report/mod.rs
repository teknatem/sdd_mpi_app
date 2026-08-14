use crate::general_ledger::api::{
    create_gl_drilldown_session, fetch_general_ledger_turnovers, fetch_gl_report,
};
use crate::general_ledger::ui::dimension_chip::{is_system_dim_id, GlDimensionChip};
use crate::general_ledger::ui::layer_badge::GlLayerBadge;
use crate::layout::global_context::AppGlobalContext;
use crate::shared::api_utils::api_base;
use crate::shared::components::date_range_picker::DateRangePicker;
use crate::shared::icons::icon;
use crate::shared::modal_frame::ModalFrame;
use crate::shared::page_frame::PageFrame;
use crate::shared::page_standard::PAGE_CAT_LIST;
use chrono::{Datelike, Utc};
use contracts::domain::a006_connection_mp::aggregate::ConnectionMP;
use contracts::domain::common::AggregateId;
use contracts::general_ledger::GL_LAYER_CLASSES;
use contracts::general_ledger::{
    GeneralLedgerTurnoverDto, GlDimensionDef, GlDrilldownQuery, GlDrilldownSessionCreate,
    GlReportQuery, GlReportResponse, GlReportRow,
};
use gloo_net::http::Request;
use leptos::prelude::*;
use leptos::task::spawn_local;
use thaw::*;
use wasm_bindgen::JsCast;

// ─────────────────────────────────────────────────────────────────────────────
// Вспомогательные типы
// ─────────────────────────────────────────────────────────────────────────────

/// Измерения, материализованные только на слое `fina` (зеркало p914).
/// Зеркалит backend-реестр LAYER_DIMENSION_PROFILES: на остальных слоях drilldown
/// по ним невозможен, поэтому в UI их не предлагаем.
fn is_fina_only_dimension(dimension_id: &str) -> bool {
    matches!(dimension_id, "customer_kind" | "fulfillment_type")
}

#[derive(Clone, Debug)]
struct CabinetOption {
    id: String,
    label: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DrilldownDialogMode {
    Nomenclature,
    All,
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

static ACCOUNT_OPTIONS: &[(&str, &str)] = &[
    ("", "Все счета"),
    ("62", "62 — Покупатели"),
    ("44", "44 — Расходы на продажу"),
    ("4401", "4401 — Расходы МП"),
    ("41", "41 — Товары"),
    ("90", "90 — Продажи"),
    ("9001", "9001 — Выручка"),
    ("9002", "9002 — Себестоимость"),
    ("91", "91 — Прочие"),
    ("76", "76 — Расчёты"),
    ("7609", "7609 — Расчёты с МП"),
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

fn download_gl_report_csv(
    rows: Vec<GlReportRow>,
    total_debit: f64,
    total_credit: f64,
    total_balance: f64,
    total_count: i64,
) {
    let mut csv =
        String::from("Вид оборота;Код оборота;Слой;Оборот Дт;Оборот Кт;Сальдо;Кол-во записей\n");
    for row in &rows {
        csv.push_str(&format!(
            "{};{};{};{};{};{};{}\n",
            row.turnover_name,
            row.turnover_code,
            row.layer,
            fmt_csv_money(row.debit_amount),
            fmt_csv_money(row.credit_amount),
            fmt_csv_money(row.balance),
            row.entry_count,
        ));
    }
    csv.push_str(&format!(
        "ИТОГО;;;{};{};{};{}\n",
        fmt_csv_money(total_debit),
        fmt_csv_money(total_credit),
        fmt_csv_money(total_balance),
        total_count,
    ));

    // BOM — Excel автоматически определяет UTF-8
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
    anchor.set_download("gl_report.csv");

    let Some(body) = document.body() else { return };
    let _ = body.append_child(&anchor);
    anchor.click();
    let _ = body.remove_child(&anchor);
    let _ = web_sys::Url::revoke_object_url(&url);
}

// ─────────────────────────────────────────────────────────────────────────────
// Состояние фильтров
// ─────────────────────────────────────────────────────────────────────────────

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

#[component]
pub fn GeneralLedgerReportPage() -> impl IntoView {
    let tabs_store = use_context::<AppGlobalContext>().expect("AppGlobalContext not found");

    let (default_from, default_to) = default_month_range();

    // ── Фильтры (черновик) ───────────────────────────────────────────────────
    let date_from = RwSignal::new(default_from.clone());
    let date_to = RwSignal::new(default_to.clone());
    let cabinet_sig = RwSignal::new(String::new());
    let account_sig = RwSignal::new(String::new());
    let layer_sig = RwSignal::new(String::new());

    // ── Зафиксированные фильтры (на момент последнего запроса) ──────────────
    let applied_date_from = RwSignal::new(default_from);
    let applied_date_to = RwSignal::new(default_to);
    let applied_cabinet = RwSignal::new(String::new());
    let applied_account = RwSignal::new(String::new());
    let applied_layer = RwSignal::new(String::new());

    // ── Данные ───────────────────────────────────────────────────────────────
    let cabinet_options = RwSignal::new(Vec::<CabinetOption>::new());
    let turnover_options = RwSignal::new(Vec::<GeneralLedgerTurnoverDto>::new());
    let report_data = RwSignal::new(Option::<GlReportResponse>::None);
    let report_loading = RwSignal::new(false);
    let report_error = RwSignal::new(Option::<String>::None);
    let report_loaded = RwSignal::new(false);
    let modal_row: RwSignal<Option<GlReportRow>> = RwSignal::new(None);
    let modal_dimensions = RwSignal::new(Vec::<GlDimensionDef>::new());
    let modal_mode = RwSignal::new(DrilldownDialogMode::All);

    // ── Загрузка кабинетов ───────────────────────────────────────────────────
    Effect::new(move |_| {
        spawn_local(async move {
            let opts = load_cabinet_options().await;
            cabinet_options.set(opts);
        });
    });

    Effect::new(move |_| {
        spawn_local(async move {
            if let Ok(resp) = fetch_general_ledger_turnovers().await {
                turnover_options.set(resp.items);
            }
        });
    });

    // ── Загрузка отчёта ───────────────────────────────────────────────────────
    let load_report = move || {
        let df = date_from.get_untracked();
        let dt = date_to.get_untracked();
        let cab = cabinet_sig.get_untracked();
        let acc = account_sig.get_untracked();
        let lay = layer_sig.get_untracked();

        applied_date_from.set(df.clone());
        applied_date_to.set(dt.clone());
        applied_cabinet.set(cab.clone());
        applied_account.set(acc.clone());
        applied_layer.set(lay.clone());

        spawn_local(async move {
            report_loading.set(true);
            report_error.set(None);

            let query = GlReportQuery {
                date_from: df,
                date_to: dt,
                connection_mp_ref: if cab.is_empty() { None } else { Some(cab) },
                account: if acc.is_empty() { None } else { Some(acc) },
                layer: if lay.is_empty() { None } else { Some(lay) },
                entity: None,
            };

            match fetch_gl_report(&query).await {
                Ok(data) => {
                    report_data.set(Some(data));
                    report_loaded.set(true);
                }
                Err(err) => report_error.set(Some(err)),
            }
            report_loading.set(false);
        });
    };

    // ── Открыть drilldown в новой закладке ──────────────────────────────────
    let open_drilldown_tab = move |row: GlReportRow, dim: GlDimensionDef| {
        let tc = row.turnover_code.clone();
        let gb = dim.id.clone();
        let df = applied_date_from.get_untracked();
        let dt = applied_date_to.get_untracked();
        let cab = applied_cabinet.get_untracked();
        let acc = applied_account.get_untracked();
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
            account: if acc.is_empty() { None } else { Some(acc) },
            layer: if lay.is_empty() { None } else { Some(lay) },
            entity: None,
            corr_account: None,
        };
        let body = GlDrilldownSessionCreate {
            title: Some(tab_title.clone()),
            query,
        };
        let tabs_store = tabs_store.clone();

        spawn_local(async move {
            if let Ok(session) = create_gl_drilldown_session(&body).await {
                let tab_key = format!("gl_drilldown__{}", session.session_id);
                tabs_store.open_tab(&tab_key, &tab_title);
            }
        });
    };

    let open_dimension_dialog =
        move |row: GlReportRow, dimensions: Vec<GlDimensionDef>, mode: DrilldownDialogMode| {
            modal_row.set(Some(row));
            modal_dimensions.set(dimensions);
            modal_mode.set(mode);
        };

    view! {
        <PageFrame page_id="general_ledger_report--list" category=PAGE_CAT_LIST>
            // ── Заголовок ────────────────────────────────────────────────────
            <div class="page__header" style="display: flex; align-items: center; justify-content: space-between; gap: 12px;">
                <div class="page__header-left">
                    <h1 class="page__title">"Отчёт GL"</h1>
                </div>
                <div class="page__header-right">
                    <Button
                        appearance=ButtonAppearance::Secondary
                        disabled=Signal::derive(move || report_data.get().is_none())
                        on_click=move |_| {
                            if let Some(data) = report_data.get_untracked() {
                                let total_count: i64 =
                                    data.rows.iter().map(|r| r.entry_count).sum();
                                download_gl_report_csv(
                                    data.rows.clone(),
                                    data.total_debit,
                                    data.total_credit,
                                    data.total_balance,
                                    total_count,
                                );
                            }
                        }
                    >
                        {icon("download")}
                        "Excel (csv)"
                    </Button>
                </div>
            </div>

            <div class="page__content">

            // ── Компактная панель фильтров (одна строка) ─────────────────────
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
                <Select value=account_sig>
                    {ACCOUNT_OPTIONS.iter().map(|(val, label)| {
                        view! { <option value=*val>{*label}</option> }
                    }).collect_view()}
                </Select>
                <Select value=layer_sig>
                    <option value="">"Все слои"</option>
                    {GL_LAYER_CLASSES.iter().map(|item| {
                        view! { <option value=item.code>{item.name}</option> }
                    }).collect_view()}
                </Select>
                <Button
                    appearance=ButtonAppearance::Primary
                    on_click=move |_| load_report()
                    disabled=Signal::derive(move || report_loading.get())
                >
                    {move || if report_loading.get() { "Загрузка..." } else { "Применить" }}
                </Button>
            </div>

            // ── Статус ────────────────────────────────────────────────────────
            {move || {
                if report_loading.get() {
                    view! {
                        <div class="page__placeholder">
                            <Spinner /> " Загрузка отчёта..."
                        </div>
                    }.into_any()
                } else if let Some(err) = report_error.get() {
                    view! {
                        <div class="alert alert--error">{format!("Ошибка: {err}")}</div>
                    }.into_any()
                } else if !report_loaded.get() {
                    view! {
                        <div class="page__placeholder">
                            "Задайте фильтры и нажмите «Применить»"
                        </div>
                    }.into_any()
                } else {
                    view! { <></> }.into_any()
                }
            }}

            // ── Сводная таблица ───────────────────────────────────────────────
            {move || {
                let Some(data) = report_data.get() else {
                    return view! { <></> }.into_any();
                };

                let rows = data.rows.clone();
                let total_debit = data.total_debit;
                let total_count: i64 = rows.iter().map(|r| r.entry_count).sum();

                let turnover_items = turnover_options.get();

                view! {
                    <div class="table-wrap" style="width: 100%;">
                        <table class="table" style="width: 100%; table-layout: fixed;">
                            <thead>
                                <tr class="table__header-row">
                                    <th class="table__header-cell" style="width: 28%;">"Вид оборота"</th>
                                    <th class="table__header-cell table__header-cell--center" style="width: 7%;">"Дт"</th>
                                    <th class="table__header-cell table__header-cell--center" style="width: 7%;">"Кт"</th>
                                    <th class="table__header-cell" style="width: 16%;">"Код оборота"</th>
                                    <th class="table__header-cell" style="width: 8%;">"Слой"</th>
                                    <th class="table__header-cell" style="width: 14%;">"Оборот"</th>
                                    <th class="table__header-cell" style="width: 10%;">"Количество"</th>
                                    <th class="table__header-cell table__header-cell--center" style="width: 18%;">"Изм."</th>
                                </tr>
                            </thead>
                            <tbody>
                                {rows.into_iter().map(|row| {
                                    let turnover_meta = turnover_items
                                        .iter()
                                        .find(|item| item.code == row.turnover_code);
                                    let debit_account = turnover_meta
                                        .map(|item| item.debit_account.clone())
                                        .filter(|value| !value.trim().is_empty())
                                        .unwrap_or_else(|| "—".to_string());
                                    let credit_account = turnover_meta
                                        .map(|item| item.credit_account.clone())
                                        .filter(|value| !value.trim().is_empty())
                                        .unwrap_or_else(|| "—".to_string());
                                    // Разрезы слоя fina (p914) материализованы
                                    // только на слое fina — на других слоях их
                                    // не предлагаем (backend всё равно отклонит).
                                    // См. реестр LAYER_DIMENSION_PROFILES.
                                    let row_layer = row.layer.trim().to_string();
                                    let available_dimensions = turnover_meta
                                        .map(|item| item.available_dimensions.clone())
                                        .unwrap_or_default()
                                        .into_iter()
                                        .filter(|dim| {
                                            !is_fina_only_dimension(&dim.id) || row_layer == "fina"
                                        })
                                        .collect::<Vec<_>>();
                                    view! {
                                        <tr class="table__row">
                                            <td class="table__cell" style="overflow-wrap: anywhere;">
                                                {row.turnover_name.clone()}
                                            </td>
                                            <td class="table__cell table__cell--center">
                                                <code class="text-code">{debit_account}</code>
                                            </td>
                                            <td class="table__cell table__cell--center">
                                                <code class="text-code">{credit_account}</code>
                                            </td>
                                            <td class="table__cell" style="overflow-wrap: anywhere;">
                                                <code
                                                    class="text-code"
                                                    style=if row.turnover_code.ends_with("_storno") {
                                                        "color: var(--color-error-700); font-weight: 600; white-space: normal; overflow-wrap: anywhere;"
                                                    } else {
                                                        "white-space: normal; overflow-wrap: anywhere;"
                                                    }
                                                >
                                                    {row.turnover_code.clone()}
                                                </code>
                                            </td>
                                            <td class="table__cell">
                                                <GlLayerBadge layer=row.layer.clone() />
                                            </td>
                                            <td class="table__cell table__cell--right">
                                                {fmt_money(row.debit_amount)}
                                            </td>
                                            <td class="table__cell table__cell--right">
                                                {row.entry_count}
                                            </td>
                                            <td class="table__cell">
                                                <div class="gl-dim-chip-list">
                                                    {if available_dimensions.is_empty() {
                                                        view! {
                                                            <span style="font-size:12px; color:var(--color-text-secondary);">"—"</span>
                                                        }.into_any()
                                                    } else {
                                                        let row_for_buttons = row.clone();
                                                        let dims_for_buttons = available_dimensions.clone();
                                                        let day_dim = dims_for_buttons
                                                            .iter()
                                                            .find(|item| item.id == "entry_date")
                                                            .cloned();
                                                        let cab_dim = dims_for_buttons
                                                            .iter()
                                                            .find(|item| item.id == "connection_mp_ref")
                                                            .cloned();
                                                        let type_dim = dims_for_buttons
                                                            .iter()
                                                            .find(|item| item.id == "registrator_type")
                                                            .cloned();
                                                        let doc_dim = dims_for_buttons
                                                            .iter()
                                                            .find(|item| item.id == "registrator_ref")
                                                            .cloned();
                                                        let nomenclature_dims = dims_for_buttons
                                                            .iter()
                                                            .filter(|item| matches!(
                                                                item.id.as_str(),
                                                                "nomenclature"
                                                                    | "dim1_category"
                                                                    | "dim2_line"
                                                                    | "dim3_model"
                                                                    | "dim4_format"
                                                                    | "dim5_sink"
                                                                    | "dim6_size"
                                                            ))
                                                            .cloned()
                                                            .collect::<Vec<_>>();
                                                        let has_nomenclature = !nomenclature_dims.is_empty();
                                                        view! {
                                                            <>
                                                                {if has_nomenclature {
                                                                    let row_for_click = row_for_buttons.clone();
                                                                    let dims_for_click = nomenclature_dims.clone();
                                                                    view! {
                                                                        <GlDimensionChip
                                                                            label="NOM".to_string()
                                                                            color_key="nom"
                                                                            title="Дополнительные измерения из связанных проекций".to_string()
                                                                            interactive=true
                                                                            on_click=Callback::new(move |_| {
                                                                                open_dimension_dialog(
                                                                                    row_for_click.clone(),
                                                                                    dims_for_click.clone(),
                                                                                    DrilldownDialogMode::Nomenclature,
                                                                                );
                                                                            })
                                                                        />
                                                                    }.into_any()
                                                                } else {
                                                                    view! { <></> }.into_any()
                                                                }}
                                                                {day_dim.map(|dim| {
                                                                    let row_for_click = row_for_buttons.clone();
                                                                    let dim_for_click = dim.clone();
                                                                    view! {
                                                                        <GlDimensionChip
                                                                            label="DAY".to_string()
                                                                            color_key="day"
                                                                            title="Основное измерение из GL: по дням".to_string()
                                                                            interactive=true
                                                                            on_click=Callback::new(move |_| {
                                                                                open_drilldown_tab(
                                                                                    row_for_click.clone(),
                                                                                    dim_for_click.clone(),
                                                                                );
                                                                            })
                                                                        />
                                                                    }
                                                                })}
                                                                {cab_dim.map(|dim| {
                                                                    let row_for_click = row_for_buttons.clone();
                                                                    let dim_for_click = dim.clone();
                                                                    view! {
                                                                        <GlDimensionChip
                                                                            label="CAB".to_string()
                                                                            color_key="cab"
                                                                            title="Основное измерение из GL: по кабинету".to_string()
                                                                            interactive=true
                                                                            on_click=Callback::new(move |_| {
                                                                                open_drilldown_tab(
                                                                                    row_for_click.clone(),
                                                                                    dim_for_click.clone(),
                                                                                );
                                                                            })
                                                                        />
                                                                    }
                                                                })}
                                                                {type_dim.map(|dim| {
                                                                    let row_for_click = row_for_buttons.clone();
                                                                    let dim_for_click = dim.clone();
                                                                    view! {
                                                                        <GlDimensionChip
                                                                            label="TYPE".to_string()
                                                                            color_key="regtype"
                                                                            title="Основное измерение из GL: по типу регистратора".to_string()
                                                                            interactive=true
                                                                            on_click=Callback::new(move |_| {
                                                                                open_drilldown_tab(
                                                                                    row_for_click.clone(),
                                                                                    dim_for_click.clone(),
                                                                                );
                                                                            })
                                                                        />
                                                                    }
                                                                })}
                                                                {doc_dim.map(|dim| {
                                                                    let row_for_click = row_for_buttons.clone();
                                                                    let dim_for_click = dim.clone();
                                                                    view! {
                                                                        <GlDimensionChip
                                                                            label="DOC".to_string()
                                                                            color_key="regref"
                                                                            title="Основное измерение из GL: по документу-регистратору".to_string()
                                                                            interactive=true
                                                                            on_click=Callback::new(move |_| {
                                                                                open_drilldown_tab(
                                                                                    row_for_click.clone(),
                                                                                    dim_for_click.clone(),
                                                                                );
                                                                            })
                                                                        />
                                                                    }
                                                                })}
                                                                {{
                                                                    let row_for_click = row_for_buttons.clone();
                                                                    let dims_for_click = dims_for_buttons.clone();
                                                                    view! {
                                                                        <GlDimensionChip
                                                                            label="ALL".to_string()
                                                                            color_key="all"
                                                                            title="Все доступные измерения: основные и дополнительные".to_string()
                                                                            interactive=true
                                                                            on_click=Callback::new(move |_| {
                                                                                open_dimension_dialog(
                                                                                    row_for_click.clone(),
                                                                                    dims_for_click.clone(),
                                                                                    DrilldownDialogMode::All,
                                                                                );
                                                                            })
                                                                        />
                                                                    }
                                                                }}
                                                            </>
                                                        }.into_any()
                                                    }}
                                                </div>
                                            </td>
                                        </tr>
                                    }
                                }).collect_view()}
                            </tbody>
                            <tfoot>
                                <tr class="table__totals-row">
                                    <td class="table__cell" colspan="5">
                                        <strong>"ИТОГО"</strong>
                                    </td>
                                    <td class="table__cell table__cell--right">
                                        <strong>{fmt_money(total_debit)}</strong>
                                    </td>
                                    <td class="table__cell table__cell--right">
                                        <strong>{total_count}</strong>
                                    </td>
                                    <td class="table__cell"></td>
                                </tr>
                            </tfoot>
                        </table>
                    </div>
                }.into_any()
            }}

            </div> // page__content

            {move || {
                let Some(row) = modal_row.get() else {
                    return view! { <></> }.into_any();
                };

                let dims = modal_dimensions.get();
                let mode = modal_mode.get();

                let main_dims = dims
                    .iter()
                    .filter(|item| {
                        matches!(
                            item.id.as_str(),
                            "entry_date" | "connection_mp_ref" | "registrator_ref"
                        )
                    })
                    .cloned()
                    .collect::<Vec<_>>();
                let nomenclature_dims = dims
                    .iter()
                    .filter(|item| {
                        matches!(
                            item.id.as_str(),
                            "nomenclature"
                                | "dim1_category"
                                | "dim2_line"
                                | "dim3_model"
                                | "dim4_format"
                                | "dim5_sink"
                                | "dim6_size"
                        )
                    })
                    .cloned()
                    .collect::<Vec<_>>();
                let auxiliary_dims = dims
                    .iter()
                    .filter(|item| {
                        matches!(item.id.as_str(), "registrator_type" | "layer")
                    })
                    .cloned()
                    .collect::<Vec<_>>();
                let system_dims = dims
                    .iter()
                    .filter(|item| is_system_dim_id(&item.id))
                    .cloned()
                    .collect::<Vec<_>>();

                let dialog_title = match mode {
                    DrilldownDialogMode::Nomenclature => "Дополнительные измерения",
                    DrilldownDialogMode::All => "Все измерения оборота",
                };

                let render_dim_list = move |items: Vec<GlDimensionDef>, row_base: GlReportRow| {
                    items.into_iter()
                        .map(|dim| {
                            let row_for_click = row_base.clone();
                            let dim_for_click = dim.clone();
                            view! {
                                <div
                                    class="gl-dim-item"
                                    on:click=move |_| open_drilldown_tab(row_for_click.clone(), dim_for_click.clone())
                                >
                                    <span class="gl-dim-item__label">{dim.label.clone()}</span>
                                    <span class="gl-dim-item__arrow">"›"</span>
                                </div>
                            }
                        })
                        .collect_view()
                };

                view! {
                    <ModalFrame
                        on_close=Callback::new(move |_| modal_row.set(None))
                        modal_style="width: 360px; padding: 0; overflow: hidden;".to_string()
                    >
                        <div style="padding: 14px 18px 12px; border-bottom: 1px solid var(--color-border);">
                            <div style="font-size: 14px; font-weight: 600; margin-bottom: 3px;">
                                {dialog_title}
                            </div>
                            <div style="font-size: 12px; color: var(--color-text-secondary);">
                                {format!("{} · {}", row.turnover_name, row.turnover_code)}
                            </div>
                        </div>

                        <div>
                            {match mode {
                                DrilldownDialogMode::Nomenclature => {
                                    if nomenclature_dims.is_empty() {
                                        view! {
                                            <div style="padding: 20px 18px; font-size:13px; color:var(--color-text-secondary);">
                                                "Дополнительные измерения недоступны"
                                            </div>
                                        }.into_any()
                                    } else {
                                        view! {
                                            <div>
                                                <div class="gl-dim-section">"Дополнительные (из проекций)"</div>
                                                {render_dim_list(nomenclature_dims.clone(), row.clone())}
                                            </div>
                                        }.into_any()
                                    }
                                }
                                DrilldownDialogMode::All => {
                                    view! {
                                        <div>
                                            {if !main_dims.is_empty() {
                                                view! {
                                                    <div>
                                                        <div class="gl-dim-section">"Основные (из GL)"</div>
                                                        {render_dim_list(main_dims.clone(), row.clone())}
                                                    </div>
                                                }.into_any()
                                            } else {
                                                view! { <></> }.into_any()
                                            }}

                                            {if !nomenclature_dims.is_empty() {
                                                view! {
                                                    <div>
                                                        <div class="gl-dim-section" style="margin-top: 2px;">"Дополнительные (из проекций)"</div>
                                                        {render_dim_list(nomenclature_dims.clone(), row.clone())}
                                                    </div>
                                                }.into_any()
                                            } else {
                                                view! { <></> }.into_any()
                                            }}

                                            {if !auxiliary_dims.is_empty() {
                                                view! {
                                                    <div>
                                                        <div class="gl-dim-section" style="margin-top: 2px;">"Вспомогательные"</div>
                                                        {render_dim_list(auxiliary_dims.clone(), row.clone())}
                                                    </div>
                                                }.into_any()
                                            } else {
                                                view! { <></> }.into_any()
                                            }}

                                            {if !system_dims.is_empty() {
                                                view! {
                                                    <div>
                                                        <div class="gl-dim-section" style="margin-top: 2px;">"Системные"</div>
                                                        {render_dim_list(system_dims.clone(), row.clone())}
                                                    </div>
                                                }.into_any()
                                            } else {
                                                view! { <></> }.into_any()
                                            }}
                                        </div>
                                    }.into_any()
                                }
                            }}
                        </div>
                    </ModalFrame>
                }.into_any()
            }}

        </PageFrame>
    }
}
