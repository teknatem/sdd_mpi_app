//! Страница «Процессы» (`sys_processes`), admin-only.
//!
//! Четыре вкладки — четыре разных вопроса, и разделены они именно так, потому
//! что отвечают разным людям в разное время:
//!
//! - **Экземпляры** — «что сейчас идёт и кого ждут». Здесь же инбокс: список
//!   экземпляров в ожидании и кнопка «сделано». Кнопка не двигает экземпляр —
//!   она публикует факт, а двинет его воркер (ADR-0011 п.9).
//! - **Определения** — «что заведено и что внутри». Определения живут в БД, а
//!   не в git (п.6), поэтому карточка Процесса с графом, паспорт Этапа с
//!   выходами и правами, сам mjs и история версий — единственное место, где
//!   всё это можно прочитать.
//! - **Каталог** — «чем механизм вообще умеет пользоваться»: закрытые списки
//!   Действий и видов событий из Rust.
//! - **Журналы** — «что механизм сделал с миром и от каких фактов проснулся».
//!   Записи `in_progress` тут не украшение: незавершённый эффект требует
//!   разбора человеком, автоматического повтора для него нет (п.10).
//!
//! Активация — единственное действие страницы с последствиями, поэтому она
//! идёт через план: сначала показывается, что изменится, и только потом
//! появляется кнопка.

pub mod catalog;
pub mod definitions;
pub mod instances;
pub mod journals;
pub mod parts;
pub mod process_card;
pub mod stage_card;

use contracts::processes::{InstanceStatus, ProcessInstance};
use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::shared::components::page_header::PageHeader;
use crate::shared::components::page_tabs::{PageTabs, TabItem, TabsVariant};
use crate::shared::page_frame::PageFrame;
use crate::shared::page_standard::PAGE_CAT_SYSTEM;
use crate::system::auth::guard::RequireAdmin;

use super::api;
use catalog::CatalogTab;
use definitions::DefinitionsTab;
use instances::InstancesTab;
use journals::JournalsTab;

#[component]
pub fn ProcessesPage() -> impl IntoView {
    view! {
        <RequireAdmin>
            <ProcessesPageInner />
        </RequireAdmin>
    }
}

#[component]
fn ProcessesPageInner() -> impl IntoView {
    let active_tab: RwSignal<&'static str> = RwSignal::new("instances");
    let instances: RwSignal<Vec<ProcessInstance>> = RwSignal::new(Vec::new());
    let error: RwSignal<Option<String>> = RwSignal::new(None);
    let busy = RwSignal::new(false);

    let waiting_count = Memo::new(move |_| {
        instances
            .get()
            .iter()
            .filter(|instance| instance.status == InstanceStatus::Waiting)
            .count()
    });

    let load_instances = move || {
        spawn_local(async move {
            match api::list_instances(None).await {
                Ok(items) => instances.set(items),
                Err(message) => error.set(Some(message)),
            }
        });
    };

    // Ручной проход: механизм двигает воркер раз в полминуты, но человеку,
    // который только что нажал «сделано», ждать незачем.
    let tick = move || {
        busy.set(true);
        spawn_local(async move {
            match api::tick().await {
                Ok(_) => error.set(None),
                Err(message) => error.set(Some(message)),
            }
            busy.set(false);
            match api::list_instances(None).await {
                Ok(items) => instances.set(items),
                Err(message) => error.set(Some(message)),
            }
        });
    };

    Effect::new(move |_| {
        load_instances();
    });

    let tabs = vec![
        TabItem::new("instances", "Экземпляры")
            .with_icon("activity")
            .with_badge(Signal::derive(move || {
                let waiting = waiting_count.get();
                if waiting > 0 {
                    waiting.to_string()
                } else {
                    String::new()
                }
            })),
        TabItem::new("definitions", "Определения").with_icon("layers"),
        TabItem::new("catalog", "Каталог").with_icon("book-open"),
        TabItem::new("journals", "Журналы").with_icon("list"),
    ];

    view! {
        <PageFrame
            page_id="sys_processes--system"
            category=PAGE_CAT_SYSTEM
            class="sys-processes"
        >
            <PageHeader
                title="Процессы"
                subtitle="Граф Этапов, экземпляры и журналы механизма (ADR-0011)"
            >
                <button
                    class="button button--secondary"
                    on:click=move |_| load_instances()
                >
                    "Обновить"
                </button>
                <button
                    class="button button--ghost"
                    on:click=move |_| tick()
                    disabled=move || busy.get()
                >
                    {move || if busy.get() { "Проход…" } else { "Двинуть сейчас" }}
                </button>
            </PageHeader>

            <PageTabs
                tabs=tabs
                active=active_tab.into()
                on_select=Callback::new(move |key: &'static str| active_tab.set(key))
                variant=TabsVariant::Light
            />

            <div class="page__content">
                <div class="sys-processes__body">
                    {move || {
                        error
                            .get()
                            .map(|message| view! { <div class="alert alert--error">{message}</div> })
                    }}

                    {move || match active_tab.get() {
                        "definitions" => view! { <DefinitionsTab /> }.into_any(),
                        "catalog" => view! { <CatalogTab /> }.into_any(),
                        "journals" => view! { <JournalsTab /> }.into_any(),
                        _ => view! { <InstancesTab instances=instances on_changed=Callback::new(move |_| tick()) /> }
                            .into_any(),
                    }}
                </div>
            </div>
        </PageFrame>
    }
}
