//! Safe two-phase repair workflow for `p916_mp_sales_funnel_turnovers`.

use anyhow::{anyhow, Context, Result};
use chrono::{NaiveDate, Utc};
use contracts::domain::common::AggregateId;
use contracts::projections::p916_mp_sales_funnel_turnovers::dto::FunnelRebuildRequest;
use contracts::quality::CheckDetails;
use contracts::usecases::{
    u503_import_from_yandex as ym_contracts, u504_import_from_wildberries as wb_contracts,
};
use sea_orm::{ConnectionTrait, DatabaseBackend, Statement};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, sync::Arc};
use uuid::Uuid;

pub const TARGET: &str = "p916_mp_sales_funnel_turnovers";
const WB_CODE: &str = "mp-wb";
const YM_CODE: &str = "mp-ym";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RepairAction {
    pub kind: String,
    pub marketplace: String,
    pub connection_mp_ref: String,
    pub sources: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RepairLimitation {
    pub category: String,
    pub connection_mp_ref: Option<String>,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RepairSpec {
    pub target: String,
    pub date_from: String,
    pub date_to: String,
    pub connection_mp_refs: Vec<String>,
    pub actions: Vec<RepairAction>,
    pub limitations: Vec<RepairLimitation>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PreparedRepair {
    pub repair_spec: RepairSpec,
    pub payload_hash: String,
    pub classification: String,
    pub preview_text: String,
    pub checks: Vec<CheckDetails>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepairRunView {
    pub repair_run_id: String,
    pub status: String,
    pub phase: String,
    pub payload_hash: String,
    pub repair_spec: RepairSpec,
    pub precheck: Option<Value>,
    pub postcheck: Option<Value>,
    pub session_ids: Vec<String>,
    pub errors: Vec<String>,
    pub created_at: String,
    pub updated_at: String,
    pub completed_at: Option<String>,
}

#[derive(Clone)]
struct ScopedConnection {
    aggregate: contracts::domain::a006_connection_mp::aggregate::ConnectionMP,
    marketplace_code: String,
}

pub fn spec_hash(spec: &RepairSpec) -> Result<String> {
    let bytes = serde_json::to_vec(spec)?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

pub async fn prepare(
    target: &str,
    date_from: &str,
    date_to: &str,
    connection_mp_refs: Vec<String>,
) -> Result<PreparedRepair> {
    validate_scope(target, date_from, date_to)?;
    let connections = resolve_connections(connection_mp_refs).await?;
    if connections.is_empty() {
        return Err(anyhow!(
            "Не найдено активных подключений WB или Яндекс.Маркета"
        ));
    }

    let refs: Vec<String> = connections
        .iter()
        .map(|item| item.aggregate.base.id.as_string())
        .collect();
    let checks = run_checks(date_from, date_to, &connections).await?;
    let violation_types: BTreeSet<String> = checks
        .iter()
        .flat_map(|check| check.result.violations.iter())
        .map(|item| item.violation_type.clone())
        .collect();

    let mut actions = Vec::new();
    let mut limitations = Vec::new();
    let oldest_ym_date = Utc::now().date_naive() - chrono::Duration::days(400);
    let today = Utc::now().date_naive();
    let requested_from = NaiveDate::parse_from_str(date_from, "%Y-%m-%d")?;
    let requested_to = NaiveDate::parse_from_str(date_to, "%Y-%m-%d")?;

    if requested_to > today {
        limitations.push(RepairLimitation {
            category: "impossible".into(),
            connection_mp_ref: None,
            detail: format!(
                "Даты после {} ещё не наступили и не могут содержать подтверждённые данные",
                today
            ),
        });
    }

    for item in &connections {
        let id = item.aggregate.base.id.as_string();
        if !item.aggregate.is_used || item.aggregate.api_key.trim().is_empty() {
            limitations.push(RepairLimitation {
                category: "impossible".into(),
                connection_mp_ref: Some(id),
                detail: "Подключение неактивно или не содержит API-ключ; загрузка невозможна"
                    .into(),
            });
            continue;
        }

        match item.marketplace_code.as_str() {
            WB_CODE => {
                let mut sources = vec![
                    "a015_wb_orders".into(),
                    "a012_wb_sales".into(),
                    "wb_advert_stats".into(),
                ];
                if requested_from < today {
                    sources.push("a036_wb_sales_funnel_daily_history".into());
                }
                if requested_from <= today && requested_to >= today {
                    sources.push("a036_wb_sales_funnel_daily".into());
                }
                actions.push(RepairAction {
                    kind: "reload_sources".into(),
                    marketplace: "wb".into(),
                    connection_mp_ref: id,
                    sources,
                });
            }
            YM_CODE => {
                let mut sources = vec!["a013_ym_order".into(), "a016_ym_returns".into()];
                if requested_to >= oldest_ym_date {
                    sources.push("a041_ym_shows_sales_daily".into());
                }
                if requested_from < oldest_ym_date {
                    limitations.push(RepairLimitation {
                        category: "impossible".into(),
                        connection_mp_ref: Some(id.clone()),
                        detail: format!(
                            "Показы/клики YM до {} старше максимального окна 400 дней и не восстанавливаются",
                            oldest_ym_date
                        ),
                    });
                }
                actions.push(RepairAction {
                    kind: "reload_sources".into(),
                    marketplace: "ym".into(),
                    connection_mp_ref: id,
                    sources,
                });
            }
            _ => {}
        }
    }
    if !actions.is_empty() {
        actions.push(RepairAction {
            kind: "rebuild_projection".into(),
            marketplace: "all".into(),
            connection_mp_ref: "*".into(),
            sources: vec![TARGET.into()],
        });
    }
    if violation_types.contains("projection_extra") {
        limitations.push(RepairLimitation {
            category: "manual_review".into(),
            connection_mp_ref: None,
            detail: "Лишние строки p916 автоматически не удаляются в v1".into(),
        });
    }
    if violation_types.contains("cancel_date_fallback") {
        limitations.push(RepairLimitation {
            category: "impossible".into(),
            connection_mp_ref: None,
            detail: "Дата части старых отмен отсутствует в источнике; перепроведение не восстановит event_date".into(),
        });
    }

    actions.sort_by(|a, b| {
        (&a.kind, &a.marketplace, &a.connection_mp_ref).cmp(&(
            &b.kind,
            &b.marketplace,
            &b.connection_mp_ref,
        ))
    });
    limitations.sort_by(|a, b| {
        (&a.category, &a.connection_mp_ref, &a.detail).cmp(&(
            &b.category,
            &b.connection_mp_ref,
            &b.detail,
        ))
    });
    let spec = RepairSpec {
        target: TARGET.into(),
        date_from: date_from.into(),
        date_to: date_to.into(),
        connection_mp_refs: refs,
        actions,
        limitations,
    };
    let payload_hash = spec_hash(&spec)?;
    let has_errors = checks.iter().any(|check| check.result.violations_total > 0);
    let source_is_missing = violation_types.contains("source_missing");
    let classification = if spec.actions.is_empty() {
        "impossible"
    } else if spec
        .limitations
        .iter()
        .any(|item| item.category == "manual_review")
    {
        "manual_review"
    } else if source_is_missing {
        "reloadable"
    } else if has_errors {
        "rebuildable"
    } else {
        "healthy"
    };
    let preview_text = format!(
        "Проверка и исправление {} за {}..{}; кабинетов: {}; действий: {}; ограничений: {}. После загрузки источников будет выполнен u508 и повторный QC.",
        TARGET,
        date_from,
        date_to,
        spec.connection_mp_refs.len(),
        spec.actions.len(),
        spec.limitations.len()
    );
    Ok(PreparedRepair {
        repair_spec: spec,
        payload_hash,
        classification: classification.into(),
        preview_text,
        checks,
    })
}

pub async fn start(
    spec: RepairSpec,
    payload_hash: &str,
    chat_id: &str,
    agent_id: &str,
    requested_by_user_id: Option<&str>,
) -> Result<RepairRunView> {
    let database_activity = crate::system::maintenance::try_begin_database_activity()
        .ok_or_else(|| anyhow!("Исправление недоступно во время обслуживания базы данных"))?;
    validate_scope(&spec.target, &spec.date_from, &spec.date_to)?;
    let actual_hash = spec_hash(&spec)?;
    if actual_hash != payload_hash {
        return Err(anyhow!(
            "План изменён после подтверждения: payload_hash не совпадает"
        ));
    }
    if let Some(existing) = find_by_chat_hash(chat_id, payload_hash).await? {
        return Ok(existing);
    }

    let id = Uuid::new_v4().to_string();
    insert_run(
        &id,
        chat_id,
        agent_id,
        requested_by_user_id,
        payload_hash,
        &spec,
    )
    .await?;
    let run_id = id.clone();
    tokio::spawn(async move {
        let _database_activity = database_activity;
        if let Err(error) = execute_run(&run_id, &spec).await {
            let _ = finish_failed(&run_id, &error.to_string()).await;
        }
    });
    get(&id)
        .await?
        .ok_or_else(|| anyhow!("Созданный repair-run не найден"))
}

pub async fn get(id: &str) -> Result<Option<RepairRunView>> {
    let db = crate::shared::data::db::get_connection();
    let row = db.query_one(Statement::from_sql_and_values(
        DatabaseBackend::Sqlite,
        "SELECT id,status,phase,payload_hash,payload_json,precheck_json,postcheck_json,session_ids_json,errors_json,created_at,updated_at,completed_at FROM sys_p916_repair_run WHERE id=?",
        [id.into()],
    )).await?;
    row.map(row_to_view).transpose()
}

pub async fn get_for_chat(id: &str, chat_id: &str) -> Result<Option<RepairRunView>> {
    let db = crate::shared::data::db::get_connection();
    let row = db.query_one(Statement::from_sql_and_values(
        DatabaseBackend::Sqlite,
        "SELECT id,status,phase,payload_hash,payload_json,precheck_json,postcheck_json,session_ids_json,errors_json,created_at,updated_at,completed_at FROM sys_p916_repair_run WHERE id=? AND chat_id=?",
        [id.into(), chat_id.into()],
    )).await?;
    row.map(row_to_view).transpose()
}

async fn find_by_chat_hash(chat_id: &str, payload_hash: &str) -> Result<Option<RepairRunView>> {
    let db = crate::shared::data::db::get_connection();
    let row = db.query_one(Statement::from_sql_and_values(
        DatabaseBackend::Sqlite,
        "SELECT id,status,phase,payload_hash,payload_json,precheck_json,postcheck_json,session_ids_json,errors_json,created_at,updated_at,completed_at FROM sys_p916_repair_run WHERE chat_id=? AND payload_hash=?",
        [chat_id.into(), payload_hash.into()],
    )).await?;
    row.map(row_to_view).transpose()
}

fn row_to_view(row: sea_orm::QueryResult) -> Result<RepairRunView> {
    let text = |name: &str| row.try_get::<String>("", name).map_err(anyhow::Error::from);
    let optional = |name: &str| {
        row.try_get::<Option<String>>("", name)
            .map_err(anyhow::Error::from)
    };
    Ok(RepairRunView {
        repair_run_id: text("id")?,
        status: text("status")?,
        phase: text("phase")?,
        payload_hash: text("payload_hash")?,
        repair_spec: serde_json::from_str(&text("payload_json")?)?,
        precheck: optional("precheck_json")?
            .map(|v| serde_json::from_str(&v))
            .transpose()?,
        postcheck: optional("postcheck_json")?
            .map(|v| serde_json::from_str(&v))
            .transpose()?,
        session_ids: serde_json::from_str(&text("session_ids_json")?)?,
        errors: serde_json::from_str(&text("errors_json")?)?,
        created_at: text("created_at")?,
        updated_at: text("updated_at")?,
        completed_at: optional("completed_at")?,
    })
}

async fn execute_run(id: &str, spec: &RepairSpec) -> Result<()> {
    update_phase(id, "precheck").await?;
    let connections = resolve_connections(spec.connection_mp_refs.clone()).await?;
    let precheck = run_checks(&spec.date_from, &spec.date_to, &connections).await?;
    update_json(id, "precheck_json", &serde_json::to_value(&precheck)?).await?;

    let date_from = NaiveDate::parse_from_str(&spec.date_from, "%Y-%m-%d")?;
    let date_to = NaiveDate::parse_from_str(&spec.date_to, "%Y-%m-%d")?;
    let mut errors = Vec::new();
    let mut sessions = Vec::new();
    let mut successful_imports = 0usize;
    let mut attempted_imports = 0usize;

    update_phase(id, "reload_sources").await?;
    for action in spec
        .actions
        .iter()
        .filter(|action| action.kind == "reload_sources")
    {
        let Some(connection) = connections
            .iter()
            .find(|item| item.aggregate.base.id.as_string() == action.connection_mp_ref)
        else {
            errors.push(format!(
                "Подключение {} исчезло после подготовки плана",
                action.connection_mp_ref
            ));
            continue;
        };
        // Изолировать источники: ошибка одного API не блокирует остальные.
        for source in &action.sources {
            for (source_from, source_to) in source_windows(source, date_from, date_to) {
                attempted_imports += 1;
                let session_id = Uuid::new_v4().to_string();
                sessions.push(session_id.clone());
                let result = if action.marketplace == "wb" {
                    let tracker = Arc::new(
                        crate::usecases::u504_import_from_wildberries::ProgressTracker::new(),
                    );
                    let executor =
                        crate::usecases::u504_import_from_wildberries::ImportExecutor::new(tracker);
                    let request = wb_contracts::request::ImportRequest {
                        connection_id: action.connection_mp_ref.clone(),
                        target_aggregates: vec![source.clone()],
                        date_from: source_from,
                        date_to: source_to,
                        mode: wb_contracts::request::ImportMode::Background,
                    };
                    executor
                        .execute_import(&session_id, &request, &connection.aggregate)
                        .await
                        .map(|_| ())
                } else {
                    let tracker =
                        Arc::new(crate::usecases::u503_import_from_yandex::ProgressTracker::new());
                    let executor =
                        crate::usecases::u503_import_from_yandex::ImportExecutor::new(tracker);
                    let request = ym_contracts::request::ImportRequest {
                        connection_id: action.connection_mp_ref.clone(),
                        target_aggregates: vec![source.clone()],
                        mode: ym_contracts::request::ImportMode::Background,
                        date_from: source_from,
                        date_to: source_to,
                        incremental_by_update: false,
                    };
                    executor
                        .execute_import(&session_id, &request, &connection.aggregate)
                        .await
                };
                if let Err(error) = result {
                    errors.push(format!(
                        "{} {} {} {}..{}: {:#}",
                        action.marketplace,
                        action.connection_mp_ref,
                        source,
                        source_from,
                        source_to,
                        error
                    ));
                } else {
                    successful_imports += 1;
                }
                update_arrays(id, &sessions, &errors).await?;
            }
        }
    }

    update_phase(id, "rebuild_projection").await?;
    let rebuild_session = Uuid::new_v4().to_string();
    sessions.push(rebuild_session.clone());
    let tracker = Arc::new(crate::usecases::u508_repost_documents::ProgressTracker::new());
    tracker.create_session(rebuild_session.clone());
    let executor = crate::usecases::u508_repost_documents::RepostExecutor::new(tracker);
    if let Err(error) = executor
        .run_funnel_rebuild(
            &rebuild_session,
            &FunnelRebuildRequest {
                date_from: spec.date_from.clone(),
                date_to: spec.date_to.clone(),
                connection_mp_refs: spec.connection_mp_refs.clone(),
            },
        )
        .await
    {
        errors.push(format!("u508: {:#}", error));
    }
    update_arrays(id, &sessions, &errors).await?;

    update_phase(id, "postcheck").await?;
    let postcheck = run_checks(&spec.date_from, &spec.date_to, &connections).await?;
    update_json(id, "postcheck_json", &serde_json::to_value(&postcheck)?).await?;
    let remaining = postcheck
        .iter()
        .map(|check| check.result.violations_total)
        .sum::<i64>();
    let status = if remaining > 0 && attempted_imports > 0 && successful_imports == 0 {
        "failed"
    } else if errors.is_empty() && remaining == 0 && spec.limitations.is_empty() {
        "completed"
    } else {
        "completed_with_limitations"
    };
    finish(id, status, &errors).await
}

fn source_windows(
    source: &str,
    date_from: NaiveDate,
    date_to: NaiveDate,
) -> Vec<(NaiveDate, NaiveDate)> {
    let today = Utc::now().date_naive();
    if source == "a036_wb_sales_funnel_daily" {
        return (date_from <= today && date_to >= today)
            .then_some((today, today))
            .into_iter()
            .collect();
    }

    let effective_to = date_to.min(if source == "a036_wb_sales_funnel_daily_history" {
        today - chrono::Duration::days(1)
    } else {
        today
    });
    if date_from > effective_to {
        return Vec::new();
    }
    if source != "a036_wb_sales_funnel_daily_history" {
        return vec![(date_from, effective_to)];
    }

    let mut windows = Vec::new();
    let mut start = date_from;
    while start <= effective_to {
        // DETAIL_HISTORY_REPORT accepts a range whose endpoint difference is < 365 days.
        let end = (start + chrono::Duration::days(364)).min(effective_to);
        windows.push((start, end));
        start = end + chrono::Duration::days(1);
    }
    windows
}

async fn resolve_connections(requested: Vec<String>) -> Result<Vec<ScopedConnection>> {
    let selected: BTreeSet<String> = requested
        .into_iter()
        .filter(|id| !id.trim().is_empty())
        .collect();
    let mut result = Vec::new();
    for connection in crate::domain::a006_connection_mp::service::list_all().await? {
        let id = connection.base.id.as_string();
        if !selected.is_empty() && !selected.contains(&id) {
            continue;
        }
        let marketplace_id = Uuid::parse_str(&connection.marketplace_id)
            .with_context(|| format!("Некорректный marketplace id у подключения {id}"))?;
        let Some(marketplace) =
            crate::domain::a005_marketplace::service::get_by_id(marketplace_id).await?
        else {
            continue;
        };
        if matches!(marketplace.base.code.as_str(), WB_CODE | YM_CODE) {
            result.push(ScopedConnection {
                aggregate: connection,
                marketplace_code: marketplace.base.code,
            });
        }
    }
    result.sort_by_key(|item| item.aggregate.base.id.as_string());
    if !selected.is_empty() && result.len() != selected.len() {
        let found: BTreeSet<String> = result
            .iter()
            .map(|item| item.aggregate.base.id.as_string())
            .collect();
        let missing = selected.difference(&found).cloned().collect::<Vec<_>>();
        return Err(anyhow!(
            "Не найдены или не поддерживаются подключения: {}",
            missing.join(", ")
        ));
    }
    Ok(result)
}

async fn run_checks(
    date_from: &str,
    date_to: &str,
    connections: &[ScopedConnection],
) -> Result<Vec<CheckDetails>> {
    let wb_refs = connections
        .iter()
        .filter(|item| item.marketplace_code == WB_CODE)
        .map(|item| item.aggregate.base.id.as_string())
        .collect::<Vec<_>>();
    let ym_refs = connections
        .iter()
        .filter(|item| item.marketplace_code == YM_CODE)
        .map(|item| item.aggregate.base.id.as_string())
        .collect::<Vec<_>>();
    let input = |refs: Vec<String>| json!({"date_from":date_from,"date_to":date_to,"connection_mp_refs":refs});
    let mut checks = Vec::new();
    if !wb_refs.is_empty() {
        checks.push(
            crate::quality::run_check_with_input(
                "wb_funnel_projection_coverage",
                input(wb_refs.clone()),
                "repair",
            )
            .await?,
        );
        checks.push(
            crate::quality::run_check_with_input(
                "wb_marketing_projection_coverage",
                input(wb_refs),
                "repair",
            )
            .await?,
        );
    }
    if !ym_refs.is_empty() {
        checks.push(
            crate::quality::run_check_with_input(
                "ym_funnel_projection_coverage",
                input(ym_refs),
                "repair",
            )
            .await?,
        );
    }
    Ok(checks)
}

fn validate_scope(target: &str, date_from: &str, date_to: &str) -> Result<()> {
    if target != TARGET {
        return Err(anyhow!("В v1 поддерживается только {TARGET}"));
    }
    let from =
        NaiveDate::parse_from_str(date_from, "%Y-%m-%d").context("Некорректный date_from")?;
    let to = NaiveDate::parse_from_str(date_to, "%Y-%m-%d").context("Некорректный date_to")?;
    if from > to {
        return Err(anyhow!("date_from должен быть не позже date_to"));
    }
    Ok(())
}

async fn insert_run(
    id: &str,
    chat_id: &str,
    agent_id: &str,
    user_id: Option<&str>,
    hash: &str,
    spec: &RepairSpec,
) -> Result<()> {
    let now = Utc::now().to_rfc3339();
    crate::shared::data::db::get_connection().execute(Statement::from_sql_and_values(
        DatabaseBackend::Sqlite,
        "INSERT INTO sys_p916_repair_run(id,chat_id,agent_id,requested_by_user_id,payload_hash,payload_json,status,phase,created_at,updated_at) VALUES(?,?,?,?,?,?,'running','queued',?,?)",
        vec![id.into(),chat_id.into(),agent_id.into(),user_id.map(str::to_string).into(),hash.into(),serde_json::to_string(spec)?.into(),now.clone().into(),now.into()],
    )).await?;
    Ok(())
}

async fn update_phase(id: &str, phase: &str) -> Result<()> {
    let now = Utc::now().to_rfc3339();
    crate::shared::data::db::get_connection()
        .execute(Statement::from_sql_and_values(
            DatabaseBackend::Sqlite,
            "UPDATE sys_p916_repair_run SET phase=?,updated_at=? WHERE id=?",
            [phase.into(), now.into(), id.into()],
        ))
        .await?;
    Ok(())
}

async fn update_json(id: &str, column: &str, value: &Value) -> Result<()> {
    if !matches!(column, "precheck_json" | "postcheck_json") {
        return Err(anyhow!("Недопустимая колонка repair-run"));
    }
    let sql = format!("UPDATE sys_p916_repair_run SET {column}=?,updated_at=? WHERE id=?");
    crate::shared::data::db::get_connection()
        .execute(Statement::from_sql_and_values(
            DatabaseBackend::Sqlite,
            &sql,
            [
                serde_json::to_string(value)?.into(),
                Utc::now().to_rfc3339().into(),
                id.into(),
            ],
        ))
        .await?;
    Ok(())
}

async fn update_arrays(id: &str, sessions: &[String], errors: &[String]) -> Result<()> {
    crate::shared::data::db::get_connection().execute(Statement::from_sql_and_values(DatabaseBackend::Sqlite,"UPDATE sys_p916_repair_run SET session_ids_json=?,errors_json=?,updated_at=? WHERE id=?",[serde_json::to_string(sessions)?.into(),serde_json::to_string(errors)?.into(),Utc::now().to_rfc3339().into(),id.into()])).await?;
    Ok(())
}

async fn finish(id: &str, status: &str, errors: &[String]) -> Result<()> {
    let now = Utc::now().to_rfc3339();
    crate::shared::data::db::get_connection().execute(Statement::from_sql_and_values(DatabaseBackend::Sqlite,"UPDATE sys_p916_repair_run SET status=?,phase='finished',errors_json=?,updated_at=?,completed_at=? WHERE id=?",[status.into(),serde_json::to_string(errors)?.into(),now.clone().into(),now.into(),id.into()])).await?;
    Ok(())
}

async fn finish_failed(id: &str, error: &str) -> Result<()> {
    finish(id, "failed", &[error.to_string()]).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec() -> RepairSpec {
        RepairSpec {
            target: TARGET.into(),
            date_from: "2026-01-01".into(),
            date_to: "2026-01-31".into(),
            connection_mp_refs: vec!["b".into(), "a".into()],
            actions: vec![],
            limitations: vec![],
        }
    }

    #[test]
    fn hash_is_stable_and_scope_sensitive() {
        let first = spec_hash(&spec()).unwrap();
        assert_eq!(first, spec_hash(&spec()).unwrap());
        let mut changed = spec();
        changed.date_to = "2026-02-01".into();
        assert_ne!(first, spec_hash(&changed).unwrap());
    }

    #[test]
    fn only_p916_is_supported() {
        assert!(validate_scope("p900_mp_sales_register", "2026-01-01", "2026-01-31").is_err());
        assert!(validate_scope(TARGET, "2026-02-01", "2026-01-31").is_err());
    }

    #[test]
    fn wb_history_is_chunked_below_api_limit() {
        let windows = source_windows(
            "a036_wb_sales_funnel_daily_history",
            NaiveDate::from_ymd_opt(2024, 1, 1).unwrap(),
            NaiveDate::from_ymd_opt(2025, 12, 31).unwrap(),
        );
        assert!(windows.len() >= 2);
        assert!(windows
            .iter()
            .all(|(from, to)| (*to - *from).num_days() < 365));
        assert!(windows
            .windows(2)
            .all(|pair| pair[0].1 + chrono::Duration::days(1) == pair[1].0));
    }

    #[test]
    fn wb_operational_source_is_current_day_only() {
        let today = Utc::now().date_naive();
        assert_eq!(
            source_windows(
                "a036_wb_sales_funnel_daily",
                today - chrono::Duration::days(30),
                today,
            ),
            vec![(today, today)]
        );
    }
}
