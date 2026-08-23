//! Вкладка «Каталог»: чем механизм вообще умеет пользоваться.
//!
//! Оба каталога **закрыты и живут в Rust**, а не в БД: Действие — это операция
//! ядра (ADR-0011 п.14), вид события — типизированный факт домена (п.5).
//! Поэтому вкладка ничего не редактирует; она отвечает на вопрос, который
//! иначе решается чтением исходников: что можно написать в `capabilities`
//! Этапа и что поставить триггером Процесса.

use leptos::prelude::*;
use leptos::task::spawn_local;

use super::super::api;
use super::parts::{Disclosure, JsonBlock};

#[component]
pub fn CatalogTab() -> impl IntoView {
    let actions: RwSignal<Vec<api::ActionInfo>> = RwSignal::new(Vec::new());
    let events: RwSignal<Vec<api::EventKindInfo>> = RwSignal::new(Vec::new());
    let error: RwSignal<Option<String>> = RwSignal::new(None);

    Effect::new(move |_| {
        spawn_local(async move {
            match api::list_actions().await {
                Ok(items) => actions.set(items),
                Err(message) => error.set(Some(message)),
            }
            match api::list_event_kinds().await {
                Ok(items) => events.set(items),
                Err(message) => error.set(Some(message)),
            }
        });
    });

    view! {
        <div class="sys-processes__section">
            {move || {
                error.get().map(|message| view! { <div class="alert alert--error">{message}</div> })
            }}

            <div class="sys-processes__block-title">"Действия"</div>
            <div class="sys-processes__note">
                "Всё, чем Этап может менять мир. Право выдаётся строкой "
                "capability вида «action:имя»; внутри mjs Действие видно как "
                "host.actions.метод. Обратимость решает, что человек увидит в плане "
                "перед допуском Процесса в работу."
            </div>
            <div class="table-wrapper">
                <table class="table__data">
                    <thead>
                        <tr>
                            <th class="table__header-cell">"Действие"</th>
                            <th class="table__header-cell">"Название"</th>
                            <th class="table__header-cell">"Обратимость"</th>
                            <th class="table__header-cell">"Право"</th>
                            <th class="table__header-cell">"В mjs"</th>
                            <th class="table__header-cell">"Пишет в таблицы"</th>
                        </tr>
                    </thead>
                    <tbody>
                        <For
                            each=move || actions.get()
                            key=|info| info.name.clone()
                            let:info
                        >
                            {
                                let schema = info.input_schema.clone();
                                let description = info.description.clone();
                                let tables = info.write_tables.join(", ");
                                view! {
                                    <tr class="table__row">
                                        <td class="table__cell sys-processes__mono">
                                            {info.name.clone()}
                                        </td>
                                        <td class="table__cell">
                                            {info.title.clone()}
                                            {(!description.is_empty())
                                                .then(|| {
                                                    view! {
                                                        <div class="sys-processes__cell-note">{description}</div>
                                                    }
                                                })}
                                            <Disclosure title="схема входа">
                                                <JsonBlock value=schema.clone() />
                                            </Disclosure>
                                        </td>
                                        <td class="table__cell">
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
                                        </td>
                                        <td class="table__cell sys-processes__mono">
                                            {info.capability.clone()}
                                        </td>
                                        <td class="table__cell sys-processes__mono">
                                            {format!("host.actions.{}", info.method)}
                                        </td>
                                        <td class="table__cell sys-processes__mono">
                                            {if tables.is_empty() {
                                                "—".to_string()
                                            } else {
                                                tables
                                            }}
                                        </td>
                                    </tr>
                                }
                            }
                        </For>
                    </tbody>
                </table>
            </div>

            <div class="sys-processes__block-title">"Виды событий"</div>
            <div class="sys-processes__note">
                "Каталог фактов домена. Ключ корреляции — свойство факта, а не подписки: "
                "объяви его подписчик, два Процесса разошлись бы в том, что считать «тем же "
                "самым днём»."
            </div>
            <div class="table-wrapper">
                <table class="table__data">
                    <thead>
                        <tr>
                            <th class="table__header-cell">"Событие"</th>
                            <th class="table__header-cell">"Что означает"</th>
                            <th class="table__header-cell">"Ключ корреляции"</th>
                        </tr>
                    </thead>
                    <tbody>
                        <For
                            each=move || events.get()
                            key=|kind| kind.name.clone()
                            let:kind
                        >
                            <tr class="table__row">
                                <td class="table__cell sys-processes__mono">{kind.name.clone()}</td>
                                <td class="table__cell">{kind.title.clone()}</td>
                                <td class="table__cell sys-processes__mono">
                                    {kind.correlation.join(", ")}
                                </td>
                            </tr>
                        </For>
                    </tbody>
                </table>
            </div>
        </div>
    }
}
