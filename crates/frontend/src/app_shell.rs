//! Application Shell - корневые компоненты приложения
//!
//! Содержит:
//! - `AppShell` - auth gate (показывает LoginPage или MainLayout)
//! - `MainLayout` - основной layout приложения (Shell + Sidebar + Tabs + RightPanel)

use crate::layout::global_context::{AppGlobalContext, Tab as TabData};
use crate::layout::left::sidebar::Sidebar;
use crate::layout::right::panel::RightPanel;
use crate::layout::tabs::TabPage;
use crate::layout::Shell;
use crate::system::auth::context::use_auth;
use crate::system::maintenance::{use_maintenance, MaintenancePage};
use crate::system::pages::login::LoginPage;
use leptos::logging::log;
use leptos::prelude::*;

/// Main application layout с Sidebar, Tabs и RightPanel.
///
/// Инициализирует router integration для синхронизации табов с URL (?active=...).
#[component]
fn MainLayout() -> impl IntoView {
    let tabs_store = leptos::context::use_context::<AppGlobalContext>()
        .expect("AppGlobalContext context not found");

    // Initialize router integration. This runs once when the component is created.
    tabs_store.init_router_integration();

    view! {
        <Shell
            left=|| view! { <Sidebar /> }.into_any()
            center=move || {
                view! {
                    <For
                        each=move || {
                            let tabs = tabs_store.opened.get();
                            log!("📋 <For> each triggered. Tabs count: {}", tabs.len());
                            for (i, tab) in tabs.iter().enumerate() {
                                log!("  {}. key='{}', title='{}'", i+1, tab.key, tab.title);
                            }
                            tabs
                        }
                        key=|tab| {
                            let key = tab.key.clone();
                            log!("🔑 <For> key function called for: '{}'", key);
                            key
                        }
                        children=move |tab: TabData| {
                            log!("👶 <For> children function called for: '{}'", tab.key);
                            view! {
                                <TabPage tab=tab tabs_store=tabs_store />
                            }
                        }
                    />
                }.into_any()
            }
            right=|| view! { <RightPanel /> }.into_any()
        />
    }
}

/// Application shell - auth gate component.
///
/// Показывает:
/// - `LoginPage` если пользователь не авторизован (при обслуживании — с строкой
///   о работах: она видна и без входа)
/// - `MaintenancePage` если идут технические работы и вошедший не администратор
/// - `MainLayout` если авторизован
#[component]
pub fn AppShell() -> impl IntoView {
    let (auth_state, _) = use_auth();
    let maintenance = use_maintenance();

    // Администратор проходит внутрь: он и снимает режим. Остальные — включая
    // тех, кто уже сидел на открытой вкладке, — видят заглушку.
    let blocked = move || {
        maintenance.status.get().active
            && !auth_state
                .get()
                .user_info
                .map(|user| user.is_admin)
                .unwrap_or(false)
    };
    let status = Signal::derive(move || maintenance.status.get());

    // Проверка входа снаружи, режим обслуживания внутри: не вошедшему нужен
    // экран входа в любом случае — администратору, чтобы снять режим, всем
    // остальным, чтобы прочитать на нём же, что происходит.
    view! {
        <Show
            when=move || auth_state.get().access_token.is_some()
            fallback=|| view! { <LoginPage /> }
        >
            <Show when=blocked fallback=|| view! { <MainLayout /> }>
                <MaintenancePage status=status />
            </Show>
        </Show>
    }
}
