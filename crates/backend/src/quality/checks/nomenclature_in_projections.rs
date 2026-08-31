//! ## Проверка: заполненность `nomenclature_ref` в проекциях
//!
//! Анализирует таблицы p909, p911, p913 на наличие строк, где поле
//! `nomenclature_ref` не заполнено (`IS NULL` или пустая строка).
//!
//! Незаполненный `nomenclature_ref` означает, что строка не может быть
//! корректно атрибутирована к номенклатуре 1С и не войдёт в аналитические
//! расчёты (обороты, рекламные расходы, сверки).
//!
//! **Типичные причины:** товар маркетплейса (a007) создан без привязки к
//! номенклатуре, либо привязка появилась позже, но документы не были
//! перепроведены.

use crate::shared::registrators;
use contracts::quality::{
    BreakdownRow, CheckBreakdown, CheckMetric, CheckResult, NipGroupsResponse, NipProjectionRow,
    NipRegistratorGroup, NipRepostResult, QualityCheckInfo, QualityCheckSource,
};

pub const CHECK_ID: &str = "nomenclature_in_projections";

/// Таблицы проекций, на которые распространяется правило: (таблица, метка).
const PROJECTIONS: [(&str, &str); 3] = [
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
        name: "Заполненность номенклатуры в проекциях".to_string(),
        description:
            "Проверяет наличие ссылки на номенклатуру (nomenclature_ref) в проекциях p909, p911, p913. \
             Незаполненные строки группируются по проекции."
                .to_string(),
        category: "Номенклатура".to_string(),
    }
}

/// Условие нарушения: пустая ссылка на номенклатуру.
const VIOLATION_PREDICATE: &str = "nomenclature_ref IS NULL OR TRIM(nomenclature_ref) = ''";

pub async fn run() -> anyhow::Result<CheckResult> {
    use sea_orm::{ConnectionTrait, Statement};

    let conn = crate::shared::data::db::get_connection();

    let mut metrics: Vec<CheckMetric> = Vec::with_capacity(PROJECTIONS.len());
    for (table, label) in PROJECTIONS {
        // Популяция (все строки проекции) и нарушения (без номенклатуры) одним запросом.
        let sql = format!(
            r#"SELECT
                   CAST(COUNT(*) AS INTEGER) AS population,
                   CAST(SUM(CASE WHEN {VIOLATION_PREDICATE} THEN 1 ELSE 0 END) AS INTEGER) AS violations
               FROM {table}"#
        );
        let rows = conn
            .query_all(Statement::from_string(
                sea_orm::DatabaseBackend::Sqlite,
                sql,
            ))
            .await?;
        let (population, violations) = rows
            .first()
            .map(|r| {
                (
                    r.try_get::<i64>("", "population").unwrap_or(0),
                    r.try_get::<i64>("", "violations").unwrap_or(0),
                )
            })
            .unwrap_or((0, 0));

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

// ─────────────────────────────────────────────────────────────────────────────
// Разрезы для страницы детализации
// ─────────────────────────────────────────────────────────────────────────────

/// Возвращает разрезы метрик: по кабинету (срез популяции) и по исправимости
/// нарушений (разбиение самих нарушений на категории устранения).
pub async fn breakdowns() -> anyhow::Result<Vec<CheckBreakdown>> {
    let mut out = Vec::new();
    if let Some(b) = by_connection_breakdown().await? {
        out.push(b);
    }
    if let Some(b) = fixability_breakdown().await? {
        out.push(b);
    }
    Ok(out)
}

/// Разрез по кабинету: популяция и нарушения каждого кабинета по всем трём проекциям.
/// Показываются только кабинеты, где есть хотя бы одно нарушение.
async fn by_connection_breakdown() -> anyhow::Result<Option<CheckBreakdown>> {
    use sea_orm::{ConnectionTrait, Statement};

    let conn = crate::shared::data::db::get_connection();
    let union = PROJECTIONS
        .iter()
        .map(|(table, _)| {
            format!(
                "SELECT connection_mp_ref, \
                 CASE WHEN {VIOLATION_PREDICATE} THEN 1 ELSE 0 END AS is_violation \
                 FROM {table}"
            )
        })
        .collect::<Vec<_>>()
        .join("\n            UNION ALL\n            ");

    let sql = format!(
        r#"SELECT
               COALESCE(c.description, u.connection_mp_ref, '(не указано)') AS cabinet,
               CAST(COUNT(*) AS INTEGER) AS population,
               CAST(SUM(u.is_violation) AS INTEGER) AS violations
           FROM (
            {union}
           ) u
           LEFT JOIN a006_connection_mp c ON u.connection_mp_ref = c.id
           GROUP BY cabinet
           HAVING violations > 0
           ORDER BY violations DESC, population DESC
           LIMIT 100"#
    );

    let rows = conn
        .query_all(Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            sql,
        ))
        .await?;

    let breakdown_rows: Vec<BreakdownRow> = rows
        .into_iter()
        .map(|r| BreakdownRow {
            label: r.try_get("", "cabinet").unwrap_or_default(),
            population: r.try_get("", "population").unwrap_or(0),
            violations: r.try_get("", "violations").unwrap_or(0),
        })
        .collect();

    if breakdown_rows.is_empty() {
        return Ok(None);
    }

    Ok(Some(CheckBreakdown {
        key: "by_connection".to_string(),
        title: "По кабинету маркетплейса".to_string(),
        dimension_label: "Кабинет".to_string(),
        is_partition: false,
        rows: breakdown_rows,
    }))
}

/// Разбиение нарушений по способу устранения: исправимо перепроведением,
/// осиротело (документ удалён), либо тип без поддержки перепроведения.
async fn fixability_breakdown() -> anyhow::Result<Option<CheckBreakdown>> {
    use sea_orm::{ConnectionTrait, Statement};

    let conn = crate::shared::data::db::get_connection();

    let mut repostable = 0i64; // документ существует и тип проводится
    let mut orphaned = 0i64; // документ удалён → только очистка
    let mut non_postable = 0i64; // документ есть, но тип без перепроведения

    for (table, _) in PROJECTIONS {
        let sql = format!(
            r#"SELECT registrator_type, registrator_ref,
                      CAST(COUNT(*) AS INTEGER) AS cnt
               FROM {table}
               WHERE {VIOLATION_PREDICATE}
               GROUP BY registrator_type, registrator_ref"#
        );
        let rows = conn
            .query_all(Statement::from_string(
                sea_orm::DatabaseBackend::Sqlite,
                sql,
            ))
            .await?;

        for row in rows {
            let registrator_type: String = row.try_get("", "registrator_type").unwrap_or_default();
            let registrator_ref: String = row.try_get("", "registrator_ref").unwrap_or_default();
            let cnt: i64 = row.try_get("", "cnt").unwrap_or(0);

            let meta = registrators::meta(&registrator_type);
            let exists = registrators::source_document_exists(&registrator_type, &registrator_ref)
                .await
                .unwrap_or(false);

            if !exists {
                orphaned += cnt;
            } else if meta.can_post {
                repostable += cnt;
            } else {
                non_postable += cnt;
            }
        }
    }

    let mut rows = Vec::new();
    if repostable > 0 {
        rows.push(BreakdownRow {
            label: "Исправимо перепроведением".to_string(),
            population: repostable,
            violations: repostable,
        });
    }
    if orphaned > 0 {
        rows.push(BreakdownRow {
            label: "Документ удалён — только очистка".to_string(),
            population: orphaned,
            violations: orphaned,
        });
    }
    if non_postable > 0 {
        rows.push(BreakdownRow {
            label: "Документ есть, тип без перепроведения".to_string(),
            population: non_postable,
            violations: non_postable,
        });
    }

    if rows.is_empty() {
        return Ok(None);
    }

    Ok(Some(CheckBreakdown {
        key: "fixability".to_string(),
        title: "Исправимость нарушений".to_string(),
        dimension_label: "Способ устранения".to_string(),
        is_partition: true,
        rows,
    }))
}

// ─────────────────────────────────────────────────────────────────────────────
// Drill-down helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Возвращает список проекционных источников этой проверки.
pub fn list_sources() -> Vec<QualityCheckSource> {
    vec![
        QualityCheckSource {
            projection_table: "p909_mp_order_line_turnovers".to_string(),
            label: "p909 — Обороты строк заказов МП".to_string(),
        },
        QualityCheckSource {
            projection_table: "p911_wb_advert_by_items".to_string(),
            label: "p911 — Рекламные расходы WB по номенклатуре".to_string(),
        },
        QualityCheckSource {
            projection_table: "p913_wb_advert_order_attr".to_string(),
            label: "p913 — Атрибуция рекламных расходов по заказам WB".to_string(),
        },
    ]
}

/// Допустимые имена таблиц — защита от SQL-инъекций через параметр `projection_table`.
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

/// Допустимые поля сортировки для списка групп — защита от SQL-инъекций.
fn allowed_sort_field(field: &str) -> &str {
    match field {
        "missing_count" | "registrator_type" | "registrator_ref" | "min_entry_date"
        | "max_entry_date" => field,
        _ => "missing_count",
    }
}

/// Возвращает страницу групп регистраторов с пустым `nomenclature_ref`
/// для указанной проекционной таблицы.
pub async fn list_groups(
    projection_table: &str,
    page: i64,
    page_size: i64,
    sort_by: &str,
    sort_desc: bool,
) -> anyhow::Result<NipGroupsResponse> {
    use sea_orm::{ConnectionTrait, Statement};

    let table = allowed_table(projection_table)?;
    let sort_field = allowed_sort_field(sort_by);
    let sort_dir = if sort_desc { "DESC" } else { "ASC" };
    let offset = page * page_size;

    let conn = crate::shared::data::db::get_connection();

    // Общее количество уникальных групп
    let count_sql = format!(
        r#"SELECT COUNT(*) AS cnt
           FROM (
               SELECT registrator_type, registrator_ref
               FROM {table}
               WHERE (nomenclature_ref IS NULL OR TRIM(nomenclature_ref) = '')
               GROUP BY registrator_type, registrator_ref
           ) sub"#
    );
    let count_rows = conn
        .query_all(Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            count_sql,
        ))
        .await?;
    let total: i64 = count_rows
        .first()
        .and_then(|r| r.try_get::<i64>("", "cnt").ok())
        .unwrap_or(0);

    // Сами группы
    let data_sql = format!(
        r#"SELECT
               registrator_type,
               registrator_ref,
               CAST(COUNT(*) AS INTEGER) AS missing_count,
               MIN(entry_date) AS min_entry_date,
               MAX(entry_date) AS max_entry_date
           FROM {table}
           WHERE (nomenclature_ref IS NULL OR TRIM(nomenclature_ref) = '')
           GROUP BY registrator_type, registrator_ref
           ORDER BY {sort_field} {sort_dir}
           LIMIT {page_size} OFFSET {offset}"#
    );
    let rows = conn
        .query_all(Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            data_sql,
        ))
        .await?;

    let mut items: Vec<NipRegistratorGroup> = Vec::with_capacity(rows.len());
    for row in rows {
        let registrator_type: String = row.try_get("", "registrator_type").unwrap_or_default();
        let registrator_ref: String = row.try_get("", "registrator_ref").unwrap_or_default();
        let missing_count: i64 = row.try_get("", "missing_count").unwrap_or(0);
        let min_entry_date: Option<String> = row.try_get("", "min_entry_date").ok();
        let max_entry_date: Option<String> = row.try_get("", "max_entry_date").ok();

        let meta = registrators::meta(&registrator_type);
        let source_exists =
            registrators::source_document_exists(&registrator_type, &registrator_ref)
                .await
                .unwrap_or(false);
        let source_columns = if source_exists {
            registrators::source_columns(&registrator_type, &registrator_ref).await
        } else {
            Vec::new()
        };

        let display_short = if registrator_ref.len() > 8 {
            format!("{}…", &registrator_ref[..8])
        } else {
            registrator_ref.clone()
        };
        let registrator_type_label = if source_exists {
            meta.type_label.to_string()
        } else {
            format!("{} (документ не найден)", meta.type_label)
        };

        items.push(NipRegistratorGroup {
            projection_table: table.to_string(),
            registrator_type: registrator_type.clone(),
            registrator_ref,
            registrator_type_label,
            display_short,
            min_entry_date,
            max_entry_date,
            missing_count,
            can_post: meta.can_post && source_exists,
            can_cleanup: false,
            tab_key_prefix: source_exists
                .then(|| meta.tab_key_prefix.map(|s| s.to_string()))
                .flatten(),
            source_columns,
        });
    }

    Ok(NipGroupsResponse {
        items,
        total,
        page,
        page_size,
    })
}

/// Возвращает строки проекции с пустым `nomenclature_ref` для одного регистратора.
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
               WHERE (nomenclature_ref IS NULL OR TRIM(nomenclature_ref) = '')
                 AND registrator_ref = '{registrator_ref}'
               ORDER BY entry_date, id
               LIMIT 500"#
        ),
        "p911_wb_advert_by_items" => format!(
            r#"SELECT id, entry_date, turnover_code, amount, connection_mp_ref,
                      'Кампания' AS context_label, wb_advert_campaign_code AS context_value
               FROM p911_wb_advert_by_items
               WHERE (nomenclature_ref IS NULL OR TRIM(nomenclature_ref) = '')
                 AND registrator_ref = '{registrator_ref}'
               ORDER BY entry_date, id
               LIMIT 500"#
        ),
        "p913_wb_advert_order_attr" => format!(
            r#"SELECT id, entry_date, turnover_code, amount, connection_mp_ref,
                      'Заказ/Кампания' AS context_label,
                      (order_key || ' / ' || wb_advert_campaign_code) AS context_value
               FROM p913_wb_advert_order_attr
               WHERE (nomenclature_ref IS NULL OR TRIM(nomenclature_ref) = '')
                 AND registrator_ref = '{registrator_ref}'
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

    let result = rows
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
        .collect();

    Ok(result)
}

/// Массово перепроводит указанные документы-регистраторы.
///
/// Возвращает статистику и список ошибок.
pub async fn bulk_repost(
    registrator_type: &str,
    registrator_refs: &[String],
) -> anyhow::Result<NipRepostResult> {
    let meta = registrators::meta(registrator_type);
    if !meta.can_post {
        return Err(anyhow::anyhow!(
            "Тип регистратора '{}' не поддерживает перепроведение",
            registrator_type
        ));
    }

    let requested = registrator_refs.len();
    let mut reposted = 0usize;
    let mut errors = Vec::new();

    for reg_ref in registrator_refs {
        match registrators::repost_document(registrator_type, reg_ref).await {
            Ok(()) => reposted += 1,
            Err(e) => errors.push(format!("{}: {}", reg_ref, e)),
        }
    }

    Ok(NipRepostResult {
        requested,
        reposted,
        errors,
    })
}
