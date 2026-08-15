//! Дельта: насколько изменилось значение и хорошая ли это новость.
//!
//! Компонент мал, но именно его переписывают неправильно: цвет берут от знака
//! числа. Знак и оценка — разные вещи. Плюс у числа тестов — хорошо, плюс у
//! числа мёртвых классов — плохо, и решает это направление метрики, которое
//! приходит из данных, а не из вёрстки.

use contracts::system::metrics::MetricDirection;
use leptos::prelude::*;

/// Оценка изменения с учётом направления метрики.
pub fn delta_modifier(direction: MetricDirection, delta: f64) -> &'static str {
    if delta == 0.0 {
        return "delta-chip--flat";
    }
    let improved = match direction {
        MetricDirection::Higher => delta > 0.0,
        MetricDirection::Lower => delta < 0.0,
        MetricDirection::Neutral => return "delta-chip--flat",
    };
    if improved {
        "delta-chip--good"
    } else {
        "delta-chip--bad"
    }
}

/// Стрелка показывает направление изменения, цвет — его оценку. Это разные
/// каналы: значение может вырасти (↑) и это плохо (красный).
fn arrow(delta: f64) -> &'static str {
    if delta > 0.0 {
        "↑"
    } else if delta < 0.0 {
        "↓"
    } else {
        "="
    }
}

#[component]
pub fn DeltaChip(
    delta: f64,
    direction: MetricDirection,
    /// Готовый текст величины изменения (без знака — знак несёт стрелка).
    formatted: String,
    /// К чему сравнение: «с прошлого старта», «за 30 дней».
    #[prop(into)]
    since: String,
) -> impl IntoView {
    let class = format!("delta-chip {}", delta_modifier(direction, delta));

    view! {
        <span class=class>
            <span class="delta-chip__arrow">{arrow(delta)}</span>
            <span>{if delta == 0.0 { "без изменений".to_string() } else { formatted }}</span>
            <span class="delta-chip__since">{since}</span>
        </span>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colour_follows_direction_not_sign() {
        // Рост там, где больше — лучше: хорошая новость.
        assert_eq!(
            delta_modifier(MetricDirection::Higher, 5.0),
            "delta-chip--good"
        );
        // Тот же плюс там, где меньше — лучше: плохая.
        assert_eq!(
            delta_modifier(MetricDirection::Lower, 5.0),
            "delta-chip--bad"
        );
        assert_eq!(
            delta_modifier(MetricDirection::Lower, -5.0),
            "delta-chip--good"
        );
    }

    #[test]
    fn neutral_and_zero_are_never_coloured() {
        assert_eq!(
            delta_modifier(MetricDirection::Neutral, 99.0),
            "delta-chip--flat"
        );
        assert_eq!(
            delta_modifier(MetricDirection::Higher, 0.0),
            "delta-chip--flat"
        );
    }

    #[test]
    fn arrow_shows_movement_independently_of_colour() {
        assert_eq!(arrow(1.0), "↑");
        assert_eq!(arrow(-1.0), "↓");
        assert_eq!(arrow(0.0), "=");
    }
}
