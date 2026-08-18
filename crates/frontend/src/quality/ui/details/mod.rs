//! # Страница детализации правила контроля качества (`quality_check_details`)
//!
//! Открывается из реестра проверок по кнопке «Детали» как таб
//! `quality_check_details_<check_id>`. Запускает проверку на бэкенде
//! (`GET /api/quality/checks/{id}/details`) и показывает:
//!
//! - заголовок правила (код, название, категория, время прогона);
//! - плитки-итоги: популяция · нарушений · соответствие %;
//! - таблицу метрик с долей соответствия по каждому источнику + drill-down;
//! - разрезы метрик (по кабинету, по исправимости и т.п.);
//! - примеры нарушений целостности GL с переходом в карточку GL;
//! - drill-down по регистраторам (группы → строки → перепроведение/очистка).

use crate::layout::global_context::AppGlobalContext;
use crate::quality::ui::drilldown::{ProjectionRowsPanel, RegistratorGroupsPanel};
use crate::shared::api_utils::api_base;
use crate::shared::icons::icon;
use crate::shared::page_frame::PageFrame;
use contracts::quality::{CheckBreakdown, CheckDetails};
use gloo_net::http::Request;
use leptos::prelude::*;
use leptos::task::spawn_local;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn fmt_pct(rate: f64) -> String {
    format!("{:.2}%", rate * 100.0)
}

/// Цвет доли соответствия: красный <90%, янтарный <99.5%, зелёный иначе.
fn rate_color(rate: f64) -> &'static str {
    if rate < 0.90 {
        "var(--color-error)"
    } else if rate < 0.995 {
        "var(--color-warning, #c77700)"
    } else {
        "var(--color-success, #1f9d55)"
    }
}

fn copy_to_clipboard(text: &str) {
    let safe = text
        .replace('\\', "\\\\")
        .replace('`', "\\`")
        .replace('$', "\\$");
    let _ = js_sys::eval(&format!("navigator.clipboard.writeText(`{safe}`)"));
}

fn build_llm_prompt(details: &CheckDetails) -> String {
    let CheckDetails {
        info,
        result,
        breakdowns,
        ..
    } = details;
    let run_at = result.run_at.format("%d.%m.%Y %H:%M:%S").to_string();
    let status = if result.violations_total == 0 {
        "✓ Нет нарушений".to_string()
    } else {
        format!(
            "⚠ Нарушений: {} из {} ({} соответствие)",
            result.violations_total,
            result.population_total,
            fmt_pct(result.compliance_rate())
        )
    };

    let mut out = format!(
        "## Отчёт контроля качества — {} ({})\n\n\
         **ID:** `{}`  **Категория:** {}  **Запущена:** {run_at}\n\
         **Описание:** {}\n\
         **Итог:** {status}\n",
        info.name, info.code, result.check_id, info.category, info.description,
    );

    if !result.metrics.is_empty() {
        out.push_str(
            "\n### Метрики\n\n| Источник | Популяция | Нарушений | Соответствие |\n|---|---|---|---|\n",
        );
        for m in &result.metrics {
            out.push_str(&format!(
                "| {} | {} | {} | {} |\n",
                m.label,
                m.population,
                m.violations,
                fmt_pct(m.compliance_rate())
            ));
        }
    }

    for b in breakdowns {
        out.push_str(&format!("\n### {}\n\n", b.title));
        if b.is_partition {
            out.push_str(&format!("| {} | Кол-во |\n|---|---|\n", b.dimension_label));
            for r in &b.rows {
                out.push_str(&format!("| {} | {} |\n", r.label, r.population));
            }
        } else {
            out.push_str(&format!(
                "| {} | Популяция | Нарушений | Соответствие |\n|---|---|---|---|\n",
                b.dimension_label
            ));
            for r in &b.rows {
                out.push_str(&format!(
                    "| {} | {} | {} | {} |\n",
                    r.label,
                    r.population,
                    r.violations,
                    fmt_pct(r.compliance_rate())
                ));
            }
        }
    }

    if !result.violations.is_empty() {
        out.push_str(&format!(
            "\n### Примеры нарушений ({} шт.)\n\n| Тип | GL ID | Таблица | Детали |\n|---|---|---|---|\n",
            result.violations.len()
        ));
        for v in &result.violations {
            out.push_str(&format!(
                "| {} | {} | {} | {} |\n",
                v.violation_type,
                v.gl_id.as_deref().unwrap_or("-"),
                v.projection_table.as_deref().unwrap_or("-"),
                v.detail.as_deref().unwrap_or("-"),
            ));
        }
    }

    out.push_str(
        "\n---\n**Проект:** Integrator, MPI (Rust / Leptos / Axum / SQLite)\n\
         **Задача:** Проанализировать причины нарушений и предложить конкретные исправления.\n",
    );
    out
}

/// Что показано в нижней панели drill-down.
#[derive(Debug, Clone, PartialEq)]
enum Drill {
    None,
    Groups {
        projection_table: String,
        projection_label: String,
    },
    Rows {
        projection_table: String,
        registrator_ref: String,
        registrator_label: String,
    },
}

// ---------------------------------------------------------------------------
// QualityCheckDetails — page
// ---------------------------------------------------------------------------

#[component]
#[allow(non_snake_case)]
pub fn QualityCheckDetails(check_id: String) -> impl IntoView {
    let tabs_store = leptos::context::use_context::<AppGlobalContext>()
        .expect("AppGlobalContext context not found");

    let (details, set_details) = signal::<Option<CheckDetails>>(None);
    let (loading, set_loading) = signal(true);
    let (error, set_error) = signal::<Option<String>>(None);
    let (drill, set_drill) = signal(Drill::None);
    let (reload, set_reload) = signal(0u32);

    let cid_for_fetch = check_id.clone();
    Effect::new(move |_| {
        let _ = reload.get();
        let cid = cid_for_fetch.clone();
        set_loading.set(true);
        set_error.set(None);
        set_drill.set(Drill::None);
        spawn_local(async move {
            let url = format!("{}/api/quality/checks/{}/details", api_base(), cid);
            match Request::get(&url).send().await {
                Ok(resp) if resp.status() == 200 => match resp.json::<CheckDetails>().await {
                    Ok(data) => {
                        set_details.set(Some(data));
                        set_loading.set(false);
                    }
                    Err(e) => {
                        set_error.set(Some(format!("Ошибка разбора: {e}")));
                        set_loading.set(false);
                    }
                },
                Ok(resp) => {
                    set_error.set(Some(format!("HTTP {}", resp.status())));
                    set_loading.set(false);
                }
                Err(e) => {
                    set_error.set(Some(format!("Ошибка запроса: {e}")));
                    set_loading.set(false);
                }
            }
        });
    });

    let cid_page = check_id.clone();
    let cid_for_rerun = check_id.clone();
    let rerun_check = Callback::new(move |_: ()| {
        let cid = cid_for_rerun.clone();
        set_loading.set(true);
        set_error.set(None);
        spawn_local(async move {
            let url = format!("{}/api/quality/checks/{}/run", api_base(), cid);
            match Request::post(&url).send().await {
                Ok(resp) if resp.status() == 200 => set_reload.update(|value| *value += 1),
                Ok(resp) => {
                    set_error.set(Some(format!("HTTP {}", resp.status())));
                    set_loading.set(false);
                }
                Err(error) => {
                    set_error.set(Some(format!("Ошибка запуска: {error}")));
                    set_loading.set(false);
                }
            }
        });
    });

    view! {
        <PageFrame page_id="quality_check_details--detail" category="detail">
            {move || {
                if loading.get() && details.get().is_none() {
                    return view! { <div style="padding: 20px; color: var(--color-text-secondary);">"Запуск проверки..."</div> }.into_any();
                }
                if let Some(e) = error.get() {
                    return view! { <div class="warning-box" style="margin: 16px;">{e}</div> }.into_any();
                }
                let Some(d) = details.get() else {
                    return view! { <div style="padding: 20px; color: var(--color-text-secondary);">"Нет данных"</div> }.into_any();
                };

                let info = d.info.clone();
                let result = d.result.clone();
                let breakdowns = d.breakdowns.clone();
                let sources = d.sources.clone();
                let run_at = result.run_at.format("%d.%m.%Y %H:%M:%S").to_string();
                let prompt = build_llm_prompt(&d);

                let population_total = result.population_total;
                let violations_total = result.violations_total;
                let compliant_total = (population_total - violations_total).max(0);
                let rate_total = result.compliance_rate();
                let viol_color = if violations_total > 0 {
                    "var(--color-error)".to_string()
                } else {
                    "var(--color-text-tertiary)".to_string()
                };
                let rate_str = fmt_pct(rate_total);
                let rate_col = rate_color(rate_total).to_string();
                let cid_metrics = cid_page.clone();
                let cid_groups = cid_page.clone();
                let cid_rows = cid_page.clone();

                view! {
                    <div class="page__header">
                        <div class="page__header-left">
                            <h1 class="page__title">
                                <span style="font-family: monospace; color: var(--color-text-secondary); margin-right: 8px;">{info.code.clone()}</span>
                                {info.name.clone()}
                            </h1>
                        </div>
                        <div class="page__header-right" style="display: flex; gap: 6px; align-items: center;">
                            <span style="font-size: 0.75rem; color: var(--color-text-tertiary);">{run_at}</span>
                            <thaw::Button
                                appearance=thaw::ButtonAppearance::Secondary
                                size=thaw::ButtonSize::Small
                                on_click=move |_| copy_to_clipboard(&prompt)
                            >
                                {icon("copy")} " Промпт"
                            </thaw::Button>
                            <thaw::Button
                                appearance=thaw::ButtonAppearance::Primary
                                size=thaw::ButtonSize::Small
                                disabled=loading.get()
                                on_click=move |_| rerun_check.run(())
                            >
                                {icon("refresh")} " Перезапустить"
                            </thaw::Button>
                        </div>
                    </div>

                    <div class="page__content">
                        // --- описание ---
                        <div style="display: flex; gap: 8px; align-items: center; margin-bottom: 4px;">
                            <span class="badge badge--secondary">{info.category.clone()}</span>
                            <span style="color: var(--color-text-secondary); font-size: 0.875rem;">{info.description.clone()}</span>
                        </div>

                        // --- headline-плитки ---
                        <div style="display: flex; gap: 12px; flex-wrap: wrap; margin: 12px 0 4px;">
                            <Tile label="Популяция" value=population_total.to_string() color="var(--color-text-primary)".to_string() hint="всего ситуаций под правилом".to_string() />
                            <Tile label="Соответствует" value=compliant_total.to_string() color="var(--color-text-primary)".to_string() hint=String::new() />
                            <Tile label="Нарушений" value=violations_total.to_string() color=viol_color hint=String::new() />
                            <Tile label="Соответствие" value=rate_str color=rate_col hint=String::new() />
                        </div>

                        // --- таблица метрик ---
                        {if !result.metrics.is_empty() {
                            let metrics = result.metrics.clone();
                            let srcs = sources.clone();
                            view! {
                                <h3 style="margin: 16px 0 6px; font-size: 0.95rem;">"Метрики по источникам"</h3>
                                <table class="table__data table--striped" style="font-size: 0.85rem;">
                                    <thead class="table__head">
                                        <tr>
                                            <th class="table__header-cell">"Источник / инвариант"</th>
                                            <th class="table__header-cell" style="text-align: right;">"Популяция"</th>
                                            <th class="table__header-cell" style="text-align: right;">"Нарушений"</th>
                                            <th class="table__header-cell" style="text-align: right;">"Соответствие"</th>
                                            <th class="table__header-cell table__header-cell--center" style="width: 170px;"></th>
                                        </tr>
                                    </thead>
                                    <tbody>
                                    {metrics.into_iter().map(|m| {
                                        let bad = m.violations > 0;
                                        let rate = m.compliance_rate();
                                        let label = m.label.clone();
                                        let matching_src = srcs.iter().find(|s| s.label == label).cloned();
                                        let cid_b = cid_metrics.clone();
                                        view! {
                                            <tr class="table__row">
                                                <td class="table__cell">{m.label.clone()}</td>
                                                <td class="table__cell" style="text-align: right; font-variant-numeric: tabular-nums; color: var(--color-text-secondary);">
                                                    {format!("{} {}", m.population, m.unit)}
                                                </td>
                                                <td class="table__cell" style=format!(
                                                    "text-align: right; font-variant-numeric: tabular-nums; font-weight: 600; color: {};",
                                                    if bad { "var(--color-error)" } else { "var(--color-text-tertiary)" }
                                                )>
                                                    {m.violations.to_string()}
                                                </td>
                                                <td class="table__cell" style=format!(
                                                    "text-align: right; font-variant-numeric: tabular-nums; font-weight: 600; color: {};",
                                                    rate_color(rate)
                                                )>
                                                    {fmt_pct(rate)}
                                                </td>
                                                <td class="table__cell table__cell--center">
                                                    {match (bad, matching_src) {
                                                        (true, Some(src)) => {
                                                            let proj_table = src.projection_table.clone();
                                                            let proj_label = src.label.clone();
                                                            let _ = &cid_b;
                                                            view! {
                                                                <thaw::Button
                                                                    appearance=thaw::ButtonAppearance::Subtle
                                                                    size=thaw::ButtonSize::Small
                                                                    on_click=move |_| set_drill.set(Drill::Groups {
                                                                        projection_table: proj_table.clone(),
                                                                        projection_label: proj_label.clone(),
                                                                    })
                                                                >
                                                                    {icon("list")} " По регистраторам"
                                                                </thaw::Button>
                                                            }.into_any()
                                                        }
                                                        _ => view! { <span /> }.into_any(),
                                                    }}
                                                </td>
                                            </tr>
                                        }
                                    }).collect_view()}
                                    </tbody>
                                </table>
                            }.into_any()
                        } else { view! { <span /> }.into_any() }}

                        // --- разрезы ---
                        {breakdowns.into_iter().map(|b| view! { <BreakdownTable breakdown=b /> }).collect_view()}

                        // --- примеры нарушений (GL) ---
                        {if !result.violations.is_empty() {
                            let violations = result.violations.clone();
                            view! {
                                <h3 style="margin: 16px 0 6px; font-size: 0.95rem;">{format!("Примеры нарушений — {} шт.", violations.len())}</h3>
                                <table class="table__data" style="font-size: 0.82rem;">
                                    <thead class="table__head">
                                        <tr>
                                            <th class="table__header-cell">"Тип"</th>
                                            <th class="table__header-cell">"GL"</th>
                                            <th class="table__header-cell">"Таблица"</th>
                                            <th class="table__header-cell">"Детали"</th>
                                        </tr>
                                    </thead>
                                    <tbody>
                                    {violations.into_iter().map(|v| {
                                        let vtype = match v.violation_type.as_str() {
                                            "orphan_gl" => "GL без детализации",
                                            "orphan_projection" => "Строка без GL",
                                            "amount_mismatch" => "Расхождение суммы",
                                            "missing_marketplace_product_ref" => "Нет товара МП",
                                            _ => "Нарушение",
                                        };
                                        let proj_table = v.projection_table.clone().unwrap_or_default();
                                        let detail = v.detail.clone().unwrap_or_default();
                                        let gl_opt = v.gl_id.clone();
                                        let store = tabs_store;
                                        view! {
                                            <tr class="table__row">
                                                <td class="table__cell"><span class="badge badge--secondary" style="font-size: 0.75rem;">{vtype}</span></td>
                                                <td class="table__cell" style="font-family: monospace; font-size: 0.78rem;">
                                                    {match gl_opt {
                                                        Some(gl_id) => {
                                                            let short = if gl_id.len() > 8 { format!("{}…", &gl_id[..8]) } else { gl_id.clone() };
                                                            let gid = gl_id.clone();
                                                            view! {
                                                                <thaw::Button
                                                                    appearance=thaw::ButtonAppearance::Subtle
                                                                    size=thaw::ButtonSize::Small
                                                                    on_click=move |_| {
                                                                        let sh = if gid.len() > 8 { gid[..8].to_string() } else { gid.clone() };
                                                                        store.open_tab(&format!("general_ledger_details_{gid}"), &format!("GL: {sh}…"));
                                                                    }
                                                                >{short}</thaw::Button>
                                                            }.into_any()
                                                        }
                                                        None => view! { <span style="color:var(--color-text-tertiary);">"-"</span> }.into_any(),
                                                    }}
                                                </td>
                                                <td class="table__cell" style="color: var(--color-text-secondary); font-size: 0.78rem;">{proj_table}</td>
                                                <td class="table__cell" style="color: var(--color-text-secondary); font-size: 0.78rem; font-family: monospace;">{detail}</td>
                                            </tr>
                                        }
                                    }).collect_view()}
                                    </tbody>
                                </table>
                            }.into_any()
                        } else { view! { <span /> }.into_any() }}

                        // --- drill-down панель ---
                        {move || match drill.get() {
                            Drill::None => view! { <span /> }.into_any(),
                            Drill::Groups { projection_table, projection_label } => {
                                let cid = cid_groups.clone();
                                let ptable_rows = projection_table.clone();
                                let store = tabs_store;
                                view! {
                                    <RegistratorGroupsPanel
                                        check_id=cid
                                        projection_table=projection_table
                                        projection_label=projection_label
                                        on_back=Callback::new(move |_| set_drill.set(Drill::None))
                                        on_close=Callback::new(move |_| set_drill.set(Drill::None))
                                        on_open_doc=Callback::new(move |(tab_key_prefix, doc_id, doc_label): (String, String, String)| {
                                            store.open_tab(&format!("{tab_key_prefix}_{doc_id}"), &doc_label);
                                        })
                                        on_open_rows=Callback::new(move |(_rtype, rref, rlabel): (String, String, String)| {
                                            set_drill.set(Drill::Rows {
                                                projection_table: ptable_rows.clone(),
                                                registrator_ref: rref,
                                                registrator_label: rlabel,
                                            });
                                        })
                                    />
                                }.into_any()
                            }
                            Drill::Rows { projection_table, registrator_ref, registrator_label } => {
                                let cid = cid_rows.clone();
                                let ptable_back = projection_table.clone();
                                view! {
                                    <ProjectionRowsPanel
                                        check_id=cid
                                        projection_table=projection_table
                                        registrator_ref=registrator_ref
                                        registrator_label=registrator_label
                                        on_back=Callback::new(move |_| set_drill.set(Drill::Groups {
                                            projection_table: ptable_back.clone(),
                                            projection_label: String::new(),
                                        }))
                                        on_close=Callback::new(move |_| set_drill.set(Drill::None))
                                    />
                                }.into_any()
                            }
                        }}
                    </div>
                }.into_any()
            }}
        </PageFrame>
    }
}

// ---------------------------------------------------------------------------
// Tile — headline-плитка
// ---------------------------------------------------------------------------

#[component]
#[allow(non_snake_case)]
fn Tile(label: &'static str, value: String, color: String, hint: String) -> impl IntoView {
    view! {
        <div style="flex: 1 1 160px; min-width: 140px; border: 1px solid var(--color-border); border-radius: 8px; background: var(--color-surface); padding: 12px 14px;">
            <div style="font-size: 0.78rem; color: var(--color-text-secondary); margin-bottom: 4px;">{label}</div>
            <div style=format!("font-size: 1.6rem; font-weight: 700; font-variant-numeric: tabular-nums; color: {color};")>{value}</div>
            {if hint.is_empty() { view! { <span /> }.into_any() } else {
                view! { <div style="font-size: 0.72rem; color: var(--color-text-tertiary); margin-top: 2px;">{hint}</div> }.into_any()
            }}
        </div>
    }
}

// ---------------------------------------------------------------------------
// BreakdownTable — таблица одного разреза
// ---------------------------------------------------------------------------

#[component]
#[allow(non_snake_case)]
fn BreakdownTable(breakdown: CheckBreakdown) -> impl IntoView {
    let CheckBreakdown {
        title,
        dimension_label,
        is_partition,
        rows,
        ..
    } = breakdown;
    let total: i64 = rows.iter().map(|r| r.population).sum();

    view! {
        <h3 style="margin: 16px 0 6px; font-size: 0.95rem;">{title}</h3>
        <table class="table__data table--striped" style="font-size: 0.85rem; max-width: 720px;">
            <thead class="table__head">
                <tr>
                    <th class="table__header-cell">{dimension_label}</th>
                    {if is_partition {
                        view! {
                            <th class="table__header-cell" style="text-align: right;">"Кол-во"</th>
                            <th class="table__header-cell" style="text-align: right;">"Доля"</th>
                        }.into_any()
                    } else {
                        view! {
                            <th class="table__header-cell" style="text-align: right;">"Популяция"</th>
                            <th class="table__header-cell" style="text-align: right;">"Нарушений"</th>
                            <th class="table__header-cell" style="text-align: right;">"Соответствие"</th>
                        }.into_any()
                    }}
                </tr>
            </thead>
            <tbody>
            {rows.into_iter().map(|r| {
                if is_partition {
                    let share = if total > 0 { r.population as f64 / total as f64 } else { 0.0 };
                    view! {
                        <tr class="table__row">
                            <td class="table__cell">{r.label.clone()}</td>
                            <td class="table__cell" style="text-align: right; font-variant-numeric: tabular-nums; font-weight: 600;">{r.population.to_string()}</td>
                            <td class="table__cell" style="text-align: right; font-variant-numeric: tabular-nums; color: var(--color-text-secondary);">{fmt_pct(share)}</td>
                        </tr>
                    }.into_any()
                } else {
                    let rate = r.compliance_rate();
                    view! {
                        <tr class="table__row">
                            <td class="table__cell">{r.label.clone()}</td>
                            <td class="table__cell" style="text-align: right; font-variant-numeric: tabular-nums; color: var(--color-text-secondary);">{r.population.to_string()}</td>
                            <td class="table__cell" style="text-align: right; font-variant-numeric: tabular-nums; font-weight: 600; color: var(--color-error);">{r.violations.to_string()}</td>
                            <td class="table__cell" style=format!("text-align: right; font-variant-numeric: tabular-nums; font-weight: 600; color: {};", rate_color(rate))>{fmt_pct(rate)}</td>
                        </tr>
                    }.into_any()
                }
            }).collect_view()}
            </tbody>
        </table>
    }
}
