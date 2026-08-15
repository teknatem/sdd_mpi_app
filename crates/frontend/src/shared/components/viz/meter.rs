//! Метр: значение против порогов.
//!
//! Цветная рамка отвечает на вопрос «плохо ли», но не на вопрос «насколько
//! далеко до плохо» — а именно он и нужен, когда метрика ползёт. Метр рисует
//! шкалу с засечками порогов и заливкой до текущего значения.
//!
//! Направление метрики учитывается в геометрии: у «меньше — лучше» шкала растёт
//! слева направо к плохому, у «больше — лучше» — наоборот, поэтому заполненная
//! часть в обоих случаях означает одно и то же — «пройденный путь к порогу».

use contracts::system::metrics::{MetricDirection, MetricStatus};
use leptos::prelude::*;

/// Верх шкалы. У «меньше — лучше» это `bad` с запасом, чтобы засечка не
/// прилипала к правому краю и было видно, что за ней ещё есть куда расти.
const HEADROOM: f64 = 1.15;

fn fill_modifier(status: MetricStatus) -> &'static str {
    match status {
        MetricStatus::Ok => "meter__fill--ok",
        MetricStatus::Warn => "meter__fill--warn",
        MetricStatus::Bad => "meter__fill--bad",
        MetricStatus::Neutral => "",
    }
}

/// Доля шкалы `[0..1]`, на которой стоит значение.
fn position(value: f64, top: f64) -> f64 {
    if top <= 0.0 {
        return 0.0;
    }
    (value / top).clamp(0.0, 1.0)
}

#[component]
pub fn Meter(
    value: f64,
    warn: f64,
    bad: f64,
    direction: MetricDirection,
    status: MetricStatus,
    /// Форматтер для подписей шкалы.
    format_value: Callback<(f64,), String>,
) -> impl IntoView {
    // «Больше — лучше» разворачиваем: bad < warn, и верх шкалы задаёт значение,
    // а не порог, иначе 100 % при пороге 40 % упрётся в край.
    let top = match direction {
        MetricDirection::Higher => value.max(warn).max(bad) * HEADROOM,
        _ => value.max(bad) * HEADROOM,
    };

    let fill = position(value, top);
    let warn_at = position(warn, top);
    let bad_at = position(bad, top);

    let fill_class = format!("meter__fill {}", fill_modifier(status));
    let warn_label = format_value.run((warn,));
    let bad_label = format_value.run((bad,));

    view! {
        <div class="meter">
            <div class="meter__track">
                <div
                    class=fill_class
                    style:width=format!("{:.1}%", fill * 100.0)
                ></div>
                <div class="meter__tick" style:left=format!("{:.1}%", warn_at * 100.0)></div>
                <div class="meter__tick" style:left=format!("{:.1}%", bad_at * 100.0)></div>
            </div>
            <div class="meter__scale">
                <span>{format!("warn {warn_label}")}</span>
                <span>{format!("bad {bad_label}")}</span>
            </div>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn position_is_clamped_to_the_track() {
        assert_eq!(position(0.0, 100.0), 0.0);
        assert_eq!(position(50.0, 100.0), 0.5);
        // Значение за верхом шкалы упирается в край, а не уезжает наружу.
        assert_eq!(position(500.0, 100.0), 1.0);
    }

    /// Пустая шкала не должна давать деления на ноль и NaN в `width`.
    #[test]
    fn zero_top_does_not_divide_by_zero() {
        assert_eq!(position(10.0, 0.0), 0.0);
    }
}
