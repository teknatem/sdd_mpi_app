use chrono::{Datelike, Duration, NaiveDate, Utc, Weekday};
use leptos::prelude::*;
use thaw::*;
use web_sys::HtmlInputElement;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum DateRangeMode {
    Year,
    #[default]
    Month,
    Week,
    Day,
}

impl DateRangeMode {
    fn label(self) -> &'static str {
        match self {
            Self::Year => "Г",
            Self::Month => "М",
            Self::Week => "Н",
            Self::Day => "Д",
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
    if month == 12 {
        NaiveDate::from_ymd_opt(year + 1, 1, 1)
            .map(|d| d - Duration::days(1))
            .expect("valid date")
    } else {
        NaiveDate::from_ymd_opt(year, month + 1, 1)
            .map(|d| d - Duration::days(1))
            .expect("valid date")
    }
}

fn unit_start(date: NaiveDate, mode: DateRangeMode) -> NaiveDate {
    match mode {
        DateRangeMode::Year => NaiveDate::from_ymd_opt(date.year(), 1, 1).expect("valid"),
        DateRangeMode::Month => {
            NaiveDate::from_ymd_opt(date.year(), date.month(), 1).expect("valid")
        }
        DateRangeMode::Week => {
            let wd = date.weekday().num_days_from_monday() as i64;
            date - Duration::days(wd)
        }
        DateRangeMode::Day => date,
    }
}

fn unit_end(date: NaiveDate, mode: DateRangeMode) -> NaiveDate {
    match mode {
        DateRangeMode::Year => NaiveDate::from_ymd_opt(date.year(), 12, 31).expect("valid"),
        DateRangeMode::Month => last_day_of_month(date.year(), date.month()),
        DateRangeMode::Week => unit_start(date, DateRangeMode::Week) + Duration::days(6),
        DateRangeMode::Day => date,
    }
}

fn snap_range(from: NaiveDate, to: NaiveDate, mode: DateRangeMode) -> (NaiveDate, NaiveDate) {
    let a = from.min(to);
    let b = from.max(to);
    (unit_start(a, mode), unit_end(b, mode))
}

fn unit_count(from: NaiveDate, to: NaiveDate, mode: DateRangeMode) -> i32 {
    let (a, b) = snap_range(from, to, mode);
    match mode {
        DateRangeMode::Year => b.year() - a.year() + 1,
        DateRangeMode::Month => {
            (b.year() - a.year()) * 12 + (b.month() as i32 - a.month() as i32) + 1
        }
        DateRangeMode::Week => (((b - a).num_days() + 1 + 6) / 7) as i32,
        DateRangeMode::Day => ((b - a).num_days() + 1) as i32,
    }
}

fn shift_by_units(date: NaiveDate, mode: DateRangeMode, delta: i32) -> NaiveDate {
    match mode {
        DateRangeMode::Year => {
            NaiveDate::from_ymd_opt(date.year() + delta, date.month(), date.day())
                .or_else(|| NaiveDate::from_ymd_opt(date.year() + delta, date.month(), 28))
                .unwrap_or(date)
        }
        DateRangeMode::Month => {
            let total = date.year() * 12 + (date.month() as i32 - 1) + delta;
            let y = total.div_euclid(12);
            let m = (total.rem_euclid(12) + 1) as u32;
            let max_day = last_day_of_month(y, m).day();
            NaiveDate::from_ymd_opt(y, m, date.day().min(max_day)).unwrap_or(date)
        }
        DateRangeMode::Week => date + Duration::weeks(delta as i64),
        DateRangeMode::Day => date + Duration::days(delta as i64),
    }
}

/// Сдвиг интервала на его длину: длина (число единиц) сохраняется.
fn shift_range(
    from: NaiveDate,
    to: NaiveDate,
    mode: DateRangeMode,
    direction: i32,
) -> (NaiveDate, NaiveDate) {
    let (a, _b) = snap_range(from, to, mode);
    let n = unit_count(from, to, mode).max(1);
    let new_a = shift_by_units(unit_start(a, mode), mode, direction * n);
    let new_b = unit_end(shift_by_units(new_a, mode, n - 1), mode);
    (new_a, new_b)
}

fn current_range(mode: DateRangeMode, span: i32) -> (NaiveDate, NaiveDate) {
    let now = Utc::now().date_naive();
    let n = span.max(1);
    let end_unit = unit_start(now, mode);
    let start = shift_by_units(end_unit, mode, -(n - 1));
    (unit_start(start, mode), unit_end(end_unit, mode))
}

fn mask_dmy_digits(digits: &str) -> String {
    let d: String = digits
        .chars()
        .filter(|c| c.is_ascii_digit())
        .take(8)
        .collect();
    match d.len() {
        0 => String::new(),
        1..=2 => d,
        3..=4 => format!("{}.{}", &d[..2], &d[2..]),
        5..=8 => format!("{}.{}.{}", &d[..2], &d[2..4], &d[4..]),
        _ => d,
    }
}

fn caret_after_digits(masked: &str, digit_count: usize) -> u32 {
    if digit_count == 0 {
        return 0;
    }
    let mut seen = 0usize;
    for (i, ch) in masked.chars().enumerate() {
        if ch.is_ascii_digit() {
            seen += 1;
            if seen == digit_count {
                return (i + 1) as u32;
            }
        }
    }
    masked.len() as u32
}

fn parse_dmy_draft(raw: &str) -> Option<NaiveDate> {
    let digits: String = raw.chars().filter(|c| c.is_ascii_digit()).collect();
    let now_year = Utc::now().date_naive().year();
    let (day, month, year) = match digits.len() {
        8 => (
            digits[0..2].parse().ok()?,
            digits[2..4].parse().ok()?,
            digits[4..8].parse().ok()?,
        ),
        6 => (
            digits[0..2].parse().ok()?,
            digits[2..4].parse().ok()?,
            2000 + digits[4..6].parse::<i32>().ok()?,
        ),
        4 => (
            digits[0..2].parse().ok()?,
            digits[2..4].parse().ok()?,
            now_year,
        ),
        _ => return NaiveDate::parse_from_str(raw.trim(), "%d.%m.%Y").ok(),
    };
    NaiveDate::from_ymd_opt(year, month, day)
}

fn commit_draft(raw: &str) -> Option<NaiveDate> {
    let digits: String = raw.chars().filter(|c| c.is_ascii_digit()).collect();
    if matches!(digits.len(), 4 | 6 | 8) {
        parse_dmy_draft(raw)
    } else {
        NaiveDate::parse_from_str(raw.trim(), "%d.%m.%Y").ok()
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

fn weekday_short(wd: Weekday) -> &'static str {
    match wd {
        Weekday::Mon => "Пн",
        Weekday::Tue => "Вт",
        Weekday::Wed => "Ср",
        Weekday::Thu => "Чт",
        Weekday::Fri => "Пт",
        Weekday::Sat => "Сб",
        Weekday::Sun => "Вс",
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum CellKind {
    Year(i32),
    Month { year: i32, month: u32 },
    Week(NaiveDate),
    Day(NaiveDate),
}

impl CellKind {
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
            Self::Week(mon) => (mon, mon + Duration::days(6)),
            Self::Day(d) => (d, d),
        }
    }
}

fn cell_class(cell: CellKind, sel_from: NaiveDate, sel_to: NaiveDate) -> &'static str {
    let (cs, ce) = cell.range();
    let a = sel_from.min(sel_to);
    let b = sel_from.max(sel_to);
    if cs > b || ce < a {
        return "date-range-picker__cell";
    }
    let contains_start = cs <= a && a <= ce;
    let contains_end = cs <= b && b <= ce;
    if contains_start && contains_end {
        "date-range-picker__cell date-range-picker__cell--start date-range-picker__cell--end"
    } else if contains_start {
        "date-range-picker__cell date-range-picker__cell--start"
    } else if contains_end {
        "date-range-picker__cell date-range-picker__cell--end"
    } else {
        "date-range-picker__cell date-range-picker__cell--in-range"
    }
}

fn apply_mask_to_input(el: &HtmlInputElement) -> (String, usize) {
    let raw = el.value();
    let sel = el.selection_start().ok().flatten().unwrap_or(0) as usize;
    let digit_before = raw.chars().take(sel).filter(|c| c.is_ascii_digit()).count();
    let digits: String = raw.chars().filter(|c| c.is_ascii_digit()).take(8).collect();
    let masked = mask_dmy_digits(&digits);
    let caret = caret_after_digits(&masked, digit_before.min(digits.len()));
    let _ = el.set_value(&masked);
    let _ = el.set_selection_range(caret, caret);
    (masked, digits.len())
}

/// DateRangePickerExp — экспериментальный выбор периода (год/месяц/неделя/день).
#[component]
pub fn DateRangePickerExp(
    #[prop(into)] date_from: Signal<String>,
    #[prop(into)] date_to: Signal<String>,
    on_change: Callback<(String, String)>,
    #[prop(optional)] label: Option<String>,
) -> impl IntoView {
    let mode = RwSignal::new(DateRangeMode::Month);
    let show_dialog = RwSignal::new(false);
    let dialog_anchor = RwSignal::new(Utc::now().date_naive());
    // Черновик выбора в диалоге (после 1-го клика — одна единица).
    let dialog_sel = RwSignal::new(Option::<(NaiveDate, NaiveDate)>::None);
    // Ожидаем второй клик для конца диапазона.
    let awaiting_end = RwSignal::new(false);

    let draft_from = RwSignal::new(String::new());
    let draft_to = RwSignal::new(String::new());
    let editing_from = RwSignal::new(false);
    let editing_to = RwSignal::new(false);
    let to_input_ref = NodeRef::<leptos::html::Input>::new();
    let from_input_ref = NodeRef::<leptos::html::Input>::new();

    Effect::new(move |_| {
        let from = date_from.get();
        let to = date_to.get();
        if !editing_from.get_untracked() {
            if let Some(d) = parse_iso(&from) {
                draft_from.set(dmy(d));
            }
        }
        if !editing_to.get_untracked() {
            if let Some(d) = parse_iso(&to) {
                draft_to.set(dmy(d));
            }
        }
    });

    let on_change_init = on_change.clone();
    Effect::new(move |_| {
        if date_from.get().is_empty() && date_to.get().is_empty() {
            let (a, b) = current_range(DateRangeMode::Month, 1);
            on_change_init.run((iso(a), iso(b)));
        }
    });

    let emit_snapped = {
        let on_change = on_change.clone();
        move |from: NaiveDate, to: NaiveDate, m: DateRangeMode| {
            let (a, b) = snap_range(from, to, m);
            draft_from.set(dmy(a));
            draft_to.set(dmy(b));
            on_change.run((iso(a), iso(b)));
        }
    };

    // Ручной ввод — без snap, чтобы не прыгали даты при наборе.
    let emit_exact = {
        let on_change = on_change.clone();
        move |from: NaiveDate, to: NaiveDate| {
            let a = from.min(to);
            let b = from.max(to);
            draft_from.set(dmy(a));
            draft_to.set(dmy(b));
            on_change.run((iso(a), iso(b)));
        }
    };

    let parsed_pair = move || -> Option<(NaiveDate, NaiveDate)> {
        Some((
            parse_iso(&date_from.get_untracked())?,
            parse_iso(&date_to.get_untracked())?,
        ))
    };

    let set_mode = {
        let emit_snapped = emit_snapped.clone();
        move |m: DateRangeMode| {
            mode.set(m);
            if let Some((a, b)) = parsed_pair() {
                emit_snapped(a, b, m);
            }
        }
    };

    let on_prev = {
        let emit_snapped = emit_snapped.clone();
        move |_| {
            let m = mode.get_untracked();
            if let Some((a, b)) = parsed_pair() {
                let (na, nb) = shift_range(a, b, m, -1);
                emit_snapped(na, nb, m);
            }
        }
    };

    let on_next = {
        let emit_snapped = emit_snapped.clone();
        move |_| {
            let m = mode.get_untracked();
            if let Some((a, b)) = parsed_pair() {
                let (na, nb) = shift_range(a, b, m, 1);
                emit_snapped(na, nb, m);
            }
        }
    };

    let on_current = {
        let emit_snapped = emit_snapped.clone();
        move |_| {
            let m = mode.get_untracked();
            let span = parsed_pair().map(|(a, b)| unit_count(a, b, m)).unwrap_or(1);
            let (na, nb) = current_range(m, span);
            emit_snapped(na, nb, m);
        }
    };

    let focus_to = move || {
        if let Some(el) = to_input_ref.get() {
            let _ = el.focus();
            let len = el.value().len() as u32;
            let _ = el.set_selection_range(0, len);
        }
    };

    let select_all = move |el: &HtmlInputElement| {
        let len = el.value().len() as u32;
        let _ = el.set_selection_range(0, len);
    };

    let apply_from_draft = {
        let emit_exact = emit_exact.clone();
        move |focus_next: bool| {
            let raw = draft_from.get_untracked();
            if let Some(d) = commit_draft(&raw) {
                draft_from.set(dmy(d));
                let to = parse_iso(&date_to.get_untracked()).unwrap_or(d);
                emit_exact(d, to);
                if focus_next {
                    focus_to();
                }
            } else if let Some(d) = parse_iso(&date_from.get_untracked()) {
                draft_from.set(dmy(d));
            }
        }
    };

    let apply_to_draft = {
        let emit_exact = emit_exact.clone();
        move || {
            let raw = draft_to.get_untracked();
            if let Some(d) = commit_draft(&raw) {
                draft_to.set(dmy(d));
                let from = parse_iso(&date_from.get_untracked()).unwrap_or(d);
                emit_exact(from, d);
            } else if let Some(d) = parse_iso(&date_to.get_untracked()) {
                draft_to.set(dmy(d));
            }
        }
    };

    let on_from_input = {
        let emit_exact = emit_exact.clone();
        move |ev: leptos::ev::Event| {
            let el: HtmlInputElement = event_target(&ev);
            let (masked, digit_len) = apply_mask_to_input(&el);
            draft_from.set(masked.clone());
            if digit_len == 8 {
                if let Some(d) = parse_dmy_draft(&masked) {
                    draft_from.set(dmy(d));
                    let to = parse_iso(&date_to.get_untracked()).unwrap_or(d);
                    emit_exact(d, to);
                    editing_from.set(false);
                    focus_to();
                }
            }
        }
    };

    let on_to_input = {
        let emit_exact = emit_exact.clone();
        move |ev: leptos::ev::Event| {
            let el: HtmlInputElement = event_target(&ev);
            let (masked, digit_len) = apply_mask_to_input(&el);
            draft_to.set(masked.clone());
            if digit_len == 8 {
                if let Some(d) = parse_dmy_draft(&masked) {
                    draft_to.set(dmy(d));
                    let from = parse_iso(&date_from.get_untracked()).unwrap_or(d);
                    emit_exact(from, d);
                }
            }
        }
    };

    let open_dialog = move |_| {
        let from = parse_iso(&date_from.get_untracked());
        let to = parse_iso(&date_to.get_untracked());
        let anchor = to.or(from).unwrap_or_else(|| Utc::now().date_naive());
        dialog_anchor.set(anchor);
        if let (Some(a), Some(b)) = (from, to) {
            dialog_sel.set(Some((a.min(b), a.max(b))));
        } else {
            dialog_sel.set(None);
        }
        awaiting_end.set(false);
        show_dialog.set(true);
    };

    let on_cell_click = move |cell: CellKind| {
        let (cs, ce) = cell.range();
        if !awaiting_end.get_untracked() {
            // Первый клик: сброс старого выбора, старт нового
            dialog_sel.set(Some((cs, ce)));
            awaiting_end.set(true);
        } else {
            // Второй клик: расширяем до выбранной ячейки
            let (f0, f1) = dialog_sel.get_untracked().unwrap_or((cs, ce));
            let start = f0.min(f1).min(cs);
            let end = f0.max(f1).max(ce);
            dialog_sel.set(Some((start, end)));
            awaiting_end.set(false);
        }
    };

    let apply_dialog = {
        let emit_snapped = emit_snapped.clone();
        move |_| {
            let m = mode.get_untracked();
            if let Some((a, b)) = dialog_sel.get_untracked() {
                emit_snapped(a, b, m);
            }
            awaiting_end.set(false);
            show_dialog.set(false);
        }
    };

    let cancel_dialog = move |_| {
        awaiting_end.set(false);
        dialog_sel.set(None);
        show_dialog.set(false);
    };

    let mode_letter = move || mode.get().label();

    view! {
        <Flex vertical=true gap=FlexGap::Size(4)>
            {label.map(|l| view! { <Label>{l}</Label> })}

            <div class="date-range-picker date-range-picker--exp">
                <div class="date-range-picker__mode" role="group" aria-label="Режим периода">
                    {[
                        DateRangeMode::Year,
                        DateRangeMode::Month,
                        DateRangeMode::Week,
                        DateRangeMode::Day,
                    ]
                        .into_iter()
                        .map(|m| {
                            let set_mode = set_mode.clone();
                            view! {
                                <button
                                    type="button"
                                    class=move || {
                                        if mode.get() == m {
                                            "date-range-picker__mode-btn date-range-picker__mode-btn--active"
                                        } else {
                                            "date-range-picker__mode-btn"
                                        }
                                    }
                                    title=m.title()
                                    on:click=move |_| set_mode(m)
                                >
                                    {m.label()}
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
                        maxlength="10"
                        inputmode="numeric"
                        autocomplete="off"
                        spellcheck="false"
                        node_ref=from_input_ref
                        prop:value=move || draft_from.get()
                        on:focus=move |ev| {
                            editing_from.set(true);
                            let el: HtmlInputElement = event_target(&ev);
                            select_all(&el);
                        }
                        on:input=on_from_input
                        on:blur={
                            let apply_from_draft = apply_from_draft.clone();
                            move |_| {
                                editing_from.set(false);
                                apply_from_draft(false);
                            }
                        }
                        on:keydown={
                            let apply_from_draft = apply_from_draft.clone();
                            move |ev: leptos::ev::KeyboardEvent| {
                                if ev.key() == "Enter" {
                                    ev.prevent_default();
                                    editing_from.set(false);
                                    apply_from_draft(true);
                                }
                            }
                        }
                    />

                    <input
                        type="text"
                        class="date-range-picker__input"
                        placeholder="дд.мм.гггг"
                        maxlength="10"
                        inputmode="numeric"
                        autocomplete="off"
                        spellcheck="false"
                        node_ref=to_input_ref
                        prop:value=move || draft_to.get()
                        on:focus=move |ev| {
                            editing_to.set(true);
                            let el: HtmlInputElement = event_target(&ev);
                            select_all(&el);
                        }
                        on:input=on_to_input
                        on:blur={
                            let apply_to_draft = apply_to_draft.clone();
                            move |_| {
                                editing_to.set(false);
                                apply_to_draft();
                            }
                        }
                        on:keydown={
                            let apply_to_draft = apply_to_draft.clone();
                            move |ev: leptos::ev::KeyboardEvent| {
                                if ev.key() == "Enter" {
                                    ev.prevent_default();
                                    editing_to.set(false);
                                    apply_to_draft();
                                }
                            }
                        }
                    />
                </div>

                <div class="drp-nav-buttons">
                    <button
                        type="button"
                        class="drp-icon-btn"
                        title=move || format!("−{}", mode.get().title())
                        on:click=on_prev
                    >
                        <div class="drp-btn-icon">
                            <svg width="10" height="12" viewBox="0 0 10 12" fill="none">
                                <path d="M7 1L2 6l5 5" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>
                            </svg>
                            <span>{mode_letter}</span>
                        </div>
                    </button>

                    <button
                        type="button"
                        class="drp-icon-btn"
                        title="Текущий период"
                        on:click=on_current
                    >
                        <svg width="14" height="14" viewBox="0 0 14 14" fill="none">
                            <circle cx="7" cy="7" r="3.5" stroke="currentColor" stroke-width="1.5"/>
                            <circle cx="7" cy="7" r="1.5" fill="currentColor"/>
                        </svg>
                    </button>

                    <button
                        type="button"
                        class="drp-icon-btn"
                        title=move || format!("+{}", mode.get().title())
                        on:click=on_next
                    >
                        <div class="drp-btn-icon">
                            <span>{mode_letter}</span>
                            <svg width="10" height="12" viewBox="0 0 10 12" fill="none">
                                <path d="M3 1l5 5-5 5" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>
                            </svg>
                        </div>
                    </button>

                    <button
                        type="button"
                        class="drp-icon-btn"
                        title="Выбрать период"
                        on:click=open_dialog
                    >
                        <svg width="14" height="14" viewBox="0 0 14 14" fill="none">
                            <rect x="1.5" y="3" width="11" height="9.5" rx="1.5" stroke="currentColor" stroke-width="1.3"/>
                            <path d="M1.5 6.5h11" stroke="currentColor" stroke-width="1.3"/>
                            <path d="M4.5 1.5v2M9.5 1.5v2" stroke="currentColor" stroke-width="1.3" stroke-linecap="round"/>
                        </svg>
                    </button>
                </div>
            </div>
        </Flex>

        <Dialog open=show_dialog>
            <DialogSurface>
                <DialogBody>
                    <DialogTitle>
                        {move || format!("Выберите {}", mode.get().title().to_lowercase())}
                    </DialogTitle>
                    <DialogContent>
                        <div class="date-range-picker__dialog">
                            {move || {
                                let m = mode.get();
                                let anchor = dialog_anchor.get();
                                let (sel_from, sel_to) = dialog_sel
                                    .get()
                                    .unwrap_or_else(|| (anchor, anchor));
                                let _awaiting = awaiting_end.get();

                                match m {
                                    DateRangeMode::Year => {
                                        let center = anchor.year();
                                        let years: Vec<i32> = ((center - 5)..=(center + 5)).collect();
                                        view! {
                                            <div class="date-range-picker__dialog-nav">
                                                <button
                                                    type="button"
                                                    class="drp-icon-btn"
                                                    on:click=move |_| {
                                                        dialog_anchor.update(|d| {
                                                            *d = shift_by_units(*d, DateRangeMode::Year, -11);
                                                        });
                                                    }
                                                >
                                                    <svg width="10" height="12" viewBox="0 0 10 12" fill="none">
                                                        <path d="M7 1L2 6l5 5" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>
                                                    </svg>
                                                </button>
                                                <span class="date-range-picker__dialog-title">
                                                    {format!("{} – {}", center - 5, center + 5)}
                                                </span>
                                                <button
                                                    type="button"
                                                    class="drp-icon-btn"
                                                    on:click=move |_| {
                                                        dialog_anchor.update(|d| {
                                                            *d = shift_by_units(*d, DateRangeMode::Year, 11);
                                                        });
                                                    }
                                                >
                                                    <svg width="10" height="12" viewBox="0 0 10 12" fill="none">
                                                        <path d="M3 1l5 5-5 5" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>
                                                    </svg>
                                                </button>
                                            </div>
                                            <div class="date-range-picker__grid date-range-picker__grid--years">
                                                {years.into_iter().map(|y| {
                                                    let cell = CellKind::Year(y);
                                                    let cls = cell_class(cell, sel_from, sel_to);
                                                    view! {
                                                        <button
                                                            type="button"
                                                            class=cls
                                                            on:click=move |_| on_cell_click(cell)
                                                        >
                                                            {y.to_string()}
                                                        </button>
                                                    }
                                                }).collect_view()}
                                            </div>
                                        }.into_any()
                                    }
                                    DateRangeMode::Month => {
                                        let year = anchor.year();
                                        view! {
                                            <div class="date-range-picker__dialog-nav">
                                                <button
                                                    type="button"
                                                    class="drp-icon-btn"
                                                    on:click=move |_| {
                                                        dialog_anchor.update(|d| {
                                                            *d = shift_by_units(*d, DateRangeMode::Year, -1);
                                                        });
                                                    }
                                                >
                                                    <svg width="10" height="12" viewBox="0 0 10 12" fill="none">
                                                        <path d="M7 1L2 6l5 5" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>
                                                    </svg>
                                                </button>
                                                <span class="date-range-picker__dialog-title">{year.to_string()}</span>
                                                <button
                                                    type="button"
                                                    class="drp-icon-btn"
                                                    on:click=move |_| {
                                                        dialog_anchor.update(|d| {
                                                            *d = shift_by_units(*d, DateRangeMode::Year, 1);
                                                        });
                                                    }
                                                >
                                                    <svg width="10" height="12" viewBox="0 0 10 12" fill="none">
                                                        <path d="M3 1l5 5-5 5" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>
                                                    </svg>
                                                </button>
                                            </div>
                                            <div class="date-range-picker__grid date-range-picker__grid--months">
                                                {(1u32..=12).map(|month| {
                                                    let cell = CellKind::Month { year, month };
                                                    let cls = cell_class(cell, sel_from, sel_to);
                                                    view! {
                                                        <button
                                                            type="button"
                                                            class=cls
                                                            on:click=move |_| on_cell_click(cell)
                                                        >
                                                            {month_name_short(month)}
                                                        </button>
                                                    }
                                                }).collect_view()}
                                            </div>
                                        }.into_any()
                                    }
                                    DateRangeMode::Week | DateRangeMode::Day => {
                                        let year = anchor.year();
                                        let month = anchor.month();
                                        let first = NaiveDate::from_ymd_opt(year, month, 1).expect("valid");
                                        let start_pad = first.weekday().num_days_from_monday() as i64;
                                        let grid_start = first - Duration::days(start_pad);
                                        let days: Vec<NaiveDate> = (0..42)
                                            .map(|i| grid_start + Duration::days(i))
                                            .collect();

                                        view! {
                                            <div class="date-range-picker__dialog-nav">
                                                <button
                                                    type="button"
                                                    class="drp-icon-btn"
                                                    on:click=move |_| {
                                                        dialog_anchor.update(|d| {
                                                            *d = shift_by_units(*d, DateRangeMode::Month, -1);
                                                        });
                                                    }
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
                                                    on:click=move |_| {
                                                        dialog_anchor.update(|d| {
                                                            *d = shift_by_units(*d, DateRangeMode::Month, 1);
                                                        });
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
                                                {if m == DateRangeMode::Week {
                                                    days.chunks(7).map(|week| {
                                                        let mon = week[0];
                                                        let cell = CellKind::Week(mon);
                                                        let cls = cell_class(cell, sel_from, sel_to);
                                                        let week_days: Vec<_> = week.to_vec();
                                                        view! {
                                                            <button
                                                                type="button"
                                                                class=format!("{cls} date-range-picker__cell--week-row")
                                                                on:click=move |_| on_cell_click(cell)
                                                            >
                                                                {week_days.into_iter().map(|d| {
                                                                    let outside = d.month() != month;
                                                                    view! {
                                                                        <span class=if outside {
                                                                            "date-range-picker__day date-range-picker__day--muted"
                                                                        } else {
                                                                            "date-range-picker__day"
                                                                        }>
                                                                            {d.day().to_string()}
                                                                        </span>
                                                                    }
                                                                }).collect_view()}
                                                            </button>
                                                        }
                                                    }).collect_view().into_any()
                                                } else {
                                                    days.into_iter().map(|d| {
                                                        let cell = CellKind::Day(d);
                                                        let base = cell_class(cell, sel_from, sel_to);
                                                        let outside = d.month() != month;
                                                        let cls = if outside {
                                                            format!("{base} date-range-picker__cell--muted")
                                                        } else {
                                                            base.to_string()
                                                        };
                                                        view! {
                                                            <button
                                                                type="button"
                                                                class=cls
                                                                on:click=move |_| on_cell_click(cell)
                                                            >
                                                                {d.day().to_string()}
                                                            </button>
                                                        }
                                                    }).collect_view().into_any()
                                                }}
                                            </div>
                                        }.into_any()
                                    }
                                }
                            }}
                            <p
                                class=move || {
                                    if awaiting_end.get() {
                                        "date-range-picker__hint"
                                    } else {
                                        "date-range-picker__hint date-range-picker__hint--hidden"
                                    }
                                }
                            >
                                "Выберите конец периода или нажмите OK"
                            </p>
                        </div>
                    </DialogContent>
                    <DialogActions>
                        <Button
                            appearance=ButtonAppearance::Primary
                            on_click=apply_dialog
                        >
                            "OK"
                        </Button>
                        <Button
                            appearance=ButtonAppearance::Subtle
                            on_click=cancel_dialog
                        >
                            "Отмена"
                        </Button>
                    </DialogActions>
                </DialogBody>
            </DialogSurface>
        </Dialog>
    }
}
