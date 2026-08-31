//! ## Проверка: строки проекций без исходных регистраторов
//!
//! Находит строки проекций, где `registrator_ref` указывает на документ,
//! которого уже нет в исходном агрегате. Такие строки нельзя исправить
//! перепроведением, их нужно удалить из проекции.

use crate::shared::registrators;
use contracts::quality::{
    BreakdownRow, CheckBreakdown, CheckMetric, CheckResult, NipCleanupResult, NipGroupsResponse,
    NipProjectionRow, NipRegistratorGroup, QualityCheckInfo, QualityCheckSource,
};

pub const CHECK_ID: &str = "projection_orphan_registrators";

const SOURCES: [(&str, &str); 3] = [
    (
        "p909_mp_order_line_turnovers",
        "p909 — Обороты строк заказов МП",
    ),
    (
        "p911_wb_advert_by_items",
        "p911 — Рекламные расходы WB по номенклатуре",
    ),
    (
        "p913_wb_advert_order_attr",
        "p913 — Атрибуция рекламных расходов по заказам WB",
    ),
];

pub fn info() -> QualityCheckInfo {
    QualityCheckInfo {
        code: String::new(),
        id: CHECK_ID.to_string(),
        name: "Cтроки проекций без регистраторов".to_string(),
        description:
            "Находит строки p909/p911/p913, где документ-регистратор уже отсутствует в исходном агрегате. \
             Такие строки можно удалить из проекции."
                .to_string(),
        category: "Целостность проекций".to_string(),
    }
}

pub fn list_sources() -> Vec<QualityCheckSource> {
    SOURCES
        .iter()
        .map(|(projection_table, label)| QualityCheckSource {
            projection_table: (*projection_table).to_string(),
            label: (*label).to_string(),
        })
        .collect()
}

fn allowed_table(table: &str) -> anyhow::Result<&str> {
    match table {
        "p909_mp_order_line_turnovers"
        | "p911_wb_advert_by_items"
        | "p913_wb_advert_order_attr" => Ok(table),
        other => Err(anyhow::anyhow!(
            "NOT_FOUND: Unknown projection_table '{}'",
            other
        )),
    }
}

fn allowed_sort_field(field: &str) -> &str {
    match field {
        "missing_count" | "registrator_type" | "registrator_ref" | "min_entry_date"
        | "max_entry_date" => field,
        _ => "missing_count",
    }
}

async fn load_orphan_groups(table: &str) -> anyhow::Result<Vec<NipRegistratorGroup>> {
    use sea_orm::{ConnectionTrait, Statement};

    let table = allowed_table(table)?;
    let conn = crate::shared::data::db::get_connection();
    let sql = format!(
        r#"SELECT
               registrator_type,
               registrator_ref,
               CAST(COUNT(*) AS INTEGER) AS missing_count,
               MIN(entry_date) AS min_entry_date,
               MAX(entry_date) AS max_entry_date
           FROM {table}
           GROUP BY registrator_type, registrator_ref"#
    );

    let rows = conn
        .query_all(Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            sql,
        ))
        .await?;

    let mut items = Vec::new();
    for row in rows {
        let registrator_type: String = row.try_get("", "registrator_type").unwrap_or_default();
        let registrator_ref: String = row.try_get("", "registrator_ref").unwrap_or_default();

        let source_exists =
            registrators::source_document_exists(&registrator_type, &registrator_ref)
                .await
                .unwrap_or(false);
        if source_exists {
            continue;
        }

        let missing_count: i64 = row.try_get("", "missing_count").unwrap_or(0);
        let min_entry_date: Option<String> = row.try_get("", "min_entry_date").ok();
        let max_entry_date: Option<String> = row.try_get("", "max_entry_date").ok();
        let meta = registrators::meta(&registrator_type);
        let display_short = if registrator_ref.len() > 8 {
            format!("{}…", &registrator_ref[..8])
        } else {
            registrator_ref.clone()
        };

        items.push(NipRegistratorGroup {
            projection_table: table.to_string(),
            registrator_type,
            registrator_ref,
            registrator_type_label: format!("{} (документ не найден)", meta.type_label),
            display_short,
            min_entry_date,
            max_entry_date,
            missing_count,
            can_post: false,
            can_cleanup: true,
            tab_key_prefix: None,
            source_columns: Vec::new(),
        });
    }

    Ok(items)
}

/// Возвращает общее число строк в таблице (популяция правила).
async fn count_rows(table: &str) -> anyhow::Result<i64> {
    use sea_orm::{ConnectionTrait, Statement};
    let table = allowed_table(table)?;
    let conn = crate::shared::data::db::get_connection();
    let rows = conn
        .query_all(Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            format!("SELECT CAST(COUNT(*) AS INTEGER) AS cnt FROM {table}"),
        ))
        .await?;
    Ok(rows
        .first()
        .and_then(|r| r.try_get::<i64>("", "cnt").ok())
        .unwrap_or(0))
}

pub async fn run() -> anyhow::Result<CheckResult> {
    let mut metrics = Vec::new();
    for (table, label) in SOURCES {
        let violations = load_orphan_groups(table)
            .await?
            .into_iter()
            .map(|g| g.missing_count)
            .sum();
        let population = count_rows(table).await?;
        metrics.push(CheckMetric {
            label: label.to_string(),
            population,
            violations,
            unit: "строк".to_string(),
        });
    }

    let population_total = metrics.iter().map(|m| m.population).sum();
    let violations_total = metrics.iter().map(|m| m.violations).sum();

    Ok(CheckResult {
        check_id: CHECK_ID.to_string(),
        run_at: chrono::Utc::now(),
        population_total,
        violations_total,
        metrics,
        violations: Vec::new(),
    })
}

/// Разрез: разбиение осиротевших строк по типу документа-регистратора.
pub async fn breakdowns() -> anyhow::Result<Vec<CheckBreakdown>> {
    use std::collections::BTreeMap;

    let mut by_type: BTreeMap<String, i64> = BTreeMap::new();
    for (table, _) in SOURCES {
        for g in load_orphan_groups(table).await? {
            let label = registrators::meta(&g.registrator_type)
                .type_label
                .to_string();
            *by_type.entry(label).or_insert(0) += g.missing_count;
        }
    }

    let mut rows: Vec<BreakdownRow> = by_type
        .into_iter()
        .filter(|(_, cnt)| *cnt > 0)
        .map(|(label, cnt)| BreakdownRow {
            label,
            population: cnt,
            violations: cnt,
        })
        .collect();
    rows.sort_by(|a, b| b.population.cmp(&a.population));

    if rows.is_empty() {
        return Ok(Vec::new());
    }

    Ok(vec![CheckBreakdown {
        key: "by_registrator_type".to_string(),
        title: "Осиротевшие строки по типу документа".to_string(),
        dimension_label: "Тип документа".to_string(),
        is_partition: true,
        rows,
    }])
}

pub async fn list_groups(
    projection_table: &str,
    page: i64,
    page_size: i64,
    sort_by: &str,
    sort_desc: bool,
) -> anyhow::Result<NipGroupsResponse> {
    let sort_field = allowed_sort_field(sort_by).to_string();
    let mut items = load_orphan_groups(projection_table).await?;

    items.sort_by(|a, b| {
        let ord = match sort_field.as_str() {
            "registrator_type" => a.registrator_type.cmp(&b.registrator_type),
            "registrator_ref" => a.registrator_ref.cmp(&b.registrator_ref),
            "min_entry_date" => a.min_entry_date.cmp(&b.min_entry_date),
            "max_entry_date" => a.max_entry_date.cmp(&b.max_entry_date),
            _ => a.missing_count.cmp(&b.missing_count),
        };
        if sort_desc {
            ord.reverse()
        } else {
            ord
        }
    });

    let total = items.len() as i64;
    let start = (page * page_size).max(0) as usize;
    let end = (start + page_size.max(0) as usize).min(items.len());
    let page_items = if start < items.len() {
        items[start..end].to_vec()
    } else {
        Vec::new()
    };

    Ok(NipGroupsResponse {
        items: page_items,
        total,
        page,
        page_size,
    })
}

pub async fn list_rows(
    projection_table: &str,
    registrator_ref: &str,
) -> anyhow::Result<Vec<NipProjectionRow>> {
    use sea_orm::{ConnectionTrait, Statement};

    let table = allowed_table(projection_table)?;
    let conn = crate::shared::data::db::get_connection();
    let sql = match table {
        "p909_mp_order_line_turnovers" => format!(
            r#"SELECT id, entry_date, turnover_code, amount, connection_mp_ref,
                      'Заказ' AS context_label, order_key AS context_value
               FROM p909_mp_order_line_turnovers
               WHERE registrator_ref = '{registrator_ref}'
               ORDER BY entry_date, id
               LIMIT 500"#
        ),
        "p911_wb_advert_by_items" => format!(
            r#"SELECT id, entry_date, turnover_code, amount, connection_mp_ref,
                      'Кампания' AS context_label, wb_advert_campaign_code AS context_value
               FROM p911_wb_advert_by_items
               WHERE registrator_ref = '{registrator_ref}'
               ORDER BY entry_date, id
               LIMIT 500"#
        ),
        "p913_wb_advert_order_attr" => format!(
            r#"SELECT id, entry_date, turnover_code, amount, connection_mp_ref,
                      'Заказ/Кампания' AS context_label,
                      (order_key || ' / ' || wb_advert_campaign_code) AS context_value
               FROM p913_wb_advert_order_attr
               WHERE registrator_ref = '{registrator_ref}'
               ORDER BY entry_date, id
               LIMIT 500"#
        ),
        _ => unreachable!(),
    };

    let rows = conn
        .query_all(Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            sql,
        ))
        .await?;

    Ok(rows
        .into_iter()
        .map(|row| NipProjectionRow {
            id: row.try_get("", "id").unwrap_or_default(),
            entry_date: row.try_get("", "entry_date").unwrap_or_default(),
            turnover_code: row.try_get("", "turnover_code").unwrap_or_default(),
            amount: row.try_get("", "amount").unwrap_or(0.0),
            connection_mp_ref: row.try_get("", "connection_mp_ref").unwrap_or_default(),
            context_label: row.try_get("", "context_label").ok(),
            context_value: row.try_get("", "context_value").ok(),
        })
        .collect())
}

pub async fn cleanup(
    projection_table: &str,
    registrator_refs: &[String],
) -> anyhow::Result<NipCleanupResult> {
    use sea_orm::{ConnectionTrait, Statement};

    let table = allowed_table(projection_table)?;
    let conn = crate::shared::data::db::get_connection();
    let requested = registrator_refs.len();
    let mut deleted_rows = 0usize;
    let mut errors = Vec::new();

    for registrator_ref in registrator_refs {
        let check_sql = format!(
            r#"SELECT registrator_type
               FROM {table}
               WHERE registrator_ref = ?
               LIMIT 1"#
        );
        let check_rows = conn
            .query_all(Statement::from_sql_and_values(
                sea_orm::DatabaseBackend::Sqlite,
                &check_sql,
                [registrator_ref.clone().into()],
            ))
            .await?;
        let Some(first_row) = check_rows.first() else {
            continue;
        };
        let registrator_type: String = first_row
            .try_get("", "registrator_type")
            .unwrap_or_default();

        let source_exists =
            registrators::source_document_exists(&registrator_type, registrator_ref)
                .await
                .unwrap_or(true);
        if source_exists {
            errors.push(format!("{registrator_ref}: исходный документ существует"));
            continue;
        }

        let delete_sql =
            format!("DELETE FROM {table} WHERE registrator_type = ? AND registrator_ref = ?");
        let result = conn
            .execute(Statement::from_sql_and_values(
                sea_orm::DatabaseBackend::Sqlite,
                &delete_sql,
                [registrator_type.into(), registrator_ref.clone().into()],
            ))
            .await?;
        deleted_rows += result.rows_affected() as usize;
    }

    Ok(NipCleanupResult {
        requested,
        deleted_rows,
        errors,
    })
}
