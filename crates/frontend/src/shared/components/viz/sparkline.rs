//! Спарклайн: маленький график тренда внутри плитки или строки таблицы.
//!
//! Существует потому, что `points_to_svg_path` — функция, а не компонент, и
//! каждая страница до сих пор писала `<svg>` руками (инлайновый SVG живёт в
//! десятке файлов фронта). Здесь та же математика, но с обвязкой, которую
//! иначе забывают: приглушённая линия, акцентная последняя точка с кольцом в
//! цвет поверхности и ховер, который не требует попасть мышью в 8 пикселей.

use leptos::prelude::*;

use crate::shared::bi_card::spark::points_to_svg_path;

/// Ниже трёх точек линия не несёт информации — вызывающий код обычно просто
/// не рендерит компонент, но и сам компонент в этом случае молчит.
pub const MIN_POINTS: usize = 3;

/// Ширина и высота системы координат SVG. Совпадает с той, в которой считает
/// `points_to_svg_path`; наружу график тянется через `width: 100%`.
const VIEW_W: f64 = 100.0;
const VIEW_H: f64 = 30.0;

#[component]
pub fn Sparkline(
    /// Значения от старых к новым.
    points: Vec<f64>,
    /// Форматтер для подписи под графиком (ховер и последнее значение).
    format_value: Callback<(f64,), String>,
    /// Подпись точки: обычно дата снимка. Длина совпадает с `points`.
    #[prop(optional)]
    labels: Vec<String>,
) -> impl IntoView {
    if points.len() < MIN_POINTS {
        return ().into_any();
    }

    let (line, area) = points_to_svg_path(&points);
    let count = points.len();

    // Последняя точка — та, о которой плитка и рассказывает.
    let min = points.iter().cloned().fold(f64::INFINITY, f64::min);
    let max = points.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let range = if (max - min).abs() < 1e-9 {
        1.0
    } else {
        max - min
    };
    let y_of = move |value: f64| 28.0 - (value - min) / range * 24.0;
    let x_of = move |index: usize| index as f64 / (count - 1) as f64 * VIEW_W;

    let last_value = *points.last().unwrap_or(&0.0);
    let last_x = VIEW_W;
    let last_y = y_of(last_value);

    // Подпись: по умолчанию последнее значение, при наведении — точка под курсором.
    let hovered = RwSignal::new(None::<usize>);
    let points_for_caption = points.clone();
    let labels_for_caption = labels.clone();
    let caption = move || {
        let index = hovered.get().unwrap_or(count - 1);
        let value = points_for_caption.get(index).copied().unwrap_or(last_value);
        let shown = format_value.run((value,));
        match labels_for_caption.get(index) {
            Some(label) if !label.is_empty() => format!("{shown} · {label}"),
            _ => shown,
        }
    };

    // Полосы-мишени во всю высоту: ховер по вертикальной полосе, а не по точке.
    let band = VIEW_W / count as f64;
    let hit_bands = (0..count)
        .map(|index| {
            let x = index as f64 * band;
            view! {
                <rect
                    class="sparkline__hit"
                    x=format!("{x:.2}")
                    y="0"
                    width=format!("{band:.2}")
                    height=format!("{VIEW_H}")
                    on:mouseenter=move |_| hovered.set(Some(index))
                    on:mouseleave=move |_| hovered.set(None)
                />
            }
        })
        .collect_view();

    // Точка под курсором подсвечивается тем же маркером, что и последняя.
    let marker = move || {
        let index = hovered.get().unwrap_or(count - 1);
        let value = points.get(index).copied().unwrap_or(last_value);
        let (cx, cy) = if hovered.get().is_some() {
            (x_of(index), y_of(value))
        } else {
            (last_x, last_y)
        };
        view! {
            <circle
                class="sparkline__dot"
                cx=format!("{cx:.2}")
                cy=format!("{cy:.2}")
                r="4"
            />
        }
    };

    view! {
        <div class="sparkline">
            <svg
                class="sparkline__svg"
                viewBox=format!("0 0 {VIEW_W} {VIEW_H}")
                preserveAspectRatio="none"
                role="img"
            >
                <path class="sparkline__area" d=area />
                <path class="sparkline__line" d=line />
                {marker}
                {hit_bands}
            </svg>
            <div class="sparkline__caption">{caption}</div>
        </div>
    }
    .into_any()
}
