//! GL Drilldown Page - детализация оборота по выбранному измерению.

use crate::general_ledger::api::{
    fetch_gl_drilldown, fetch_gl_drilldown_session, fetch_gl_drilldown_session_data,
};
use crate::layout::global_context::AppGlobalContext;
use crate::shared::icons::icon;
use crate::shared::page_frame::PageFrame;
use crate::shared::page_standard::PAGE_CAT_LIST;
use contracts::general_ledger::{
    AggregateRepresentation, GlDrilldownQuery, GlDrilldownResponse, GlDrilldownRow,
    GlDrilldownSessionRecord,
};
use leptos::prelude::*;
use leptos::task::spawn_local;
use thaw::*;
use wasm_bindgen::JsCast;

fn split_group_key(group_key: &str) -> (&str, &str) {
    if let Some(pos) = group_key.find("~~") {
        (&group_key[..pos], &group_key[pos + 2..])
    } else {
        ("", group_key)
    }
}

fn extract_doc_id(reg_ref: &str) -> &str {
    reg_ref
}

fn reg_type_name(reg_type: &str) -> &'static str {
    match reg_type {
        "a012_wb_sales" => "WB Продажа",
        "a013_ym_order" => "YM Заказ",
        "a014_ozon_transactions" => "OZON Транзакция",
        "a015_wb_orders" => "WB Заказ",
        "a016_ym_returns" => "YM Возврат",
        "a026_wb_advert_daily" => "WB Реклама",
        "a021_production_output" => "Производство",
        "a022_kit_variant" => "Комплект",
        "a023_purchase_of_goods" => "Закупка",
        "a028_missing_cost_registry" => "Реестр себестоимости",
        "p903_wb_finance_report" => "WB Финотчёт",
        "p907_ym_payment_report" => "YM Платёж",
        _ => "Документ",
    }
}

fn reg_display(reg_type: &str, reg_ref: &str, date: &str) -> String {
    let id = extract_doc_id(reg_ref);
    let short = if id.len() >= 8 { &id[..8] } else { id };
    format!("{} · {} · #{}", reg_type_name(reg_type), date, short)
}

/// Представление реального документа: «наименование · дата · #номер».
fn repr_display(rep: &AggregateRepresentation) -> String {
    let mut parts = vec![rep.title.clone()];
    if let Some(date) = rep.date.as_ref().filter(|s| !s.trim().is_empty()) {
        parts.push(date.clone());
    }
    if let Some(doc) = rep.doc_id.as_ref().filter(|s| !s.trim().is_empty()) {
        parts.push(format!("#{doc}"));
    }
    parts.join(" · ")
}

/// Короткая подпись вкладки документа из реального представления.
fn repr_tab_label(rep: &AggregateRepresentation) -> String {
    match rep.doc_id.as_ref().filter(|s| !s.trim().is_empty()) {
        Some(doc) => format!("{} · #{doc}", rep.title),
        None => rep.title.clone(),
    }
}

fn reg_tab_key(reg_type: &str, reg_ref: &str) -> Option<String> {
    let id = extract_doc_id(reg_ref);
    match reg_type {
        "a012_wb_sales" => Some(format!("a012_wb_sales_details_{id}")),
        "a013_ym_order" => Some(format!("a013_ym_order_details_{id}")),
        "a014_ozon_transactions" => Some(format!("a014_ozon_transactions_details_{id}")),
        "a015_wb_orders" => Some(format!("a015_wb_orders_details_{id}")),
        "a016_ym_returns" => Some(format!("a016_ym_returns_details_{id}")),
        "a026_wb_advert_daily" => Some(format!("a026_wb_advert_daily_details_{id}")),
        "a021_production_output" => Some(format!("a021_production_output_details_{id}")),
        "a022_kit_variant" => Some(format!("a022_kit_variant_details_{id}")),
        "a023_purchase_of_goods" => Some(format!("a023_purchase_of_goods_details_{id}")),
        "a028_missing_cost_registry" => Some(format!("a028_missing_cost_registry_details_{id}")),
        "p907_ym_payment_report" if !id.trim().is_empty() => {
            Some(format!("p907_ym_payment_report_details_{id}"))
        }
        "p903_wb_finance_report" if !id.trim().is_empty() => Some(format!(
            "p903_wb_finance_report_details_id_{}",
            urlencoding::encode(id)
        )),
        _ => None,
    }
}

fn reg_tab_label(reg_type: &str, reg_ref: &str) -> String {
    let id = extract_doc_id(reg_ref);
    let short = if id.len() >= 8 { &id[..8] } else { id };
    format!("{} · {}", reg_type_name(reg_type), short)
}

/// Группирует строки drilldown по дню (group_label), сохраняя порядок строк
/// (бэкенд уже отсортировал по дню, внутри — по сумме).
fn group_rows_by_day(rows: Vec<GlDrilldownRow>) -> Vec<(String, Vec<GlDrilldownRow>)> {
    let mut groups: Vec<(String, Vec<GlDrilldownRow>)> = Vec::new();
    for r in rows {
        let day = r.group_label.clone();
        if let Some(last) = groups.last_mut() {
            if last.0 == day {
                last.1.push(r);
                continue;
            }
        }
        groups.push((day, vec![r]));
    }
    groups
}

/// Рендерит строку регистратора (документа) внутри дневной группы: реальное
/// представление + гиперссылка (если доступна), сумма и количество.
fn render_registrator_row(r: GlDrilldownRow, tabs_store: AppGlobalContext) -> impl IntoView {
    let group_key = r.group_key.clone();
    let date = r.group_label.clone();
    let (reg_type, reg_ref) = split_group_key(&group_key);
    let reg_type = reg_type.to_string();
    let reg_ref = reg_ref.to_string();
    // Реальное представление документа (если бэкенд его прислал), иначе фолбэк.
    let (display, tab_label) = match r.representation.as_ref() {
        Some(rep) => (repr_display(rep), repr_tab_label(rep)),
        None => (
            reg_display(&reg_type, &reg_ref, &date),
            reg_tab_label(&reg_type, &reg_ref),
        ),
    };
    let tab_key = reg_tab_key(&reg_type, &reg_ref);
    let label_cell = if let Some(key) = tab_key {
        view! {
            <td class="table__cell" style="padding-left:24px;">
                <button
                    class="gl-link-btn"
                    on:click=move |_| tabs_store.open_tab(&key, &tab_label)
                >
                    {display}
                </button>
            </td>
        }
        .into_any()
    } else {
        view! {
            <td class="table__cell" style="padding-left:24px;">
                <span style="color:var(--color-text-secondary); font-size:12px;">{display}</span>
            </td>
        }
        .into_any()
    };
    view! {
        <tr class="table__row">
            {label_cell}
            <td class="table__cell table__cell--right">
                <span class="gl-drilldown__money">{fmt_money(r.amount)}</span>
            </td>
            <td class="table__cell table__cell--right">{r.entry_count}</td>
        </tr>
    }
}

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
    format!("{v:.2}").replace('.', ",")
}

fn csv_escape(value: &str) -> String {
    if value.contains(';') || value.contains('"') || value.contains('\n') || value.contains('\r') {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

fn filter_value(value: Option<&str>, empty_label: &str) -> String {
    value
        .filter(|v| !v.trim().is_empty())
        .unwrap_or(empty_label)
        .to_string()
}

fn visible_group_label(query: &GlDrilldownQuery, row: &GlDrilldownRow) -> String {
    if query.group_by == "registrator_ref" {
        if let Some(rep) = row.representation.as_ref() {
            return repr_display(rep);
        }
        let (reg_type, reg_ref) = split_group_key(&row.group_key);
        reg_display(reg_type, reg_ref, &row.group_label)
    } else {
        row.group_label.clone()
    }
}

fn sanitize_filename_part(value: &str) -> String {
    let mut sanitized = String::with_capacity(value.len());
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-') {
            sanitized.push(ch);
        } else {
            sanitized.push('_');
        }
    }
    sanitized.trim_matches('_').to_string()
}

fn download_gl_drilldown_csv(query: &GlDrilldownQuery, resp: &GlDrilldownResponse) {
    let mut csv = String::from("\u{FEFF}");
    csv.push_str(&format!(
        "Оборот;Код оборота;Группировка;Период;Кабинет;Счёт;Слой\n{};{};{};{} - {};{};{};{}\n\n",
        csv_escape(&resp.turnover_name),
        csv_escape(&resp.turnover_code),
        csv_escape(&resp.group_by_label),
        csv_escape(&query.date_from),
        csv_escape(&query.date_to),
        csv_escape(&filter_value(
            query.connection_mp_ref.as_deref(),
            "Все кабинеты"
        )),
        csv_escape(&filter_value(query.account.as_deref(), "Все счета")),
        csv_escape(&filter_value(query.layer.as_deref(), "Все слои")),
    ));

    csv.push_str(&format!(
        "{};Сумма;Кол-во\n",
        csv_escape(&resp.group_by_label)
    ));
    for row in &resp.rows {
        // Для разреза по регистратору добавляем день (двухуровневая группировка),
        // т.к. в таблице день показан отдельным заголовком группы.
        let label = if query.group_by == "registrator_ref" {
            format!("{} · {}", row.group_label, visible_group_label(query, row))
        } else {
            visible_group_label(query, row)
        };
        csv.push_str(&format!(
            "{};{};{}\n",
            csv_escape(&label),
            fmt_csv_money(row.amount),
            row.entry_count,
        ));
    }
    csv.push_str(&format!(
        "ИТОГО;{};{}\n",
        fmt_csv_money(resp.total_amount),
        resp.total_count,
    ));

    let Some(window) = web_sys::window() else {
        return;
    };
    let Some(document) = window.document() else {
        return;
    };

    let array = js_sys::Array::new();
    array.push(&wasm_bindgen::JsValue::from_str(&csv));

    let opts = web_sys::BlobPropertyBag::new();
    opts.set_type("text/csv;charset=utf-8;");
    let Ok(blob) = web_sys::Blob::new_with_str_sequence_and_options(&array, &opts) else {
        return;
    };
    let Ok(url) = web_sys::Url::create_object_url_with_blob(&blob) else {
        return;
    };

    let filename = format!(
        "gl_drilldown_{}_{}.csv",
        sanitize_filename_part(&resp.turnover_code),
        sanitize_filename_part(&query.group_by)
    );

    let Ok(el) = document.create_element("a") else {
        let _ = web_sys::Url::revoke_object_url(&url);
        return;
    };
    let Ok(anchor) = el.dyn_into::<web_sys::HtmlAnchorElement>() else {
        let _ = web_sys::Url::revoke_object_url(&url);
        return;
    };
    anchor.set_href(&url);
    anchor.set_download(&filename);

    let Some(body) = document.body() else {
        let _ = web_sys::Url::revoke_object_url(&url);
        return;
    };
    let _ = body.append_child(&anchor);
    anchor.click();
    let _ = body.remove_child(&anchor);
    let _ = web_sys::Url::revoke_object_url(&url);
}

#[component]
pub fn GlDrilldownPage(
    session_id: Option<String>,
    initial_query: Option<GlDrilldownQuery>,
    on_close: Callback<()>,
) -> impl IntoView {
    let tabs_store =
        leptos::context::use_context::<AppGlobalContext>().expect("AppGlobalContext not found");

    let query = RwSignal::new(initial_query.clone());
    let session = RwSignal::new(Option::<GlDrilldownSessionRecord>::None);
    let data = RwSignal::new(Option::<GlDrilldownResponse>::None);
    let loading = RwSignal::new(false);
    let error = RwSignal::new(Option::<String>::None);

    let do_load = move || {
        let session_id = session_id.clone();
        let initial_query = initial_query.clone();
        spawn_local(async move {
            loading.set(true);
            error.set(None);
            data.set(None);

            let result = if let Some(id) = session_id {
                match fetch_gl_drilldown_session(&id).await {
                    Ok(session_record) => {
                        let session_query = session_record.query.clone();
                        session.set(Some(session_record));
                        query.set(Some(session_query));
                        fetch_gl_drilldown_session_data(&id).await
                    }
                    Err(err) => Err(err),
                }
            } else if let Some(q) = initial_query {
                session.set(None);
                query.set(Some(q.clone()));
                fetch_gl_drilldown(&q).await
            } else {
                Err("GL drilldown query is missing".to_string())
            };

            match result {
                Ok(resp) => data.set(Some(resp)),
                Err(err) => error.set(Some(err)),
            }
            loading.set(false);
        });
    };

    let do_load_effect = do_load.clone();
    Effect::new(move |_| do_load_effect());

    let subtitle = Signal::derive(move || {
        if let Some(session_record) = session.get() {
            if !session_record.title.trim().is_empty() {
                return session_record.title;
            }
        }
        if let Some(resp) = data.get() {
            return format!("{} / {}", resp.turnover_code, resp.group_by_label);
        }
        query
            .get()
            .map(|q| format!("{} / {}", q.turnover_code, q.group_by))
            .unwrap_or_default()
    });

    view! {
        <PageFrame page_id="gl_drilldown--detail" category=PAGE_CAT_LIST>
            <div class="page__header">
                <div class="page__header-left">
                    <h1 class="page__title">"Детализация GL"</h1>
                    <span class="page__subtitle">{subtitle}</span>
                </div>
                <div class="page__header-right">
                    <Button
                        appearance=ButtonAppearance::Secondary
                        on_click=move |_| do_load()
                        disabled=Signal::derive(move || loading.get())
                    >
                        {move || if loading.get() { "Загрузка..." } else { "Обновить" }}
                    </Button>
                    <Button appearance=ButtonAppearance::Secondary on_click=move |_| on_close.run(())>
                        "Закрыть"
                    </Button>
                </div>
            </div>

            <div class="page__content">
                {move || {
                    if loading.get() {
                        return view! {
                            <div class="page__placeholder">
                                <Spinner /> " Загрузка..."
                            </div>
                        }
                        .into_any();
                    }
                    if let Some(err) = error.get() {
                        return view! {
                            <div class="alert alert--error">{format!("Ошибка: {err}")}</div>
                        }
                        .into_any();
                    }
                    let Some(resp) = data.get() else {
                        return view! { <></> }.into_any();
                    };
                    let Some(active_query) = query.get() else {
                        return view! {
                            <div class="page__placeholder">
                                "Не удалось восстановить параметры детализации"
                            </div>
                        }
                        .into_any();
                    };

                    let rows = resp.rows.clone();
                    let total_amount = resp.total_amount;
                    let total_count = resp.total_count;
                    let group_label = resp.group_by_label.clone();
                    let turnover_name = resp.turnover_name.clone();
                    let turnover_code = resp.turnover_code.clone();
                    let period_label =
                        format!("{} - {}", active_query.date_from, active_query.date_to);
                    let connection_label = filter_value(
                        active_query.connection_mp_ref.as_deref(),
                        "Все кабинеты",
                    );
                    let account_label =
                        filter_value(active_query.account.as_deref(), "Все счета");
                    let layer_label = filter_value(active_query.layer.as_deref(), "Все слои");
                    let export_query = active_query.clone();
                    let is_reg_dim = active_query.group_by == "registrator_ref";

                    view! {
                        <div class="card gl-drilldown__filters-card">
                            <div class="card__body">
                                <div class="gl-drilldown__filters-top">
                                    <div>
                                        <div class="gl-drilldown__filters-title">{turnover_name.clone()}</div>
                                        <div class="gl-drilldown__filters-subtitle">
                                            {format!("{} · {}", turnover_code.clone(), group_label.clone())}
                                        </div>
                                    </div>
                                    <div class="gl-drilldown__summary">
                                        <div class="gl-drilldown__stats">
                                            <div class="gl-drilldown__stat">
                                                <span class="gl-drilldown__stat-label">"Сумма"</span>
                                                <strong class="gl-drilldown__stat-value">{fmt_money(total_amount)}</strong>
                                            </div>
                                            <div class="gl-drilldown__stat">
                                                <span class="gl-drilldown__stat-label">"Записей"</span>
                                                <strong class="gl-drilldown__stat-value">{total_count}</strong>
                                            </div>
                                        </div>
                                        <Button appearance=ButtonAppearance::Primary on_click=move |_| {
                                            if let Some(resp) = data.get_untracked() {
                                                download_gl_drilldown_csv(&export_query, &resp);
                                            }
                                        }>
                                            {icon("download")}
                                            "Excel (csv)"
                                        </Button>
                                    </div>
                                </div>

                                <div class="gl-drilldown__filters-grid">
                                    <div class="gl-drilldown__filter-item">
                                        <span class="gl-drilldown__filter-label">"Период"</span>
                                        <span class="gl-drilldown__filter-value">{period_label}</span>
                                    </div>
                                    <div class="gl-drilldown__filter-item">
                                        <span class="gl-drilldown__filter-label">"Кабинет"</span>
                                        <span class="gl-drilldown__filter-value">{connection_label}</span>
                                    </div>
                                    <div class="gl-drilldown__filter-item">
                                        <span class="gl-drilldown__filter-label">"Счёт"</span>
                                        <span class="gl-drilldown__filter-value">{account_label}</span>
                                    </div>
                                    <div class="gl-drilldown__filter-item">
                                        <span class="gl-drilldown__filter-label">"Слой"</span>
                                        <span class="gl-drilldown__filter-value">{layer_label}</span>
                                    </div>
                                </div>
                            </div>
                        </div>

                        <div class="table-wrap">
                            <table class="table gl-drilldown__table">
                                <thead>
                                    <tr class="table__header-row">
                                        <th class="table__header-cell">{group_label.clone()}</th>
                                        <th class="table__header-cell">
                                            "Сумма"
                                        </th>
                                        <th class="table__header-cell">
                                            "Кол-во"
                                        </th>
                                    </tr>
                                </thead>
                                <tbody>
                                    {if is_reg_dim {
                                        // Двухуровневая группировка: заголовок дня + подытог,
                                        // под ним — строки регистраторов этого дня.
                                        group_rows_by_day(rows)
                                            .into_iter()
                                            .map(|(day, day_rows)| {
                                                let day_amount: f64 = day_rows.iter().map(|r| r.amount).sum();
                                                let day_count: i64 = day_rows.iter().map(|r| r.entry_count).sum();
                                                let reg_views = day_rows
                                                    .into_iter()
                                                    .map(|r| render_registrator_row(r, tabs_store))
                                                    .collect_view();
                                                view! {
                                                    <tr class="table__row gl-drilldown__day-row" style="background:var(--color-bg-subtle,#f5f6f8);">
                                                        <td class="table__cell"><strong>{day}</strong></td>
                                                        <td class="table__cell table__cell--right">
                                                            <strong>{fmt_money(day_amount)}</strong>
                                                        </td>
                                                        <td class="table__cell table__cell--right">
                                                            <strong>{day_count}</strong>
                                                        </td>
                                                    </tr>
                                                    {reg_views}
                                                }
                                            })
                                            .collect_view()
                                            .into_any()
                                    } else {
                                        rows.into_iter()
                                            .map(|r| {
                                                let label = visible_group_label(&active_query, &r);
                                                view! {
                                                    <tr class="table__row">
                                                        <td class="table__cell">{label}</td>
                                                        <td class="table__cell table__cell--right">
                                                            <span class="gl-drilldown__money">{fmt_money(r.amount)}</span>
                                                        </td>
                                                        <td class="table__cell table__cell--right">
                                                            {r.entry_count}
                                                        </td>
                                                    </tr>
                                                }
                                            })
                                            .collect_view()
                                            .into_any()
                                    }}
                                </tbody>
                                <tfoot>
                                    <tr class="table__totals-row">
                                        <td class="table__cell">
                                            <strong>"ИТОГО"</strong>
                                        </td>
                                        <td class="table__cell table__cell--right">
                                            <strong>{fmt_money(total_amount)}</strong>
                                        </td>
                                        <td class="table__cell table__cell--right">
                                            <strong>{total_count}</strong>
                                        </td>
                                    </tr>
                                </tfoot>
                            </table>
                        </div>
                    }
                    .into_any()
                }}
            </div>
        </PageFrame>
    }
}
