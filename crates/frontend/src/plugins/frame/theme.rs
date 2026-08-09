use crate::shared::theme::registry::{theme_by_id, ThemeDef};

/// Активная тема приложения (из реестра `shared::theme::registry`).
///
/// Значения токенов внутрь iframe больше не копируются вручную — тема подключается
/// через `<link>` на те же файлы, что и у приложения (см. [`super::srcdoc`]). Здесь нужно
/// лишь сообщить фрейму, какую тему загрузить; неизвестные имена из localStorage
/// схлопываются в тему по умолчанию через `theme_by_id`.
pub(super) fn current_theme() -> &'static ThemeDef {
    let name = read_raw_theme_name();
    theme_by_id(&name)
}

fn read_raw_theme_name() -> String {
    if let Some(window) = web_sys::window() {
        if let Ok(Some(storage)) = window.local_storage() {
            if let Ok(Some(theme)) = storage.get_item("app_theme") {
                if !theme.trim().is_empty() {
                    return theme;
                }
            }
        }
        if let Some(body) = window.document().and_then(|d| d.body()) {
            if let Some(theme) = body.get_attribute("data-theme") {
                if !theme.trim().is_empty() {
                    return theme;
                }
            }
        }
    }
    String::new()
}
