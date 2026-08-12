use crate::app_shell::AppShell;
use crate::layout::global_context::AppGlobalContext;
use crate::shared::change_tokens::ChangeTokenContext;
use crate::shared::modal_stack::{KeydownGuard, ModalHost, ModalStackService};
use crate::system::auth::context::AuthProvider;
use crate::system::maintenance::MaintenanceContext;
use crate::system::tasks::api as tasks_api;
use gloo_timers::future::TimeoutFuture;
use js_sys::Function;
use leptos::prelude::*;
use leptos::task::spawn_local;
use thaw::{ConfigProvider, Theme};
use wasm_bindgen::closure::Closure;
use wasm_bindgen::JsCast;
use web_sys::KeyboardEvent;

/// Context for Thaw UI theme management
#[derive(Clone, Copy)]
pub struct ThawThemeContext(pub RwSignal<Theme>);

#[component]
pub fn App() -> impl IntoView {
    // Provide the AppGlobalContext store to the whole app via context.
    provide_context(AppGlobalContext::new());
    // Provide centralized modal stack for CRUD-details modals
    provide_context(ModalStackService::new());
    // Режим обслуживания опрашивается глобально: он может включиться в любой
    // момент — в том числе автоматически, операцией переноса базы, запущенной
    // другим администратором.
    let maintenance = MaintenanceContext::new();
    provide_context(maintenance);
    maintenance.start_polling();
    // Provide change token context and start the global polling loop
    let ct = ChangeTokenContext::new();
    provide_context(ct);
    spawn_local(async move {
        loop {
            TimeoutFuture::new(4000).await;
            if let Ok(tokens) = tasks_api::fetch_change_tokens().await {
                if tokens.sys_tasks != ct.sys_tasks.get_untracked() {
                    ct.sys_tasks.set(tokens.sys_tasks);
                }
                if tokens.a027_wb_documents != ct.a027_wb_documents.get_untracked() {
                    ct.a027_wb_documents.set(tokens.a027_wb_documents);
                }
                if tokens.a015_wb_orders != ct.a015_wb_orders.get_untracked() {
                    ct.a015_wb_orders.set(tokens.a015_wb_orders);
                }
                if tokens.a012_wb_sales != ct.a012_wb_sales.get_untracked() {
                    ct.a012_wb_sales.set(tokens.a012_wb_sales);
                }
                if tokens.a013_ym_order != ct.a013_ym_order.get_untracked() {
                    ct.a013_ym_order.set(tokens.a013_ym_order);
                }
                if tokens.plugins != ct.plugins.get_untracked() {
                    ct.plugins.set(tokens.plugins);
                }
            }
        }
    });

    let tabs_store = use_context::<AppGlobalContext>().unwrap();
    let modal_svc = use_context::<ModalStackService>().unwrap();

    // Global ESC handler: close the active tab when Escape is pressed,
    // unless a modal is open (modal handles its own ESC) or the tab is dirty.
    let _esc_guard = StoredValue::new_local(web_sys::window().map(|window| {
        let closure = Closure::wrap(Box::new(move |event: web_sys::Event| {
            if let Some(ke) = event.dyn_ref::<KeyboardEvent>() {
                if ke.key() == "Escape" && !modal_svc.is_open() {
                    if let Some(active_key) = tabs_store.active.get_untracked() {
                        let is_dirty = tabs_store.opened.with_untracked(|tabs| {
                            tabs.iter()
                                .find(|t| t.key == active_key)
                                .map(|t| t.dirty)
                                .unwrap_or(false)
                        });
                        if !is_dirty {
                            tabs_store.close_tab(&active_key);
                        }
                    }
                }
            }
        }) as Box<dyn FnMut(_)>);

        let js_fn = closure.as_ref().unchecked_ref::<Function>().clone();
        let _ = window.add_event_listener_with_callback("keydown", &js_fn);

        KeydownGuard {
            window,
            js_fn,
            _closure: closure,
        }
    }));

    // Thaw UI theme - start with dark theme
    let theme = RwSignal::new(Theme::dark());

    // Provide Thaw theme context for ThemeSelect to update
    provide_context(ThawThemeContext(theme));

    view! {
        <ConfigProvider theme>
            <AuthProvider>
                <AppShell />
                <ModalHost />
            </AuthProvider>
        </ConfigProvider>
    }
}
