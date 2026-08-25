use crate::app::ThawThemeContext;
use crate::shared::theme::registry::{
    theme_by_id, ThemeContext, ThemeDef, DEFAULT_THEME_ID, THEMES,
};
use leptos::prelude::*;
use wasm_bindgen::JsCast;

const THEME_STORAGE_KEY: &str = "app_theme";
const THEME_KIND_STORAGE_KEY: &str = "app_theme_kind";
const THEME_BASE_STORAGE_KEY: &str = "app_theme_base";

/// Get saved theme from localStorage
fn get_saved_theme() -> String {
    if let Some(window) = web_sys::window() {
        if let Ok(Some(storage)) = window.local_storage() {
            if let Ok(Some(theme)) = storage.get_item(THEME_STORAGE_KEY) {
                return theme;
            }
        }
    }
    DEFAULT_THEME_ID.to_string()
}

/// Save theme to localStorage. kind/base сохраняются рядом — их читает
/// anti-FOUC скрипт в index.html до старта WASM.
fn save_theme(def: &ThemeDef) {
    if let Some(window) = web_sys::window() {
        if let Ok(Some(storage)) = window.local_storage() {
            let _ = storage.set_item(THEME_STORAGE_KEY, def.id);
            let _ = storage.set_item(THEME_KIND_STORAGE_KEY, def.kind.as_str());
            let _ = storage.set_item(THEME_BASE_STORAGE_KEY, def.base.as_str());
        }
    }
}

/// Apply theme to the document.
/// Атрибуты ставятся на `<html>` И `<body>`: фон задан на html (base.css),
/// а CSS-гейт строгого режима (strict-guard.css) матчит оба элемента.
fn apply_theme(def: &ThemeDef) {
    if let Some(window) = web_sys::window() {
        if let Some(document) = window.document() {
            let mut targets: Vec<web_sys::Element> = Vec::with_capacity(2);
            if let Some(root) = document.document_element() {
                targets.push(root);
            }
            if let Some(body) = document.body() {
                targets.push(body.into());
            }
            for el in targets {
                let _ = el.set_attribute("data-theme", def.id);
                let _ = el.set_attribute("data-theme-kind", def.kind.as_str());
                let _ = el.set_attribute("data-theme-base", def.base.as_str());
            }

            // Update theme stylesheet link
            if let Some(link) = document.get_element_by_id("theme-stylesheet") {
                if let Ok(link_element) = link.dyn_into::<web_sys::HtmlLinkElement>() {
                    link_element.set_disabled(false);
                    let _ =
                        link_element.set_href(&format!("static/themes/{}/{}.css", def.id, def.id));
                }
            }
        }
    }
}

/// Сохранённая тема — то, что уже применил anti-FOUC скрипт в index.html.
/// Нужна `app.rs`, чтобы выдать [`ThemeContext`] сразу с верным значением:
/// иначе первый кадр отрисовался бы по теме по умолчанию.
pub fn saved_theme_def() -> &'static ThemeDef {
    theme_by_id(&get_saved_theme())
}

/// ThemeSelect component for switching themes
#[component]
pub fn ThemeSelect() -> impl IntoView {
    // Load saved theme on mount
    let saved_theme = get_saved_theme();
    let current_theme = RwSignal::new(saved_theme.clone());
    let is_open = RwSignal::new(false);

    // Get Thaw theme context
    let thaw_theme_ctx = leptos::context::use_context::<ThawThemeContext>();
    let theme_ctx = leptos::context::use_context::<ThemeContext>();

    // Apply saved theme on mount (including Thaw theme)
    Effect::new(move |_| {
        let def = theme_by_id(&saved_theme);
        apply_theme(def);
        // Записать kind/base даже при первом визите — anti-FOUC скрипту на будущее.
        save_theme(def);
        if let Some(ctx) = thaw_theme_ctx {
            ctx.0.set(def.base.thaw_theme());
        }
        if let Some(ctx) = theme_ctx {
            ctx.0.set(def);
        }
    });

    let change_theme = move |theme_id: &'static str| {
        let def = theme_by_id(theme_id);
        apply_theme(def);
        save_theme(def);

        if let Some(ctx) = thaw_theme_ctx {
            ctx.0.set(def.base.thaw_theme());
        }
        if let Some(ctx) = theme_ctx {
            ctx.0.set(def);
        }

        current_theme.set(def.id.to_string());
        is_open.set(false);
    };

    let toggle_dropdown = move |_| {
        is_open.update(|v| *v = !*v);
    };

    view! {
        <div class="theme-select-wrapper">
            <button
                class="app-header__icon-button"
                on:click=toggle_dropdown
                title="Выбор темы"
            >
                <svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                    <circle cx="13.5" cy="6.5" r=".5" fill="currentColor"/>
                    <circle cx="17.5" cy="10.5" r=".5" fill="currentColor"/>
                    <circle cx="8.5" cy="7.5" r=".5" fill="currentColor"/>
                    <circle cx="6.5" cy="12.5" r=".5" fill="currentColor"/>
                    <path d="M12 2C6.5 2 2 6.5 2 12s4.5 10 10 10c.926 0 1.648-.746 1.648-1.688 0-.437-.18-.835-.437-1.125-.29-.289-.438-.652-.438-1.125a1.64 1.64 0 0 1 1.668-1.668h1.996c3.051 0 5.555-2.503 5.555-5.554C21.965 6.012 17.461 2 12 2z"/>
                </svg>
            </button>

            <Show when=move || is_open.get()>
                <div class="theme-dropdown">
                    <For
                        each=move || THEMES.iter()
                        key=|def| def.id
                        children=move |def: &'static ThemeDef| {
                            let is_active = move || current_theme.get() == def.id;

                            view! {
                                <button
                                    class=move || {
                                        if is_active() {
                                            "theme-dropdown__item theme-dropdown__item--active"
                                        } else {
                                            "theme-dropdown__item"
                                        }
                                    }
                                    on:click=move |_| change_theme(def.id)
                                >
                                    {def.label}
                                </button>
                            }
                        }
                    />
                </div>
            </Show>
        </div>
    }
}
