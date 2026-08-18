//! dv005 - DataView: GL account view totals (2 periods)
//!
//! Scalar data source: sys_general_ledger filtered by account_view registry sections.
//! Required params:
//!   account = chart-of-accounts code (for example 7609)
//! Optional params:
//!   section = main | info | all (default main)
//!   metric = balance | debit | credit (default balance)

use anyhow::{anyhow, Result};
use contracts::shared::analytics::{IndicatorId, IndicatorStatus, IndicatorValue};
use contracts::shared::data_view::ViewContext;
use contracts::shared::drilldown::{DrilldownResponse, DrilldownRow, MetricColumnDef};
use sea_orm::{FromQueryResult, Statement, Value};
use std::collections::HashMap;

use crate::general_ledger::account_view::registry::{find_view, GlAccountViewEntry};
use crate::general_ledger::get_turnover_class;
use crate::shared::data::db::get_connection;

const VIEW_ID: &str = "dv005_gl_account_view_total";
const DEFAULT_METRIC: &str = "balance";
const DEFAULT_SECTION: &str = "main";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ViewSection {
    Main,
    Info,
    All,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ViewMetric {
    Balance,
    Debit,
    Credit,
}

#[derive(Debug, FromQueryResult)]
struct AggRow {
    total: f64,
}

#[derive(Debug, FromQueryResult)]
struct GroupRow {
    group_key: String,
    total: f64,
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

fn required_param<'a>(ctx: &'a ViewContext, key: &str) -> Result<&'a str> {
    ctx.params
        .get(key)
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("Missing required param '{}' for {}", key, VIEW_ID))
}

fn resolve_section(ctx: &ViewContext) -> Result<ViewSection> {
    match ctx
        .params
        .get("section")
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .unwrap_or(DEFAULT_SECTION)
    {
        "main" => Ok(ViewSection::Main),
        "info" => Ok(ViewSection::Info),
        "all" => Ok(ViewSection::All),
        other => Err(anyhow!(
            "Unsupported section '{}' for {}. Expected main | info | all",
            other,
            VIEW_ID
        )),
    }
}

fn resolve_metric(ctx: &ViewContext) -> Result<ViewMetric> {
    match ctx
        .params
        .get("metric")
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .unwrap_or(DEFAULT_METRIC)
    {
        "balance" => Ok(ViewMetric::Balance),
        "debit" => Ok(ViewMetric::Debit),
        "credit" => Ok(ViewMetric::Credit),
        other => Err(anyhow!(
            "Unsupported metric '{}' for {}. Expected balance | debit | credit",
            other,
            VIEW_ID
        )),
    }
}

fn metric_label(metric: ViewMetric) -> &'static str {
    match metric {
        ViewMetric::Balance => "Сальдо",
        ViewMetric::Debit => "Оборот Дт",
        ViewMetric::Credit => "Оборот Кт",
    }
}

fn section_label(section: ViewSection) -> &'static str {
    match section {
        ViewSection::Main => "main",
        ViewSection::Info => "info",
        ViewSection::All => "all",
    }
}

fn section_title(section: ViewSection) -> &'static str {
    match section {
        ViewSection::Main => "Основные обороты",
        ViewSection::Info => "Дополнительно",
        ViewSection::All => "Все обороты",
    }
}

fn build_metric_expr(alias: &str, metric: ViewMetric) -> String {
    match metric {
        ViewMetric::Balance => format!(
            "CASE WHEN {alias}.debit_account = ? THEN COALESCE({alias}.amount, 0) WHEN {alias}.credit_account = ? THEN -COALESCE({alias}.amount, 0) ELSE 0.0 END"
        ),
        ViewMetric::Debit => {
            format!(
                "CASE WHEN {alias}.debit_account = ? THEN COALESCE({alias}.amount, 0) ELSE 0.0 END"
            )
        }
        ViewMetric::Credit => {
            format!(
                "CASE WHEN {alias}.credit_account = ? THEN COALESCE({alias}.amount, 0) ELSE 0.0 END"
            )
        }
    }
}

fn append_metric_params(params: &mut Vec<Value>, account: &str, metric: ViewMetric) {
    match metric {
        ViewMetric::Balance => {
            params.push(account.to_string().into());
            params.push(account.to_string().into());
        }
        ViewMetric::Debit | ViewMetric::Credit => {
            params.push(account.to_string().into());
        }
    }
}

fn append_connection_filter(sql: &mut String, params: &mut Vec<Value>, refs: &[String]) {
    if refs.is_empty() {
        return;
    }

    let placeholders: Vec<&str> = refs.iter().map(|_| "?").collect();
    sql.push_str(&format!(
        " AND gl.connection_mp_ref IN ({})",
        placeholders.join(", ")
    ));
    for value in refs {
        params.push(value.clone().into());
    }
}

fn append_entries_predicate(
    sql: &mut String,
    params: &mut Vec<Value>,
    alias: &str,
    entries: &[GlAccountViewEntry],
) {
    sql.push('(');
    for (index, entry) in entries.iter().enumerate() {
        if index > 0 {
            sql.push_str(" OR ");
        }
        if entry.layer.is_empty() {
            sql.push_str(&format!("{alias}.turnover_code = ?"));
            params.push(entry.turnover_code.to_string().into());
        } else {
            sql.push_str(&format!(
                "({alias}.turnover_code = ? AND COALESCE({alias}.layer, '') = ?)"
            ));
            params.push(entry.turnover_code.to_string().into());
            params.push(entry.layer.to_string().into());
        }
    }
    sql.push(')');
}

fn append_section_filter(
    sql: &mut String,
    params: &mut Vec<Value>,
    account: &str,
    section: ViewSection,
) -> Result<()> {
    let Some(view_def) = find_view(account) else {
        if section == ViewSection::All {
            return Ok(());
        }
        return Err(anyhow!(
            "No gl_account_view configuration registered for account '{}'",
            account
        ));
    };

    match section {
        ViewSection::All => Ok(()),
        ViewSection::Main => {
            sql.push_str(" AND ");
            append_entries_predicate(sql, params, "gl", view_def.main_entries);
            Ok(())
        }
        ViewSection::Info => {
            sql.push_str(" AND NOT ");
            append_entries_predicate(sql, params, "gl", view_def.main_entries);
            Ok(())
        }
    }
}

fn build_group_key_expr(group_by: &str) -> Result<&'static str> {
    match group_by {
        "entry_date" => Ok("DATE(gl.entry_date)"),
        "connection_mp_ref" => Ok("COALESCE(gl.connection_mp_ref, '')"),
        "turnover_code" => Ok("COALESCE(gl.turnover_code, '')"),
        "layer" => Ok("COALESCE(gl.layer, '')"),
        "registrator_type" => Ok("COALESCE(gl.registrator_type, '')"),
        "registrator_ref" => Ok("COALESCE(gl.registrator_ref, '')"),
        "corr_account" => Ok(
            "CASE WHEN gl.debit_account = ? THEN COALESCE(gl.credit_account, '') ELSE COALESCE(gl.debit_account, '') END",
        ),
        other => Err(anyhow!(
            "Unsupported group_by '{}' for {}",
            other,
            VIEW_ID
        )),
    }
}

fn group_by_label(group_by: &str) -> String {
    match group_by {
        "entry_date" => "По дням".to_string(),
        "connection_mp_ref" => "По кабинету МП".to_string(),
        "turnover_code" => "По обороту".to_string(),
        "layer" => "По слою".to_string(),
        "corr_account" => "По корр. счету".to_string(),
        "registrator_type" => "По типу документа".to_string(),
        "registrator_ref" => "По документу".to_string(),
        _ => group_by.to_string(),
    }
}

fn row_label(group_by: &str, key: &str) -> String {
    match group_by {
        "entry_date" => fmt_day_label(key),
        "turnover_code" => get_turnover_class(key)
            .map(|item| item.name.to_string())
            .unwrap_or_else(|| key.to_string()),
        _ if key.is_empty() => "—".to_string(),
        _ => key.to_string(),
    }
}

async fn fetch_total(
    account: &str,
    metric: ViewMetric,
    section: ViewSection,
    date_from: &str,
    date_to: &str,
    connection_mp_refs: &[String],
) -> Result<f64> {
    let db = get_connection();
    let metric_expr = build_metric_expr("gl", metric);
    let mut sql = format!(
        r#"
        SELECT CAST(COALESCE(SUM({metric_expr}), 0) AS REAL) AS total
        FROM sys_general_ledger gl
        WHERE (gl.debit_account = ? OR gl.credit_account = ?)
          AND gl.entry_date >= ?
          AND gl.entry_date <= ?
        "#
    );
    let mut params = Vec::new();
    append_metric_params(&mut params, account, metric);
    params.push(account.to_string().into());
    params.push(account.to_string().into());
    params.push(date_from.to_string().into());
    params.push(date_to.to_string().into());
    append_connection_filter(&mut sql, &mut params, connection_mp_refs);
    append_section_filter(&mut sql, &mut params, account, section)?;

    let stmt = Statement::from_sql_and_values(sea_orm::DatabaseBackend::Sqlite, &sql, params);
    let row = AggRow::find_by_statement(stmt)
        .one(db)
        .await?
        .unwrap_or(AggRow { total: 0.0 });
    Ok(row.total)
}

async fn fetch_grouped_rows(
    account: &str,
    metric: ViewMetric,
    section: ViewSection,
    date_from: &str,
    date_to: &str,
    connection_mp_refs: &[String],
    group_by: &str,
) -> Result<Vec<GroupRow>> {
    let db = get_connection();
    let group_key_expr = build_group_key_expr(group_by)?;
    let metric_expr = build_metric_expr("gl", metric);
    let mut sql = format!(
        r#"
        SELECT
            {group_key_expr} AS group_key,
            CAST(COALESCE(SUM({metric_expr}), 0) AS REAL) AS total
        FROM sys_general_ledger gl
        WHERE (gl.debit_account = ? OR gl.credit_account = ?)
          AND gl.entry_date >= ?
          AND gl.entry_date <= ?
        "#
    );
    let mut params = Vec::new();
    if group_by == "corr_account" {
        params.push(account.to_string().into());
    }
    append_metric_params(&mut params, account, metric);
    params.push(account.to_string().into());
    params.push(account.to_string().into());
    params.push(date_from.to_string().into());
    params.push(date_to.to_string().into());
    append_connection_filter(&mut sql, &mut params, connection_mp_refs);
    append_section_filter(&mut sql, &mut params, account, section)?;
    sql.push_str(" GROUP BY group_key ORDER BY group_key ASC");

    let stmt = Statement::from_sql_and_values(sea_orm::DatabaseBackend::Sqlite, &sql, params);
    Ok(GroupRow::find_by_statement(stmt).all(db).await?)
}

pub async fn compute_scalar(ctx: &ViewContext) -> Result<IndicatorValue> {
    let account = required_param(ctx, "account")?.to_string();
    let metric = resolve_metric(ctx)?;
    let section = resolve_section(ctx)?;
    let (p2_from, p2_to) = resolve_period2(ctx);

    let (current_result, daily_result, prev_result) = tokio::join!(
        fetch_total(
            &account,
            metric,
            section,
            &ctx.date_from,
            &ctx.date_to,
            &ctx.connection_mp_refs
        ),
        fetch_grouped_rows(
            &account,
            metric,
            section,
            &ctx.date_from,
            &ctx.date_to,
            &ctx.connection_mp_refs,
            "entry_date"
        ),
        fetch_total(
            &account,
            metric,
            section,
            &p2_from,
            &p2_to,
            &ctx.connection_mp_refs
        ),
    );

    let current = current_result?;
    let daily = daily_result?;
    let previous = prev_result?;
    let change = pct_change(current, previous);
    let details = vec![
        format!("Счёт: {}", account),
        format!("Раздел: {}", section_title(section)),
        format!("Метрика: {}", metric_label(metric)),
    ];

    Ok(IndicatorValue {
        id: IndicatorId::new(VIEW_ID),
        value: Some(current),
        previous_value: Some(previous),
        change_percent: change,
        status: IndicatorStatus::Neutral,
        subtitle: Some(format!(
            "gl_account_view__{} [{} / {}]",
            account,
            section_label(section),
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

pub async fn compute_drilldown(ctx: &ViewContext, group_by: &str) -> Result<DrilldownResponse> {
    let account = required_param(ctx, "account")?.to_string();
    let metric = resolve_metric(ctx)?;
    let section = resolve_section(ctx)?;
    let (p2_from, p2_to) = resolve_period2(ctx);

    let (rows1, rows2) = tokio::join!(
        fetch_grouped_rows(
            &account,
            metric,
            section,
            &ctx.date_from,
            &ctx.date_to,
            &ctx.connection_mp_refs,
            group_by
        ),
        fetch_grouped_rows(
            &account,
            metric,
            section,
            &p2_from,
            &p2_to,
            &ctx.connection_mp_refs,
            group_by
        ),
    );

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

    for row in rows1? {
        let key = day_key(&row.group_key);
        let label = row_label(group_by, &row.group_key);
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
            .value1 += row.total;
    }

    for row in rows2? {
        let key = day_key(&row.group_key);
        let label = row_label(group_by, &row.group_key);
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
            .value2 += row.total;
    }

    let mut rows: Vec<DrilldownRow> = merged
        .into_values()
        .map(|mut row| {
            row.delta_pct = pct_change(row.value1, row.value2);
            row
        })
        .collect();

    if group_by == "entry_date" {
        rows.sort_by(|a, b| a.group_key.cmp(&b.group_key));
    } else {
        rows.sort_by(|a, b| {
            b.value1
                .partial_cmp(&a.value1)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
    }

    Ok(DrilldownResponse {
        rows,
        group_by_label: group_by_label(group_by),
        period1_label: period_label(&ctx.date_from, &ctx.date_to),
        period2_label: period_label(&p2_from, &p2_to),
        metric_label: metric_label(metric).to_string(),
        metric_columns: vec![],
        selected_dimension: None,
        coverage: None,
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

    if let Some(metric_id) = metric_ids.first() {
        let _ = match metric_id.as_str() {
            "balance" => Ok(ViewMetric::Balance),
            "debit" => Ok(ViewMetric::Debit),
            "credit" => Ok(ViewMetric::Credit),
            other => Err(anyhow!("Unsupported metric '{}' for {}", other, VIEW_ID)),
        }?;

        let mut next_ctx = ctx.clone();
        next_ctx
            .params
            .insert("metric".to_string(), metric_id.to_string());
        let mut response = compute_drilldown(&next_ctx, group_by).await?;
        let label = response.metric_label.clone();
        response.metric_label.clear();
        response.metric_columns = vec![MetricColumnDef {
            id: metric_id.to_string(),
            label,
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
    } else {
        compute_drilldown(ctx, group_by).await
    }
}

#[cfg(test)]
mod tests {
    use super::{append_entries_predicate, build_group_key_expr, build_metric_expr, ViewMetric};

    #[test]
    fn balance_metric_expr_is_signed_by_account_side() {
        assert_eq!(
            build_metric_expr("gl", ViewMetric::Balance),
            "CASE WHEN gl.debit_account = ? THEN COALESCE(gl.amount, 0) WHEN gl.credit_account = ? THEN -COALESCE(gl.amount, 0) ELSE 0.0 END"
        );
    }

    #[test]
    fn corr_account_grouping_uses_same_case_as_account_view() {
        assert_eq!(
            build_group_key_expr("corr_account").unwrap(),
            "CASE WHEN gl.debit_account = ? THEN COALESCE(gl.credit_account, '') ELSE COALESCE(gl.debit_account, '') END"
        );
    }

    #[test]
    fn entry_predicate_keeps_turnover_and_layer_pairs() {
        let mut sql = String::new();
        let mut params = Vec::new();
        append_entries_predicate(
            &mut sql,
            &mut params,
            "gl",
            crate::general_ledger::account_view::registry::ACCOUNT_7609_VIEW.main_entries,
        );

        assert!(sql.contains("gl.turnover_code = ?"));
        assert!(sql.contains("COALESCE(gl.layer, '') = ?"));
        assert!(!params.is_empty());
    }
}

pub fn meta() -> contracts::shared::data_view::DataViewMeta {
    const JSON: &str = include_str!("metadata.json");
    serde_json::from_str(JSON).expect("dv005/metadata.json parse error")
}
