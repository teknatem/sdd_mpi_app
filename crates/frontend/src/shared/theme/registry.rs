//! Единый реестр тем приложения.
//!
//! Каждая тема описывается тремя атрибутами:
//! - `kind` — строгая (`strict`) или весёлая (`fancy`). Строгий режим по CSS-флагу
//!   отключает все рискованные эффекты (фоновая картинка, blur, noise, scrim) —
//!   см. `static/themes/core/strict-guard.css`.
//! - `base` — светлая или тёмная база: выбирает тему Thaw и ветку
//!   `[data-theme-base=...]`-селекторов в CSS.
//!
//! Добавление темы = одна строка в [`THEMES`] + файл `static/themes/<id>/<id>.css`.
//! Никаких других списков тем в коде быть не должно.

use thaw::Theme;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ThemeKind {
    Strict,
    Fancy,
}

impl ThemeKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            ThemeKind::Strict => "strict",
            ThemeKind::Fancy => "fancy",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ThemeBase {
    Light,
    Dark,
}

impl ThemeBase {
    pub fn as_str(&self) -> &'static str {
        match self {
            ThemeBase::Light => "light",
            ThemeBase::Dark => "dark",
        }
    }

    pub fn thaw_theme(&self) -> Theme {
        match self {
            ThemeBase::Light => Theme::light(),
            ThemeBase::Dark => Theme::dark(),
        }
    }

    /// Ранний фон iframe до загрузки `<link>` темы (анти-вспышка).
    pub fn iframe_bg_fallback(&self) -> &'static str {
        match self {
            ThemeBase::Light => "#f9fafb",
            ThemeBase::Dark => "#292929",
        }
    }
}

pub struct ThemeDef {
    pub id: &'static str,
    pub label: &'static str,
    pub kind: ThemeKind,
    pub base: ThemeBase,
}

pub const THEMES: &[ThemeDef] = &[
    ThemeDef {
        id: "dark",
        label: "Темная",
        kind: ThemeKind::Strict,
        base: ThemeBase::Dark,
    },
    ThemeDef {
        id: "light",
        label: "Светлая",
        kind: ThemeKind::Strict,
        base: ThemeBase::Light,
    },
    ThemeDef {
        id: "forest",
        label: "Лесная",
        kind: ThemeKind::Fancy,
        base: ThemeBase::Dark,
    },
];

pub const DEFAULT_THEME_ID: &str = "dark";

/// Тема по id; неизвестные/пустые id падают на тему по умолчанию (dark).
pub fn theme_by_id(id: &str) -> &'static ThemeDef {
    THEMES
        .iter()
        .find(|t| t.id == id)
        .or_else(|| THEMES.iter().find(|t| t.id == DEFAULT_THEME_ID))
        .expect("DEFAULT_THEME_ID must exist in THEMES")
}

/// JSON-карта тем для инжекта в plugin-iframe:
/// `{"dark":{"kind":"strict","base":"dark"},...}`.
pub fn themes_json() -> String {
    let entries: Vec<String> = THEMES
        .iter()
        .map(|t| {
            format!(
                r#""{}":{{"kind":"{}","base":"{}"}}"#,
                t.id,
                t.kind.as_str(),
                t.base.as_str()
            )
        })
        .collect();
    format!("{{{}}}", entries.join(","))
}
