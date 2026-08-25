//! Карточка Этапа: что у него внутри.
//!
//! Этап устроен как quality-проверка — манифест плюс mjs в QuickJS, — но с
//! двумя отличиями, и карточка показывает именно их: множество **именованных
//! выходов** (по ним Процесс выбирает следующий Этап, поэтому они часть
//! контракта) и право **звать Действия**, то есть менять мир.
//!
//! Сухой прогон здесь же: допуск Этапа в работу — это просмотр плана эффектов
//! человеком (ADR-0011 п.8), а план получается только прогоном.

use contracts::processes::{
    DefinitionStatus, EdgeTarget, ProcessRecord, StageRecord, StageVerdict,
};
use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::shared::components::card_animated::CardAnimated;

use super::super::api;
use super::definitions::VersionHistory;
use super::parts::{
    counted, definition_status_badge, definition_status_label, input_skeleton, schema_fields,
    short_digest, CodeBlock, Disclosure, Fact, JsonBlock,
};

// ═══════════════════════════════════════════════════════════════════════
// Карточка Этапа
// ═══════════════════════════════════════════════════════════════════════

#[component]
pub fn StageCard(
    record: StageRecord,
    delay_ms: u32,
    processes: RwSignal<Vec<ProcessRecord>>,
    actions: RwSignal<Vec<api::ActionInfo>>,
    on_changed: Callback<()>,
) -> impl IntoView {
    let code = StoredValue::new(record.code.clone());
    let version = record.version;
    let status = record.status;
    let manifest = record.definition.manifest.clone();
    let script = record.definition.script.clone();
    let script_lines = script.lines().count();
    let digest = short_digest(&record.digest);
    let created = record.created_at.clone();
    let author = record.created_by.clone().unwrap_or_else(|| "—".to_string());

    let title = manifest.title.clone();
    let description = manifest.description.clone();
    let has_description = !description.is_empty();
    let outputs = manifest.outputs.clone();
    let capabilities = manifest.capabilities.clone();
    let input_schema = manifest.input_schema.clone();
    let entrypoint = manifest.entrypoint.clone();
    let export = manifest.export.clone();

    let reads: Vec<String> = capabilities
        .iter()
        .filter_map(|capability| capability.trim().strip_prefix("db:read:"))
        .map(|table| table.trim().to_string())
        .collect();
    let action_names: Vec<String> = capabilities
        .iter()
        .filter_map(|capability| capability.trim().strip_prefix("action:"))
        .map(|name| name.trim().to_string())
        .collect();
    let other: Vec<String> = capabilities
        .iter()
        .filter(|capability| {
            let capability = capability.trim();
            !capability.starts_with("db:read:") && !capability.starts_with("action:")
        })
        .cloned()
        .collect();

    let fields = input_schema.as_ref().map(schema_fields).unwrap_or_default();
    let error: RwSignal<Option<String>> = RwSignal::new(None);

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
                        {(status == DefinitionStatus::Draft)
                            .then(|| {
                                view! {
                                    <button
                                        class="button button--primary"
                                        on:click=move |_| {
                                            let code = code.get_value();
                                            spawn_local(async move {
                                                match api::activate_stage(&code, version).await {
                                                    Ok(_) => on_changed.run(()),
                                                    Err(message) => error.set(Some(message)),
                                                }
                                            });
                                        }
                                    >
                                        "Активировать"
                                    </button>
                                }
                            })}
                    </span>
                </div>

                {has_description
                    .then(|| view! { <p class="sys-processes__desc">{description}</p> })}

                <div class="sys-processes__facts">
                    <Fact label="Выходы">
                        {outputs
                            .clone()
                            .into_iter()
                            .map(|output| {
                                let described = output.data_schema.is_some();
                                view! {
                                    <div class="sys-processes__output">
                                        <span class="sys-processes__output-name">{output.name}</span>
                                        <span class="sys-processes__output-desc">
                                            {if output.description.is_empty() {
                                                "без описания".to_string()
                                            } else {
                                                output.description
                                            }}
                                        </span>
                                        {described
                                            .then(|| {
                                                view! {
                                                    <span class="badge badge--neutral">"схема данных"</span>
                                                }
                                            })}
                                    </div>
                                }
                            })
                            .collect_view()}
                    </Fact>

                    <Fact label="Читает">
                        {if reads.is_empty() {
                            view! {
                                <span class="sys-processes__fact-hint">"таблицы не запрошены"</span>
                            }
                                .into_any()
                        } else {
                            reads
                                .clone()
                                .into_iter()
                                .map(|table| {
                                    view! {
                                        <span class="sys-processes__mono sys-processes__cap-item">
                                            {table}
                                        </span>
                                    }
                                })
                                .collect_view()
                                .into_any()
                        }}
                    </Fact>

                    <Fact label="Меняет мир">
                        {
                            let names = action_names.clone();
                            move || {
                                let names = names.clone();
                                if names.is_empty() {
                                    return view! {
                                        <span class="sys-processes__fact-hint">
                                            "Действий не просит — Этап только читает и решает"
                                        </span>
                                    }
                                        .into_any();
                                }
                                let catalog = actions.get();
                                names
                                    .into_iter()
                                    .map(|name| {
                                        let info = catalog
                                            .iter()
                                            .find(|info| info.name == name)
                                            .cloned();
                                        view! {
                                            <div class="sys-processes__cap">
                                                <span class="sys-processes__mono">{name.clone()}</span>
                                                {match info {
                                                    Some(info) => {
                                                        let tables = info.write_tables.join(", ");
                                                        view! {
                                                            <span>{info.title.clone()}</span>
                                                            <span class=if info.reversible {
                                                                "badge badge--neutral"
                                                            } else {
                                                                "badge badge--warning"
                                                            }>
                                                                {if info.reversible {
                                                                    "обратимо"
                                                                } else {
                                                                    "необратимо"
                                                                }}
                                                            </span>
                                                            {(!tables.is_empty())
                                                                .then(|| {
                                                                    view! {
                                                                        <span class="sys-processes__fact-hint">
                                                                            {format!("пишет: {tables}")}
                                                                        </span>
                                                                    }
                                                                })}
                                                        }
                                                            .into_any()
                                                    }
                                                    None => {
                                                        view! {
                                                            <span class="badge badge--error">
                                                                "нет в каталоге Действий"
                                                            </span>
                                                        }
                                                            .into_any()
                                                    }
                                                }}
                                            </div>
                                        }
                                    })
                                    .collect_view()
                                    .into_any()
                            }
                        }
                    </Fact>

                    {(!other.is_empty())
                        .then(|| {
                            view! {
                                <Fact label="Прочие права">
                                    {other
                                        .clone()
                                        .into_iter()
                                        .map(|capability| {
                                            view! {
                                                <span class="sys-processes__mono sys-processes__cap-item">
                                                    {capability}
                                                </span>
                                            }
                                        })
                                        .collect_view()}
                                </Fact>
                            }
                        })}

                    <Fact label="Вход">
                        {if fields.is_empty() {
                            view! {
                                <span class="sys-processes__fact-hint">
                                    "схема не описана — вход не проверяется"
                                </span>
                            }
                                .into_any()
                        } else {
                            fields
                                .clone()
                                .into_iter()
                                .map(|field| {
                                    view! {
                                        <div class="sys-processes__output">
                                            <span class="sys-processes__output-name">{field.name}</span>
                                            <span class="sys-processes__output-desc">{field.kind}</span>
                                            {field
                                                .required
                                                .then(|| {
                                                    view! {
                                                        <span class="badge badge--primary">"обязательное"</span>
                                                    }
                                                })}
                                            {(!field.note.is_empty())
                                                .then(|| {
                                                    view! {
                                                        <span class="sys-processes__fact-hint">{field.note}</span>
                                                    }
                                                })}
                                        </div>
                                    }
                                })
                                .collect_view()
                                .into_any()
                        }}
                    </Fact>

                    <Fact label="Модуль">
                        <span class="sys-processes__mono">
                            {format!("{entrypoint} → {export}()")}
                        </span>
                        <span class="sys-processes__fact-hint">
                            {format!(
                                " · {} · отпечаток {digest} · заведена {created}, автор {author}",
                                counted(script_lines, "строка", "строки", "строк"),
                            )}
                        </span>
                    </Fact>

                    <Fact label="Где используется">
                        {move || {
                            let usage = stage_usage(&code.get_value(), &processes.get());
                            if usage.is_empty() {
                                view! {
                                    <span class="sys-processes__fact-hint">
                                        "ни один Процесс на этот Этап не ссылается"
                                    </span>
                                }
                                    .into_any()
                            } else {
                                usage
                                    .into_iter()
                                    .map(|line| view! { <div>{line}</div> })
                                    .collect_view()
                                    .into_any()
                            }
                        }}
                    </Fact>
                </div>

                {input_schema
                    .clone()
                    .map(|schema| {
                        view! {
                            <Disclosure title="Схема входа" hint="JSON Schema">
                                <JsonBlock value=schema.clone() />
                            </Disclosure>
                        }
                    })}

                <Disclosure
                    title="Код Этапа"
                    hint=counted(script_lines, "строка", "строки", "строк")
                >
                    <CodeBlock text=script.clone() />
                </Disclosure>

                <Disclosure title="История версий">
                    <VersionHistory code=code.get_value() stage=true />
                </Disclosure>

                <Disclosure title="Сухой прогон">
                    <DryRunBlock
                        code=code.get_value()
                        version=version
                        skeleton=input_skeleton(input_schema.as_ref())
                    />
                </Disclosure>

                {move || {
                    error.get().map(|message| view! { <div class="alert alert--error">{message}</div> })
                }}
            </div>
        </CardAnimated>
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Сухой прогон
// ═══════════════════════════════════════════════════════════════════════

/// Прогон Этапа с записью плана эффектов вместо самих эффектов.
///
/// Вход редактируется руками и не подставляется «правдоподобным»: Этап пилота
/// требует кабинет и дату, и угаданные значения увели бы прогон в данные,
/// которых человек не выбирал. Заготовка показывает форму, значения — за ним.
#[component]
fn DryRunBlock(code: String, version: i32, skeleton: String) -> impl IntoView {
    let code = StoredValue::new(code);
    let input = RwSignal::new(skeleton);
    let result: RwSignal<Option<contracts::processes::StageRun>> = RwSignal::new(None);
    let error: RwSignal<Option<String>> = RwSignal::new(None);
    let busy = RwSignal::new(false);

    let run = move || {
        let raw = input.get_untracked();
        let value = match serde_json::from_str::<serde_json::Value>(&raw) {
            Ok(value) => value,
            Err(parse_error) => {
                error.set(Some(format!("Вход не разобран как JSON: {parse_error}")));
                return;
            }
        };
        busy.set(true);
        error.set(None);
        spawn_local(async move {
            match api::dry_run_stage(&code.get_value(), version, &value).await {
                Ok(run) => result.set(Some(run)),
                Err(message) => error.set(Some(message)),
            }
            busy.set(false);
        });
    };

    view! {
        <div class="sys-processes__dry">
            <div class="sys-processes__note">
                "Этап исполняется по-настоящему, но все его Действия идут сухим прогоном: "
                "в журнал эффектов ложится план, мир не меняется."
            </div>
            <textarea
                class="sys-processes__dry-input"
                rows="6"
                spellcheck="false"
                prop:value=move || input.get()
                on:input=move |event| input.set(event_target_value(&event))
            ></textarea>
            <div>
                <button
                    class="button button--primary"
                    disabled=move || busy.get()
                    on:click=move |_| run()
                >
                    {move || if busy.get() { "Прогон…" } else { "Прогнать" } }
                </button>
            </div>

            {move || {
                error.get().map(|message| view! { <div class="alert alert--error">{message}</div> })
            }}

            {move || {
                result
                    .get()
                    .map(|run| {
                        let logs = run.logs.clone();
                        let effects = run.effect_ids.len();
                        view! {
                            <div class="sys-processes__dry-result">
                                {match run.verdict.clone() {
                                    StageVerdict::Outcome(outcome) => {
                                        view! {
                                            <div class="sys-processes__cap">
                                                <span class="badge badge--success">"выход графа"</span>
                                                <span class="sys-processes__output-name">
                                                    {outcome.outcome.clone()}
                                                </span>
                                            </div>
                                            {(!outcome.data.is_null())
                                                .then(|| view! { <JsonBlock value=outcome.data.clone() /> })}
                                        }
                                            .into_any()
                                    }
                                    StageVerdict::TemporaryFailure { message } => {
                                        view! {
                                            <div class="sys-processes__cap">
                                                <span class="badge badge--warning">"временный сбой"</span>
                                                <span>{message}</span>
                                            </div>
                                        }
                                            .into_any()
                                    }
                                    StageVerdict::Defect { message } => {
                                        view! {
                                            <div class="sys-processes__cap">
                                                <span class="badge badge--error">"дефект Этапа"</span>
                                                <span>{message}</span>
                                            </div>
                                        }
                                            .into_any()
                                    }
                                }}
                                <div class="sys-processes__fact-hint">
                                    {format!(
                                        "{} мс · {}",
                                        run.duration_ms,
                                        counted(effects, "запись", "записи", "записей"),
                                    )}
                                    " в журнале эффектов"
                                </div>
                                {(!logs.is_empty())
                                    .then(|| {
                                        view! {
                                            <CodeBlock text=logs.join("\n") />
                                        }
                                    })}
                            </div>
                        }
                    })
            }}
        </div>
    }
}

/// В каких Процессах стоит этот Этап и в какой роли.
fn stage_usage(code: &str, processes: &[ProcessRecord]) -> Vec<String> {
    processes
        .iter()
        .filter_map(|record| {
            let manifest = &record.definition.manifest;
            let mut roles: Vec<&str> = Vec::new();
            if manifest.entry == code {
                roles.push("вход");
            }
            if manifest
                .edges
                .iter()
                .any(|edge| edge.to.stage_code() == Some(code))
            {
                roles.push("цель ребра");
            }
            if manifest.edges.iter().any(|edge| {
                edge.wait
                    .as_ref()
                    .and_then(|wait| wait.on_timeout.as_ref())
                    .and_then(EdgeTarget::stage_code)
                    == Some(code)
            }) {
                roles.push("запасной по дедлайну");
            }
            if roles.is_empty() && manifest.edges.iter().any(|edge| edge.from == code) {
                roles.push("источник рёбер, но недостижим");
            }
            if roles.is_empty() {
                return None;
            }
            Some(format!(
                "{} v{} «{}» — {}",
                record.code,
                record.version,
                manifest.title,
                roles.join(", "),
            ))
        })
        .collect()
}
