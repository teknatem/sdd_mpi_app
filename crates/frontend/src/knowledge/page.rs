//! Каркас страницы «Инвентаризация знаний».
//!
//! Шесть вкладок отвечают на шесть разных вопросов, и порядок их — порядок,
//! в котором эти вопросы задают: сколько всего (Сводка) → что именно (Единицы) →
//! откуда оно берётся (Поверхности) → что написано руками (Статьи) → чем это
//! размечено (Словарь) → что чинить (Проблемы).
//!
//! Прежняя страница «База знаний» была первой и последней из этих шести
//! одновременно, и потому не отвечала толком ни на один.

use leptos::prelude::*;

use crate::shared::components::page_header::PageHeader;
use crate::shared::components::page_tabs::{PageTabs, TabItem, TabsVariant};
use crate::shared::icons::icon;
use crate::shared::page_frame::PageFrame;
use crate::shared::page_standard::PAGE_CAT_LIST;

use super::view_model::InventoryVm;

#[component]
pub fn KnowledgeInventoryPage() -> impl IntoView {
    let vm = InventoryVm::new();
    vm.load();

    let units_badge = Signal::derive(move || {
        vm.data
            .get()
            .map(|data| data.snapshot.unit_count.to_string())
            .unwrap_or_default()
    });
    let issues_badge = Signal::derive(move || {
        let count = vm
            .data
            .get()
            .map(|data| {
                data.units
                    .iter()
                    .filter(|unit| !unit.issues.is_empty())
                    .count()
            })
            .unwrap_or(0);
        if count == 0 {
            String::new()
        } else {
            count.to_string()
        }
    });

    let tabs = vec![
        TabItem::new("summary", "Сводка").with_icon("bar-chart-3"),
        TabItem::new("units", "Единицы")
            .with_icon("list")
            .with_badge(units_badge),
        TabItem::new("surfaces", "Поверхности").with_icon("layers"),
        TabItem::new("articles", "Статьи").with_icon("book-open-text"),
        TabItem::new("vocabulary", "Словарь").with_icon("tags"),
        TabItem::new("issues", "Проблемы")
            .with_icon("triangle-alert")
            .with_badge(issues_badge),
    ];

    view! {
        <PageFrame
            page_id="knowledge_base--list"
            category=PAGE_CAT_LIST
            class="page--wide knowledge-inventory"
        >
            <PageHeader
                title="Инвентаризация знаний"
                subtitle="Что в системе есть и что из этого достижимо чату"
            >
                <button
                    class="button button--secondary"
                    disabled=move || vm.collecting.get()
                    on:click=move |_| vm.collect_now()
                >
                    {icon("refresh-cw")}
                    {move || if vm.collecting.get() { " Пересобираю…" } else { " Пересобрать снимок" }}
                </button>
            </PageHeader>

            <PageTabs
                tabs=tabs
                active=vm.tab.into()
                on_select=Callback::new(move |key| vm.tab.set(key))
                variant=TabsVariant::Light
            />

            {move || vm.error.get().map(|message| view! {
                <div class="knowledge-inventory__banner knowledge-inventory__banner--error">
                    {message}
                </div>
            })}
            {move || vm.notice.get().map(|message| view! {
                <div class="knowledge-inventory__banner">{message}</div>
            })}

            <Show when=move || vm.loading.get() && vm.data.get().is_none()>
                <div class="knowledge-inventory__empty">"Собираю инвентаризацию…"</div>
            </Show>

            {move || match vm.tab.get() {
                "units" => view! { <super::tabs::units::UnitsTab vm=vm /> }.into_any(),
                "surfaces" => view! { <super::tabs::surfaces::SurfacesTab vm=vm /> }.into_any(),
                "articles" => view! { <super::tabs::articles::ArticlesTab /> }.into_any(),
                "vocabulary" => view! { <super::tabs::vocabulary::VocabularyTab /> }.into_any(),
                "issues" => view! { <super::tabs::issues::IssuesTab vm=vm /> }.into_any(),
                _ => view! { <super::tabs::summary::SummaryTab vm=vm /> }.into_any(),
            }}
        </PageFrame>
    }
}
