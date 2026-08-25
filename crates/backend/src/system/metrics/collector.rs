//! Сбор снимка метрик.
//!
//! Условие подсистемы — «никаких длительных сканирований». Поэтому здесь нет ни
//! одного обхода данных: всё либо PRAGMA, либо чтение маленьких `sys_*`-таблиц
//! по индексу, либо статические реестры в памяти. Самое дорогое место —
//! `sys_data_profile`, но он посчитан заранее и читается как обычная таблица.
//!
//! Ошибка одного источника не отменяет снимок: метрика просто не попадёт в него,
//! а на странице будет видно «нет данных». Терять остальные сорок значений из-за
//! недоступного S3 или неприменённой миграции бессмысленно.

use contracts::system::metrics::{DetailRow, DetailTable, MetricSnapshotDto};
use sea_orm::{DatabaseBackend, FromQueryResult, Statement};
use std::collections::BTreeMap;

use super::{codebase::codebase, repository};
use crate::shared::config;

#[derive(FromQueryResult)]
struct CountRow {
    n: i64,
}

/// Аккумулятор снимка: значения + таблицы-детализации.
#[derive(Default)]
struct Draft {
    values: BTreeMap<String, f64>,
    details: Vec<DetailTable>,
}

impl Draft {
    fn put(&mut self, key: &str, value: f64) {
        // NaN/inf в REAL-колонке превращаются в мусор, который потом ломает
        // график: лучше не записать метрику вовсе.
        if value.is_finite() {
            self.values.insert(key.to_string(), value);
        }
    }
}

/// Собрать снимок и записать его. Возвращает id снимка и число метрик.
pub async fn collect_and_store(trigger: &str) -> anyhow::Result<(String, i64, usize)> {
    let started = std::time::Instant::now();
    let mut draft = Draft::default();

    // Метрики кода приходят из бинаря — они уже посчитаны на сборке.
    let code = codebase();
    for (key, value) in &code.metrics {
        draft.put(key, *value);
    }
    draft.details.extend(code.details.iter().cloned());

    collect_database(&mut draft).await;
    collect_quality(&mut draft).await;
    collect_access(&mut draft);
    collect_tasks(&mut draft).await;
    collect_instance(&mut draft).await;
    collect_knowledge(&mut draft).await;

    let (app_version, git_commit, build_profile, schema_version) = build_passport().await;
    let collect_ms = started.elapsed().as_millis() as i64;

    let snapshot = MetricSnapshotDto {
        id: uuid::Uuid::new_v4().to_string(),
        captured_at: chrono::Utc::now().to_rfc3339(),
        trigger: trigger.to_string(),
        app_version,
        git_commit,
        build_profile,
        schema_version,
        code_generated_at: code.generated_at.clone(),
        collect_ms,
    };

    let details_json = serde_json::to_string(&draft.details).unwrap_or_else(|_| "[]".to_string());
    let written = draft.values.len();

    repository::insert_snapshot(&snapshot, &draft.values, &details_json).await?;
    if let Err(error) = repository::prune(repository::KEEP_SNAPSHOTS).await {
        tracing::warn!("[metrics] снимки не почищены: {error}");
    }

    tracing::info!(
        "[metrics] снимок {} записан: {} метрик за {} мс",
        snapshot.id,
        written,
        collect_ms
    );

    Ok((snapshot.id, collect_ms, written))
}

async fn build_passport() -> (String, Option<String>, String, i64) {
    match config::load_config() {
        Ok(config) => {
            let source = crate::system::datasets::source_info(&config).await;
            let commit = (source.git_commit != "unknown").then_some(source.git_commit);
            (
                source.app_version,
                commit,
                source.build_profile,
                source.schema_version,
            )
        }
        Err(error) => {
            // Без конфига страница всё равно должна открыться: паспорт станет
            // беднее, метрики — нет.
            tracing::warn!("[metrics] паспорт сборки неполон: {error}");
            (
                env!("CARGO_PKG_VERSION").to_string(),
                option_env!("GIT_COMMIT").map(str::to_string),
                option_env!("BUILD_PROFILE")
                    .unwrap_or("unknown")
                    .to_string(),
                crate::shared::data::migration_runner::current_migration_version()
                    .await
                    .unwrap_or(-1),
            )
        }
    }
}

async fn collect_database(draft: &mut Draft) {
    match crate::shared::data::raw_storage::vacuum_status().await {
        Ok(status) => {
            draft.put("db.file_mb", status.file_mb);
            draft.put("db.wal_mb", status.wal_mb);
            draft.put("db.reclaimable_mb", status.reclaimable_mb);
        }
        Err(error) => tracing::warn!("[metrics] размер БД не получен: {error}"),
    }

    match crate::shared::data::raw_storage::status().await {
        Ok(status) => draft.put("db.raw_storage_mb", status.total_mb),
        Err(error) => tracing::warn!("[metrics] статус raw-хранилища не получен: {error}"),
    }

    // Профиль данных считается отдельной подсистемой при старте и по требованию.
    // Здесь только читаем — COUNT(*) по 200 таблицам был бы тем самым «долгим
    // сканированием», которого страница обещала не делать.
    let profile = crate::shared::data::data_profile::list_all().await;
    if profile.is_empty() {
        return;
    }

    let rows_total: i64 = profile.iter().map(|row| row.row_count).sum();
    draft.put("db.rows_total", rows_total as f64);
    draft.put("db.tables_profiled", profile.len() as f64);

    if let Some(newest) = profile
        .iter()
        .filter_map(|row| chrono::DateTime::parse_from_rfc3339(&row.computed_at).ok())
        .max()
    {
        let age_hours =
            (chrono::Utc::now() - newest.with_timezone(&chrono::Utc)).num_minutes() as f64 / 60.0;
        draft.put("db.profile_age_hours", age_hours.max(0.0));
    }

    // `list_all` уже отсортирован по убыванию строк.
    draft.details.push(DetailTable {
        code: "db.top_tables".to_string(),
        label: "Крупнейшие таблицы".to_string(),
        value_label: "Строк".to_string(),
        rows: profile
            .iter()
            .take(10)
            .map(|row| DetailRow {
                name: row.table_name.clone(),
                value: row.row_count as f64,
                extra: row
                    .date_max
                    .as_ref()
                    .map(|date| format!("данные по {date}")),
            })
            .collect(),
    });
}

async fn collect_quality(draft: &mut Draft) {
    let overviews = match crate::quality::list_check_overviews().await {
        Ok(items) => items,
        Err(error) => {
            tracing::warn!("[metrics] обзор quality-проверок не получен: {error}");
            return;
        }
    };

    draft.put("quality.checks", overviews.len() as f64);

    let now = chrono::Utc::now();
    let mut min_compliance: Option<f64> = None;
    let mut failing = 0.0;
    let mut never_run = 0.0;
    let mut stalest_days: f64 = 0.0;
    let mut rows = Vec::with_capacity(overviews.len());

    for overview in &overviews {
        let Some(run) = overview.latest_run.as_ref() else {
            never_run += 1.0;
            rows.push(DetailRow {
                name: overview.info.name.clone(),
                value: 0.0,
                extra: Some("не запускалась".to_string()),
            });
            continue;
        };

        let age_days = (now - run.started_at).num_minutes() as f64 / 1440.0;
        stalest_days = stalest_days.max(age_days);

        let population = run.population_total.unwrap_or(0);
        let violations = run.violations_total.unwrap_or(0);
        if violations > 0 {
            failing += 1.0;
        }

        // Доля без знаменателя бессмысленна: пустая популяция — это «нечего
        // нарушать», а не «100 % соответствия», и в минимум она не идёт.
        let compliance = if population > 0 {
            let value = (population - violations) as f64 * 100.0 / population as f64;
            min_compliance = Some(min_compliance.map_or(value, |current: f64| current.min(value)));
            value
        } else {
            0.0
        };

        rows.push(DetailRow {
            name: overview.info.name.clone(),
            value: compliance,
            extra: Some(format!(
                "{} из {} · {}",
                violations,
                population,
                run.started_at.format("%d.%m %H:%M")
            )),
        });
    }

    if let Some(value) = min_compliance {
        draft.put("quality.min_compliance", value);
    }
    draft.put("quality.failing_checks", failing);
    draft.put("quality.never_run", never_run);
    draft.put("quality.stalest_run_days", stalest_days);

    draft.details.push(DetailTable {
        code: "quality.checks".to_string(),
        label: "Проверки качества данных".to_string(),
        value_label: "Соответствие, %".to_string(),
        rows,
    });
}

fn collect_access(draft: &mut Draft) {
    let report = crate::system::audit::compute_violations();
    draft.put("access.violations", report.violations.len() as f64);
    // Число роутов по реестру policy — не то же самое, что `arch.routes` из
    // ARCHITECTURE.md: тот считает вызовы `.route()` только в `api/routes.rs`.
    // Расхождение между двумя цифрами само по себе полезный сигнал.
    draft.put("access.routes_total", report.total_routes as f64);
    draft.put("access.unscoped_routes", report.unscoped_routes as f64);
    draft.put("access.open_routes", report.open_routes as f64);
    draft.put("access.scoped_routes", report.scoped_routes as f64);
}

async fn collect_tasks(draft: &mut Draft) {
    // `datetime(...)` вокруг колонки — идиома этого проекта: в `sys_task_runs`
    // время лежит в формате SeaORM, и сравнивать строки напрямую нельзя.
    if let Some(failed) = scalar(
        "SELECT COUNT(*) AS n FROM sys_task_runs \
         WHERE status = 'Failed' AND datetime(started_at) >= datetime('now', '-1 day')",
    )
    .await
    {
        draft.put("tasks.failed_24h", failed as f64);
    }

    if let Some(enabled) = scalar(
        "SELECT COUNT(*) AS n FROM sys_tasks \
         WHERE is_enabled = 1 AND COALESCE(is_deleted, 0) = 0",
    )
    .await
    {
        draft.put("tasks.enabled", enabled as f64);
    }
}

async fn collect_instance(draft: &mut Draft) {
    // Снимок пишется после сбора, поэтому текущий рестарт сюда ещё не попал —
    // прибавляем его сами, иначе на свежей базе будет ноль.
    match repository::snapshots_since(7).await {
        Ok(count) => draft.put("instance.restarts_7d", count as f64 + 1.0),
        Err(error) => tracing::warn!("[metrics] число рестартов не получено: {error}"),
    }
}

/// Агрегаты инвентаризации знаний из её последнего снимка.
///
/// Здесь ничего не считается заново: инвентаризация — свой механизм со своими
/// таблицами и своей страницей. Сюда переезжают два числа — разрывы
/// цепочки достижимости, единственное в инвентаризации, у чего есть
/// направление и пороги. Объём корпуса и число статей направления не имеют и
/// остаются на своей странице.
///
/// Храповик коммита их не видит: он сравнивает `codebase_metrics.json`, а это
/// рантайм. Пороги работают на странице, гейта нет — и не должно быть, число
/// сирот зависит от файлов навыков экземпляра.
///
/// Снимка может ещё не быть (первый старт) — тогда метрик просто не будет, и
/// это честнее нуля.
async fn collect_knowledge(draft: &mut Draft) {
    let db = crate::shared::data::db::get_connection();
    let latest = match crate::knowledge::repository::latest(db).await {
        Ok(Some(found)) => found,
        Ok(None) => return,
        Err(error) => {
            tracing::warn!("[metrics] снимок знаний не прочитан: {error}");
            return;
        }
    };
    let (_snapshot, summary) = latest;
    draft.put(
        "knowledge.unreachable_surfaces",
        summary.unreachable_surfaces.len() as f64,
    );
    draft.put("knowledge.orphan_tools", summary.orphan_tools.len() as f64);
}

async fn scalar(sql: &str) -> Option<i64> {
    let db = crate::shared::data::db::get_connection();
    match CountRow::find_by_statement(Statement::from_string(DatabaseBackend::Sqlite, sql))
        .one(db)
        .await
    {
        Ok(row) => row.map(|row| row.n),
        Err(error) => {
            tracing::warn!("[metrics] запрос не выполнен ({sql}): {error}");
            None
        }
    }
}
