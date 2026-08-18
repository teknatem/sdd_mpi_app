//! dv003 - DataView: MP order line KPI (2 periods)
//!
//! Data source: p909_mp_order_line_turnovers
//! Layer policy: oper only

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

fn pct_change_abs(cur: f64, prev: f64) -> Option<f64> {
    if prev.abs() < 0.01 {
        None
    } else {
        Some(((cur.abs() - prev.abs()) / prev.abs()) * 100.0)
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum MetricPolarity {
    Positive,
    Expense,
    MagnitudeNegative,
}

#[derive(Clone, Copy)]
struct MetricDef {
    id: &'static str,
    label: &'static str,
    condition_sql: &'static str,
    polarity: MetricPolarity,
}

impl MetricDef {
    fn aggregate_sql(self, alias: &str) -> String {
        let condition = self.condition_sql.replace("{alias}", alias);
        format!(
            "CAST(COALESCE(SUM(CASE WHEN {condition} THEN COALESCE({alias}.amount, 0) ELSE 0 END), 0) AS REAL)"
        )
    }

    fn single_metric_predicate(self, alias: &str) -> String {
        self.condition_sql.replace("{alias}", alias)
    }

    fn delta_pct(self, current: f64, previous: f64) -> Option<f64> {
        match self.polarity {
            MetricPolarity::Positive | MetricPolarity::Expense => pct_change(current, previous),
            MetricPolarity::MagnitudeNegative => pct_change_abs(current, previous),
        }
    }

    fn sort_value(self, value: f64) -> f64 {
        match self.polarity {
            MetricPolarity::MagnitudeNegative => value.abs(),
            _ => value,
        }
    }

    fn status(self, change: Option<f64>) -> IndicatorStatus {
        match self.polarity {
            MetricPolarity::Positive => match change {
                Some(c) if c > 5.0 => IndicatorStatus::Good,
                Some(c) if c < -5.0 => IndicatorStatus::Bad,
                _ => IndicatorStatus::Neutral,
            },
            MetricPolarity::Expense | MetricPolarity::MagnitudeNegative => match change {
                Some(c) if c < -5.0 => IndicatorStatus::Good,
                Some(c) if c > 5.0 => IndicatorStatus::Bad,
                _ => IndicatorStatus::Neutral,
            },
        }
    }
}

fn resolve_metric(ctx: &ViewContext) -> MetricDef {
    resolve_metric_by_id(
        ctx.params
            .get("metric")
            .map(|s| s.as_str())
            .unwrap_or("revenue"),
    )
}

fn resolve_metric_by_id(id: &str) -> MetricDef {
    match id {
        "revenue_price" => MetricDef {
            id: "revenue_price",
            label: "Выручка (прайс)",
            condition_sql: "{alias}.turnover_code = 'customer_revenue'",
            polarity: MetricPolarity::Positive,
        },
        "coinvest" => MetricDef {
            id: "coinvest",
            label: "Соинвест",
            condition_sql: "{alias}.turnover_code = 'wb_coinvestment'",
            polarity: MetricPolarity::Positive,
        },
        "acquiring" => MetricDef {
            id: "acquiring",
            label: "Эквайринг",
            condition_sql: "{alias}.turnover_code = 'mp_acquiring'",
            polarity: MetricPolarity::Expense,
        },
        "cost" => MetricDef {
            id: "cost",
            label: "Себестоимость",
            condition_sql: "{alias}.turnover_code = 'item_cost'",
            polarity: MetricPolarity::Expense,
        },
        "commission" => MetricDef {
            id: "commission",
            label: "Комиссия",
            condition_sql: "{alias}.turnover_code = 'mp_commission'",
            polarity: MetricPolarity::Expense,
        },
        "returns" => MetricDef {
            id: "returns",
            label: "Возвраты",
            condition_sql:
                "{alias}.event_kind = 'returned' AND {alias}.turnover_code IN ('customer_revenue', 'spp_discount')",
            polarity: MetricPolarity::MagnitudeNegative,
        },
        _ => MetricDef {
            id: "revenue",
            label: "Выручка",
            condition_sql: "{alias}.turnover_code IN ('customer_revenue', 'spp_discount')",
            polarity: MetricPolarity::Positive,
        },
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
        SELECT CAST(COALESCE(SUM(COALESCE(p.amount, 0)), 0) AS REAL) AS total
        FROM p909_mp_order_line_turnovers p
        WHERE p.layer = 'oper'
          AND p.entry_date >= ? AND p.entry_date <= ?
          AND ({metric_predicate})
        "#,
        metric_predicate = metric.single_metric_predicate("p")
    );

    let mut params: Vec<sea_orm::Value> =
        vec![date_from.to_string().into(), date_to.to_string().into()];

    if !connection_mp_refs.is_empty() {
        let placeholders: Vec<&str> = connection_mp_refs.iter().map(|_| "?").collect();
        sql.push_str(&format!(
            " AND p.connection_mp_ref IN ({})",
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
            CAST(COALESCE(SUM(COALESCE(t.amount, 0)), 0) AS REAL) AS total
        FROM p909_mp_order_line_turnovers t
        WHERE t.layer = 'oper'
          AND t.entry_date >= ? AND t.entry_date <= ?
          AND ({metric_predicate})
        "#,
        metric_predicate = metric.single_metric_predicate("t")
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
        for value in connection_mp_refs {
            params.push(value.clone().into());
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
                CAST(COALESCE(SUM(COALESCE(t.amount, 0)), 0) AS REAL) AS total
            FROM p909_mp_order_line_turnovers t
            WHERE t.layer = 'oper'
              AND t.entry_date >= ? AND t.entry_date <= ?
              AND ({metric_predicate})
            "#,
            metric_predicate = metric.single_metric_predicate("t")
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
            for value in connection_mp_refs {
                params.push(value.clone().into());
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
                CAST(COALESCE(SUM(COALESCE(t.amount, 0)), 0) AS REAL) AS total
            FROM p909_mp_order_line_turnovers t
            LEFT JOIN {src_tbl} {alias} ON t.{join_col} = {alias}.id
            WHERE t.layer = 'oper'
              AND t.entry_date >= ? AND t.entry_date <= ?
              AND ({metric_predicate})
            "#,
            metric_predicate = metric.single_metric_predicate("t")
        );

        let mut params: Vec<sea_orm::Value> =
            vec![date_from.to_string().into(), date_to.to_string().into()];

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
            CAST(COALESCE(SUM(COALESCE(t.amount, 0)), 0) AS REAL) AS total
        FROM p909_mp_order_line_turnovers t
        {join_clause}
        WHERE t.layer = 'oper'
          AND t.entry_date >= ? AND t.entry_date <= ?
          AND ({metric_predicate})
        "#,
        metric_predicate = metric.single_metric_predicate("t")
    );

    let mut params: Vec<sea_orm::Value> =
        vec![date_from.to_string().into(), date_to.to_string().into()];

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
            FROM p909_mp_order_line_turnovers t
            WHERE t.layer = 'oper'
              AND t.entry_date >= ? AND t.entry_date <= ?
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
            FROM p909_mp_order_line_turnovers t
            LEFT JOIN {src_tbl} {alias} ON t.{join_col} = {alias}.id
            WHERE t.layer = 'oper'
              AND t.entry_date >= ? AND t.entry_date <= ?
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
            FROM p909_mp_order_line_turnovers t
            {join_clause}
            WHERE t.layer = 'oper'
              AND t.entry_date >= ? AND t.entry_date <= ?
            "#
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
        for value in connection_mp_refs {
            params.push(value.clone().into());
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

    let current = current_result?;
    let daily = sort_daily_rows(daily_result?);
    let previous = prev_result?;
    let spark_points: Vec<f64> = daily.iter().map(|row| row.total).collect();
    let change = metric.delta_pct(current, previous);

    Ok(IndicatorValue {
        id: IndicatorId::new("dv003_mp_order_line_turnovers"),
        value: Some(current),
        previous_value: Some(previous),
        change_percent: change,
        status: metric.status(change),
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

    for row in rows1 {
        let key = day_key(&row.group_key);
        let label = day_key(&row.label);
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
            .value1 += row.total;
    }

    for row in rows2 {
        let key = day_key(&row.group_key);
        let label = day_key(&row.label);
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
            .value2 += row.total;
    }

    let mut rows: Vec<DrilldownRow> = merged
        .into_values()
        .map(|mut row| {
            row.delta_pct = metric.delta_pct(row.value1, row.value2);
            row
        })
        .collect();

    if is_date_group {
        rows.sort_by(|a, b| a.group_key.cmp(&b.group_key));
    } else {
        rows.sort_by(|a, b| {
            metric
                .sort_value(b.value1)
                .partial_cmp(&metric.sort_value(a.value1))
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
        for (index, (id, _)) in metric_resolved.iter().enumerate() {
            let metric_value = entry.metric_values.entry(id.clone()).or_default();
            metric_value.value1 += values.get(index).copied().unwrap_or(0.0);
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
        for (index, (id, _)) in metric_resolved.iter().enumerate() {
            let metric_value = entry.metric_values.entry(id.clone()).or_default();
            metric_value.value2 += values.get(index).copied().unwrap_or(0.0);
        }
    }

    let mut rows: Vec<DrilldownRow> = merged
        .into_values()
        .map(|mut row| {
            for (id, metric_value) in &mut row.metric_values {
                let metric = metric_resolved
                    .iter()
                    .find(|(metric_id, _)| metric_id == id)
                    .map(|(_, metric)| *metric)
                    .unwrap_or_else(|| resolve_metric_by_id(id));
                metric_value.delta_pct = metric.delta_pct(metric_value.value1, metric_value.value2);
            }
            row
        })
        .collect();

    if is_date_group {
        rows.sort_by(|a, b| a.group_key.cmp(&b.group_key));
    } else {
        let first_metric = metric_resolved.first().map(|(_, metric)| *metric);
        rows.sort_by(|a, b| {
            let va = first_metric
                .map(|metric| {
                    a.metric_values
                        .get(metric.id)
                        .map(|mv| metric.sort_value(mv.value1))
                        .unwrap_or(0.0)
                })
                .unwrap_or(0.0);
            let vb = first_metric
                .map(|metric| {
                    b.metric_values
                        .get(metric.id)
                        .map(|mv| metric.sort_value(mv.value1))
                        .unwrap_or(0.0)
                })
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
    use super::{pct_change_abs, resolve_metric_by_id, sort_daily_rows, DailyRow};
    use contracts::shared::analytics::IndicatorStatus;

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
    fn returns_change_uses_magnitude() {
        assert_eq!(pct_change_abs(-300.0, -100.0), Some(200.0));
        assert_eq!(pct_change_abs(-80.0, -100.0), Some(-20.0));
    }

    #[test]
    fn status_polarity_matches_metric_kind() {
        let revenue = resolve_metric_by_id("revenue");
        let expense = resolve_metric_by_id("cost");
        let returns = resolve_metric_by_id("returns");

        assert_eq!(revenue.status(Some(12.0)), IndicatorStatus::Good);
        assert_eq!(revenue.status(Some(-12.0)), IndicatorStatus::Bad);
        assert_eq!(expense.status(Some(12.0)), IndicatorStatus::Bad);
        assert_eq!(expense.status(Some(-12.0)), IndicatorStatus::Good);
        assert_eq!(returns.status(Some(12.0)), IndicatorStatus::Bad);
        assert_eq!(returns.status(Some(-12.0)), IndicatorStatus::Good);
    }

    #[test]
    fn sort_for_returns_uses_absolute_value() {
        let returns = resolve_metric_by_id("returns");
        assert!(returns.sort_value(-500.0) > returns.sort_value(-100.0));
    }
}

pub fn meta() -> contracts::shared::data_view::DataViewMeta {
    const JSON: &str = include_str!("metadata.json");
    serde_json::from_str(JSON).expect("dv003/metadata.json parse error")
}
