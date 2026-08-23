//! Карточка Процесса: паспорт и граф.
//!
//! Граф рисуется по **объявленным выходам Этапов**, а не по рёбрам. Разница
//! видна ровно там, где она важна: выход без ребра — почти всегда забытое
//! ребро, и увидеть его надо здесь, а не на живом экземпляре, который в этот
//! выход уже ушёл.
//!
//! Критичность здесь **не выводится**. Её считает бэкенд в плане активации, и
//! второй счётчик тех же правил на фронте разошёлся бы с гейтом молча. Карточка
//! показывает факты, из которых критичность берётся, — какие Действия просит
//! каждый Этап и обратимы ли они, — а вердикт остаётся за планом.

use contracts::processes::{
    ActivationPlan, DefinitionStatus, EdgeTarget, ProcessCriticality, ProcessEdge, ProcessManifest,
    ProcessRecord, StageRecord,
};
use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::shared::components::card_animated::CardAnimated;

use super::super::api;
use super::definitions::VersionHistory;
use super::parts::{
    counted, deadline_label, definition_status_badge, definition_status_label, find_stage,
    short_digest, stage_ref_label, Disclosure, Fact,
};

// ═══════════════════════════════════════════════════════════════════════
// Карточка Процесса
// ═══════════════════════════════════════════════════════════════════════

#[component]
pub fn ProcessCard(
    record: ProcessRecord,
    delay_ms: u32,
    stages: RwSignal<Vec<StageRecord>>,
    actions: RwSignal<Vec<api::ActionInfo>>,
    events: RwSignal<Vec<api::EventKindInfo>>,
    on_changed: Callback<()>,
) -> impl IntoView {
    let code = StoredValue::new(record.code.clone());
    let version = record.version;
    let status = record.status;
    let manifest = StoredValue::new(record.definition.manifest.clone());
    let digest = short_digest(&record.digest);
    let created = record.created_at.clone();
    let author = record.created_by.clone().unwrap_or_else(|| "—".to_string());

    let plan: RwSignal<Option<ActivationPlan>> = RwSignal::new(None);
    let error: RwSignal<Option<String>> = RwSignal::new(None);

    let title = record.definition.manifest.title.clone();
    let description = record.definition.manifest.description.clone();
    let has_description = !description.is_empty();
    let stage_count = record.definition.manifest.stage_codes().len();
    let edge_count = record.definition.manifest.edges.len();
    let check = record.definition.manifest.quality_check.clone();
    let entry = record.definition.manifest.entry.clone();
    let trigger_event = record.definition.manifest.trigger.event.clone();

    view! {
        <CardAnimated delay_ms=delay_ms>
            <div class="sys-processes__card">
                <div class="sys-processes__card-head">
                    <span class="sys-processes__card-code">
                        {format!("{} v{version}", code.get_value())}
                    </span>
                    <span class=definition_status_badge(status)>
                        {definition_status_label(status)}
                    </span>
                    <span class="sys-processes__card-title">{title}</span>
                    <span class="sys-processes__card-actions">
                        <button
                            class="button button--ghost"
                            on:click=move |_| {
                                let code = code.get_value();
                                spawn_local(async move {
                                    match api::activation_plan(&code, version).await {
                                        Ok(value) => {
                                            error.set(None);
                                            plan.set(Some(value));
                                        }
                                        Err(message) => error.set(Some(message)),
                                    }
                                });
                            }
                        >
                            "План активации"
                        </button>
                        {(status == DefinitionStatus::Active)
                            .then(|| {
                                view! {
                                    <button
                                        class="button button--ghost"
                                        on:click=move |_| {
                                            let code = code.get_value();
                                            spawn_local(async move {
                                                match api::deactivate_process(&code).await {
                                                    Ok(_) => on_changed.run(()),
                                                    Err(message) => error.set(Some(message)),
                                                }
                                            });
                                        }
                                    >
                                        "Снять с работы"
                                    </button>
                                }
                            })}
                    </span>
                </div>

                {has_description
                    .then(|| view! { <p class="sys-processes__desc">{description}</p> })}

                <div class="sys-processes__facts">
                    <Fact label="Триггер">
                        {
                            let event = trigger_event.clone();
                            move || {
                                let event = event.clone();
                                let known = events
                                    .get()
                                    .into_iter()
                                    .find(|kind| kind.name == event);
                                match known {
                                    Some(kind) => {
                                        view! {
                                            <span class="sys-processes__mono">{kind.name.clone()}</span>
                                            <span class="sys-processes__fact-hint">
                                                {format!(
                                                    " · {} · ключ: {}",
                                                    kind.title,
                                                    kind.correlation.join(", "),
                                                )}
                                            </span>
                                        }
                                            .into_any()
                                    }
                                    None => {
                                        view! {
                                            <span class="sys-processes__mono">{event}</span>
                                            <span class="badge badge--error">"нет в каталоге"</span>
                                        }
                                            .into_any()
                                    }
                                }
                            }
                        }
                    </Fact>

                    <Fact label="Вход">
                        {
                            let entry = entry.clone();
                            move || {
                                let entry = entry.clone();
                                view! { <span>{stage_ref_label(&entry, &stages.get())}</span> }
                            }
                        }
                    </Fact>

                    <Fact label="Размер графа">
                        {format!(
                            "{}, {}",
                            counted(stage_count, "Этап", "Этапа", "Этапов"),
                            counted(edge_count, "ребро", "ребра", "рёбер"),
                        )}
                    </Fact>

                    <Fact label="Парная проверка">
                        {match check.clone() {
                            Some(check) => {
                                view! { <span class="sys-processes__mono">{check}</span> }.into_any()
                            }
                            None => {
                                view! {
                                    <span class="sys-processes__fact-hint">
                                        "не задана — Процесс с эффектами без неё не активируется"
                                    </span>
                                }
                                    .into_any()
                            }
                        }}
                    </Fact>

                    <Fact label="Эффекты Этапов">
                        {move || {
                            let calls = process_effects(
                                &manifest.get_value(),
                                &stages.get(),
                                &actions.get(),
                            );
                            if calls.is_empty() {
                                view! {
                                    <span class="sys-processes__fact-hint">
                                        "ни один Этап не просит Действий — Процесс только читает и решает"
                                    </span>
                                }
                                    .into_any()
                            } else {
                                calls
                                    .into_iter()
                                    .map(|call| {
                                        view! {
                                            <div class="sys-processes__cap">
                                                <span class="sys-processes__mono">{call.stage_code}</span>
                                                <span class="sys-processes__graph-arrow">"→"</span>
                                                <span class="sys-processes__mono">{call.action}</span>
                                                <span>{call.title}</span>
                                                <span class=if call.reversible {
                                                    "badge badge--neutral"
                                                } else {
                                                    "badge badge--warning"
                                                }>
                                                    {if call.reversible {
                                                        "обратимо"
                                                    } else {
                                                        "необратимо"
                                                    }}
                                                </span>
                                            </div>
                                        }
                                    })
                                    .collect_view()
                                    .into_any()
                            }
                        }}
                    </Fact>

                    <Fact label="Отпечаток">
                        <span class="sys-processes__mono">{digest}</span>
                        <span class="sys-processes__fact-hint">
                            {format!(" · заведена {created}, автор {author}")}
                        </span>
                    </Fact>
                </div>

                <div class="sys-processes__block-title">"Граф"</div>
                {move || graph_view(&manifest.get_value(), &stages.get())}

                <Disclosure title="История версий">
                    <VersionHistory code=code.get_value() stage=false />
                </Disclosure>

                {move || {
                    error.get().map(|message| view! { <div class="alert alert--error">{message}</div> })
                }}

                {move || {
                    plan.get()
                        .map(|value| {
                            view! {
                                <ActivationPlanBlock
                                    code=code.get_value()
                                    version=version
                                    plan=value
                                    on_done=on_changed
                                />
                            }
                        })
                }}
            </div>
        </CardAnimated>
    }
}

// ═══════════════════════════════════════════════════════════════════════
// План активации
// ═══════════════════════════════════════════════════════════════════════

/// План активации: что изменится и почему активации может не быть.
#[component]
fn ActivationPlanBlock(
    code: String,
    version: i32,
    plan: ActivationPlan,
    on_done: Callback<()>,
) -> impl IntoView {
    let allowed = plan.problems.is_empty();
    let criticality = plan.criticality;
    let process_changes = plan.process.changes.clone();
    let stage_diffs = plan.stages.clone();
    let pins = plan.pinned_stages.clone();
    let problems = plan.problems.clone();
    let activate_code = code.clone();

    view! {
        <div class="sys-processes__details">
            <div class="sys-processes__block-title">
                {format!("План активации {code} v{version}")}
            </div>
            <div class="sys-processes__cap">
                <span class="sys-processes__fact-key">"Критичность"</span>
                <span class=criticality_badge(criticality)>{criticality_label(criticality)}</span>
                <span class="sys-processes__fact-hint">
                    "Критичный Процесс не активируется без парной quality-проверки."
                </span>
            </div>

            <div class="sys-processes__note">"Изменения графа:"</div>
            <ul class="sys-processes__list">
                {if process_changes.is_empty() {
                    vec![view! { <li>{"граф не менялся".to_string()}</li> }]
                } else {
                    process_changes
                        .into_iter()
                        .map(|change| view! { <li>{change}</li> })
                        .collect()
                }}
            </ul>

            <div class="sys-processes__note">"Изменения Этапов под графом:"</div>
            <ul class="sys-processes__list">
                {if stage_diffs.is_empty() {
                    vec![view! { <li>{"Этапы не менялись".to_string()}</li> }]
                } else {
                    stage_diffs
                        .into_iter()
                        .map(|diff| {
                            let line = format!(
                                "{} {} → v{}: {}",
                                diff.code,
                                diff.from_version
                                    .map(|version| format!("v{version}"))
                                    .unwrap_or_else(|| "—".to_string()),
                                diff.to_version,
                                diff.changes.join("; "),
                            );
                            view! { <li>{line}</li> }
                        })
                        .collect()
                }}
            </ul>

            <div class="sys-processes__note">"Версии Этапов, которые запинит активация:"</div>
            <ul class="sys-processes__list">
                {pins
                    .into_iter()
                    .map(|pin| view! { <li>{format!("{} v{}", pin.code, pin.version)}</li> })
                    .collect::<Vec<_>>()}
            </ul>

            {(!problems.is_empty())
                .then(|| {
                    view! {
                        <div class="alert alert--error">
                            <ul class="sys-processes__list">
                                {problems
                                    .into_iter()
                                    .map(|problem| view! { <li>{problem}</li> })
                                    .collect::<Vec<_>>()}
                            </ul>
                        </div>
                    }
                })}

            {allowed
                .then(|| {
                    view! {
                        <button
                            class="button button--primary"
                            on:click=move |_| {
                                let code = activate_code.clone();
                                spawn_local(async move {
                                    let _ = api::activate_process(&code, version).await;
                                    on_done.run(());
                                });
                            }
                        >
                            "Активировать"
                        </button>
                    }
                })}
        </div>
    }
}

fn criticality_label(criticality: ProcessCriticality) -> &'static str {
    match criticality {
        ProcessCriticality::ReadOnly => "только чтение",
        ProcessCriticality::Effectful => "есть эффекты",
        ProcessCriticality::Irreversible => "есть необратимые эффекты",
    }
}

fn criticality_badge(criticality: ProcessCriticality) -> &'static str {
    match criticality {
        ProcessCriticality::ReadOnly => "badge badge--success",
        ProcessCriticality::Effectful => "badge badge--warning",
        ProcessCriticality::Irreversible => "badge badge--error",
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Граф
// ═══════════════════════════════════════════════════════════════════════

fn edge_target_label(target: &EdgeTarget, stages: &[StageRecord]) -> String {
    match target {
        EdgeTarget::Done => "готово — экземпляр завершён".to_string(),
        EdgeTarget::Stage { code } => stage_ref_label(code, stages),
    }
}

/// Одна строка графа под Этапом: выход и то, куда он ведёт.
struct GraphRow {
    outcome: String,
    edge: Option<ProcessEdge>,
    /// Объявлен ли выход манифестом Этапа. Ребро по необъявленному выходу —
    /// дефект графа, и он должен быть виден, а не пропущен при отрисовке.
    declared: bool,
}

fn graph_rows(manifest: &ProcessManifest, code: &str, declared: &[String]) -> Vec<GraphRow> {
    let mut rows: Vec<GraphRow> = declared
        .iter()
        .map(|outcome| GraphRow {
            outcome: outcome.clone(),
            edge: manifest.edge(code, outcome).cloned(),
            declared: true,
        })
        .collect();

    for edge in manifest.edges.iter().filter(|edge| edge.from == code) {
        if !declared.iter().any(|name| name == &edge.outcome) {
            rows.push(GraphRow {
                outcome: edge.outcome.clone(),
                edge: Some(edge.clone()),
                declared: false,
            });
        }
    }

    rows
}

/// Граф Процесса: Этап, его объявленные выходы и цель каждого.
///
/// Рисуется по **объявленным выходам**, а не по рёбрам: выход без ребра — почти
/// всегда забытое ребро, и увидеть его надо здесь, а не на живом экземпляре.
fn graph_view(manifest: &ProcessManifest, stages: &[StageRecord]) -> AnyView {
    let entry = manifest.entry.clone();
    manifest
        .stage_codes()
        .into_iter()
        .map(|code| {
            let stage = find_stage(stages, &code);
            let title = stage.map(|record| record.definition.manifest.title.clone());
            let known = title.is_some();
            let status = stage.map(|record| record.status);
            let declared: Vec<String> = stage
                .map(|record| {
                    record
                        .definition
                        .manifest
                        .outputs
                        .iter()
                        .map(|output| output.name.clone())
                        .collect()
                })
                .unwrap_or_default();
            let rows = graph_rows(manifest, &code, &declared);
            let is_entry = code == entry;

            view! {
                <div class="sys-processes__graph-node">
                    <div class="sys-processes__graph-node-head">
                        <span class="sys-processes__mono">{code.clone()}</span>
                        <span class="sys-processes__graph-node-title">
                            {title.unwrap_or_else(|| "определения нет".to_string())}
                        </span>
                        {is_entry
                            .then(|| view! { <span class="badge badge--primary">"вход"</span> })}
                        {status
                            .map(|status| {
                                view! {
                                    <span class=definition_status_badge(status)>
                                        {definition_status_label(status)}
                                    </span>
                                }
                            })}
                        {(!known)
                            .then(|| {
                                view! { <span class="badge badge--error">"Этап не заведён"</span> }
                            })}
                    </div>
                    {rows
                        .into_iter()
                        .map(|row| graph_row_view(row, stages))
                        .collect_view()}
                </div>
            }
        })
        .collect_view()
        .into_any()
}

fn graph_row_view(row: GraphRow, stages: &[StageRecord]) -> AnyView {
    let GraphRow {
        outcome,
        edge,
        declared,
    } = row;

    let Some(edge) = edge else {
        return view! {
            <div class="sys-processes__graph-edge sys-processes__graph-edge--warn">
                <span class="sys-processes__graph-outcome">{outcome}</span>
                <span class="sys-processes__graph-arrow">"→"</span>
                <span class="sys-processes__graph-target">
                    "ребра нет — выход никуда не ведёт"
                </span>
            </div>
        }
        .into_any();
    };

    let target = edge_target_label(&edge.to, stages);
    let wait = edge.wait.clone().map(|wait| {
        let timeout = match wait.on_timeout.as_ref() {
            Some(target) => format!("по дедлайну → {}", edge_target_label(target, stages)),
            None => "по дедлайну остаётся человеку".to_string(),
        };
        format!(
            "перед переходом ждёт {} не дольше {}; {timeout}",
            wait.event,
            deadline_label(wait.deadline_minutes),
        )
    });

    view! {
        <div class="sys-processes__graph-edge">
            <span class="sys-processes__graph-outcome">{outcome}</span>
            {(!declared)
                .then(|| {
                    view! { <span class="badge badge--error">"выход не объявлен Этапом"</span> }
                })}
            <span class="sys-processes__graph-arrow">"→"</span>
            <span class="sys-processes__graph-target">{target}</span>
        </div>
        {wait
            .map(|text| view! { <div class="sys-processes__graph-wait">{text}</div> })}
    }
    .into_any()
}

/// Действие, которое просит Этап графа.
struct EffectCall {
    stage_code: String,
    action: String,
    title: String,
    reversible: bool,
}

/// Чем Процесс меняет мир: Действия всех Этапов его графа.
///
/// Неизвестное Действие считается необратимым — та же осторожность, что у
/// бэкендового счёта критичности: худшее предположение дешевле ошибки.
fn process_effects(
    manifest: &ProcessManifest,
    stages: &[StageRecord],
    actions: &[api::ActionInfo],
) -> Vec<EffectCall> {
    let mut calls = Vec::new();
    for code in manifest.stage_codes() {
        let Some(stage) = find_stage(stages, &code) else {
            continue;
        };
        for capability in &stage.definition.manifest.capabilities {
            let Some(name) = capability.trim().strip_prefix("action:") else {
                continue;
            };
            let name = name.trim();
            let info = actions.iter().find(|info| info.name == name);
            calls.push(EffectCall {
                stage_code: code.clone(),
                action: name.to_string(),
                title: info
                    .map(|info| info.title.clone())
                    .unwrap_or_else(|| "нет в каталоге Действий".to_string()),
                reversible: info.map(|info| info.reversible).unwrap_or(false),
            });
        }
    }
    calls
}
