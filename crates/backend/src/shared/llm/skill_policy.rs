//! DB-backed access policy for LLM skills.
//!
//! Skill markdown files describe content and tools. This module is the only
//! source of specialization access and cross-skill capabilities.

use contracts::domain::a017_llm_agent::aggregate::AgentType;
use sea_orm::{ConnectionTrait, DatabaseBackend, DatabaseConnection, Statement, TransactionTrait};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;

pub const ARTIFACT_PUBLISH: &str = "artifact_publish";
pub const SKILL_SCRIPT_EXECUTE: &str = "skill_script_execute";
pub const SKILL_SCRIPT_DEVELOP: &str = "skill_script_develop";
pub const DATA_REPAIR_EXECUTE: &str = "data_repair_execute";
const MUTATING_ARTIFACT_TOOLS: &[&str] = &["build_chart", "build_table", "plugin_upsert"];
const MUTATING_DATA_REPAIR_TOOLS: &[&str] = &["execute_funnel_repair"];
const CAPABILITIES: &[&str] = &[
    ARTIFACT_PUBLISH,
    SKILL_SCRIPT_EXECUTE,
    SKILL_SCRIPT_DEVELOP,
    DATA_REPAIR_EXECUTE,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillAccessLevel {
    Immediate,
    Extended,
    Denied,
}

impl SkillAccessLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Immediate => "immediate",
            Self::Extended => "extended",
            Self::Denied => "denied",
        }
    }

    fn from_db(value: &str) -> Option<Self> {
        match value {
            "immediate" => Some(Self::Immediate),
            "extended" => Some(Self::Extended),
            "denied" => Some(Self::Denied),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct EffectiveSkillAccess {
    pub specialization: String,
    pub skill_id: String,
    pub level: SkillAccessLevel,
    pub source: &'static str,
}

#[derive(Debug, Clone, Serialize)]
pub struct SpecializationCapability {
    pub specialization: String,
    pub capability: String,
    pub is_allowed: bool,
    pub source: &'static str,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MatrixCellInput {
    pub specialization: String,
    pub skill_id: String,
    pub level: SkillAccessLevel,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CapabilityInput {
    pub specialization: String,
    pub capability: String,
    pub is_allowed: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SaveMatrixRequest {
    pub revision: String,
    #[serde(default)]
    pub cells: Vec<MatrixCellInput>,
    #[serde(default)]
    pub capabilities: Vec<CapabilityInput>,
}

pub fn specializations() -> Vec<AgentType> {
    vec![
        AgentType::BusinessAnalyst,
        AgentType::SalesAnalyst,
        AgentType::Marketer,
        AgentType::Financier,
        AgentType::SystemAdmin,
        AgentType::KbAdmin,
        AgentType::PluginAdmin,
        AgentType::CoordinatorAdmin,
        AgentType::Tester,
    ]
}

pub fn default_access(agent_type: &AgentType) -> SkillAccessLevel {
    if matches!(agent_type, AgentType::CoordinatorAdmin) {
        SkillAccessLevel::Immediate
    } else {
        SkillAccessLevel::Denied
    }
}

pub fn default_capability(agent_type: &AgentType, capability: &str) -> bool {
    match capability {
        ARTIFACT_PUBLISH => matches!(agent_type, AgentType::CoordinatorAdmin),
        SKILL_SCRIPT_EXECUTE | SKILL_SCRIPT_DEVELOP => {
            matches!(
                agent_type,
                AgentType::CoordinatorAdmin | AgentType::PluginAdmin
            )
        }
        DATA_REPAIR_EXECUTE => matches!(agent_type, AgentType::CoordinatorAdmin),
        _ => false,
    }
}

async fn configured_access(
    db: &DatabaseConnection,
    agent_type: &AgentType,
    skill_id: &str,
) -> anyhow::Result<Option<SkillAccessLevel>> {
    let row = db
        .query_one(Statement::from_sql_and_values(
            DatabaseBackend::Sqlite,
            "SELECT access_level FROM sys_llm_skill_access \
             WHERE specialization = ? AND skill_id = ?",
            [agent_type.as_str().into(), skill_id.into()],
        ))
        .await?;
    Ok(row
        .and_then(|r| r.try_get::<String>("", "access_level").ok())
        .and_then(|v| SkillAccessLevel::from_db(&v)))
}

pub async fn effective_access(
    agent_type: &AgentType,
    skill_id: &str,
) -> anyhow::Result<EffectiveSkillAccess> {
    let db = crate::shared::data::db::get_connection();
    let configured = configured_access(db, agent_type, skill_id).await?;
    Ok(EffectiveSkillAccess {
        specialization: agent_type.as_str().to_string(),
        skill_id: skill_id.to_string(),
        level: configured.unwrap_or_else(|| default_access(agent_type)),
        source: if configured.is_some() {
            "configured"
        } else {
            "default"
        },
    })
}

pub async fn accesses_for(
    agent_type: &AgentType,
) -> anyhow::Result<HashMap<String, SkillAccessLevel>> {
    accesses_for_snapshot(agent_type, &super::skills::snapshot()).await
}

pub async fn accesses_for_snapshot(
    agent_type: &AgentType,
    snapshot: &super::skills::SkillRegistrySnapshot,
) -> anyhow::Result<HashMap<String, SkillAccessLevel>> {
    let db = crate::shared::data::db::get_connection();
    let rows = db
        .query_all(Statement::from_sql_and_values(
            DatabaseBackend::Sqlite,
            "SELECT skill_id, access_level FROM sys_llm_skill_access WHERE specialization = ?",
            [agent_type.as_str().into()],
        ))
        .await?;
    let mut configured = HashMap::new();
    for row in rows {
        let id: String = row.try_get("", "skill_id")?;
        let raw: String = row.try_get("", "access_level")?;
        if let Some(level) = SkillAccessLevel::from_db(&raw) {
            configured.insert(id, level);
        }
    }
    // Навык, которого нет в матрице, проваливается в default_access (Denied) и при
    // этом не попадает ни в один список list_skills — агент его просто не видит и
    // не может назвать причину. Новый навык из внешнего каталога попадал в эту дыру
    // молча: файл подхватывался reload'ом, а строки доступа никто не заводил.
    let unconfigured: Vec<&str> = snapshot
        .skills
        .iter()
        .filter(|skill| !configured.contains_key(&skill.id))
        .map(|skill| skill.id.as_str())
        .collect();
    if !unconfigured.is_empty() {
        tracing::warn!(
            "[skill_policy] у специализации '{}' нет строк матрицы доступа для навыков: {} — \
             применён уровень по умолчанию ({}). Заполни матрицу, иначе навык невидим для агента.",
            agent_type.as_str(),
            unconfigured.join(", "),
            default_access(agent_type).as_str()
        );
    }

    Ok(snapshot
        .skills
        .iter()
        .map(|skill| {
            let level = configured
                .get(&skill.id)
                .copied()
                .unwrap_or_else(|| default_access(agent_type));
            (skill.id.clone(), level)
        })
        .collect())
}

pub async fn capability_allowed(agent_type: &AgentType, capability: &str) -> anyhow::Result<bool> {
    let db = crate::shared::data::db::get_connection();
    let row = db
        .query_one(Statement::from_sql_and_values(
            DatabaseBackend::Sqlite,
            "SELECT is_allowed FROM sys_llm_specialization_capability \
             WHERE specialization = ? AND capability = ?",
            [agent_type.as_str().into(), capability.into()],
        ))
        .await?;
    Ok(row
        .and_then(|r| r.try_get::<i64>("", "is_allowed").ok())
        .map(|v| v != 0)
        .unwrap_or_else(|| default_capability(agent_type, capability)))
}

pub fn is_artifact_mutation(tool_name: &str) -> bool {
    MUTATING_ARTIFACT_TOOLS.contains(&tool_name)
}

pub fn is_data_repair_mutation(tool_name: &str) -> bool {
    MUTATING_DATA_REPAIR_TOOLS.contains(&tool_name)
}

fn revision_for(
    cells: &[EffectiveSkillAccess],
    capabilities: &[SpecializationCapability],
) -> String {
    let mut lines: Vec<String> = cells
        .iter()
        .map(|c| {
            format!(
                "s:{}:{}:{}:{}",
                c.specialization,
                c.skill_id,
                c.level.as_str(),
                c.source
            )
        })
        .collect();
    lines.extend(capabilities.iter().map(|c| {
        format!(
            "c:{}:{}:{}:{}",
            c.specialization, c.capability, c.is_allowed, c.source
        )
    }));
    lines.sort();
    format!("{:x}", Sha256::digest(lines.join("\n").as_bytes()))
}

fn normalize_default_specialization(role: &str) -> Option<&'static str> {
    if role == "general" {
        return Some(AgentType::CoordinatorAdmin.as_str());
    }
    specializations()
        .into_iter()
        .find(|specialization| specialization.as_str() == role)
        .map(|specialization| specialization.as_str())
}

fn default_for_diagnostics(
    skills: &[super::skills::Skill],
    cells: &[EffectiveSkillAccess],
) -> Vec<String> {
    let mut diagnostics = Vec::new();
    for skill in skills {
        for role in &skill.default_for {
            let Some(specialization) = normalize_default_specialization(role) else {
                diagnostics.push(format!(
                    "Skill '{}': default_for содержит неизвестную роль '{}'",
                    skill.id, role
                ));
                continue;
            };
            let assigned = cells.iter().any(|cell| {
                cell.skill_id == skill.id
                    && cell.specialization == specialization
                    && cell.level != SkillAccessLevel::Denied
            });
            if !assigned {
                diagnostics.push(format!(
                    "Skill '{}': default_for содержит роль '{}', но она не назначена в матрице доступа",
                    skill.id, role
                ));
            }
        }
    }
    diagnostics
}

pub async fn matrix_json() -> anyhow::Result<serde_json::Value> {
    let db = crate::shared::data::db::get_connection();
    let skill_snapshot = super::skills::snapshot();
    let access_rows = db
        .query_all(Statement::from_string(
            DatabaseBackend::Sqlite,
            "SELECT specialization, skill_id, access_level FROM sys_llm_skill_access",
        ))
        .await?;
    let mut configured_access = HashMap::new();
    for row in access_rows {
        let specialization: String = row.try_get("", "specialization")?;
        let skill_id: String = row.try_get("", "skill_id")?;
        let level: String = row.try_get("", "access_level")?;
        if let Some(level) = SkillAccessLevel::from_db(&level) {
            configured_access.insert((specialization, skill_id), level);
        }
    }

    let capability_rows = db
        .query_all(Statement::from_string(
            DatabaseBackend::Sqlite,
            "SELECT specialization, capability, is_allowed \
             FROM sys_llm_specialization_capability",
        ))
        .await?;
    let mut configured_capabilities = HashMap::new();
    for row in capability_rows {
        let specialization: String = row.try_get("", "specialization")?;
        let capability: String = row.try_get("", "capability")?;
        let is_allowed: i64 = row.try_get("", "is_allowed")?;
        configured_capabilities.insert((specialization, capability), is_allowed != 0);
    }

    let mut cells = Vec::new();
    let mut capabilities = Vec::new();
    for specialization in specializations() {
        for skill in skill_snapshot.skills.iter() {
            let configured =
                configured_access.get(&(specialization.as_str().to_string(), skill.id.clone()));
            cells.push(EffectiveSkillAccess {
                specialization: specialization.as_str().to_string(),
                skill_id: skill.id.clone(),
                level: configured
                    .copied()
                    .unwrap_or_else(|| default_access(&specialization)),
                source: if configured.is_some() {
                    "configured"
                } else {
                    "default"
                },
            });
        }
        for capability in CAPABILITIES {
            let configured = configured_capabilities
                .get(&(specialization.as_str().to_string(), capability.to_string()));
            capabilities.push(SpecializationCapability {
                specialization: specialization.as_str().to_string(),
                capability: capability.to_string(),
                is_allowed: configured
                    .copied()
                    .unwrap_or_else(|| default_capability(&specialization, capability)),
                source: if configured.is_some() {
                    "configured"
                } else {
                    "default"
                },
            });
        }
    }
    let revision = revision_for(&cells, &capabilities);
    let mut diagnostics = super::skills::diagnostics();
    diagnostics.extend(default_for_diagnostics(&skill_snapshot.skills, &cells));
    let specializations: Vec<_> = specializations()
        .into_iter()
        .map(|s| {
            serde_json::json!({
                "id": s.as_str(),
                "title": s.display_name(),
                "is_coordinator": matches!(s, AgentType::CoordinatorAdmin),
            })
        })
        .collect();
    let skills: Vec<_> = skill_snapshot
        .skills
        .iter()
        .map(|s| {
            serde_json::json!({
                "id": s.id,
                "title": s.title,
                "description": s.description,
                "intents": s.intents,
                "tools": s.tool_names,
                "digest": s.digest,
            })
        })
        .collect();
    Ok(serde_json::json!({
        "revision": revision,
        "generation": skill_snapshot.generation,
        "catalog_digest": skill_snapshot.catalog_digest,
        "specializations": specializations,
        "skills": skills,
        "cells": cells,
        "capabilities": capabilities,
        "capability_specs": [
            {
                "id": ARTIFACT_PUBLISH,
                "title": "Публикация артефактов",
                "description": "Сохранение chart/table/plugin артефактов."
            },
            {
                "id": SKILL_SCRIPT_EXECUTE,
                "title": "Выполнение skill scripts",
                "description": "Запуск стабильных JavaScript-задач из активного skill."
            },
            {
                "id": SKILL_SCRIPT_DEVELOP,
                "title": "Разработка skill scripts",
                "description": "Запуск JavaScript-задач, помеченных как development."
            }
        ],
        "diagnostics": diagnostics,
    }))
}

fn validate_matrix_snapshot(
    request: &SaveMatrixRequest,
    known_specializations: &std::collections::HashSet<String>,
    known_skills: &std::collections::HashSet<String>,
) -> anyhow::Result<()> {
    let expected_cells = known_specializations.len() * known_skills.len();
    if request.cells.len() != expected_cells {
        anyhow::bail!(
            "Full matrix snapshot expected {} skill cells, received {}",
            expected_cells,
            request.cells.len()
        );
    }
    let expected_capabilities = known_specializations.len() * CAPABILITIES.len();
    if request.capabilities.len() != expected_capabilities {
        anyhow::bail!(
            "Full matrix snapshot expected {} capability cells, received {}",
            expected_capabilities,
            request.capabilities.len()
        );
    }
    let mut seen_cells = std::collections::HashSet::new();
    for cell in &request.cells {
        if !known_specializations.contains(&cell.specialization)
            || !known_skills.contains(&cell.skill_id)
        {
            anyhow::bail!("Invalid specialization/skill cell");
        }
        if !seen_cells.insert((cell.specialization.as_str(), cell.skill_id.as_str())) {
            anyhow::bail!("Duplicate specialization/skill cell");
        }
    }
    let mut seen_capabilities = std::collections::HashSet::new();
    for capability in &request.capabilities {
        if !known_specializations.contains(&capability.specialization)
            || !CAPABILITIES.contains(&capability.capability.as_str())
        {
            anyhow::bail!("Invalid specialization capability");
        }
        if !seen_capabilities.insert((
            capability.specialization.as_str(),
            capability.capability.as_str(),
        )) {
            anyhow::bail!("Duplicate specialization capability");
        }
    }
    Ok(())
}

pub async fn save_matrix(request: SaveMatrixRequest) -> anyhow::Result<serde_json::Value> {
    let known_specializations: std::collections::HashSet<_> = specializations()
        .into_iter()
        .map(|s| s.as_str().to_string())
        .collect();
    let known_skills: std::collections::HashSet<_> = super::skills::skills()
        .iter()
        .map(|s| s.id.clone())
        .collect();
    validate_matrix_snapshot(&request, &known_specializations, &known_skills)?;

    let _write_guard = crate::shared::data::db::acquire_sqlite_write_lock().await;
    let current = matrix_json().await?;
    if current["revision"].as_str().unwrap_or_default() != request.revision {
        anyhow::bail!("REVISION_CONFLICT");
    }

    let db = crate::shared::data::db::get_connection();
    let txn = db.begin().await?;
    for cell in request.cells {
        txn.execute(Statement::from_sql_and_values(
            DatabaseBackend::Sqlite,
            "INSERT INTO sys_llm_skill_access \
             (specialization, skill_id, access_level, updated_at) VALUES (?, ?, ?, CURRENT_TIMESTAMP) \
             ON CONFLICT(specialization, skill_id) DO UPDATE SET \
             access_level = excluded.access_level, updated_at = CURRENT_TIMESTAMP",
            [
                cell.specialization.into(),
                cell.skill_id.into(),
                cell.level.as_str().into(),
            ],
        ))
        .await?;
    }
    for capability in request.capabilities {
        txn.execute(Statement::from_sql_and_values(
            DatabaseBackend::Sqlite,
            "INSERT INTO sys_llm_specialization_capability \
             (specialization, capability, is_allowed, updated_at) VALUES (?, ?, ?, CURRENT_TIMESTAMP) \
             ON CONFLICT(specialization, capability) DO UPDATE SET \
             is_allowed = excluded.is_allowed, updated_at = CURRENT_TIMESTAMP",
            [
                capability.specialization.into(),
                capability.capability.into(),
                (capability.is_allowed as i64).into(),
            ],
        ))
        .await?;
    }
    txn.commit().await?;
    matrix_json().await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coordinator_gets_new_skills_by_default() {
        assert_eq!(
            default_access(&AgentType::CoordinatorAdmin),
            SkillAccessLevel::Immediate
        );
        assert_eq!(
            default_access(&AgentType::BusinessAnalyst),
            SkillAccessLevel::Denied
        );
    }

    #[test]
    fn artifact_mutations_are_explicit() {
        assert!(is_artifact_mutation("build_chart"));
        assert!(is_artifact_mutation("build_table"));
        assert!(is_artifact_mutation("plugin_upsert"));
        assert!(!is_artifact_mutation("preview_data"));
    }

    #[test]
    fn data_repair_is_coordinator_only_by_default() {
        assert!(default_capability(
            &AgentType::CoordinatorAdmin,
            DATA_REPAIR_EXECUTE
        ));
        assert!(!default_capability(
            &AgentType::PluginAdmin,
            DATA_REPAIR_EXECUTE
        ));
        assert!(is_data_repair_mutation("execute_funnel_repair"));
        assert!(!is_data_repair_mutation("prepare_funnel_repair"));
    }

    #[test]
    fn script_capabilities_default_to_coordinator_and_plugin_admin() {
        assert!(default_capability(
            &AgentType::CoordinatorAdmin,
            SKILL_SCRIPT_EXECUTE
        ));
        assert!(default_capability(
            &AgentType::PluginAdmin,
            SKILL_SCRIPT_DEVELOP
        ));
        assert!(!default_capability(
            &AgentType::BusinessAnalyst,
            SKILL_SCRIPT_EXECUTE
        ));
    }

    #[test]
    fn default_for_warns_only_when_role_is_not_assigned() {
        let skill = super::super::skills::snapshot()
            .skills
            .iter()
            .find(|skill| skill.id == "data-analytics")
            .expect("data-analytics seed")
            .clone();
        let mut cells = vec![
            EffectiveSkillAccess {
                specialization: "business_analyst".into(),
                skill_id: skill.id.clone(),
                level: SkillAccessLevel::Denied,
                source: "configured",
            },
            EffectiveSkillAccess {
                specialization: "coordinator_admin".into(),
                skill_id: skill.id.clone(),
                level: SkillAccessLevel::Immediate,
                source: "default",
            },
        ];

        let diagnostics = default_for_diagnostics(std::slice::from_ref(&skill), &cells);
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].contains("business_analyst"));

        cells[0].level = SkillAccessLevel::Extended;
        assert!(default_for_diagnostics(std::slice::from_ref(&skill), &cells).is_empty());
    }

    fn one_cell_snapshot() -> SaveMatrixRequest {
        SaveMatrixRequest {
            revision: "test".into(),
            cells: vec![MatrixCellInput {
                specialization: "sales_analyst".into(),
                skill_id: "sample".into(),
                level: SkillAccessLevel::Immediate,
            }],
            capabilities: CAPABILITIES
                .iter()
                .map(|capability| CapabilityInput {
                    specialization: "sales_analyst".into(),
                    capability: (*capability).to_string(),
                    is_allowed: false,
                })
                .collect(),
        }
    }

    #[test]
    fn full_matrix_snapshot_rejects_missing_and_duplicate_cells() {
        let specs = std::collections::HashSet::from(["sales_analyst".to_string()]);
        let skills = std::collections::HashSet::from(["sample".to_string()]);
        let valid = one_cell_snapshot();
        assert!(validate_matrix_snapshot(&valid, &specs, &skills).is_ok());

        let mut missing = one_cell_snapshot();
        missing.cells.clear();
        assert!(validate_matrix_snapshot(&missing, &specs, &skills).is_err());

        let mut duplicate = one_cell_snapshot();
        duplicate.cells.push(duplicate.cells[0].clone());
        assert!(validate_matrix_snapshot(&duplicate, &specs, &skills).is_err());
    }

    #[tokio::test]
    async fn configured_denied_overrides_coordinator_default() {
        let db = sea_orm::Database::connect("sqlite::memory:")
            .await
            .expect("in-memory sqlite");
        db.execute(Statement::from_string(
            DatabaseBackend::Sqlite,
            "CREATE TABLE sys_llm_skill_access (
                specialization TEXT NOT NULL,
                skill_id TEXT NOT NULL,
                access_level TEXT NOT NULL,
                PRIMARY KEY (specialization, skill_id)
             )"
            .to_string(),
        ))
        .await
        .expect("policy fixture");
        db.execute(Statement::from_string(
            DatabaseBackend::Sqlite,
            "INSERT INTO sys_llm_skill_access VALUES
                ('coordinator_admin', 'restricted-skill', 'denied')"
                .to_string(),
        ))
        .await
        .expect("policy row");
        assert_eq!(
            configured_access(&db, &AgentType::CoordinatorAdmin, "restricted-skill")
                .await
                .unwrap(),
            Some(SkillAccessLevel::Denied)
        );
    }

    #[tokio::test]
    async fn migration_creates_policy_and_renames_legacy_coordinator() {
        let db = sea_orm::Database::connect("sqlite::memory:")
            .await
            .expect("in-memory sqlite");
        db.execute_unprepared(
            "CREATE TABLE a017_llm_agent (agent_type TEXT);
             CREATE TABLE a038_llm_connection (agent_type TEXT);
             CREATE TABLE a039_mail_message (agent_type TEXT);
             CREATE TABLE a018_llm_chat_message (id TEXT);
             INSERT INTO a017_llm_agent VALUES ('general');",
        )
        .await
        .expect("migration prerequisites");
        db.execute_unprepared(include_str!(
            "../../../../../migrations/0183_llm_skill_policy.sql"
        ))
        .await
        .expect("skill policy migration");
        db.execute_unprepared(include_str!(
            "../../../../../migrations/0184_llm_skill_script_capabilities.sql"
        ))
        .await
        .expect("skill script capability migration");
        db.execute_unprepared(include_str!(
            "../../../../../migrations/0185_llm_skill_reload_log.sql"
        ))
        .await
        .expect("skill reload log migration");
        let row = db
            .query_one(Statement::from_string(
                DatabaseBackend::Sqlite,
                "SELECT agent_type FROM a017_llm_agent".to_string(),
            ))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            row.try_get::<String>("", "agent_type").unwrap(),
            "coordinator_admin"
        );
        let seeded = db
            .query_one(Statement::from_string(
                DatabaseBackend::Sqlite,
                "SELECT access_level FROM sys_llm_skill_access
                 WHERE specialization='business_analyst' AND skill_id='data-analytics'"
                    .to_string(),
            ))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            seeded.try_get::<String>("", "access_level").unwrap(),
            "immediate"
        );
        let script_capability = db
            .query_one(Statement::from_string(
                DatabaseBackend::Sqlite,
                "SELECT is_allowed FROM sys_llm_specialization_capability
                 WHERE specialization='plugin_admin'
                   AND capability='skill_script_execute'"
                    .to_string(),
            ))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            script_capability.try_get::<i64>("", "is_allowed").unwrap(),
            1
        );
        let audit_table = db
            .query_one(Statement::from_string(
                DatabaseBackend::Sqlite,
                "SELECT name FROM sqlite_master
                 WHERE type='table' AND name='sys_llm_skill_reload_log'"
                    .to_string(),
            ))
            .await
            .unwrap();
        assert!(audit_table.is_some());
    }
}
