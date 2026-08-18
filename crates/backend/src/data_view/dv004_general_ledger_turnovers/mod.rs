//! dv004 - DataView: General ledger turnovers KPI (2 periods)
//!
//! Scalar data source: sys_general_ledger
//! Required params:
//!   metric = amount (default)
//!   turnover_code = classifier turnover code
//!   layer = oper | fact | plan

use anyhow::{anyhow, Result};
use contracts::general_ledger::GlDrilldownQuery;
use contracts::shared::analytics::{
    IndicatorId, IndicatorStatus, IndicatorValue, SignPolicy, TurnoverClassDef,
};
use contracts::shared::data_view::ViewContext;
use contracts::shared::drilldown::{
    DrilldownCapabilitiesResponse, DrilldownCoverageSummary, DrilldownDimensionCapability,
    DrilldownResponse, DrilldownRow, MetricColumnDef,
};
use sea_orm::{FromQueryResult, Statement};
use std::collections::{HashMap, HashSet};

use crate::general_ledger::drilldown_dimensions::dimensions_for_turnover_at_layer;
use crate::general_ledger::report_repository;
use crate::shared::analytics::turnover_registry::get_turnover_class;
use crate::shared::data::db::get_connection;

const VIEW_ID: &str = "dv004_general_ledger_turnovers";
const DEFAULT_METRIC_ID: &str = "amount";

/// Synthetic drill dimension that splits a composite indicator into the
/// component turnovers it sums over. Only offered for multi-turnover formulas.
const TURNOVER_DIMENSION_ID: &str = "turnover";
const TURNOVER_DIMENSION_LABEL: &str = "По обороту";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ViewMetric {
    Amount,
    EntryCount,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SignedTurnover {
    code: String,
    sign: i8,
}

fn fmt_day_label(iso: &str) -> String {
    let date_part = iso.split('T').next().unwrap_or(iso);
    let parts: Vec<&str> = date_part.split('-').collect();
    if parts.len() >= 3 {
        parts[2]
            .parse::<u32>()
            .map(|d| format!("{:02}", d))
            .unwrap_or_else(|_| iso.to_string())
    } else {
        iso.to_string()
    }
}

fn shift_date(d: &str, months: i32) -> String {
    let parts: Vec<&str> = d.split('-').collect();
    if parts.len() < 3 {
        return d.to_string();
    }
    let y: i32 = parts[0].parse().unwrap_or(2025);
    let m: i32 = parts[1].parse().unwrap_or(1);
    let day: i32 = parts[2].parse().unwrap_or(1);

    let total = y * 12 + (m - 1) + months;
    let ny = total / 12;
    let nm = total % 12 + 1;
    let max_day = match nm {
        2 => {
            if (ny % 4 == 0 && ny % 100 != 0) || ny % 400 == 0 {
                29
            } else {
                28
            }
        }
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    };
    let nd = day.min(max_day);
    format!("{:04}-{:02}-{:02}", ny, nm, nd)
}

fn resolve_period2(ctx: &ViewContext) -> (String, String) {
    match (&ctx.period2_from, &ctx.period2_to) {
        (Some(f), Some(t)) => (f.clone(), t.clone()),
        _ => (shift_date(&ctx.date_from, -1), shift_date(&ctx.date_to, -1)),
    }
}

fn period_label(date_from: &str, date_to: &str) -> String {
    let months = [
        "янв", "фев", "мар", "апр", "май", "июн", "июл", "авг", "сен", "окт", "ноя", "дек",
    ];
    let parts: Vec<&str> = date_from.split('-').collect();
    if parts.len() >= 2 {
        let y = parts[0];
        let m: usize = parts[1].parse().unwrap_or(1);
        let month_name = months.get(m.saturating_sub(1)).copied().unwrap_or("?");
        let to_parts: Vec<&str> = date_to.split('-').collect();
        if to_parts.get(1) == Some(&parts[1]) && to_parts.get(0) == Some(&parts[0]) {
            format!("{} {}", month_name, y)
        } else {
            format!("{} - {}", date_from, date_to)
        }
    } else {
        format!("{} - {}", date_from, date_to)
    }
}

fn pct_change(cur: f64, prev: f64) -> Option<f64> {
    if prev.abs() < 0.01 {
        None
    } else {
        Some(((cur - prev) / prev.abs()) * 100.0)
    }
}

fn status_by_sign_policy(sign_policy: SignPolicy, change: Option<f64>) -> IndicatorStatus {
    match sign_policy {
        SignPolicy::IncomePositive => match change {
            Some(c) if c > 5.0 => IndicatorStatus::Good,
            Some(c) if c < -5.0 => IndicatorStatus::Bad,
            _ => IndicatorStatus::Neutral,
        },
        SignPolicy::ExpensePositive => match change {
            Some(c) if c > 5.0 => IndicatorStatus::Bad,
            Some(c) if c < -5.0 => IndicatorStatus::Good,
            _ => IndicatorStatus::Neutral,
        },
        SignPolicy::Natural => IndicatorStatus::Neutral,
    }
}

fn resolve_metric(ctx: &ViewContext) -> Result<ViewMetric> {
    let metric = ctx
        .params
        .get("metric")
        .map(|value| value.as_str())
        .unwrap_or(DEFAULT_METRIC_ID);
    match metric {
        "amount" => Ok(ViewMetric::Amount),
        "entry_count" => Ok(ViewMetric::EntryCount),
        _ => Err(anyhow!(
            "Unsupported metric '{}' for {}. Expected amount | entry_count",
            metric,
            VIEW_ID
        )),
    }
}

fn metric_label(metric: ViewMetric) -> &'static str {
    match metric {
        ViewMetric::Amount => "Сумма оборота",
        ViewMetric::EntryCount => "Количество проводок",
    }
}

fn display_formula_verbose(turnovers: &[SignedTurnover]) -> String {
    let mut out = String::new();
    for (index, turnover) in turnovers.iter().enumerate() {
        if index > 0 {
            if turnover.sign >= 0 {
                out.push_str(" + ");
            } else {
                out.push_str(" - ");
            }
        } else if turnover.sign < 0 {
            out.push('-');
        }

        let label = get_turnover_class(&turnover.code)
            .map(|class| format!("{} ({})", class.name, class.code))
            .unwrap_or_else(|| turnover.code.clone());
        out.push_str(&label);
    }
    out
}

fn required_param<'a>(ctx: &'a ViewContext, key: &str) -> Result<&'a str> {
    ctx.params
        .get(key)
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("Missing required param '{}' for {}", key, VIEW_ID))
}

fn parse_signed_turnovers(raw: &str) -> Result<Vec<SignedTurnover>> {
    let mut result = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for token in raw
        .split(|ch| ch == ',' || ch == ';' || ch == '\n' || ch == '\r')
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let (sign, code) = match token.chars().next() {
            Some('+') => (1_i8, token[1..].trim()),
            Some('-') => (-1_i8, token[1..].trim()),
            _ => (1_i8, token),
        };

        if code.is_empty() {
            return Err(anyhow!(
                "Empty turnover code in turnover_items for {}",
                VIEW_ID
            ));
        }

        if !seen.insert(code.to_string()) {
            return Err(anyhow!(
                "Duplicate turnover_code '{}' in turnover_items for {}",
                code,
                VIEW_ID
            ));
        }

        result.push(SignedTurnover {
            code: code.to_string(),
            sign,
        });
    }

    if result.is_empty() {
        return Err(anyhow!(
            "turnover_items must contain at least one turnover_code for {}",
            VIEW_ID
        ));
    }

    Ok(result)
}

fn resolve_turnovers(ctx: &ViewContext) -> Result<(Vec<SignedTurnover>, String)> {
    let layer = required_param(ctx, "layer")?.to_string();
    let turnovers = if let Some(raw) = ctx
        .params
        .get("turnover_items")
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
    {
        parse_signed_turnovers(raw)?
    } else {
        vec![SignedTurnover {
            code: required_param(ctx, "turnover_code")?.to_string(),
            sign: 1,
        }]
    };

    for turnover in &turnovers {
        get_turnover_class(&turnover.code)
            .ok_or_else(|| anyhow!("Unknown turnover_code '{}' for {}", turnover.code, VIEW_ID))?;
    }

    Ok((turnovers, layer))
}

fn display_formula(turnovers: &[SignedTurnover]) -> String {
    let mut out = String::new();
    for (index, turnover) in turnovers.iter().enumerate() {
        if index > 0 {
            if turnover.sign >= 0 {
                out.push_str(" + ");
            } else {
                out.push_str(" - ");
            }
        } else if turnover.sign < 0 {
            out.push('-');
        }
        out.push_str(&turnover.code);
    }
    out
}

fn resolve_primary_class(turnovers: &[SignedTurnover]) -> Result<&'static TurnoverClassDef> {
    let first = turnovers
        .first()
        .ok_or_else(|| anyhow!("No turnovers configured for {}", VIEW_ID))?;
    get_turnover_class(&first.code)
        .ok_or_else(|| anyhow!("Unknown turnover_code '{}' for {}", first.code, VIEW_ID))
}

fn resolve_sign_policy(turnovers: &[SignedTurnover]) -> SignPolicy {
    let mut policies = turnovers
        .iter()
        .filter_map(|turnover| get_turnover_class(&turnover.code).map(|class| class.sign_policy));

    let Some(first) = policies.next() else {
        return SignPolicy::Natural;
    };

    if policies.all(|policy| policy == first) {
        first
    } else {
        SignPolicy::Natural
    }
}

fn common_dimensions_for_turnovers(
    turnovers: &[SignedTurnover],
    layer: &str,
) -> Vec<contracts::general_ledger::GlDimensionDef> {
    let Some(first) = turnovers.first() else {
        return Vec::new();
    };

    let first_dimensions = dimensions_for_turnover_at_layer(&first.code, layer);
    first_dimensions
        .into_iter()
        .filter(|dimension| {
            turnovers.iter().skip(1).all(|turnover| {
                dimensions_for_turnover_at_layer(&turnover.code, layer)
                    .iter()
                    .any(|candidate| candidate.id == dimension.id)
            })
        })
        .collect()
}

fn union_dimensions_for_turnovers(
    turnovers: &[SignedTurnover],
    layer: &str,
) -> Vec<contracts::general_ledger::GlDimensionDef> {
    let mut seen = HashSet::new();
    let mut result = Vec::new();

    for turnover in turnovers {
        for dimension in dimensions_for_turnover_at_layer(&turnover.code, layer) {
            if seen.insert(dimension.id.clone()) {
                result.push(dimension);
            }
        }
    }

    result
}

fn turnover_supports_dimension(turnover_code: &str, dimension_id: &str, layer: &str) -> bool {
    dimensions_for_turnover_at_layer(turnover_code, layer)
        .iter()
        .any(|dimension| dimension.id == dimension_id)
}

fn round_pct(value: f64) -> f64 {
    (value * 10.0).round() / 10.0
}

fn compute_coverage_pct(
    turnovers: &[SignedTurnover],
    supported_turnover_codes: &[String],
    component_totals: &HashMap<String, f64>,
) -> Option<f64> {
    if turnovers.is_empty() {
        return Some(100.0);
    }

    let supported: HashSet<&str> = supported_turnover_codes
        .iter()
        .map(String::as_str)
        .collect();
    let total_abs = turnovers
        .iter()
        .map(|turnover| {
            component_totals
                .get(&turnover.code)
                .copied()
                .unwrap_or(0.0)
                .abs()
        })
        .sum::<f64>();

    if total_abs >= 0.01 {
        let covered_abs = turnovers
            .iter()
            .filter(|turnover| supported.contains(turnover.code.as_str()))
            .map(|turnover| {
                component_totals
                    .get(&turnover.code)
                    .copied()
                    .unwrap_or(0.0)
                    .abs()
            })
            .sum::<f64>();
        return Some(round_pct((covered_abs / total_abs) * 100.0));
    }

    let supported_count = turnovers
        .iter()
        .filter(|turnover| supported.contains(turnover.code.as_str()))
        .count();
    Some(round_pct(
        (supported_count as f64 / turnovers.len() as f64) * 100.0,
    ))
}

fn build_metric_case_expr(alias: &str, turnovers: &[SignedTurnover], metric: ViewMetric) -> String {
    let when_clauses = turnovers
        .iter()
        .map(|turnover| match metric {
            ViewMetric::Amount => format!(
                "WHEN {alias}.turnover_code = ? THEN COALESCE({alias}.amount, 0) * {}.0",
                turnover.sign
            ),
            ViewMetric::EntryCount => {
                format!("WHEN {alias}.turnover_code = ? THEN {}.0", turnover.sign)
            }
        })
        .collect::<Vec<_>>()
        .join(" ");
    format!("CASE {when_clauses} ELSE 0 END")
}

#[derive(Debug, FromQueryResult)]
struct AggRow {
    total: f64,
}

#[derive(Debug, FromQueryResult)]
struct DailyRow {
    total: f64,
}

#[derive(Debug, FromQueryResult)]
struct TurnoverTotalRow {
    turnover_code: String,
    total: f64,
}

async fn fetch_aggregate_from_table(
    table_name: &str,
    metric: ViewMetric,
    turnovers: &[SignedTurnover],
    layer: &str,
    date_from: &str,
    date_to: &str,
    connection_mp_refs: &[String],
) -> Result<f64> {
    let db = get_connection();
    let metric_expr = build_metric_case_expr("t", turnovers, metric);
    let turnover_placeholders: Vec<&str> = turnovers.iter().map(|_| "?").collect();
    let mut sql = format!(
        r#"
        SELECT CAST(COALESCE(SUM({metric_expr}), 0) AS REAL) AS total
        FROM {table_name} t
        WHERE t.turnover_code IN ({turnover_placeholders})
          AND t.layer = ?
          AND t.entry_date >= ? AND t.entry_date <= ?
        "#,
        turnover_placeholders = turnover_placeholders.join(", ")
    );
    let mut params: Vec<sea_orm::Value> = turnovers
        .iter()
        .map(|turnover| turnover.code.clone().into())
        .collect();
    params.extend(
        turnovers
            .iter()
            .map(|turnover| turnover.code.clone().into())
            .collect::<Vec<_>>(),
    );
    params.push(layer.to_string().into());
    params.push(date_from.to_string().into());
    params.push(date_to.to_string().into());

    if !connection_mp_refs.is_empty() {
        let placeholders: Vec<&str> = connection_mp_refs.iter().map(|_| "?").collect();
        sql.push_str(&format!(
            " AND t.connection_mp_ref IN ({})",
            placeholders.join(", ")
        ));
        for value in connection_mp_refs {
            params.push(value.clone().into());
        }
    }

    let stmt = Statement::from_sql_and_values(sea_orm::DatabaseBackend::Sqlite, &sql, params);
    let row = AggRow::find_by_statement(stmt)
        .one(db)
        .await?
        .unwrap_or(AggRow { total: 0.0 });
    Ok(row.total)
}

async fn fetch_daily_rows_from_table(
    table_name: &str,
    metric: ViewMetric,
    turnovers: &[SignedTurnover],
    layer: &str,
    date_from: &str,
    date_to: &str,
    connection_mp_refs: &[String],
) -> Result<Vec<DailyRow>> {
    let db = get_connection();
    let metric_expr = build_metric_case_expr("t", turnovers, metric);
    let turnover_placeholders: Vec<&str> = turnovers.iter().map(|_| "?").collect();
    let mut sql = format!(
        r#"
        SELECT
            CAST(COALESCE(SUM({metric_expr}), 0) AS REAL) AS total
        FROM {table_name} t
        WHERE t.turnover_code IN ({turnover_placeholders})
          AND t.layer = ?
          AND t.entry_date >= ? AND t.entry_date <= ?
        "#,
        turnover_placeholders = turnover_placeholders.join(", ")
    );
    let mut params: Vec<sea_orm::Value> = turnovers
        .iter()
        .map(|turnover| turnover.code.clone().into())
        .collect();
    params.extend(
        turnovers
            .iter()
            .map(|turnover| turnover.code.clone().into())
            .collect::<Vec<_>>(),
    );
    params.push(layer.to_string().into());
    params.push(date_from.to_string().into());
    params.push(date_to.to_string().into());

    if !connection_mp_refs.is_empty() {
        let placeholders: Vec<&str> = connection_mp_refs.iter().map(|_| "?").collect();
        sql.push_str(&format!(
            " AND t.connection_mp_ref IN ({})",
            placeholders.join(", ")
        ));
        for value in connection_mp_refs {
            params.push(value.clone().into());
        }
    }

    sql.push_str(" GROUP BY DATE(t.entry_date) ORDER BY DATE(t.entry_date) ASC");
    let stmt = Statement::from_sql_and_values(sea_orm::DatabaseBackend::Sqlite, &sql, params);
    Ok(DailyRow::find_by_statement(stmt).all(db).await?)
}

async fn fetch_turnover_component_totals_from_table(
    table_name: &str,
    metric: ViewMetric,
    turnovers: &[SignedTurnover],
    layer: &str,
    date_from: &str,
    date_to: &str,
    connection_mp_refs: &[String],
) -> Result<HashMap<String, f64>> {
    let db = get_connection();
    let turnover_placeholders: Vec<&str> = turnovers.iter().map(|_| "?").collect();
    let metric_expr = match metric {
        ViewMetric::Amount => "CAST(COALESCE(SUM(COALESCE(t.amount, 0)), 0) AS REAL)",
        ViewMetric::EntryCount => "CAST(COUNT(*) AS REAL)",
    };
    let mut sql = format!(
        r#"
        SELECT
            t.turnover_code,
            {metric_expr} AS total
        FROM {table_name} t
        WHERE t.turnover_code IN ({turnover_placeholders})
          AND t.layer = ?
          AND t.entry_date >= ? AND t.entry_date <= ?
        "#,
        turnover_placeholders = turnover_placeholders.join(", ")
    );
    let mut params: Vec<sea_orm::Value> = turnovers
        .iter()
        .map(|turnover| turnover.code.clone().into())
        .collect();
    params.push(layer.to_string().into());
    params.push(date_from.to_string().into());
    params.push(date_to.to_string().into());

    if !connection_mp_refs.is_empty() {
        let placeholders: Vec<&str> = connection_mp_refs.iter().map(|_| "?").collect();
        sql.push_str(&format!(
            " AND t.connection_mp_ref IN ({})",
            placeholders.join(", ")
        ));
        for value in connection_mp_refs {
            params.push(value.clone().into());
        }
    }

    sql.push_str(" GROUP BY t.turnover_code");
    let stmt = Statement::from_sql_and_values(sea_orm::DatabaseBackend::Sqlite, &sql, params);
    let rows = TurnoverTotalRow::find_by_statement(stmt).all(db).await?;
    let raw_totals = rows
        .into_iter()
        .map(|row| (row.turnover_code, row.total))
        .collect::<HashMap<_, _>>();

    Ok(turnovers
        .iter()
        .map(|turnover| {
            (
                turnover.code.clone(),
                raw_totals.get(&turnover.code).copied().unwrap_or(0.0) * f64::from(turnover.sign),
            )
        })
        .collect())
}

async fn build_drilldown_capabilities(
    metric: ViewMetric,
    turnovers: &[SignedTurnover],
    layer: &str,
    ctx: &ViewContext,
) -> Result<DrilldownCapabilitiesResponse> {
    if turnovers.len() == 1 {
        let turnover = turnovers
            .first()
            .ok_or_else(|| anyhow!("No turnovers configured for {}", VIEW_ID))?;
        let safe_dimensions = dimensions_for_turnover_at_layer(&turnover.code, layer)
            .into_iter()
            .map(|dimension| DrilldownDimensionCapability {
                id: dimension.id,
                label: dimension.label,
                mode: "safe".to_string(),
                coverage_pct: Some(100.0),
                supported_turnover_codes: vec![turnover.code.clone()],
                missing_turnover_codes: Vec::new(),
            })
            .collect();

        return Ok(DrilldownCapabilitiesResponse {
            safe_dimensions,
            partial_dimensions: Vec::new(),
        });
    }

    let component_totals = fetch_turnover_component_totals_from_table(
        "sys_general_ledger",
        metric,
        turnovers,
        layer,
        &ctx.date_from,
        &ctx.date_to,
        &ctx.connection_mp_refs,
    )
    .await?;

    let safe_dimension_ids = common_dimensions_for_turnovers(turnovers, layer)
        .into_iter()
        .map(|dimension| dimension.id)
        .collect::<HashSet<_>>();

    let mut safe_dimensions = Vec::new();
    let mut partial_dimensions = Vec::new();

    for dimension in union_dimensions_for_turnovers(turnovers, layer) {
        let supported_turnover_codes = turnovers
            .iter()
            .filter(|turnover| turnover_supports_dimension(&turnover.code, &dimension.id, layer))
            .map(|turnover| turnover.code.clone())
            .collect::<Vec<_>>();
        let missing_turnover_codes = turnovers
            .iter()
            .filter(|turnover| !turnover_supports_dimension(&turnover.code, &dimension.id, layer))
            .map(|turnover| turnover.code.clone())
            .collect::<Vec<_>>();
        let is_safe = safe_dimension_ids.contains(&dimension.id);
        let capability = DrilldownDimensionCapability {
            id: dimension.id,
            label: dimension.label,
            mode: if is_safe {
                "safe".to_string()
            } else {
                "partial".to_string()
            },
            coverage_pct: Some(if is_safe {
                100.0
            } else {
                compute_coverage_pct(turnovers, &supported_turnover_codes, &component_totals)
                    .unwrap_or(0.0)
            }),
            supported_turnover_codes,
            missing_turnover_codes,
        };

        if is_safe {
            safe_dimensions.push(capability);
        } else {
            partial_dimensions.push(capability);
        }
    }

    // Synthetic dimension: decompose a composite indicator into its component
    // turnovers. Each turnover becomes its own row, so coverage is always 100%.
    // Placed first so it leads the "Основные" group in the UI.
    safe_dimensions.insert(
        0,
        DrilldownDimensionCapability {
            id: TURNOVER_DIMENSION_ID.to_string(),
            label: TURNOVER_DIMENSION_LABEL.to_string(),
            mode: "safe".to_string(),
            coverage_pct: Some(100.0),
            supported_turnover_codes: turnovers
                .iter()
                .map(|turnover| turnover.code.clone())
                .collect(),
            missing_turnover_codes: Vec::new(),
        },
    );

    Ok(DrilldownCapabilitiesResponse {
        safe_dimensions,
        partial_dimensions,
    })
}

fn find_dimension_capability(
    capabilities: &DrilldownCapabilitiesResponse,
    group_by: &str,
) -> Option<DrilldownDimensionCapability> {
    capabilities
        .safe_dimensions
        .iter()
        .chain(capabilities.partial_dimensions.iter())
        .find(|capability| capability.id == group_by)
        .cloned()
}

pub async fn compute_scalar(ctx: &ViewContext) -> Result<IndicatorValue> {
    let metric = resolve_metric(ctx)?;
    let (turnovers, layer) = resolve_turnovers(ctx)?;
    let primary_class = resolve_primary_class(&turnovers)?;
    let sign_policy = match metric {
        ViewMetric::Amount => resolve_sign_policy(&turnovers),
        ViewMetric::EntryCount => SignPolicy::Natural,
    };
    let (p2_from, p2_to) = resolve_period2(ctx);

    let (current_result, daily_result, prev_result) = tokio::join!(
        fetch_aggregate_from_table(
            "sys_general_ledger",
            metric,
            &turnovers,
            &layer,
            &ctx.date_from,
            &ctx.date_to,
            &ctx.connection_mp_refs
        ),
        fetch_daily_rows_from_table(
            "sys_general_ledger",
            metric,
            &turnovers,
            &layer,
            &ctx.date_from,
            &ctx.date_to,
            &ctx.connection_mp_refs
        ),
        fetch_aggregate_from_table(
            "sys_general_ledger",
            metric,
            &turnovers,
            &layer,
            &p2_from,
            &p2_to,
            &ctx.connection_mp_refs
        ),
    );

    let current = current_result?;
    let daily = daily_result?;
    let previous = prev_result?;
    let change = pct_change(current, previous);
    let mut details = Vec::new();
    if turnovers.len() == 1 {
        details.push(format!(
            "Оборот: {} ({})",
            primary_class.name, primary_class.code
        ));
    } else {
        details.push(format!("Формула: {}", display_formula_verbose(&turnovers)));
    }
    details.push(format!("Слой: {}", layer));
    details.push(format!("Метрика: {}", metric_label(metric)));

    Ok(IndicatorValue {
        id: IndicatorId::new(VIEW_ID),
        value: Some(current),
        previous_value: Some(previous),
        change_percent: change,
        status: status_by_sign_policy(sign_policy, change),
        subtitle: Some(format!(
            "{} [{} / {}]",
            if turnovers.len() == 1 {
                primary_class.name.to_string()
            } else {
                display_formula(&turnovers)
            },
            layer,
            metric_label(metric)
        )),
        details,
        spark_points: daily.into_iter().map(|row| row.total).collect(),
    })
}

fn metric_ids_supported(metric_ids: &[String]) -> Result<()> {
    if metric_ids.is_empty() || metric_ids.len() == 1 {
        Ok(())
    } else {
        Err(anyhow!(
            "{} supports only a single metric per drilldown request",
            VIEW_ID
        ))
    }
}

pub async fn compute_drilldown_capabilities(
    ctx: &ViewContext,
) -> Result<DrilldownCapabilitiesResponse> {
    let metric = resolve_metric(ctx)?;
    let (turnovers, layer) = resolve_turnovers(ctx)?;
    build_drilldown_capabilities(metric, &turnovers, &layer, ctx).await
}

/// Метка строки: для регистратора — реальное представление документа
/// (наименование · дата · #номер), иначе — день/сырое значение (через `day_key`).
fn registrator_label<F: Fn(&str) -> String>(
    row: &contracts::general_ledger::GlDrilldownRow,
    day_key: F,
) -> String {
    match &row.representation {
        Some(rep) => crate::shared::representation::to_label(rep),
        None => day_key(&row.group_label),
    }
}

pub async fn compute_drilldown(ctx: &ViewContext, group_by: &str) -> Result<DrilldownResponse> {
    let metric = resolve_metric(ctx)?;
    let (turnovers, layer) = resolve_turnovers(ctx)?;
    let capabilities = build_drilldown_capabilities(metric, &turnovers, &layer, ctx).await?;
    let selected_dimension =
        find_dimension_capability(&capabilities, group_by).ok_or_else(|| {
            anyhow!(
                "Unsupported group_by '{}' for turnover formula '{}'",
                group_by,
                display_formula(&turnovers)
            )
        })?;

    if group_by == TURNOVER_DIMENSION_ID {
        return build_turnover_breakdown(metric, &turnovers, &layer, ctx, selected_dimension).await;
    }

    let supported_turnovers = turnovers
        .iter()
        .filter(|turnover| {
            selected_dimension
                .supported_turnover_codes
                .iter()
                .any(|code| code == &turnover.code)
        })
        .cloned()
        .collect::<Vec<_>>();
    let layer_filter: String = layer;

    let (p2_from, p2_to) = resolve_period2(ctx);
    let query_p1 = GlDrilldownQuery {
        turnover_code: String::new(),
        group_by: group_by.to_string(),
        date_from: ctx.date_from.clone(),
        date_to: ctx.date_to.clone(),
        connection_mp_ref: None,
        connection_mp_refs: ctx.connection_mp_refs.clone(),
        account: None,
        layer: Some(layer_filter.clone()),
        entity: None,
        corr_account: None,
    };
    let query_p2 = GlDrilldownQuery {
        turnover_code: String::new(),
        group_by: group_by.to_string(),
        date_from: p2_from.clone(),
        date_to: p2_to.clone(),
        connection_mp_ref: None,
        connection_mp_refs: ctx.connection_mp_refs.clone(),
        account: None,
        layer: Some(layer_filter.clone()),
        entity: None,
        corr_account: None,
    };

    let rows1 = fetch_composite_drilldown(metric, &supported_turnovers, &query_p1).await?;
    let rows2 = fetch_composite_drilldown(metric, &supported_turnovers, &query_p2).await?;
    let covered_total1: f64 = rows1.iter().map(|row| row.amount).sum();
    let covered_total2: f64 = rows2.iter().map(|row| row.amount).sum();
    let has_full_coverage = supported_turnovers.len() == turnovers.len();
    let total1 = if has_full_coverage {
        covered_total1
    } else {
        fetch_aggregate_from_table(
            "sys_general_ledger",
            metric,
            &turnovers,
            &layer_filter,
            &ctx.date_from,
            &ctx.date_to,
            &ctx.connection_mp_refs,
        )
        .await?
    };
    let total2 = if has_full_coverage {
        covered_total2
    } else {
        fetch_aggregate_from_table(
            "sys_general_ledger",
            metric,
            &turnovers,
            &layer_filter,
            &p2_from,
            &p2_to,
            &ctx.connection_mp_refs,
        )
        .await?
    };

    let is_date_group = group_by == "entry_date";
    // По дням сливаем строки по дню месяца, чтобы один и тот же день из П1 и П2
    // совпадал (01 ↔ 01), даже когда периоды лежат в разных месяцах.
    let day_key = |raw: &str| {
        if is_date_group {
            fmt_day_label(raw)
        } else {
            raw.to_string()
        }
    };

    let mut merged: HashMap<String, DrilldownRow> = HashMap::new();

    for row in rows1 {
        let key = day_key(&row.group_key);
        // Для разреза по регистратору метка = реальное представление документа
        // (наименование · дата · #номер), иначе — день/сырое значение.
        let label = registrator_label(&row, &day_key);
        merged
            .entry(key.clone())
            .or_insert_with(|| DrilldownRow {
                group_key: key,
                label,
                value1: 0.0,
                value2: 0.0,
                delta_pct: None,
                metric_values: HashMap::new(),
            })
            .value1 += row.amount;
    }

    for row in rows2 {
        let key = day_key(&row.group_key);
        let label = registrator_label(&row, &day_key);
        merged
            .entry(key.clone())
            .or_insert_with(|| DrilldownRow {
                group_key: key,
                label,
                value1: 0.0,
                value2: 0.0,
                delta_pct: None,
                metric_values: HashMap::new(),
            })
            .value2 += row.amount;
    }

    let mut rows: Vec<DrilldownRow> = merged
        .into_values()
        .map(|mut row| {
            row.delta_pct = pct_change(row.value1, row.value2);
            row
        })
        .collect();

    let other_value1 = total1 - covered_total1;
    let other_value2 = total2 - covered_total2;
    if other_value1.abs() >= 0.01 || other_value2.abs() >= 0.01 {
        rows.push(DrilldownRow {
            group_key: "__other__".to_string(),
            label: "Прочее".to_string(),
            value1: other_value1,
            value2: other_value2,
            delta_pct: pct_change(other_value1, other_value2),
            metric_values: HashMap::new(),
        });
    }

    if is_date_group {
        rows.sort_by(|a, b| a.group_key.cmp(&b.group_key));
    } else {
        rows.sort_by(|a, b| {
            b.value1
                .partial_cmp(&a.value1)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
    }

    let coverage_pct_period1 = if selected_dimension.mode == "safe" {
        Some(100.0)
    } else {
        let component_totals_period1 = fetch_turnover_component_totals_from_table(
            "sys_general_ledger",
            metric,
            &turnovers,
            &layer_filter,
            &ctx.date_from,
            &ctx.date_to,
            &ctx.connection_mp_refs,
        )
        .await?;
        compute_coverage_pct(
            &turnovers,
            &selected_dimension.supported_turnover_codes,
            &component_totals_period1,
        )
    };
    let coverage_pct_period2 = if selected_dimension.mode == "safe" {
        Some(100.0)
    } else {
        let component_totals_period2 = fetch_turnover_component_totals_from_table(
            "sys_general_ledger",
            metric,
            &turnovers,
            &layer_filter,
            &p2_from,
            &p2_to,
            &ctx.connection_mp_refs,
        )
        .await?;
        compute_coverage_pct(
            &turnovers,
            &selected_dimension.supported_turnover_codes,
            &component_totals_period2,
        )
    };
    let coverage = DrilldownCoverageSummary {
        mode: selected_dimension.mode.clone(),
        coverage_pct_period1,
        coverage_pct_period2,
        covered_value1: covered_total1,
        covered_value2: covered_total2,
        other_value1,
        other_value2,
    };

    // Доп. колонки, зависящие от измерения (напр. «Артикул» для номенклатуры).
    // Один батч-запрос по уже посчитанным ключам — не утяжеляет агрегацию.
    let group_keys: Vec<String> = rows.iter().map(|row| row.group_key.clone()).collect();
    let extra = crate::general_ledger::drilldown_extra::resolve(group_by, &group_keys)
        .await
        .unwrap_or_default();

    Ok(DrilldownResponse {
        rows,
        group_by_label: crate::general_ledger::drilldown_dimensions::dimension_label(group_by)
            .unwrap_or(group_by)
            .to_string(),
        period1_label: period_label(&ctx.date_from, &ctx.date_to),
        period2_label: period_label(&p2_from, &p2_to),
        metric_label: if turnovers.len() == 1 {
            format!(
                "{} / {}",
                resolve_primary_class(&turnovers)?.name,
                metric_label(metric)
            )
        } else {
            format!("{} / {}", display_formula(&turnovers), metric_label(metric))
        },
        metric_columns: vec![],
        selected_dimension: Some(selected_dimension),
        coverage: Some(coverage),
        extra_columns: extra.columns,
        extra_values: extra.values,
    })
}

/// Build the "по обороту" breakdown: one row per component turnover, with the
/// signed contribution of each to the composite value. Coverage is always 100%
/// (every turnover is its own row), so there is no "Прочее" remainder.
async fn build_turnover_breakdown(
    metric: ViewMetric,
    turnovers: &[SignedTurnover],
    layer: &str,
    ctx: &ViewContext,
    selected_dimension: DrilldownDimensionCapability,
) -> Result<DrilldownResponse> {
    let (p2_from, p2_to) = resolve_period2(ctx);

    let (totals1, totals2) = tokio::join!(
        fetch_turnover_component_totals_from_table(
            "sys_general_ledger",
            metric,
            turnovers,
            layer,
            &ctx.date_from,
            &ctx.date_to,
            &ctx.connection_mp_refs,
        ),
        fetch_turnover_component_totals_from_table(
            "sys_general_ledger",
            metric,
            turnovers,
            layer,
            &p2_from,
            &p2_to,
            &ctx.connection_mp_refs,
        ),
    );
    let totals1 = totals1?;
    let totals2 = totals2?;

    let mut rows: Vec<DrilldownRow> = turnovers
        .iter()
        .map(|turnover| {
            let value1 = totals1.get(&turnover.code).copied().unwrap_or(0.0);
            let value2 = totals2.get(&turnover.code).copied().unwrap_or(0.0);
            let label = get_turnover_class(&turnover.code)
                .map(|class| class.name.to_string())
                .unwrap_or_else(|| turnover.code.clone());
            DrilldownRow {
                group_key: turnover.code.clone(),
                label,
                value1,
                value2,
                delta_pct: pct_change(value1, value2),
                metric_values: HashMap::new(),
            }
        })
        .collect();
    rows.sort_by(|a, b| {
        b.value1
            .partial_cmp(&a.value1)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let covered_total1: f64 = rows.iter().map(|row| row.value1).sum();
    let covered_total2: f64 = rows.iter().map(|row| row.value2).sum();

    let coverage = DrilldownCoverageSummary {
        mode: "safe".to_string(),
        coverage_pct_period1: Some(100.0),
        coverage_pct_period2: Some(100.0),
        covered_value1: covered_total1,
        covered_value2: covered_total2,
        other_value1: 0.0,
        other_value2: 0.0,
    };

    Ok(DrilldownResponse {
        rows,
        group_by_label: TURNOVER_DIMENSION_LABEL.to_string(),
        period1_label: period_label(&ctx.date_from, &ctx.date_to),
        period2_label: period_label(&p2_from, &p2_to),
        metric_label: format!("{} / {}", display_formula(turnovers), metric_label(metric)),
        metric_columns: vec![],
        selected_dimension: Some(selected_dimension),
        coverage: Some(coverage),
        extra_columns: vec![],
        extra_values: std::collections::HashMap::new(),
    })
}

pub async fn compute_drilldown_multi(
    ctx: &ViewContext,
    group_by: &str,
    metric_ids: &[String],
) -> Result<DrilldownResponse> {
    metric_ids_supported(metric_ids)?;
    if metric_ids.is_empty() {
        compute_drilldown(ctx, group_by).await
    } else {
        let metric_id = metric_ids[0].trim();
        match metric_id {
            "amount" | "entry_count" => {}
            other => return Err(anyhow!("Unsupported metric '{}' for {}", other, VIEW_ID)),
        };

        let mut next_ctx = ctx.clone();
        next_ctx
            .params
            .insert("metric".to_string(), metric_id.to_string());
        let mut response = compute_drilldown(&next_ctx, group_by).await?;
        let metric_label = response.metric_label.clone();
        response.metric_label = String::new();
        response.metric_columns = vec![MetricColumnDef {
            id: metric_id.to_string(),
            label: metric_label.clone(),
        }];
        for row in &mut response.rows {
            row.metric_values.insert(
                metric_id.to_string(),
                contracts::shared::drilldown::MetricValues {
                    value1: row.value1,
                    value2: row.value2,
                    delta_pct: row.delta_pct,
                },
            );
        }
        Ok(response)
    }
}

async fn fetch_composite_drilldown(
    metric: ViewMetric,
    turnovers: &[SignedTurnover],
    base_query: &GlDrilldownQuery,
) -> Result<Vec<contracts::general_ledger::GlDrilldownRow>> {
    let mut merged: HashMap<String, contracts::general_ledger::GlDrilldownRow> = HashMap::new();

    for turnover in turnovers {
        let mut query = base_query.clone();
        query.turnover_code = turnover.code.clone();
        let response = report_repository::get_drilldown(&query).await?;
        for row in response.rows {
            let entry = merged.entry(row.group_key.clone()).or_insert_with(|| {
                contracts::general_ledger::GlDrilldownRow {
                    group_key: row.group_key.clone(),
                    group_label: row.group_label.clone(),
                    amount: 0.0,
                    entry_count: 0,
                    representation: row.representation.clone(),
                }
            });
            let signed_amount = f64::from(turnover.sign) * row.amount;
            let signed_count = i64::from(turnover.sign) * i64::from(row.entry_count);
            entry.amount += match metric {
                ViewMetric::Amount => signed_amount,
                ViewMetric::EntryCount => signed_count as f64,
            };
            entry.entry_count += row.entry_count;
        }
    }

    Ok(merged.into_values().collect())
}

#[cfg(test)]
mod tests {
    use super::{
        build_drilldown_capabilities, build_metric_case_expr, common_dimensions_for_turnovers,
        compute_coverage_pct, display_formula, parse_signed_turnovers, pct_change,
        status_by_sign_policy, SignedTurnover, ViewMetric,
    };
    use contracts::shared::analytics::{IndicatorStatus, SignPolicy};
    use contracts::shared::data_view::ViewContext;
    use std::collections::HashMap;

    #[test]
    fn expense_growth_is_bad_and_decline_is_good() {
        assert_eq!(
            status_by_sign_policy(SignPolicy::ExpensePositive, Some(12.0)),
            IndicatorStatus::Bad
        );
        assert_eq!(
            status_by_sign_policy(SignPolicy::ExpensePositive, Some(-12.0)),
            IndicatorStatus::Good
        );
    }

    #[test]
    fn natural_sign_policy_is_always_neutral() {
        assert_eq!(
            status_by_sign_policy(SignPolicy::Natural, Some(25.0)),
            IndicatorStatus::Neutral
        );
    }

    #[test]
    fn pct_change_returns_none_for_zero_previous_value() {
        assert_eq!(pct_change(10.0, 0.0), None);
    }

    #[test]
    fn turnover_items_parser_supports_signed_tokens() {
        let parsed =
            parse_signed_turnovers("item_cost, +item_cost_storno, -mp_commission").unwrap();
        assert_eq!(
            parsed,
            vec![
                SignedTurnover {
                    code: "item_cost".to_string(),
                    sign: 1,
                },
                SignedTurnover {
                    code: "item_cost_storno".to_string(),
                    sign: 1,
                },
                SignedTurnover {
                    code: "mp_commission".to_string(),
                    sign: -1,
                },
            ]
        );
    }

    #[test]
    fn turnover_items_reject_duplicate_codes() {
        let err = parse_signed_turnovers("item_cost, -item_cost").unwrap_err();
        assert!(err.to_string().contains("Duplicate turnover_code"));
    }

    #[test]
    fn display_formula_formats_signs() {
        let formula = display_formula(&[
            SignedTurnover {
                code: "item_cost".to_string(),
                sign: 1,
            },
            SignedTurnover {
                code: "item_cost_storno".to_string(),
                sign: 1,
            },
            SignedTurnover {
                code: "mp_commission".to_string(),
                sign: -1,
            },
        ]);
        assert_eq!(formula, "item_cost + item_cost_storno - mp_commission");
    }

    #[test]
    fn common_dimensions_for_composite_turnovers_is_intersection() {
        let dims = common_dimensions_for_turnovers(
            &[
                SignedTurnover {
                    code: "item_cost".to_string(),
                    sign: 1,
                },
                SignedTurnover {
                    code: "mp_commission_adjustment".to_string(),
                    sign: 1,
                },
            ],
            "oper",
        );
        let ids = dims.iter().map(|item| item.id.as_str()).collect::<Vec<_>>();
        assert!(ids.contains(&"entry_date"));
        assert!(ids.contains(&"registrator_ref"));
        assert!(!ids.contains(&"nomenclature"));
    }

    #[test]
    fn entry_count_metric_expr_uses_signed_row_count() {
        assert_eq!(
            build_metric_case_expr(
                "t",
                &[SignedTurnover {
                    code: "customer_revenue_pl_storno".to_string(),
                    sign: 1,
                }],
                ViewMetric::EntryCount,
            ),
            "CASE WHEN t.turnover_code = ? THEN 1.0 ELSE 0 END"
        );
    }

    #[test]
    fn partial_coverage_uses_absolute_component_weights() {
        let turnovers = vec![
            SignedTurnover {
                code: "customer_revenue_pl".to_string(),
                sign: 1,
            },
            SignedTurnover {
                code: "mp_commission".to_string(),
                sign: -1,
            },
        ];
        let mut totals = HashMap::new();
        totals.insert("customer_revenue_pl".to_string(), 98.0);
        totals.insert("mp_commission".to_string(), -2.0);

        let pct = compute_coverage_pct(&turnovers, &["customer_revenue_pl".to_string()], &totals);
        assert_eq!(pct, Some(98.0));
    }

    #[test]
    fn zero_total_coverage_falls_back_to_supported_turnover_share() {
        let turnovers = vec![
            SignedTurnover {
                code: "a".to_string(),
                sign: 1,
            },
            SignedTurnover {
                code: "b".to_string(),
                sign: -1,
            },
        ];
        let mut totals = HashMap::new();
        totals.insert("a".to_string(), 0.0);
        totals.insert("b".to_string(), 0.0);

        let pct = compute_coverage_pct(&turnovers, &["a".to_string()], &totals);
        assert_eq!(pct, Some(50.0));
    }

    #[tokio::test]
    async fn single_turnover_capabilities_are_all_safe_without_partial_dimensions() {
        let ctx = ViewContext::default();
        let caps = build_drilldown_capabilities(
            ViewMetric::Amount,
            &[SignedTurnover {
                code: "mp_penalty".to_string(),
                sign: 1,
            }],
            "fact",
            &ctx,
        )
        .await
        .unwrap();

        assert!(!caps.safe_dimensions.is_empty());
        assert!(caps.partial_dimensions.is_empty());
        assert!(caps
            .safe_dimensions
            .iter()
            .all(|dimension| dimension.mode == "safe" && dimension.coverage_pct == Some(100.0)));
    }
}

pub fn meta() -> contracts::shared::data_view::DataViewMeta {
    const JSON: &str = include_str!("metadata.json");
    serde_json::from_str(JSON).expect("dv004/metadata.json parse error")
}
