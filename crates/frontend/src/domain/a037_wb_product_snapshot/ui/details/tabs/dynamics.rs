//! Dynamics tab ("Динамика") — per-product time series of stocks and ratings.
//!
//! Two separate charts (never a dual-axis): stocks (WB + MP, counts) and ratings
//! (product + feedback, 0..5). Categorical colours are the validated palette slots
//! 1 (blue) and 2 (aqua); last-value labels satisfy the relief rule for aqua.

use super::super::api::{fmt_date, SeriesPointDto};
use super::super::view_model::WbProductSnapshotDetailsVm;
use crate::shared::components::card_animated::CardAnimated;
use leptos::prelude::*;
use thaw::*;

const SERIES_1: &str = "#2a78d6"; // blue
const SERIES_2: &str = "#1baf7a"; // aqua

struct ChartSeries {
    name: &'static str,
    color: &'static str,
    values: Vec<f64>,
}

/// Мини-график: линии по дням для набора серий с общей осью Y.
fn mini_line_chart(title: &str, labels: &[String], series: Vec<ChartSeries>) -> AnyView {
    let w = 720.0_f64;
    let h = 240.0_f64;
    let pad_left = 44.0;
    let pad_right = 16.0;
    let pad_top = 16.0;
    let pad_bottom = 28.0;
    let plot_w = w - pad_left - pad_right;
    let plot_h = h - pad_top - pad_bottom;

    let n = labels.len();
    if n == 0 {
        return view! { <div class="text-muted">"Нет точек за период"</div> }.into_any();
    }

    // Общий диапазон Y по всем сериям.
    let mut y_min = f64::INFINITY;
    let mut y_max = f64::NEG_INFINITY;
    for s in &series {
        for &v in &s.values {
            y_min = y_min.min(v);
            y_max = y_max.max(v);
        }
    }
    if !y_min.is_finite() || !y_max.is_finite() {
        y_min = 0.0;
        y_max = 1.0;
    }
    if (y_max - y_min).abs() < f64::EPSILON {
        // плоская линия — раздвигаем, чтобы не делить на ноль
        y_min -= 1.0;
        y_max += 1.0;
    }

    let x_at = move |i: usize| -> f64 {
        if n == 1 {
            pad_left + plot_w / 2.0
        } else {
            pad_left + (i as f64 / (n as f64 - 1.0)) * plot_w
        }
    };
    let y_at = move |v: f64| -> f64 { pad_top + (1.0 - (v - y_min) / (y_max - y_min)) * plot_h };

    let baseline_y = pad_top + plot_h;

    // Легенда.
    let legend: Vec<AnyView> = series
        .iter()
        .map(|s| {
            let color = s.color;
            let name = s.name;
            view! {
                <span style="display:inline-flex;align-items:center;gap:6px;margin-right:14px;">
                    <span style=format!("width:12px;height:2px;background:{};display:inline-block;", color)></span>
                    <span class="text-muted" style="font-size:12px;">{name}</span>
                </span>
            }
            .into_any()
        })
        .collect();

    // Полилинии + точки + подпись последнего значения.
    let series_views: Vec<AnyView> = series
        .iter()
        .map(|s| {
            let color = s.color;
            let points_attr: String = s
                .values
                .iter()
                .enumerate()
                .map(|(i, &v)| format!("{:.1},{:.1}", x_at(i), y_at(v)))
                .collect::<Vec<_>>()
                .join(" ");

            let dots: Vec<AnyView> = s
                .values
                .iter()
                .enumerate()
                .map(|(i, &v)| {
                    view! {
                        <circle cx=format!("{:.1}", x_at(i)) cy=format!("{:.1}", y_at(v)) r="2.5" fill=color />
                    }
                    .into_any()
                })
                .collect();

            let last_label = s
                .values
                .last()
                .map(|&v| {
                    let lx = x_at(n - 1);
                    let ly = y_at(v);
                    let txt = if v.fract().abs() < f64::EPSILON {
                        format!("{}", v as i64)
                    } else {
                        format!("{:.2}", v)
                    };
                    view! {
                        <text
                            x=format!("{:.1}", lx - 4.0)
                            y=format!("{:.1}", ly - 6.0)
                            text-anchor="end"
                            style=format!("fill:{};font-size:11px;", color)
                        >{txt}</text>
                    }
                    .into_any()
                })
                .unwrap_or_else(|| view! { <></> }.into_any());

            view! {
                <polyline points=points_attr fill="none" stroke=color stroke-width="2" />
                {dots}
                {last_label}
            }
            .into_any()
        })
        .collect();

    let first_label = labels.first().cloned().unwrap_or_default();
    let last_label_txt = labels.last().cloned().unwrap_or_default();
    let y_max_txt = if y_max.fract().abs() < f64::EPSILON {
        format!("{}", y_max as i64)
    } else {
        format!("{:.1}", y_max)
    };
    let y_min_txt = if y_min.fract().abs() < f64::EPSILON {
        format!("{}", y_min as i64)
    } else {
        format!("{:.1}", y_min)
    };

    view! {
        <div style="margin-bottom:8px;">
            <div style="display:flex;align-items:center;justify-content:space-between;margin-bottom:6px;">
                <span style="font-weight:600;">{title.to_string()}</span>
                <span>{legend}</span>
            </div>
            <svg viewBox=format!("0 0 {} {}", w, h) style="width:100%;height:auto;max-width:720px;">
                // ось + baseline
                <line x1=format!("{:.1}", pad_left) y1=format!("{:.1}", pad_top)
                      x2=format!("{:.1}", pad_left) y2=format!("{:.1}", baseline_y)
                      stroke="currentColor" stroke-opacity="0.2" stroke-width="1" />
                <line x1=format!("{:.1}", pad_left) y1=format!("{:.1}", baseline_y)
                      x2=format!("{:.1}", pad_left + plot_w) y2=format!("{:.1}", baseline_y)
                      stroke="currentColor" stroke-opacity="0.2" stroke-width="1" />
                // подписи Y (min/max)
                <text x=format!("{:.1}", pad_left - 6.0) y=format!("{:.1}", pad_top + 4.0)
                      text-anchor="end" class="text-muted" style="font-size:11px;">{y_max_txt}</text>
                <text x=format!("{:.1}", pad_left - 6.0) y=format!("{:.1}", baseline_y)
                      text-anchor="end" class="text-muted" style="font-size:11px;">{y_min_txt}</text>
                // подписи X (первая/последняя дата)
                <text x=format!("{:.1}", pad_left) y=format!("{:.1}", h - 8.0)
                      text-anchor="start" class="text-muted" style="font-size:11px;">{fmt_date(&first_label)}</text>
                <text x=format!("{:.1}", pad_left + plot_w) y=format!("{:.1}", h - 8.0)
                      text-anchor="end" class="text-muted" style="font-size:11px;">{fmt_date(&last_label_txt)}</text>
                {series_views}
            </svg>
        </div>
    }
    .into_any()
}

#[component]
pub fn DynamicsTab(vm: WbProductSnapshotDetailsVm) -> impl IntoView {
    let doc = vm.doc;
    let selected_nm_id = vm.selected_nm_id;
    let series = vm.series;
    let series_loading = vm.series_loading;
    let series_from = vm.series_from;
    let series_to = vm.series_to;

    // Значение селекта товара как строка nm_id.
    let nm_select = RwSignal::new(
        selected_nm_id
            .get_untracked()
            .map(|v| v.to_string())
            .unwrap_or_default(),
    );
    Effect::new({
        let vm = vm.clone();
        move || {
            let value = nm_select.get();
            if let Ok(nm) = value.parse::<i64>() {
                if selected_nm_id.get_untracked() != Some(nm) {
                    untrack(|| {
                        selected_nm_id.set(Some(nm));
                    });
                    vm.load_series();
                }
            }
        }
    });

    // Первичная загрузка серии при открытии вкладки.
    Effect::new({
        let vm = vm.clone();
        move || {
            if selected_nm_id.get().is_some() && series.get_untracked().is_empty() {
                vm.load_series();
            }
        }
    });

    let reload = {
        let vm = vm.clone();
        move |_| vm.load_series()
    };

    view! {
        <CardAnimated delay_ms=0 nav_id="a037_wb_product_snapshot_details_dynamics">
            <Flex gap=FlexGap::Small align=FlexAlign::End>
                <div style="min-width: 340px;">
                    <Flex vertical=true gap=FlexGap::Small>
                        <Label>"Товар"</Label>
                        <Select value=nm_select>
                            {move || {
                                doc.get().map(|d| {
                                    d.lines.into_iter().map(|line| {
                                        let id = line.nm_id.to_string();
                                        let label = format!("{} · {}", line.nm_id, line.title);
                                        view! { <option value=id>{label}</option> }
                                    }).collect_view()
                                })
                            }}
                        </Select>
                    </Flex>
                </div>
                <div>
                    <Flex vertical=true gap=FlexGap::Small>
                        <Label>"С"</Label>
                        <input
                            type="date"
                            class="input"
                            prop:value=move || series_from.get()
                            on:change=move |e| series_from.set(event_target_value(&e))
                        />
                    </Flex>
                </div>
                <div>
                    <Flex vertical=true gap=FlexGap::Small>
                        <Label>"По"</Label>
                        <input
                            type="date"
                            class="input"
                            prop:value=move || series_to.get()
                            on:change=move |e| series_to.set(event_target_value(&e))
                        />
                    </Flex>
                </div>
                <Button appearance=ButtonAppearance::Primary on_click=reload>
                    {move || if series_loading.get() { "Загрузка..." } else { "Обновить" }}
                </Button>
            </Flex>

            <div style="margin-top:16px;">
                {move || {
                    let points: Vec<SeriesPointDto> = series.get();
                    if points.is_empty() {
                        return view! {
                            <div class="text-muted" style="padding:16px 0;">
                                "Нет точек за выбранный период. Динамика набирается по мере ежедневных снимков."
                            </div>
                        }.into_any();
                    }
                    let labels: Vec<String> = points.iter().map(|p| p.date.clone()).collect();
                    let stocks = vec![
                        ChartSeries { name: "Остаток WB", color: SERIES_1, values: points.iter().map(|p| p.stock_wb as f64).collect() },
                        ChartSeries { name: "Остаток МП", color: SERIES_2, values: points.iter().map(|p| p.stock_mp as f64).collect() },
                    ];
                    let ratings = vec![
                        ChartSeries { name: "Рейтинг карточки", color: SERIES_1, values: points.iter().map(|p| p.product_rating).collect() },
                        ChartSeries { name: "Оценка покупателей", color: SERIES_2, values: points.iter().map(|p| p.feedback_rating).collect() },
                    ];
                    view! {
                        <div>
                            {mini_line_chart("Остатки, шт", &labels, stocks)}
                            {mini_line_chart("Рейтинги", &labels, ratings)}
                        </div>
                    }.into_any()
                }}
            </div>
        </CardAnimated>
    }
}
