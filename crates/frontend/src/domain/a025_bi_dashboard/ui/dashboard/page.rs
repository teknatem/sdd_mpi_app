use crate::app::ThawThemeContext;
use crate::data_view::api as dv_api;
use crate::data_view::types::{FilterDef, FilterKind, FilterRef};
use crate::data_view::ui::filter_bar::apply_defaults;
use crate::data_view::ui::FilterBar;
use crate::general_ledger::ui::dimension_chip::{chip_from_code_main, GlDimensionChip};
use crate::layout::global_context::AppGlobalContext;
use crate::shared::api_utils::api_base;
use crate::shared::bi_card::{
    available_designs, default_design_name, get_style_css, render_card_html, IndicatorCardParams,
};
use crate::shared::icons::icon;
use crate::shared::indicator_format::{format_int_with_triads, format_money_with_format_spec};
use crate::shared::page_frame::PageFrame;
use chrono::{Datelike, Duration, NaiveDate, Utc};
use contracts::domain::a006_connection_mp::aggregate::ConnectionMP;
use contracts::domain::common::AggregateId;
use contracts::shared::data_view::ViewContext;
use gloo_net::http::Request;
use gloo_timers::future::TimeoutFuture;
use leptos::prelude::window_event_listener;
use leptos::prelude::*;
use std::collections::HashMap;
use thaw::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::spawn_local;

#[derive(Clone, Debug, serde::Deserialize)]
#[allow(dead_code)]
struct DashboardItem {
    pub indicator_id: String,
    #[serde(default)]
    pub indicator_name: String,
    #[serde(default)]
    pub sort_order: i32,
    #[serde(default = "default_col_class")]
    pub col_class: String,
    #[serde(default)]
    pub param_overrides: HashMap<String, String>,
}

fn default_col_class() -> String {
    "1x1".to_string()
}

#[derive(Clone, Debug, serde::Deserialize)]
struct DashboardGroup {
    #[allow(dead_code)]
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub sort_order: i32,
    #[serde(default)]
    pub items: Vec<DashboardItem>,
    #[serde(default)]
    pub subgroups: Vec<DashboardGroup>,
}

#[derive(Clone, Debug, serde::Deserialize)]
struct DashboardLayout {
    #[serde(default)]
    pub groups: Vec<DashboardGroup>,
}

#[derive(Clone, Debug, Default, serde::Deserialize)]
struct IndicatorViewSpec {
    #[serde(default)]
    pub custom_css: Option<String>,
    #[serde(default)]
    pub format: serde_json::Value,
    #[serde(default)]
    pub preview_values: HashMap<String, String>,
}

#[derive(Clone, Debug, Default, serde::Deserialize)]
struct IndicatorDataSpec {
    #[serde(default)]
    pub view_id: Option<String>,
    #[serde(default)]
    pub metric_id: Option<String>,
}

#[derive(Clone, Debug, Default, serde::Deserialize)]
struct IndicatorParamDef {
    #[serde(default)]
    pub key: String,
    #[serde(default)]
    pub default_value: Option<String>,
}

#[derive(Clone, Debug, serde::Deserialize)]
struct IndicatorDef {
    pub id: String,
    #[serde(default)]
    pub code: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub comment: Option<String>,
    #[serde(default)]
    pub view_spec: IndicatorViewSpec,
    #[serde(default)]
    pub data_spec: IndicatorDataSpec,
    #[serde(default)]
    pub params: Vec<IndicatorParamDef>,
}

/// Computed value from /api/a024-bi-indicator/compute-batch
#[derive(Clone, Debug, Default, serde::Deserialize)]
struct ComputedValue {
    /// id сериализуется как строка (IndicatorId — newtype over String)
    pub id: String,
    pub value: Option<f64>,
    pub previous_value: Option<f64>,
    pub change_percent: Option<f64>,
    /// "Good" | "Bad" | "Neutral" | "Warning"
    pub status: Option<String>,
    #[serde(default)]
    pub subtitle: Option<String>,
    #[serde(default)]
    pub details: Vec<String>,
    /// Daily values for period 1 (for sparkline). Empty when not available.
    #[serde(default)]
    pub spark_points: Vec<f64>,
}

#[derive(Clone, Debug, serde::Deserialize)]
struct BiDashboardData {
    #[allow(dead_code)]
    pub id: String,
    pub code: String,
    pub description: String,
    pub layout: DashboardLayout,
    #[serde(default)]
    pub filters: Vec<FilterRef>,
}

/// A single drill-down dimension a user can group an indicator by.
/// Carries structured coverage info so the UI can render it as a colored
/// badge instead of baking "[100% safe]" into the label text.
#[derive(Clone, Debug, PartialEq)]
struct DrillDim {
    id: String,
    label: String,
    /// "safe" → fully covers the indicator; "partial" → covers `coverage_pct`,
    /// the remainder lands in a "Прочее" bucket.
    mode: String,
    coverage_pct: f64,
}

#[derive(Clone, Debug, PartialEq)]
struct DrillDimensionGroup {
    title: &'static str,
    /// CSS accent key ("main" | "docs" | "nom" | "other") for per-group color.
    accent: &'static str,
    items: Vec<DrillDim>,
}

/// Maps a drill dimension id to the GL dimension `code_main`, so the picker can
/// render the same standard dimension badge as the «Измерения GL» catalog
/// (see [`crate::general_ledger::ui::dimension_chip`]). Unknown ids fall back to
/// a neutral chip.
fn drill_dim_code_main(id: &str) -> &'static str {
    match id {
        "turnover" | "turnover_code" => "Turn",
        "entry_date" => "Day",
        "connection_mp_ref" => "Cab",
        "layer" => "Layer",
        "registrator_type" => "RegType",
        "registrator_ref" => "RegRef",
        "nomenclature" | "dim1_category" | "dim2_line" | "dim3_model" | "dim4_format"
        | "dim5_sink" | "dim6_size" => "Nom",
        "debit_account" => "Dr",
        "credit_account" => "Cr",
        "customer_kind" => "uf",
        "fulfillment_type" => "fulf",
        _ => "default",
    }
}

/// Число колонок для сетки групп: не более 3, но строки заполняются равномерно,
/// чтобы 4 группы шли 2+2, а 5 — 3+2 (а не «3 + одинокая»).
fn balanced_group_columns(group_count: usize) -> usize {
    if group_count <= 1 {
        return 1;
    }
    let rows = group_count.div_ceil(3);
    group_count.div_ceil(rows)
}

#[derive(Clone, Debug)]
struct DashboardMpOption {
    id: String,
    label: String,
}

const DASHBOARD_PERIOD_MODE_PARAM: &str = "dashboard_period_mode";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DashboardPeriodMode {
    Month,
    Week,
    Day,
    Custom,
}

impl DashboardPeriodMode {
    fn as_param_value(self) -> &'static str {
        match self {
            Self::Month => "month",
            Self::Week => "week",
            Self::Day => "day",
            Self::Custom => "custom",
        }
    }

    fn from_param(value: &str) -> Option<Self> {
        match value.trim() {
            "month" => Some(Self::Month),
            "week" => Some(Self::Week),
            "day" => Some(Self::Day),
            "custom" => Some(Self::Custom),
            _ => None,
        }
    }

    fn from_ctx(ctx: &ViewContext) -> Self {
        ctx.params
            .get(DASHBOARD_PERIOD_MODE_PARAM)
            .and_then(|value| Self::from_param(value))
            .unwrap_or_else(|| infer_dashboard_period_mode(ctx))
    }

    fn title(self) -> &'static str {
        match self {
            Self::Month => "Месяц",
            Self::Week => "Неделя",
            Self::Day => "День",
            Self::Custom => "Период",
        }
    }
}

fn fmt_ymd(date: NaiveDate) -> String {
    date.format("%Y-%m-%d").to_string()
}

fn parse_ymd(value: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(value, "%Y-%m-%d").ok()
}

fn month_bounds(anchor: NaiveDate) -> (NaiveDate, NaiveDate) {
    let start = NaiveDate::from_ymd_opt(anchor.year(), anchor.month(), 1).unwrap_or(anchor);
    let end = if anchor.month() == 12 {
        NaiveDate::from_ymd_opt(anchor.year() + 1, 1, 1)
            .map(|date| date - Duration::days(1))
            .unwrap_or(anchor)
    } else {
        NaiveDate::from_ymd_opt(anchor.year(), anchor.month() + 1, 1)
            .map(|date| date - Duration::days(1))
            .unwrap_or(anchor)
    };
    (start, end)
}

fn shift_month_anchor(anchor: NaiveDate, delta: i32) -> NaiveDate {
    let month_index = anchor.year() * 12 + anchor.month0() as i32 + delta;
    let year = month_index.div_euclid(12);
    let month0 = month_index.rem_euclid(12) as u32;
    NaiveDate::from_ymd_opt(year, month0 + 1, 1).unwrap_or(anchor)
}

fn week_bounds(anchor: NaiveDate) -> (NaiveDate, NaiveDate) {
    let start = anchor - Duration::days(anchor.weekday().num_days_from_monday() as i64);
    let end = start + Duration::days(6);
    (start, end)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DashboardPeriodSlot {
    Primary,
    Comparison,
}

impl DashboardPeriodSlot {
    fn title(self) -> &'static str {
        match self {
            Self::Primary => "Период 1",
            Self::Comparison => "Период 2",
        }
    }

    fn reset_label(self, mode: DashboardPeriodMode) -> &'static str {
        match (self, mode) {
            (Self::Primary, DashboardPeriodMode::Month) => "Текущий",
            (Self::Comparison, DashboardPeriodMode::Month) => "Прошлый",
            (Self::Primary, DashboardPeriodMode::Week) => "Текущая",
            (Self::Comparison, DashboardPeriodMode::Week) => "Прошлая",
            (Self::Primary, DashboardPeriodMode::Day) => "Сегодня",
            (Self::Comparison, DashboardPeriodMode::Day) => "Вчера",
            (_, DashboardPeriodMode::Custom) => "Сброс",
        }
    }
}

fn canonical_range_for_mode(
    mode: DashboardPeriodMode,
    anchor: NaiveDate,
) -> (NaiveDate, NaiveDate) {
    match mode {
        DashboardPeriodMode::Month => month_bounds(anchor),
        DashboardPeriodMode::Week => week_bounds(anchor),
        DashboardPeriodMode::Day | DashboardPeriodMode::Custom => (anchor, anchor),
    }
}

fn period_slot_dates(
    ctx: &ViewContext,
    slot: DashboardPeriodSlot,
) -> Option<(NaiveDate, NaiveDate)> {
    match slot {
        DashboardPeriodSlot::Primary => parse_ymd(&ctx.date_from).zip(parse_ymd(&ctx.date_to)),
        DashboardPeriodSlot::Comparison => ctx
            .period2_from
            .as_deref()
            .and_then(parse_ymd)
            .zip(ctx.period2_to.as_deref().and_then(parse_ymd)),
    }
}

fn set_period_slot(
    ctx: &mut ViewContext,
    slot: DashboardPeriodSlot,
    from: NaiveDate,
    to: NaiveDate,
) {
    match slot {
        DashboardPeriodSlot::Primary => {
            ctx.date_from = fmt_ymd(from);
            ctx.date_to = fmt_ymd(to);
        }
        DashboardPeriodSlot::Comparison => {
            ctx.period2_from = Some(fmt_ymd(from));
            ctx.period2_to = Some(fmt_ymd(to));
        }
    }
}

fn short_date_label(date: NaiveDate) -> String {
    date.format("%d.%m").to_string()
}

fn compact_period_label(mode: DashboardPeriodMode, from: NaiveDate, to: NaiveDate) -> String {
    match mode {
        DashboardPeriodMode::Month if is_full_month_range(from, to) => {
            month_abbr(from.month()).to_string()
        }
        _ if from == to => short_date_label(from),
        _ => format!("{} - {}", short_date_label(from), short_date_label(to)),
    }
}

fn period_control_label(mode: DashboardPeriodMode, from: NaiveDate, to: NaiveDate) -> String {
    match mode {
        DashboardPeriodMode::Month if is_full_month_range(from, to) => {
            format!("{} {}", month_abbr(from.month()), from.year())
        }
        _ if from == to => from.format("%d.%m.%Y").to_string(),
        _ => format!("{} - {}", short_date_label(from), short_date_label(to)),
    }
}

fn is_full_month_range(from: NaiveDate, to: NaiveDate) -> bool {
    let (month_start, month_end) = month_bounds(from);
    month_start == from && month_end == to
}

fn is_full_week_range(from: NaiveDate, to: NaiveDate) -> bool {
    from.weekday().num_days_from_monday() == 0 && to == from + Duration::days(6)
}

fn infer_dashboard_period_mode(ctx: &ViewContext) -> DashboardPeriodMode {
    let Some(date_from) = parse_ymd(&ctx.date_from) else {
        return DashboardPeriodMode::Month;
    };
    let Some(date_to) = parse_ymd(&ctx.date_to) else {
        return DashboardPeriodMode::Month;
    };

    if is_full_month_range(date_from, date_to)
        && period_slot_dates(ctx, DashboardPeriodSlot::Comparison)
            .map(|(from, to)| is_full_month_range(from, to))
            .unwrap_or(true)
    {
        return DashboardPeriodMode::Month;
    }

    if is_full_week_range(date_from, date_to)
        && period_slot_dates(ctx, DashboardPeriodSlot::Comparison)
            .map(|(from, to)| is_full_week_range(from, to))
            .unwrap_or(true)
    {
        return DashboardPeriodMode::Week;
    }

    if date_from == date_to
        && period_slot_dates(ctx, DashboardPeriodSlot::Comparison)
            .map(|(from, to)| from == to)
            .unwrap_or(true)
    {
        return DashboardPeriodMode::Day;
    }

    DashboardPeriodMode::Custom
}

fn sync_dashboard_period_mode(ctx: &mut ViewContext) {
    let mode = ctx
        .params
        .get(DASHBOARD_PERIOD_MODE_PARAM)
        .and_then(|value| DashboardPeriodMode::from_param(value))
        .unwrap_or_else(|| infer_dashboard_period_mode(ctx));
    ctx.params.insert(
        DASHBOARD_PERIOD_MODE_PARAM.to_string(),
        mode.as_param_value().to_string(),
    );
}

fn shift_period_anchor(mode: DashboardPeriodMode, anchor: NaiveDate, delta: i32) -> NaiveDate {
    match mode {
        DashboardPeriodMode::Month => shift_month_anchor(anchor, delta),
        DashboardPeriodMode::Week => anchor + Duration::days((delta as i64) * 7),
        DashboardPeriodMode::Day => anchor + Duration::days(delta as i64),
        DashboardPeriodMode::Custom => anchor,
    }
}

fn period_slot_anchor(
    ctx: &ViewContext,
    slot: DashboardPeriodSlot,
    mode: DashboardPeriodMode,
) -> NaiveDate {
    let primary_anchor = period_slot_dates(ctx, DashboardPeriodSlot::Primary)
        .map(|(_, to)| to)
        .unwrap_or_else(|| Utc::now().date_naive());
    period_slot_dates(ctx, slot)
        .map(|(_, to)| to)
        .unwrap_or_else(|| match slot {
            DashboardPeriodSlot::Primary => primary_anchor,
            DashboardPeriodSlot::Comparison => shift_period_anchor(mode, primary_anchor, -1),
        })
}

fn apply_period_mode(ctx: &mut ViewContext, mode: DashboardPeriodMode, anchor: NaiveDate) {
    if mode != DashboardPeriodMode::Custom {
        let primary_anchor = period_slot_dates(ctx, DashboardPeriodSlot::Primary)
            .map(|(_, to)| to)
            .unwrap_or(anchor);
        let comparison_anchor = period_slot_anchor(ctx, DashboardPeriodSlot::Comparison, mode);
        let (p1_from, p1_to) = canonical_range_for_mode(mode, primary_anchor);
        let (p2_from, p2_to) = canonical_range_for_mode(mode, comparison_anchor);
        set_period_slot(ctx, DashboardPeriodSlot::Primary, p1_from, p1_to);
        set_period_slot(ctx, DashboardPeriodSlot::Comparison, p2_from, p2_to);
    } else {
        let primary_anchor =
            period_slot_anchor(ctx, DashboardPeriodSlot::Primary, DashboardPeriodMode::Day);
        let comparison_anchor = period_slot_anchor(
            ctx,
            DashboardPeriodSlot::Comparison,
            DashboardPeriodMode::Day,
        );
        if ctx.date_from.trim().is_empty() {
            ctx.date_from = fmt_ymd(primary_anchor);
        }
        if ctx.date_to.trim().is_empty() {
            ctx.date_to = ctx.date_from.clone();
        }
        if ctx.period2_from.is_none() {
            ctx.period2_from = Some(fmt_ymd(comparison_anchor));
        }
        if ctx.period2_to.is_none() {
            ctx.period2_to = ctx.period2_from.clone();
        }
    }
    ctx.params.insert(
        DASHBOARD_PERIOD_MODE_PARAM.to_string(),
        mode.as_param_value().to_string(),
    );
}

fn period_summary(ctx: &ViewContext, slot: DashboardPeriodSlot, compact: bool) -> Option<String> {
    let mode = DashboardPeriodMode::from_ctx(ctx);
    let (from, to) = period_slot_dates(ctx, slot)?;
    Some(if compact {
        compact_period_label(mode, from, to)
    } else {
        period_control_label(mode, from, to)
    })
}

fn non_period_dashboard_filters(filters: &[FilterDef]) -> Vec<FilterDef> {
    filters
        .iter()
        .filter(|def| !matches!(def.kind, FilterKind::DateRange { .. }))
        .filter(|def| def.id != "connection_mp_refs")
        .cloned()
        .collect()
}

fn indicator_default_params(def: &IndicatorDef) -> HashMap<String, String> {
    def.params
        .iter()
        .filter_map(|param| {
            let value = param.default_value.as_ref()?.trim();
            if param.key.trim().is_empty() || value.is_empty() {
                None
            } else {
                Some((param.key.clone(), value.to_string()))
            }
        })
        .collect()
}

async fn fetch_indicator_drill_dimensions(
    def: &IndicatorDef,
    ctx: &ViewContext,
    params: &HashMap<String, String>,
) -> Result<Vec<DrillDim>, String> {
    let Some(view_id) = def.data_spec.view_id.as_deref() else {
        return Ok(vec![]);
    };

    let mut drill_ctx = ctx.clone();
    drill_ctx.params = params.clone();
    let capabilities =
        dv_api::fetch_drilldown_capabilities(view_id, &drill_ctx, def.data_spec.metric_id.clone())
            .await?;

    Ok(capabilities
        .safe_dimensions
        .into_iter()
        .map(|dim| DrillDim {
            id: dim.id,
            label: dim.label,
            mode: "safe".to_string(),
            coverage_pct: dim.coverage_pct.unwrap_or(100.0),
        })
        .chain(
            capabilities
                .partial_dimensions
                .into_iter()
                .map(|dim| DrillDim {
                    id: dim.id,
                    label: dim.label,
                    mode: "partial".to_string(),
                    coverage_pct: dim.coverage_pct.unwrap_or(0.0),
                }),
        )
        .collect())
}

fn merge_indicator_params(
    defaults: &HashMap<String, String>,
    overrides: &HashMap<String, String>,
) -> HashMap<String, String> {
    let mut merged = defaults.clone();
    merged.extend(overrides.clone());
    merged
}

fn group_drill_dimensions(dims: &[DrillDim]) -> Vec<DrillDimensionGroup> {
    let classify = |id: &str| match id {
        "turnover" | "entry_date" | "connection_mp_ref" | "layer" => 0,
        "registrator_type" | "registrator_ref" => 1,
        "nomenclature" | "dim1_category" | "dim2_line" | "dim3_model" | "dim4_format"
        | "dim5_sink" | "dim6_size" => 2,
        _ => 3,
    };

    let mut buckets: [Vec<DrillDim>; 4] = [vec![], vec![], vec![], vec![]];
    for dim in dims {
        buckets[classify(&dim.id)].push(dim.clone());
    }

    let specs = [
        ("Основные", "main", 0),
        ("Документы", "docs", 1),
        ("Номенклатура", "nom", 2),
        ("Другое", "other", 3),
    ];

    specs
        .into_iter()
        .filter_map(|(title, accent, index)| {
            let items = std::mem::take(&mut buckets[index]);
            if items.is_empty() {
                None
            } else {
                Some(DrillDimensionGroup {
                    title,
                    accent,
                    items,
                })
            }
        })
        .collect()
}

/// Reads a non-OK HTTP response and returns a human-readable error string.
/// For 403 responses that carry the backend's `access_denied` JSON body,
/// formats the scope name and required access level in Russian.
async fn read_http_error(resp: web_sys::Response) -> String {
    let status = resp.status();
    if status == 403 {
        if let Ok(promise) = resp.text() {
            if let Ok(val) = wasm_bindgen_futures::JsFuture::from(promise).await {
                if let Some(text) = val.as_string() {
                    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
                        if json["error"].as_str() == Some("access_denied") {
                            let scope = json["scope_id"].as_str().unwrap_or("неизвестен");
                            let access = match json["required_access"].as_str().unwrap_or("all") {
                                "read" => "чтение",
                                _ => "полный доступ",
                            };
                            return format!(
                                "Доступ запрещён: недостаточно прав для «{}» (требуется: {})",
                                scope, access
                            );
                        }
                    }
                }
            }
        }
        return "Доступ запрещён (403 Forbidden)".to_string();
    }
    format!("Ошибка HTTP {}", status)
}

async fn fetch_dashboard(id: &str) -> Result<serde_json::Value, String> {
    use web_sys::{Request, RequestInit, RequestMode, Response};

    let opts = RequestInit::new();
    opts.set_method("GET");
    opts.set_mode(RequestMode::Cors);

    let url = format!("{}/api/a025-bi-dashboard/{}", api_base(), id);
    let request = Request::new_with_str_and_init(&url, &opts).map_err(|e| format!("{e:?}"))?;
    request
        .headers()
        .set("Accept", "application/json")
        .map_err(|e| format!("{e:?}"))?;

    let window = web_sys::window().ok_or_else(|| "no window".to_string())?;
    let resp_value = wasm_bindgen_futures::JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(|e| format!("{e:?}"))?;
    let resp: Response = resp_value.dyn_into().map_err(|e| format!("{e:?}"))?;

    if !resp.ok() {
        return Err(read_http_error(resp).await);
    }

    let text = wasm_bindgen_futures::JsFuture::from(resp.text().map_err(|e| format!("{e:?}"))?)
        .await
        .map_err(|e| format!("{e:?}"))?;
    let text: String = text.as_string().ok_or_else(|| "bad text".to_string())?;
    serde_json::from_str(&text).map_err(|e| format!("{e}"))
}

async fn fetch_dashboard_mp_options() -> Result<Vec<DashboardMpOption>, String> {
    let url = format!("{}/api/connection_mp", api_base());
    let resp = Request::get(&url)
        .send()
        .await
        .map_err(|e| format!("{e}"))?;
    if !resp.ok() {
        return Err(format!("HTTP {}", resp.status()));
    }
    let items: Vec<ConnectionMP> = resp.json().await.map_err(|e| format!("{e}"))?;

    let mut options: Vec<DashboardMpOption> = items
        .into_iter()
        .map(|conn| {
            let label = if conn.base.description.trim().is_empty() {
                conn.base.code.clone()
            } else {
                conn.base.description.clone()
            };
            DashboardMpOption {
                id: conn.base.id.as_string(),
                label,
            }
        })
        .collect();
    options.sort_by(|left, right| left.label.cmp(&right.label));
    Ok(options)
}

fn collect_indicator_ids(
    groups: &[DashboardGroup],
    out: &mut Vec<String>,
    seen: &mut std::collections::HashSet<String>,
) {
    for group in groups {
        for item in &group.items {
            if seen.insert(item.indicator_id.clone()) {
                out.push(item.indicator_id.clone());
            }
        }
        collect_indicator_ids(&group.subgroups, out, seen);
    }
}

async fn fetch_indicator_defs(ids: &[String]) -> Result<HashMap<String, IndicatorDef>, String> {
    use web_sys::{Request, RequestInit, RequestMode, Response};

    if ids.is_empty() {
        return Ok(HashMap::new());
    }

    let opts = RequestInit::new();
    opts.set_method("POST");
    opts.set_mode(RequestMode::Cors);
    let body = serde_json::json!({ "ids": ids });
    opts.set_body(&wasm_bindgen::JsValue::from_str(&body.to_string()));

    let url = format!("{}/api/a024-bi-indicator/resolve-batch", api_base());
    let request = Request::new_with_str_and_init(&url, &opts).map_err(|e| format!("{e:?}"))?;
    request
        .headers()
        .set("Accept", "application/json")
        .map_err(|e| format!("{e:?}"))?;
    request
        .headers()
        .set("Content-Type", "application/json")
        .map_err(|e| format!("{e:?}"))?;

    let window = web_sys::window().ok_or_else(|| "no window".to_string())?;
    let resp_value = wasm_bindgen_futures::JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(|e| format!("{e:?}"))?;
    let resp: Response = resp_value.dyn_into().map_err(|e| format!("{e:?}"))?;

    if !resp.ok() {
        return Err(read_http_error(resp).await);
    }

    let text = wasm_bindgen_futures::JsFuture::from(resp.text().map_err(|e| format!("{e:?}"))?)
        .await
        .map_err(|e| format!("{e:?}"))?;
    let text: String = text.as_string().ok_or_else(|| "bad text".to_string())?;
    let items: Vec<serde_json::Value> = serde_json::from_str(&text).map_err(|e| format!("{e}"))?;
    let mut out = HashMap::new();
    for item in items {
        let Ok(def) = serde_json::from_value::<IndicatorDef>(item) else {
            continue;
        };
        out.insert(def.id.clone(), def);
    }

    Ok(out)
}

fn resolve_dashboard_filters(filter_refs: &[FilterRef], registry: &[FilterDef]) -> Vec<FilterDef> {
    let registry_map: HashMap<&str, &FilterDef> =
        registry.iter().map(|def| (def.id.as_str(), def)).collect();

    let mut refs = filter_refs.to_vec();
    refs.sort_by_key(|r| r.order);

    refs.into_iter()
        .filter_map(|filter_ref| {
            let mut def = (*registry_map.get(filter_ref.filter_id.as_str())?).clone();
            if let Some(label_override) = filter_ref.label_override {
                if !label_override.trim().is_empty() {
                    def.label = label_override;
                }
            }
            Some(def)
        })
        .collect()
}

async fn derive_dashboard_filters_from_indicators(
    indicator_defs: &HashMap<String, IndicatorDef>,
) -> Vec<FilterRef> {
    let mut view_ids: Vec<String> = indicator_defs
        .values()
        .filter_map(|def| def.data_spec.view_id.clone())
        .filter(|view_id| !view_id.trim().is_empty())
        .collect();
    view_ids.sort();
    view_ids.dedup();

    let mut merged: Vec<FilterRef> = Vec::new();

    for view_id in view_ids {
        match dv_api::fetch_by_id(&view_id).await {
            Ok(meta) => {
                let mut refs = meta.filters;
                refs.sort_by_key(|filter_ref| filter_ref.order);
                for filter_ref in refs {
                    if let Some(existing) = merged
                        .iter_mut()
                        .find(|existing| existing.filter_id == filter_ref.filter_id)
                    {
                        existing.required |= filter_ref.required;
                        if existing
                            .default_value
                            .as_deref()
                            .unwrap_or("")
                            .trim()
                            .is_empty()
                        {
                            existing.default_value = filter_ref.default_value.clone();
                        }
                        if existing
                            .label_override
                            .as_deref()
                            .unwrap_or("")
                            .trim()
                            .is_empty()
                        {
                            existing.label_override = filter_ref.label_override.clone();
                        }
                        existing.order = existing.order.min(filter_ref.order);
                    } else {
                        merged.push(filter_ref);
                    }
                }
            }
            Err(err) => {
                leptos::logging::warn!(
                    "Failed to derive dashboard filters from DataView {}: {}",
                    view_id,
                    err
                );
            }
        }
    }

    merged.sort_by(|a, b| {
        a.order
            .cmp(&b.order)
            .then_with(|| a.filter_id.cmp(&b.filter_id))
    });
    for (idx, filter_ref) in merged.iter_mut().enumerate() {
        filter_ref.order = idx as u32;
    }
    merged
}

fn default_dashboard_ctx() -> ViewContext {
    let now = Utc::now().date_naive();
    let current_month_start = NaiveDate::from_ymd_opt(now.year(), now.month(), 1).unwrap_or(now);
    let current_month_end = if now.month() == 12 {
        NaiveDate::from_ymd_opt(now.year() + 1, 1, 1)
            .map(|d| d - Duration::days(1))
            .unwrap_or(now)
    } else {
        NaiveDate::from_ymd_opt(now.year(), now.month() + 1, 1)
            .map(|d| d - Duration::days(1))
            .unwrap_or(now)
    };
    let (prev_year, prev_month) = if now.month() == 1 {
        (now.year() - 1, 12)
    } else {
        (now.year(), now.month() - 1)
    };
    let previous_month_start =
        NaiveDate::from_ymd_opt(prev_year, prev_month, 1).unwrap_or(current_month_start);
    let previous_month_end = current_month_start - Duration::days(1);

    let mut ctx = ViewContext {
        date_from: current_month_start.format("%Y-%m-%d").to_string(),
        date_to: current_month_end.format("%Y-%m-%d").to_string(),
        period2_from: Some(previous_month_start.format("%Y-%m-%d").to_string()),
        period2_to: Some(previous_month_end.format("%Y-%m-%d").to_string()),
        connection_mp_refs: vec![],
        params: HashMap::new(),
    };
    sync_dashboard_period_mode(&mut ctx);
    ctx
}

fn merge_view_ctx(default_ctx: ViewContext, prev_ctx: ViewContext) -> ViewContext {
    let mut merged = default_ctx;
    if !prev_ctx.date_from.trim().is_empty() {
        merged.date_from = prev_ctx.date_from;
    }
    if !prev_ctx.date_to.trim().is_empty() {
        merged.date_to = prev_ctx.date_to;
    }
    if prev_ctx.period2_from.is_some() {
        merged.period2_from = prev_ctx.period2_from;
    }
    if prev_ctx.period2_to.is_some() {
        merged.period2_to = prev_ctx.period2_to;
    }
    if !prev_ctx.connection_mp_refs.is_empty() {
        merged.connection_mp_refs = prev_ctx.connection_mp_refs;
    }
    merged.params.extend(prev_ctx.params);
    sync_dashboard_period_mode(&mut merged);
    merged
}

/// Compute dashboard indicator values through /api/a024-bi-indicator/compute-batch.
/// Returns `Err` with a human-readable message on HTTP errors (including 403 access denied).
async fn fetch_indicator_data(
    indicator_defs: &HashMap<String, IndicatorDef>,
    ctx: &ViewContext,
) -> Result<HashMap<String, ComputedValue>, String> {
    use web_sys::{Request, RequestInit, RequestMode, Response};

    let indicator_ids: Vec<String> = indicator_defs.keys().cloned().collect();
    if indicator_ids.is_empty() {
        return Ok(HashMap::new());
    }

    let body = serde_json::json!({
        "indicator_ids": indicator_ids,
        "date_from": ctx.date_from,
        "date_to": ctx.date_to,
        "period2_from": ctx.period2_from,
        "period2_to": ctx.period2_to,
        "connection_mp_refs": ctx.connection_mp_refs.join(","),
        "params": ctx.params,
    });

    let opts = RequestInit::new();
    opts.set_method("POST");
    opts.set_mode(RequestMode::Cors);
    let body_str = body.to_string();
    opts.set_body(&wasm_bindgen::JsValue::from_str(&body_str));

    let window = web_sys::window().ok_or_else(|| "no window".to_string())?;
    let request = Request::new_with_str_and_init(
        &format!("{}/api/a024-bi-indicator/compute-batch", api_base()),
        &opts,
    )
    .map_err(|e| format!("{e:?}"))?;
    let _ = request.headers().set("Accept", "application/json");
    let _ = request.headers().set("Content-Type", "application/json");

    let resp_val = wasm_bindgen_futures::JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(|e| format!("{e:?}"))?;
    let resp: Response = resp_val.dyn_into().map_err(|e| format!("{e:?}"))?;

    if !resp.ok() {
        return Err(read_http_error(resp).await);
    }

    let text_promise = resp.text().map_err(|e| format!("{e:?}"))?;
    let text_val = wasm_bindgen_futures::JsFuture::from(text_promise)
        .await
        .map_err(|e| format!("{e:?}"))?;
    let text = text_val.as_string().ok_or_else(|| "bad text".to_string())?;

    let parsed: serde_json::Value = serde_json::from_str(&text).map_err(|e| format!("{e}"))?;

    let values = parsed["values"].as_array().cloned().unwrap_or_default();
    let mut result: HashMap<String, ComputedValue> = HashMap::new();
    for val in values {
        if let Ok(cv) = serde_json::from_value::<ComputedValue>(val) {
            if !cv.id.is_empty() {
                result.insert(cv.id.clone(), cv);
            }
        }
    }
    Ok(result)
}

/// Вычислить DataView-индикаторы через /api/a024-bi-indicator/:id/compute
/// (те у которых задан data_spec.view_id)
fn reload_dashboard_data(
    id: String,
    loading: RwSignal<bool>,
    error: RwSignal<Option<String>>,
    dashboard: RwSignal<Option<BiDashboardData>>,
    view_ctx: RwSignal<ViewContext>,
    dashboard_filter_defs: RwSignal<Vec<FilterDef>>,
    indicator_defs: RwSignal<HashMap<String, IndicatorDef>>,
    indicator_values: RwSignal<HashMap<String, ComputedValue>>,
    preserve_session_filters: bool,
) {
    leptos::task::spawn_local(async move {
        loading.set(true);
        error.set(None);

        match fetch_dashboard(&id).await {
            Ok(raw) => match serde_json::from_value::<BiDashboardData>(raw) {
                Ok(data) => {
                    let mut ids = Vec::new();
                    let mut seen = std::collections::HashSet::new();
                    collect_indicator_ids(&data.layout.groups, &mut ids, &mut seen);
                    let defs = match fetch_indicator_defs(&ids).await {
                        Ok(d) => d,
                        Err(e) => {
                            error.set(Some(e));
                            loading.set(false);
                            return;
                        }
                    };

                    let effective_filter_refs = if data.filters.is_empty() {
                        derive_dashboard_filters_from_indicators(&defs).await
                    } else {
                        data.filters.clone()
                    };

                    let registry = dv_api::fetch_global_filters().await.unwrap_or_default();
                    let resolved_filters =
                        resolve_dashboard_filters(&effective_filter_refs, &registry);
                    dashboard_filter_defs.set(resolved_filters.clone());

                    let mut next_ctx = default_dashboard_ctx();
                    for filter_ref in &effective_filter_refs {
                        if let Some(default_value) = &filter_ref.default_value {
                            apply_defaults(&mut next_ctx, &filter_ref.filter_id, default_value);
                        }
                    }
                    if preserve_session_filters {
                        next_ctx = merge_view_ctx(next_ctx, view_ctx.get_untracked());
                    } else {
                        let inferred_mode = infer_dashboard_period_mode(&next_ctx);
                        next_ctx.params.insert(
                            DASHBOARD_PERIOD_MODE_PARAM.to_string(),
                            inferred_mode.as_param_value().to_string(),
                        );
                    }
                    sync_dashboard_period_mode(&mut next_ctx);
                    view_ctx.set(next_ctx);

                    // Индикаторы пересчитываются отдельным reactive-effect,
                    // чтобы все карточки обновлялись атомарно на один и тот же ctx.
                    indicator_values.set(HashMap::new());

                    indicator_defs.set(defs);
                    dashboard.set(Some(data));
                }
                Err(e) => error.set(Some(format!("Ошибка парсинга: {}", e))),
            },
            Err(e) => error.set(Some(e)),
        }

        loading.set(false);
    });
}

fn get_app_theme() -> String {
    #[cfg(target_arch = "wasm32")]
    {
        let theme = web_sys::window()
            .and_then(|w| w.local_storage().ok().flatten())
            .and_then(|s| s.get_item("app_theme").ok().flatten())
            .unwrap_or_default();
        crate::shared::theme::registry::theme_by_id(&theme)
            .base
            .as_str()
            .to_string()
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        "dark".to_string()
    }
}

fn get_sidebar_scrollbar_tokens() -> (String, String) {
    let default_thumb = "rgba(0, 0, 0, 0.18)".to_string();
    let default_hover = "rgba(0, 0, 0, 0.28)".to_string();

    #[cfg(target_arch = "wasm32")]
    {
        let Some(window) = web_sys::window() else {
            return (default_thumb, default_hover);
        };
        let Some(document) = window.document() else {
            return (default_thumb, default_hover);
        };
        let Some(root) = document.document_element() else {
            return (default_thumb, default_hover);
        };
        let Ok(Some(style)) = window.get_computed_style(&root) else {
            return (default_thumb, default_hover);
        };

        // Use --list-scrollbar-thumb: it is properly overridden per theme
        // (dark.css defines white-based values; sidebar token is never overridden in dark mode).
        let thumb = style
            .get_property_value("--list-scrollbar-thumb")
            .ok()
            .map(|v| v.trim().to_string())
            .unwrap_or_default();
        let hover = style
            .get_property_value("--list-scrollbar-thumb-hover")
            .ok()
            .map(|v| v.trim().to_string())
            .unwrap_or_default();

        let thumb = if thumb.is_empty() {
            default_thumb
        } else {
            thumb
        };
        let hover = if hover.is_empty() {
            default_hover
        } else {
            hover
        };
        (thumb, hover)
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        (default_thumb, default_hover)
    }
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('\"', "&quot;")
        .replace('\'', "&#39;")
}

fn item_title(item: &DashboardItem, indicator_defs: &HashMap<String, IndicatorDef>) -> String {
    if !item.indicator_name.trim().is_empty() {
        item.indicator_name.clone()
    } else if let Some(def) = indicator_defs.get(&item.indicator_id) {
        if !def.code.trim().is_empty() && !def.description.trim().is_empty() {
            format!("{} — {}", def.code, def.description)
        } else if !def.description.trim().is_empty() {
            def.description.clone()
        } else if !def.code.trim().is_empty() {
            def.code.clone()
        } else if item.indicator_id.len() > 8 {
            format!("Indicator {}...", &item.indicator_id[..8])
        } else {
            format!("Indicator {}", item.indicator_id)
        }
    } else if item.indicator_id.len() > 8 {
        format!("Индикатор {}…", &item.indicator_id[..8])
    } else {
        format!("Индикатор {}", item.indicator_id)
    }
}

/// Компактное форматирование числового значения (M/K аббревиатуры — идентично detail-view).
fn format_value(value: f64, format_spec: &serde_json::Value) -> String {
    let kind = format_spec["kind"].as_str().unwrap_or("Number");
    let abs = value.abs();
    match kind {
        "Money" => format_money_with_format_spec(value, format_spec),
        "Percent" => {
            let decimals = format_spec["decimals"].as_u64().unwrap_or(1) as usize;
            format!("{:.prec$}%", value, prec = decimals)
        }
        "Integer" => {
            if abs >= 1_000_000_000.0 {
                format!("{:.1}B", value / 1_000_000_000.0)
            } else if abs >= 1_000_000.0 {
                format!("{:.1}M", value / 1_000_000.0)
            } else {
                format!("{}", value as i64)
            }
        }
        _ => {
            let decimals = format_spec["decimals"].as_u64().unwrap_or(2) as usize;
            if abs >= 1_000_000_000.0 {
                format!("{:.1}B", value / 1_000_000_000.0)
            } else if abs >= 1_000_000.0 {
                format!("{:.1}M", value / 1_000_000.0)
            } else {
                format!("{:.prec$}", value, prec = decimals)
            }
        }
    }
}

fn format_full_value(value: f64, format_spec: &serde_json::Value) -> String {
    let kind = format_spec["kind"].as_str().unwrap_or("Number");
    match kind {
        "Money" => format_money_with_format_spec(value, format_spec),
        "Integer" => format_int_with_triads(value.round() as i64),
        "Percent" => {
            let decimals = format_spec["decimals"].as_u64().unwrap_or(1) as usize;
            format!("{:.prec$}%", value, prec = decimals)
        }
        _ => {
            let decimals = format_spec["decimals"].as_u64().unwrap_or(2) as usize;
            let formatted = format!("{:.prec$}", value, prec = decimals);
            let mut parts = formatted.splitn(2, '.');
            let whole = parts
                .next()
                .and_then(|s| s.parse::<i64>().ok())
                .map(format_int_with_triads)
                .unwrap_or_else(|| formatted.clone());
            match parts.next() {
                Some(frac) => format!("{whole}.{frac}"),
                None => whole,
            }
        }
    }
}

fn push_detail_line(lines: &mut Vec<String>, value: impl Into<String>) {
    let value = value.into();
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return;
    }
    if !lines.iter().any(|item| item == trimmed) {
        lines.push(trimmed.to_string());
    }
}

fn build_indicator_description(def: Option<&IndicatorDef>) -> Option<String> {
    let def = def?;
    if let Some(comment) = def.comment.as_ref().map(|value| value.trim()) {
        if !comment.is_empty() {
            return Some(comment.to_string());
        }
    }

    match (
        def.data_spec.view_id.as_deref(),
        def.data_spec.metric_id.as_deref(),
    ) {
        (Some(view_id), Some(metric_id)) if !view_id.trim().is_empty() => Some(format!(
            "Индикатор рассчитывается через {} по метрике {}.",
            view_id, metric_id
        )),
        (Some(view_id), None) if !view_id.trim().is_empty() => {
            Some(format!("Индикатор рассчитывается через {}.", view_id))
        }
        _ => None,
    }
}

fn build_indicator_details(
    def: Option<&IndicatorDef>,
    computed: Option<&ComputedValue>,
    effective_params: &HashMap<String, String>,
) -> Vec<String> {
    let mut lines = Vec::new();

    if let Some(computed) = computed {
        if let Some(subtitle) = computed.subtitle.as_deref() {
            push_detail_line(&mut lines, format!("Схема расчёта: {}", subtitle));
        }
        for detail in &computed.details {
            push_detail_line(&mut lines, detail.clone());
        }
    }

    if let Some(def) = def {
        if let Some(view_id) = def
            .data_spec
            .view_id
            .as_deref()
            .filter(|value| !value.trim().is_empty())
        {
            push_detail_line(&mut lines, format!("Источник данных: {}", view_id));
        }
        if let Some(metric_id) = def
            .data_spec
            .metric_id
            .as_deref()
            .filter(|value| !value.trim().is_empty())
        {
            push_detail_line(&mut lines, format!("Показатель: {}", metric_id));
        }
    }

    let mut param_pairs: Vec<_> = effective_params
        .iter()
        .filter(|(key, value)| {
            let key = key.as_str();
            !value.trim().is_empty()
                && key != "metric"
                && key != "period2_from"
                && key != "period2_to"
        })
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect();
    param_pairs.sort_by(|left, right| left.0.cmp(&right.0));

    for (key, value) in param_pairs.into_iter().take(6) {
        push_detail_line(&mut lines, format!("Параметр: {} = {}", key, value));
    }

    lines
}

/// Аббревиатура месяца на русском
fn month_abbr(m: u32) -> &'static str {
    match m {
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
        _ => "???",
    }
}

fn fmt_date_label(s: &str) -> Option<String> {
    let date = NaiveDate::parse_from_str(s, "%Y-%m-%d").ok()?;
    Some(date.format("%d.%m.%Y").to_string())
}

fn period_label(from: &str, to: &str) -> String {
    match (fmt_date_label(from), fmt_date_label(to)) {
        (Some(f), Some(t)) if f == t => f,
        (Some(f), Some(t)) => format!("{f} — {t}"),
        (Some(f), None) => format!("с {f}"),
        (None, Some(t)) => format!("до {t}"),
        _ => "Период не задан".to_string(),
    }
}

/// Компактный хинт для meta_1: "Янв – Фев 2026 · 4 каб."
fn compact_filter_hint(ctx: &ViewContext) -> String {
    let mut parts: Vec<String> = Vec::new();

    if let Some(date_part) = period_summary(ctx, DashboardPeriodSlot::Primary, true) {
        parts.push(date_part);
    }

    // Кол-во кабинетов
    let selected_count = ctx.connection_mp_refs.len();
    if selected_count > 0 {
        parts.push(format!("{} каб.", selected_count));
    }

    parts.join(" · ")
}

fn has_custom_css_for_all(indicator_defs: &HashMap<String, IndicatorDef>) -> bool {
    !indicator_defs.is_empty()
        && indicator_defs.values().all(|def| {
            def.view_spec
                .custom_css
                .as_deref()
                .map(|css| !css.trim().is_empty())
                .unwrap_or(false)
        })
}

fn render_indicator_html(
    item: &DashboardItem,
    view_ctx: &ViewContext,
    indicator_defs: &HashMap<String, IndicatorDef>,
    indicator_values: &HashMap<String, ComputedValue>,
    theme: &str,
    design_key: &str,
) -> String {
    let def = indicator_defs.get(&item.indicator_id);
    let computed = indicator_values.get(&item.indicator_id);
    let preview_values = def.map(|d| &d.view_spec.preview_values);
    let preview = |key: &str| -> String {
        preview_values
            .and_then(|pv| pv.get(key))
            .cloned()
            .unwrap_or_default()
    };
    let hidden: std::collections::HashSet<String> = preview("_hidden")
        .split(',')
        .filter_map(|k| {
            let key = k.trim();
            if key.is_empty() {
                None
            } else {
                Some(key.to_string())
            }
        })
        .collect();
    let is_hidden = |key: &str| hidden.contains(key);

    let format_spec = def
        .map(|d| d.view_spec.format.clone())
        .unwrap_or(serde_json::Value::Null);

    // For fields with a live source: use computed value, fallback to preview("key") if missing.
    // For fields with source="—": always use preview("key").
    let value_str = computed
        .and_then(|cv| cv.value)
        .map(|v| format_value(v, &format_spec))
        .unwrap_or_else(|| {
            let pv = preview("value");
            if !pv.is_empty() {
                pv
            } else {
                "—".to_string()
            }
        });

    let change_pct = computed.and_then(|cv| cv.change_percent);
    let delta_str = change_pct
        .map(|pct| {
            if pct > 0.0 {
                format!("{:+.1}%", pct)
            } else if pct < 0.0 {
                format!("{:.1}%", pct)
            } else {
                "0.0%".to_string()
            }
        })
        .unwrap_or_else(|| {
            let pv = preview("delta");
            if !pv.is_empty() {
                pv
            } else {
                "—".to_string()
            }
        });
    let delta_dir: String = change_pct
        .map(|pct| {
            if pct > 0.0 {
                "up".to_string()
            } else if pct < 0.0 {
                "down".to_string()
            } else {
                "flat".to_string()
            }
        })
        .unwrap_or_else(|| {
            let pv = preview("delta_dir");
            if !pv.is_empty() {
                pv
            } else {
                "flat".to_string()
            }
        });

    let status: String = computed
        .and_then(|cv| cv.status.as_deref())
        .map(|s| match s {
            "Good" => "ok",
            "Bad" => "bad",
            "Warning" => "warn",
            _ => "neutral",
        })
        .map(|s| s.to_string())
        .unwrap_or_else(|| {
            let pv = preview("status");
            if !pv.is_empty() {
                pv
            } else {
                "neutral".to_string()
            }
        });

    let mut graph_type = preview("graph_type")
        .parse::<u8>()
        .ok()
        .map(|v| v.min(2))
        .unwrap_or_else(|| {
            let progress = preview("progress").parse::<u8>().unwrap_or(0);
            let has_spark = !preview("spark_points").trim().is_empty();
            if progress > 0 {
                1
            } else if has_spark {
                2
            } else {
                2
            }
        });
    if (graph_type == 1 && is_hidden("progress")) || (graph_type == 2 && is_hidden("spark")) {
        graph_type = 0;
    }
    let progress = if graph_type == 1 && !is_hidden("progress") {
        preview("progress").parse::<u8>().unwrap_or(0).min(100)
    } else {
        0
    };
    let spark_points = if graph_type == 2 && !is_hidden("spark") {
        let from_computed = computed
            .filter(|cv| !cv.spark_points.is_empty())
            .map(|cv| cv.spark_points.clone());
        from_computed.unwrap_or_else(|| {
            preview("spark_points")
                .split(',')
                .filter_map(|p| p.trim().parse::<f64>().ok())
                .collect()
        })
    } else {
        vec![]
    };
    let meta_1 = if is_hidden("meta_1") {
        String::new()
    } else {
        let val = preview("meta_1");
        if val.trim().is_empty() {
            compact_filter_hint(view_ctx)
        } else {
            val
        }
    };
    let meta_2 = if is_hidden("meta_2") {
        String::new()
    } else {
        preview("meta_2")
    };

    // name: use preview("name") if set, otherwise fallback to code/description
    let card_name = {
        let pv = preview("name");
        if !pv.trim().is_empty() {
            pv
        } else {
            item_title(item, indicator_defs)
        }
    };

    let params = IndicatorCardParams {
        style_name: design_key.to_string(),
        theme: theme.to_string(),
        name: card_name,
        value: value_str,
        unit: if is_hidden("unit") {
            String::new()
        } else {
            preview("unit")
        },
        delta: delta_str,
        delta_dir,
        status,
        chip: if is_hidden("chip") {
            String::new()
        } else {
            preview("chip")
        },
        col_class: "col-12".to_string(),
        graph_type,
        progress,
        spark_points,
        meta_1,
        meta_2,
        hint: if is_hidden("hint") {
            String::new()
        } else {
            preview("hint")
        },
        footer_1: if is_hidden("footer_1") {
            String::new()
        } else {
            preview("footer_1")
        },
        footer_2: if is_hidden("footer_2") {
            String::new()
        } else {
            preview("footer_2")
        },
        custom_html: None,
        custom_css: if design_key == "custom" {
            def.and_then(|d| d.view_spec.custom_css.clone())
        } else {
            None
        },
    };

    render_card_html(&params)
}

fn sort_groups_recursive(groups: &mut Vec<DashboardGroup>) {
    groups.sort_by_key(|g| g.sort_order);
    for group in groups {
        group.items.sort_by_key(|i| i.sort_order);
        sort_groups_recursive(&mut group.subgroups);
    }
}

fn render_group_html(
    group: &DashboardGroup,
    view_ctx: &ViewContext,
    indicator_defs: &HashMap<String, IndicatorDef>,
    indicator_values: &HashMap<String, ComputedValue>,
    theme: &str,
    design_key: &str,
    depth: usize,
) -> String {
    let title = if group.title.trim().is_empty() {
        "Без названия".to_string()
    } else {
        group.title.clone()
    };

    let title_class = if depth == 0 {
        "group__title group__title--root"
    } else {
        "group__title group__title--sub"
    };

    let cards_html = if group.items.is_empty() {
        "<div class=\"cards-empty\">Нет индикаторов</div>".to_string()
    } else {
        group
            .items
            .iter()
            .map(|item| {
                let card_html = render_indicator_html(
                    item,
                    view_ctx,
                    indicator_defs,
                    indicator_values,
                    theme,
                    design_key,
                );
                format!(
                    r#"<div class="card-slot" data-indicator-id="{}">{card_html}</div>"#,
                    item.indicator_id
                )
            })
            .collect::<Vec<_>>()
            .join("")
    };

    let subgroups_html = group
        .subgroups
        .iter()
        .map(|sub| {
            render_group_html(
                sub,
                view_ctx,
                indicator_defs,
                indicator_values,
                theme,
                design_key,
                depth + 1,
            )
        })
        .collect::<Vec<_>>()
        .join("");

    format!(
        r#"<section class="group">
<div class="{title_class}">{title}</div>
<div class="cards">{cards_html}</div>
{subgroups_html}
</section>"#,
        title_class = title_class,
        title = escape_html(&title),
        cards_html = cards_html,
        subgroups_html = subgroups_html
    )
}

fn build_dashboard_srcdoc(
    groups: &[DashboardGroup],
    view_ctx: &ViewContext,
    theme: &str,
    design_key: &str,
    indicator_defs: &HashMap<String, IndicatorDef>,
    indicator_values: &HashMap<String, ComputedValue>,
    sidebar_scrollbar_thumb: &str,
    sidebar_scrollbar_thumb_hover: &str,
) -> String {
    let groups_html = if groups.is_empty() {
        "<div class=\"empty\">Дашборд пуст. Добавьте группы и индикаторы в редакторе.</div>"
            .to_string()
    } else {
        groups
            .iter()
            .map(|g| {
                render_group_html(
                    g,
                    view_ctx,
                    indicator_defs,
                    indicator_values,
                    theme,
                    design_key,
                    0,
                )
            })
            .collect::<Vec<_>>()
            .join("")
    };

    let style_css = get_style_css(design_key);
    let css = format!(
        r#"
{style_css}
html,body{{margin:0;padding:0;}}
:root{{
  --sb-thumb: {sidebar_scrollbar_thumb};
  --sb-thumb-hover: {sidebar_scrollbar_thumb_hover};
  --bi-primary:#3b82f6;
  --bi-success:#22c55e;
  --bi-danger:#ef4444;
  --bi-warning:#f59e0b;
  --bi-text:#1e293b;
  --bi-text-secondary:#64748b;
  --bi-bg:#ffffff;
  --bi-bg-secondary:#f8fafc;
  --bi-border:#e2e8f0;
}}
body[data-theme-base="dark"]{{
  --bi-text:#e5e7eb;
  --bi-text-secondary:#9aa4b2;
  --bi-bg:#0b1220;
  --bi-bg-secondary:#0f1a2e;
  --bi-border:rgba(255,255,255,.12);
}}
body{{
  background:transparent !important;
  min-height:100% !important;
  display:block !important;
  justify-content:initial !important;
  align-items:initial !important;
  padding:0 !important;
}}
html{{
  overflow:auto;
  scrollbar-width:thin;
  scrollbar-color:var(--sb-thumb) transparent;
}}
html::-webkit-scrollbar{{width:6px;height:6px;}}
html::-webkit-scrollbar-track{{background:transparent;}}
html::-webkit-scrollbar-thumb{{
  background:var(--sb-thumb);
  border-radius:3px;
}}
html::-webkit-scrollbar-thumb:hover{{
  background:var(--sb-thumb-hover);
}}
.dashboard{{
  margin:12px 110px 110px;
  display:flex;
  flex-direction:column;
  gap:14px;
}}
.group{{
  display:flex;
  flex-direction:column;
  gap:10px;
}}
.group__title{{
  font-weight:700;
  padding:0;
  margin:0;
  background:none !important;
  border:none !important;
  color:var(--text,var(--bi-text));
}}
.group__title--root{{font-size:16px;}}
.group__title--sub{{font-size:14px;opacity:.9;margin-top:2px;}}
.cards{{
  display:flex;
  flex-wrap:wrap;
  gap:12px;
  align-items:stretch;
}}
.card-slot{{
  flex:0 0 280px;
  width:280px;
  min-width:280px;
}}
.card-slot .indicator-card{{
  width:100%;
  min-height:124px;
}}
.cards-empty,.empty{{
  color:var(--muted,var(--bi-text-secondary));
  font-size:13px;
  padding:2px 0;
}}
"#,
        sidebar_scrollbar_thumb = sidebar_scrollbar_thumb,
        sidebar_scrollbar_thumb_hover = sidebar_scrollbar_thumb_hover,
    );

    let mut html = String::new();
    html.push_str("<!DOCTYPE html><html><head><meta charset=\"UTF-8\"><style>");
    html.push_str(&css);
    html.push_str(concat!(
        "</style><script>(function(){",
        "var ac=null;",
        "document.addEventListener('click',function(e){",
          "var btn=e.target.closest('.indicator-card');if(!btn)return;",
          "var slot=btn.closest('[data-indicator-id]');if(!slot)return;",
          "if(ac&&ac!==btn){ac.style.cssText='';}",
          "ac=btn;",
          "var r=btn.getBoundingClientRect();",
          "var cx=r.left+r.width/2,cy=r.top+r.height/2;",
          "var dx=(window.innerWidth/2-cx)*0.3,dy=(window.innerHeight/2-cy)*0.3;",
          "btn.style.transition='transform 0.22s cubic-bezier(0.4,0,0.2,1),opacity 0.22s ease';",
          "btn.style.transform='translate('+dx+'px,'+dy+'px) scale(1.08)';",
          "btn.style.opacity='0';btn.style.pointerEvents='none';",
          "setTimeout(function(){",
            "window.parent.postMessage({type:'indicator_click',id:slot.dataset.indicatorId,cx:cx,cy:cy},'*');",
          "},230);",
        "});",
        "window.addEventListener('message',function(e){",
          "if(!e.data||e.data.type!=='indicator_restore')return;",
          "if(!ac)return;",
          "var c=ac;ac=null;",
          "c.style.transition='transform 0.28s cubic-bezier(0.2,0.9,0.2,1),opacity 0.28s ease';",
          "c.style.transform='';c.style.opacity='';c.style.pointerEvents='';",
          "setTimeout(function(){c.style.transition='';},300);",
        "});",
        "})();</script></head><body data-theme=\""
    ));
    let base = if theme == "light" { "light" } else { "dark" };
    html.push_str(base);
    html.push_str("\" data-theme-base=\"");
    html.push_str(base);
    html.push_str("\"><div class=\"dashboard\">");
    html.push_str(&groups_html);
    html.push_str("</div></body></html>");
    html
}

#[component]
fn IndicatorRefreshOverlay(
    #[prop(into)] card_count: Signal<usize>,
    #[prop(into)] filter_hint: Signal<String>,
) -> impl IntoView {
    view! {
        <div class="loading-overlay loading-overlay--dashboard">
            <div class="indicator-refresh">
                <div class="indicator-refresh__badge">
                    {move || {
                        let count = card_count.get();
                        if count == 0 {
                            "Подготавливаем макет".to_string()
                        } else {
                            format!("{count} карточек")
                        }
                    }}
                </div>
                <div class="indicator-refresh__headline">
                    <span class="indicator-refresh__pulse"></span>
                    <div class="indicator-refresh__titles">
                        <strong>"Формируем индикаторы"</strong>
                        <span>
                            {move || {
                                let hint = filter_hint.get();
                                if hint.trim().is_empty() {
                                    "Собираем новые значения, сравниваем период и обновляем витрину.".to_string()
                                } else {
                                    format!("{hint} • обновляем значения и сравнение периодов")
                                }
                            }}
                        </span>
                    </div>
                </div>

                <div class="indicator-refresh__timeline">
                    <div class="indicator-refresh__step">
                        <span class="indicator-refresh__dot"></span>
                        <span>"Читаем DataView и GL"</span>
                    </div>
                    <div class="indicator-refresh__step">
                        <span class="indicator-refresh__dot"></span>
                        <span>"Считаем сравнение с прошлым периодом"</span>
                    </div>
                    <div class="indicator-refresh__step">
                        <span class="indicator-refresh__dot"></span>
                        <span>"Перерисовываем карточки дашборда"</span>
                    </div>
                </div>

                <div class="indicator-refresh__cards" aria-hidden="true">
                    <div class="indicator-refresh__card indicator-refresh__card--lg">
                        <span class="indicator-refresh__line indicator-refresh__line--short"></span>
                        <span class="indicator-refresh__line indicator-refresh__line--value"></span>
                        <span class="indicator-refresh__line indicator-refresh__line--medium"></span>
                    </div>
                    <div class="indicator-refresh__card">
                        <span class="indicator-refresh__line indicator-refresh__line--short"></span>
                        <span class="indicator-refresh__line indicator-refresh__line--value"></span>
                        <span class="indicator-refresh__line indicator-refresh__line--short"></span>
                    </div>
                    <div class="indicator-refresh__card indicator-refresh__card--accent">
                        <span class="indicator-refresh__line indicator-refresh__line--short"></span>
                        <span class="indicator-refresh__line indicator-refresh__line--value"></span>
                        <span class="indicator-refresh__line indicator-refresh__line--medium"></span>
                    </div>
                </div>
            </div>
        </div>
    }
}

/* #[component]
fn DashboardPeriodControlsOld(ctx: RwSignal<ViewContext>) -> impl IntoView {
    let active_mode = Signal::derive(move || DashboardPeriodMode::from_ctx(&ctx.get()));
    let current_period_title = Signal::derive(move || primary_period_summary(&ctx.get()));
    let comparison_period_title =
        Signal::derive(move || comparison_period_summary(&ctx.get()).unwrap_or_default());

    view! {
        <div
            class="dashboard-period-controls"
            style="display:flex; flex-direction:column; gap:12px; padding:14px 16px; border:1px solid var(--colorNeutralStroke2, #e5e7eb); border-radius:14px; background:var(--colorNeutralBackground1, #fff); margin-bottom:14px;"
        >
            <div style="display:flex; flex-wrap:wrap; gap:8px; align-items:center; justify-content:space-between;">
                <div style="display:flex; flex-wrap:wrap; gap:8px; align-items:center;">
                    <span style="font-size:12px; font-weight:700; text-transform:uppercase; letter-spacing:0.04em; color:var(--colorNeutralForeground3, #6b7280);">
                        "Период"
                    </span>
                    {[
                        DashboardPeriodMode::Month,
                        DashboardPeriodMode::Week,
                        DashboardPeriodMode::Day,
                        DashboardPeriodMode::Custom,
                    ]
                        .into_iter()
                        .map(|entry| {
                            let label = entry.title();
                            view! {
                                <button
                                    type="button"
                                    style=move || {
                                        if active_mode.get() == entry {
                                            "border:1px solid var(--colorBrandStroke1, #2563eb); background:var(--colorBrandBackground2, #eff6ff); color:var(--colorBrandForeground1, #1d4ed8); padding:8px 12px; border-radius:999px; font-weight:600; cursor:pointer;"
                                        } else {
                                            "border:1px solid var(--colorNeutralStroke2, #d1d5db); background:var(--colorNeutralBackground1, #fff); color:var(--colorNeutralForeground1, #111827); padding:8px 12px; border-radius:999px; font-weight:500; cursor:pointer;"
                                        }
                                    }
                                    on:click=move |_| {
                                        ctx.update(|current| {
                                            let anchor = ctx_anchor_date(current);
                                            apply_period_mode(current, entry, anchor);
                                        });
                                    }
                                >
                                    {label}
                                </button>
                            }
                        })
                        .collect_view()}
                </div>

                <Show when=move || active_mode.get() != DashboardPeriodMode::Custom>
                    <div style="display:flex; flex-wrap:wrap; gap:8px; align-items:center;">
                        <button
                            type="button"
                            style="border:1px solid var(--colorNeutralStroke2, #d1d5db); background:var(--colorNeutralBackground1, #fff); color:var(--colorNeutralForeground1, #111827); width:36px; height:36px; border-radius:10px; font-size:18px; cursor:pointer;"
                            on:click=move |_| {
                                ctx.update(|current| {
                                    let mode = DashboardPeriodMode::from_ctx(current);
                                    if mode == DashboardPeriodMode::Custom {
                                        return;
                                    }
                                    let anchor = shift_period_anchor(mode, ctx_anchor_date(current), -1);
                                    apply_period_mode(current, mode, anchor);
                                });
                            }
                        >
                            "←"
                        </button>

                        <button
                            type="button"
                            style="border:1px solid var(--colorNeutralStroke2, #d1d5db); background:var(--colorNeutralBackground1, #fff); color:var(--colorNeutralForeground1, #111827); padding:8px 12px; border-radius:10px; font-weight:600; cursor:pointer;"
                            on:click=move |_| {
                                ctx.update(|current| {
                                    let mode = DashboardPeriodMode::from_ctx(current);
                                    apply_period_mode(current, mode, Utc::now().date_naive());
                                });
                            }
                        >
                            {move || active_mode.get().reset_label()}
                        </button>

                        <button
                            type="button"
                            style="border:1px solid var(--colorNeutralStroke2, #d1d5db); background:var(--colorNeutralBackground1, #fff); color:var(--colorNeutralForeground1, #111827); width:36px; height:36px; border-radius:10px; font-size:18px; cursor:pointer;"
                            on:click=move |_| {
                                ctx.update(|current| {
                                    let mode = DashboardPeriodMode::from_ctx(current);
                                    if mode == DashboardPeriodMode::Custom {
                                        return;
                                    }
                                    let anchor = shift_period_anchor(mode, ctx_anchor_date(current), 1);
                                    apply_period_mode(current, mode, anchor);
                                });
                            }
                        >
                            "→"
                        </button>
                    </div>
                </Show>
            </div>

            <Show
                when=move || active_mode.get() == DashboardPeriodMode::Custom
                fallback=move || {
                    view! {
                        <div
                            style="display:flex; flex-wrap:wrap; gap:12px; align-items:stretch; justify-content:space-between;"
                        >
                            <div
                                style="flex:1 1 320px; min-width:260px; padding:12px 14px; border-radius:12px; background:var(--colorNeutralBackground2, #f8fafc); border:1px solid var(--colorNeutralStroke2, #e5e7eb);"
                            >
                                <div style="font-size:12px; font-weight:700; text-transform:uppercase; letter-spacing:0.04em; color:var(--colorNeutralForeground3, #6b7280); margin-bottom:6px;">
                                    {move || active_mode.get().title()}
                                </div>
                                <div style="font-size:18px; font-weight:700; color:var(--colorNeutralForeground1, #111827);">
                                    {move || current_period_title.get()}
                                </div>
                            </div>

                            <Show when=move || !comparison_period_title.get().trim().is_empty()>
                                <div
                                    style="flex:1 1 280px; min-width:240px; padding:12px 14px; border-radius:12px; background:var(--colorBrandBackground2, #eff6ff); border:1px solid var(--colorBrandStroke2, #bfdbfe);"
                                >
                                    <div style="font-size:12px; font-weight:700; text-transform:uppercase; letter-spacing:0.04em; color:var(--colorNeutralForeground3, #6b7280); margin-bottom:6px;">
                                        "Сравнение"
                                    </div>
                                    <div style="font-size:15px; font-weight:600; color:var(--colorNeutralForeground1, #111827);">
                                        {move || comparison_period_title.get()}
                                    </div>
                                </div>
                            </Show>
                        </div>
                    }
                }
            >
                <div style="display:flex; flex-wrap:wrap; gap:12px;">
                    <div
                        style="flex:1 1 320px; min-width:260px; display:flex; flex-direction:column; gap:8px; padding:12px 14px; border-radius:12px; background:var(--colorNeutralBackground2, #f8fafc); border:1px solid var(--colorNeutralStroke2, #e5e7eb);"
                    >
                        <div style="font-size:12px; font-weight:700; text-transform:uppercase; letter-spacing:0.04em; color:var(--colorNeutralForeground3, #6b7280);">
                            "Период 1"
                        </div>
                        <div style="display:flex; flex-wrap:wrap; gap:10px;">
                            <label style="display:flex; flex-direction:column; gap:6px; min-width:150px; flex:1 1 150px;">
                                <span style="font-size:12px; color:var(--colorNeutralForeground3, #6b7280);">
                                    "С"
                                </span>
                                <input
                                    type="date"
                                    style="border:1px solid var(--colorNeutralStroke2, #d1d5db); border-radius:10px; padding:8px 10px;"
                                    prop:value=move || ctx.get().date_from
                                    on:input=move |ev| {
                                        let value = event_target_value(&ev);
                                        ctx.update(|current| {
                                            current.date_from = value.clone();
                                            current.params.insert(
                                                DASHBOARD_PERIOD_MODE_PARAM.to_string(),
                                                DashboardPeriodMode::Custom.as_param_value().to_string(),
                                            );
                                        });
                                    }
                                />
                            </label>
                            <label style="display:flex; flex-direction:column; gap:6px; min-width:150px; flex:1 1 150px;">
                                <span style="font-size:12px; color:var(--colorNeutralForeground3, #6b7280);">
                                    "По"
                                </span>
                                <input
                                    type="date"
                                    style="border:1px solid var(--colorNeutralStroke2, #d1d5db); border-radius:10px; padding:8px 10px;"
                                    prop:value=move || ctx.get().date_to
                                    on:input=move |ev| {
                                        let value = event_target_value(&ev);
                                        ctx.update(|current| {
                                            current.date_to = value.clone();
                                            current.params.insert(
                                                DASHBOARD_PERIOD_MODE_PARAM.to_string(),
                                                DashboardPeriodMode::Custom.as_param_value().to_string(),
                                            );
                                        });
                                    }
                                />
                            </label>
                        </div>
                    </div>

                    <div
                        style="flex:1 1 320px; min-width:260px; display:flex; flex-direction:column; gap:8px; padding:12px 14px; border-radius:12px; background:var(--colorBrandBackground2, #eff6ff); border:1px solid var(--colorBrandStroke2, #bfdbfe);"
                    >
                        <div style="font-size:12px; font-weight:700; text-transform:uppercase; letter-spacing:0.04em; color:var(--colorNeutralForeground3, #6b7280);">
                            "Период 2"
                        </div>
                        <div style="display:flex; flex-wrap:wrap; gap:10px;">
                            <label style="display:flex; flex-direction:column; gap:6px; min-width:150px; flex:1 1 150px;">
                                <span style="font-size:12px; color:var(--colorNeutralForeground3, #6b7280);">
                                    "С"
                                </span>
                                <input
                                    type="date"
                                    style="border:1px solid var(--colorNeutralStroke2, #d1d5db); border-radius:10px; padding:8px 10px;"
                                    prop:value=move || ctx.get().period2_from.unwrap_or_default()
                                    on:input=move |ev| {
                                        let value = event_target_value(&ev);
                                        ctx.update(|current| {
                                            current.period2_from = if value.trim().is_empty() {
                                                None
                                            } else {
                                                Some(value.clone())
                                            };
                                            current.params.insert(
                                                DASHBOARD_PERIOD_MODE_PARAM.to_string(),
                                                DashboardPeriodMode::Custom.as_param_value().to_string(),
                                            );
                                        });
                                    }
                                />
                            </label>
                            <label style="display:flex; flex-direction:column; gap:6px; min-width:150px; flex:1 1 150px;">
                                <span style="font-size:12px; color:var(--colorNeutralForeground3, #6b7280);">
                                    "По"
                                </span>
                                <input
                                    type="date"
                                    style="border:1px solid var(--colorNeutralStroke2, #d1d5db); border-radius:10px; padding:8px 10px;"
                                    prop:value=move || ctx.get().period2_to.unwrap_or_default()
                                    on:input=move |ev| {
                                        let value = event_target_value(&ev);
                                        ctx.update(|current| {
                                            current.period2_to = if value.trim().is_empty() {
                                                None
                                            } else {
                                                Some(value.clone())
                                            };
                                            current.params.insert(
                                                DASHBOARD_PERIOD_MODE_PARAM.to_string(),
                                                DashboardPeriodMode::Custom.as_param_value().to_string(),
                                            );
                                        });
                                    }
                                />
                            </label>
                        </div>
                    </div>
                </div>
            </Show>
        </div>
    }
}

*/
#[component]
fn DashboardPeriodControls(ctx: RwSignal<ViewContext>) -> impl IntoView {
    let active_mode = Signal::derive(move || DashboardPeriodMode::from_ctx(&ctx.get()));

    view! {
        <div class="dashboard-period-controls">
            <span class="dpc-section-label">"Период"</span>
            <div class="dpc-mode-tabs">
                {[DashboardPeriodMode::Month, DashboardPeriodMode::Week, DashboardPeriodMode::Day, DashboardPeriodMode::Custom]
                    .into_iter()
                    .map(|entry| {
                        let label = entry.title();
                        view! {
                            <button
                                type="button"
                                class=move || if active_mode.get() == entry {
                                    "dpc-mode-tab dpc-mode-tab--active"
                                } else {
                                    "dpc-mode-tab"
                                }
                                on:click=move |_| {
                                    ctx.update(|current| {
                                        let anchor = period_slot_anchor(
                                            current,
                                            DashboardPeriodSlot::Primary,
                                            entry,
                                        );
                                        apply_period_mode(current, entry, anchor);
                                    });
                                }
                            >
                                {label}
                            </button>
                        }
                    })
                    .collect_view()}
            </div>

            {[DashboardPeriodSlot::Primary, DashboardPeriodSlot::Comparison]
                .into_iter()
                .map(|slot| {
                    let group_class = match slot {
                        DashboardPeriodSlot::Primary => "dpc-slot-group dpc-slot-group--primary",
                        DashboardPeriodSlot::Comparison => "dpc-slot-group dpc-slot-group--comparison",
                    };
                    let badge_class = match slot {
                        DashboardPeriodSlot::Primary => "dpc-slot-badge dpc-slot-badge--primary",
                        DashboardPeriodSlot::Comparison => "dpc-slot-badge dpc-slot-badge--comparison",
                    };

                    view! {
                        <div class=group_class>
                            <div class=badge_class>{slot.title()}</div>

                            <Show
                                when=move || active_mode.get() == DashboardPeriodMode::Custom
                                fallback=move || {
                                    view! {
                                        <>
                                            <button
                                                type="button"
                                                class="dpc-nav-btn"
                                                on:click=move |_| {
                                                    ctx.update(|current| {
                                                        let mode = DashboardPeriodMode::from_ctx(current);
                                                        let anchor = period_slot_anchor(current, slot, mode);
                                                        let shifted_anchor = shift_period_anchor(mode, anchor, -1);
                                                        let (from, to) = canonical_range_for_mode(mode, shifted_anchor);
                                                        set_period_slot(current, slot, from, to);
                                                        current.params.insert(
                                                            DASHBOARD_PERIOD_MODE_PARAM.to_string(),
                                                            mode.as_param_value().to_string(),
                                                        );
                                                    });
                                                }
                                            >
                                                <span style="display:inline-flex; transform:rotate(180deg);">{icon("chevron-right")}</span>
                                            </button>

                                            <Button
                                                appearance=ButtonAppearance::Secondary
                                                size=ButtonSize::Small
                                                on:click=move |_| {
                                                    ctx.update(|current| {
                                                        let mode = DashboardPeriodMode::from_ctx(current);
                                                        let today = Utc::now().date_naive();
                                                        let anchor = match slot {
                                                            DashboardPeriodSlot::Primary => today,
                                                            DashboardPeriodSlot::Comparison => shift_period_anchor(mode, today, -1),
                                                        };
                                                        let (from, to) = canonical_range_for_mode(mode, anchor);
                                                        set_period_slot(current, slot, from, to);
                                                        current.params.insert(
                                                            DASHBOARD_PERIOD_MODE_PARAM.to_string(),
                                                            mode.as_param_value().to_string(),
                                                        );
                                                    });
                                                }
                                            >
                                                {move || slot.reset_label(active_mode.get())}
                                            </Button>

                                            <div class="dpc-period-display">
                                                {move || {
                                                    let current = ctx.get();
                                                    period_summary(&current, slot, false)
                                                        .unwrap_or_else(|| "Период не задан".to_string())
                                                }}
                                            </div>

                                            <button
                                                type="button"
                                                class="dpc-nav-btn"
                                                on:click=move |_| {
                                                    ctx.update(|current| {
                                                        let mode = DashboardPeriodMode::from_ctx(current);
                                                        let anchor = period_slot_anchor(current, slot, mode);
                                                        let shifted_anchor = shift_period_anchor(mode, anchor, 1);
                                                        let (from, to) = canonical_range_for_mode(mode, shifted_anchor);
                                                        set_period_slot(current, slot, from, to);
                                                        current.params.insert(
                                                            DASHBOARD_PERIOD_MODE_PARAM.to_string(),
                                                            mode.as_param_value().to_string(),
                                                        );
                                                    });
                                                }
                                            >
                                                {icon("chevron-right")}
                                            </button>

                                            <Show when=move || active_mode.get() == DashboardPeriodMode::Day>
                                                <input
                                                    type="date"
                                                    class="dpc-date-input"
                                                    prop:value=move || {
                                                        period_slot_dates(&ctx.get(), slot)
                                                            .map(|(_, to)| fmt_ymd(to))
                                                            .unwrap_or_default()
                                                    }
                                                    on:input=move |ev| {
                                                        let value = event_target_value(&ev);
                                                        if let Some(date) = parse_ymd(&value) {
                                                            ctx.update(|current| {
                                                                set_period_slot(current, slot, date, date);
                                                                current.params.insert(
                                                                    DASHBOARD_PERIOD_MODE_PARAM.to_string(),
                                                                    DashboardPeriodMode::Day.as_param_value().to_string(),
                                                                );
                                                            });
                                                        }
                                                    }
                                                />
                                            </Show>
                                        </>
                                    }
                                }
                            >
                                <div style="display:flex; flex-wrap:nowrap; gap:8px; align-items:center; flex:0 0 auto;">
                                    <input
                                        type="date"
                                        class="dpc-date-input"
                                        prop:value=move || {
                                            period_slot_dates(&ctx.get(), slot)
                                                .map(|(from, _)| fmt_ymd(from))
                                                .unwrap_or_default()
                                        }
                                        on:input=move |ev| {
                                            let value = event_target_value(&ev);
                                            if value.trim().is_empty() {
                                                return;
                                            }
                                            if let Some(from) = parse_ymd(&value) {
                                                ctx.update(|current| {
                                                    let to = period_slot_dates(current, slot)
                                                        .map(|(_, to)| to)
                                                        .unwrap_or(from);
                                                    set_period_slot(current, slot, from, to);
                                                    current.params.insert(
                                                        DASHBOARD_PERIOD_MODE_PARAM.to_string(),
                                                        DashboardPeriodMode::Custom.as_param_value().to_string(),
                                                    );
                                                });
                                            }
                                        }
                                    />

                                    <span class="dpc-sep">"—"</span>

                                    <input
                                        type="date"
                                        class="dpc-date-input"
                                        prop:value=move || {
                                            period_slot_dates(&ctx.get(), slot)
                                                .map(|(_, to)| fmt_ymd(to))
                                                .unwrap_or_default()
                                        }
                                        on:input=move |ev| {
                                            let value = event_target_value(&ev);
                                            if value.trim().is_empty() {
                                                return;
                                            }
                                            if let Some(to) = parse_ymd(&value) {
                                                ctx.update(|current| {
                                                    let from = period_slot_dates(current, slot)
                                                        .map(|(from, _)| from)
                                                        .unwrap_or(to);
                                                    set_period_slot(current, slot, from, to);
                                                    current.params.insert(
                                                        DASHBOARD_PERIOD_MODE_PARAM.to_string(),
                                                        DashboardPeriodMode::Custom.as_param_value().to_string(),
                                                    );
                                                });
                                            }
                                        }
                                    />
                                </div>
                            </Show>
                        </div>
                    }
                })
                .collect_view()}
        </div>
    }
}

#[component]
fn DashboardConnectionMpControls(ctx: RwSignal<ViewContext>) -> impl IntoView {
    let options = RwSignal::new(Vec::<DashboardMpOption>::new());
    let loading = RwSignal::new(false);
    let fetch_error = RwSignal::new(None::<String>);
    let requested = RwSignal::new(false);

    Effect::new(move |_| {
        if requested.get() {
            return;
        }
        requested.set(true);
        spawn_local(async move {
            loading.set(true);
            fetch_error.set(None);
            match fetch_dashboard_mp_options().await {
                Ok(items) => options.set(items),
                Err(err) => fetch_error.set(Some(err)),
            }
            loading.set(false);
        });
    });

    view! {
        <div class="dashboard-mp-controls">
            <span class="dmc-section-label">"Кабинет МП"</span>
            <div class="dmc-group">
                <Button
                    appearance=ButtonAppearance::Secondary
                    size=ButtonSize::Small
                    on:click=move |_| {
                        ctx.update(|current| current.connection_mp_refs.clear());
                    }
                >
                    "Все"
                </Button>

                {move || {
                    if loading.get() {
                        view! {
                            <div class="dmc-status">"Загрузка..."</div>
                        }.into_any()
                    } else if let Some(err) = fetch_error.get() {
                        view! {
                            <div class="dmc-error">{err}</div>
                        }.into_any()
                    } else {
                        view! {
                            <For
                                each=move || options.get()
                                key=|opt| opt.id.clone()
                                children=move |opt: DashboardMpOption| {
                                    let option_id = opt.id.clone();
                                    let class_option_id = option_id.clone();
                                    let option_label = opt.label.clone();
                                    view! {
                                        <button
                                            type="button"
                                            class=move || {
                                                if ctx.get().connection_mp_refs.iter().any(|id| id == &class_option_id) {
                                                    "dmc-btn dmc-btn--active"
                                                } else {
                                                    "dmc-btn"
                                                }
                                            }
                                            on:click=move |_| {
                                                ctx.update(|current| {
                                                    if current.connection_mp_refs.iter().any(|id| id == &option_id) {
                                                        current.connection_mp_refs.retain(|id| id != &option_id);
                                                    } else {
                                                        current.connection_mp_refs.push(option_id.clone());
                                                        current.connection_mp_refs.sort();
                                                        current.connection_mp_refs.dedup();
                                                    }
                                                });
                                            }
                                        >
                                            {option_label.clone()}
                                        </button>
                                    }
                                }
                            />
                        }.into_any()
                    }
                }}
            </div>
        </div>
    }
}

/// State passed from the iframe postMessage to drive the detail modal.
#[derive(Clone, Debug)]
struct IndicatorSelection {
    id: String,
    /// Horizontal offset of the card center from the viewport center (px).
    from_x: f64,
    /// Vertical offset of the card center from the viewport center (px).
    from_y: f64,
}

#[component]
pub fn BiDashboardView(id: String) -> impl IntoView {
    let tabs_ctx = use_context::<AppGlobalContext>().expect("AppGlobalContext not found");
    let loading: RwSignal<bool> = RwSignal::new(false);
    let error: RwSignal<Option<String>> = RwSignal::new(None);
    let dashboard: RwSignal<Option<BiDashboardData>> = RwSignal::new(None);
    let view_ctx: RwSignal<ViewContext> = RwSignal::new(ViewContext::default());
    let rendered_ctx: RwSignal<ViewContext> = RwSignal::new(ViewContext::default());
    let dashboard_filter_defs: RwSignal<Vec<FilterDef>> = RwSignal::new(vec![]);
    let indicator_defs: RwSignal<HashMap<String, IndicatorDef>> = RwSignal::new(HashMap::new());
    let indicator_values: RwSignal<HashMap<String, ComputedValue>> = RwSignal::new(HashMap::new());
    let indicator_refreshing: RwSignal<bool> = RwSignal::new(false);
    let indicator_refresh_seq: RwSignal<u64> = RwSignal::new(0);
    let dashboard_design: RwSignal<String> = RwSignal::new(default_design_name().to_string());
    let thaw_theme_ctx = leptos::context::use_context::<ThawThemeContext>();
    let selected_indicator: RwSignal<Option<IndicatorSelection>> = RwSignal::new(None);

    // Listen for postMessage events from the indicator cards iframe.
    // The handle must be stored until cleanup — WindowListenerHandle has no Drop impl,
    // so `let _ = ...` would drop it without removing the listener, leaking the closure
    // (which captures `selected_indicator`) past this component's lifetime.
    let msg_handle =
        window_event_listener(leptos::ev::message, move |ev: web_sys::MessageEvent| {
            let data = ev.data();
            let get_str = |key: &str| -> Option<String> {
                js_sys::Reflect::get(&data, &wasm_bindgen::JsValue::from_str(key))
                    .ok()
                    .and_then(|v| v.as_string())
            };
            let get_f64 = |key: &str| -> Option<f64> {
                js_sys::Reflect::get(&data, &wasm_bindgen::JsValue::from_str(key))
                    .ok()
                    .and_then(|v| v.as_f64())
            };
            if get_str("type").as_deref() != Some("indicator_click") {
                return;
            }
            let Some(indicator_id) = get_str("id") else {
                return;
            };
            let cx_in_iframe = get_f64("cx").unwrap_or(0.0);
            let cy_in_iframe = get_f64("cy").unwrap_or(0.0);

            let (from_x, from_y) = {
                let win = web_sys::window().unwrap();
                let doc = win.document().unwrap();
                let iframe_el = doc
                    .query_selector(".dashboard-viewer__iframe")
                    .ok()
                    .flatten();
                if let Some(el) = iframe_el {
                    let rect = el.get_bounding_client_rect();
                    let vw = win.inner_width().unwrap().as_f64().unwrap_or(1280.0);
                    let vh = win.inner_height().unwrap().as_f64().unwrap_or(800.0);
                    let cx_vp = rect.left() + cx_in_iframe;
                    let cy_vp = rect.top() + cy_in_iframe;
                    (cx_vp - vw / 2.0, cy_vp - vh / 2.0)
                } else {
                    (0.0, 0.0)
                }
            };

            selected_indicator.set(Some(IndicatorSelection {
                id: indicator_id,
                from_x,
                from_y,
            }));
        });
    on_cleanup(move || msg_handle.remove());

    let current_theme = Signal::derive(move || {
        if let Some(ctx) = thaw_theme_ctx {
            let theme = ctx.0.get();
            if theme.name == "light" {
                "light".to_string()
            } else {
                "dark".to_string()
            }
        } else {
            get_app_theme()
        }
    });

    let dashboard_design_options = Signal::derive(move || {
        let defs = indicator_defs.get();
        available_designs(has_custom_css_for_all(&defs))
    });

    let is_filter_expanded = RwSignal::new(true);

    let visible_dashboard_filters =
        Signal::derive(move || non_period_dashboard_filters(&dashboard_filter_defs.get()));
    let active_filters_count = Signal::derive(move || {
        visible_dashboard_filters.get().len()
            + usize::from(!view_ctx.get().connection_mp_refs.is_empty())
    });

    reload_dashboard_data(
        id.clone(),
        loading,
        error,
        dashboard,
        view_ctx,
        dashboard_filter_defs,
        indicator_defs,
        indicator_values,
        false,
    );

    // Реактивный эффект: пересчитываем данные индикаторов при смене фильтров
    Effect::new(move |_| {
        let ctx = view_ctx.get();
        let defs = indicator_defs.get();
        let request_id = indicator_refresh_seq.get_untracked().wrapping_add(1);
        indicator_refresh_seq.set(request_id);
        if defs.is_empty() {
            rendered_ctx.set(ctx);
            indicator_values.set(HashMap::new());
            indicator_refreshing.set(false);
            return;
        }
        indicator_refreshing.set(true);
        leptos::task::spawn_local(async move {
            let computed = match fetch_indicator_data(&defs, &ctx).await {
                Ok(c) => c,
                Err(e) => {
                    indicator_refreshing.set(false);
                    error.set(Some(e));
                    return;
                }
            };
            if indicator_refresh_seq.get_untracked() != request_id {
                return;
            }
            rendered_ctx.set(ctx);
            indicator_values.set(computed);
            indicator_refreshing.set(false);
        });
    });

    Effect::new(move |_| {
        let current = dashboard_design.get();
        let allowed = dashboard_design_options.get();
        if !allowed.iter().any(|entry| entry.key == current.as_str()) {
            dashboard_design.set(default_design_name().to_string());
        }
    });

    let srcdoc = Signal::derive(move || {
        dashboard
            .get()
            .map(|data| {
                let mut groups = data.layout.groups.clone();
                sort_groups_recursive(&mut groups);
                let (thumb, hover) = get_sidebar_scrollbar_tokens();
                build_dashboard_srcdoc(
                    &groups,
                    &rendered_ctx.get(),
                    &current_theme.get(),
                    &dashboard_design.get(),
                    &indicator_defs.get(),
                    &indicator_values.get(),
                    &thumb,
                    &hover,
                )
            })
            .unwrap_or_default()
    });

    view! {
        <PageFrame page_id="a025_bi_dashboard--view" category="dashboard">
            {move || if loading.get() {
                view! { <div class="placeholder">"Загрузка дашборда..."</div> }.into_any()
            } else if let Some(e) = error.get() {
                view! {
                    <div class="warning-box">
                        <span class="warning-box__icon">"⚠"</span>
                        <span class="warning-box__text">{e}</span>
                    </div>
                }.into_any()
            } else if let Some(data) = dashboard.get() {
                let title = data.description.clone();
                let code = data.code.clone();
                let detail_tab_key = format!("a025_bi_dashboard_details_{}", id.clone());
                let detail_tab_title = format!("Дашборд · {}", code.clone());
                let tabs_ctx_edit = tabs_ctx;
                let refresh_id = id.clone();

                view! {
                    <div class="page__header">
                        <div class="page__header-left">
                            <h1 class="page__title">{title}</h1>
                            <span class="text-muted" style="margin-left: 8px">{code}</span>
                        </div>
                        <div class="page__header-right">
                            <div style="display:flex; align-items:center; gap:8px; margin-right: 6px;">
                                <span class="text-muted" style="font-size: 12px;">"Дизайн"</span>
                                <select
                                    class="form__select form__select--sm"
                                    prop:value=move || dashboard_design.get()
                                    on:change=move |ev| {
                                        let target = ev.target().unwrap();
                                        let sel: &web_sys::HtmlSelectElement = target.unchecked_ref();
                                        dashboard_design.set(sel.value());
                                    }
                                >
                                    {move || {
                                        dashboard_design_options
                                            .get()
                                            .into_iter()
                                            .map(|entry| {
                                                view! { <option value=entry.key>{entry.label}</option> }
                                            })
                                            .collect_view()
                                    }}
                                </select>
                            </div>
                            <Button
                                appearance=ButtonAppearance::Secondary
                                on_click=move |_| {
                                    tabs_ctx_edit.open_tab(&detail_tab_key, &detail_tab_title);
                                }
                            >
                                {icon("edit-2")} " Изменить"
                            </Button>
                            <Button
                                appearance=ButtonAppearance::Secondary
                                on_click=move |_| {
                                    reload_dashboard_data(
                                        refresh_id.clone(),
                                        loading,
                                        error,
                                        dashboard,
                                        view_ctx,
                                        dashboard_filter_defs,
                                        indicator_defs,
                                        indicator_values,
                                        true,
                                    );
                                }
                            >
                                {icon("refresh")} " Обновить"
                            </Button>
                        </div>
                    </div>

                    <div class="filter-panel">
                        <div class="filter-panel-header">
                            <div
                                class="filter-panel-header__left"
                                on:click=move |_| is_filter_expanded.update(|e| *e = !*e)
                            >
                                <svg
                                    width="16" height="16" viewBox="0 0 24 24"
                                    fill="none" stroke="currentColor" stroke-width="2"
                                    stroke-linecap="round" stroke-linejoin="round"
                                    class=move || if is_filter_expanded.get() {
                                        "filter-panel__chevron filter-panel__chevron--expanded"
                                    } else {
                                        "filter-panel__chevron"
                                    }
                                >
                                    <polyline points="6 9 12 15 18 9"></polyline>
                                </svg>
                                {icon("filter")}
                                <span class="filter-panel__title">"Фильтры"</span>
                                {move || {
                                    let count = active_filters_count.get();
                                    if count > 0 {
                                        view! { <span class="filter-panel__badge">{count}</span> }.into_any()
                                    } else {
                                        view! { <></> }.into_any()
                                    }
                                }}
                            </div>
                            <div class="filter-panel-header__right" />
                        </div>

                        <Show when=move || is_filter_expanded.get()>
                            <div class="filter-panel-content">
                                <DashboardPeriodControls ctx=view_ctx />
                                <DashboardConnectionMpControls ctx=view_ctx />
                                {move || {
                                    let filters = visible_dashboard_filters.get();
                                    if filters.is_empty() {
                                        view! { <></> }.into_any()
                                    } else {
                                        view! { <FilterBar filters=filters ctx=view_ctx /> }.into_any()
                                    }
                                }}
                            </div>
                        </Show>
                    </div>

                    <div class="dashboard-content" style="position: relative;">
                        <iframe
                            class="dashboard-viewer__iframe"
                            sandbox="allow-scripts"
                            srcdoc=move || srcdoc.get()
                        />
                        <Show when=move || indicator_refreshing.get()>
                            <IndicatorRefreshOverlay
                                card_count=move || indicator_defs.get().len()
                                filter_hint=move || compact_filter_hint(&view_ctx.get())
                            />
                        </Show>
                        <Show when=move || false && indicator_refreshing.get()>
                            <div class="loading-overlay">
                                <div class="loading-overlay__spinner">
                                    <span class="spinner spinner--sm" />
                                    <span>"Обновление индикаторов..."</span>
                                </div>
                            </div>
                        </Show>
                    </div>
                }.into_any()
            } else {
                view! { <div class="placeholder">"Дашборд не найден"</div> }.into_any()
            }}

            {move || {
                let Some(sel) = selected_indicator.get() else {
                    return view! { <></> }.into_any();
                };
                let on_close = Callback::new(move |_| selected_indicator.set(None));
                view! {
                    <IndicatorDetailModal
                        sel=sel
                        indicator_defs=indicator_defs
                        indicator_values=indicator_values
                        on_close=on_close
                        ctx=rendered_ctx.get_untracked()
                    />
                }.into_any()
            }}
        </PageFrame>
    }
}

/// Sends an `indicator_restore` postMessage to the dashboard iframe,
/// telling it to animate the previously selected card back into place.
fn send_indicator_restore() {
    if let Some(win) = web_sys::window() {
        if let Some(doc) = win.document() {
            if let Ok(Some(el)) = doc.query_selector(".dashboard-viewer__iframe") {
                let iframe: &web_sys::HtmlIFrameElement = el.unchecked_ref();
                if let Some(cw) = iframe.content_window() {
                    let msg = js_sys::Object::new();
                    let _ = js_sys::Reflect::set(
                        &msg,
                        &wasm_bindgen::JsValue::from_str("type"),
                        &wasm_bindgen::JsValue::from_str("indicator_restore"),
                    );
                    let _ = cw.post_message(&msg, "*");
                }
            }
        }
    }
}

#[component]
fn IndicatorDetailModal(
    sel: IndicatorSelection,
    indicator_defs: RwSignal<HashMap<String, IndicatorDef>>,
    indicator_values: RwSignal<HashMap<String, ComputedValue>>,
    on_close: Callback<()>,
    ctx: ViewContext,
) -> impl IntoView {
    let def = indicator_defs.get_untracked().get(&sel.id).cloned();
    let computed = indicator_values.get_untracked().get(&sel.id).cloned();

    let name = def
        .as_ref()
        .map(|d| {
            if d.description.trim().is_empty() {
                d.code.clone()
            } else {
                d.description.clone()
            }
        })
        .unwrap_or_else(|| sel.id.clone());

    let code = def.as_ref().map(|d| d.code.clone()).unwrap_or_default();
    let view_id = def
        .as_ref()
        .and_then(|d| d.data_spec.view_id.clone())
        .unwrap_or_default();
    let metric_id = def
        .as_ref()
        .and_then(|d| d.data_spec.metric_id.clone())
        .unwrap_or_default();
    let indicator_default_params = def
        .as_ref()
        .map(indicator_default_params)
        .unwrap_or_default();
    let effective_indicator_params = merge_indicator_params(&indicator_default_params, &ctx.params);
    let format_spec = def
        .as_ref()
        .map(|d| d.view_spec.format.clone())
        .unwrap_or(serde_json::Value::Null);

    // Drilldown: only available when indicator has a DataView (view_id)
    let view_id_opt = def.as_ref().and_then(|d| d.data_spec.view_id.clone());
    let metric_id_opt = def.as_ref().and_then(|d| d.data_spec.metric_id.clone());
    let has_drilldown = view_id_opt.is_some();
    let user_description = build_indicator_description(def.as_ref());
    let computation_details =
        build_indicator_details(def.as_ref(), computed.as_ref(), &effective_indicator_params);
    // Drilldown ("Детализация") is the primary tab; when an indicator has no
    // DataView we open straight on the technical "Подробности" tab.
    let active_tab = RwSignal::new(if has_drilldown { "drill" } else { "details" }.to_string());
    let tabs_store = use_context::<AppGlobalContext>().expect("AppGlobalContext not found");
    let about_description = user_description.clone();
    let about_details = computation_details.clone();

    // Async-загружаемые измерения из DataViewMeta
    let dv_dims: RwSignal<Option<Vec<DrillDim>>> = RwSignal::new(None);
    if let Some(def_for_dims) = def.clone() {
        let params_for_dims = effective_indicator_params.clone();
        let ctx_for_dims = ctx.clone();
        spawn_local(async move {
            match fetch_indicator_drill_dimensions(&def_for_dims, &ctx_for_dims, &params_for_dims)
                .await
            {
                Ok(dims) => {
                    dv_dims.set(Some(dims));
                }
                Err(e) => {
                    let failed_view_id = def_for_dims.data_spec.view_id.clone().unwrap_or_default();
                    leptos::logging::warn!(
                        "Drilldown dimensions fetch failed for {}: {}",
                        failed_view_id,
                        e
                    );
                    dv_dims.set(Some(vec![]));
                }
            }
        });
    }

    let value_full_str = computed
        .as_ref()
        .and_then(|cv| cv.value)
        .map(|v| format_full_value(v, &format_spec))
        .unwrap_or_else(|| "—".to_string());

    let prev_full_str = computed
        .as_ref()
        .and_then(|cv| cv.previous_value)
        .map(|v| format_full_value(v, &format_spec))
        .unwrap_or_else(|| "—".to_string());

    let change_pct = computed.as_ref().and_then(|cv| cv.change_percent);
    let delta_str = change_pct
        .map(|pct| {
            if pct > 0.0 {
                format!("+{:.1}%", pct)
            } else {
                format!("{:.1}%", pct)
            }
        })
        .unwrap_or_else(|| "—".to_string());

    let delta_class = match change_pct {
        Some(p) if p > 0.0 => "indicator-detail__delta--up",
        Some(p) if p < 0.0 => "indicator-detail__delta--down",
        _ => "indicator-detail__delta--flat",
    };

    let status = computed
        .as_ref()
        .and_then(|cv| cv.status.clone())
        .unwrap_or_else(|| "Neutral".to_string());

    let status_class = match status.as_str() {
        "Good" => "indicator-detail__status--good",
        "Bad" => "indicator-detail__status--bad",
        "Warning" => "indicator-detail__status--warning",
        _ => "indicator-detail__status--neutral",
    };

    let current_period_label = period_label(&ctx.date_from, &ctx.date_to);
    let has_period2 = ctx.period2_from.is_some() || ctx.period2_to.is_some();
    let comparison_period_title = if has_period2 {
        "Период сравнения"
    } else {
        "Предыдущий период"
    };
    let comparison_period_label = match (ctx.period2_from.as_deref(), ctx.period2_to.as_deref()) {
        (Some(from), Some(to)) => period_label(from, to),
        (Some(from), None) => period_label(from, ""),
        (None, Some(to)) => period_label("", to),
        (None, None) => "Автоматическое сравнение".to_string(),
    };

    let overview_current_period_label = current_period_label.clone();
    let overview_value_full_str = value_full_str.clone();
    let overview_comparison_period_title = comparison_period_title.to_string();
    let overview_comparison_period_label = comparison_period_label.clone();
    let overview_prev_full_str = prev_full_str.clone();
    let overview_delta_str = delta_str.clone();
    let overview_view_id = view_id.clone();
    let overview_delta_class = delta_class.to_string();
    let overview_subtitle = computed.as_ref().and_then(|value| value.subtitle.clone());
    let overview_effective_indicator_params = StoredValue::new(effective_indicator_params.clone());

    let modal_style = format!(
        "--from-x: {}px; --from-y: {}px;",
        sel.from_x as i32, sel.from_y as i32
    );

    // Closing state: triggers reverse animation before the modal is removed from DOM.
    let is_closing = RwSignal::new(false);

    let do_close = Callback::new(move |_: ()| {
        if is_closing.get_untracked() {
            return;
        }
        is_closing.set(true);
        // Tell the iframe to restore the card immediately (animations overlap naturally).
        send_indicator_restore();
        spawn_local(async move {
            TimeoutFuture::new(220).await;
            on_close.run(());
        });
    });

    // Mouse-down tracking so that dragging out of the overlay does not close it.
    let overlay_mousedown = RwSignal::new(false);

    let is_direct = |ev: &leptos::ev::MouseEvent| -> bool {
        matches!((ev.target(), ev.current_target()), (Some(t), Some(ct)) if t == ct)
    };

    let open_edit = {
        let tabs_store = tabs_store.clone();
        let indicator_id = sel.id.clone();
        let code = code.clone();
        let name = name.clone();
        let do_close = do_close.clone();
        move |_| {
            use crate::layout::tabs::{detail_tab_label, pick_identifier};
            use contracts::domain::a024_bi_indicator::ENTITY_METADATA as A024;

            let identifier = pick_identifier(None, Some(&code), Some(&name), &indicator_id);
            let title = detail_tab_label(A024.ui.element_name, identifier);
            tabs_store.open_tab(
                &format!("a024_bi_indicator_details_{}", indicator_id),
                &title,
            );
            do_close.run(());
        }
    };

    let open_timeline = Callback::new({
        let tabs_store = tabs_store.clone();
        let indicator_id = sel.id.clone();
        let name = name.clone();
        let ctx = ctx.clone();
        let do_close = do_close.clone();
        move |_| {
            let tab_key = format!(
                "bi_timeline__{}__{}__{}__{}__{}__{}",
                indicator_id,
                ctx.date_from,
                ctx.date_to,
                ctx.period2_from.clone().unwrap_or_default(),
                ctx.period2_to.clone().unwrap_or_default(),
                ctx.connection_mp_refs.join(",")
            );
            tabs_store.open_tab(&tab_key, &format!("Timeline · {}", name));
            do_close.run(());
        }
    });

    view! {
        <div
            class=move || {
                if is_closing.get() {
                    "modal-overlay modal-overlay--indicator modal-overlay--closing".to_string()
                } else {
                    "modal-overlay modal-overlay--indicator".to_string()
                }
            }
            style="z-index: 1000;"
            on:mousedown=move |ev: leptos::ev::MouseEvent| {
                overlay_mousedown.set(is_direct(&ev));
            }
            on:click=move |ev: leptos::ev::MouseEvent| {
                if overlay_mousedown.get() && is_direct(&ev) {
                    overlay_mousedown.set(false);
                    do_close.run(());
                }
            }
        >
            <div
                class=move || {
                    if is_closing.get() {
                        "modal indicator-detail-modal indicator-detail-modal--closing".to_string()
                    } else {
                        "modal indicator-detail-modal".to_string()
                    }
                }
                style=modal_style
                on:click=|ev: leptos::ev::MouseEvent| ev.stop_propagation()
            >
                <div class="modal-header indicator-detail__header">
                    <div class="indicator-detail__header-main">
                        <span class="modal-title indicator-detail__title">{name.clone()}</span>
                        {if !code.is_empty() {
                            view! { <span class="indicator-detail__code-badge">{code.clone()}</span> }.into_any()
                        } else {
                            view! { <></> }.into_any()
                        }}
                        <span class=format!("indicator-detail__status {}", status_class)>{status.clone()}</span>
                    </div>
                    <div class="indicator-detail__header-actions">
                        {move || {
                            if !has_drilldown {
                                return view! { <></> }.into_any();
                            }
                            let is_timeline_compatible = dv_dims
                                .get()
                                .map(|dims| {
                                    dims.iter().any(|dim| dim.id == "date" || dim.id == "entry_date")
                                })
                                .unwrap_or(false);
                            if is_timeline_compatible {
                                view! {
                                    <button
                                        type="button"
                                        class="indicator-detail__edit-link"
                                        on:click=move |_| open_timeline.run(())
                                    >
                                        "Timeline"
                                    </button>
                                }
                                .into_any()
                            } else {
                                view! { <></> }.into_any()
                            }
                        }}
                        <button
                            type="button"
                            class="indicator-detail__edit-link"
                            on:click=open_edit
                        >
                            "Изменить"
                        </button>
                        <button
                            class="modal__close"
                            on:click=move |_| do_close.run(())
                            aria-label="Закрыть"
                        >
                            {icon("x")}
                        </button>
                    </div>
                </div>
                <div class="modal-body indicator-detail__body">
                    <div class="detail-tabs">
                        {if has_drilldown {
                            view! {
                                <button
                                    type="button"
                                    class=move || {
                                        if active_tab.get() == "drill" {
                                            "detail-tabs__item detail-tabs__item--active".to_string()
                                        } else {
                                            "detail-tabs__item".to_string()
                                        }
                                    }
                                    on:click=move |_| active_tab.set("drill".to_string())
                                >
                                    "Детализация"
                                </button>
                            }.into_any()
                        } else {
                            view! { <></> }.into_any()
                        }}
                        <button
                            type="button"
                            class=move || {
                                if active_tab.get() == "details" {
                                    "detail-tabs__item detail-tabs__item--active".to_string()
                                } else {
                                    "detail-tabs__item".to_string()
                                }
                            }
                            on:click=move |_| active_tab.set("details".to_string())
                        >
                            "Подробности"
                        </button>
                    </div>

                    {move || if active_tab.get() == "details" || !has_drilldown {
                        view! {
                            <div class="indicator-detail__details-tab">
                                // Comparison block lives on the "Детализация" tab; keep it here
                                // only for indicators that have no drilldown tab to host it.
                                {if !has_drilldown {
                                    view! {
                                        <div class="indicator-detail__periods indicator-detail__periods--with-delta">
                                            <div class="indicator-detail__period-card">
                                                <span class="indicator-detail__period-caption">"Текущий период"</span>
                                                <span class="indicator-detail__period-range">{overview_current_period_label.clone()}</span>
                                                <span class="indicator-detail__period-value">{overview_value_full_str.clone()}</span>
                                            </div>
                                            <div class="indicator-detail__period-card">
                                                <span class="indicator-detail__period-caption">{overview_comparison_period_title.clone()}</span>
                                                <span class="indicator-detail__period-range">{overview_comparison_period_label.clone()}</span>
                                                <span class="indicator-detail__period-value">{overview_prev_full_str.clone()}</span>
                                            </div>
                                            <div class="indicator-detail__period-card indicator-detail__period-card--delta">
                                                <span class="indicator-detail__period-caption">"Изменение"</span>
                                                <span class=format!("indicator-detail__delta {}", overview_delta_class)>{overview_delta_str.clone()}</span>
                                            </div>
                                        </div>
                                    }.into_any()
                                } else {
                                    view! { <></> }.into_any()
                                }}
                                <div class="indicator-detail__meta">
                                    {if !overview_view_id.is_empty() {
                                        view! {
                                            <div class="indicator-detail__meta-row">
                                                <span class="indicator-detail__meta-label">"Источник данных"</span>
                                                <span class="indicator-detail__meta-value">{overview_view_id.clone()}</span>
                                            </div>
                                        }.into_any()
                                    } else {
                                        view! { <></> }.into_any()
                                    }}
                                    {if !metric_id.is_empty() {
                                        view! {
                                            <div class="indicator-detail__meta-row">
                                                <span class="indicator-detail__meta-label">"Метрика"</span>
                                                <span class="indicator-detail__meta-value">{metric_id.clone()}</span>
                                            </div>
                                        }.into_any()
                                    } else {
                                        view! { <></> }.into_any()
                                    }}
                                    {overview_subtitle.clone().map(|subtitle| view! {
                                        <div class="indicator-detail__meta-row">
                                            <span class="indicator-detail__meta-label">"Схема расчёта"</span>
                                            <span class="indicator-detail__meta-value">{subtitle}</span>
                                        </div>
                                    })}
                                </div>
                                <section class="indicator-detail__section">
                                    <span class="indicator-detail__section-eyebrow">"Краткое описание"</span>
                                    {if let Some(text) = about_description.clone() {
                                        view! { <p class="indicator-detail__description">{text}</p> }.into_any()
                                    } else {
                                        view! { <div class="indicator-detail__empty">"Описание для этого индикатора пока не заполнено."</div> }.into_any()
                                    }}
                                </section>
                                <section class="indicator-detail__section">
                                    <span class="indicator-detail__section-eyebrow">"Подробности расчёта"</span>
                                    {if about_details.is_empty() {
                                        view! { <div class="indicator-detail__empty">"Ключевые детали расчёта пока не сформированы."</div> }.into_any()
                                    } else {
                                        view! {
                                            <div class="indicator-detail__details-block">
                                                {about_details.iter().cloned().map(|line| view! {
                                                    <div class="indicator-detail__details-line">{line}</div>
                                                }).collect_view()}
                                            </div>
                                        }.into_any()
                                    }}
                                </section>
                            </div>
                        }.into_any()
                    } else {
                        let indicator_id_c = sel.id.clone();
                        let indicator_name_c = name.clone();
                        let view_id_c = view_id_opt.clone().unwrap_or_default();
                        let metric_id_c = metric_id_opt.clone();
                        let ctx_c = ctx.clone();
                        let tabs_store = Some(tabs_store.clone());

                        view! {
                            <>
                            <div class="indicator-detail__periods indicator-detail__periods--with-delta">
                                <div class="indicator-detail__period-card">
                                    <span class="indicator-detail__period-caption">"Текущий период"</span>
                                    <span class="indicator-detail__period-range">{overview_current_period_label.clone()}</span>
                                    <span class="indicator-detail__period-value">{overview_value_full_str.clone()}</span>
                                </div>
                                <div class="indicator-detail__period-card">
                                    <span class="indicator-detail__period-caption">{overview_comparison_period_title.clone()}</span>
                                    <span class="indicator-detail__period-range">{overview_comparison_period_label.clone()}</span>
                                    <span class="indicator-detail__period-value">{overview_prev_full_str.clone()}</span>
                                </div>
                                <div class="indicator-detail__period-card indicator-detail__period-card--delta">
                                    <span class="indicator-detail__period-caption">"Изменение"</span>
                                    <span class=format!("indicator-detail__delta {}", overview_delta_class)>{overview_delta_str.clone()}</span>
                                </div>
                            </div>
                            <div class="drill-picker">
                                {move || {
                                    match dv_dims.get() {
                                        None => view! {
                                            <div class="drill-picker__loading">
                                                <span class="spinner spinner--sm" />
                                                " Загрузка измерений..."
                                            </div>
                                        }.into_any(),

                                        Some(dims_list) => {
                                            let id = indicator_id_c.clone();
                                            let vid = view_id_c.clone();
                                            let metric = metric_id_c.clone();
                                            let drill_ctx = ctx_c.clone();
                                            let iname = indicator_name_c.clone();
                                            let ts = tabs_store.clone();
                                            let grouped_dims = group_drill_dimensions(&dims_list);
                                            let drill_params = overview_effective_indicator_params.get_value();
                                            let group_cols = balanced_group_columns(grouped_dims.len());

                                            view! {
                                                <div
                                                    class="drill-picker__groups"
                                                    style=format!("--drill-cols: {group_cols};")
                                                >
                                                    {if dims_list.is_empty() {
                                                        view! {
                                                            <div class="drill-picker__empty">
                                                                "Нет общих измерений для выбранных оборотов."
                                                            </div>
                                                        }.into_any()
                                                    } else {
                                                        grouped_dims.into_iter().map(|group| {
                                                            let group_title = group.title;
                                                            let group_accent = group.accent;
                                                            let group_items = group.items;
                                                            let id_group = id.clone();
                                                            let vid_group = vid.clone();
                                                            let metric_group = metric.clone();
                                                            let drill_ctx_group = drill_ctx.clone();
                                                            let iname_group = iname.clone();
                                                            let ts_group = ts.clone();
                                                            let params_group = drill_params.clone();

                                                            view! {
                                                                <section class=format!("drill-picker__group drill-picker__group--{}", group_accent)>
                                                                    <span class="drill-picker__group-title">{group_title}</span>
                                                                    <div class="drill-picker__list">
                                                                        {group_items.into_iter().map(|item| {
                                                                            let dim = item.id.clone();
                                                                            let chip = chip_from_code_main(drill_dim_code_main(&item.id), &item.label);
                                                                            let label_text = item.label.clone();
                                                                            let dim_label = item.label.clone();
                                                                            let is_partial = item.mode == "partial";
                                                                            let coverage = item.coverage_pct;
                                                                            let tab_title = format!("{} · {}", iname_group, dim_label);
                                                                            let store_opt = ts_group.clone();
                                                                            let vid2 = vid_group.clone();
                                                                            let id2 = id_group.clone();
                                                                            let iname2 = iname_group.clone();
                                                                            let metric2 = metric_group.clone();
                                                                            let ctx2 = drill_ctx_group.clone();
                                                                            let params2 = params_group.clone();
                                                                            view! {
                                                                                <button
                                                                                    type="button"
                                                                                    class="drill-picker__item"
                                                                                    on:click=move |_| {
                                                                                        let store_opt = store_opt.clone();
                                                                                        let dim = dim.clone();
                                                                                        let dim_label = dim_label.clone();
                                                                                        let tab_title = tab_title.clone();
                                                                                        let vid2 = vid2.clone();
                                                                                        let id2 = id2.clone();
                                                                                        let iname2 = iname2.clone();
                                                                                        let metric2 = metric2.clone();
                                                                                        let ctx2 = ctx2.clone();
                                                                                        let params2 = params2.clone();

                                                                                        spawn_local(async move {
                                                                                            if let Some(session_id) = post_drilldown_session(
                                                                                                vid2,
                                                                                                id2,
                                                                                                iname2,
                                                                                                metric2,
                                                                                                dim,
                                                                                                dim_label,
                                                                                                ctx2,
                                                                                                params2,
                                                                                            ).await {
                                                                                                let tab_key = format!("drilldown__{}", session_id);
                                                                                                if let Some(ref store) = store_opt {
                                                                                                    store.open_tab(&tab_key, &tab_title);
                                                                                                }
                                                                                            }
                                                                                            do_close.run(());
                                                                                        });
                                                                                    }
                                                                                >
                                                                                    <GlDimensionChip
                                                                                        label=chip.label
                                                                                        color_key=chip.color_key
                                                                                        title=chip.title
                                                                                    />
                                                                                    <span class="drill-picker__item-label">{label_text}</span>
                                                                                    {if is_partial {
                                                                                        view! {
                                                                                            <span class="drill-picker__item-badge drill-picker__item-badge--partial">
                                                                                                {format!("≈{:.0}%", coverage)}
                                                                                            </span>
                                                                                        }.into_any()
                                                                                    } else {
                                                                                        view! { <></> }.into_any()
                                                                                    }}
                                                                                </button>
                                                                            }
                                                                        }).collect_view()}
                                                                    </div>
                                                                </section>
                                                            }
                                                        }).collect_view().into_any()
                                                    }}
                                                </div>
                                            }.into_any()
                                        }
                                    }
                                }}
                            </div>
                            </>
                        }.into_any()
                    }}
                </div>
            </div>
        </div>
    }
}

// ── Drilldown session helper ──────────────────────────────────────────────────

async fn post_drilldown_session(
    view_id: String,
    indicator_id: String,
    indicator_name: String,
    metric_id: Option<String>,
    group_by: String,
    group_by_label: String,
    ctx: ViewContext,
    params: HashMap<String, String>,
) -> Option<String> {
    let body = serde_json::json!({
        "view_id": view_id,
        "indicator_id": indicator_id,
        "indicator_name": indicator_name,
        "metric_id": metric_id,
        "group_by": group_by,
        "group_by_label": group_by_label,
        "date_from": ctx.date_from,
        "date_to": ctx.date_to,
        "period2_from": ctx.period2_from,
        "period2_to": ctx.period2_to,
        "connection_mp_refs": ctx.connection_mp_refs,
        "params": params,
    });

    let url = format!("{}/api/sys-drilldown", api_base());
    let resp = Request::post(&url)
        .header("Content-Type", "application/json")
        .body(body.to_string())
        .ok()?
        .send()
        .await
        .ok()?;

    if !resp.ok() {
        leptos::logging::error!("post_drilldown_session: HTTP {}", resp.status());
        return None;
    }

    let json: serde_json::Value = resp.json().await.ok()?;
    json["session_id"].as_str().map(String::from)
}
