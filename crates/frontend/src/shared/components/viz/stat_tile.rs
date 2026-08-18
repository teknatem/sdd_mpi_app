//! Плитка показателя: подпись · значение · оценка · дельта · тренд.
//!
//! Контракт до сих пор переписывали независимо как минимум трижды
//! (`d407-tile`, `stat-card` в raw_storage, плитка метрик проекта). Здесь он
//! один, и вместе с ним чинятся две вещи, которые в копиях обычно теряются:
//! статус не остаётся одним лишь цветом (рядом значок и подпись), а крупное
//! число набирается пропорциональными цифрами — `tabular-nums` уместен в
//! колонках, а отдельно стоящее «121» делает разреженным.
//!
//! Слоты `meter` и `trend` намеренно `Children`, а не готовые данные: плитка не
//! должна знать ни про пороги, ни про ряды — она их только размещает.

use contracts::system::metrics::MetricStatus;
use leptos::prelude::*;

fn tile_modifier(status: MetricStatus) -> &'static str {
    match status {
        MetricStatus::Ok => "stat-tile--ok",
        MetricStatus::Warn => "stat-tile--warn",
        MetricStatus::Bad => "stat-tile--bad",
        MetricStatus::Neutral => "",
    }
}

/// Значок и подпись состояния. Существуют затем, чтобы оценка читалась без
/// различения цвета — при дальтонизме, в ч/б печати и в forced-colors.
///
/// Оформление берём у общего `.badge`, а не заводим своё: стандарт требует
/// именно этого, и своя версия отличалась бы от бейджей на соседних страницах.
fn status_mark(status: MetricStatus) -> Option<(&'static str, &'static str, &'static str)> {
    match status {
        MetricStatus::Ok => Some(("badge--success", "✓", "норма")),
        MetricStatus::Warn => Some(("badge--warning", "▲", "внимание")),
        MetricStatus::Bad => Some(("badge--error", "■", "порог")),
        MetricStatus::Neutral => None,
    }
}

#[component]
pub fn StatTile(
    #[prop(into)] label: String,
    /// Готовое к показу значение.
    #[prop(into)]
    value: String,
    #[prop(optional, into)] unit: String,
    #[prop(default = MetricStatus::Neutral)] status: MetricStatus,
    /// Пояснение под значением: откуда цифра.
    #[prop(optional, into)]
    hint: String,
    // Слоты объявлены через `default = None`, а не `optional`: у `optional`
    // макрос Leptos разворачивает `Option` сам и требует на месте вызова уже
    // развёрнутое значение, а вызывающему коду удобнее отдавать `Option`
    // (шкала есть не у каждой метрики, тренд — не на первом снимке).
    /// Слот дельты — обычно `<DeltaChip/>`.
    #[prop(default = None)]
    delta: Option<AnyView>,
    /// Слот шкалы — обычно `<Meter/>`, когда у метрики есть пороги.
    #[prop(default = None)]
    meter: Option<AnyView>,
    /// Слот тренда — обычно `<Sparkline/>`.
    #[prop(default = None)]
    trend: Option<AnyView>,
) -> impl IntoView {
    let class = format!("stat-tile {}", tile_modifier(status));

    let mark = status_mark(status).map(|(modifier, glyph, caption)| {
        view! {
            <span class=format!("badge {modifier} stat-tile__status")>
                <span aria-hidden="true">{glyph}</span>
                <span>{caption}</span>
            </span>
        }
    });

    let has_foot = mark.is_some() || !hint.is_empty();

    // Точка статуса — единственное цветное пятно на карточке. Красить всю
    // поверхность нельзя: когда за порогом два десятка плиток, цветными
    // становятся все, и выделение перестаёт выделять.
    let dot = (!matches!(status, MetricStatus::Neutral))
        .then(|| view! { <span class="stat-tile__dot" aria-hidden="true"></span> });

    view! {
        <div class=class>
            <div class="stat-tile__head">
                {dot}
                <span class="stat-tile__label">{label}</span>
            </div>
            <div class="stat-tile__value">
                <span class="stat-tile__number">{value}</span>
                {(!unit.is_empty()).then(|| view! { <span class="stat-tile__unit">{unit}</span> })}
            </div>
            {delta}
            {meter}
            {trend}
            {has_foot
                .then(|| {
                    view! {
                        <div class="stat-tile__foot">
                            {(!hint.is_empty())
                                .then(|| view! { <span class="stat-tile__hint">{hint}</span> })}
                            {mark}
                        </div>
                    }
                })}
        </div>
    }
}
