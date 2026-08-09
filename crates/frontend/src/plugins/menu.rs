//! Динамическая категория мега-меню «Плагины».
//!
//! Список строится из `GET /api/plugin` (включённые + активные плагины). Клик открывает
//! вкладку с ключом `plugin__<id>` — её рендерит `PluginHost` через `render_tab_content`.
//! Видна всем аутентифицированным пользователям (использование плагинов — auth-only).

use crate::layout::global_context::AppGlobalContext;
use crate::plugins::api;
use crate::shared::icons;
use crate::shared::icons::icon;
use contracts::plugins::PluginListItem;
use leptos::prelude::*;
use leptos::task::spawn_local;

#[component]
pub fn PluginsMenuCategory() -> impl IntoView {
    let (is_open, set_is_open) = signal(false);
    let (items, set_items) = signal(Vec::<PluginListItem>::new());

    let tabs_store = leptos::context::use_context::<AppGlobalContext>()
        .expect("AppGlobalContext context not found");

    // Загрузка списка плагинов при монтировании.
    spawn_local(async move {
        if let Ok(list) = api::list_enabled().await {
            set_items.set(list);
        }
    });

    view! {
        <div
            class="mega-menu-category"
            on:mouseenter=move |_| set_is_open.set(true)
            on:mouseleave=move |_| set_is_open.set(false)
        >
            <button
                class="mega-menu-btn"
                class:mega-menu-btn-active=move || is_open.get()
            >
                <span>"Плагины"</span>
                <span
                    class="mega-menu-chevron"
                    class:mega-menu-chevron-open=move || is_open.get()
                >
                    {icons::icon("chevron-down")}
                </span>
            </button>

            <div
                class="mega-menu-panel"
                class:mega-menu-panel-open=move || is_open.get()
            >
                <div class="mega-menu-content mega-menu-grid-1">
                    <button
                        class="mega-menu-card"
                        on:click=move |_| {
                            tabs_store.open_tab("plugins", "Плагины — реестр");
                            set_is_open.set(false);
                        }
                    >
                        <div class="mega-menu-card-icon">
                            {icons::icon("table")}
                        </div>
                        <div class="mega-menu-card-title">
                            "Реестр плагинов"
                        </div>
                    </button>
                    {move || {
                        let list = items.get();
                        if list.is_empty() {
                            view! {
                                <div class="mega-menu-empty">"Нет доступных плагинов"</div>
                            }.into_any()
                        } else {
                            list.into_iter().map(|p| {
                                let tabs_store = tabs_store.clone();
                                let key = format!("plugin__{}", p.id);
                                let title = p.title.clone();
                                view! {
                                    <button
                                        class="mega-menu-card"
                                        on:click=move |_| {
                                            tabs_store.open_tab(&key, &title);
                                            set_is_open.set(false);
                                        }
                                    >
                                        <div class="mega-menu-card-icon">
                                            {icons::icon("package")}
                                        </div>
                                        <div class="mega-menu-card-title">
                                            {p.title.clone()}
                                        </div>
                                    </button>
                                }
                            }).collect_view().into_any()
                        }
                    }}
                </div>
            </div>
        </div>
    }
}

/// Группа левого сайдбара «Плагины» (использование — всем, управление — админам).
/// В сайдбаре остаётся только ссылка на реестр; отдельные плагины открываются из реестра.
///
/// `expanded_group` — общий аккордеон-сигнал сайдбара: хранит id единственной раскрытой
/// группы, поэтому раскрытие «Плагинов» закрывает любую другую группу и наоборот.
#[component]
pub fn PluginsSidebarGroup(expanded_group: RwSignal<Option<String>>) -> impl IntoView {
    let ctx = use_context::<AppGlobalContext>().expect("AppGlobalContext not found");
    const GROUP_ID: &str = "plugins";
    let is_expanded = move || expanded_group.with(|g| g.as_deref() == Some(GROUP_ID));

    view! {
        <div>
            <div
                class="app-sidebar__item"
                style:padding-left="12px"
                on:click=move |_| {
                    expanded_group
                        .update(|current| {
                            if current.as_deref() == Some(GROUP_ID) {
                                *current = None;
                            } else {
                                *current = Some(GROUP_ID.to_string());
                            }
                        });
                }
            >
                <div class="app-sidebar__item-content">
                    {icon("box")}
                    <span>"Плагины"</span>
                </div>
                <div
                    class="app-sidebar__chevron"
                    class:app-sidebar__chevron--expanded=is_expanded
                >
                    {icon("chevron-right")}
                </div>
            </div>

            <div class="app-sidebar__collapse" class:app-sidebar__collapse--open=is_expanded>
                <div class="app-sidebar__collapse-inner">
                    <div class="app-sidebar__children">
                        <div
                            class="app-sidebar__item"
                            style:padding-left="10px"
                            on:click=move |_| ctx.open_tab("plugins", "Плагины — реестр")
                        >
                            <div class="app-sidebar__item-content">
                                {icon("table")}
                                <span>"Реестр плагинов"</span>
                            </div>
                        </div>
                    </div>
                </div>
            </div>
        </div>
    }
}
