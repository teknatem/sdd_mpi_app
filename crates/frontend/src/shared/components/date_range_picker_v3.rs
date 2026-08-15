//! DateRangePickerV3 — экспериментальный выбор периода (поколение 3).
//!
//! Развитие предыдущих экспериментов с упором на четыре вещи:
//! 1. **Ввод текстом без масок.** Пока пользователь печатает, поле не переписывается —
//!    именно перезапись каретки давала «перескакивание». Разбор — на коммите
//!    (Enter / Tab / уход фокуса / 8-я цифра), свободным парсером: `01022026`,
//!    `1.2.26`, `0102` (год подставляется), `5` (день текущего месяца периода).
//! 2. **Быстрый ввод.** После полной первой даты фокус сам переходит ко второй;
//!    вставка двух дат в любое поле сразу применяет весь диапазон.
//! 3. **Интервалы.** В попапе — пресеты («3 мес», «13 нед», «С начала года») и выбор
//!    диапазона двумя кликами с подсветкой при наведении.
//! 4. **Никаких нативных `input[type=date]`** — значит, нет чёрной иконки календаря
//!    в тёмной теме. Попап открывается чипом с человекочитаемым периодом.
//!
//! Контракт тот же, что у остальных пикеров: `date_from`/`date_to` в ISO
//! (`yyyy-mm-dd`), наружу — `on_change((from, to))`.

use chrono::{Datelike, Duration, NaiveDate, Utc, Weekday};
use leptos::prelude::*;
use thaw::*;
use web_sys::HtmlInputElement;

// ── Режимы ────────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum RangeUnit {
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

    fn title(self) -> &'static str {
        match self {
            Self::Year => "Год",
            Self::Month => "Месяц",
            Self::Week => "Неделя",
            Self::Day => "День",
        }
    }

    fn short_label(self) -> &'static str {
        match self {
            Self::Year => "Г",
            Self::Month => "М",
            Self::Week => "Н",
            Self::Day => "Д",
        }
    }

    const ALL: [Self; 4] = [Self::Year, Self::Month, Self::Week, Self::Day];
}

// ── Даты: формат и разбор ─────────────────────────────────────────────────────

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

fn normalize_year(year: i32) -> i32 {
    if year < 100 {
        2000 + year
    } else {
        year
    }
}

/// Свободный разбор пользовательского ввода.
///
/// Понимает `01022026`, `01.02.2026`, `1.2.26`, `1/2`, `0102`, `102`, `5`.
/// Всё, что не задано, берётся из `anchor` — так работает «год подставляется сам».
fn parse_loose(raw: &str, anchor: NaiveDate) -> Option<NaiveDate> {
    let groups: Vec<&str> = raw
        .split(|c: char| !c.is_ascii_digit())
        .filter(|s| !s.is_empty())
        .collect();

    let (day, month, year): (u32, u32, i32) = match groups.len() {
        0 => return None,
        1 => {
            let d = groups[0];
            match d.len() {
                1 | 2 => (d.parse().ok()?, anchor.month(), anchor.year()),
                3 => (d[0..1].parse().ok()?, d[1..3].parse().ok()?, anchor.year()),
                4 => (d[0..2].parse().ok()?, d[2..4].parse().ok()?, anchor.year()),
                5 => (
                    d[0..1].parse().ok()?,
                    d[1..3].parse().ok()?,
                    normalize_year(d[3..5].parse().ok()?),
                ),
                6 => (
                    d[0..2].parse().ok()?,
                    d[2..4].parse().ok()?,
                    normalize_year(d[4..6].parse().ok()?),
                ),
                7 => (
                    d[0..1].parse().ok()?,
                    d[1..3].parse().ok()?,
                    d[3..7].parse().ok()?,
                ),
                8 => (
                    d[0..2].parse().ok()?,
                    d[2..4].parse().ok()?,
                    d[4..8].parse().ok()?,
                ),
                _ => return None,
            }
        }
        2 => (
            groups[0].parse().ok()?,
            groups[1].parse().ok()?,
            anchor.year(),
        ),
        _ => (
            groups[0].parse().ok()?,
            groups[1].parse().ok()?,
            normalize_year(groups[2].parse().ok()?),
        ),
    };

    NaiveDate::from_ymd_opt(year, month, day)
}

fn digit_count(raw: &str) -> usize {
    raw.chars().filter(|c| c.is_ascii_digit()).count()
}

/// Две полные даты можно вставить одной строкой в любом привычном оформлении:
/// `01022026 28022026`, `01.02.2026 — 28.02.2026` и т. п.
fn parse_pair_input(raw: &str) -> Option<(NaiveDate, NaiveDate)> {
    let digits: String = raw.chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.len() != 16 {
        return None;
    }

    let anchor = today();
    let from = parse_loose(&digits[..8], anchor)?;
    let to = parse_loose(&digits[8..], from)?;
    Some((from.min(to), from.max(to)))
}

// ── Даты: единицы периода ─────────────────────────────────────────────────────

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

/// Определить режим по диапазону: ровно год / месяц / неделя / день.
fn detect_unit(from: NaiveDate, to: NaiveDate) -> Option<RangeUnit> {
    RangeUnit::ALL
        .into_iter()
        .find(|unit| is_aligned(from, to, *unit))
}

/// Сдвиг интервала на его длину.
///
/// Выровненный по единицам диапазон сдвигается «по единицам» (Февраль → Март),
/// произвольный — сохраняет свои края (05.02–10.02 → 05.01–10.01).
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

/// Текущий период длиной `span` единиц, заканчивающийся сегодняшней единицей.
fn current_range(unit: RangeUnit, span: i32) -> (NaiveDate, NaiveDate) {
    let n = span.max(1);
    let end_unit = unit_start(today(), unit);
    let start = shift_by_units(end_unit, unit, -(n - 1));
    (unit_start(start, unit), unit_end(end_unit, unit))
}

// ── Подписи ───────────────────────────────────────────────────────────────────

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

/// Человекочитаемая подпись периода для чипа: «Фев 2026», «Фев–Апр 2026», «13 нед».
fn range_label(from: NaiveDate, to: NaiveDate, unit: RangeUnit) -> String {
    let a = from.min(to);
    let b = from.max(to);
    let n = unit_count(a, b, unit);

    match unit {
        RangeUnit::Year => {
            if n <= 1 {
                a.year().to_string()
            } else {
                format!("{}–{}", a.year(), b.year())
            }
        }
        RangeUnit::Month => {
            if n <= 1 {
                format!("{} {}", month_name_short(a.month()), a.year())
            } else if a.year() == b.year() {
                format!(
                    "{}–{} {}",
                    month_name_short(a.month()),
                    month_name_short(b.month()),
                    a.year()
                )
            } else {
                format!(
                    "{} {} – {} {}",
                    month_name_short(a.month()),
                    a.year(),
                    month_name_short(b.month()),
                    b.year()
                )
            }
        }
        RangeUnit::Week => {
            if n <= 1 {
                format!("Нед {:02} · {}", a.iso_week().week(), a.year())
            } else {
                format!("{n} нед")
            }
        }
        RangeUnit::Day => {
            if n <= 1 {
                format!("{} {}", weekday_short(a.weekday()), a.format("%d.%m"))
            } else {
                format!("{n} дн")
            }
        }
    }
}

// ── Пресеты ───────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq)]
enum Preset {
    /// Последние N единиц, включая текущую.
    Last(i32),
    /// Предыдущая единица.
    Prev,
    /// С начала года по сегодня.
    Ytd,
    /// С начала месяца по сегодня.
    Mtd,
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
        Preset::Mtd => snap_range(
            NaiveDate::from_ymd_opt(now.year(), now.month(), 1).expect("valid"),
            now,
            unit,
        ),
    }
}

fn presets_for(unit: RangeUnit) -> Vec<(&'static str, Preset)> {
    match unit {
        RangeUnit::Year => vec![
            ("Текущий", Preset::Last(1)),
            ("Прошлый", Preset::Prev),
            ("3 года", Preset::Last(3)),
            ("5 лет", Preset::Last(5)),
        ],
        RangeUnit::Month => vec![
            ("Текущий", Preset::Last(1)),
            ("Прошлый", Preset::Prev),
            ("3 мес", Preset::Last(3)),
            ("6 мес", Preset::Last(6)),
            ("12 мес", Preset::Last(12)),
            ("С начала года", Preset::Ytd),
        ],
        RangeUnit::Week => vec![
            ("Текущая", Preset::Last(1)),
            ("Прошлая", Preset::Prev),
            ("4 нед", Preset::Last(4)),
            ("13 нед", Preset::Last(13)),
            ("С начала месяца", Preset::Mtd),
        ],
        RangeUnit::Day => vec![
            ("Сегодня", Preset::Last(1)),
            ("Вчера", Preset::Prev),
            ("7 дн", Preset::Last(7)),
            ("30 дн", Preset::Last(30)),
            ("С начала месяца", Preset::Mtd),
        ],
    }
}

// ── Ячейки календаря ──────────────────────────────────────────────────────────

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

    fn contains_today(self) -> bool {
        let (a, b) = self.range();
        let now = today();
        a <= now && now <= b
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

    if cell.contains_today() {
        class.push_str(" date-range-picker__cell--today");
    }
    class
}

// ── Компонент ─────────────────────────────────────────────────────────────────

/// DateRangePickerV3 — выбор периода: 4 режима, свободный ввод текстом, интервалы.
#[component]
pub fn DateRangePickerV3(
    /// Дата «от» в формате yyyy-mm-dd
    #[prop(into)]
    date_from: Signal<String>,

    /// Дата «до» в формате yyyy-mm-dd
    #[prop(into)]
    date_to: Signal<String>,

    /// Callback при изменении диапазона (from, to) в формате yyyy-mm-dd
    on_change: Callback<(String, String)>,

    /// Опциональная метка
    #[prop(optional)]
    label: Option<String>,
) -> impl IntoView {
    let unit = RwSignal::new(RangeUnit::Month);

    let from_ref = NodeRef::<leptos::html::Input>::new();
    let to_ref = NodeRef::<leptos::html::Input>::new();
    let editing_from = RwSignal::new(false);
    let editing_to = RwSignal::new(false);

    let popup_open = RwSignal::new(false);
    let popup_anchor = RwSignal::new(today());
    let popup_sel = RwSignal::new(Option::<(NaiveDate, NaiveDate)>::None);
    let popup_hover = RwSignal::new(Option::<(NaiveDate, NaiveDate)>::None);
    let awaiting_end = RwSignal::new(false);

    let pair = move || -> Option<(NaiveDate, NaiveDate)> {
        Some((
            parse_iso(&date_from.get_untracked())?,
            parse_iso(&date_to.get_untracked())?,
        ))
    };

    // Пустой период на монтировании → текущий месяц.
    Effect::new(move |_| {
        if date_from.get().is_empty() && date_to.get().is_empty() {
            let (a, b) = current_range(RangeUnit::Month, 1);
            on_change.run((iso(a), iso(b)));
        }
    });

    // Внешнее значение → в поля. Поле, которое сейчас редактируют, не трогаем:
    // перезапись под каретку и есть причина «перескакивания».
    Effect::new(move |_| {
        let from = date_from.get();
        let to = date_to.get();
        let from_el = from_ref.get();
        let to_el = to_ref.get();

        if let (Some(el), Some(d)) = (from_el, parse_iso(&from)) {
            if !editing_from.get_untracked() {
                el.set_value(&dmy(d));
            }
        }
        if let (Some(el), Some(d)) = (to_el, parse_iso(&to)) {
            if !editing_to.get_untracked() {
                el.set_value(&dmy(d));
            }
        }
    });

    // Пока пользователь не переключал режим руками, режим следует за диапазоном.
    Effect::new(move |_| {
        let from = date_from.get();
        let to = date_to.get();
        if let (Some(a), Some(b)) = (parse_iso(&from), parse_iso(&to)) {
            if let Some(detected) = detect_unit(a, b) {
                if unit.get_untracked() != detected {
                    unit.set(detected);
                }
            }
        }
    });

    let emit = move |from: NaiveDate, to: NaiveDate| {
        let a = from.min(to);
        let b = from.max(to);
        if pair() != Some((a, b)) {
            on_change.run((iso(a), iso(b)));
        }
    };

    let emit_snapped = move |from: NaiveDate, to: NaiveDate, u: RangeUnit| {
        let (a, b) = snap_range(from, to, u);
        emit(a, b);
    };

    // ── Текстовые поля ────────────────────────────────────────────────────────

    let focus_field = move |node: NodeRef<leptos::html::Input>| {
        if let Some(el) = node.get_untracked() {
            let _ = el.focus();
            let len = el.value().chars().count() as u32;
            let _ = el.set_selection_range(0, len);
        }
    };

    // Разобрать содержимое поля и применить. `advance` — перейти во второе поле.
    let commit_field = move |is_from: bool, advance: bool| {
        let node = if is_from { from_ref } else { to_ref };
        let Some(el) = node.get_untracked() else {
            return;
        };
        let raw = el.value();
        let current = pair();
        let (cur_from, cur_to) = match current {
            Some((a, b)) => (Some(a), Some(b)),
            None => (None, None),
        };
        let anchor = cur_from.or(cur_to).unwrap_or_else(today);

        match parse_loose(&raw, anchor) {
            Some(d) => {
                el.set_value(&dmy(d));
                // Край, который «тянут», всегда остаётся тем, что набрал пользователь;
                // противоположный подтягивается, если пересёк его.
                let (a, b) = if is_from {
                    (d, cur_to.filter(|t| *t >= d).unwrap_or(d))
                } else {
                    (cur_from.filter(|f| *f <= d).unwrap_or(d), d)
                };
                emit(a, b);
            }
            None => {
                let fallback = if is_from { cur_from } else { cur_to };
                if let Some(d) = fallback {
                    el.set_value(&dmy(d));
                }
            }
        }

        if advance && is_from {
            focus_field(to_ref);
        }
    };

    // ↑/↓ на поле: ±1 день, с Shift — ±1 месяц.
    let nudge_field = move |is_from: bool, delta: i32, by_month: bool| {
        let node = if is_from { from_ref } else { to_ref };
        let Some(el) = node.get_untracked() else {
            return;
        };
        let current = pair();
        let anchor = current.map(|(a, _)| a).unwrap_or_else(today);
        let base = parse_loose(&el.value(), anchor)
            .or_else(|| current.map(|(a, b)| if is_from { a } else { b }));
        let Some(base) = base else {
            return;
        };
        let next = shift_by_units(
            base,
            if by_month {
                RangeUnit::Month
            } else {
                RangeUnit::Day
            },
            delta,
        );
        el.set_value(&dmy(next));
        let (cur_from, cur_to) = match current {
            Some((a, b)) => (Some(a), Some(b)),
            None => (None, None),
        };
        let (a, b) = if is_from {
            (next, cur_to.filter(|t| *t >= next).unwrap_or(next))
        } else {
            (cur_from.filter(|f| *f <= next).unwrap_or(next), next)
        };
        emit(a, b);
    };

    // ── Панель ────────────────────────────────────────────────────────────────

    let set_unit = move |u: RangeUnit| {
        unit.set(u);
        if let Some((a, b)) = pair() {
            emit_snapped(a, b, u);
        }
    };

    let step = move |direction: i32| {
        let u = unit.get_untracked();
        if let Some((a, b)) = pair() {
            let (na, nb) = shift_range(a, b, u, direction);
            emit(na, nb);
        }
    };

    let go_current = move |_| {
        let u = unit.get_untracked();
        let span = pair().map(|(a, b)| unit_count(a, b, u)).unwrap_or(1);
        let (na, nb) = current_range(u, span);
        emit(na, nb);
    };

    // ── Попап ─────────────────────────────────────────────────────────────────

    let open_popup = move |_| {
        let current = pair();
        popup_anchor.set(current.map(|(_, b)| b).unwrap_or_else(today));
        popup_sel.set(current);
        popup_hover.set(None);
        awaiting_end.set(false);
        popup_open.set(true);
    };

    let close_popup = move || {
        awaiting_end.set(false);
        popup_hover.set(None);
        popup_open.set(false);
    };

    let apply_and_close = move |a: NaiveDate, b: NaiveDate| {
        emit_snapped(a, b, unit.get_untracked());
        close_popup();
    };

    let on_cell_click = move |cell: Cell, extend: bool| {
        let (cs, ce) = cell.range();
        if awaiting_end.get_untracked() || extend {
            let (s0, s1) = popup_sel.get_untracked().unwrap_or((cs, ce));
            let a = s0.min(s1).min(cs);
            let b = s0.max(s1).max(ce);
            popup_sel.set(Some((a, b)));
            apply_and_close(a, b);
        } else {
            popup_sel.set(Some((cs, ce)));
            awaiting_end.set(true);
        }
    };

    let on_cell_hover = move |cell: Cell| {
        if awaiting_end.get_untracked() {
            popup_hover.set(Some(cell.range()));
        }
    };

    // Диапазон, который сейчас надо подсветить: выбор + то, что под курсором.
    let highlight = move || -> Option<(NaiveDate, NaiveDate)> {
        let sel = popup_sel.get()?;
        match popup_hover.get() {
            Some((ha, hb)) if awaiting_end.get() => {
                Some((sel.0.min(sel.1).min(ha), sel.0.max(sel.1).max(hb)))
            }
            _ => Some(sel),
        }
    };

    let apply_popup = move |_| {
        if let Some((a, b)) = popup_sel.get_untracked() {
            apply_and_close(a, b);
        } else {
            close_popup();
        }
    };

    let chip_label = move || {
        pair()
            .map(|(a, b)| range_label(a, b, unit.get()))
            .unwrap_or_else(|| "Период".to_string())
    };

    view! {
        <Flex vertical=true gap=FlexGap::Size(4)>
            {label.map(|l| view! { <Label>{l}</Label> })}

            <div class="date-range-picker date-range-picker--v3">
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
                                    title=u.title()
                                    on:click=move |_| set_unit(u)
                                >
                                    {u.label()}
                                </button>
                            }
                        })
                        .collect_view()}
                </div>

                <div class="date-range-picker__dates">
                    <input
                        type="text"
                        class="date-range-picker__input"
                        placeholder="дд.мм.гггг"
                        inputmode="numeric"
                        autocomplete="off"
                        spellcheck="false"
                        aria-label="Начало периода"
                        node_ref=from_ref
                        on:focus=move |ev| {
                            editing_from.set(true);
                            let el: HtmlInputElement = event_target(&ev);
                            let len = el.value().chars().count() as u32;
                            let _ = el.set_selection_range(0, len);
                        }
                        on:input=move |ev| {
                            let el: HtmlInputElement = event_target(&ev);
                            if let Some((a, b)) = parse_pair_input(&el.value()) {
                                el.set_value(&dmy(a));
                                if let Some(to_el) = to_ref.get_untracked() {
                                    to_el.set_value(&dmy(b));
                                }
                                emit(a, b);
                                focus_field(to_ref);
                            } else if digit_count(&el.value()) == 8 {
                                commit_field(true, true);
                            }
                        }
                        on:blur=move |_| {
                            editing_from.set(false);
                            commit_field(true, false);
                        }
                        on:keydown=move |ev: leptos::ev::KeyboardEvent| {
                            match ev.key().as_str() {
                                "Enter" => {
                                    ev.prevent_default();
                                    commit_field(true, true);
                                }
                                "ArrowUp" => {
                                    ev.prevent_default();
                                    nudge_field(true, 1, ev.shift_key());
                                }
                                "ArrowDown" => {
                                    ev.prevent_default();
                                    nudge_field(true, -1, ev.shift_key());
                                }
                                "Escape" => {
                                    ev.prevent_default();
                                    if let Some((a, _)) = pair() {
                                        let el: HtmlInputElement = event_target(&ev);
                                        el.set_value(&dmy(a));
                                        let _ = el.blur();
                                    }
                                }
                                _ => {}
                            }
                        }
                    />

                    <span class="date-range-picker__sep" aria-hidden="true">"."</span>

                    <input
                        type="text"
                        class="date-range-picker__input"
                        placeholder="дд.мм.гггг"
                        inputmode="numeric"
                        autocomplete="off"
                        spellcheck="false"
                        aria-label="Конец периода"
                        node_ref=to_ref
                        on:focus=move |ev| {
                            editing_to.set(true);
                            let el: HtmlInputElement = event_target(&ev);
                            let len = el.value().chars().count() as u32;
                            let _ = el.set_selection_range(0, len);
                        }
                        on:input=move |ev| {
                            let el: HtmlInputElement = event_target(&ev);
                            if let Some((a, b)) = parse_pair_input(&el.value()) {
                                if let Some(from_el) = from_ref.get_untracked() {
                                    from_el.set_value(&dmy(a));
                                }
                                el.set_value(&dmy(b));
                                emit(a, b);
                            } else if digit_count(&el.value()) == 8 {
                                commit_field(false, false);
                            }
                        }
                        on:blur=move |_| {
                            editing_to.set(false);
                            commit_field(false, false);
                        }
                        on:keydown=move |ev: leptos::ev::KeyboardEvent| {
                            match ev.key().as_str() {
                                "Enter" => {
                                    ev.prevent_default();
                                    commit_field(false, false);
                                }
                                "ArrowUp" => {
                                    ev.prevent_default();
                                    nudge_field(false, 1, ev.shift_key());
                                }
                                "ArrowDown" => {
                                    ev.prevent_default();
                                    nudge_field(false, -1, ev.shift_key());
                                }
                                "Escape" => {
                                    ev.prevent_default();
                                    if let Some((_, b)) = pair() {
                                        let el: HtmlInputElement = event_target(&ev);
                                        el.set_value(&dmy(b));
                                        let _ = el.blur();
                                    }
                                }
                                _ => {}
                            }
                        }
                    />
                </div>

                <div class="drp-nav-buttons">
                    <button
                        type="button"
                        class="drp-icon-btn"
                        title=move || format!("Предыдущий период (−{})", unit.get().title().to_lowercase())
                        on:click=move |_| step(-1)
                    >
                        <div class="drp-btn-icon">
                            <svg width="10" height="12" viewBox="0 0 10 12" fill="none">
                                <path d="M7 1L2 6l5 5" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>
                            </svg>
                            <span>{move || unit.get().short_label()}</span>
                        </div>
                    </button>

                    <button
                        type="button"
                        class="drp-icon-btn"
                        title="Текущий период"
                        on:click=go_current
                    >
                        <svg width="14" height="14" viewBox="0 0 14 14" fill="none">
                            <circle cx="7" cy="7" r="3.5" stroke="currentColor" stroke-width="1.5"/>
                            <circle cx="7" cy="7" r="1.5" fill="currentColor"/>
                        </svg>
                    </button>

                    <button
                        type="button"
                        class="drp-icon-btn"
                        title=move || format!("Следующий период (+{})", unit.get().title().to_lowercase())
                        on:click=move |_| step(1)
                    >
                        <div class="drp-btn-icon">
                            <span>{move || unit.get().short_label()}</span>
                            <svg width="10" height="12" viewBox="0 0 10 12" fill="none">
                                <path d="M3 1l5 5-5 5" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>
                            </svg>
                        </div>
                    </button>
                </div>

                <button
                    type="button"
                    class="date-range-picker__chip"
                    title="Выбрать период или интервал"
                    on:click=open_popup
                >
                    <span class="date-range-picker__chip-text">{chip_label}</span>
                    <svg width="9" height="9" viewBox="0 0 10 10" fill="none" aria-hidden="true">
                        <path d="M2 4l3 3 3-3" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>
                    </svg>
                </button>
            </div>
        </Flex>

        <Dialog open=popup_open>
            <DialogSurface>
                <DialogBody>
                    <DialogTitle>
                        {move || format!("Период · {}", unit.get().title().to_lowercase())}
                    </DialogTitle>
                    <DialogContent>
                        <div class="date-range-picker__dialog">
                            <div class="date-range-picker__presets">
                                {move || {
                                    let u = unit.get();
                                    presets_for(u)
                                        .into_iter()
                                        .map(|(text, preset)| {
                                            view! {
                                                <button
                                                    type="button"
                                                    class="date-range-picker__preset"
                                                    on:click=move |_| {
                                                        let (a, b) = preset_range(preset, u);
                                                        popup_sel.set(Some((a, b)));
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
                                let anchor = popup_anchor.get();
                                let sel = highlight();

                                match u {
                                    RangeUnit::Year => {
                                        let center = anchor.year();
                                        let first = center - 5;
                                        let years: Vec<i32> = (first..=(first + 11)).collect();
                                        view! {
                                            <div class="date-range-picker__dialog-nav">
                                                <button
                                                    type="button"
                                                    class="drp-icon-btn"
                                                    title="Раньше"
                                                    on:click=move |_| popup_anchor.update(|d| {
                                                        *d = shift_by_units(*d, RangeUnit::Year, -12);
                                                    })
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
                                                    class="drp-icon-btn"
                                                    title="Позже"
                                                    on:click=move |_| popup_anchor.update(|d| {
                                                        *d = shift_by_units(*d, RangeUnit::Year, 12);
                                                    })
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
                                                                class=cell_class(cell, sel)
                                                                on:mouseenter=move |_| on_cell_hover(cell)
                                                                on:click=move |ev: leptos::ev::MouseEvent| {
                                                                    on_cell_click(cell, ev.shift_key())
                                                                }
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
                                        let year = anchor.year();
                                        view! {
                                            <div class="date-range-picker__dialog-nav">
                                                <button
                                                    type="button"
                                                    class="drp-icon-btn"
                                                    title="Предыдущий год"
                                                    on:click=move |_| popup_anchor.update(|d| {
                                                        *d = shift_by_units(*d, RangeUnit::Year, -1);
                                                    })
                                                >
                                                    <svg width="10" height="12" viewBox="0 0 10 12" fill="none">
                                                        <path d="M7 1L2 6l5 5" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>
                                                    </svg>
                                                </button>
                                                <span class="date-range-picker__dialog-title">{year.to_string()}</span>
                                                <button
                                                    type="button"
                                                    class="drp-icon-btn"
                                                    title="Следующий год"
                                                    on:click=move |_| popup_anchor.update(|d| {
                                                        *d = shift_by_units(*d, RangeUnit::Year, 1);
                                                    })
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
                                                                class=cell_class(cell, sel)
                                                                on:mouseenter=move |_| on_cell_hover(cell)
                                                                on:click=move |ev: leptos::ev::MouseEvent| {
                                                                    on_cell_click(cell, ev.shift_key())
                                                                }
                                                            >
                                                                {month_name_short(month)}
                                                            </button>
                                                        }
                                                    })
                                                    .collect_view()}
                                            </div>
                                        }
                                            .into_any()
                                    }
                                    RangeUnit::Week | RangeUnit::Day => {
                                        let year = anchor.year();
                                        let month = anchor.month();
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
                                                    class="drp-icon-btn"
                                                    title="Предыдущий месяц"
                                                    on:click=move |_| popup_anchor.update(|d| {
                                                        *d = shift_by_units(*d, RangeUnit::Month, -1);
                                                    })
                                                >
                                                    <svg width="10" height="12" viewBox="0 0 10 12" fill="none">
                                                        <path d="M7 1L2 6l5 5" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>
                                                    </svg>
                                                </button>
                                                <span class="date-range-picker__dialog-title">
                                                    {format!("{} {}", month_name_short(month), year)}
                                                </span>
                                                <button
                                                    type="button"
                                                    class="drp-icon-btn"
                                                    title="Следующий месяц"
                                                    on:click=move |_| popup_anchor.update(|d| {
                                                        *d = shift_by_units(*d, RangeUnit::Month, 1);
                                                    })
                                                >
                                                    <svg width="10" height="12" viewBox="0 0 10 12" fill="none">
                                                        <path d="M3 1l5 5-5 5" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>
                                                    </svg>
                                                </button>
                                            </div>
                                            <div class="date-range-picker__weekday-row">
                                                {[
                                                    Weekday::Mon,
                                                    Weekday::Tue,
                                                    Weekday::Wed,
                                                    Weekday::Thu,
                                                    Weekday::Fri,
                                                    Weekday::Sat,
                                                    Weekday::Sun,
                                                ]
                                                    .into_iter()
                                                    .map(|wd| {
                                                        view! {
                                                            <span class="date-range-picker__weekday">{weekday_short(wd)}</span>
                                                        }
                                                    })
                                                    .collect_view()}
                                            </div>
                                            <div class="date-range-picker__grid date-range-picker__grid--days">
                                                {if u == RangeUnit::Week {
                                                    days.chunks(7)
                                                        .map(|week| {
                                                            let cell = Cell::Week(week[0]);
                                                            let week_days = week.to_vec();
                                                            view! {
                                                                <button
                                                                    type="button"
                                                                    class=format!(
                                                                        "{} date-range-picker__cell--week-row",
                                                                        cell_class(cell, sel),
                                                                    )
                                                                    on:mouseenter=move |_| on_cell_hover(cell)
                                                                    on:click=move |ev: leptos::ev::MouseEvent| {
                                                                        on_cell_click(cell, ev.shift_key())
                                                                    }
                                                                >
                                                                    {week_days
                                                                        .into_iter()
                                                                        .map(|d| {
                                                                            let outside = d.month() != month;
                                                                            view! {
                                                                                <span class=if outside {
                                                                                    "date-range-picker__day date-range-picker__day--muted"
                                                                                } else {
                                                                                    "date-range-picker__day"
                                                                                }>{d.day().to_string()}</span>
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
                                                            let mut class = cell_class(cell, sel);
                                                            if d.month() != month {
                                                                class.push_str(" date-range-picker__cell--muted");
                                                            }
                                                            view! {
                                                                <button
                                                                    type="button"
                                                                    class=class
                                                                    on:mouseenter=move |_| on_cell_hover(cell)
                                                                    on:click=move |ev: leptos::ev::MouseEvent| {
                                                                        on_cell_click(cell, ev.shift_key())
                                                                    }
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

                            <p class="date-range-picker__hint">
                                {move || {
                                    let u = unit.get();
                                    match highlight() {
                                        Some((a, b)) => {
                                            let tail = if awaiting_end.get() {
                                                " · выберите конец интервала"
                                            } else {
                                                ""
                                            };
                                            format!(
                                                "{} . {} — {}{}",
                                                dmy(a),
                                                dmy(b),
                                                range_label(a, b, u),
                                                tail,
                                            )
                                        }
                                        None => "Выберите начало периода".to_string(),
                                    }
                                }}
                            </p>
                        </div>
                    </DialogContent>
                    <DialogActions>
                        <Button appearance=ButtonAppearance::Primary on_click=apply_popup>
                            "OK"
                        </Button>
                        <Button
                            appearance=ButtonAppearance::Subtle
                            on_click=move |_| close_popup()
                        >
                            "Отмена"
                        </Button>
                    </DialogActions>
                </DialogBody>
            </DialogSurface>
        </Dialog>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date(year: i32, month: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(year, month, day).expect("test date")
    }

    #[test]
    fn parses_compact_date_and_fills_year() {
        let anchor = date(2026, 8, 14);
        assert_eq!(parse_loose("01022026", anchor), Some(date(2026, 2, 1)));
        assert_eq!(parse_loose("0102", anchor), Some(date(2026, 2, 1)));
        assert_eq!(parse_loose("1.2.26", anchor), Some(date(2026, 2, 1)));
    }

    #[test]
    fn parses_pasted_range_in_either_order() {
        assert_eq!(
            parse_pair_input("28.02.2026 — 01.02.2026"),
            Some((date(2026, 2, 1), date(2026, 2, 28))),
        );
    }

    #[test]
    fn snaps_multi_unit_ranges() {
        assert_eq!(
            snap_range(date(2026, 2, 10), date(2026, 4, 3), RangeUnit::Month),
            (date(2026, 2, 1), date(2026, 4, 30)),
        );
        assert_eq!(
            snap_range(date(2026, 2, 4), date(2026, 2, 18), RangeUnit::Week),
            (date(2026, 2, 2), date(2026, 2, 22)),
        );
    }
}
