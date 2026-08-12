use anyhow::Result;
use chrono::{Datelike, Duration, NaiveDate, NaiveDateTime, Utc};
use contracts::system::ext_api_log::{
    ExtApiHistoryPoint, ExtApiHistoryRequest, ExtApiHistoryResponse, ExtApiLogRow, ExtApiMetric,
    ExtApiScale, ExtApiSummaryResponse, ExtApiTotals,
};

use super::repository;

/// Тот же сдвиг, что у истории регламентных заданий (`runs_service`): графики МСК.
const HISTORY_TZ_OFFSET_HOURS: i64 = 3;

/// Сколько суток храним лог. Прунинг — фоновым циклом из `main.rs`
/// (планировщик выключен в config.toml, регламентное задание не сработало бы).
pub const RETENTION_DAYS: i64 = 90;

pub async fn record(m: repository::Model) -> Result<()> {
    repository::insert(m)
        .await
        .map_err(|e| anyhow::anyhow!("Database error: {}", e))
}

pub async fn list_recent(limit: u64) -> Result<Vec<ExtApiLogRow>> {
    repository::list_recent(limit)
        .await
        .map_err(|e| anyhow::anyhow!("Database error: {}", e))
}

pub async fn prune_old() -> Result<u64> {
    repository::prune_older_than(RETENTION_DAYS)
        .await
        .map_err(|e| anyhow::anyhow!("Database error: {}", e))
}

/// Границы периода в UTC для SQL + локальное (МСК) начало и число бакетов.
struct Period {
    local_start: NaiveDateTime,
    from_sql: String,
    to_sql: String,
    bucket_count: u32,
    bucket_size_seconds: i64,
}

fn resolve_period(date_from: &str, scale: ExtApiScale) -> Result<Period> {
    let from = NaiveDate::parse_from_str(date_from, "%Y-%m-%d")
        .map_err(|e| anyhow::anyhow!("Invalid date_from '{}': {}", date_from, e))?;
    let to = period_end(from, scale);
    let local_start = from
        .and_hms_opt(0, 0, 0)
        .ok_or_else(|| anyhow::anyhow!("Invalid start date"))?;
    let local_end = to
        .and_hms_opt(0, 0, 0)
        .ok_or_else(|| anyhow::anyhow!("Invalid end date"))?;
    let utc_start = local_start - Duration::hours(HISTORY_TZ_OFFSET_HOURS);
    let utc_end = local_end - Duration::hours(HISTORY_TZ_OFFSET_HOURS);
    Ok(Period {
        local_start,
        from_sql: format!("{}Z", utc_start.format("%Y-%m-%dT%H:%M:%S")),
        to_sql: format!("{}Z", utc_end.format("%Y-%m-%dT%H:%M:%S")),
        bucket_count: bucket_count(local_start, local_end, scale),
        bucket_size_seconds: bucket_size_seconds(scale),
    })
}

pub async fn query_history(req: ExtApiHistoryRequest) -> Result<ExtApiHistoryResponse> {
    let period = resolve_period(&req.date_from, req.scale)?;
    let events = repository::query_period(&period.from_sql, &period.to_sql)
        .await
        .map_err(|e| anyhow::anyhow!("Database error: {}", e))?;

    let size = period.bucket_count as usize;
    // sums/counts разделены: для AvgDurationMs нужно поделить сумму на число вызовов
    // в бакете, а не на длину периода.
    let mut sums = vec![0.0_f64; size];
    let mut counts = vec![0_u64; size];
    let mut totals = ExtApiTotals::default();
    let mut duration_sum = 0.0_f64;

    for ev in events {
        let Some(offset) = bucket_offset(&ev.ts, &period) else {
            continue;
        };

        totals.req_count += 1;
        totals.bytes_out += ev.bytes_out;
        if ev.status >= 400 {
            totals.error_count += 1;
        }
        duration_sum += ev.duration_ms as f64;

        // Запрос — точечное событие, поэтому попадает ровно в один бакет
        // (в отличие от запуска задания, который размазывается по своему интервалу).
        let value = match req.metric {
            ExtApiMetric::RequestCount => 1.0,
            ExtApiMetric::TrafficBytes => ev.bytes_out as f64,
            ExtApiMetric::AvgDurationMs => ev.duration_ms as f64,
            ExtApiMetric::ErrorCount => {
                if ev.status >= 400 {
                    1.0
                } else {
                    0.0
                }
            }
        };
        if let Some(slot) = sums.get_mut(offset) {
            *slot += value;
        }
        if let Some(slot) = counts.get_mut(offset) {
            *slot += 1;
        }
    }

    if totals.req_count > 0 {
        totals.avg_ms = duration_sum / totals.req_count as f64;
    }

    let points = sums
        .into_iter()
        .enumerate()
        .filter_map(|(offset, sum)| {
            let hits = counts.get(offset).copied().unwrap_or(0);
            if hits == 0 {
                return None;
            }
            let value = match req.metric {
                ExtApiMetric::AvgDurationMs => sum / hits as f64,
                _ => sum,
            };
            if value <= 0.0 {
                return None;
            }
            let bucket =
                period.local_start + Duration::seconds(offset as i64 * period.bucket_size_seconds);
            Some(ExtApiHistoryPoint {
                bucket: format!("{} MSK", bucket.format("%Y-%m-%dT%H:%M:%S")),
                value,
                offset: offset as u32,
            })
        })
        .collect();

    Ok(ExtApiHistoryResponse {
        points,
        bucket_count: period.bucket_count,
        date_from: req.date_from,
        totals,
    })
}

pub async fn summary(date_from: &str, scale: ExtApiScale) -> Result<ExtApiSummaryResponse> {
    let period = resolve_period(date_from, scale)?;
    let by_route = repository::summary_by_route(&period.from_sql, &period.to_sql)
        .await
        .map_err(|e| anyhow::anyhow!("Database error: {}", e))?;
    let by_client = repository::summary_by_client(&period.from_sql, &period.to_sql)
        .await
        .map_err(|e| anyhow::anyhow!("Database error: {}", e))?;
    Ok(ExtApiSummaryResponse {
        by_route,
        by_client,
    })
}

/// Индекс бакета для метки времени события (UTC ISO8601) внутри периода.
fn bucket_offset(ts: &str, period: &Period) -> Option<usize> {
    let parsed = chrono::DateTime::parse_from_rfc3339(ts).ok()?.naive_utc();
    let local = parsed + Duration::hours(HISTORY_TZ_OFFSET_HOURS);
    let seconds = (local - period.local_start).num_seconds();
    if seconds < 0 {
        return None;
    }
    let offset = (seconds / period.bucket_size_seconds) as usize;
    (offset < period.bucket_count as usize).then_some(offset)
}

fn bucket_size_seconds(scale: ExtApiScale) -> i64 {
    match scale {
        ExtApiScale::Day => 60,
        ExtApiScale::Week => 5 * 60,
        ExtApiScale::Month => 60 * 60,
    }
}

fn bucket_count(start: NaiveDateTime, end: NaiveDateTime, scale: ExtApiScale) -> u32 {
    let minutes = (end - start).num_minutes().max(0);
    match scale {
        ExtApiScale::Day => minutes as u32,
        ExtApiScale::Week => (minutes / 5) as u32,
        ExtApiScale::Month => (minutes / 60) as u32,
    }
}

fn period_end(date_from: NaiveDate, scale: ExtApiScale) -> NaiveDate {
    match scale {
        ExtApiScale::Day => date_from + Duration::days(1),
        ExtApiScale::Week => date_from + Duration::days(7),
        ExtApiScale::Month => add_one_month(date_from),
    }
}

fn add_one_month(date: NaiveDate) -> NaiveDate {
    let (year, month) = if date.month() == 12 {
        (date.year() + 1, 1)
    } else {
        (date.year(), date.month() + 1)
    };
    let last_day = last_day_of_month(year, month);
    NaiveDate::from_ymd_opt(year, month, date.day().min(last_day)).unwrap_or(date)
}

fn last_day_of_month(year: i32, month: u32) -> u32 {
    let next_month = if month == 12 {
        NaiveDate::from_ymd_opt(year + 1, 1, 1)
    } else {
        NaiveDate::from_ymd_opt(year, month + 1, 1)
    };
    next_month
        .map(|date| (date - Duration::days(1)).day())
        .unwrap_or(31)
}

/// Фоновый цикл прунинга: при старте и далее раз в сутки.
pub async fn run_prune_loop() {
    loop {
        if let Some(_database_activity) = crate::system::maintenance::try_begin_database_activity()
        {
            match prune_old().await {
                Ok(n) if n > 0 => {
                    println!("[ext-api-log] pruned {n} rows older than {RETENTION_DAYS} days")
                }
                Ok(_) => {}
                Err(e) => tracing::warn!("[ext-api-log] prune failed: {e}"),
            }
        }
        tokio::time::sleep(std::time::Duration::from_secs(24 * 60 * 60)).await;
    }
}

/// Текущее время в формате, в котором храним `ts`.
pub fn now_ts() -> String {
    Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn day_period_has_minute_buckets() {
        let p = resolve_period("2026-07-16", ExtApiScale::Day).unwrap();
        assert_eq!(p.bucket_count, 24 * 60);
        assert_eq!(p.bucket_size_seconds, 60);
        // Локальные 00:00 МСК = 21:00 UTC предыдущих суток.
        assert_eq!(p.from_sql, "2026-07-15T21:00:00Z");
        assert_eq!(p.to_sql, "2026-07-16T21:00:00Z");
    }

    #[test]
    fn week_and_month_bucket_counts() {
        let w = resolve_period("2026-07-13", ExtApiScale::Week).unwrap();
        assert_eq!(w.bucket_count, 7 * 24 * 12); // 5-минутные бакеты
        let m = resolve_period("2026-07-01", ExtApiScale::Month).unwrap();
        assert_eq!(m.bucket_count, 31 * 24); // часовые бакеты, июль = 31 день
    }

    #[test]
    fn event_lands_in_msk_bucket_not_utc() {
        let p = resolve_period("2026-07-16", ExtApiScale::Day).unwrap();
        // 18:44 UTC = 21:44 МСК → бакет 21*60+44.
        assert_eq!(
            bucket_offset("2026-07-16T18:44:56.133Z", &p),
            Some(21 * 60 + 44)
        );
        // 21:00 UTC 15-го = ровно 00:00 МСК 16-го → первый бакет.
        assert_eq!(bucket_offset("2026-07-15T21:00:00.000Z", &p), Some(0));
    }

    #[test]
    fn events_outside_period_are_dropped() {
        let p = resolve_period("2026-07-16", ExtApiScale::Day).unwrap();
        // За минуту до начала суток по МСК.
        assert_eq!(bucket_offset("2026-07-15T20:59:59.000Z", &p), None);
        // Ровно конец периода — уже следующие сутки.
        assert_eq!(bucket_offset("2026-07-16T21:00:00.000Z", &p), None);
    }
}
