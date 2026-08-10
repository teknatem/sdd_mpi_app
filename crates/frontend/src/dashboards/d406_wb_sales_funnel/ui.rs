use crate::dashboards::d406_wb_sales_funnel::api;
use crate::shared::api_utils::api_base;
use crate::shared::export::{export_to_excel, ExcelExportable};
use crate::shared::page_frame::PageFrame;
use crate::shared::time_bar_chart::format_number_triads;
use chrono::{Datelike, Utc};
use contracts::dashboards::d406_wb_sales_funnel::{
    FunnelChannel, FunnelDateAxis, WbSalesFunnelConversions, WbSalesFunnelMetrics,
    WbSalesFunnelOrdersResponse, WbSalesFunnelResponse, WbSalesFunnelRow,
};
use contracts::domain::a006_connection_mp::aggregate::ConnectionMP;
use contracts::domain::common::AggregateId;
use gloo_net::http::Request;
use leptos::prelude::*;
use leptos::task::spawn_local;
use std::cmp::Ordering;
use thaw::{Tab, TabList};

// ---------------------------------------------------------------------------
// Форматирование
// ---------------------------------------------------------------------------

fn fmt_int(value: i64) -> String {
    format_number_triads(value as f64, 0)
}

/// Показ метрики показов с учётом доступности источника: `N/A`, если источника нет
/// (напр. нет подписки «Джем»/рекламы) — «недоступно» ≠ «ноль».
fn fmt_avail(value: i64, available: bool) -> String {
    if available {
        fmt_int(value)
    } else {
        "N/A".to_string()
    }
}

fn fmt_money(value: f64) -> String {
    format_number_triads(value, 0)
}

/// Конверсия в % (или прочерк, если знаменатель = 0).
fn fmt_pct(value: Option<f64>) -> String {
    match value {
        Some(v) => format!("{v:.1}%"),
        None => "—".to_string(),
    }
}

/// Сумма с учётом доступности: `N/A`, если стадия недоступна в выбранном канале.
fn fmt_money_avail(value: f64, available: bool) -> String {
    if available {
        fmt_money(value)
    } else {
        "N/A".to_string()
    }
}

/// Проекция метрик на выбранный канал (All/Paid/Free). Возвращает «отображаемые» метрики
/// (поля показы/переходы/корзина/заказы/выкупы/отмены/возвраты/сумма замещены значениями канала)
/// и флаг доступности рекламо-зависимых стадий (переходы…возвраты). Показы несут собственную
/// доступность через `show_total_available`.
///
/// All — как есть; Paid — из `paid_*`; Free — `total − paid` (обрезка ≥0). Free показы =
/// органические (a040), обычно N/A.
fn channel_metrics(m: &WbSalesFunnelMetrics, ch: FunnelChannel) -> (WbSalesFunnelMetrics, bool) {
    let mut d = m.clone();
    let stages_available = matches!(ch, FunnelChannel::All) || m.advert_available;
    // Счётчики самого маркетплейса (funnel_cancel_*, total_impressions) — дневные
    // агрегаты без атрибуции к рекламе, разложить их по каналам нечем. Под канальным
    // фильтром показываем N/A, а не долю: делённое пополам число было бы выдумкой.
    if !matches!(ch, FunnelChannel::All) {
        d.funnel_cancel_count = 0;
        d.funnel_cancel_sum = 0.0;
        d.funnel_cancel_available = false;
        d.total_impressions = 0;
        d.total_impressions_available = false;
    }
    match ch {
        FunnelChannel::All => {
            // У YM показы приходят готовым общим счётчиком (a041), а не суммой
            // платных и органических — подставляем его в колонку «Показы».
            // У WB общего счётчика нет, там остаётся free + paid.
            if m.total_impressions_available {
                d.show_total_count = m.total_impressions;
                d.show_total_available = true;
            }
        }
        FunnelChannel::Paid => {
            d.show_total_count = m.show_paid_count;
            d.show_total_available = m.show_paid_available;
            d.open_count = m.paid_open_count;
            d.cart_count = m.paid_cart_count;
            d.order_count = m.paid_order_count;
            d.order_sum = m.paid_order_sum;
            d.buyout_count = m.paid_buyout_count;
            d.buyout_sum = m.paid_buyout_sum;
            d.cancel_count = m.paid_cancel_count;
            d.cancel_sum = m.paid_cancel_sum;
            d.return_count = m.paid_return_count;
            d.return_sum = m.paid_return_sum;
        }
        FunnelChannel::Free => {
            d.show_total_count = m.show_free_count;
            d.show_total_available = m.show_free_available;
            d.open_count = (m.open_count - m.paid_open_count).max(0);
            d.cart_count = (m.cart_count - m.paid_cart_count).max(0);
            d.order_count = (m.order_count - m.paid_order_count).max(0);
            d.order_sum = (m.order_sum - m.paid_order_sum).max(0.0);
            d.buyout_count = (m.buyout_count - m.paid_buyout_count).max(0);
            d.buyout_sum = (m.buyout_sum - m.paid_buyout_sum).max(0.0);
            d.cancel_count = (m.cancel_count - m.paid_cancel_count).max(0);
            d.cancel_sum = (m.cancel_sum - m.paid_cancel_sum).max(0.0);
            d.return_count = (m.return_count - m.paid_return_count).max(0);
            d.return_sum = (m.return_sum - m.paid_return_sum).max(0.0);
        }
    }
    (d, stages_available)
}

fn first_day_of_month() -> String {
    let today = Utc::now().date_naive();
    format!("{:04}-{:02}-01", today.year(), today.month())
}

fn today() -> String {
    Utc::now().date_naive().format("%Y-%m-%d").to_string()
}

/// Артикул для колонки «Артикул»: article → nm_id → a007-ref → прочерк.
fn article_label(row: &WbSalesFunnelRow) -> String {
    if let Some(a) = row.article.as_ref().filter(|s| !s.trim().is_empty()) {
        a.clone()
    } else if let Some(nm) = row.nm_id {
        format!("nm {nm}")
    } else if let Some(mpr) = row.marketplace_product_ref.as_ref() {
        mpr.chars().take(8).collect::<String>()
    } else {
        "—".to_string()
    }
}

// ---------------------------------------------------------------------------
// Агрегация/сортировка на клиенте
// ---------------------------------------------------------------------------

/// Пересчитать «Итого» по (уже отфильтрованному) набору строк.
fn compute_totals(rows: &[WbSalesFunnelRow]) -> WbSalesFunnelMetrics {
    let mut t = WbSalesFunnelMetrics::default();
    for r in rows {
        let m = &r.metrics;
        t.show_free_count += m.show_free_count;
        t.show_paid_count += m.show_paid_count;
        t.show_total_count += m.show_total_count;
        t.show_free_available |= m.show_free_available;
        t.show_paid_available |= m.show_paid_available;
        t.show_total_available |= m.show_total_available;
        t.total_impressions += m.total_impressions;
        t.total_impressions_available |= m.total_impressions_available;
        t.advert_available |= m.advert_available;
        t.open_count += m.open_count;
        t.cart_count += m.cart_count;
        t.paid_open_count += m.paid_open_count;
        t.paid_cart_count += m.paid_cart_count;
        t.wishlist_count += m.wishlist_count;
        t.funnel_order_count += m.funnel_order_count;
        t.funnel_order_sum += m.funnel_order_sum;
        t.funnel_cancel_count += m.funnel_cancel_count;
        t.funnel_cancel_sum += m.funnel_cancel_sum;
        t.funnel_cancel_available |= m.funnel_cancel_available;
        t.order_count += m.order_count;
        t.order_sum += m.order_sum;
        t.paid_order_count += m.paid_order_count;
        t.paid_order_sum += m.paid_order_sum;
        t.cancel_count += m.cancel_count;
        t.cancel_sum += m.cancel_sum;
        t.paid_cancel_count += m.paid_cancel_count;
        t.paid_cancel_sum += m.paid_cancel_sum;
        t.buyout_count += m.buyout_count;
        t.buyout_sum += m.buyout_sum;
        t.paid_buyout_count += m.paid_buyout_count;
        t.paid_buyout_sum += m.paid_buyout_sum;
        t.return_count += m.return_count;
        t.return_sum += m.return_sum;
        t.paid_return_count += m.paid_return_count;
        t.paid_return_sum += m.paid_return_sum;
    }
    t
}

fn cmp_opt_f64(a: Option<f64>, b: Option<f64>) -> Ordering {
    match (a, b) {
        (Some(x), Some(y)) => x.partial_cmp(&y).unwrap_or(Ordering::Equal),
        (Some(_), None) => Ordering::Greater,
        (None, Some(_)) => Ordering::Less,
        (None, None) => Ordering::Equal,
    }
}

/// Сортировка строк по коду поля (см. заголовки таблицы).
fn sort_rows(rows: &mut [WbSalesFunnelRow], field: &str, ascending: bool) {
    rows.sort_by(|a, b| {
        let ord = match field {
            "date" => a.date.cmp(&b.date),
            "marketplace" => a
                .marketplace
                .as_deref()
                .unwrap_or("")
                .cmp(b.marketplace.as_deref().unwrap_or("")),
            "article" => article_label(a).cmp(&article_label(b)),
            "show_total" => a.metrics.show_total_count.cmp(&b.metrics.show_total_count),
            "show_paid" => a.metrics.show_paid_count.cmp(&b.metrics.show_paid_count),
            "show_free" => a.metrics.show_free_count.cmp(&b.metrics.show_free_count),
            "open" => a.metrics.open_count.cmp(&b.metrics.open_count),
            "cart" => a.metrics.cart_count.cmp(&b.metrics.cart_count),
            "order" => a.metrics.order_count.cmp(&b.metrics.order_count),
            "buyout" => a.metrics.buyout_count.cmp(&b.metrics.buyout_count),
            "cancel" => a.metrics.cancel_count.cmp(&b.metrics.cancel_count),
            "funnel_cancel" => a
                .metrics
                .funnel_cancel_count
                .cmp(&b.metrics.funnel_cancel_count),
            "return" => a.metrics.return_count.cmp(&b.metrics.return_count),
            "order_sum" => a
                .metrics
                .order_sum
                .partial_cmp(&b.metrics.order_sum)
                .unwrap_or(Ordering::Equal),
            "conv_open_cart" => cmp_opt_f64(a.conversions.open_to_cart, b.conversions.open_to_cart),
            "conv_cart_order" => {
                cmp_opt_f64(a.conversions.cart_to_order, b.conversions.cart_to_order)
            }
            "conv_order_buyout" => {
                cmp_opt_f64(a.conversions.order_to_buyout, b.conversions.order_to_buyout)
            }
            "conv_cancel" => cmp_opt_f64(a.conversions.cancel_rate, b.conversions.cancel_rate),
            "conv_funnel_cancel" => cmp_opt_f64(
                a.conversions.funnel_cancel_rate,
                b.conversions.funnel_cancel_rate,
            ),
            _ => Ordering::Equal,
        };
        if ascending {
            ord
        } else {
            ord.reverse()
        }
    });
}

/// Отфильтровать (по маркетплейсу) и отсортировать строки ответа.
fn view_rows(
    response: &WbSalesFunnelResponse,
    marketplace: &str,
    sort_field: &str,
    ascending: bool,
) -> Vec<WbSalesFunnelRow> {
    let mut rows: Vec<WbSalesFunnelRow> = response
        .rows
        .iter()
        .filter(|r| marketplace.is_empty() || r.marketplace.as_deref() == Some(marketplace))
        .cloned()
        .collect();
    sort_rows(&mut rows, sort_field, ascending);
    rows
}

// ---------------------------------------------------------------------------
// CSV-экспорт
// ---------------------------------------------------------------------------

struct FunnelExportRow {
    date: String,
    marketplace: String,
    article: String,
    name: String,
    /// Метрики уже спроецированы на выбранный канал (см. export_rows).
    metrics: WbSalesFunnelMetrics,
    conversions: WbSalesFunnelConversions,
    /// Доступность рекламо-зависимых стадий в выбранном канале (иначе N/A).
    stages_available: bool,
}

impl ExcelExportable for FunnelExportRow {
    fn headers() -> Vec<&'static str> {
        vec![
            "Дата",
            "Маркетплейс",
            "Артикул",
            "Наименование",
            "Показы",
            "Переходы",
            "Корзина",
            "Заказы",
            "Выкупы",
            "Отмены (заказы)",
            "Отмены (воронка)",
            "Возвраты",
            "Сумма заказов",
            "Переход→корзина, %",
            "Корзина→заказ, %",
            "Заказ→выкуп, %",
            "Доля отмен (заказы), %",
            "Доля отказов (воронка), %",
        ]
    }

    fn to_csv_row(&self) -> Vec<String> {
        let m = &self.metrics;
        let c = &self.conversions;
        let a = self.stages_available;
        let pct = |v: Option<f64>| v.map(|x| format!("{x:.1}")).unwrap_or_default();
        let cnt = |v: i64| if a { v.to_string() } else { "N/A".to_string() };
        vec![
            self.date.clone(),
            self.marketplace.clone(),
            self.article.clone(),
            self.name.clone(),
            fmt_avail(m.show_total_count, m.show_total_available),
            cnt(m.open_count),
            cnt(m.cart_count),
            cnt(m.order_count),
            cnt(m.buyout_count),
            cnt(m.cancel_count),
            // Счётчик отказов маркетплейса: своя доступность, не зависит от канального N/A.
            fmt_avail(m.funnel_cancel_count, m.funnel_cancel_available),
            cnt(m.return_count),
            if a {
                format!("{:.0}", m.order_sum)
            } else {
                "N/A".to_string()
            },
            pct(c.open_to_cart),
            pct(c.cart_to_order),
            pct(c.order_to_buyout),
            pct(c.cancel_rate),
            pct(c.funnel_cancel_rate),
        ]
    }
}

fn export_rows(
    rows: &[WbSalesFunnelRow],
    totals: &WbSalesFunnelMetrics,
    channel: FunnelChannel,
) -> Vec<FunnelExportRow> {
    // Конверсии/значения экспортируем по выбранному каналу.
    let conv_of = |m: &WbSalesFunnelMetrics| {
        WbSalesFunnelConversions::from_metrics_with_funnel_cancel(
            m.open_count,
            m.cart_count,
            m.order_count,
            m.buyout_count,
            m.cancel_count,
            m.funnel_cancel_count,
            m.funnel_order_count,
        )
    };
    let mut out: Vec<FunnelExportRow> = Vec::with_capacity(rows.len() + 1);
    // Первая строка — «Итого за период».
    let (dt, ta) = channel_metrics(totals, channel);
    let dtc = conv_of(&dt);
    out.push(FunnelExportRow {
        date: "Итого за период".to_string(),
        marketplace: String::new(),
        article: String::new(),
        name: String::new(),
        metrics: dt,
        conversions: dtc,
        stages_available: ta,
    });
    for r in rows {
        let (dm, avail) = channel_metrics(&r.metrics, channel);
        let dc = conv_of(&dm);
        out.push(FunnelExportRow {
            date: r.date.clone(),
            marketplace: r.marketplace.clone().unwrap_or_default(),
            article: article_label(r),
            name: r.product_name.clone().unwrap_or_default(),
            metrics: dm,
            conversions: dc,
            stages_available: avail,
        });
    }
    out
}

// ---------------------------------------------------------------------------
// Ячейки таблицы
// ---------------------------------------------------------------------------

/// Контекст ячейки для drilldown списка заказов (`nm_id × дата` одного кабинета).
#[derive(Clone, Debug)]
struct DrillCtx {
    connection_mp_ref: String,
    nm_id: i64,
    date: String,
    article: String,
}

/// Числовые `<td>` строки (метрики канала + конверсии) — общий фрагмент для строк и «Итого».
/// `m` уже спроецированы на выбранный канал (`channel_metrics`); `stages_available` = доступны ли
/// рекламо-зависимые стадии (переходы…возвраты) в этом канале (иначе `N/A`). `drill` (только у
/// строк данных) делает ячейку «Заказы» кликабельной — открывает список заказов с меткой канала.
fn metric_cells(
    m: &WbSalesFunnelMetrics,
    c: &WbSalesFunnelConversions,
    stages_available: bool,
    drill: Option<(RwSignal<Option<DrillCtx>>, DrillCtx)>,
) -> impl IntoView {
    let order_str = fmt_avail(m.order_count, stages_available);
    let order_cell: AnyView = match drill {
        Some((sig, ctx)) => view! {
            <td
                class="d406-n d406-drill"
                title="Клик — список заказов с меткой канала (платн./беспл.)"
                on:click=move |_| sig.set(Some(ctx.clone()))
            >{order_str}</td>
        }
        .into_any(),
        None => view! { <td class="d406-n">{order_str}</td> }.into_any(),
    };
    view! {
        <td class="d406-n">{fmt_avail(m.show_total_count, m.show_total_available)}</td>
        <td class="d406-n">{fmt_avail(m.open_count, stages_available)}</td>
        <td class="d406-n">{fmt_avail(m.cart_count, stages_available)}</td>
        {order_cell}
        <td class="d406-n">{fmt_avail(m.buyout_count, stages_available)}</td>
        <td class="d406-n">{fmt_avail(m.cancel_count, stages_available)}</td>
        // Счётчик отказов маркетплейса: канального сплита у него нет (это дневной
        // агрегат), поэтому доступность своя, а не stages_available.
        <td class="d406-n">{fmt_avail(m.funnel_cancel_count, m.funnel_cancel_available)}</td>
        <td class="d406-n">{fmt_avail(m.return_count, stages_available)}</td>
        <td class="d406-money">{fmt_money_avail(m.order_sum, stages_available)}</td>
        <td class="d406-c">{fmt_pct(c.open_to_cart)}</td>
        <td class="d406-c">{fmt_pct(c.cart_to_order)}</td>
        <td class="d406-c">{fmt_pct(c.order_to_buyout)}</td>
        <td class="d406-c">{fmt_pct(c.cancel_rate)}</td>
        <td class="d406-c">{fmt_pct(c.funnel_cancel_rate)}</td>
    }
}

/// Бейдж маркетплейса.
fn marketplace_badge(row: &WbSalesFunnelRow) -> impl IntoView {
    match row.marketplace.as_ref() {
        Some(name) => {
            let code = row.marketplace_code.clone().unwrap_or_default();
            let cls = format!(
                "d406-badge d406-badge--{}",
                if code.is_empty() {
                    "muted".to_string()
                } else {
                    code
                }
            );
            view! { <span class=cls>{name.clone()}</span> }.into_any()
        }
        None => view! { <span class="d406-badge d406-badge--muted">"—"</span> }.into_any(),
    }
}

// ---------------------------------------------------------------------------
// Заголовок с сортировкой
// ---------------------------------------------------------------------------

fn sort_th(
    label: &'static str,
    field: &'static str,
    left: bool,
    title: &'static str,
    sort_field: RwSignal<String>,
    sort_asc: RwSignal<bool>,
) -> impl IntoView {
    let class = if left { "d406-th--left" } else { "" };
    let indicator = move || {
        if sort_field.get() == field {
            if sort_asc.get() {
                "▲"
            } else {
                "▼"
            }
        } else {
            "↕"
        }
    };
    let ind_class = move || {
        if sort_field.get() == field {
            "d406-sort d406-sort--active"
        } else {
            "d406-sort"
        }
    };
    view! {
        <th
            class=class
            title=title
            on:click=move |_| {
                if sort_field.get() == field {
                    sort_asc.update(|v| *v = !*v);
                } else {
                    sort_field.set(field.to_string());
                    sort_asc.set(false); // по числовым — сначала по убыванию (интереснее)
                }
            }
        >
            {label}
            <span class=ind_class>{indicator}</span>
        </th>
    }
}

// ---------------------------------------------------------------------------
// Диаграмма-воронка (инлайн SVG)
// ---------------------------------------------------------------------------

/// Ступени воронки из «Итого». Показы включаются только если источник доступен.
fn funnel_stages(m: &WbSalesFunnelMetrics) -> Vec<(&'static str, i64)> {
    let mut stages: Vec<(&'static str, i64)> = Vec::new();
    if m.show_total_available {
        stages.push(("Показы", m.show_total_count));
    }
    stages.push(("Переходы", m.open_count));
    stages.push(("Корзина", m.cart_count));
    stages.push(("Заказы", m.order_count));
    stages.push(("Выкупы", m.buyout_count));
    stages
}

fn render_funnel(m: &WbSalesFunnelMetrics) -> impl IntoView {
    let stages = funnel_stages(m);
    let n = stages.len();

    // Геометрия viewBox.
    let vb_w = 1000.0_f64;
    let gutter_x = 196.0_f64; // правый край левой колонки с названиями
    let x0 = 214.0_f64; // левый край области воронки
    let region_w = 984.0_f64 - x0; // ширина области воронки
    let stage_h = 46.0_f64;
    let gap_v = 38.0_f64;
    let pad_top = 22.0_f64;
    let pad_bottom = 14.0_f64;
    let vb_h = pad_top + (n as f64) * stage_h + (n.saturating_sub(1) as f64) * gap_v + pad_bottom;

    let max_val = stages.iter().map(|(_, v)| *v).max().unwrap_or(0).max(1) as f64;
    let cx = x0 + region_w / 2.0;

    let mut bars: Vec<AnyView> = Vec::new();
    let mut labels: Vec<AnyView> = Vec::new();

    for (i, (name, value)) in stages.iter().enumerate() {
        let y = pad_top + (i as f64) * (stage_h + gap_v);
        let frac = (*value as f64) / max_val;
        let bw = (region_w * frac).max(3.0);
        let bx = x0 + (region_w - bw) / 2.0;
        let mid_y = y + stage_h / 2.0;

        // Полоса.
        bars.push(
            view! {
                <rect
                    x=bx
                    y=y
                    width=bw
                    height=stage_h
                    rx="6"
                    fill="#2563eb"
                />
            }
            .into_any(),
        );

        // Название ступени (левая колонка, выравнивание вправо).
        labels.push(
            view! {
                <text
                    x=gutter_x
                    y=mid_y
                    text-anchor="end"
                    dominant-baseline="central"
                    class="d406-funnel-stage-label"
                >
                    {name.to_string()}
                </text>
            }
            .into_any(),
        );

        // Значение: белым по центру полосы, если она достаточно широкая; иначе — справа.
        let value_str = format_number_triads(*value as f64, 0);
        if bw >= 88.0 {
            labels.push(
                view! {
                    <text
                        x=cx
                        y=mid_y
                        text-anchor="middle"
                        dominant-baseline="central"
                        class="d406-funnel-value"
                    >
                        {value_str}
                    </text>
                }
                .into_any(),
            );
        } else {
            let right_x = bx + bw + 8.0;
            labels.push(
                view! {
                    <text
                        x=right_x
                        y=mid_y
                        text-anchor="start"
                        dominant-baseline="central"
                        class="d406-funnel-stage-label"
                    >
                        {value_str}
                    </text>
                }
                .into_any(),
            );
        }

        // Конверсия к следующей ступени — в промежутке.
        if i + 1 < n {
            let next_val = stages[i + 1].1;
            let conv = if *value > 0 {
                format!("↓ {:.1}%", next_val as f64 / *value as f64 * 100.0)
            } else {
                "↓ —".to_string()
            };
            let conv_y = y + stage_h + gap_v / 2.0;
            labels.push(
                view! {
                    <text
                        x=cx
                        y=conv_y
                        text-anchor="middle"
                        dominant-baseline="central"
                        class="d406-funnel-conv"
                    >
                        {conv}
                    </text>
                }
                .into_any(),
            );
        }
    }

    let view_box = format!("0 0 {vb_w} {vb_h}");
    view! {
        <svg
            class="d406-funnel-svg"
            viewBox=view_box
            preserveAspectRatio="xMidYMid meet"
            role="img"
            aria-label="Диаграмма воронки продаж"
        >
            {bars}
            {labels}
        </svg>
    }
}

// ---------------------------------------------------------------------------
// Главный компонент
// ---------------------------------------------------------------------------

#[component]
pub fn WbSalesFunnelDashboard() -> impl IntoView {
    let date_from = RwSignal::new(first_day_of_month());
    let date_to = RwSignal::new(today());
    let connection_mp_ref = RwSignal::new(String::new());
    let nm_id = RwSignal::new(String::new());
    let axis = RwSignal::new(FunnelDateAxis::Cohort);
    let data = RwSignal::new(None::<WbSalesFunnelResponse>);
    let loading = RwSignal::new(false);
    let error = RwSignal::new(None::<String>);
    // Список кабинетов (id, подпись) для выпадающего выбора вместо ручного ввода UUID.
    let cabinets = RwSignal::new(Vec::<(String, String)>::new());

    // Новое состояние.
    let active_tab = RwSignal::new("table".to_string());
    let marketplace_filter = RwSignal::new(String::new());
    let channel = RwSignal::new(FunnelChannel::All);
    let sort_field = RwSignal::new("date".to_string());
    let sort_asc = RwSignal::new(true);
    // Пагинация на клиенте: рендерим в DOM только текущую страницу (ответ может содержать
    // десятки тысяч строк товар×дата — весь набор в DOM = гигабайты памяти и жуткие тормоза).
    let page = RwSignal::new(0usize);
    let page_size = RwSignal::new(100usize);

    // Drilldown списка заказов ячейки (платн./беспл.).
    let drilldown = RwSignal::new(None::<DrillCtx>);
    let drill_data = RwSignal::new(None::<WbSalesFunnelOrdersResponse>);
    let drill_loading = RwSignal::new(false);
    let drill_error = RwSignal::new(None::<String>);

    spawn_local(async move {
        let url = format!("{}/api/connection_mp", api_base());
        let Ok(resp) = Request::get(&url).send().await else {
            return;
        };
        if !resp.ok() {
            return;
        }
        if let Ok(data) = resp.json::<Vec<ConnectionMP>>().await {
            let mut opts: Vec<(String, String)> = data
                .into_iter()
                .map(|conn| {
                    let label = if conn.base.description.trim().is_empty() {
                        conn.base.code.clone()
                    } else {
                        conn.base.description.clone()
                    };
                    (conn.base.id.as_string(), label)
                })
                .collect();
            opts.sort_by(|a, b| a.1.cmp(&b.1));
            cabinets.set(opts);
        }
    });

    let load = move || {
        let df = date_from.get_untracked();
        let dt = date_to.get_untracked();
        let conn = connection_mp_ref.get_untracked();
        let nm = nm_id.get_untracked();
        let ax = axis.get_untracked();
        loading.set(true);
        error.set(None);
        page.set(0);
        spawn_local(async move {
            match api::get_wb_sales_funnel(&df, &dt, &conn, &nm, ax).await {
                Ok(response) => {
                    data.set(Some(response));
                    loading.set(false);
                }
                Err(message) => {
                    error.set(Some(message));
                    loading.set(false);
                }
            }
        });
    };

    Effect::new(move |_| load());

    // Смена фильтра/сортировки меняет набор/порядок строк → возвращаемся на первую страницу.
    // Канал (проекция значений) порядок не меняет, поэтому в ключ не входит.
    Effect::new(move |prev: Option<(String, String, bool)>| {
        let key = (marketplace_filter.get(), sort_field.get(), sort_asc.get());
        if prev.is_some() && prev.as_ref() != Some(&key) {
            page.set(0);
        }
        key
    });

    // Загрузка drilldown-списка при открытии ячейки.
    Effect::new(move |_| {
        let Some(ctx) = drilldown.get() else {
            return;
        };
        drill_data.set(None);
        drill_error.set(None);
        drill_loading.set(true);
        let conn = ctx.connection_mp_ref.clone();
        let nm = ctx.nm_id;
        let date = ctx.date.clone();
        spawn_local(async move {
            match api::get_wb_sales_funnel_orders(&conn, nm, &date).await {
                Ok(response) => {
                    drill_data.set(Some(response));
                    drill_loading.set(false);
                }
                Err(message) => {
                    drill_error.set(Some(message));
                    drill_loading.set(false);
                }
            }
        });
    });

    // Экспорт текущего (отфильтрованного+отсортированного) набора в CSV.
    let do_export = move |_| {
        let Some(response) = data.get_untracked() else {
            return;
        };
        let rows = view_rows(
            &response,
            &marketplace_filter.get_untracked(),
            &sort_field.get_untracked(),
            sort_asc.get_untracked(),
        );
        let totals = compute_totals(&rows);
        let export = export_rows(&rows, &totals, channel.get_untracked());
        let _ = export_to_excel(&export, "sales_funnel.csv");
    };

    view! {
        <PageFrame page_id="d406_wb_sales_funnel--dashboard" category="dashboard" class="page--wide d406-page">
            <div class="d406-shell">
                <div class="d406-head">
                    <h1 class="d406-title">"Воронка продаж"</h1>
                    <div class="d406-note">
                        "Показы → переходы → корзина → заказы → выкупы. Фильтр «Канал» строит воронку "
                        "всего / платного (реклама a026 + заказы по атрибуции p913) / органического трафика. "
                        "N/A — рекламных данных за период нет (бесплатные показы органики пока N/A)."
                    </div>
                </div>

                <div class="d406-toolbar">
                    <div class="d406-field">
                        <label>"Период с"</label>
                        <input
                            type="date"
                            prop:value=move || date_from.get()
                            on:input=move |ev| date_from.set(event_target_value(&ev))
                        />
                    </div>
                    <div class="d406-field">
                        <label>"Период по"</label>
                        <input
                            type="date"
                            prop:value=move || date_to.get()
                            on:input=move |ev| date_to.set(event_target_value(&ev))
                        />
                    </div>
                    <div class="d406-field">
                        <label>"Кабинет"</label>
                        <select
                            prop:value=move || connection_mp_ref.get()
                            on:change=move |ev| {
                                connection_mp_ref.set(event_target_value(&ev));
                                load();
                            }
                        >
                            <option value="">"Все кабинеты"</option>
                            <For
                                each=move || cabinets.get()
                                key=|(id, _)| id.clone()
                                children=move |(id, label)| {
                                    view! { <option value=id.clone()>{label}</option> }
                                }
                            />
                        </select>
                    </div>
                    <div class="d406-field">
                        <label>"Маркетплейс"</label>
                        <select
                            prop:value=move || marketplace_filter.get()
                            on:change=move |ev| marketplace_filter.set(event_target_value(&ev))
                        >
                            <option value="">"Все"</option>
                            <For
                                each=move || {
                                    let mut list: Vec<String> = data
                                        .with(|opt| {
                                            opt.as_ref()
                                                .map(|r| {
                                                    let mut set: Vec<String> = r
                                                        .rows
                                                        .iter()
                                                        .filter_map(|row| row.marketplace.clone())
                                                        .collect();
                                                    set.sort();
                                                    set.dedup();
                                                    set
                                                })
                                                .unwrap_or_default()
                                        });
                                    list.sort();
                                    list
                                }
                                key=|name| name.clone()
                                children=move |name| {
                                    let label = name.clone();
                                    view! { <option value=name>{label}</option> }
                                }
                            />
                        </select>
                    </div>
                    <div class="d406-field">
                        <label title="Канал трафика: вся воронка, только платный (реклама) или органический">
                            "Канал"
                        </label>
                        <select
                            on:change=move |ev| {
                                channel.set(match event_target_value(&ev).as_str() {
                                    "paid" => FunnelChannel::Paid,
                                    "free" => FunnelChannel::Free,
                                    _ => FunnelChannel::All,
                                });
                            }
                        >
                            <option value="all">"Все"</option>
                            <option value="paid">"Платные"</option>
                            <option value="free">"Бесплатные"</option>
                        </select>
                    </div>
                    <div class="d406-field">
                        <label>"nm_id"</label>
                        <input
                            placeholder="все"
                            prop:value=move || nm_id.get()
                            on:input=move |ev| nm_id.set(event_target_value(&ev))
                        />
                    </div>
                    <div class="d406-field">
                        <label title="Когорта — по дате заказа (винтаж); Событие — по дате транзакции события">
                            "Ось дат"
                        </label>
                        <select
                            on:change=move |ev| {
                                let v = event_target_value(&ev);
                                axis.set(if v == "event" { FunnelDateAxis::Event } else { FunnelDateAxis::Cohort });
                            }
                        >
                            <option value="cohort">"Когорта (дата заказа)"</option>
                            <option value="event">"Событие (дата транзакции)"</option>
                        </select>
                    </div>
                    <div class="d406-actions">
                        <button class="d406-btn d406-btn--primary" on:click=move |_| load() disabled=move || loading.get()>
                            {move || if loading.get() { "Загрузка..." } else { "Обновить" }}
                        </button>
                        <button class="d406-btn" on:click=do_export title="Выгрузить текущий срез в CSV (Excel)">
                            "Экспорт CSV"
                        </button>
                    </div>
                </div>

                <div class="d406-tabbar">
                    <TabList selected_value=active_tab>
                        <Tab value="table".to_string()>"Таблица"</Tab>
                        <Tab value="chart".to_string()>"Диаграмма"</Tab>
                    </TabList>
                </div>

                {move || error.get().map(|message| view! {
                    <div class="d406-state">{message}</div>
                })}

                {move || {
                    let mp = marketplace_filter.get();
                    let sf = sort_field.get();
                    let asc = sort_asc.get();
                    let ch = channel.get();
                    // Читаем ответ по ссылке (без клона всего набора на каждый ре-рендер);
                    // клонируем только отфильтрованные строки внутри view_rows.
                    let Some((rows, totals_raw)) = data.with(|opt| {
                        opt.as_ref().map(|response| {
                            let rows = view_rows(response, &mp, &sf, asc);
                            let totals = compute_totals(&rows);
                            (rows, totals)
                        })
                    }) else {
                        return view! { <div class="d406-state">"Загрузка данных..."</div> }.into_any();
                    };
                    // Проекция итогов на выбранный канал (Все/Платные/Бесплатные).
                    let (totals, totals_avail) = channel_metrics(&totals_raw, ch);
                    let totals_conv = WbSalesFunnelConversions::from_metrics_with_funnel_cancel(
                        totals.open_count, totals.cart_count, totals.order_count,
                        totals.buyout_count, totals.cancel_count,
                        totals.funnel_cancel_count, totals.funnel_order_count,
                    );

                    if active_tab.get().as_str() == "chart" {
                        let period = format!("{} — {}", date_from.get_untracked(), date_to.get_untracked());
                        let mp_label = if mp.is_empty() { "все маркетплейсы".to_string() } else { mp.clone() };
                        let ch_label = match ch {
                            FunnelChannel::All => "весь трафик",
                            FunnelChannel::Paid => "платный трафик",
                            FunnelChannel::Free => "органический трафик",
                        };
                        view! {
                            <div class="d406-chart-wrap">
                                <div class="d406-chart-head">
                                    <h2 class="d406-chart-title">"Воронка за период"</h2>
                                    <span class="d406-chart-sub">{format!("{period} · {mp_label} · {ch_label}")}</span>
                                </div>
                                {render_funnel(&totals)}
                            </div>
                        }.into_any()
                    } else {
                        // Пагинация: в DOM попадает только текущая страница строк.
                        let total_rows = rows.len();
                        let ps = page_size.get().max(1);
                        let page_count = ((total_rows + ps - 1) / ps).max(1);
                        let cur = page.get().min(page_count - 1);
                        let start = cur * ps;
                        let end = (start + ps).min(total_rows);
                        let range_label = if total_rows == 0 {
                            "Нет строк".to_string()
                        } else {
                            format!("Показано {}–{} из {}", start + 1, end, total_rows)
                        };
                        view! {
                            <div class="d406-table-wrap">
                                <table class="d406-table">
                                    <thead>
                                        <tr>
                                            {sort_th("Дата", "date", true, "Дата выбранной оси", sort_field, sort_asc)}
                                            {sort_th("Маркетплейс", "marketplace", true, "Маркетплейс кабинета", sort_field, sort_asc)}
                                            {sort_th("Артикул", "article", true, "Артикул товара (наименование — в подсказке)", sort_field, sort_asc)}
                                            {sort_th("Показы", "show_total", false, "Показы выбранного канала: Все — всего; Платные — реклама (a026); Бесплатные — органика (a040, N/A)", sort_field, sort_asc)}
                                            {sort_th("Переходы", "open", false, "Переходы в карточку (канал: платные — a026 clicks; бесплатные — всего − платные)", sort_field, sort_asc)}
                                            {sort_th("Корзина", "cart", false, "Добавления в корзину (канал: платные — a026 atbs; бесплатные — всего − платные)", sort_field, sort_asc)}
                                            {sort_th("Заказы", "order", false, "Заказы канала (платные — srid ∈ рекламы p913). Клик — список заказов.", sort_field, sort_asc)}
                                            {sort_th("Выкупы", "buyout", false, "Выкупы товара (a012), канал — по заказу", sort_field, sort_asc)}
                                            {sort_th("Отмены", "cancel", false, "Отмены заказов по документам (WB — a015, YM — a013), шт. Считаются на дату отмены конкретного заказа.", sort_field, sort_asc)}
                                            {sort_th("Отказы (воронка)", "funnel_cancel", false, "Отказы по дневному счётчику самого маркетплейса (WB — a036, YM — a041), шт. Полнее колонки «Отмены»; складывать их нельзя. Канального сплита нет → под фильтром каналов N/A.", sort_field, sort_asc)}
                                            {sort_th("Возвраты", "return", false, "Возвраты покупателя (WB — a012, YM — a016), шт.", sort_field, sort_asc)}
                                            {sort_th("Сумма заказов", "order_sum", false, "Сумма заказов, ₽", sort_field, sort_asc)}
                                            {sort_th("Переход→корзина", "conv_open_cart", false, "Корзина / переходы", sort_field, sort_asc)}
                                            {sort_th("Корзина→заказ", "conv_cart_order", false, "Заказы / корзина", sort_field, sort_asc)}
                                            {sort_th("Заказ→выкуп", "conv_order_buyout", false, "Выкупы / заказы", sort_field, sort_asc)}
                                            {sort_th("Доля отмен", "conv_cancel", false, "Отмены / заказы (обе метрики — из документов заказов)", sort_field, sort_asc)}
                                            {sort_th("Доля отказов (воронка)", "conv_funnel_cancel", false, "Отказы / заказы по счётчикам маркетплейса (собственный знаменатель — счётчик заказов воронки, а не документы)", sort_field, sort_asc)}
                                        </tr>
                                    </thead>
                                    <tbody>
                                        <tr class="d406-row d406-row--total">
                                            <td class="d406-date">"Итого за период"</td>
                                            <td></td>
                                            <td></td>
                                            {metric_cells(&totals, &totals_conv, totals_avail, None)}
                                        </tr>
                                        {if total_rows == 0 {
                                            view! {
                                                <tr><td class="d406-state" colspan="15">"Нет данных за выбранный период."</td></tr>
                                            }.into_any()
                                        } else {
                                            rows.into_iter().skip(start).take(end - start).map(|row| {
                                                let article = article_label(&row);
                                                let name = row.product_name.clone().unwrap_or_default();
                                                // Проекция строки на выбранный канал.
                                                let (dm, avail) = channel_metrics(&row.metrics, ch);
                                                let dc = WbSalesFunnelConversions::from_metrics_with_funnel_cancel(
                                                    dm.open_count, dm.cart_count, dm.order_count,
                                                    dm.buyout_count, dm.cancel_count,
                                                    dm.funnel_cancel_count, dm.funnel_order_count,
                                                );
                                                // Drilldown доступен только при известном nm_id.
                                                let drill = row.nm_id.map(|nm| {
                                                    (drilldown, DrillCtx {
                                                        connection_mp_ref: row.connection_mp_ref.clone(),
                                                        nm_id: nm,
                                                        date: row.date.clone(),
                                                        article: article.clone(),
                                                    })
                                                });
                                                view! {
                                                    <tr class="d406-row">
                                                        <td class="d406-date">{row.date.clone()}</td>
                                                        <td>{marketplace_badge(&row)}</td>
                                                        <td class="d406-name" title=name>{article}</td>
                                                        {metric_cells(&dm, &dc, avail, drill)}
                                                    </tr>
                                                }
                                            }).collect_view().into_any()
                                        }}
                                    </tbody>
                                </table>
                            </div>
                            <div class="d406-pager">
                                <span class="d406-pager-info">{range_label}</span>
                                <div class="d406-pager-ctrls">
                                    <button
                                        class="d406-btn"
                                        disabled=cur == 0
                                        on:click=move |_| page.set(cur.saturating_sub(1))
                                    >"‹ Назад"</button>
                                    <span class="d406-pager-page">{format!("Стр. {} из {}", cur + 1, page_count)}</span>
                                    <button
                                        class="d406-btn"
                                        disabled=cur + 1 >= page_count
                                        on:click=move |_| page.set(cur + 1)
                                    >"Вперёд ›"</button>
                                    <select
                                        class="d406-pager-size"
                                        title="Строк на странице"
                                        on:change=move |ev| {
                                            if let Ok(v) = event_target_value(&ev).parse::<usize>() {
                                                page_size.set(v);
                                                page.set(0);
                                            }
                                        }
                                    >
                                        <option value="100" selected=ps == 100>"100 / стр."</option>
                                        <option value="250" selected=ps == 250>"250 / стр."</option>
                                        <option value="500" selected=ps == 500>"500 / стр."</option>
                                        <option value="1000" selected=ps == 1000>"1000 / стр."</option>
                                    </select>
                                </div>
                            </div>
                        }.into_any()
                    }
                }}

                {move || drilldown.get().map(|ctx| {
                    let head = format!("Заказы · {} · {}", ctx.article, ctx.date);
                    view! {
                        <div class="d406-modal-overlay" on:click=move |_| drilldown.set(None)>
                            <div class="d406-modal" on:click=move |ev| ev.stop_propagation()>
                                <div class="d406-modal-head">
                                    <h3 class="d406-modal-title">{head}</h3>
                                    <button class="d406-btn" on:click=move |_| drilldown.set(None)>"✕"</button>
                                </div>
                                <div class="d406-modal-body">
                                    {move || {
                                        if drill_loading.get() {
                                            return view! { <div class="d406-state">"Загрузка..."</div> }.into_any();
                                        }
                                        if let Some(message) = drill_error.get() {
                                            return view! { <div class="d406-state">{message}</div> }.into_any();
                                        }
                                        let Some(resp) = drill_data.get() else {
                                            return view! { <div class="d406-state">"—"</div> }.into_any();
                                        };
                                        let summary = format!(
                                            "Платные: {} · Бесплатные: {} · Всего: {}",
                                            resp.paid_count, resp.free_count, resp.paid_count + resp.free_count,
                                        );
                                        view! {
                                            <div class="d406-drill-summary">{summary}</div>
                                            <table class="d406-table">
                                                <thead>
                                                    <tr>
                                                        <th class="d406-th--left">"srid"</th>
                                                        <th class="d406-th--left">"Дата"</th>
                                                        <th>"Сумма"</th>
                                                        <th class="d406-th--left">"Канал"</th>
                                                        <th class="d406-th--left">"Кампания"</th>
                                                        <th>"Отмена"</th>
                                                    </tr>
                                                </thead>
                                                <tbody>
                                                    {resp.items.into_iter().map(|it| {
                                                        let channel = if it.is_paid { "Платный" } else { "Бесплатный" };
                                                        let ch_cls = if it.is_paid { "d406-badge d406-badge--mp-wb" } else { "d406-badge d406-badge--muted" };
                                                        let campaign = it.advert_campaign.clone().unwrap_or_default();
                                                        let cancel = if it.is_cancel { "да" } else { "" };
                                                        view! {
                                                            <tr class="d406-row">
                                                                <td class="d406-name">{it.srid}</td>
                                                                <td class="d406-date">{it.order_date}</td>
                                                                <td class="d406-money">{fmt_money(it.amount)}</td>
                                                                <td><span class=ch_cls>{channel}</span></td>
                                                                <td class="d406-name">{campaign}</td>
                                                                <td class="d406-n">{cancel}</td>
                                                            </tr>
                                                        }
                                                    }).collect_view()}
                                                </tbody>
                                            </table>
                                        }.into_any()
                                    }}
                                </div>
                            </div>
                        </div>
                    }
                })}
            </div>
        </PageFrame>
    }
}
