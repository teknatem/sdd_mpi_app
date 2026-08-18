//! Компактная строка пар «подпись — значение», разбитая на секции.
//!
//! Нужна там, где надо показать много коротких фактов о состоянии объекта
//! (версия, коммит, профиль сборки, время снимка) и при этом не отдать под них
//! экран. Плоский ряд из восьми пар нечитаем — глаз не находит границ между
//! смысловыми группами; секции с разделителем эти границы возвращают.
//!
//! Аналога в проекте не было: `gldim-kv` и `gl-td-row` вертикальные и
//! заточены под свои страницы, `card-meta` и `summary-item` мертвы.

use leptos::prelude::*;

#[component]
pub fn MetaStrip(children: Children) -> impl IntoView {
    view! { <div class="meta-strip">{children()}</div> }
}

#[component]
pub fn MetaSection(#[prop(into)] label: String, children: Children) -> impl IntoView {
    view! {
        <div class="meta-strip__section">
            <span class="meta-strip__section-label">{label}</span>
            <div class="meta-strip__items">{children()}</div>
        </div>
    }
}

#[component]
pub fn MetaItem(
    #[prop(into)] label: String,
    #[prop(into)] value: String,
    /// Моноширинное значение — для хешей и идентификаторов, где важна не
    /// читаемость слова, а сравнимость символ в символ.
    #[prop(optional)]
    mono: bool,
) -> impl IntoView {
    let value_class = if mono {
        "meta-strip__value meta-strip__value--mono"
    } else {
        "meta-strip__value"
    };

    view! {
        <div class="meta-strip__item">
            <span class="meta-strip__label">{label}</span>
            <span class=value_class>{value}</span>
        </div>
    }
}
