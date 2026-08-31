//! Admin-only одноразовая консолидация YM-подключений к модели «подключение = бизнес».
//!
//! Старая схема заводила отдельное подключение на каждый магазин (FBS/FBY/…), каждое
//! со своим `supplier_id`, но общим `business_account_id`. Новая модель: одно подключение
//! на бизнес (обычно «… FBS»), которое обходит все магазины. Эта команда переприязывает
//! исторические данные старых per-store подключений на выжившее (целевое) подключение
//! бизнеса, сохраняя `campaign_id`/`partner_id` в самих записях.
//!
//! Затрагивает источники: a013_ym_order (`connection_id` + header_json), a016_ym_returns
//! (`connection_mp_ref` + header_json), p907_ym_payment_report (`connection_mp_ref`).
//! После переприязки p907 пересобирается a035 (группировка идёт по connection_mp_ref).
//!
//! ВНИМАНИЕ: проекции продаж (p900/p904/p915) формируются при проведении и здесь НЕ
//! обновляются — после консолидации заказы нужно перепровести отдельно.

use axum::{extract::Query, http::StatusCode, Json};
use sea_orm::{ConnectionTrait, Statement, Value};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use uuid::Uuid;

use contracts::domain::common::AggregateId;
use contracts::enums::marketplace_type::MarketplaceType;

use crate::shared::data::db::get_connection;

#[derive(Deserialize)]
pub struct ConsolidateParams {
    /// По умолчанию true — только отчёт, без изменений. Применение: `?dry_run=false`.
    #[serde(default = "default_true")]
    dry_run: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Serialize)]
pub struct GroupReport {
    business_account_id: String,
    target_id: String,
    target_description: String,
    source_ids: Vec<String>,
    a013_rows: u64,
    a016_rows: u64,
    p907_rows: u64,
}

#[derive(Serialize)]
pub struct ConsolidateReport {
    dry_run: bool,
    groups: Vec<GroupReport>,
    total_a013: u64,
    total_a016: u64,
    total_p907: u64,
    a035_created: Option<usize>,
    a035_updated: Option<usize>,
    note: String,
}

struct ConnInfo {
    id: String,
    description: String,
    is_used: bool,
}

impl ConnInfo {
    fn desc_fbs(&self) -> bool {
        self.description.to_uppercase().contains("FBS")
    }
}

fn sv(s: impl Into<String>) -> Value {
    Value::String(Some(Box::new(s.into())))
}

async fn count_rows(sql: &str, source_id: &str) -> anyhow::Result<u64> {
    let db = get_connection();
    let stmt = Statement::from_sql_and_values(db.get_database_backend(), sql, vec![sv(source_id)]);
    let row = db.query_one(stmt).await?;
    Ok(row
        .and_then(|r| r.try_get::<i64>("", "cnt").ok())
        .unwrap_or(0)
        .max(0) as u64)
}

async fn exec_update(sql: &str, values: Vec<Value>) -> anyhow::Result<u64> {
    let db = get_connection();
    let stmt = Statement::from_sql_and_values(db.get_database_backend(), sql, values);
    let res = db.execute(stmt).await?;
    Ok(res.rows_affected())
}

const COUNT_A013: &str = "SELECT COUNT(*) AS cnt FROM a013_ym_order WHERE connection_id = ?";
const COUNT_A016: &str =
    "SELECT COUNT(*) AS cnt FROM a016_ym_returns WHERE json_extract(header_json, '$.connection_id') = ?";
const COUNT_P907: &str =
    "SELECT COUNT(*) AS cnt FROM p907_ym_payment_report WHERE connection_mp_ref = ?";

/// Выбрать целевое (выжившее) подключение в группе бизнеса.
/// Приоритет: используемое + «FBS» в названии → «FBS» → используемое → первое.
fn pick_target(group: &[ConnInfo]) -> usize {
    group
        .iter()
        .position(|c| c.is_used && c.desc_fbs())
        .or_else(|| group.iter().position(|c| c.desc_fbs()))
        .or_else(|| group.iter().position(|c| c.is_used))
        .unwrap_or(0)
}

async fn run(dry_run: bool) -> anyhow::Result<ConsolidateReport> {
    // 1. Загрузить подключения и отобрать YM с заданным business_account_id.
    let connections = super::service::list_all().await?;

    let mut mp_type_cache: HashMap<String, Option<MarketplaceType>> = HashMap::new();
    let mut groups: HashMap<String, Vec<ConnInfo>> = HashMap::new();

    for conn in &connections {
        let biz = match conn
            .business_account_id
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            Some(s) => s.to_string(),
            None => continue,
        };

        // Тип маркетплейса (кэш по marketplace_id).
        let mp_type = match mp_type_cache.get(&conn.marketplace_id) {
            Some(t) => t.clone(),
            None => {
                let t = match Uuid::parse_str(&conn.marketplace_id) {
                    Ok(uuid) => crate::domain::a005_marketplace::service::get_by_id(uuid)
                        .await
                        .ok()
                        .flatten()
                        .and_then(|m| m.marketplace_type),
                    Err(_) => None,
                };
                mp_type_cache.insert(conn.marketplace_id.clone(), t.clone());
                t
            }
        };
        if mp_type != Some(MarketplaceType::YandexMarket) {
            continue;
        }

        groups.entry(biz).or_default().push(ConnInfo {
            id: conn.base.id.as_string(),
            description: conn.base.description.clone(),
            is_used: conn.is_used,
        });
    }

    // 2. Для каждой группы из >1 подключения построить план переприязки.
    let mut reports: Vec<GroupReport> = Vec::new();
    let (mut total_a013, mut total_a016, mut total_p907) = (0u64, 0u64, 0u64);

    for (biz, group) in &groups {
        if group.len() < 2 {
            continue; // один магазин-подключение — консолидировать нечего
        }
        let target_idx = pick_target(group);
        let target = &group[target_idx];

        let mut grp = GroupReport {
            business_account_id: biz.clone(),
            target_id: target.id.clone(),
            target_description: target.description.clone(),
            source_ids: Vec::new(),
            a013_rows: 0,
            a016_rows: 0,
            p907_rows: 0,
        };

        for (idx, src) in group.iter().enumerate() {
            if idx == target_idx {
                continue;
            }
            grp.source_ids.push(src.id.clone());
            let c13 = count_rows(COUNT_A013, &src.id).await?;
            let c16 = count_rows(COUNT_A016, &src.id).await?;
            let c907 = count_rows(COUNT_P907, &src.id).await?;
            grp.a013_rows += c13;
            grp.a016_rows += c16;
            grp.p907_rows += c907;

            if !dry_run {
                exec_update(
                    "UPDATE a013_ym_order \
                     SET connection_id = ?, header_json = json_set(header_json, '$.connection_id', ?) \
                     WHERE connection_id = ?",
                    vec![sv(&target.id), sv(&target.id), sv(&src.id)],
                )
                .await?;
                exec_update(
                    "UPDATE a016_ym_returns \
                     SET connection_mp_ref = ?, header_json = json_set(header_json, '$.connection_id', ?) \
                     WHERE json_extract(header_json, '$.connection_id') = ?",
                    vec![sv(&target.id), sv(&target.id), sv(&src.id)],
                )
                .await?;
                exec_update(
                    "UPDATE p907_ym_payment_report SET connection_mp_ref = ? WHERE connection_mp_ref = ?",
                    vec![sv(&target.id), sv(&src.id)],
                )
                .await?;
            }
        }

        total_a013 += grp.a013_rows;
        total_a016 += grp.a016_rows;
        total_p907 += grp.p907_rows;
        reports.push(grp);
    }

    // 3. После переприязки p907 пересобрать a035 (группировка по connection_mp_ref).
    let (a035_created, a035_updated) = if dry_run {
        (None, None)
    } else {
        let res = crate::domain::a035_ym_settlement_recon::service::generate("", "").await?;
        (Some(res.created), Some(res.updated))
    };

    Ok(ConsolidateReport {
        dry_run,
        groups: reports,
        total_a013,
        total_a016,
        total_p907,
        a035_created,
        a035_updated,
        note: "Проекции продаж (p900/p904/p915) формируются при проведении и не обновлены \
               этой командой — после консолидации перепроведите затронутые заказы (u508/репост)."
            .to_string(),
    })
}

/// POST /api/ym/consolidate-connections?dry_run=true|false (admin-only).
pub async fn consolidate_ym_connections(
    Query(params): Query<ConsolidateParams>,
) -> Result<Json<ConsolidateReport>, (StatusCode, Json<serde_json::Value>)> {
    match run(params.dry_run).await {
        Ok(report) => Ok(Json(report)),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )),
    }
}
