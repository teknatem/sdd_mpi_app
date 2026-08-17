//! DateRangePickerV4 — компактная полоса + большой поповер выбора периода.
//!
//! На полосе только то, что нужно при просмотре: подпись периода и шаг ←/→.
//! Вся сложность (единица, пресеты, сетка, интервал двумя кликами) — в поповере
//! по пиктограмме «измерить». Контракт тот же: ISO `date_from`/`date_to` + `on_change`.

use crate::shared::icons::icon;
use chrono::{Datelike, Duration, NaiveDate, Utc, Weekday};
use leptos::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
enum RangeUnit {
    Year,
    #[default]
    Month,
    Week,
    Day,
}

impl RangeUnit {
    fn label(self) -> &'static str {
        match self {
            Self::Year => "Год",
            Self::Month => "Мес",
            Self::Week => "Нед",
            Self::Day => "День",
        }
    }

    const ALL: [Self; 4] = [Self::Year, Self::Month, Self::Week, Self::Day];
}

fn today() -> NaiveDate {
    Utc::now().date_naive()
}

fn iso(date: NaiveDate) -> String {
    date.format("%Y-%m-%d").to_string()
}

fn dmy(date: NaiveDate) -> String {
    date.format("%d.%m.%Y").to_string()
}

fn parse_iso(value: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(value, "%Y-%m-%d").ok()
}

fn last_day_of_month(year: i32, month: u32) -> NaiveDate {
    let (y, m) = if month == 12 {
        (year + 1, 1)
    } else {
        (year, month + 1)
    };
    NaiveDate::from_ymd_opt(y, m, 1)
        .map(|d| d - Duration::days(1))
        .expect("valid date")
}

fn unit_start(date: NaiveDate, unit: RangeUnit) -> NaiveDate {
    match unit {
        RangeUnit::Year => NaiveDate::from_ymd_opt(date.year(), 1, 1).expect("valid"),
        RangeUnit::Month => NaiveDate::from_ymd_opt(date.year(), date.month(), 1).expect("valid"),
        RangeUnit::Week => date - Duration::days(date.weekday().num_days_from_monday() as i64),
        RangeUnit::Day => date,
    }
}

fn unit_end(date: NaiveDate, unit: RangeUnit) -> NaiveDate {
    match unit {
        RangeUnit::Year => NaiveDate::from_ymd_opt(date.year(), 12, 31).expect("valid"),
        RangeUnit::Month => last_day_of_month(date.year(), date.month()),
        RangeUnit::Week => unit_start(date, RangeUnit::Week) + Duration::days(6),
        RangeUnit::Day => date,
    }
}

fn snap_range(from: NaiveDate, to: NaiveDate, unit: RangeUnit) -> (NaiveDate, NaiveDate) {
    let a = from.min(to);
    let b = from.max(to);
    (unit_start(a, unit), unit_end(b, unit))
}

fn unit_count(from: NaiveDate, to: NaiveDate, unit: RangeUnit) -> i32 {
    let (a, b) = snap_range(from, to, unit);
    match unit {
        RangeUnit::Year => b.year() - a.year() + 1,
        RangeUnit::Month => (b.year() - a.year()) * 12 + (b.month() as i32 - a.month() as i32) + 1,
        RangeUnit::Week => (((b - a).num_days() + 1 + 6) / 7) as i32,
        RangeUnit::Day => ((b - a).num_days() + 1) as i32,
    }
}

fn shift_by_units(date: NaiveDate, unit: RangeUnit, delta: i32) -> NaiveDate {
    match unit {
        RangeUnit::Year => {
            let year = date.year() + delta;
            let max_day = last_day_of_month(year, date.month()).day();
            NaiveDate::from_ymd_opt(year, date.month(), date.day().min(max_day)).unwrap_or(date)
        }
        RangeUnit::Month => {
            let total = date.year() * 12 + (date.month() as i32 - 1) + delta;
            let y = total.div_euclid(12);
            let m = (total.rem_euclid(12) + 1) as u32;
            let max_day = last_day_of_month(y, m).day();
            NaiveDate::from_ymd_opt(y, m, date.day().min(max_day)).unwrap_or(date)
        }
        RangeUnit::Week => date + Duration::weeks(delta as i64),
        RangeUnit::Day => date + Duration::days(delta as i64),
    }
}

fn is_aligned(from: NaiveDate, to: NaiveDate, unit: RangeUnit) -> bool {
    unit_start(from, unit) == from && unit_end(to, unit) == to
}

fn detect_unit(from: NaiveDate, to: NaiveDate) -> RangeUnit {
    RangeUnit::ALL
        .into_iter()
        .find(|unit| is_aligned(from, to, *unit) && unit_count(from, to, *unit) == 1)
        .or_else(|| {
            RangeUnit::ALL
                .into_iter()
                .find(|unit| is_aligned(from, to, *unit))
        })
        .unwrap_or(RangeUnit::Day)
}

fn shift_range(
    from: NaiveDate,
    to: NaiveDate,
    unit: RangeUnit,
    direction: i32,
) -> (NaiveDate, NaiveDate) {
    let a = from.min(to);
    let b = from.max(to);
    let n = unit_count(a, b, unit).max(1);
    if is_aligned(a, b, unit) {
        let new_a = shift_by_units(a, unit, direction * n);
        let new_b = unit_end(shift_by_units(new_a, unit, n - 1), unit);
        (unit_start(new_a, unit), new_b)
    } else {
        (
            shift_by_units(a, unit, direction * n),
            shift_by_units(b, unit, direction * n),
        )
    }
}

fn current_range(unit: RangeUnit, span: i32) -> (NaiveDate, NaiveDate) {
    let n = span.max(1);
    let end_unit = unit_start(today(), unit);
    let start = shift_by_units(end_unit, unit, -(n - 1));
    (unit_start(start, unit), unit_end(end_unit, unit))
}

fn month_name(month: u32) -> &'static str {
    match month {
        1 => "Январь",
        2 => "Февраль",
        3 => "Март",
        4 => "Апрель",
        5 => "Май",
        6 => "Июнь",
        7 => "Июль",
        8 => "Август",
        9 => "Сентябрь",
        10 => "Октябрь",
        11 => "Ноябрь",
        12 => "Декабрь",
        _ => "",
    }
}

fn month_name_short(month: u32) -> &'static str {
    match month {
        1 => "Янв",
        2 => "Фев",
        3 => "Мар",
        4 => "Апр",
        5 => "Май",
        6 => "Июн",
        7 => "Июл",
        8 => "Авг",
        9 => "Сен",
        10 => "Окт",
        11 => "Ноя",
        12 => "Дек",
        _ => "",
    }
}

fn weekday_short(weekday: Weekday) -> &'static str {
    match weekday {
        Weekday::Mon => "Пн",
        Weekday::Tue => "Вт",
        Weekday::Wed => "Ср",
        Weekday::Thu => "Чт",
        Weekday::Fri => "Пт",
        Weekday::Sat => "Сб",
        Weekday::Sun => "Вс",
    }
}

/// Подпись на полосе: читается с одного взгляда.
fn bar_label(from: NaiveDate, to: NaiveDate) -> String {
    let a = from.min(to);
    let b = from.max(to);
    let unit = detect_unit(a, b);
    let n = unit_count(a, b, unit);

    match unit {
        RangeUnit::Year if n == 1 => a.year().to_string(),
        RangeUnit::Year => format!("{}–{}", a.year(), b.year()),
        RangeUnit::Month if n == 1 => format!("{} {}", month_name(a.month()), a.year()),
        RangeUnit::Month if a.year() == b.year() => format!(
            "{} – {} {}",
            month_name_short(a.month()),
            month_name_short(b.month()),
            a.year()
        ),
        RangeUnit::Month => format!(
            "{} {} – {} {}",
            month_name_short(a.month()),
            a.year(),
            month_name_short(b.month()),
            b.year()
        ),
        RangeUnit::Week if n == 1 => {
            format!("{}–{} {}", a.format("%d.%m"), b.format("%d.%m"), a.year())
        }
        RangeUnit::Week => format!("{} нед", n),
        RangeUnit::Day if n == 1 => a.format("%d.%m.%Y").to_string(),
        RangeUnit::Day if a.year() == b.year() => {
            format!("{} – {}", a.format("%d.%m"), b.format("%d.%m.%Y"))
        }
        RangeUnit::Day => format!("{} – {}", dmy(a), dmy(b)),
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Preset {
    Last(i32),
    Prev,
    Ytd,
}

fn preset_range(preset: Preset, unit: RangeUnit) -> (NaiveDate, NaiveDate) {
    let now = today();
    match preset {
        Preset::Last(n) => current_range(unit, n),
        Preset::Prev => {
            let (a, b) = current_range(unit, 1);
            shift_range(a, b, unit, -1)
        }
        Preset::Ytd => snap_range(
            NaiveDate::from_ymd_opt(now.year(), 1, 1).expect("valid"),
            now,
            unit,
        ),
    }
}

fn presets_for(unit: RangeUnit) -> Vec<(&'static str, Preset)> {
    match unit {
        RangeUnit::Year => vec![
            ("Этот год", Preset::Last(1)),
            ("Прошлый", Preset::Prev),
            ("3 года", Preset::Last(3)),
        ],
        RangeUnit::Month => vec![
            ("Этот месяц", Preset::Last(1)),
            ("Прошлый", Preset::Prev),
            ("3 мес", Preset::Last(3)),
            ("6 мес", Preset::Last(6)),
            ("С начала года", Preset::Ytd),
        ],
        RangeUnit::Week => vec![
            ("Эта неделя", Preset::Last(1)),
            ("Прошлая", Preset::Prev),
            ("4 нед", Preset::Last(4)),
            ("13 нед", Preset::Last(13)),
        ],
        RangeUnit::Day => vec![
            ("Сегодня", Preset::Last(1)),
            ("Вчера", Preset::Prev),
            ("7 дн", Preset::Last(7)),
            ("30 дн", Preset::Last(30)),
        ],
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Cell {
    Year(i32),
    Month { year: i32, month: u32 },
    Week(NaiveDate),
    Day(NaiveDate),
}

impl Cell {
    fn range(self) -> (NaiveDate, NaiveDate) {
        match self {
            Self::Year(y) => (
                NaiveDate::from_ymd_opt(y, 1, 1).expect("valid"),
                NaiveDate::from_ymd_opt(y, 12, 31).expect("valid"),
            ),
            Self::Month { year, month } => (
                NaiveDate::from_ymd_opt(year, month, 1).expect("valid"),
                last_day_of_month(year, month),
            ),
            Self::Week(monday) => (monday, monday + Duration::days(6)),
            Self::Day(d) => (d, d),
        }
    }
}

fn cell_class(cell: Cell, selection: Option<(NaiveDate, NaiveDate)>) -> String {
    let mut class = String::from("date-range-picker__cell");
    let (cs, ce) = cell.range();
    if let Some((from, to)) = selection {
        let a = from.min(to);
        let b = from.max(to);
        if cs <= b && ce >= a {
            let starts = cs <= a && a <= ce;
            let ends = cs <= b && b <= ce;
            if starts {
                class.push_str(" date-range-picker__cell--start");
            }
            if ends {
                class.push_str(" date-range-picker__cell--end");
            }
            if !starts && !ends {
                class.push_str(" date-range-picker__cell--in-range");
            }
        }
    }
    let now = today();
    if cs <= now && now <= ce {
        class.push_str(" date-range-picker__cell--today");
    }
    class
}

/// DateRangePickerV4 — просмотр периода на полосе, выбор в большом поповере.
#[component]
pub fn DateRangePickerV4(
    #[prop(into)] date_from: Signal<String>,
    #[prop(into)] date_to: Signal<String>,
    on_change: Callback<(String, String)>,
    #[prop(optional)] label: Option<String>,
) -> impl IntoView {
    let unit = RwSignal::new(RangeUnit::Month);
    let open = RwSignal::new(false);
    let anchor = RwSignal::new(today());
    let sel = RwSignal::new(Option::<(NaiveDate, NaiveDate)>::None);
    let hover = RwSignal::new(Option::<(NaiveDate, NaiveDate)>::None);
    let awaiting_end = RwSignal::new(false);

    let pair = move || -> Option<(NaiveDate, NaiveDate)> {
        Some((
            parse_iso(&date_from.get_untracked())?,
            parse_iso(&date_to.get_untracked())?,
        ))
    };

    Effect::new(move |_| {
        if date_from.get().is_empty() && date_to.get().is_empty() {
            let (a, b) = current_range(RangeUnit::Month, 1);
            on_change.run((iso(a), iso(b)));
        }
    });

    let emit = {
        let on_change = on_change.clone();
        move |from: NaiveDate, to: NaiveDate, u: RangeUnit| {
            let (a, b) = snap_range(from, to, u);
            on_change.run((iso(a), iso(b)));
        }
    };

    let step = {
        let emit = emit.clone();
        move |direction: i32| {
            if let Some((a, b)) = pair() {
                let u = detect_unit(a, b);
                unit.set(u);
                let (na, nb) = shift_range(a, b, u, direction);
                emit(na, nb, u);
            }
        }
    };

    let open_popover = move |_| {
        if let Some((a, b)) = pair() {
            let u = detect_unit(a, b);
            unit.set(u);
            sel.set(Some((a.min(b), a.max(b))));
            anchor.set(b);
        } else {
            sel.set(None);
            anchor.set(today());
        }
        hover.set(None);
        awaiting_end.set(false);
        open.set(true);
    };

    let close_popover = move || {
        awaiting_end.set(false);
        hover.set(None);
        open.set(false);
    };

    let apply_and_close = {
        let emit = emit.clone();
        let close_popover = close_popover.clone();
        move |a: NaiveDate, b: NaiveDate| {
            let u = unit.get_untracked();
            emit(a, b, u);
            close_popover();
        }
    };

    let on_cell_click = {
        let apply_and_close = apply_and_close.clone();
        move |cell: Cell| {
            let (cs, ce) = cell.range();
            if awaiting_end.get_untracked() {
                let (s0, s1) = sel.get_untracked().unwrap_or((cs, ce));
                let a = s0.min(s1).min(cs);
                let b = s0.max(s1).max(ce);
                sel.set(Some((a, b)));
                apply_and_close(a, b);
            } else {
                sel.set(Some((cs, ce)));
                awaiting_end.set(true);
                hover.set(None);
            }
        }
    };

    let highlight = move || -> Option<(NaiveDate, NaiveDate)> {
        let selected = sel.get()?;
        match hover.get() {
            Some((ha, hb)) if awaiting_end.get() => Some((
                selected.0.min(selected.1).min(ha),
                selected.0.max(selected.1).max(hb),
            )),
            _ => Some(selected),
        }
    };

    let apply_ok = {
        let apply_and_close = apply_and_close.clone();
        let close_popover = close_popover.clone();
        move |_| {
            if let Some((a, b)) = sel.get_untracked() {
                apply_and_close(a, b);
            } else {
                close_popover();
            }
        }
    };

    view! {
        <div class="date-range-picker date-range-picker--v4">
            {label.map(|text| {
                view! { <span class="date-range-picker__bar-label">{text}</span> }
            })}

            <div class="date-range-picker__bar">
                <button
                    type="button"
                    class="date-range-picker__step"
                    title="Предыдущий период"
                    on:click=move |_| step(-1)
                >
                    <svg width="10" height="12" viewBox="0 0 10 12" fill="none" aria-hidden="true">
                        <path d="M7 1L2 6l5 5" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>
                    </svg>
                </button>

                <span class="date-range-picker__period">
                    {move || {
                        match (parse_iso(&date_from.get()), parse_iso(&date_to.get())) {
                            (Some(a), Some(b)) => bar_label(a, b),
                            _ => "Период".to_string(),
                        }
                    }}
                </span>

                <button
                    type="button"
                    class="date-range-picker__step"
                    title="Следующий период"
                    on:click=move |_| step(1)
                >
                    <svg width="10" height="12" viewBox="0 0 10 12" fill="none" aria-hidden="true">
                        <path d="M3 1l5 5-5 5" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>
                    </svg>
                </button>

                <button
                    type="button"
                    class="date-range-picker__measure"
                    title="Измерить период"
                    aria-label="Измерить период"
                    aria-expanded=move || open.get()
                    on:click=open_popover
                >
                    {icon("ruler")}
                </button>
            </div>
        </div>

        <Show when=move || open.get()>
            <div class="date-range-picker__popover">
                <button
                    type="button"
                    class="date-range-picker__popover-backdrop"
                    aria-label="Закрыть"
                    on:click=move |_| close_popover()
                />
                <div
                    class="date-range-picker__popover-panel"
                    role="dialog"
                    aria-label="Выбор периода"
                    on:click=move |ev| ev.stop_propagation()
                >
                    <div class="date-range-picker__mode" role="group" aria-label="Единица периода">
                        {RangeUnit::ALL
                            .into_iter()
                            .map(|u| {
                                view! {
                                    <button
                                        type="button"
                                        class=move || {
                                            if unit.get() == u {
                                                "date-range-picker__mode-btn date-range-picker__mode-btn--active"
                                            } else {
                                                "date-range-picker__mode-btn"
                                            }
                                        }
                                        on:click=move |_| {
                                            unit.set(u);
                                            if let Some((a, b)) = sel.get_untracked() {
                                                sel.set(Some(snap_range(a, b, u)));
                                            }
                                            awaiting_end.set(false);
                                            hover.set(None);
                                        }
                                    >
                                        {u.label()}
                                    </button>
                                }
                            })
                            .collect_view()}
                    </div>

                    <div class="date-range-picker__presets">
                        {move || {
                            let u = unit.get();
                            presets_for(u)
                                .into_iter()
                                .map(|(text, preset)| {
                                    let apply_and_close = apply_and_close.clone();
                                    view! {
                                        <button
                                            type="button"
                                            class="date-range-picker__preset"
                                            on:click=move |_| {
                                                let (a, b) = preset_range(preset, u);
                                                apply_and_close(a, b);
                                            }
                                        >
                                            {text}
                                        </button>
                                    }
                                })
                                .collect_view()
                        }}
                    </div>

                    {move || {
                        let u = unit.get();
                        let view_anchor = anchor.get();
                        let selection = highlight();

                        match u {
                            RangeUnit::Year => {
                                let center = view_anchor.year();
                                let first = center - 5;
                                let years: Vec<i32> = (first..=first + 11).collect();
                                view! {
                                    <div class="date-range-picker__dialog-nav">
                                        <button
                                            type="button"
                                            class="date-range-picker__step"
                                            on:click=move |_| {
                                                anchor.update(|d| *d = shift_by_units(*d, RangeUnit::Year, -12));
                                            }
                                        >
                                            <svg width="10" height="12" viewBox="0 0 10 12" fill="none">
                                                <path d="M7 1L2 6l5 5" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>
                                            </svg>
                                        </button>
                                        <span class="date-range-picker__dialog-title">
                                            {format!("{} – {}", first, first + 11)}
                                        </span>
                                        <button
                                            type="button"
                                            class="date-range-picker__step"
                                            on:click=move |_| {
                                                anchor.update(|d| *d = shift_by_units(*d, RangeUnit::Year, 12));
                                            }
                                        >
                                            <svg width="10" height="12" viewBox="0 0 10 12" fill="none">
                                                <path d="M3 1l5 5-5 5" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>
                                            </svg>
                                        </button>
                                    </div>
                                    <div class="date-range-picker__grid date-range-picker__grid--years">
                                        {years
                                            .into_iter()
                                            .map(|y| {
                                                let cell = Cell::Year(y);
                                                view! {
                                                    <button
                                                        type="button"
                                                        class=cell_class(cell, selection)
                                                        on:mouseenter=move |_| {
                                                            if awaiting_end.get_untracked() {
                                                                hover.set(Some(cell.range()));
                                                            }
                                                        }
                                                        on:click=move |_| on_cell_click(cell)
                                                    >
                                                        {y.to_string()}
                                                    </button>
                                                }
                                            })
                                            .collect_view()}
                                    </div>
                                }
                                    .into_any()
                            }
                            RangeUnit::Month => {
                                let year = view_anchor.year();
                                view! {
                                    <div class="date-range-picker__dialog-nav">
                                        <button
                                            type="button"
                                            class="date-range-picker__step"
                                            on:click=move |_| {
                                                anchor.update(|d| *d = shift_by_units(*d, RangeUnit::Year, -1));
                                            }
                                        >
                                            <svg width="10" height="12" viewBox="0 0 10 12" fill="none">
                                                <path d="M7 1L2 6l5 5" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>
                                            </svg>
                                        </button>
                                        <span class="date-range-picker__dialog-title">{year.to_string()}</span>
                                        <button
                                            type="button"
                                            class="date-range-picker__step"
                                            on:click=move |_| {
                                                anchor.update(|d| *d = shift_by_units(*d, RangeUnit::Year, 1));
                                            }
                                        >
                                            <svg width="10" height="12" viewBox="0 0 10 12" fill="none">
                                                <path d="M3 1l5 5-5 5" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>
                                            </svg>
                                        </button>
                                    </div>
                                    <div class="date-range-picker__grid date-range-picker__grid--months">
                                        {(1u32..=12)
                                            .map(|month| {
                                                let cell = Cell::Month { year, month };
                                                view! {
                                                    <button
                                                        type="button"
                                                        class=cell_class(cell, selection)
                                                        on:mouseenter=move |_| {
                                                            if awaiting_end.get_untracked() {
                                                                hover.set(Some(cell.range()));
                                                            }
                                                        }
                                                        on:click=move |_| on_cell_click(cell)
                                                    >
                                                        {month_name(month)}
                                                    </button>
                                                }
                                            })
                                            .collect_view()}
                                    </div>
                                }
                                    .into_any()
                            }
                            RangeUnit::Week | RangeUnit::Day => {
                                let year = view_anchor.year();
                                let month = view_anchor.month();
                                let first = NaiveDate::from_ymd_opt(year, month, 1).expect("valid");
                                let pad = first.weekday().num_days_from_monday() as i64;
                                let grid_start = first - Duration::days(pad);
                                let days: Vec<NaiveDate> = (0..42)
                                    .map(|i| grid_start + Duration::days(i))
                                    .collect();
                                view! {
                                    <div class="date-range-picker__dialog-nav">
                                        <button
                                            type="button"
                                            class="date-range-picker__step"
                                            on:click=move |_| {
                                                anchor.update(|d| *d = shift_by_units(*d, RangeUnit::Month, -1));
                                            }
                                        >
                                            <svg width="10" height="12" viewBox="0 0 10 12" fill="none">
                                                <path d="M7 1L2 6l5 5" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>
                                            </svg>
                                        </button>
                                        <span class="date-range-picker__dialog-title">
                                            {format!("{} {}", month_name(month), year)}
                                        </span>
                                        <button
                                            type="button"
                                            class="date-range-picker__step"
                                            on:click=move |_| {
                                                anchor.update(|d| *d = shift_by_units(*d, RangeUnit::Month, 1));
                                            }
                                        >
                                            <svg width="10" height="12" viewBox="0 0 10 12" fill="none">
                                                <path d="M3 1l5 5-5 5" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>
                                            </svg>
                                        </button>
                                    </div>
                                    <div class="date-range-picker__weekday-row">
                                        {[Weekday::Mon, Weekday::Tue, Weekday::Wed, Weekday::Thu, Weekday::Fri, Weekday::Sat, Weekday::Sun]
                                            .into_iter()
                                            .map(|wd| view! {
                                                <span class="date-range-picker__weekday">{weekday_short(wd)}</span>
                                            })
                                            .collect_view()}
                                    </div>
                                    <div class="date-range-picker__grid date-range-picker__grid--days">
                                        {if u == RangeUnit::Week {
                                            days.chunks(7)
                                                .map(|week| {
                                                    let monday = week[0];
                                                    let cell = Cell::Week(monday);
                                                    let week_days: Vec<_> = week.to_vec();
                                                    view! {
                                                        <button
                                                            type="button"
                                                            class=format!(
                                                                "{} date-range-picker__cell--week-row",
                                                                cell_class(cell, selection)
                                                            )
                                                            on:mouseenter=move |_| {
                                                                if awaiting_end.get_untracked() {
                                                                    hover.set(Some(cell.range()));
                                                                }
                                                            }
                                                            on:click=move |_| on_cell_click(cell)
                                                        >
                                                            {week_days
                                                                .into_iter()
                                                                .map(|d| {
                                                                    let muted = d.month() != month;
                                                                    view! {
                                                                        <span class=if muted {
                                                                            "date-range-picker__day date-range-picker__day--muted"
                                                                        } else {
                                                                            "date-range-picker__day"
                                                                        }>
                                                                            {d.day().to_string()}
                                                                        </span>
                                                                    }
                                                                })
                                                                .collect_view()}
                                                        </button>
                                                    }
                                                })
                                                .collect_view()
                                                .into_any()
                                        } else {
                                            days.into_iter()
                                                .map(|d| {
                                                    let cell = Cell::Day(d);
                                                    let muted = d.month() != month;
                                                    let cls = if muted {
                                                        format!("{} date-range-picker__cell--muted", cell_class(cell, selection))
                                                    } else {
                                                        cell_class(cell, selection)
                                                    };
                                                    view! {
                                                        <button
                                                            type="button"
                                                            class=cls
                                                            on:mouseenter=move |_| {
                                                                if awaiting_end.get_untracked() {
                                                                    hover.set(Some(cell.range()));
                                                                }
                                                            }
                                                            on:click=move |_| on_cell_click(cell)
                                                        >
                                                            {d.day().to_string()}
                                                        </button>
                                                    }
                                                })
                                                .collect_view()
                                                .into_any()
                                        }}
                                    </div>
                                }
                                    .into_any()
                            }
                        }
                    }}

                    <div class="date-range-picker__popover-foot">
                        <p class="date-range-picker__hint">
                            {move || {
                                match sel.get() {
                                    Some((a, b)) if awaiting_end.get() => {
                                        format!("{} · кликните конец интервала или OK", dmy(a.min(b)))
                                    }
                                    Some((a, b)) => format!("{}  {}", dmy(a.min(b)), dmy(a.max(b))),
                                    None => "Кликните начало периода".to_string(),
                                }
                            }}
                        </p>
                        <div class="date-range-picker__popover-actions">
                            <button
                                type="button"
                                class="date-range-picker__action date-range-picker__action--primary"
                                on:click=apply_ok
                            >
                                "OK"
                            </button>
                            <button
                                type="button"
                                class="date-range-picker__action"
                                on:click=move |_| close_popover()
                            >
                                "Отмена"
                            </button>
                        </div>
                    </div>
                </div>
            </div>
        </Show>
    }
}
