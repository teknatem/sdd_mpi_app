//! dv008 - DataView: WB sales funnel (2 periods)
//!
//! Source: a036_wb_sales_funnel_daily (lines_json exploded via json_each), same
//! data as the "Воронка продаж WB" report plugin (crates/backend/src/plugins/funnel.rs),
//! reimplemented natively here so the LLM chat / BI layer can query it through the
//! ordinary DataView tools instead of invoking the plugin engine.
//!
//! Required params: none (metric defaults to order_sum for scalar).
//! Optional params:
//!   metric = open_count | cart_count | order_count | order_sum | buyout_count |
//!            buyout_sum | cart_conv_pct | order_conv_pct | buyout_pct
//!            (scalar only; default order_sum)

use anyhow::{anyhow, Result};
use contracts::shared::analytics::{IndicatorId, IndicatorStatus, IndicatorValue};
use contracts::shared::data_view::ViewContext;
use contracts::shared::drilldown::{DrilldownResponse, DrilldownRow, MetricColumnDef};
use sea_orm::{ConnectionTrait, FromQueryResult, Statement};
use std::collections::HashMap;

use crate::shared::data::db::get_connection;

const VIEW_ID: &str = "dv008_wb_sales_funnel";
const DEFAULT_METRIC_ID: &str = "order_sum";

const ALL_METRIC_IDS: &[&str] = &[
    "open_count",
    "cart_count",
    "order_count",
    "order_sum",
    "buyout_count",
    "buyout_sum",
    "cart_conv_pct",
    "order_conv_pct",
    "buyout_pct",
];

struct MetricDef {
    /// Aggregate SQL expression over the `d, json_each(d.lines_json) j` row set.
    expr: &'static str,
    label: &'static str,
}

fn resolve_metric_def(id: &str) -> Result<MetricDef> {
    let m = |expr: &'static str, label: &'static str| MetricDef { expr, label };
    match id {
        "open_count" => Ok(m(
            "CAST(COALESCE(SUM(json_extract(j.value, '$.metrics.open_count')), 0) AS REAL)",
            // Именно ПЕРЕХОДЫ (открытия карточки), а не показы в выдаче: слово «просмотры»
            // читается как impressions и путало LLM с show_* из p916.
            "Переходы в карточку",
        )),
        "cart_count" => Ok(m(
            "CAST(COALESCE(SUM(json_extract(j.value, '$.metrics.cart_count')), 0) AS REAL)",
            "В корзину, шт.",
        )),
        "order_count" => Ok(m(
            "CAST(COALESCE(SUM(json_extract(j.value, '$.metrics.order_count')), 0) AS REAL)",
            "Заказано, шт.",
        )),
        "order_sum" => Ok(m(
            "CAST(COALESCE(SUM(json_extract(j.value, '$.metrics.order_sum')), 0) AS REAL)",
            "Заказано, сумма",
        )),
        "buyout_count" => Ok(m(
            "CAST(COALESCE(SUM(json_extract(j.value, '$.metrics.buyout_count')), 0) AS REAL)",
            "Выкуплено, шт.",
        )),
        "buyout_sum" => Ok(m(
            "CAST(COALESCE(SUM(json_extract(j.value, '$.metrics.buyout_sum')), 0) AS REAL)",
            "Выкуплено, сумма",
        )),
        "cart_conv_pct" => Ok(m(
            "CASE WHEN COALESCE(SUM(json_extract(j.value, '$.metrics.open_count')), 0) = 0 THEN 0 \
             ELSE COALESCE(SUM(json_extract(j.value, '$.metrics.cart_count')), 0) * 100.0 \
                  / SUM(json_extract(j.value, '$.metrics.open_count')) END",
            "Конверсия в корзину, %",
        )),
        "order_conv_pct" => Ok(m(
            "CASE WHEN COALESCE(SUM(json_extract(j.value, '$.metrics.cart_count')), 0) = 0 THEN 0 \
             ELSE COALESCE(SUM(json_extract(j.value, '$.metrics.order_count')), 0) * 100.0 \
                  / SUM(json_extract(j.value, '$.metrics.cart_count')) END",
            "Конверсия в заказ, %",
        )),
        "buyout_pct" => Ok(m(
            "CASE WHEN COALESCE(SUM(json_extract(j.value, '$.metrics.order_count')), 0) = 0 THEN 0 \
             ELSE COALESCE(SUM(json_extract(j.value, '$.metrics.buyout_count')), 0) * 100.0 \
                  / SUM(json_extract(j.value, '$.metrics.order_count')) END",
            "Процент выкупа, %",
        )),
        other => Err(anyhow!(
            "Unsupported metric '{}' for {}. Expected one of: {}",
            other,
            VIEW_ID,
            ALL_METRIC_IDS.join(", ")
        )),
    }
}

fn resolve_metric(ctx: &ViewContext) -> Result<(String, MetricDef)> {
    let id = ctx
        .params
        .get("metric")
        .map(|v| v.trim())
        .filter(|v| !v.is_empty())
        .unwrap_or(DEFAULT_METRIC_ID)
        .to_string();
    let def = resolve_metric_def(&id)?;
    Ok((id, def))
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

/// Дней в диапазоне (включительно). None — если дату не разобрать.
fn span_days(date_from: &str, date_to: &str) -> Option<i64> {
    let f = chrono::NaiveDate::parse_from_str(date_from, "%Y-%m-%d").ok()?;
    let t = chrono::NaiveDate::parse_from_str(date_to, "%Y-%m-%d").ok()?;
    Some((t - f).num_days() + 1)
}

/// Диапазон укладывается в один календарный месяц.
fn within_single_month(date_from: &str, date_to: &str) -> bool {
    date_from.len() >= 7 && date_to.len() >= 7 && date_from[..7] == date_to[..7]
}

fn shift_days(d: &str, days: i64) -> String {
    match chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d") {
        Ok(date) => (date - chrono::Duration::days(days))
            .format("%Y-%m-%d")
            .to_string(),
        Err(_) => d.to_string(),
    }
}

/// Период сравнения по умолчанию — непосредственно предшествующее окно.
///
/// Сдвиг ровно на месяц верен только для запроса в пределах одного месяца.
/// Для диапазона в год он давал период2, перекрывающий период1 на одиннадцать
/// месяцев: сравнение шло почти само с собой (value2 ≈ value1).
fn resolve_period2(ctx: &ViewContext) -> (String, String) {
    match (&ctx.period2_from, &ctx.period2_to) {
        (Some(f), Some(t)) => (f.clone(), t.clone()),
        _ if within_single_month(&ctx.date_from, &ctx.date_to) => {
            (shift_date(&ctx.date_from, -1), shift_date(&ctx.date_to, -1))
        }
        _ => match span_days(&ctx.date_from, &ctx.date_to) {
            Some(days) if days > 0 => (
                shift_days(&ctx.date_from, days),
                shift_days(&ctx.date_to, days),
            ),
            _ => (shift_date(&ctx.date_from, -1), shift_date(&ctx.date_to, -1)),
        },
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
        if to_parts.first() == Some(&parts[0]) && to_parts.get(1) == Some(&parts[1]) {
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

/// Day-of-month label, used so period-1 and period-2 "date" rows line up
/// (01 ↔ 01) even when the two periods fall in different months.
///
/// Схлопывание к дню месяца допустимо ТОЛЬКО когда каждый период лежит внутри
/// одного месяца. На диапазоне в год оно молча склеивало тринадцать первых
/// чисел в одну строку «01», и по такой таблице считали динамику.
fn day_key(iso: &str) -> String {
    let parts: Vec<&str> = iso.split('-').collect();
    if parts.len() >= 3 {
        parts[2].to_string()
    } else {
        iso.to_string()
    }
}

#[derive(Debug, FromQueryResult)]
struct AggRow {
    total: f64,
}

#[derive(Debug, FromQueryResult)]
struct DailyRow {
    total: f64,
}

fn append_connection_filter(sql: &mut String, params: &mut Vec<sea_orm::Value>, refs: &[String]) {
    if refs.is_empty() {
        return;
    }
    let placeholders: Vec<&str> = refs.iter().map(|_| "?").collect();
    sql.push_str(&format!(
        " AND d.connection_id IN ({})",
        placeholders.join(", ")
    ));
    for value in refs {
        params.push(value.clone().into());
    }
}

async fn fetch_aggregate(
    metric_expr: &str,
    date_from: &str,
    date_to: &str,
    connection_mp_refs: &[String],
) -> Result<f64> {
    let db = get_connection();
    let mut sql = format!(
        r#"
        SELECT {metric_expr} AS total
        FROM a036_wb_sales_funnel_daily d, json_each(d.lines_json) j
        WHERE d.is_deleted = 0 AND d.document_date >= ? AND d.document_date <= ?
        "#
    );
    let mut params: Vec<sea_orm::Value> =
        vec![date_from.to_string().into(), date_to.to_string().into()];
    append_connection_filter(&mut sql, &mut params, connection_mp_refs);

    let stmt = Statement::from_sql_and_values(sea_orm::DatabaseBackend::Sqlite, &sql, params);
    let row = AggRow::find_by_statement(stmt)
        .one(db)
        .await?
        .unwrap_or(AggRow { total: 0.0 });
    Ok(row.total)
}

async fn fetch_daily_rows(
    metric_expr: &str,
    date_from: &str,
    date_to: &str,
    connection_mp_refs: &[String],
) -> Result<Vec<DailyRow>> {
    let db = get_connection();
    let mut sql = format!(
        r#"
        SELECT {metric_expr} AS total
        FROM a036_wb_sales_funnel_daily d, json_each(d.lines_json) j
        WHERE d.is_deleted = 0 AND d.document_date >= ? AND d.document_date <= ?
        "#
    );
    let mut params: Vec<sea_orm::Value> =
        vec![date_from.to_string().into(), date_to.to_string().into()];
    append_connection_filter(&mut sql, &mut params, connection_mp_refs);
    sql.push_str(" GROUP BY d.document_date ORDER BY d.document_date ASC");

    let stmt = Statement::from_sql_and_values(sea_orm::DatabaseBackend::Sqlite, &sql, params);
    Ok(DailyRow::find_by_statement(stmt).all(db).await?)
}

fn group_by_label(group_by: &str) -> Result<&'static str> {
    match group_by {
        "nm_id" => Ok("По товару"),
        "date" => Ok("По дню"),
        "week" => Ok("По неделе"),
        "month" => Ok("По месяцу"),
        "connection_mp_ref" => Ok("По кабинету МП"),
        other => Err(anyhow!("Unsupported group_by '{}' for {}", other, VIEW_ID)),
    }
}

/// SQL-выражение ключа группировки по периоду.
///
/// Помесячный разрез — рабочий ответ на «динамику за год»: группировка по дню
/// вернула бы 368 строк, которые нечитаемы и в отчёте, и в контексте модели.
fn period_key_expr(group_by: &str) -> Option<&'static str> {
    match group_by {
        "date" => Some("d.document_date"),
        // %Y-%W: неделя года по ISO-подобной нумерации SQLite (неделя с понедельника).
        "week" => Some("strftime('%Y-W%W', d.document_date)"),
        "month" => Some("substr(d.document_date, 1, 7)"),
        _ => None,
    }
}

/// Слагаемые, из которых собираются ВСЕ метрики представления.
///
/// Дрилдаун тянет только их, а проценты выводит из накопленных сумм. Складывать
/// сами проценты нельзя: при слиянии строк в одну группу это давало «процент
/// выкупа 1023%» вместо 81,8% — сумму тринадцати дневных долей.
const BASE_METRIC_IDS: &[&str] = &[
    "open_count",
    "cart_count",
    "order_count",
    "order_sum",
    "buyout_count",
    "buyout_sum",
];

/// Накопленные слагаемые одной группы.
#[derive(Debug, Default, Clone, Copy)]
struct Bases([f64; BASE_METRIC_IDS.len()]);

impl Bases {
    fn add(&mut self, values: &[f64]) {
        for (slot, value) in self.0.iter_mut().zip(values.iter()) {
            *slot += value;
        }
    }

    fn sum_of(&self, id: &str) -> f64 {
        BASE_METRIC_IDS
            .iter()
            .position(|base| *base == id)
            .map(|i| self.0[i])
            .unwrap_or(0.0)
    }

    /// Значение любой метрики представления из накопленных слагаемых.
    fn metric(&self, id: &str) -> f64 {
        let ratio = |numerator: &str, denominator: &str| {
            let d = self.sum_of(denominator);
            if d == 0.0 {
                0.0
            } else {
                self.sum_of(numerator) * 100.0 / d
            }
        };
        match id {
            "cart_conv_pct" => ratio("cart_count", "open_count"),
            "order_conv_pct" => ratio("order_count", "cart_count"),
            "buyout_pct" => ratio("buyout_count", "order_count"),
            other => self.sum_of(other),
        }
    }
}

/// Fetch one period of drilldown base sums in a single query.
async fn fetch_drilldown_multi_period(
    group_by: &str,
    date_from: &str,
    date_to: &str,
    connection_mp_refs: &[String],
) -> Result<Vec<(String, String, Bases)>> {
    let db = get_connection();
    let metric_cols: String = BASE_METRIC_IDS
        .iter()
        .enumerate()
        .map(|(i, id)| {
            format!(
                "CAST(COALESCE(SUM(json_extract(j.value, '$.metrics.{id}')), 0) AS REAL) AS m{i}"
            )
        })
        .collect::<Vec<_>>()
        .join(", ");

    let mut params: Vec<sea_orm::Value> =
        vec![date_from.to_string().into(), date_to.to_string().into()];

    let mut sql = match group_by {
        _ if period_key_expr(group_by).is_some() => {
            let key = period_key_expr(group_by).expect("checked above");
            format!(
                r#"
            SELECT {key} AS group_key, {key} AS label, {metric_cols}
            FROM a036_wb_sales_funnel_daily d, json_each(d.lines_json) j
            WHERE d.is_deleted = 0 AND d.document_date >= ? AND d.document_date <= ?
            "#
            )
        }
        "nm_id" => format!(
            r#"
            SELECT
                CAST(CAST(json_extract(j.value, '$.nm_id') AS INTEGER) AS TEXT) AS group_key,
                COALESCE(MAX(json_extract(j.value, '$.title')), '') AS label,
                {metric_cols}
            FROM a036_wb_sales_funnel_daily d, json_each(d.lines_json) j
            WHERE d.is_deleted = 0 AND d.document_date >= ? AND d.document_date <= ?
            "#
        ),
        "connection_mp_ref" => format!(
            r#"
            SELECT
                d.connection_id AS group_key,
                COALESCE(c.description, d.connection_id) AS label,
                {metric_cols}
            FROM a036_wb_sales_funnel_daily d
            LEFT JOIN a006_connection_mp c ON c.id = d.connection_id, json_each(d.lines_json) j
            WHERE d.is_deleted = 0 AND d.document_date >= ? AND d.document_date <= ?
            "#
        ),
        other => return Err(anyhow!("Unsupported group_by '{}' for {}", other, VIEW_ID)),
    };

    append_connection_filter(&mut sql, &mut params, connection_mp_refs);

    let group_clause = match group_by {
        _ if period_key_expr(group_by).is_some() => format!(
            " GROUP BY {} ORDER BY group_key ASC",
            period_key_expr(group_by).expect("checked above")
        ),
        "nm_id" => " GROUP BY CAST(json_extract(j.value, '$.nm_id') AS INTEGER) ORDER BY m0 DESC"
            .to_string(),
        "connection_mp_ref" => " GROUP BY d.connection_id ORDER BY m0 DESC".to_string(),
        _ => unreachable!(),
    };
    sql.push_str(&group_clause);

    let stmt = Statement::from_sql_and_values(sea_orm::DatabaseBackend::Sqlite, &sql, params);
    let rows = db.query_all(stmt).await?;
    rows.into_iter()
        .map(|row| {
            let group_key: String = row.try_get("", "group_key")?;
            let label: String = row.try_get("", "label")?;
            let values: Vec<f64> = (0..BASE_METRIC_IDS.len())
                .map(|i| row.try_get::<f64>("", &format!("m{}", i)).unwrap_or(0.0))
                .collect();
            let mut bases = Bases::default();
            bases.add(&values);
            Ok((group_key, label, bases))
        })
        .collect::<std::result::Result<Vec<_>, sea_orm::DbErr>>()
        .map_err(anyhow::Error::from)
}

pub async fn compute_scalar(ctx: &ViewContext) -> Result<IndicatorValue> {
    let (metric_id, def) = resolve_metric(ctx)?;
    let (p2_from, p2_to) = resolve_period2(ctx);

    let (current_result, daily_result, prev_result) = tokio::join!(
        fetch_aggregate(
            def.expr,
            &ctx.date_from,
            &ctx.date_to,
            &ctx.connection_mp_refs
        ),
        fetch_daily_rows(
            def.expr,
            &ctx.date_from,
            &ctx.date_to,
            &ctx.connection_mp_refs
        ),
        fetch_aggregate(def.expr, &p2_from, &p2_to, &ctx.connection_mp_refs),
    );

    let current = current_result?;
    let daily = daily_result?;
    let previous = prev_result?;
    let change = pct_change(current, previous);

    Ok(IndicatorValue {
        id: IndicatorId::new(VIEW_ID),
        value: Some(current),
        previous_value: Some(previous),
        change_percent: change,
        status: IndicatorStatus::Neutral,
        subtitle: Some(format!("Воронка продаж WB [{}]", def.label)),
        details: vec![format!("Метрика: {} ({})", def.label, metric_id)],
        spark_points: daily.into_iter().map(|row| row.total).collect(),
    })
}

pub async fn compute_drilldown_multi(
    ctx: &ViewContext,
    group_by: &str,
    metric_ids: &[String],
) -> Result<DrilldownResponse> {
    let group_label = group_by_label(group_by)?;
    let ids: Vec<String> = if metric_ids.is_empty() {
        ALL_METRIC_IDS.iter().map(|s| s.to_string()).collect()
    } else {
        metric_ids.to_vec()
    };
    let metrics: Vec<(String, MetricDef)> = ids
        .iter()
        .map(|id| resolve_metric_def(id).map(|def| (id.clone(), def)))
        .collect::<Result<Vec<_>>>()?;

    let (p2_from, p2_to) = resolve_period2(ctx);
    // Схлопывать даты к дню месяца можно только когда оба периода лежат внутри
    // одного месяца — иначе «01» склеит первые числа всех месяцев диапазона.
    let collapse_to_day = group_by == "date"
        && within_single_month(&ctx.date_from, &ctx.date_to)
        && within_single_month(&p2_from, &p2_to);
    let is_date_group = period_key_expr(group_by).is_some();

    let (rows1, rows2) = tokio::join!(
        fetch_drilldown_multi_period(
            group_by,
            &ctx.date_from,
            &ctx.date_to,
            &ctx.connection_mp_refs
        ),
        fetch_drilldown_multi_period(group_by, &p2_from, &p2_to, &ctx.connection_mp_refs),
    );
    let rows1 = rows1?;
    let rows2 = rows2?;

    let key_of = |raw: &str| {
        if collapse_to_day {
            day_key(raw)
        } else {
            raw.to_string()
        }
    };

    // Копим слагаемые, а не готовые метрики: проценты выводим один раз в конце.
    let mut merged: HashMap<String, (String, Bases, Bases)> = HashMap::new();

    for (group_key, label, bases) in rows1 {
        let key = key_of(&group_key);
        let label = if collapse_to_day {
            key_of(&label)
        } else {
            label
        };
        let entry = merged
            .entry(key)
            .or_insert_with(|| (label, Bases::default(), Bases::default()));
        entry.1.add(&bases.0);
    }

    for (group_key, label, bases) in rows2 {
        let key = key_of(&group_key);
        let label = if collapse_to_day {
            key_of(&label)
        } else {
            label
        };
        let entry = merged
            .entry(key)
            .or_insert_with(|| (label, Bases::default(), Bases::default()));
        entry.2.add(&bases.0);
    }

    let mut rows: Vec<DrilldownRow> = merged
        .into_iter()
        .map(|(group_key, (label, bases1, bases2))| {
            let metric_values = metrics
                .iter()
                .map(|(id, _)| {
                    let value1 = bases1.metric(id);
                    let value2 = bases2.metric(id);
                    (
                        id.clone(),
                        contracts::shared::drilldown::MetricValues {
                            value1,
                            value2,
                            delta_pct: pct_change(value1, value2),
                        },
                    )
                })
                .collect();
            DrilldownRow {
                group_key,
                label,
                value1: 0.0,
                value2: 0.0,
                delta_pct: None,
                metric_values,
            }
        })
        .collect();

    if is_date_group {
        rows.sort_by(|a, b| a.group_key.cmp(&b.group_key));
    } else {
        let first_id = metrics.first().map(|(id, _)| id.clone());
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

    let metric_columns: Vec<MetricColumnDef> = metrics
        .iter()
        .map(|(id, def)| MetricColumnDef {
            id: id.clone(),
            label: def.label.to_string(),
        })
        .collect();

    Ok(DrilldownResponse {
        rows,
        group_by_label: group_label.to_string(),
        period1_label: period_label(&ctx.date_from, &ctx.date_to),
        period2_label: period_label(&p2_from, &p2_to),
        metric_label: String::new(),
        metric_columns,
        selected_dimension: None,
        coverage: None,
        extra_columns: vec![],
        extra_values: HashMap::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::{
        day_key, pct_change, resolve_metric_def, span_days, within_single_month, Bases,
        BASE_METRIC_IDS,
    };

    fn bases_of(pairs: &[(&str, f64)]) -> Bases {
        let mut b = Bases::default();
        let values: Vec<f64> = BASE_METRIC_IDS
            .iter()
            .map(|id| {
                pairs
                    .iter()
                    .find(|(name, _)| name == id)
                    .map(|(_, v)| *v)
                    .unwrap_or(0.0)
            })
            .collect();
        b.add(&values);
        b
    }

    /// Регрессия: при слиянии строк складывались готовые проценты. На запросе
    /// «воронка WB за год с группировкой по дате» тринадцать первых чисел
    /// схлопывались в строку «01», и процент выкупа выходил 1023,8% вместо 81,8%.
    #[test]
    fn ratios_are_recomputed_from_sums_not_added() {
        let mut merged = Bases::default();
        // Тринадцать дней, каждый ~81,8% выкупа.
        for _ in 0..13 {
            let day = bases_of(&[("order_count", 173.0), ("buyout_count", 141.0)]);
            merged.add(&day.0);
        }
        let buyout_pct = merged.metric("buyout_pct");
        assert!(
            (buyout_pct - 81.5).abs() < 1.0,
            "процент выкупа должен остаться долей, получено {buyout_pct}"
        );
        // Счётчики, наоборот, обязаны складываться.
        assert_eq!(merged.metric("order_count"), 173.0 * 13.0);
    }

    #[test]
    fn ratio_with_zero_denominator_is_zero_not_nan() {
        let empty = bases_of(&[("buyout_count", 5.0)]);
        assert_eq!(empty.metric("buyout_pct"), 0.0);
        assert_eq!(empty.metric("cart_conv_pct"), 0.0);
    }

    #[test]
    fn every_view_metric_derives_from_bases() {
        let b = bases_of(&[
            ("open_count", 1000.0),
            ("cart_count", 100.0),
            ("order_count", 50.0),
            ("order_sum", 12345.0),
            ("buyout_count", 40.0),
            ("buyout_sum", 9000.0),
        ]);
        assert_eq!(b.metric("cart_conv_pct"), 10.0);
        assert_eq!(b.metric("order_conv_pct"), 50.0);
        assert_eq!(b.metric("buyout_pct"), 80.0);
        assert_eq!(b.metric("order_sum"), 12345.0);
        assert_eq!(b.metric("buyout_sum"), 9000.0);
    }

    /// Схлопывание к дню месяца осмысленно только внутри одного месяца.
    #[test]
    fn period_grain_keys_cover_day_week_and_month() {
        use super::period_key_expr;
        assert_eq!(period_key_expr("date"), Some("d.document_date"));
        assert_eq!(
            period_key_expr("month"),
            Some("substr(d.document_date, 1, 7)")
        );
        assert!(period_key_expr("week").is_some());
        // Разрезы не по периоду сюда не попадают — у них своя ветка SQL.
        assert_eq!(period_key_expr("nm_id"), None);
        assert_eq!(period_key_expr("connection_mp_ref"), None);
    }

    #[test]
    fn new_grains_are_labelled() {
        use super::group_by_label;
        assert_eq!(group_by_label("month").unwrap(), "По месяцу");
        assert_eq!(group_by_label("week").unwrap(), "По неделе");
        assert!(group_by_label("quarter").is_err());
    }

    #[test]
    fn single_month_detection_gates_day_collapsing() {
        assert!(within_single_month("2026-07-01", "2026-07-31"));
        assert!(!within_single_month("2025-08-01", "2026-08-03"));
        assert!(!within_single_month("2026-06-25", "2026-07-05"));
    }

    /// Регрессия: период сравнения сдвигался ровно на месяц независимо от длины
    /// запроса, из-за чего годовой диапазон сравнивался почти сам с собой.
    #[test]
    fn span_days_counts_range_inclusively() {
        assert_eq!(span_days("2026-07-01", "2026-07-31"), Some(31));
        assert_eq!(span_days("2026-07-01", "2026-07-01"), Some(1));
        assert_eq!(span_days("2025-08-01", "2026-08-03"), Some(368));
        assert_eq!(span_days("не дата", "2026-07-01"), None);
    }

    #[test]
    fn day_key_extracts_day_of_month() {
        assert_eq!(day_key("2026-07-05"), "05");
    }

    #[test]
    fn pct_change_none_for_near_zero_previous() {
        assert_eq!(pct_change(10.0, 0.0), None);
    }

    #[test]
    fn unsupported_metric_is_rejected() {
        assert!(resolve_metric_def("not_a_metric").is_err());
    }

    #[test]
    fn all_known_metrics_resolve() {
        for id in super::ALL_METRIC_IDS {
            assert!(
                resolve_metric_def(id).is_ok(),
                "metric {} should resolve",
                id
            );
        }
    }
}

pub fn meta() -> contracts::shared::data_view::DataViewMeta {
    const JSON: &str = include_str!("metadata.json");
    serde_json::from_str(JSON).expect("dv008/metadata.json parse error")
}
