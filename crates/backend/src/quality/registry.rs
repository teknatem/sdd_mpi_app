use contracts::plugins::PluginCapability;
use contracts::quality::{
    CheckBreakdown, CheckMetric, QualityCheckInfo, QualityCheckReloadReport, QualityCheckSource,
    ViolationItem,
};
use serde::Deserialize;
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock, RwLock};

const MAX_MANIFEST_BYTES: u64 = 64 * 1024;
const MAX_SCRIPT_BYTES: u64 = 512 * 1024;
const MAX_SCHEMA_BYTES: u64 = 256 * 1024;

#[derive(Debug, Clone)]
pub struct JavascriptCheck {
    pub source: String,
    pub export: String,
    pub capabilities: Vec<String>,
}

#[derive(Debug, Clone, Copy)]
pub enum RustCheck {
    NomenclatureInProjections,
    ProjectionOrphanRegistrators,
    GlProjectionIntegrity,
    P903GlIntegrity,
}

#[derive(Debug, Clone)]
pub enum CheckExecutor {
    Rust(RustCheck),
    Javascript(JavascriptCheck),
}

#[derive(Debug, Clone)]
pub struct CheckDefinition {
    pub info: QualityCheckInfo,
    pub kind: String,
    pub digest: String,
    pub default_input: Value,
    pub input_schema: Option<Value>,
    pub executor: CheckExecutor,
}

#[derive(Debug, Clone)]
pub struct RegistrySnapshot {
    pub generation: u64,
    pub catalog_digest: String,
    pub definitions: Arc<Vec<CheckDefinition>>,
    pub diagnostics: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct CheckOutput {
    #[serde(default)]
    pub metrics: Vec<CheckMetric>,
    #[serde(default)]
    pub violations: Vec<ViolationItem>,
    #[serde(default)]
    pub breakdowns: Vec<CheckBreakdown>,
    #[serde(default)]
    pub sources: Vec<QualityCheckSource>,
}

#[derive(Debug, Deserialize)]
struct Manifest {
    id: String,
    code: String,
    name: String,
    description: String,
    category: String,
    kind: String,
    #[serde(default = "default_entrypoint")]
    entrypoint: String,
    #[serde(default = "default_export")]
    export: String,
    #[serde(default)]
    input_schema: Option<String>,
    #[serde(default)]
    capabilities: Vec<String>,
    #[serde(default = "empty_object")]
    default_input: Value,
}

fn default_entrypoint() -> String {
    "check.mjs".to_string()
}
fn default_export() -> String {
    "run".to_string()
}
fn empty_object() -> Value {
    Value::Object(Map::new())
}

static REGISTRY: OnceLock<RwLock<Arc<RegistrySnapshot>>> = OnceLock::new();

fn registry() -> &'static RwLock<Arc<RegistrySnapshot>> {
    REGISTRY.get_or_init(|| {
        // The external catalog is never made active before its JavaScript has
        // passed the asynchronous compile/export validation in `reload()`.
        let snapshot =
            build_embedded_snapshot(1, Vec::new()).expect("embedded quality catalog must be valid");
        RwLock::new(Arc::new(snapshot))
    })
}

pub fn snapshot() -> Arc<RegistrySnapshot> {
    registry()
        .read()
        .expect("quality registry poisoned")
        .clone()
}

pub fn definition(id: &str) -> Option<CheckDefinition> {
    snapshot()
        .definitions
        .iter()
        .find(|item| item.info.id == id)
        .cloned()
}

fn rust_definitions() -> Vec<CheckDefinition> {
    use crate::quality::checks;
    [
        (
            "QC-001",
            checks::nomenclature_in_projections::info(),
            RustCheck::NomenclatureInProjections,
        ),
        (
            "QC-002",
            checks::projection_orphan_registrators::info(),
            RustCheck::ProjectionOrphanRegistrators,
        ),
        (
            "QC-004",
            checks::gl_projection_integrity::info(),
            RustCheck::GlProjectionIntegrity,
        ),
        (
            "QC-005",
            checks::p903_gl_integrity::info(),
            RustCheck::P903GlIntegrity,
        ),
    ]
    .into_iter()
    .map(|(code, mut info, check)| {
        info.code = code.to_string();
        CheckDefinition {
            digest: sha256(format!("rust:{}:{code}", info.id).as_bytes()),
            info,
            kind: "regular".to_string(),
            default_input: empty_object(),
            input_schema: None,
            executor: CheckExecutor::Rust(check),
        }
    })
    .collect()
}

fn embedded_packages() -> [(&'static str, &'static str, Option<&'static str>); 6] {
    [
        (
            include_str!("../../quality_checks/marketplace_product_ref_required/check.json"),
            include_str!("../../quality_checks/marketplace_product_ref_required/check.mjs"),
            None,
        ),
        (
            include_str!("../../quality_checks/p907_gl_coverage/check.json"),
            include_str!("../../quality_checks/p907_gl_coverage/check.mjs"),
            None,
        ),
        (
            include_str!("../../quality_checks/wb_funnel_projection_coverage/check.json"),
            include_str!("../../quality_checks/wb_funnel_projection_coverage/check.mjs"),
            Some(include_str!(
                "../../quality_checks/wb_funnel_projection_coverage/schema.json"
            )),
        ),
        (
            include_str!("../../quality_checks/wb_marketing_projection_coverage/check.json"),
            include_str!("../../quality_checks/wb_marketing_projection_coverage/check.mjs"),
            Some(include_str!(
                "../../quality_checks/wb_marketing_projection_coverage/schema.json"
            )),
        ),
        (
            include_str!("../../quality_checks/ym_funnel_projection_coverage/check.json"),
            include_str!("../../quality_checks/ym_funnel_projection_coverage/check.mjs"),
            Some(include_str!(
                "../../quality_checks/ym_funnel_projection_coverage/schema.json"
            )),
        ),
        (
            include_str!("../../quality_checks/wb_day_not_closed/check.json"),
            include_str!("../../quality_checks/wb_day_not_closed/check.mjs"),
            None,
        ),
    ]
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn safe_relative_file(value: &str) -> bool {
    let path = Path::new(value);
    !value.trim().is_empty()
        && !path.is_absolute()
        && path
            .components()
            .all(|part| matches!(part, std::path::Component::Normal(_)))
}

fn validate_manifest(manifest: &Manifest) -> Result<(), String> {
    if manifest.id.is_empty()
        || !manifest
            .id
            .chars()
            .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_')
    {
        return Err(format!("invalid check id '{}'", manifest.id));
    }
    if !manifest.code.starts_with("QC-") || manifest.code.len() != 6 {
        return Err(format!("invalid code '{}'", manifest.code));
    }
    if !matches!(manifest.kind.as_str(), "regular" | "domain") {
        return Err(format!("invalid kind '{}'", manifest.kind));
    }
    if !safe_relative_file(&manifest.entrypoint)
        || !(manifest.entrypoint.ends_with(".mjs") || manifest.entrypoint.ends_with(".js"))
    {
        return Err(format!("unsafe entrypoint '{}'", manifest.entrypoint));
    }
    if manifest.export.is_empty()
        || !manifest.export.chars().enumerate().all(|(index, ch)| {
            ch.is_ascii_alphabetic() || ch == '_' || ch == '$' || (index > 0 && ch.is_ascii_digit())
        })
    {
        return Err(format!("invalid export '{}'", manifest.export));
    }
    if !manifest.default_input.is_object() {
        return Err("default_input must be an object".to_string());
    }
    let mut capabilities = HashSet::new();
    for raw in &manifest.capabilities {
        match PluginCapability::parse(raw) {
            PluginCapability::DbRead(table)
                if table
                    .chars()
                    .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_') =>
            {
                if !capabilities.insert(table) {
                    return Err(format!("duplicate quality check capability: '{raw}'"));
                }
            }
            _ => {
                return Err(format!(
                    "quality check capability must be exact db:read:<table>: '{raw}'"
                ))
            }
        }
    }
    Ok(())
}

pub(crate) fn definition_from_parts(
    manifest_raw: &str,
    script: String,
    schema_raw: Option<&str>,
) -> Result<CheckDefinition, String> {
    let manifest: Manifest = serde_json::from_str(manifest_raw)
        .map_err(|error| format!("invalid check.json: {error}"))?;
    validate_manifest(&manifest)?;
    let marker = format!("export async function {}", manifest.export);
    let marker_sync = format!("export function {}", manifest.export);
    if !script.contains(&marker) && !script.contains(&marker_sync) {
        return Err(format!("entrypoint does not export '{}'", manifest.export));
    }
    let input_schema = match schema_raw {
        Some(raw) => {
            let schema: Value = serde_json::from_str(raw)
                .map_err(|error| format!("invalid input schema: {error}"))?;
            jsonschema::validator_for(&schema)
                .map_err(|error| format!("invalid input schema: {error}"))?;
            Some(schema)
        }
        None => None,
    };
    if let Some(schema) = &input_schema {
        jsonschema::validator_for(schema)
            .map_err(|error| format!("invalid input schema: {error}"))?
            .validate(&manifest.default_input)
            .map_err(|error| format!("default_input does not match input schema: {error}"))?;
    }
    let digest =
        sha256(format!("{manifest_raw}\n{script}\n{}", schema_raw.unwrap_or("")).as_bytes());
    Ok(CheckDefinition {
        info: QualityCheckInfo {
            code: manifest.code,
            id: manifest.id,
            name: manifest.name,
            description: manifest.description,
            category: manifest.category,
        },
        kind: manifest.kind,
        digest,
        default_input: manifest.default_input,
        input_schema,
        executor: CheckExecutor::Javascript(JavascriptCheck {
            source: script,
            export: manifest.export,
            capabilities: manifest.capabilities,
        }),
    })
}

pub(crate) fn external_root() -> Option<PathBuf> {
    crate::shared::config::load_config()
        .ok()
        .map(|cfg| crate::shared::config::get_quality_checks_path(&cfg))
}

/// Validate an authoring bundle without changing the active catalog.
pub async fn validate_authoring_bundle(
    manifest: &Value,
    script: &str,
    schema: Option<&Value>,
) -> Result<CheckDefinition, Vec<String>> {
    let entrypoint = manifest
        .get("entrypoint")
        .and_then(Value::as_str)
        .unwrap_or("check.mjs");
    if entrypoint != "check.mjs" {
        return Err(vec![
            "runtime authoring requires entrypoint 'check.mjs'".into()
        ]);
    }
    let schema_path = manifest.get("input_schema").and_then(Value::as_str);
    if schema.is_some() != schema_path.is_some() {
        return Err(vec![
            "manifest.input_schema and schema must either both be present or both be absent".into(),
        ]);
    }
    if schema_path.is_some_and(|path| path != "schema.json") {
        return Err(vec![
            "runtime authoring requires input_schema 'schema.json'".into(),
        ]);
    }
    let manifest_raw = serde_json::to_string_pretty(manifest)
        .map_err(|error| vec![format!("manifest serialization failed: {error}")])?;
    let schema_raw = schema
        .map(serde_json::to_string_pretty)
        .transpose()
        .map_err(|error| vec![format!("schema serialization failed: {error}")])?;
    let definition =
        definition_from_parts(&manifest_raw, script.to_string(), schema_raw.as_deref())
            .map_err(|error| vec![error])?;
    let CheckExecutor::Javascript(js) = &definition.executor else {
        return Err(vec!["authoring bundle must be JavaScript".to_string()]);
    };
    let report = crate::plugins::engine::validate_server_script(&js.source).await;
    if !report.ok || !report.server_exports.iter().any(|item| item == &js.export) {
        return Err(report
            .errors
            .into_iter()
            .map(|error| format!("{}: {}", error.stage, error.message))
            .collect());
    }
    Ok(definition)
}

/// Return a portable representation of the active definition. Rust checks are
/// intentionally read-only: authoring can create and override MJS packages,
/// but cannot pretend that Rust source is editable at runtime.
pub fn authoring_bundle(id: &str) -> Option<Value> {
    let definition = definition(id)?;
    match definition.executor {
        CheckExecutor::Javascript(js) => Some(serde_json::json!({
            "editable": true,
            "manifest": {
                "id": definition.info.id,
                "code": definition.info.code,
                "name": definition.info.name,
                "description": definition.info.description,
                "category": definition.info.category,
                "kind": definition.kind,
                "entrypoint": "check.mjs",
                "export": js.export,
                "input_schema": definition.input_schema.as_ref().map(|_| "schema.json"),
                "capabilities": js.capabilities,
                "default_input": definition.default_input,
            },
            "script": js.source,
            "schema": definition.input_schema,
            "digest": definition.digest,
        })),
        CheckExecutor::Rust(_) => Some(serde_json::json!({
            "editable": false,
            "reason": "Проверка реализована в Rust и не изменяется в runtime",
            "info": definition.info,
            "kind": definition.kind,
            "digest": definition.digest,
        })),
    }
}

/// Save an external package and activate it through the normal atomic registry
/// reload. If the complete catalog is invalid, restore the previous files.
pub async fn upsert_authoring_bundle(
    manifest: Value,
    script: String,
    schema: Option<Value>,
) -> Result<QualityCheckReloadReport, Vec<String>> {
    let definition = validate_authoring_bundle(&manifest, &script, schema.as_ref()).await?;
    let root =
        external_root().ok_or_else(|| vec!["[quality].checks_path is unavailable".into()])?;
    std::fs::create_dir_all(&root)
        .map_err(|error| vec![format!("cannot create {}: {error}", root.display())])?;
    let id = definition.info.id;
    let nonce = uuid::Uuid::new_v4();
    let stage = root.join(format!(".{id}.{nonce}.tmp"));
    let backup = root.join(format!(".{id}.{nonce}.bak"));
    let target = root.join(&id);
    std::fs::create_dir(&stage)
        .map_err(|error| vec![format!("cannot create staging directory: {error}")])?;
    let write_result = (|| -> Result<(), String> {
        std::fs::write(
            stage.join("check.json"),
            serde_json::to_string_pretty(&manifest).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        std::fs::write(stage.join("check.mjs"), &script).map_err(|e| e.to_string())?;
        if let Some(schema) = &schema {
            std::fs::write(
                stage.join("schema.json"),
                serde_json::to_string_pretty(schema).map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string())?;
        }
        Ok(())
    })();
    if let Err(error) = write_result {
        let _ = std::fs::remove_dir_all(&stage);
        return Err(vec![format!("cannot write quality package: {error}")]);
    }
    let had_target = target.exists();
    if had_target {
        std::fs::rename(&target, &backup)
            .map_err(|error| vec![format!("cannot stage existing package: {error}")])?;
    }
    if let Err(error) = std::fs::rename(&stage, &target) {
        if had_target {
            let _ = std::fs::rename(&backup, &target);
        }
        let _ = std::fs::remove_dir_all(&stage);
        return Err(vec![format!("cannot activate package files: {error}")]);
    }
    let report = reload().await;
    if report.ok {
        if had_target {
            let _ = std::fs::remove_dir_all(&backup);
        }
        Ok(report)
    } else {
        let _ = std::fs::remove_dir_all(&target);
        if had_target {
            let _ = std::fs::rename(&backup, &target);
        }
        Err(report.diagnostics)
    }
}

fn load_external(root: &Path) -> Result<Vec<CheckDefinition>, Vec<String>> {
    if !root.is_dir() {
        return Ok(Vec::new());
    }
    let mut diagnostics = Vec::new();
    let mut definitions = Vec::new();
    let entries = match std::fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) => return Err(vec![format!("cannot read {}: {error}", root.display())]),
    };
    for entry in entries.flatten().filter(|entry| entry.path().is_dir()) {
        let package = entry.path();
        let manifest_path = package.join("check.json");
        if !manifest_path.is_file() {
            continue;
        }
        let result = (|| -> Result<CheckDefinition, String> {
            if manifest_path
                .metadata()
                .map(|m| m.len())
                .unwrap_or(u64::MAX)
                > MAX_MANIFEST_BYTES
            {
                return Err("check.json exceeds 64 KiB".to_string());
            }
            let manifest_raw = std::fs::read_to_string(&manifest_path)
                .map_err(|error| format!("cannot read check.json: {error}"))?;
            let manifest: Manifest = serde_json::from_str(&manifest_raw)
                .map_err(|error| format!("invalid check.json: {error}"))?;
            validate_manifest(&manifest)?;
            let script_path = package.join(&manifest.entrypoint);
            if script_path.metadata().map(|m| m.len()).unwrap_or(u64::MAX) > MAX_SCRIPT_BYTES {
                return Err("script exceeds 512 KiB".to_string());
            }
            let script = std::fs::read_to_string(&script_path)
                .map_err(|error| format!("cannot read {}: {error}", manifest.entrypoint))?;
            let schema_owned = match manifest.input_schema.as_deref() {
                Some(relative) if safe_relative_file(relative) => {
                    let path = package.join(relative);
                    if path.metadata().map(|m| m.len()).unwrap_or(u64::MAX) > MAX_SCHEMA_BYTES {
                        return Err("schema exceeds 256 KiB".to_string());
                    }
                    Some(
                        std::fs::read_to_string(path)
                            .map_err(|error| format!("cannot read schema: {error}"))?,
                    )
                }
                Some(_) => return Err("unsafe input_schema path".to_string()),
                None => None,
            };
            definition_from_parts(&manifest_raw, script, schema_owned.as_deref())
        })();
        match result {
            Ok(definition) => definitions.push(definition),
            Err(error) => diagnostics.push(format!("{}: {error}", package.display())),
        }
    }
    let mut ids = HashSet::new();
    let mut codes = HashSet::new();
    for definition in &definitions {
        if !ids.insert(definition.info.id.clone()) {
            diagnostics.push(format!(
                "duplicate external quality check id: {}",
                definition.info.id
            ));
        }
        if !codes.insert(definition.info.code.clone()) {
            diagnostics.push(format!(
                "duplicate external quality check code: {}",
                definition.info.code
            ));
        }
    }
    if diagnostics.is_empty() {
        Ok(definitions)
    } else {
        Err(diagnostics)
    }
}

fn base_definitions() -> Result<BTreeMap<String, CheckDefinition>, Vec<String>> {
    let mut by_id: BTreeMap<String, CheckDefinition> = rust_definitions()
        .into_iter()
        .map(|item| (item.info.id.clone(), item))
        .collect();
    for (manifest, script, schema) in embedded_packages() {
        match definition_from_parts(manifest, script.to_string(), schema) {
            Ok(item) => {
                by_id.insert(item.info.id.clone(), item);
            }
            Err(error) => return Err(vec![format!("embedded quality check: {error}")]),
        }
    }
    Ok(by_id)
}

fn finalize_snapshot(
    generation: u64,
    by_id: BTreeMap<String, CheckDefinition>,
    diagnostics: Vec<String>,
) -> Result<RegistrySnapshot, Vec<String>> {
    let mut definitions = by_id.into_values().collect::<Vec<_>>();
    definitions.sort_by(|left, right| left.info.code.cmp(&right.info.code));
    let mut codes = HashSet::new();
    let duplicates = definitions
        .iter()
        .filter_map(|item| {
            (!codes.insert(item.info.code.clone())).then_some(item.info.code.clone())
        })
        .collect::<Vec<_>>();
    if !duplicates.is_empty() {
        return Err(vec![format!(
            "duplicate quality check codes: {}",
            duplicates.join(", ")
        )]);
    }
    let catalog_digest = sha256(
        definitions
            .iter()
            .map(|item| format!("{}:{}", item.info.id, item.digest))
            .collect::<Vec<_>>()
            .join("\n")
            .as_bytes(),
    );
    Ok(RegistrySnapshot {
        generation,
        catalog_digest,
        definitions: Arc::new(definitions),
        diagnostics,
    })
}

fn build_embedded_snapshot(
    generation: u64,
    diagnostics: Vec<String>,
) -> Result<RegistrySnapshot, Vec<String>> {
    finalize_snapshot(generation, base_definitions()?, diagnostics)
}

fn build_snapshot(generation: u64) -> Result<RegistrySnapshot, Vec<String>> {
    let root = external_root();
    build_snapshot_from_root(generation, root.as_deref())
}

fn build_snapshot_from_root(
    generation: u64,
    external_root: Option<&Path>,
) -> Result<RegistrySnapshot, Vec<String>> {
    let mut by_id = base_definitions()?;
    if let Some(root) = external_root {
        for item in load_external(root)? {
            by_id.insert(item.info.id.clone(), item);
        }
    }
    finalize_snapshot(generation, by_id, Vec::new())
}

pub async fn reload() -> QualityCheckReloadReport {
    let previous = snapshot();
    let candidate = match build_snapshot(previous.generation + 1) {
        Ok(candidate) => candidate,
        Err(diagnostics) => {
            return QualityCheckReloadReport {
                ok: false,
                generation: previous.generation,
                catalog_digest: previous.catalog_digest.clone(),
                added: Vec::new(),
                changed: Vec::new(),
                removed: Vec::new(),
                diagnostics,
            }
        }
    };
    let mut diagnostics = Vec::new();
    for item in candidate.definitions.iter() {
        if let CheckExecutor::Javascript(js) = &item.executor {
            let report = crate::plugins::engine::validate_server_script(&js.source).await;
            if !report.ok
                || !report
                    .server_exports
                    .iter()
                    .any(|export| export == &js.export)
            {
                diagnostics.push(format!(
                    "{}: JavaScript validation failed: {:?}",
                    item.info.id, report.errors
                ));
            }
        }
    }
    if !diagnostics.is_empty() {
        return QualityCheckReloadReport {
            ok: false,
            generation: previous.generation,
            catalog_digest: previous.catalog_digest.clone(),
            added: Vec::new(),
            changed: Vec::new(),
            removed: Vec::new(),
            diagnostics,
        };
    }
    let old = previous
        .definitions
        .iter()
        .map(|item| (item.info.id.clone(), item.digest.clone()))
        .collect::<BTreeMap<_, _>>();
    let new = candidate
        .definitions
        .iter()
        .map(|item| (item.info.id.clone(), item.digest.clone()))
        .collect::<BTreeMap<_, _>>();
    let added = new
        .keys()
        .filter(|id| !old.contains_key(*id))
        .cloned()
        .collect();
    let removed = old
        .keys()
        .filter(|id| !new.contains_key(*id))
        .cloned()
        .collect();
    let changed = new
        .iter()
        .filter_map(|(id, digest)| {
            old.get(id)
                .filter(|old_digest| *old_digest != digest)
                .map(|_| id.clone())
        })
        .collect();
    let report = QualityCheckReloadReport {
        ok: true,
        generation: candidate.generation,
        catalog_digest: candidate.catalog_digest.clone(),
        added,
        changed,
        removed,
        diagnostics: Vec::new(),
    };
    *registry().write().expect("quality registry poisoned") = Arc::new(candidate);
    report
}

pub fn merge_input(default_input: &Value, input: Value) -> Value {
    let mut merged = default_input.as_object().cloned().unwrap_or_default();
    if let Some(values) = input.as_object() {
        merged.extend(values.clone());
    }
    Value::Object(merged)
}

pub fn validate_input(definition: &CheckDefinition, input: &Value) -> anyhow::Result<()> {
    if let Some(schema) = &definition.input_schema {
        jsonschema::validator_for(schema)
            .map_err(|error| anyhow::anyhow!(error.to_string()))?
            .validate(input)
            .map_err(|error| anyhow::anyhow!("invalid quality check input: {error}"))?;
    }
    Ok(())
}

pub fn validate_output(output: &CheckOutput) -> anyhow::Result<()> {
    if output.violations.len() > 20 {
        anyhow::bail!("quality check returned more than 20 violation samples");
    }
    for metric in &output.metrics {
        if metric.population < 0 || metric.violations < 0 || metric.violations > metric.population {
            anyhow::bail!(
                "invalid metric '{}': population={}, violations={}",
                metric.label,
                metric.population,
                metric.violations
            );
        }
    }
    output
        .metrics
        .iter()
        .try_fold((0_i64, 0_i64), |(population, violations), metric| {
            Some((
                population.checked_add(metric.population)?,
                violations.checked_add(metric.violations)?,
            ))
        })
        .ok_or_else(|| anyhow::anyhow!("quality check metric totals overflow"))?;
    let mut keys = HashSet::new();
    for breakdown in &output.breakdowns {
        if !keys.insert(&breakdown.key) {
            anyhow::bail!("duplicate breakdown key '{}'", breakdown.key);
        }
        for row in &breakdown.rows {
            if row.population < 0 || row.violations < 0 || row.violations > row.population {
                anyhow::bail!("invalid breakdown row '{}'", row.label);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_catalog() -> PathBuf {
        let path =
            std::env::temp_dir().join(format!("leptos-quality-registry-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&path).expect("create temporary catalog");
        path
    }

    fn write_package(root: &Path, directory: &str, manifest: &str, script: &str) {
        let package = root.join(directory);
        std::fs::create_dir_all(&package).expect("create package");
        std::fs::write(package.join("check.json"), manifest).expect("write manifest");
        std::fs::write(package.join("check.mjs"), script).expect("write script");
    }

    #[test]
    fn stable_codes_and_unique_ids_are_loaded() {
        let snapshot = build_snapshot(7).expect("registry");
        assert_eq!(snapshot.definitions.len(), 10);
        assert_eq!(
            snapshot
                .definitions
                .iter()
                .find(|d| d.info.id == "p907_gl_coverage")
                .unwrap()
                .info
                .code,
            "QC-006"
        );
    }

    #[test]
    fn invalid_output_is_rejected() {
        let output = CheckOutput {
            metrics: vec![CheckMetric {
                label: "bad".into(),
                population: 1,
                violations: 2,
                unit: "rows".into(),
            }],
            ..Default::default()
        };
        assert!(validate_output(&output).is_err());
    }

    #[test]
    fn manifest_schema_capability_and_export_are_validated() {
        let base = r#"{
            "id":"test_check","code":"QC-099","name":"Test","description":"test",
            "category":"test","kind":"regular","capabilities":[],"default_input":{}
        }"#;
        assert!(definition_from_parts(base, "export const nope = 1;".into(), None).is_err());

        let wildcard = base.replace("\"capabilities\":[]", "\"capabilities\":[\"db:read:*\"]");
        assert!(definition_from_parts(
            &wildcard,
            "export async function run() { return {}; }".into(),
            None,
        )
        .is_err());

        assert!(definition_from_parts(
            base,
            "export async function run() { return {}; }".into(),
            Some(r#"{"type":"definitely-not-a-json-schema-type"}"#),
        )
        .is_err());
    }

    #[tokio::test]
    async fn embedded_javascript_modules_compile_and_export_run() {
        let snapshot = build_embedded_snapshot(1, Vec::new()).expect("embedded registry");
        for definition in snapshot.definitions.iter() {
            if let CheckExecutor::Javascript(js) = &definition.executor {
                let report = crate::plugins::engine::validate_server_script(&js.source).await;
                assert!(report.ok, "{}: {:?}", definition.info.id, report.errors);
                assert!(report
                    .server_exports
                    .iter()
                    .any(|export| export == &js.export));
            }
        }
    }

    #[tokio::test]
    async fn runtime_authoring_validates_without_writing_files() {
        let manifest = serde_json::json!({
            "id":"runtime_test","code":"QC-099","name":"Runtime test",
            "description":"One invariant","category":"test","kind":"regular",
            "entrypoint":"check.mjs","export":"run","capabilities":[],"default_input":{}
        });
        let script = "export async function run() { return {metrics:[],violations:[],breakdowns:[],sources:[]}; }";
        assert!(validate_authoring_bundle(&manifest, script, None)
            .await
            .is_ok());

        let with_schema_path = serde_json::json!({
            "id":"runtime_test","code":"QC-099","name":"Runtime test",
            "description":"One invariant","category":"test","kind":"regular",
            "entrypoint":"check.mjs","export":"run","input_schema":"schema.json",
            "capabilities":[],"default_input":{}
        });
        assert!(validate_authoring_bundle(&with_schema_path, script, None)
            .await
            .is_err());
    }

    #[test]
    fn external_package_atomically_overrides_embedded_definition() {
        let root = temp_catalog();
        write_package(
            &root,
            "override",
            r#"{
                "id":"p907_gl_coverage","code":"QC-006","name":"External override",
                "description":"test","category":"test","kind":"regular",
                "capabilities":[],"default_input":{}
            }"#,
            "export async function run() { return { metrics: [], violations: [], breakdowns: [], sources: [] }; }",
        );

        let snapshot = build_snapshot_from_root(9, Some(&root)).expect("external override");
        let definition = snapshot
            .definitions
            .iter()
            .find(|item| item.info.id == "p907_gl_coverage")
            .expect("overridden definition");
        assert_eq!(definition.info.name, "External override");
        assert_eq!(snapshot.definitions.len(), 10);

        std::fs::remove_dir_all(root).expect("remove temporary catalog");
    }

    #[test]
    fn invalid_external_catalog_does_not_produce_a_candidate_snapshot() {
        let previous = build_embedded_snapshot(11, Vec::new()).expect("previous snapshot");
        let root = temp_catalog();
        write_package(
            &root,
            "invalid",
            r#"{
                "id":"broken","code":"QC-008","name":"Broken",
                "description":"test","category":"test","kind":"regular",
                "capabilities":["db:write:any"],"default_input":{}
            }"#,
            "export async function run() { return {}; }",
        );

        let candidate = build_snapshot_from_root(previous.generation + 1, Some(&root));
        assert!(candidate.is_err());
        assert_eq!(previous.generation, 11);
        assert_eq!(previous.definitions.len(), 10);

        std::fs::remove_dir_all(root).expect("remove temporary catalog");
    }

    #[test]
    fn duplicate_external_ids_are_rejected() {
        let root = temp_catalog();
        let manifest = r#"{
            "id":"duplicate","code":"QC-008","name":"Duplicate",
            "description":"test","category":"test","kind":"regular",
            "capabilities":[],"default_input":{}
        }"#;
        let script = "export async function run() { return {}; }";
        write_package(&root, "first", manifest, script);
        write_package(&root, "second", manifest, script);

        let error = build_snapshot_from_root(1, Some(&root)).expect_err("duplicate id");
        assert!(error
            .iter()
            .any(|line| line.contains("duplicate external quality check id")));

        std::fs::remove_dir_all(root).expect("remove temporary catalog");
    }
}
