//! Карточка одного Этапа отдельной вкладкой (`sys_stage_details_<code>`).
//!
//! Та же [`StageCard`], что и на вкладке «Определения», но адресуемая ссылкой.
//! Нужна, чтобы на Этап можно было **сослаться** — из графа в плагине, из
//! журнала, из чата, — а не только доскроллить до него в общем списке.
//!
//! Каталоги грузятся те же три, что и в `definitions`: карточка Этапа читается
//! только вместе с Процессами («где используется») и Действиями («чем меняет
//! мир»). Дублирование загрузки осознанное — вкладка открывается точечно и
//! живёт своей жизнью, а не как часть списка.

use contracts::processes::{ProcessRecord, StageRecord};
use leptos::prelude::*;
use leptos::task::spawn_local;

use super::super::api;
use super::stage_card::StageCard;
use crate::shared::components::page_header::PageHeader;
use crate::shared::page_frame::PageFrame;
use crate::shared::page_standard::PAGE_CAT_SYSTEM;
use crate::system::auth::guard::RequireAdmin;

#[component]
pub fn StageDetailsPage(code: String) -> impl IntoView {
    // children у RequireAdmin — Fn, поэтому code отдаём копией на каждый вызов.
    let code = StoredValue::new(code);
    view! {
        <RequireAdmin>
            <StageDetailsInner code=code.get_value() />
        </RequireAdmin>
    }
}

#[component]
fn StageDetailsInner(code: String) -> impl IntoView {
    let processes: RwSignal<Vec<ProcessRecord>> = RwSignal::new(Vec::new());
    let stages: RwSignal<Vec<StageRecord>> = RwSignal::new(Vec::new());
    let actions: RwSignal<Vec<api::ActionInfo>> = RwSignal::new(Vec::new());
    let error: RwSignal<Option<String>> = RwSignal::new(None);
    let loaded = RwSignal::new(false);

    let load = move || {
        spawn_local(async move {
            match api::list_stages_full().await {
                Ok(items) => stages.set(items),
                Err(message) => error.set(Some(message)),
            }
            match api::list_processes_full().await {
                Ok(items) => processes.set(items),
                Err(message) => error.set(Some(message)),
            }
            match api::list_actions().await {
                Ok(items) => actions.set(items),
                Err(message) => error.set(Some(message)),
            }
            loaded.set(true);
        });
    };

    Effect::new(move |_| {
        load();
    });

    let on_changed = Callback::new(move |_| load());
    let wanted = StoredValue::new(code.clone());
    let title = code.clone();

    view! {
        <PageFrame
            page_id="sys_processes--system"
            category=PAGE_CAT_SYSTEM
            class="sys-processes"
        >
            <PageHeader
                title=format!("Этап {title}")
                subtitle="Паспорт Этапа: выходы, права, mjs и история версий"
            >
                <button class="button button--secondary" on:click=move |_| load()>
                    "Обновить"
                </button>
            </PageHeader>
            <div class="sys-processes__section">
                {move || {
                    error
                        .get()
                        .map(|message| view! { <div class="alert alert--error">{message}</div> })
                }}
                {move || {
                    let code = wanted.get_value();
                    let found = stages
                        .get()
                        .into_iter()
                        .find(|record| record.code == code);
                    match found {
                        Some(record) => {
                            view! {
                                <StageCard
                                    record=record
                                    delay_ms=0
                                    processes=processes
                                    actions=actions
                                    on_changed=on_changed
                                />
                            }
                                .into_any()
                        }
                        None if loaded.get() => {
                            view! {
                                <div class="sys-processes__empty">
                                    {format!("Этап «{code}» не найден в каталоге.")}
                                </div>
                            }
                                .into_any()
                        }
                        None => {
                            view! { <div class="sys-processes__empty">"Загрузка…"</div> }.into_any()
                        }
                    }
                }}
            </div>
        </PageFrame>
    }
}
