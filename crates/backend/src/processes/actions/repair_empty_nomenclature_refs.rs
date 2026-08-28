//! Действие `repair_empty_nomenclature_refs` — перепровести документы с пустым
//! `nomenclature_ref` в проекциях (обёртка над QC nomenclature_in_projections).

use anyhow::Result;
use async_trait::async_trait;
use contracts::processes::{ActionActor, ActionInfo};
use contracts::quality::NipRepostResult;
use sea_orm::{ConnectionTrait, DatabaseConnection, Statement};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::OnceLock;

use super::Action;
use crate::quality::checks::nomenclature_in_projections;

#[derive(Debug, Deserialize)]
struct Input {
    /// Если задано — только эта проекция; иначе все три из QC.
    #[serde(default)]
    projection_table: Option<String>,
    /// Потолок групп регистраторов за один вызов (защита от бесконечного репоста).
    #[serde(default = "default_limit")]
    max_groups: u32,
}

fn default_limit() -> u32 {
    200
}

const PROJECTIONS: &[&str] = &[
    "p909_mp_order_line_turnovers",
    "p911_wb_advert_by_items",
    "p913_wb_advert_order_attr",
];

pub struct RepairEmptyNomenclatureRefs;

fn info() -> &'static ActionInfo {
    static INFO: OnceLock<ActionInfo> = OnceLock::new();
    INFO.get_or_init(|| ActionInfo {
        name: "repair_empty_nomenclature_refs",
        method: "repairEmptyNomenclatureRefs",
        title: "Починить пустые nomenclature_ref в проекциях",
        description: "Находит регистраторы со строками без nomenclature_ref в p909/p911/p913 \
                      и точечно перепроводит их (тот же путь, что bulk_repost QC).",
        capability: "action:repair_empty_nomenclature_refs",
        reversible: false,
        write_tables: &[
            "sys_general_ledger",
            "p909_mp_order_line_turnovers",
            "p911_wb_advert_by_items",
            "p913_wb_advert_order_attr",
        ],
        input_schema: json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "type": "object",
            "additionalProperties": false,
            "properties": {
                "projection_table": {
                    "type": ["string", "null"],
                    "description": "Одна из p909/p911/p913; пусто — все"
                },
                "max_groups": {
                    "type": "integer",
                    "minimum": 1,
                    "maximum": 2000,
                    "default": 200
                }
            }
        }),
    })
}

#[async_trait]
impl Action for RepairEmptyNomenclatureRefs {
    fn info(&self) -> &ActionInfo {
        info()
    }

    async fn execute(
        &self,
        db: &DatabaseConnection,
        input: &Value,
        _actor: &ActionActor,
    ) -> Result<Value> {
        let input: Input = serde_json::from_value(input.clone())?;
        let tables: Vec<&str> = match input.projection_table.as_deref() {
            Some(table) => {
                if !PROJECTIONS.contains(&table) {
                    anyhow::bail!("неизвестная проекция '{table}': ожидается одна из {PROJECTIONS:?}");
                }
                vec![table]
            }
            None => PROJECTIONS.to_vec(),
        };

        let mut requested = 0usize;
        let mut reposted = 0usize;
        let mut errors: Vec<String> = Vec::new();
        let mut groups_seen = 0u32;

        for table in tables {
            if groups_seen >= input.max_groups {
                break;
            }
            let remaining = (input.max_groups - groups_seen) as i64;
            let groups = list_missing_groups(db, table, remaining).await?;
            for (registrator_type, registrator_ref) in groups {
                groups_seen += 1;
                let result: NipRepostResult = nomenclature_in_projections::bulk_repost(
                    &registrator_type,
                    &[registrator_ref.clone()],
                )
                .await
                .unwrap_or_else(|e| NipRepostResult {
                    requested: 1,
                    reposted: 0,
                    errors: vec![format!("{registrator_type}/{registrator_ref}: {e}")],
                });
                requested += result.requested;
                reposted += result.reposted;
                errors.extend(result.errors);
            }
        }

        Ok(json!({
            "groups": groups_seen,
            "requested": requested,
            "reposted": reposted,
            "errors": errors,
            "capped": groups_seen >= input.max_groups,
        }))
    }

    async fn plan(
        &self,
        db: &DatabaseConnection,
        input: &Value,
        _actor: &ActionActor,
    ) -> Result<Value> {
        let input: Input = serde_json::from_value(input.clone())?;
        let tables: Vec<&str> = match input.projection_table.as_deref() {
            Some(table) => vec![table],
            None => PROJECTIONS.to_vec(),
        };
        let mut would_touch = 0i64;
        for table in tables {
            if !PROJECTIONS.contains(&table) {
                continue;
            }
            would_touch += count_missing_groups(db, table).await.unwrap_or(0);
        }
        Ok(json!({
            "effect": "точечное перепроведение регистраторов с пустым nomenclature_ref",
            "projection_table": input.projection_table,
            "max_groups": input.max_groups,
            "groups_with_missing_refs": would_touch,
            "reversible": false,
        }))
    }
}

async fn list_missing_groups(
    db: &DatabaseConnection,
    table: &str,
    limit: i64,
) -> Result<Vec<(String, String)>> {
    let sql = format!(
        r#"SELECT registrator_type, registrator_ref
           FROM {table}
           WHERE (nomenclature_ref IS NULL OR TRIM(nomenclature_ref) = '')
             AND registrator_type IS NOT NULL
             AND registrator_ref IS NOT NULL
             AND TRIM(registrator_ref) != ''
           GROUP BY registrator_type, registrator_ref
           ORDER BY MIN(entry_date) ASC
           LIMIT {limit}"#
    );
    let rows = db
        .query_all(Statement::from_string(sea_orm::DatabaseBackend::Sqlite, sql))
        .await?;
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let reg_type: String = row.try_get("", "registrator_type")?;
        let reg_ref: String = row.try_get("", "registrator_ref")?;
        out.push((reg_type, reg_ref));
    }
    Ok(out)
}

async fn count_missing_groups(db: &DatabaseConnection, table: &str) -> Result<i64> {
    let sql = format!(
        r#"SELECT COUNT(*) AS cnt FROM (
               SELECT 1
               FROM {table}
               WHERE (nomenclature_ref IS NULL OR TRIM(nomenclature_ref) = '')
                 AND registrator_type IS NOT NULL
                 AND registrator_ref IS NOT NULL
               GROUP BY registrator_type, registrator_ref
           )"#
    );
    let rows = db
        .query_all(Statement::from_string(sea_orm::DatabaseBackend::Sqlite, sql))
        .await?;
    Ok(rows
        .first()
        .and_then(|r| r.try_get::<i64>("", "cnt").ok())
        .unwrap_or(0))
}
