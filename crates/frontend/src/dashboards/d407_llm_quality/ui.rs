//! Дашборд качества работы LLM-подсистемы.
//!
//! Показывает то, что до сих пор копилось без потребителя: отказы и латентность
//! инструментов (`sys_tool_trace`), вердикты о качестве ответов (`sys_llm_verdict`)
//! и наблюдаемую ценность статей базы знаний. Смысл страницы — дать базовую линию,
//! относительно которой видно, помогают ли правки промптов и навыков.

use crate::dashboards::d407_llm_quality::api;
use crate::shared::page_frame::PageFrame;
use contracts::dashboards::d407_llm_quality::{
    CostStat, LlmQualityOverview, RecentVerdict, SkillVerdictStat, ToolStat,
};
use leptos::prelude::*;
use leptos::task::spawn_local;

fn fmt_pct(value: f64) -> String {
    format!("{:.1}%", value * 100.0)
}

fn fmt_ms(value: f64) -> String {
    if value >= 1000.0 {
        format!("{:.1} с", value / 1000.0)
    } else {
        format!("{value:.0} мс")
    }
}

/// Доля успешных вердиктов по источнику оценки.
fn solved_share(overview: &LlmQualityOverview, source: &str) -> Option<(i64, i64)> {
    let total: i64 = overview
        .verdicts
        .iter()
        .filter(|item| item.source == source)
        .map(|item| item.n)
        .sum();
    if total == 0 {
        return None;
    }
    let solved: i64 = overview
        .verdicts
        .iter()
        .filter(|item| item.source == source && item.verdict == "solved")
        .map(|item| item.n)
        .sum();
    Some((solved, total))
}

fn verdict_class(verdict: &str) -> &'static str {
    match verdict {
        "solved" => "d407-badge d407-badge--ok",
        "partial" => "d407-badge d407-badge--warn",
        _ => "d407-badge d407-badge--bad",
    }
}

#[component]
fn StatTile(label: &'static str, value: String, hint: String) -> impl IntoView {
    view! {
        <div class="d407-tile">
            <div class="d407-tile__label">{label}</div>
            <div class="d407-tile__value">{value}</div>
            <div class="d407-tile__hint">{hint}</div>
        </div>
    }
}

#[component]
fn ToolTable(tools: Vec<ToolStat>) -> impl IntoView {
    if tools.is_empty() {
        return view! { <div class="d407-empty">"Вызовов инструментов за период нет."</div> }
            .into_any();
    }
    view! {
        <div class="d407-tablewrap">
            <table class="d407-table">
                <thead>
                    <tr>
                        <th class="d407-th--left">"Инструмент"</th>
                        <th>"Вызовов"</th>
                        <th>"Отказов"</th>
                        <th>"Доля отказов"</th>
                        <th>"Средн. время"</th>
                        <th>"Макс."</th>
                    </tr>
                </thead>
                <tbody>
                    {tools
                        .into_iter()
                        .map(|tool| {
                            let rate = tool.failure_rate();
                            let row_class = if rate >= 0.2 {
                                "d407-row--bad"
                            } else if rate > 0.0 {
                                "d407-row--warn"
                            } else {
                                ""
                            };
                            view! {
                                <tr class=row_class>
                                    <td class="d407-td--left">{tool.tool.clone()}</td>
                                    <td>{tool.calls}</td>
                                    <td>{tool.failures}</td>
                                    <td>{fmt_pct(rate)}</td>
                                    <td>{fmt_ms(tool.avg_ms)}</td>
                                    <td>{fmt_ms(tool.max_ms as f64)}</td>
                                </tr>
                            }
                        })
                        .collect_view()}
                </tbody>
            </table>
        </div>
    }
    .into_any()
}

#[component]
fn SkillTable(rows: Vec<SkillVerdictStat>) -> impl IntoView {
    if rows.is_empty() {
        return view! {
            <div class="d407-empty">
                "Вердиктов пока нет. Запусти задание «LLM — оценка качества ответов»."
            </div>
        }
        .into_any();
    }
    view! {
        <div class="d407-tablewrap">
            <table class="d407-table">
                <thead>
                    <tr>
                        <th class="d407-th--left">"Навык"</th>
                        <th>"Оценок"</th>
                        <th>"Решено"</th>
                        <th>"Провалов"</th>
                        <th>"Доля решённых"</th>
                    </tr>
                </thead>
                <tbody>
                    {rows
                        .into_iter()
                        .map(|row| {
                            let share = if row.total > 0 {
                                row.solved as f64 / row.total as f64
                            } else {
                                0.0
                            };
                            view! {
                                <tr>
                                    <td class="d407-td--left">{row.skill_id.clone()}</td>
                                    <td>{row.total}</td>
                                    <td>{row.solved}</td>
                                    <td>{row.failed}</td>
                                    <td>{fmt_pct(share)}</td>
                                </tr>
                            }
                        })
                        .collect_view()}
                </tbody>
            </table>
        </div>
    }
    .into_any()
}

/// Стоимость по валютам. Отдельной таблицей, а не плиткой: валюты не
/// складываются, и одной цифрой сводку честно не показать.
#[component]
fn CostTable(rows: Vec<CostStat>) -> impl IntoView {
    if rows.is_empty() {
        return view! {
            <div class="d407-empty">
                "Стоимость не считается: у подключений (a038) не заполнен прайс за 1M токенов."
            </div>
        }
        .into_any();
    }
    view! {
        <div class="d407-tablewrap">
            <table class="d407-table">
                <thead>
                    <tr>
                        <th class="d407-th--left">"Валюта"</th>
                        <th>"Всего"</th>
                        <th>"Ответов"</th>
                        <th>"Средняя за ответ"</th>
                        <th>"Средняя за решённую"</th>
                    </tr>
                </thead>
                <tbody>
                    {rows
                        .into_iter()
                        .map(|row| {
                            let per_solved = row
                                .avg_per_solved()
                                .map(|value| format!("{value:.4}"))
                                .unwrap_or_else(|| "—".to_string());
                            view! {
                                <tr>
                                    <td class="d407-td--left">{row.currency.clone()}</td>
                                    <td>{format!("{:.2}", row.total())}</td>
                                    <td>{row.answers}</td>
                                    <td>{format!("{:.4}", row.avg_per_answer())}</td>
                                    <td>{per_solved}</td>
                                </tr>
                            }
                        })
                        .collect_view()}
                </tbody>
            </table>
        </div>
    }
    .into_any()
}

#[component]
fn VerdictList(rows: Vec<RecentVerdict>) -> impl IntoView {
    if rows.is_empty() {
        return view! { <div class="d407-empty">"Оценок за период нет."</div> }.into_any();
    }
    view! {
        <div class="d407-verdicts">
            {rows
                .into_iter()
                .map(|row| {
                    let title = row
                        .chat_title
                        .clone()
                        .filter(|value| !value.is_empty())
                        .unwrap_or_else(|| row.chat_id.clone());
                    let meta = [
                        row.case_id.clone().filter(|v| !v.is_empty()),
                        row.skill_id.clone().filter(|v| !v.is_empty()),
                        row.agent_type.clone().filter(|v| !v.is_empty()),
                        row.failure_kind.clone().filter(|v| !v.is_empty()),
                    ]
                    .into_iter()
                    .flatten()
                    .collect::<Vec<_>>()
                    .join(" · ");
                    view! {
                        <div class="d407-verdict">
                            <div class="d407-verdict__head">
                                <span class=verdict_class(&row.verdict)>{row.verdict.clone()}</span>
                                <span class="d407-verdict__title">{title}</span>
                                <span class="d407-verdict__date">{row.created_at.clone()}</span>
                            </div>
                            <div class="d407-verdict__reason">{row.reason.clone()}</div>
                            <div class="d407-verdict__meta">{meta}</div>
                        </div>
                    }
                })
                .collect_view()}
        </div>
    }
    .into_any()
}

#[component]
pub fn LlmQualityDashboard() -> impl IntoView {
    let days = RwSignal::new(30_i64);
    let data = RwSignal::new(None::<LlmQualityOverview>);
    let loading = RwSignal::new(false);
    let error = RwSignal::new(None::<String>);

    let load = move || {
        loading.set(true);
        error.set(None);
        let window = days.get_untracked();
        spawn_local(async move {
            match api::get_llm_quality_overview(window).await {
                Ok(response) => {
                    data.set(Some(response));
                    loading.set(false);
                }
                Err(message) => {
                    error.set(Some(message));
                    loading.set(false);
                }
            }
        });
    };

    // Первая загрузка при открытии вкладки.
    Effect::new(move |_| {
        load();
    });

    view! {
        <PageFrame
            page_id="d407_llm_quality--dashboard"
            category="dashboard"
            class="page--wide d407-page"
        >
            <div class="d407-shell">
                <div class="d407-head">
                    <h1 class="d407-title">"Качество работы агентов"</h1>
                    <div class="d407-note">
                        "Отказы и латентность инструментов, вердикты о качестве ответов и ценность "
                        "статей базы знаний. Вердикты ставит задание «LLM — оценка качества ответов»; "
                        "регрессию на неизменном наборе вопросов даёт «LLM CI»."
                    </div>
                </div>

                <div class="d407-toolbar">
                    <div class="d407-field">
                        <label>"Период, дней"</label>
                        <select
                            prop:value=move || days.get().to_string()
                            on:change=move |ev| {
                                if let Ok(value) = event_target_value(&ev).parse::<i64>() {
                                    days.set(value);
                                    load();
                                }
                            }
                        >
                            <option value="7">"7"</option>
                            <option value="30">"30"</option>
                            <option value="90">"90"</option>
                        </select>
                    </div>
                    <button
                        class="d407-btn d407-btn--primary"
                        on:click=move |_| load()
                        disabled=move || loading.get()
                    >
                        "Обновить"
                    </button>
                </div>

                {move || {
                    error
                        .get()
                        .map(|message| view! { <div class="d407-state d407-state--error">{message}</div> })
                }}

                {move || {
                    if loading.get() && data.get().is_none() {
                        return view! { <div class="d407-state">"Загрузка..."</div> }.into_any();
                    }
                    let Some(overview) = data.get() else {
                        return view! { <div class="d407-state">"Нет данных."</div> }.into_any();
                    };

                    let audit = solved_share(&overview, "audit");
                    let golden = solved_share(&overview, "golden");
                    let total_calls: i64 = overview.tools.iter().map(|t| t.calls).sum();
                    let total_failures: i64 = overview.tools.iter().map(|t| t.failures).sum();
                    let failure_rate = if total_calls > 0 {
                        total_failures as f64 / total_calls as f64
                    } else {
                        0.0
                    };

                    view! {
                        <div class="d407-tiles">
                            <StatTile
                                label="Отказы инструментов"
                                value=fmt_pct(failure_rate)
                                hint=format!("{total_failures} из {total_calls} вызовов")
                            />
                            <StatTile
                                label="Решённые диалоги"
                                value=audit
                                    .map(|(solved, total)| {
                                        fmt_pct(solved as f64 / total as f64)
                                    })
                                    .unwrap_or_else(|| "—".to_string())
                                hint=audit
                                    .map(|(solved, total)| format!("{solved} из {total} оценок"))
                                    .unwrap_or_else(|| "оценок ещё нет".to_string())
                            />
                            <StatTile
                                label="Голден-сет"
                                value=golden
                                    .map(|(solved, total)| {
                                        fmt_pct(solved as f64 / total as f64)
                                    })
                                    .unwrap_or_else(|| "—".to_string())
                                hint=golden
                                    .map(|(solved, total)| format!("{solved} из {total} кейсов"))
                                    .unwrap_or_else(|| "прогонов ещё нет".to_string())
                            />
                            <StatTile
                                label="Итераций на ответ"
                                value=format!("{:.1}", overview.iterations.avg_iterations)
                                hint=format!(
                                    "максимум {} · тяжёлых ответов {}",
                                    overview.iterations.max_iterations,
                                    overview.iterations.heavy_answers,
                                )
                            />
                        </div>

                        <div class="d407-section">
                            <h2 class="d407-subtitle">"Стоимость прогонов"</h2>
                            <CostTable rows=overview.costs.clone() />
                        </div>

                        <div class="d407-section">
                            <h2 class="d407-subtitle">"Инструменты"</h2>
                            <ToolTable tools=overview.tools.clone() />
                        </div>

                        <div class="d407-section">
                            <h2 class="d407-subtitle">"Качество по навыкам"</h2>
                            <SkillTable rows=overview.by_skill.clone() />
                        </div>

                        <div class="d407-section">
                            <h2 class="d407-subtitle">"Причины провалов"</h2>
                            {if overview.failure_kinds.is_empty() {
                                view! { <div class="d407-empty">"Провалов не зафиксировано."</div> }
                                    .into_any()
                            } else {
                                view! {
                                    <div class="d407-chips">
                                        {overview
                                            .failure_kinds
                                            .iter()
                                            .map(|item| {
                                                view! {
                                                    <span class="d407-chip">
                                                        {item.failure_kind.clone()}
                                                        <b>{item.n}</b>
                                                    </span>
                                                }
                                            })
                                            .collect_view()}
                                    </div>
                                }
                                    .into_any()
                            }}
                        </div>

                        <div class="d407-section">
                            <h2 class="d407-subtitle">"Ценность статей базы знаний"</h2>
                            {if overview.kb_articles.is_empty() {
                                view! {
                                    <div class="d407-empty">"Обращений к статьям за период нет."</div>
                                }
                                    .into_any()
                            } else {
                                view! {
                                    <div class="d407-tablewrap">
                                        <table class="d407-table">
                                            <thead>
                                                <tr>
                                                    <th class="d407-th--left">"Статья"</th>
                                                    <th>"Найдена"</th>
                                                    <th>"Прочитана"</th>
                                                    <th>"Процитирована"</th>
                                                    <th>"Замечаний"</th>
                                                </tr>
                                            </thead>
                                            <tbody>
                                                {overview
                                                    .kb_articles
                                                    .iter()
                                                    .map(|article| {
                                                        let title = if article.title.is_empty() {
                                                            article.doc_id.clone()
                                                        } else {
                                                            article.title.clone()
                                                        };
                                                        view! {
                                                            <tr>
                                                                <td class="d407-td--left">{title}</td>
                                                                <td>{article.search_hits}</td>
                                                                <td>{article.read_hits}</td>
                                                                <td>{article.cited_hits}</td>
                                                                <td>{article.open_issue_count}</td>
                                                            </tr>
                                                        }
                                                    })
                                                    .collect_view()}
                                            </tbody>
                                        </table>
                                    </div>
                                }
                                    .into_any()
                            }}
                        </div>

                        <div class="d407-section">
                            <h2 class="d407-subtitle">"Последние оценки"</h2>
                            <VerdictList rows=overview.recent_verdicts.clone() />
                        </div>
                    }
                        .into_any()
                }}
            </div>
        </PageFrame>
    }
}
