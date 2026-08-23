//! Вкладка «Определения»: что заведено и что внутри.
//!
//! Определения живут в БД, а не в git (ADR-0011 п.6), поэтому здесь же то, что
//! в обычном коде заменяет чтение исходника: граф Процесса рёбрами, паспорт
//! Этапа с выходами и правами, сам mjs и история версий.
//!
//! Карточка, а не строка таблицы, — потому что вопросов к определению много и
//! они разной природы. «Куда идёт выход „расхождение“», «чем этот Этап меняет
//! мир», «что подавать на вход» — три разных ответа, и в шесть колонок они не
//! складываются.
//!
//! Сама вкладка — только раскладка и загрузка каталогов: карточки живут в
//! `process_card` и `stage_card`, потому что читаются они по отдельности и
//! отвечают на разные вопросы.

use contracts::processes::{DefinitionVersion, ProcessRecord, StageRecord};
use leptos::prelude::*;
use leptos::task::spawn_local;

use super::super::api;
use super::parts::{definition_status_badge, definition_status_label, short_digest};
use super::process_card::ProcessCard;
use super::stage_card::StageCard;

/// Шаг шахматной задержки появления карточек.
const STAGGER_MS: u32 = 40;

#[component]
pub fn DefinitionsTab() -> impl IntoView {
    let processes: RwSignal<Vec<ProcessRecord>> = RwSignal::new(Vec::new());
    let stages: RwSignal<Vec<StageRecord>> = RwSignal::new(Vec::new());
    let actions: RwSignal<Vec<api::ActionInfo>> = RwSignal::new(Vec::new());
    let events: RwSignal<Vec<api::EventKindInfo>> = RwSignal::new(Vec::new());
    let error: RwSignal<Option<String>> = RwSignal::new(None);

    // Четыре каталога грузятся вместе: карточка Процесса читается только с
    // Этапами (титулы в графе), карточка Этапа — только с Процессами («где
    // используется») и Действиями («чем меняет мир»).
    let load = move || {
        spawn_local(async move {
            match api::list_processes_full().await {
                Ok(items) => processes.set(items),
                Err(message) => error.set(Some(message)),
            }
            match api::list_stages_full().await {
                Ok(items) => stages.set(items),
                Err(message) => error.set(Some(message)),
            }
            match api::list_actions().await {
                Ok(items) => actions.set(items),
                Err(message) => error.set(Some(message)),
            }
            match api::list_event_kinds().await {
                Ok(items) => events.set(items),
                Err(message) => error.set(Some(message)),
            }
        });
    };

    Effect::new(move |_| {
        load();
    });

    let on_changed = Callback::new(move |_| load());

    view! {
        <div class="sys-processes__section">
            {move || {
                error.get().map(|message| view! { <div class="alert alert--error">{message}</div> })
            }}

            <div class="sys-processes__block-title">"Процессы"</div>
            <div class="sys-processes__note">
                "Граф Этапов с триггером. Экземпляр стартует на активной версии и доживает "
                "на ней: правка определения работающие прогоны не меняет."
            </div>
            {move || {
                let items = processes.get();
                if items.is_empty() {
                    view! { <div class="sys-processes__empty">"Процессов не заведено."</div> }
                        .into_any()
                } else {
                    items
                        .into_iter()
                        .enumerate()
                        .map(|(index, record)| {
                            view! {
                                <ProcessCard
                                    record=record
                                    delay_ms=index as u32 * STAGGER_MS
                                    stages=stages
                                    actions=actions
                                    events=events
                                    on_changed=on_changed
                                />
                            }
                        })
                        .collect_view()
                        .into_any()
                }
            }}

            <div class="sys-processes__block-title">"Этапы"</div>
            <div class="sys-processes__note">
                "Каталог общий: Этап адресуется кодом, а конкретную версию пинит активация "
                "Процесса. Поэтому один Этап может стоять в нескольких графах."
            </div>
            {move || {
                let items = stages.get();
                if items.is_empty() {
                    view! { <div class="sys-processes__empty">"Этапов не заведено."</div> }
                        .into_any()
                } else {
                    items
                        .into_iter()
                        .enumerate()
                        .map(|(index, record)| {
                            view! {
                                <StageCard
                                    record=record
                                    delay_ms=index as u32 * STAGGER_MS
                                    processes=processes
                                    actions=actions
                                    on_changed=on_changed
                                />
                            }
                        })
                        .collect_view()
                        .into_any()
                }
            }}
        </div>
    }
}

// ═══════════════════════════════════════════════════════════════════════
// История версий
// ═══════════════════════════════════════════════════════════════════════

/// История версий определения. Запрос уходит при первом раскрытии: до него
/// компонента нет вовсе, а карточек на странице сколько угодно.
#[component]
pub fn VersionHistory(
    code: String,
    /// `true` — Этап, `false` — Процесс. Различие только в маршруте.
    stage: bool,
) -> impl IntoView {
    let versions: RwSignal<Vec<DefinitionVersion>> = RwSignal::new(Vec::new());
    let error: RwSignal<Option<String>> = RwSignal::new(None);

    Effect::new(move |_| {
        let code = code.clone();
        spawn_local(async move {
            let loaded = if stage {
                api::list_stage_versions(&code).await
            } else {
                api::list_process_versions(&code).await
            };
            match loaded {
                Ok(items) => versions.set(items),
                Err(message) => error.set(Some(message)),
            }
        });
    });

    view! {
        {move || {
            error.get().map(|message| view! { <div class="alert alert--error">{message}</div> })
        }}
        <div class="table-wrapper">
            <table class="table__data">
                <thead>
                    <tr>
                        <th class="table__header-cell">"Версия"</th>
                        <th class="table__header-cell">"Состояние"</th>
                        <th class="table__header-cell">"Название"</th>
                        <th class="table__header-cell">"Отпечаток"</th>
                        <th class="table__header-cell">"Заведена"</th>
                        <th class="table__header-cell">"Автор"</th>
                    </tr>
                </thead>
                <tbody>
                    <For
                        each=move || versions.get()
                        key=|item| format!("{}:{}:{:?}", item.code, item.version, item.status)
                        let:item
                    >
                        <tr class="table__row">
                            <td class="table__cell table__cell--right">{item.version}</td>
                            <td class="table__cell">
                                <span class=definition_status_badge(item.status)>
                                    {definition_status_label(item.status)}
                                </span>
                            </td>
                            <td class="table__cell">{item.title.clone()}</td>
                            <td class="table__cell sys-processes__mono">
                                {short_digest(&item.digest)}
                            </td>
                            <td class="table__cell">{item.created_at.clone()}</td>
                            <td class="table__cell">
                                {item.created_by.clone().unwrap_or_else(|| "—".to_string())}
                            </td>
                        </tr>
                    </For>
                </tbody>
            </table>
        </div>
    }
}
