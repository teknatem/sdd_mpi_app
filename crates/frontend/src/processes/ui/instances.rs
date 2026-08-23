//! Вкладка «Экземпляры»: что сейчас идёт и кого ждут.
//!
//! Здесь же инбокс. Кнопка «сделано» не двигает экземпляр — она публикует факт
//! `human.action.done`, а двинет его воркер (ADR-0011 п.9). Разница
//! принципиальна: «человек разобрал» — это событие домена, а не команда
//! конкретному прогону.

use contracts::processes::{InstanceDetails, InstanceStatus, ProcessInstance};
use leptos::prelude::*;
use leptos::task::spawn_local;

use super::super::api;
use super::parts::{CodeBlock, Disclosure, EffectsTable, JsonBlock};

pub fn status_label(status: InstanceStatus) -> &'static str {
    match status {
        InstanceStatus::Running => "идёт",
        InstanceStatus::Waiting => "ждёт",
        InstanceStatus::Done => "готово",
        InstanceStatus::Quarantined => "карантин",
    }
}

pub fn status_badge(status: InstanceStatus) -> &'static str {
    match status {
        InstanceStatus::Running => "badge badge--primary",
        InstanceStatus::Waiting => "badge badge--warning",
        InstanceStatus::Done => "badge badge--success",
        InstanceStatus::Quarantined => "badge badge--error",
    }
}

fn verdict_label(verdict: &str) -> &'static str {
    match verdict {
        "outcome" => "выход графа",
        "temporary_failure" => "временный сбой",
        _ => "дефект",
    }
}

fn verdict_badge(verdict: &str) -> &'static str {
    match verdict {
        "outcome" => "badge badge--success",
        "temporary_failure" => "badge badge--warning",
        _ => "badge badge--error",
    }
}

#[component]
pub fn InstancesTab(
    instances: RwSignal<Vec<ProcessInstance>>,
    /// Позвать после действия, у которого есть последствия: список надо
    /// перечитать, а механизм — двинуть.
    on_changed: Callback<()>,
) -> impl IntoView {
    let selected: RwSignal<Option<String>> = RwSignal::new(None);

    view! {
        <div class="sys-processes__section">
            {move || {
                instances
                    .get()
                    .is_empty()
                    .then(|| {
                        view! {
                            <div class="sys-processes__empty">
                                "Экземпляров нет. Прогон заводит событие-триггер активного Процесса."
                            </div>
                        }
                    })
            }}
            <div class="table-wrapper">
                <table class="table__data">
                    <thead>
                        <tr>
                            <th class="table__header-cell">"Процесс"</th>
                            <th class="table__header-cell">"Про что"</th>
                            <th class="table__header-cell">"Состояние"</th>
                            <th class="table__header-cell">"Курсор"</th>
                            <th class="table__header-cell">"Ждёт"</th>
                            <th class="table__header-cell">"Обновлён"</th>
                            <th class="table__header-cell">""</th>
                        </tr>
                    </thead>
                    <tbody>
                        <For
                            each=move || instances.get()
                            key=|instance| format!("{}:{}", instance.id, instance.updated_at)
                            let:instance
                        >
                            {
                                let id = instance.id.clone();
                                let open_id = id.clone();
                                let done_id = id.clone();
                                let status = instance.status;
                                let waiting = status == InstanceStatus::Waiting;
                                view! {
                                    <tr class="table__row">
                                        <td class="table__cell">
                                            {format!("{} v{}", instance.process_code, instance.process_version)}
                                        </td>
                                        <td class="table__cell">{instance.correlation_token.clone()}</td>
                                        <td class="table__cell">
                                            <span class=status_badge(status)>{status_label(status)}</span>
                                        </td>
                                        <td class="table__cell">
                                            {instance
                                                .stage_code
                                                .clone()
                                                .map(|code| format!("{code} · заход {}", instance.visit))
                                                .unwrap_or_else(|| "—".to_string())}
                                        </td>
                                        <td class="table__cell">
                                            {instance
                                                .wait
                                                .as_ref()
                                                .map(|wait| format!("{} до {}", wait.event, wait.deadline_at))
                                                .unwrap_or_else(|| "—".to_string())}
                                        </td>
                                        <td class="table__cell">{instance.updated_at.clone()}</td>
                                        <td class="table__cell table__cell--right">
                                            <button
                                                class="button button--ghost"
                                                on:click=move |_| {
                                                    let id = open_id.clone();
                                                    selected
                                                        .update(|current| {
                                                            *current = if current.as_deref() == Some(id.as_str()) {
                                                                None
                                                            } else {
                                                                Some(id)
                                                            };
                                                        })
                                                }
                                            >
                                                "Разбор"
                                            </button>
                                            {waiting
                                                .then(|| {
                                                    view! {
                                                        <button
                                                            class="button button--primary"
                                                            on:click=move |_| {
                                                                let id = done_id.clone();
                                                                spawn_local(async move {
                                                                    let _ = api::human_action_done(&id).await;
                                                                    on_changed.run(());
                                                                });
                                                            }
                                                        >
                                                            "Сделано"
                                                        </button>
                                                    }
                                                })}
                                        </td>
                                    </tr>
                                }
                            }
                        </For>
                    </tbody>
                </table>
            </div>

            {move || {
                selected
                    .get()
                    .map(|id| view! { <InstanceDetailsBlock id=id /> })
            }}
        </div>
    }
}

/// Разбор одного экземпляра: где стоим, как сюда пришли и что изменили.
#[component]
fn InstanceDetailsBlock(id: String) -> impl IntoView {
    let details: RwSignal<Option<InstanceDetails>> = RwSignal::new(None);
    let error: RwSignal<Option<String>> = RwSignal::new(None);

    let load_id = id.clone();
    Effect::new(move |_| {
        let id = load_id.clone();
        spawn_local(async move {
            match api::get_instance(&id).await {
                Ok(value) => details.set(Some(value)),
                Err(message) => error.set(Some(message)),
            }
        });
    });

    view! {
        <div class="sys-processes__details">
            {move || {
                error.get().map(|message| view! { <div class="alert alert--error">{message}</div> })
            }}
            {move || {
                details
                    .get()
                    .map(|value| {
                        let instance = value.instance.clone();
                        let steps = value.steps.clone();
                        let effects = value.effects.clone();
                        view! {
                            <div class="sys-processes__block-title">"Состояние"</div>
                            <InstanceFacts instance=instance />

                            <div class="sys-processes__block-title">"Шаги"</div>
                            <div class="table-wrapper">
                                <table class="table__data">
                                    <thead>
                                        <tr>
                                            <th class="table__header-cell">"Этап"</th>
                                            <th class="table__header-cell">"Заход"</th>
                                            <th class="table__header-cell">"Исход"</th>
                                            <th class="table__header-cell">"Выход / сообщение"</th>
                                            <th class="table__header-cell">"Лог"</th>
                                            <th class="table__header-cell">"Длит., мс"</th>
                                            <th class="table__header-cell">"Когда"</th>
                                        </tr>
                                    </thead>
                                    <tbody>
                                        {steps
                                            .into_iter()
                                            .map(|step| {
                                                let logs = step.logs.clone();
                                                let data = step.data.clone();
                                                view! {
                                                    <tr class="table__row">
                                                        <td class="table__cell">{step.stage_code}</td>
                                                        <td class="table__cell table__cell--right">{step.visit}</td>
                                                        <td class="table__cell">
                                                            <span class=verdict_badge(&step.verdict)>
                                                                {verdict_label(&step.verdict)}
                                                            </span>
                                                        </td>
                                                        <td class="table__cell">
                                                            {step
                                                                .outcome
                                                                .or(step.message)
                                                                .unwrap_or_else(|| "—".to_string())}
                                                            {data
                                                                .filter(|value| !value.is_null())
                                                                .map(|value| {
                                                                    view! {
                                                                        <Disclosure title="данные выхода">
                                                                            <JsonBlock value=value.clone() />
                                                                        </Disclosure>
                                                                    }
                                                                })}
                                                        </td>
                                                        <td class="table__cell">
                                                            {if logs.is_empty() {
                                                                view! { <span>"—"</span> }.into_any()
                                                            } else {
                                                                let count = logs.len();
                                                                let text = logs.join("\n");
                                                                view! {
                                                                    <Disclosure
                                                                        title="строк"
                                                                        hint=count.to_string()
                                                                    >
                                                                        <CodeBlock text=text.clone() />
                                                                    </Disclosure>
                                                                }
                                                                    .into_any()
                                                            }}
                                                        </td>
                                                        <td class="table__cell table__cell--right">
                                                            {step.duration_ms}
                                                        </td>
                                                        <td class="table__cell">{step.created_at}</td>
                                                    </tr>
                                                }
                                            })
                                            .collect_view()}
                                    </tbody>
                                </table>
                            </div>

                            <div class="sys-processes__block-title">"Эффекты"</div>
                            <EffectsTable records=effects />
                        }
                    })
            }}
        </div>
    }
}

/// Собственное состояние экземпляра — то, чего не видно ни в шагах, ни в
/// эффектах: куда он смотрит сейчас, сколько раз падал и чем именно.
#[component]
fn InstanceFacts(instance: ProcessInstance) -> impl IntoView {
    let cursor = instance
        .stage_code
        .clone()
        .map(|code| format!("{code} · заход {}", instance.visit))
        .unwrap_or_else(|| "— (экземпляр завершён)".to_string());
    let wait = instance.wait.clone();
    let input = instance.input.clone();
    let has_input = !input.is_null();
    let correlation = instance
        .correlation
        .fields()
        .map(|(field, value)| format!("{field} = {value}"))
        .collect::<Vec<_>>()
        .join(", ");

    view! {
        <div class="sys-processes__facts">
            <div class="sys-processes__fact">
                <div class="sys-processes__fact-key">"Определение"</div>
                <div class="sys-processes__fact-value">
                    {format!("{} v{}", instance.process_code, instance.process_version)}
                    <span class="sys-processes__fact-hint">
                        " · версия запинена на старте и до конца прогона не меняется"
                    </span>
                </div>
            </div>
            <div class="sys-processes__fact">
                <div class="sys-processes__fact-key">"Про что"</div>
                <div class="sys-processes__fact-value">
                    <span class="sys-processes__mono">{instance.correlation_token.clone()}</span>
                    {(!correlation.is_empty())
                        .then(|| {
                            view! {
                                <span class="sys-processes__fact-hint">
                                    {format!(" · {correlation}")}
                                </span>
                            }
                        })}
                </div>
            </div>
            <div class="sys-processes__fact">
                <div class="sys-processes__fact-key">"Курсор"</div>
                <div class="sys-processes__fact-value">{cursor}</div>
            </div>
            <div class="sys-processes__fact">
                <div class="sys-processes__fact-key">"Попытки"</div>
                <div class="sys-processes__fact-value">
                    {instance.attempts}
                    {instance
                        .next_attempt_at
                        .clone()
                        .map(|at| {
                            view! {
                                <span class="sys-processes__fact-hint">
                                    {format!(" · следующая не раньше {at}")}
                                </span>
                            }
                        })}
                </div>
            </div>
            {wait
                .map(|wait| {
                    view! {
                        <div class="sys-processes__fact">
                            <div class="sys-processes__fact-key">"Ожидание"</div>
                            <div class="sys-processes__fact-value">
                                <span class="sys-processes__mono">{wait.event.clone()}</span>
                                <span class="sys-processes__fact-hint">
                                    {format!(
                                        " · дедлайн {} · ключ {} · события до №{} не считаются",
                                        wait.deadline_at,
                                        wait.token,
                                        wait.since_seq,
                                    )}
                                </span>
                            </div>
                        </div>
                    }
                })}
            {instance
                .last_outcome
                .clone()
                .map(|outcome| {
                    view! {
                        <div class="sys-processes__fact">
                            <div class="sys-processes__fact-key">"Последний выход"</div>
                            <div class="sys-processes__fact-value">{outcome}</div>
                        </div>
                    }
                })}
            {instance
                .last_error
                .clone()
                .map(|error| {
                    view! {
                        <div class="sys-processes__fact">
                            <div class="sys-processes__fact-key">"Последняя ошибка"</div>
                            <div class="sys-processes__fact-value">
                                <span class="badge badge--error">"сбой"</span>
                                <span>{error}</span>
                            </div>
                        </div>
                    }
                })}
            <div class="sys-processes__fact">
                <div class="sys-processes__fact-key">"Время"</div>
                <div class="sys-processes__fact-value">
                    {format!(
                        "начат {} · обновлён {} · завершён {}",
                        instance.started_at,
                        instance.updated_at,
                        instance.finished_at.clone().unwrap_or_else(|| "—".to_string()),
                    )}
                </div>
            </div>
            <div class="sys-processes__fact">
                <div class="sys-processes__fact-key">"Аренда"</div>
                <div class="sys-processes__fact-value">
                    {instance
                        .claim_session_id
                        .clone()
                        .map(|session| {
                            format!("арендован сессией {session}")
                        })
                        .unwrap_or_else(|| "свободен".to_string())}
                </div>
            </div>
            {has_input
                .then(|| {
                    view! {
                        <div class="sys-processes__fact">
                            <div class="sys-processes__fact-key">"Вход Этапа"</div>
                            <div class="sys-processes__fact-value">
                                <Disclosure title="показать">
                                    <JsonBlock value=input.clone() />
                                </Disclosure>
                            </div>
                        </div>
                    }
                })}
        </div>
    }
}
