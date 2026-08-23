//! Вкладка «Журналы»: что механизм сделал с миром и от каких фактов проснулся.
//!
//! Записи `in_progress` тут не украшение: незавершённый эффект требует разбора
//! человеком, автоматического повтора для него нет (ADR-0011 п.10) — что
//! произошло с миром, неизвестно.

use contracts::processes::{DomainEvent, EffectRecord, EffectStatus};
use leptos::prelude::*;
use leptos::task::spawn_local;

use super::super::api;
use super::parts::{Disclosure, EffectsTable, JsonBlock};

/// Сколько записей тянем за раз: экран разбора, а не выгрузка.
const JOURNAL_LIMIT: u64 = 100;

#[component]
pub fn JournalsTab() -> impl IntoView {
    let effects: RwSignal<Vec<EffectRecord>> = RwSignal::new(Vec::new());
    let events: RwSignal<Vec<DomainEvent>> = RwSignal::new(Vec::new());
    let error: RwSignal<Option<String>> = RwSignal::new(None);
    let only_open = RwSignal::new(false);

    Effect::new(move |_| {
        spawn_local(async move {
            match api::list_effects(JOURNAL_LIMIT).await {
                Ok(items) => effects.set(items),
                Err(message) => error.set(Some(message)),
            }
            match api::list_events(JOURNAL_LIMIT).await {
                Ok(items) => events.set(items),
                Err(message) => error.set(Some(message)),
            }
        });
    });

    // `Signal::derive`, а не `Memo`: у записи журнала нет `PartialEq`, и мемо
    // не смогло бы сравнить прошлый результат с новым.
    let shown = Signal::derive(move || {
        let all = effects.get();
        if only_open.get() {
            all.into_iter()
                .filter(|record| record.status == EffectStatus::InProgress)
                .collect()
        } else {
            all
        }
    });

    view! {
        <div class="sys-processes__section">
            {move || {
                error.get().map(|message| view! { <div class="alert alert--error">{message}</div> })
            }}

            <div class="sys-processes__block-title">"Журнал эффектов"</div>
            <div class="sys-processes__note">
                "Незавершённая запись — не повод повторить, а повод разобрать: что произошло "
                "с миром, неизвестно."
            </div>
            <div>
                <button
                    class="button button--ghost"
                    on:click=move |_| only_open.update(|value| *value = !*value)
                >
                    {move || {
                        if only_open.get() { "Показать все" } else { "Только незавершённые" }
                    }}
                </button>
            </div>
            {move || view! { <EffectsTable records=shown.get() /> }}

            <div class="sys-processes__block-title">"Доменные события"</div>
            <div class="sys-processes__note">
                "Факты, от которых механизм стартует экземпляры и будит ожидающих. "
                "Ключ корреляции отвечает на вопрос «про что этот факт»."
            </div>
            <div class="table-wrapper">
                <table class="table__data">
                    <thead>
                        <tr>
                            <th class="table__header-cell">"№"</th>
                            <th class="table__header-cell">"Событие"</th>
                            <th class="table__header-cell">"Ключ"</th>
                            <th class="table__header-cell">"Источник"</th>
                            <th class="table__header-cell">"Данные"</th>
                            <th class="table__header-cell">"Когда"</th>
                        </tr>
                    </thead>
                    <tbody>
                        <For
                            each=move || events.get()
                            key=|event| event.id.clone()
                            let:event
                        >
                            {
                                let payload = event.payload.clone();
                                view! {
                                    <tr class="table__row">
                                        <td class="table__cell table__cell--right">{event.seq}</td>
                                        <td class="table__cell">{event.kind.as_str()}</td>
                                        <td class="table__cell sys-processes__mono">
                                            {event.correlation_token.clone()}
                                        </td>
                                        <td class="table__cell">{event.source.clone()}</td>
                                        <td class="table__cell">
                                            {if payload.is_null() {
                                                view! { <span>"—"</span> }.into_any()
                                            } else {
                                                view! {
                                                    <Disclosure title="показать">
                                                        <JsonBlock value=payload.clone() />
                                                    </Disclosure>
                                                }
                                                    .into_any()
                                            }}
                                        </td>
                                        <td class="table__cell">{event.published_at.clone()}</td>
                                    </tr>
                                }
                            }
                        </For>
                    </tbody>
                </table>
            </div>
        </div>
    }
}
