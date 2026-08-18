//! Спарклайн: график тренда внутри плитки или строки таблицы.
//!
//! Существует потому, что `points_to_svg_path` — функция, а не компонент, и
//! каждая страница до сих пор писала `<svg>` руками (инлайновый SVG живёт в
//! десятке файлов фронта). Здесь та же математика, но с обвязкой, которую
//! иначе забывают: приглушённая линия, видимые отсчёты, акцент на текущей точке
//! и ховер, который не требует попасть мышью в 8 пикселей.
//!
//! ## Почему точки рисуются путями, а не `<circle>`
//!
//! График растягивается по ширине контейнера через `preserveAspectRatio="none"`,
//! то есть горизонтальный масштаб произвольный. `<circle r="4">` в такой системе
//! координат рисуется эллипсом, тем более плоским, чем шире плитка. Нулевой
//! подпуть (`M x y l0 0`) с круглым колпачком и `vector-effect: non-scaling-stroke`
//! даёт ровный круг диаметром в `stroke-width` **экранных** пикселей — растяжение
//! viewBox обводки не касается. Сегмент `l0 0` обязателен: без явного сегмента
//! часть движков нулевой подпуть не отрисовывает.
//!
//! ## Область ответственности
//!
//! Это полоска тренда, а не полноценный график: ни сетки, ни осей, ни линий
//! порогов здесь нет. Развёрнутый разбор одной метрики — отдельный экран;
//! математика под него (`SparkScale`, `nice_step`, `grid_ticks`) уже лежит в
//! `bi_card::spark`, но в плитку она не помещается: на высоте 28 px сетка
//! добавляет линий, а не смысла.

use leptos::prelude::*;

use crate::shared::bi_card::spark::{path_with_scale, SparkScale, VIEW_H, VIEW_W};

/// Ниже трёх точек линия не несёт информации — вызывающий код обычно просто
/// не рендерит компонент, но и сам компонент в этом случае молчит.
pub const MIN_POINTS: usize = 3;

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

    let count = points.len();
    let scale = SparkScale::of(&points);
    let (line, area) = path_with_scale(&points, &scale);

    let last_value = *points.last().unwrap_or(&0.0);

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
    // Полоса центрирована на своей точке — иначе курсор подсвечивает соседнюю.
    let band = VIEW_W / (count - 1) as f64;
    let hit_bands = (0..count)
        .map(|index| {
            let left = (scale.x_of(index) - band / 2.0).max(0.0);
            let right = (scale.x_of(index) + band / 2.0).min(VIEW_W);
            let width = (right - left).max(0.0);
            view! {
                <rect
                    class="sparkline__hit"
                    x=format!("{left:.2}")
                    y="0"
                    width=format!("{width:.2}")
                    height=format!("{VIEW_H}")
                    on:mouseenter=move |_| hovered.set(Some(index))
                    on:mouseleave=move |_| hovered.set(None)
                />
            }
        })
        .collect_view();

    // Отсчёты ряда. Видимые точки отвечают на вопрос «где здесь измерения»,
    // которого сплошная линия не решает: излом и артефакт сглаживания выглядят
    // одинаково, пока не видно, что точка там действительно есть.
    let dots = points
        .iter()
        .enumerate()
        .map(|(index, value)| {
            let (x, y) = (scale.x_of(index), scale.y_of(*value));
            view! { <path class="sparkline__point" d=format!("M{x:.2} {y:.2} l0 0") /> }
        })
        .collect_view();

    // Точка под курсором подсвечивается тем же приёмом, что и последняя.
    let points_for_marker = points.clone();
    let marker = move || {
        let index = hovered.get().unwrap_or(count - 1);
        let value = points_for_marker
            .get(index)
            .copied()
            .unwrap_or(last_value);
        let (x, y) = (scale.x_of(index), scale.y_of(value));
        view! {
            <path
                class="sparkline__point sparkline__point--accent"
                d=format!("M{x:.2} {y:.2} l0 0")
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
                {dots}
                {marker}
                {hit_bands}
            </svg>
            <div class="sparkline__caption">{caption}</div>
        </div>
    }
    .into_any()
}
