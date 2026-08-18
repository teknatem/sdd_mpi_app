//! Поле поиска по подстроке — отдельный блок, а не элемент списка.
//!
//! Контрол «иконка + инпут + крестик» в проекте уже был, но жил как
//! `spec-list__search`, то есть элемент блока `spec-list`, и все четыре
//! потребителя обязаны были обернуть его в `<div class="spec-list">`. Страницам,
//! у которых контент не список спецификаций, это навязывало чужую разметку;
//! здесь тот же контрол со своим именем.
//!
//! Существующие `spec-list__search` намеренно оставлены как есть: их миграция —
//! отдельная работа, а не побочный эффект появления этого компонента.

use leptos::prelude::*;

use crate::shared::icons::icon;

#[component]
pub fn SearchBox(
    /// Строка запроса. Сигнал снаружи: фильтрация — дело вызывающей страницы,
    /// компонент только редактирует значение.
    query: RwSignal<String>,
    #[prop(optional, into)] placeholder: String,
) -> impl IntoView {
    let placeholder = if placeholder.is_empty() {
        "Поиск по видимым текстам".to_string()
    } else {
        placeholder
    };

    view! {
        <div class="search-box">
            <span class="search-box__icon">{icon("search")}</span>
            <input
                class="form__input"
                type="text"
                placeholder=placeholder
                prop:value=move || query.get()
                on:input=move |ev| query.set(event_target_value(&ev))
            />
            <Show when=move || !query.get().is_empty()>
                <button
                    class="search-box__clear"
                    title="Очистить"
                    on:click=move |_| query.set(String::new())
                >
                    {icon("x")}
                </button>
            </Show>
        </div>
    }
}
