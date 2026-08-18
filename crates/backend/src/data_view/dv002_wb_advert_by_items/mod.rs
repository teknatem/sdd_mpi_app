//! dv002 - DataView: WB рекламные расходы по номенклатуре (2 периода)
//!
//! Источник данных: p911_wb_advert_by_items
//! Поддерживаемая метрика:
//!   advertising_expenses = SUM(amount)

use anyhow::Result;
use contracts::shared::analytics::{IndicatorId, IndicatorStatus, IndicatorValue};
use contracts::shared::data_view::ViewContext;
use contracts::shared::drilldown::{DrilldownResponse, DrilldownRow, MetricColumnDef};
use sea_orm::{ConnectionTrait, FromQueryResult, Statement};

use crate::shared::data::db::get_connection;

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

fn status_by_change(change: Option<f64>) -> IndicatorStatus {
    match change {
        Some(c) if c < -5.0 => IndicatorStatus::Good,
        Some(c) if c > 5.0 => IndicatorStatus::Bad,
        _ => IndicatorStatus::Neutral,
    }
}

#[derive(Clone, Copy)]
struct MetricDef {
    label: &'static str,
}

impl MetricDef {
    fn aggregate_sql(self, alias: &str) -> String {
        format!("CAST(COALESCE(SUM(COALESCE({alias}.amount, 0)), 0) AS REAL)")
    }
}

fn resolve_metric(ctx: &ViewContext) -> MetricDef {
    resolve_metric_by_id(
        ctx.params
            .get("metric")
            .map(|s| s.as_str())
            .unwrap_or("advertising_expenses"),
    )
}

fn resolve_metric_by_id(_id: &str) -> MetricDef {
    MetricDef {
        label: "Рекламные расходы",
    }
}

#[derive(Debug, FromQueryResult)]
struct AggRow {
    total: f64,
}

#[derive(Debug, FromQueryResult)]
struct DailyRow {
    #[allow(dead_code)]
    day_offset: String,
    #[allow(dead_code)]
    day_label: String,
    total: f64,
}

fn sort_daily_rows(mut rows: Vec<DailyRow>) -> Vec<DailyRow> {
    rows.sort_by(|a, b| {
        a.day_offset
            .cmp(&b.day_offset)
            .then_with(|| a.day_label.cmp(&b.day_label))
    });
    rows
}

#[derive(Debug, FromQueryResult)]
struct DrilldownAggRow {
    group_key: String,
    label: String,
    total: f64,
}

async fn fetch_aggregate(
    metric: MetricDef,
    date_from: &str,
    date_to: &str,
    connection_mp_refs: &[String],
) -> Result<f64> {
    let db = get_connection();

    let mut sql = format!(
        r#"
        SELECT {metric_expr} AS total
        FROM p911_wb_advert_by_items p
        WHERE substr(p.entry_date, 1, 10) >= ? AND substr(p.entry_date, 1, 10) <= ?
        "#,
        metric_expr = metric.aggregate_sql("p")
    );

    let mut params: Vec<sea_orm::Value> =
        vec![date_from.to_string().into(), date_to.to_string().into()];

    if !connection_mp_refs.is_empty() {
        let placeholders: Vec<&str> = connection_mp_refs.iter().map(|_| "?").collect();
        sql.push_str(&format!(
            " AND p.connection_mp_ref IN ({})",
            placeholders.join(", ")
        ));
        for r in connection_mp_refs {
            params.push(r.clone().into());
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
    metric: MetricDef,
    date_from: &str,
    date_to: &str,
    connection_mp_refs: &[String],
) -> Result<Vec<DailyRow>> {
    let db = get_connection();

    let mut sql = format!(
        r#"
        SELECT
            printf('%06d', CAST(julianday(DATE(t.entry_date)) - julianday(?) AS INTEGER)) AS day_offset,
            DATE(t.entry_date) AS day_label,
            {metric_expr} AS total
        FROM p911_wb_advert_by_items t
        WHERE substr(t.entry_date, 1, 10) >= ? AND substr(t.entry_date, 1, 10) <= ?
        "#,
        metric_expr = metric.aggregate_sql("t")
    );

    let mut params: Vec<sea_orm::Value> = vec![
        date_from.to_string().into(),
        date_from.to_string().into(),
        date_to.to_string().into(),
    ];

    if !connection_mp_refs.is_empty() {
        let placeholders: Vec<&str> = connection_mp_refs.iter().map(|_| "?").collect();
        sql.push_str(&format!(
            " AND t.connection_mp_ref IN ({})",
            placeholders.join(", ")
        ));
        for r in connection_mp_refs {
            params.push(r.clone().into());
        }
    }

    sql.push_str(" GROUP BY DATE(t.entry_date) ORDER BY day_offset ASC");

    let stmt = Statement::from_sql_and_values(sea_orm::DatabaseBackend::Sqlite, &sql, params);
    Ok(DailyRow::find_by_statement(stmt).all(db).await?)
}

async fn fetch_drilldown_period(
    group_col: &str,
    metric: MetricDef,
    ref_table: Option<&str>,
    ref_display_col: Option<&str>,
    source_table: Option<&str>,
    join_on_col: Option<&str>,
    date_from: &str,
    date_to: &str,
    connection_mp_refs: &[String],
) -> Result<Vec<DrilldownAggRow>> {
    let db = get_connection();

    if group_col == "entry_date" {
        let mut sql = format!(
            r#"
            SELECT
                printf('%06d', CAST(julianday(DATE(t.entry_date)) - julianday(?) AS INTEGER)) AS group_key,
                DATE(t.entry_date) AS label,
                {metric_expr} AS total
            FROM p911_wb_advert_by_items t
            WHERE substr(t.entry_date, 1, 10) >= ? AND substr(t.entry_date, 1, 10) <= ?
            "#,
            metric_expr = metric.aggregate_sql("t")
        );

        let mut params: Vec<sea_orm::Value> = vec![
            date_from.to_string().into(),
            date_from.to_string().into(),
            date_to.to_string().into(),
        ];

        if !connection_mp_refs.is_empty() {
            let placeholders: Vec<&str> = connection_mp_refs.iter().map(|_| "?").collect();
            sql.push_str(&format!(
                " AND t.connection_mp_ref IN ({})",
                placeholders.join(", ")
            ));
            for r in connection_mp_refs {
                params.push(r.clone().into());
            }
        }

        sql.push_str(" GROUP BY DATE(t.entry_date) ORDER BY group_key ASC");

        let stmt = Statement::from_sql_and_values(sea_orm::DatabaseBackend::Sqlite, &sql, params);
        return Ok(DrilldownAggRow::find_by_statement(stmt).all(db).await?);
    }

    if let (Some(src_tbl), Some(join_col)) = (source_table, join_on_col) {
        let alias = "dim_t";
        let mut sql = format!(
            r#"
            SELECT
                COALESCE({alias}.{group_col}, '') AS group_key,
                COALESCE({alias}.{group_col}, '') AS label,
                {metric_expr} AS total
            FROM p911_wb_advert_by_items t
            LEFT JOIN {src_tbl} {alias} ON t.{join_col} = {alias}.id
            WHERE substr(t.entry_date, 1, 10) >= ? AND substr(t.entry_date, 1, 10) <= ?
            "#,
            metric_expr = metric.aggregate_sql("t")
        );

        let mut params: Vec<sea_orm::Value> =
            vec![date_from.to_string().into(), date_to.to_string().into()];

        if !connection_mp_refs.is_empty() {
            let placeholders: Vec<&str> = connection_mp_refs.iter().map(|_| "?").collect();
            sql.push_str(&format!(
                " AND t.connection_mp_ref IN ({})",
                placeholders.join(", ")
            ));
            for r in connection_mp_refs {
                params.push(r.clone().into());
            }
        }

        sql.push_str(&format!(
            " GROUP BY {alias}.{group_col} ORDER BY total DESC"
        ));

        let stmt = Statement::from_sql_and_values(sea_orm::DatabaseBackend::Sqlite, &sql, params);
        return Ok(DrilldownAggRow::find_by_statement(stmt).all(db).await?);
    }

    let (select_label, join_clause) = if let (Some(rt), Some(rdc)) = (ref_table, ref_display_col) {
        let alias = "ref_t";
        (
            format!("COALESCE({alias}.{rdc}, t.{group_col})"),
            format!("LEFT JOIN {rt} {alias} ON t.{group_col} = {alias}.id"),
        )
    } else {
        (format!("t.{group_col}"), String::new())
    };

    let mut sql = format!(
        r#"
        SELECT
            COALESCE(t.{group_col}, '') AS group_key,
            COALESCE({select_label}, '') AS label,
            {metric_expr} AS total
        FROM p911_wb_advert_by_items t
        {join_clause}
        WHERE substr(t.entry_date, 1, 10) >= ? AND substr(t.entry_date, 1, 10) <= ?
        "#,
        metric_expr = metric.aggregate_sql("t")
    );

    let mut params: Vec<sea_orm::Value> =
        vec![date_from.to_string().into(), date_to.to_string().into()];

    if !connection_mp_refs.is_empty() {
        let placeholders: Vec<&str> = connection_mp_refs.iter().map(|_| "?").collect();
        sql.push_str(&format!(
            " AND t.connection_mp_ref IN ({})",
            placeholders.join(", ")
        ));
        for r in connection_mp_refs {
            params.push(r.clone().into());
        }
    }

    sql.push_str(&format!(" GROUP BY t.{group_col} ORDER BY total DESC"));

    let stmt = Statement::from_sql_and_values(sea_orm::DatabaseBackend::Sqlite, &sql, params);
    Ok(DrilldownAggRow::find_by_statement(stmt).all(db).await?)
}

async fn fetch_drilldown_multi_period(
    group_col: &str,
    metrics: &[(&str, MetricDef)],
    ref_table: Option<&str>,
    ref_display_col: Option<&str>,
    source_table: Option<&str>,
    join_on_col: Option<&str>,
    date_from: &str,
    date_to: &str,
    connection_mp_refs: &[String],
) -> Result<Vec<(String, String, Vec<f64>)>> {
    let db = get_connection();

    let metric_cols: String = metrics
        .iter()
        .enumerate()
        .map(|(i, (_, metric))| format!("{} AS m{}", metric.aggregate_sql("t"), i))
        .collect::<Vec<_>>()
        .join(", ");

    let sql;
    let mut params: Vec<sea_orm::Value>;

    if group_col == "entry_date" {
        sql = format!(
            r#"
            SELECT
                printf('%06d', CAST(julianday(DATE(t.entry_date)) - julianday(?) AS INTEGER)) AS group_key,
                DATE(t.entry_date) AS label,
                {metric_cols}
            FROM p911_wb_advert_by_items t
            WHERE substr(t.entry_date, 1, 10) >= ? AND substr(t.entry_date, 1, 10) <= ?
            "#
        );
        params = vec![
            date_from.to_string().into(),
            date_from.to_string().into(),
            date_to.to_string().into(),
        ];
    } else if let (Some(src_tbl), Some(join_col)) = (source_table, join_on_col) {
        let alias = "dim_t";
        sql = format!(
            r#"
            SELECT
                COALESCE({alias}.{group_col}, '') AS group_key,
                COALESCE({alias}.{group_col}, '') AS label,
                {metric_cols}
            FROM p911_wb_advert_by_items t
            LEFT JOIN {src_tbl} {alias} ON t.{join_col} = {alias}.id
            WHERE substr(t.entry_date, 1, 10) >= ? AND substr(t.entry_date, 1, 10) <= ?
            "#
        );
        params = vec![date_from.to_string().into(), date_to.to_string().into()];
    } else {
        let (select_label, join_clause) =
            if let (Some(rt), Some(rdc)) = (ref_table, ref_display_col) {
                let alias = "ref_t";
                (
                    format!("COALESCE({alias}.{rdc}, t.{group_col})"),
                    format!("LEFT JOIN {rt} {alias} ON t.{group_col} = {alias}.id"),
                )
            } else {
                (format!("t.{group_col}"), String::new())
            };
        sql = format!(
            r#"
            SELECT
                COALESCE(t.{group_col}, '') AS group_key,
                COALESCE({select_label}, '') AS label,
                {metric_cols}
            FROM p911_wb_advert_by_items t
            {join_clause}
            WHERE substr(t.entry_date, 1, 10) >= ? AND substr(t.entry_date, 1, 10) <= ?
            "#,
        );
        params = vec![date_from.to_string().into(), date_to.to_string().into()];
    }

    if !connection_mp_refs.is_empty() {
        let placeholders: Vec<&str> = connection_mp_refs.iter().map(|_| "?").collect();
        let mut final_sql = sql.trim_end().to_string();
        final_sql.push_str(&format!(
            " AND t.connection_mp_ref IN ({})",
            placeholders.join(", ")
        ));
        for r in connection_mp_refs {
            params.push(r.clone().into());
        }
        let group_clause = if group_col == "entry_date" {
            " GROUP BY DATE(t.entry_date) ORDER BY group_key ASC".to_string()
        } else if source_table.is_some() {
            format!(" GROUP BY dim_t.{group_col} ORDER BY m0 DESC")
        } else {
            format!(" GROUP BY t.{group_col} ORDER BY m0 DESC")
        };
        let final_sql = format!("{}{}", final_sql, group_clause);
        let stmt =
            Statement::from_sql_and_values(sea_orm::DatabaseBackend::Sqlite, &final_sql, params);
        let rows = db.query_all(stmt).await?;
        return rows
            .into_iter()
            .map(|row| {
                let group_key: String = row.try_get("", "group_key")?;
                let label: String = row.try_get("", "label")?;
                let values: Vec<f64> = (0..metrics.len())
                    .map(|i| row.try_get::<f64>("", &format!("m{}", i)).unwrap_or(0.0))
                    .collect();
                Ok((group_key, label, values))
            })
            .collect::<Result<Vec<_>, sea_orm::DbErr>>()
            .map_err(anyhow::Error::from);
    }

    let group_clause = if group_col == "entry_date" {
        " GROUP BY DATE(t.entry_date) ORDER BY group_key ASC".to_string()
    } else if source_table.is_some() {
        format!(" GROUP BY dim_t.{group_col} ORDER BY m0 DESC")
    } else {
        format!(" GROUP BY t.{group_col} ORDER BY m0 DESC")
    };
    let final_sql = format!("{}{}", sql.trim_end(), group_clause);
    let stmt = Statement::from_sql_and_values(sea_orm::DatabaseBackend::Sqlite, &final_sql, params);
    let rows = db.query_all(stmt).await?;
    rows.into_iter()
        .map(|row| {
            let group_key: String = row.try_get("", "group_key")?;
            let label: String = row.try_get("", "label")?;
            let values: Vec<f64> = (0..metrics.len())
                .map(|i| row.try_get::<f64>("", &format!("m{}", i)).unwrap_or(0.0))
                .collect();
            Ok((group_key, label, values))
        })
        .collect::<Result<Vec<_>, sea_orm::DbErr>>()
        .map_err(anyhow::Error::from)
}

pub async fn compute_scalar(ctx: &ViewContext) -> Result<IndicatorValue> {
    let metric = resolve_metric(ctx);
    let (p2_from, p2_to) = resolve_period2(ctx);

    let (current_result, daily_result, prev_result) = tokio::join!(
        fetch_aggregate(
            metric,
            &ctx.date_from,
            &ctx.date_to,
            &ctx.connection_mp_refs
        ),
        fetch_daily_rows(
            metric,
            &ctx.date_from,
            &ctx.date_to,
            &ctx.connection_mp_refs
        ),
        fetch_aggregate(metric, &p2_from, &p2_to, &ctx.connection_mp_refs),
    );

    let cur = current_result?;
    let daily = sort_daily_rows(daily_result?);
    let prev = prev_result?;

    let spark_points: Vec<f64> = daily.iter().map(|r| r.total).collect();
    let change = pct_change(cur, prev);

    Ok(IndicatorValue {
        id: IndicatorId::new("dv002_wb_advert_by_items"),
        value: Some(cur),
        previous_value: Some(prev),
        change_percent: change,
        status: status_by_change(change),
        subtitle: None,
        details: vec![],
        spark_points,
    })
}

pub async fn compute_drilldown(ctx: &ViewContext, group_by: &str) -> Result<DrilldownResponse> {
    let dim = meta()
        .available_dimensions
        .into_iter()
        .find(|d| d.id == group_by)
        .ok_or_else(|| anyhow::anyhow!("Unknown group_by field: {}", group_by))?;

    let metric = resolve_metric(ctx);
    let group_by_label = dim.label.clone();
    let db_col = dim.db_column.as_deref().unwrap_or(group_by).to_string();

    let (p2_from, p2_to) = resolve_period2(ctx);
    let period1_label = period_label(&ctx.date_from, &ctx.date_to);
    let period2_label = period_label(&p2_from, &p2_to);

    let (rows1_result, rows2_result) = tokio::join!(
        fetch_drilldown_period(
            &db_col,
            metric,
            dim.ref_table.as_deref(),
            dim.ref_display_column.as_deref(),
            dim.source_table.as_deref(),
            dim.join_on_column.as_deref(),
            &ctx.date_from,
            &ctx.date_to,
            &ctx.connection_mp_refs,
        ),
        fetch_drilldown_period(
            &db_col,
            metric,
            dim.ref_table.as_deref(),
            dim.ref_display_column.as_deref(),
            dim.source_table.as_deref(),
            dim.join_on_column.as_deref(),
            &p2_from,
            &p2_to,
            &ctx.connection_mp_refs,
        ),
    );

    let rows1 = rows1_result?;
    let rows2 = rows2_result?;

    let is_date_group = group_by == "date";
    // По дням сливаем строки по дню месяца, чтобы один и тот же день из П1 и П2
    // совпадал (01 ↔ 01), даже когда периоды лежат в разных месяцах.
    let day_key = |raw: &str| {
        if is_date_group {
            fmt_day_label(raw)
        } else {
            raw.to_string()
        }
    };

    let mut merged: std::collections::HashMap<String, DrilldownRow> =
        std::collections::HashMap::new();

    for r in rows1 {
        let key = day_key(&r.group_key);
        let label = day_key(&r.label);
        merged
            .entry(key.clone())
            .or_insert_with(|| DrilldownRow {
                group_key: key,
                label,
                value1: 0.0,
                value2: 0.0,
                delta_pct: None,
                metric_values: std::collections::HashMap::new(),
            })
            .value1 += r.total;
    }

    for r in rows2 {
        let key = day_key(&r.group_key);
        let label = day_key(&r.label);
        merged
            .entry(key.clone())
            .or_insert_with(|| DrilldownRow {
                group_key: key,
                label,
                value1: 0.0,
                value2: 0.0,
                delta_pct: None,
                metric_values: std::collections::HashMap::new(),
            })
            .value2 += r.total;
    }

    let mut rows: Vec<DrilldownRow> = merged
        .into_values()
        .map(|mut r| {
            r.delta_pct = pct_change(r.value1, r.value2);
            r
        })
        .collect();

    if is_date_group {
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
        group_by_label,
        period1_label,
        period2_label,
        metric_label: metric.label.to_string(),
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
    if metric_ids.is_empty() {
        return compute_drilldown(ctx, group_by).await;
    }

    let dim = meta()
        .available_dimensions
        .into_iter()
        .find(|d| d.id == group_by)
        .ok_or_else(|| anyhow::anyhow!("Unknown group_by field: {}", group_by))?;

    let metric_resolved: Vec<(String, MetricDef)> = metric_ids
        .iter()
        .map(|id| (id.clone(), resolve_metric_by_id(id)))
        .collect();

    let metrics: Vec<(&str, MetricDef)> = metric_resolved
        .iter()
        .map(|(id, metric)| (id.as_str(), *metric))
        .collect();

    let group_by_label = dim.label.clone();
    let db_col = dim.db_column.as_deref().unwrap_or(group_by).to_string();
    let (p2_from, p2_to) = resolve_period2(ctx);
    let period1_label = period_label(&ctx.date_from, &ctx.date_to);
    let period2_label = period_label(&p2_from, &p2_to);

    let is_date_group = group_by == "date";

    let (rows1_result, rows2_result) = tokio::join!(
        fetch_drilldown_multi_period(
            &db_col,
            &metrics,
            dim.ref_table.as_deref(),
            dim.ref_display_column.as_deref(),
            dim.source_table.as_deref(),
            dim.join_on_column.as_deref(),
            &ctx.date_from,
            &ctx.date_to,
            &ctx.connection_mp_refs,
        ),
        fetch_drilldown_multi_period(
            &db_col,
            &metrics,
            dim.ref_table.as_deref(),
            dim.ref_display_column.as_deref(),
            dim.source_table.as_deref(),
            dim.join_on_column.as_deref(),
            &p2_from,
            &p2_to,
            &ctx.connection_mp_refs,
        ),
    );

    let rows1 = rows1_result?;
    let rows2 = rows2_result?;

    // По дням ключом служит день месяца, чтобы один и тот же день из П1 и П2
    // совпадал (01 ↔ 01) даже в разных месяцах.
    let day_key = |raw: &str| {
        if is_date_group {
            fmt_day_label(raw)
        } else {
            raw.to_string()
        }
    };
    let mut merged: std::collections::HashMap<String, DrilldownRow> =
        std::collections::HashMap::new();

    for (group_key, label, values) in rows1 {
        let key = day_key(&group_key);
        let label = day_key(&label);
        let entry = merged.entry(key.clone()).or_insert_with(|| DrilldownRow {
            group_key: key,
            label,
            value1: 0.0,
            value2: 0.0,
            delta_pct: None,
            metric_values: std::collections::HashMap::new(),
        });
        for (i, (id, _)) in metric_resolved.iter().enumerate() {
            let mv = entry.metric_values.entry(id.clone()).or_default();
            mv.value1 += values.get(i).copied().unwrap_or(0.0);
        }
    }

    for (group_key, label, values) in rows2 {
        let key = day_key(&group_key);
        let label = day_key(&label);
        let entry = merged.entry(key.clone()).or_insert_with(|| DrilldownRow {
            group_key: key,
            label,
            value1: 0.0,
            value2: 0.0,
            delta_pct: None,
            metric_values: std::collections::HashMap::new(),
        });
        for (i, (id, _)) in metric_resolved.iter().enumerate() {
            let mv = entry.metric_values.entry(id.clone()).or_default();
            mv.value2 += values.get(i).copied().unwrap_or(0.0);
        }
    }

    let mut rows: Vec<DrilldownRow> = merged
        .into_values()
        .map(|mut r| {
            for mv in r.metric_values.values_mut() {
                mv.delta_pct = pct_change(mv.value1, mv.value2);
            }
            r
        })
        .collect();

    if is_date_group {
        rows.sort_by(|a, b| a.group_key.cmp(&b.group_key));
    } else {
        let first_id = metric_resolved.first().map(|(id, _)| id.clone());
        rows.sort_by(|a, b| {
            let va = first_id
                .as_ref()
                .and_then(|id| a.metric_values.get(id))
                .map(|mv| mv.value1)
                .unwrap_or(0.0);
            let vb = first_id
                .as_ref()
                .and_then(|id| b.metric_values.get(id))
                .map(|mv| mv.value1)
                .unwrap_or(0.0);
            vb.partial_cmp(&va).unwrap_or(std::cmp::Ordering::Equal)
        });
    }

    let metric_columns: Vec<MetricColumnDef> = metric_resolved
        .iter()
        .map(|(id, metric)| MetricColumnDef {
            id: id.clone(),
            label: metric.label.to_string(),
        })
        .collect();

    Ok(DrilldownResponse {
        rows,
        group_by_label,
        period1_label,
        period2_label,
        metric_label: String::new(),
        metric_columns,
        selected_dimension: None,
        coverage: None,
        extra_columns: vec![],
        extra_values: std::collections::HashMap::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::{pct_change, sort_daily_rows, status_by_change, DailyRow};
    use contracts::shared::analytics::IndicatorStatus;

    fn finalize_scalar(
        current_total: f64,
        previous_total: f64,
        spark_points: &[f64],
    ) -> (f64, Option<f64>) {
        let cur = current_total;
        let change = pct_change(cur, previous_total);
        let _ = spark_points;
        (cur, change)
    }

    #[test]
    fn sparkline_daily_rows_are_sorted_by_day_offset() {
        let rows = vec![
            DailyRow {
                day_offset: "000009".to_string(),
                day_label: "2026-03-10".to_string(),
                total: 10.0,
            },
            DailyRow {
                day_offset: "000001".to_string(),
                day_label: "2026-03-02".to_string(),
                total: 2.0,
            },
            DailyRow {
                day_offset: "000000".to_string(),
                day_label: "2026-03-01".to_string(),
                total: 1.0,
            },
        ];

        let sorted = sort_daily_rows(rows);
        let totals: Vec<f64> = sorted.into_iter().map(|row| row.total).collect();

        assert_eq!(totals, vec![1.0, 2.0, 10.0]);
    }

    #[test]
    fn scalar_uses_period_aggregate_not_sum_of_daily_points() {
        let spark_points = vec![620.0, 310.0, 155.0];
        let (cur, change) = finalize_scalar(620.64, 1000.0, &spark_points);

        assert_eq!(spark_points.iter().sum::<f64>(), 1085.0);
        assert_eq!(cur, 620.64);
        assert_eq!(change, Some(((620.64 - 1000.0) / 1000.0) * 100.0));
    }

    #[test]
    fn expense_status_is_inverted_vs_growth_metrics() {
        assert_eq!(status_by_change(Some(12.0)), IndicatorStatus::Bad);
        assert_eq!(status_by_change(Some(-12.0)), IndicatorStatus::Good);
        assert_eq!(status_by_change(Some(2.0)), IndicatorStatus::Neutral);
        assert_eq!(status_by_change(None), IndicatorStatus::Neutral);
    }
}

pub fn meta() -> contracts::shared::data_view::DataViewMeta {
    const JSON: &str = include_str!("metadata.json");
    serde_json::from_str(JSON).expect("dv002/metadata.json parse error")
}
