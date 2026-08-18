//! dv007 - DataView: ratio of two GL turnover formulas, expressed in percent.
//!
//! Required params:
//!   numerator_turnover_code or numerator_turnover_items
//!   numerator_layer
//!   denominator_turnover_code or denominator_turnover_items
//!   denominator_layer
//! Optional params:
//!   metric = ratio_percent (default)

use anyhow::{anyhow, Result};
use contracts::shared::analytics::{IndicatorId, IndicatorStatus, IndicatorValue};
use contracts::shared::data_view::ViewContext;
use contracts::shared::drilldown::DrilldownResponse;
use sea_orm::{FromQueryResult, Statement};

use crate::shared::analytics::turnover_registry::get_turnover_class;
use crate::shared::data::db::get_connection;

const VIEW_ID: &str = "dv007_gl_turnover_ratio_percent";
const METRIC_ID: &str = "ratio_percent";

#[derive(Debug, Clone, PartialEq, Eq)]
struct SignedTurnover {
    code: String,
    sign: i8,
}

#[derive(Debug, FromQueryResult)]
struct AggRow {
    total: f64,
}

#[derive(Debug, FromQueryResult)]
struct DailyRow {
    total: f64,
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

fn resolve_metric(ctx: &ViewContext) -> Result<&str> {
    let metric = ctx
        .params
        .get("metric")
        .map(String::as_str)
        .unwrap_or(METRIC_ID);
    if metric == METRIC_ID {
        Ok(metric)
    } else {
        Err(anyhow!(
            "Unsupported metric '{}' for {}. Expected '{}'",
            metric,
            VIEW_ID,
            METRIC_ID
        ))
    }
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
                "Empty turnover code in turnover formula for {}",
                VIEW_ID
            ));
        }

        if !seen.insert(code.to_string()) {
            return Err(anyhow!(
                "Duplicate turnover_code '{}' in turnover formula for {}",
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
            "Turnover formula must contain at least one turnover_code for {}",
            VIEW_ID
        ));
    }

    Ok(result)
}

fn resolve_turnovers(ctx: &ViewContext, prefix: &str) -> Result<(Vec<SignedTurnover>, String)> {
    let layer = required_param(ctx, &format!("{prefix}_layer"))?.to_string();
    let items_key = format!("{prefix}_turnover_items");
    let code_key = format!("{prefix}_turnover_code");

    let turnovers = if let Some(raw) = ctx
        .params
        .get(&items_key)
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
    {
        parse_signed_turnovers(raw)?
    } else {
        vec![SignedTurnover {
            code: required_param(ctx, &code_key)?.to_string(),
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

fn build_signed_amount_case_expr(alias: &str, turnovers: &[SignedTurnover]) -> String {
    let when_clauses = turnovers
        .iter()
        .map(|turnover| {
            format!(
                "WHEN {alias}.turnover_code = ? THEN COALESCE({alias}.amount, 0) * {}.0",
                turnover.sign
            )
        })
        .collect::<Vec<_>>()
        .join(" ");
    format!("CASE {when_clauses} ELSE 0 END")
}

async fn fetch_aggregate(
    turnovers: &[SignedTurnover],
    layer: &str,
    date_from: &str,
    date_to: &str,
    connection_mp_refs: &[String],
) -> Result<f64> {
    let db = get_connection();
    let signed_amount_expr = build_signed_amount_case_expr("t", turnovers);
    let turnover_placeholders: Vec<&str> = turnovers.iter().map(|_| "?").collect();
    let mut sql = format!(
        r#"
        SELECT CAST(COALESCE(SUM({signed_amount_expr}), 0) AS REAL) AS total
        FROM sys_general_ledger t
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

async fn fetch_daily_rows(
    turnovers: &[SignedTurnover],
    layer: &str,
    date_from: &str,
    date_to: &str,
    connection_mp_refs: &[String],
) -> Result<Vec<DailyRow>> {
    let db = get_connection();
    let signed_amount_expr = build_signed_amount_case_expr("t", turnovers);
    let turnover_placeholders: Vec<&str> = turnovers.iter().map(|_| "?").collect();
    let mut sql = format!(
        r#"
        SELECT CAST(COALESCE(SUM({signed_amount_expr}), 0) AS REAL) AS total
        FROM sys_general_ledger t
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

fn ratio_percent(numerator: f64, denominator: f64) -> Option<f64> {
    if denominator.abs() < 0.000_001 {
        None
    } else {
        Some((numerator / denominator) * 100.0)
    }
}

fn pct_change(cur: Option<f64>, prev: Option<f64>) -> Option<f64> {
    match (cur, prev) {
        (Some(cur), Some(prev)) if prev.abs() >= 0.01 => Some(((cur - prev) / prev.abs()) * 100.0),
        _ => None,
    }
}

fn build_spark_points(numerator: &[f64], denominator: &[f64]) -> Vec<f64> {
    numerator
        .iter()
        .zip(denominator.iter())
        .filter_map(|(num, den)| ratio_percent(*num, *den))
        .collect()
}

fn formula_label(turnovers: &[SignedTurnover], layer: &str) -> String {
    format!("{} [{}]", display_formula(turnovers), layer)
}

fn format_value(value: f64) -> String {
    if (value.round() - value).abs() < 0.000_001 {
        format!("{}", value as i64)
    } else {
        format!("{value:.2}")
    }
}

pub async fn compute_scalar(ctx: &ViewContext) -> Result<IndicatorValue> {
    let _ = resolve_metric(ctx)?;
    let (numerator_turnovers, numerator_layer) = resolve_turnovers(ctx, "numerator")?;
    let (denominator_turnovers, denominator_layer) = resolve_turnovers(ctx, "denominator")?;
    let (p2_from, p2_to) = resolve_period2(ctx);

    let (
        numerator_current_result,
        numerator_daily_result,
        numerator_prev_result,
        denominator_current_result,
        denominator_daily_result,
        denominator_prev_result,
    ) = tokio::join!(
        fetch_aggregate(
            &numerator_turnovers,
            &numerator_layer,
            &ctx.date_from,
            &ctx.date_to,
            &ctx.connection_mp_refs
        ),
        fetch_daily_rows(
            &numerator_turnovers,
            &numerator_layer,
            &ctx.date_from,
            &ctx.date_to,
            &ctx.connection_mp_refs
        ),
        fetch_aggregate(
            &numerator_turnovers,
            &numerator_layer,
            &p2_from,
            &p2_to,
            &ctx.connection_mp_refs
        ),
        fetch_aggregate(
            &denominator_turnovers,
            &denominator_layer,
            &ctx.date_from,
            &ctx.date_to,
            &ctx.connection_mp_refs
        ),
        fetch_daily_rows(
            &denominator_turnovers,
            &denominator_layer,
            &ctx.date_from,
            &ctx.date_to,
            &ctx.connection_mp_refs
        ),
        fetch_aggregate(
            &denominator_turnovers,
            &denominator_layer,
            &p2_from,
            &p2_to,
            &ctx.connection_mp_refs
        ),
    );

    let numerator_current = numerator_current_result?;
    let numerator_daily = numerator_daily_result?;
    let numerator_previous = numerator_prev_result?;
    let denominator_current = denominator_current_result?;
    let denominator_daily = denominator_daily_result?;
    let denominator_previous = denominator_prev_result?;

    let current = ratio_percent(numerator_current, denominator_current);
    let previous = ratio_percent(numerator_previous, denominator_previous);
    let details = vec![
        format!(
            "Числитель: {} [{}] = {}",
            display_formula_verbose(&numerator_turnovers),
            numerator_layer,
            format_value(numerator_current)
        ),
        format!(
            "Знаменатель: {} [{}] = {}",
            display_formula_verbose(&denominator_turnovers),
            denominator_layer,
            format_value(denominator_current)
        ),
        "Формула: числитель / знаменатель * 100".to_string(),
    ];

    Ok(IndicatorValue {
        id: IndicatorId::new(VIEW_ID),
        value: current,
        previous_value: previous,
        change_percent: pct_change(current, previous),
        status: IndicatorStatus::Neutral,
        subtitle: Some(format!(
            "{} / {}",
            formula_label(&numerator_turnovers, &numerator_layer),
            formula_label(&denominator_turnovers, &denominator_layer)
        )),
        details,
        spark_points: build_spark_points(
            &numerator_daily
                .into_iter()
                .map(|row| row.total)
                .collect::<Vec<_>>(),
            &denominator_daily
                .into_iter()
                .map(|row| row.total)
                .collect::<Vec<_>>(),
        ),
    })
}

pub async fn compute_drilldown_multi(
    _ctx: &ViewContext,
    _group_by: &str,
    _metric_ids: &[String],
) -> Result<DrilldownResponse> {
    Err(anyhow!(
        "{} does not support drilldown. Use drilldown on source GL turnovers instead.",
        VIEW_ID
    ))
}

#[cfg(test)]
mod tests {
    use super::{parse_signed_turnovers, ratio_percent, SignedTurnover};

    #[test]
    fn ratio_percent_multiplies_by_hundred() {
        assert_eq!(ratio_percent(-20.0, 100.0), Some(-20.0));
    }

    #[test]
    fn signed_turnover_formula_supports_minus_prefix() {
        assert_eq!(
            parse_signed_turnovers("customer_revenue_pl_storno, -customer_revenue_pl").unwrap(),
            vec![
                SignedTurnover {
                    code: "customer_revenue_pl_storno".to_string(),
                    sign: 1,
                },
                SignedTurnover {
                    code: "customer_revenue_pl".to_string(),
                    sign: -1,
                },
            ]
        );
    }
}

pub fn meta() -> contracts::shared::data_view::DataViewMeta {
    const JSON: &str = include_str!("metadata.json");
    serde_json::from_str(JSON).expect("dv007/metadata.json parse error")
}
