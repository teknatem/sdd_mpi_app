//! Реестр Skills (capability-пакетов) для LLM-чата.
//!
//! Skill = id + описание + промпт-фрагмент + набор инструментов (по именам).
//! Заменяет статичный выбор инструментов по роли агента: набор инструментов и
//! системный промпт собираются из **активных** навыков (progressive disclosure).
//!
//! - `core` — всегда активен (понять систему + мета-инструменты list_skills/use_skill);
//! - бандлы (напр. `b_sql`) переиспользуются несколькими навыками;
//! - `agent_type` сохраняется как default-bias + allow-list (граница безопасности `use_skill`).

use super::frontmatter::{parse_list, parse_scalar, split_frontmatter};
use super::types::ToolDefinition;
use chrono::Utc;
use contracts::domain::a017_llm_agent::aggregate::AgentType;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock, RwLock};

// ─── Базовый промпт (role-agnostic) ──────────────────────────────────────────

const CORE_PROMPT: &str = include_str!("../../domain/a018_llm_chat/prompts/core.md");

// ─── Встроенный (seed) каталог навыков ───────────────────────────────────────
// Навыки — это ДАННЫЕ: markdown-файлы с frontmatter (crates/backend/skills/*.md).
// Встроенный набор всегда доступен (как embedded-доки KB); внешний каталог
// `[llm] skills_path` из конфига ДОПОЛНЯЕТ/ПЕРЕОПРЕДЕЛЯЕТ их по `id` — новый навык =
// новый файл, без пересборки (подхват после рестарта backend).
const EMBEDDED_SKILL_FILES: &[&str] = &[
    include_str!("../../../skills/data-analytics.md"),
    include_str!("../../../skills/bi-authoring.md"),
    include_str!("../../../skills/plugin-authoring.md"),
    include_str!("../../../skills/chart-builder.md"),
    include_str!("../../../skills/table-builder.md"),
    include_str!("../../../skills/system-admin.md"),
    include_str!("../../../skills/mailbox.md"),
    include_str!("../../../skills/kb-curation.md"),
    include_str!("../../../skills/kb-authoring.md"),
    include_str!("../../../skills/marketing-analytics.md"),
    include_str!("../../../skills/sales-analytics.md"),
    // Пакетный навык: встроенная копия даёт промпт и инструменты, но без ресурсов
    // (шаблон анкеты приходит из внешнего каталога `[llm] skills_path`).
    include_str!("../../../skills/finance-analytics/SKILL.md"),
    include_str!("../../../skills/price-optimization.md"),
    include_str!("../../../skills/support.md"),
    include_str!("../../../skills/quality-monitoring.md"),
    include_str!("../../../skills/quality-check-authoring.md"),
    include_str!("../../../skills/llm-quality-review.md"),
    include_str!("../../../skills/agent-delegation.md"),
];

// ─── Core: всегда активные инструменты ───────────────────────────────────────

/// Базовый набор (понять систему) + мета-инструменты навыков. Всегда активен.
const CORE_TOOLS: &[&str] = &[
    "get_architecture_overview",
    "get_entity_schema",
    "search_knowledge",
    "get_knowledge",
    // Пожаловаться на статью должен уметь любой агент, который может её прочитать,
    // иначе метрика неточностей никогда не наполнится. Запись статей (kb_propose_article)
    // намеренно НЕ здесь: она требует явного use_skill("kb-authoring").
    "kb_report_issue",
    "list_skills",
    "use_skill",
    "list_skill_resources",
    "read_skill_resource",
    "run_skill_task",
    // Рабочий каталог чата: анкета, план, журнал шагов. В core, а не в отдельном
    // навыке — состояние задачи нужно любому навыку, и терять его на переключении
    // между ними нельзя.
    "list_chat_files",
    "read_chat_file",
    "write_chat_file",
    "update_plan_step",
    "save_step",
    "start_activity",
    "switch_activity",
];

// Бандл интроспекции БД (`list_entities`, `get_join_hint`, `execute_query`) переиспользуется
// несколькими навыками — имена перечислены в их `tool_names` (дедуп при сборке).

// ─── Реестр навыков (data-driven) ────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct SkillResource {
    /// Package-relative slash-separated path.
    pub path: String,
    pub kind: String,
    pub content: String,
}

#[derive(Debug, Clone)]
pub struct SkillTask {
    pub id: String,
    pub title: String,
    pub runtime: String,
    /// Package-relative path to an ES module.
    pub entrypoint: String,
    pub export: String,
    /// `stable` requires skill_script_execute; `development` requires
    /// skill_script_develop.
    pub mode: String,
    pub input_schema: Option<Value>,
    pub input_schema_path: Option<String>,
    pub capabilities: Vec<String>,
    /// Source snapshot loaded atomically with the package registry.
    pub source: String,
}

#[derive(Debug, Clone)]
pub struct Skill {
    pub id: String,
    pub title: String,
    pub description: String,
    /// Router-интенты, ведущие к этому навыку.
    pub intents: Vec<String>,
    pub prompt: String,
    /// Имена инструментов навыка (резолвятся в определения из «вселенной» tool'ов).
    pub tool_names: Vec<String>,
    /// Роли, которым навык доступен через use_skill (граница безопасности). General — все.
    /// Роли, у кого навык активен по умолчанию (default-bias).
    pub resources: Vec<SkillResource>,
    pub tasks: Vec<SkillTask>,
    /// Canonical package root. Legacy/embedded single-file skills have none.
    pub package_root: Option<PathBuf>,
    pub origin: String,
    pub digest: String,
    pub compatibility_warnings: Vec<String>,
}

/// Immutable catalog version captured once for the whole chat-message cycle.
#[derive(Debug, Clone)]
pub struct SkillRegistrySnapshot {
    pub generation: u64,
    pub catalog_digest: String,
    pub loaded_at: String,
    pub skills: Arc<Vec<Skill>>,
    pub diagnostics: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SkillCatalogDiff {
    pub added: Vec<String>,
    pub changed: Vec<String>,
    pub removed: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
struct SkillManifest {
    id: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    intents: Vec<String>,
    #[serde(default)]
    tools: Vec<String>,
    #[serde(default)]
    allowed_for: Vec<String>,
    #[serde(default)]
    default_for: Vec<String>,
    #[serde(default)]
    resources: Vec<String>,
    #[serde(default)]
    tasks: Vec<SkillTaskManifest>,
}

#[derive(Debug, Default, Deserialize)]
struct SkillTaskManifest {
    id: String,
    #[serde(default)]
    title: String,
    #[serde(default = "default_task_runtime")]
    runtime: String,
    entrypoint: String,
    #[serde(default = "default_task_export")]
    export: String,
    #[serde(default = "default_task_mode")]
    mode: String,
    #[serde(default)]
    input_schema: Option<String>,
    #[serde(default)]
    capabilities: Vec<String>,
}

fn default_task_runtime() -> String {
    "javascript".to_string()
}

fn default_task_export() -> String {
    "run".to_string()
}

fn default_task_mode() -> String {
    "development".to_string()
}

static REGISTRY: OnceLock<RwLock<Arc<SkillRegistrySnapshot>>> = OnceLock::new();
static DIAGNOSTICS: OnceLock<std::sync::Mutex<Vec<String>>> = OnceLock::new();
static RELOAD_LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();

fn record_diagnostic(message: String) {
    let diagnostics = DIAGNOSTICS.get_or_init(|| std::sync::Mutex::new(Vec::new()));
    if let Ok(mut items) = diagnostics.lock() {
        items.push(message);
    }
}

/// Реестр навыков — грузится один раз при первом обращении (OnceLock). Живёт весь
/// процесс, поэтому раздаёт `&'static`-ссылки. Обновление файлов — после рестарта.
pub fn snapshot() -> Arc<SkillRegistrySnapshot> {
    REGISTRY
        .get_or_init(|| RwLock::new(Arc::new(build_snapshot(1))))
        .read()
        .map(|snapshot| snapshot.clone())
        .unwrap_or_else(|_| Arc::new(empty_snapshot()))
}

pub fn skills() -> Arc<Vec<Skill>> {
    snapshot().skills.clone()
}

fn empty_snapshot() -> SkillRegistrySnapshot {
    SkillRegistrySnapshot {
        generation: 0,
        catalog_digest: String::new(),
        loaded_at: Utc::now().to_rfc3339(),
        skills: Arc::new(Vec::new()),
        diagnostics: vec!["Skill registry lock is unavailable".to_string()],
    }
}

fn catalog_digest(skills: &[Skill]) -> String {
    let mut rows = skills
        .iter()
        .map(|skill| format!("{}:{}", skill.id, skill.digest))
        .collect::<Vec<_>>();
    rows.sort();
    format!("{:x}", Sha256::digest(rows.join("\n").as_bytes()))
}

fn build_snapshot(generation: u64) -> SkillRegistrySnapshot {
    let skills = load_registry();
    let diagnostics = current_diagnostics();
    SkillRegistrySnapshot {
        generation,
        catalog_digest: catalog_digest(&skills),
        loaded_at: Utc::now().to_rfc3339(),
        skills: Arc::new(skills),
        diagnostics,
    }
}

fn snapshot_diff(
    previous: &SkillRegistrySnapshot,
    next: &SkillRegistrySnapshot,
) -> SkillCatalogDiff {
    let before = previous
        .skills
        .iter()
        .map(|skill| (skill.id.as_str(), skill.digest.as_str()))
        .collect::<HashMap<_, _>>();
    let after = next
        .skills
        .iter()
        .map(|skill| (skill.id.as_str(), skill.digest.as_str()))
        .collect::<HashMap<_, _>>();
    let mut added = after
        .keys()
        .filter(|id| !before.contains_key(**id))
        .map(|id| (*id).to_string())
        .collect::<Vec<_>>();
    let mut changed = after
        .iter()
        .filter(|(id, digest)| before.get(**id).is_some_and(|old| old != *digest))
        .map(|(id, _)| (*id).to_string())
        .collect::<Vec<_>>();
    let mut removed = before
        .keys()
        .filter(|id| !after.contains_key(**id))
        .map(|id| (*id).to_string())
        .collect::<Vec<_>>();
    added.sort();
    changed.sort();
    removed.sort();
    SkillCatalogDiff {
        added,
        changed,
        removed,
    }
}

async fn write_reload_audit(
    actor_user_id: Option<&str>,
    previous: &SkillRegistrySnapshot,
    next: &SkillRegistrySnapshot,
    status: &str,
    diff: &SkillCatalogDiff,
) {
    use sea_orm::{ConnectionTrait, DatabaseBackend, Statement};
    let db = crate::shared::data::db::get_connection();
    let result = db
        .execute(Statement::from_sql_and_values(
            DatabaseBackend::Sqlite,
            "INSERT INTO sys_llm_skill_reload_log \
             (actor_user_id, previous_digest, new_digest, generation, status, diff_json, diagnostics_json) \
             VALUES (?, ?, ?, ?, ?, ?, ?)",
            [
                actor_user_id.map(str::to_string).into(),
                previous.catalog_digest.clone().into(),
                next.catalog_digest.clone().into(),
                (next.generation as i64).into(),
                status.to_string().into(),
                serde_json::to_string(diff).unwrap_or_default().into(),
                serde_json::to_string(&next.diagnostics).unwrap_or_default().into(),
            ],
        ))
        .await;
    if let Err(error) = result {
        tracing::warn!("[skills] failed to write reload audit: {error}");
    }
}

/// Scan, validate and atomically activate a new catalog snapshot.
pub async fn reload(actor_user_id: Option<String>) -> Value {
    let _guard = RELOAD_LOCK
        .get_or_init(|| tokio::sync::Mutex::new(()))
        .lock()
        .await;
    let previous = snapshot();
    let next_generation = previous.generation.saturating_add(1);
    let loaded = tokio::task::spawn_blocking(move || build_snapshot(next_generation)).await;
    let next = match loaded {
        Ok(snapshot) => Arc::new(snapshot),
        Err(error) => {
            let diff = SkillCatalogDiff {
                added: Vec::new(),
                changed: Vec::new(),
                removed: Vec::new(),
            };
            let mut rejected = (*previous).clone();
            rejected
                .diagnostics
                .push(format!("Catalog worker failed: {error}"));
            write_reload_audit(
                actor_user_id.as_deref(),
                &previous,
                &rejected,
                "rejected",
                &diff,
            )
            .await;
            return json!({
                "ok": false,
                "status": "rejected",
                "generation": previous.generation,
                "catalog_digest": previous.catalog_digest,
                "diagnostics": rejected.diagnostics,
                "added": [], "changed": [], "removed": [],
            });
        }
    };
    let diff = snapshot_diff(&previous, &next);
    if next.skills.is_empty() {
        write_reload_audit(
            actor_user_id.as_deref(),
            &previous,
            &next,
            "rejected",
            &diff,
        )
        .await;
        return json!({
            "ok": false,
            "status": "rejected",
            "generation": previous.generation,
            "catalog_digest": previous.catalog_digest,
            "diagnostics": next.diagnostics,
            "added": diff.added, "changed": diff.changed, "removed": diff.removed,
        });
    }
    if next
        .diagnostics
        .iter()
        .any(|message| message.starts_with("CRITICAL:"))
    {
        write_reload_audit(
            actor_user_id.as_deref(),
            &previous,
            &next,
            "rejected",
            &diff,
        )
        .await;
        return json!({
            "ok": false,
            "status": "rejected",
            "generation": previous.generation,
            "catalog_digest": previous.catalog_digest,
            "diagnostics": next.diagnostics,
            "added": diff.added, "changed": diff.changed, "removed": diff.removed,
        });
    }
    let activated = REGISTRY
        .get_or_init(|| RwLock::new(previous.clone()))
        .write()
        .map(|mut current| *current = next.clone())
        .is_ok();
    let status = if activated { "activated" } else { "rejected" };
    write_reload_audit(actor_user_id.as_deref(), &previous, &next, status, &diff).await;
    json!({
        "ok": activated,
        "status": status,
        "generation": if activated { next.generation } else { previous.generation },
        "catalog_digest": if activated { &next.catalog_digest } else { &previous.catalog_digest },
        "total": next.skills.len(),
        "tasks": next.skills.iter().map(|skill| skill.tasks.len()).sum::<usize>(),
        "resources": next.skills.iter().map(|skill| skill.resources.len()).sum::<usize>(),
        "diagnostics": next.diagnostics,
        "added": diff.added,
        "changed": diff.changed,
        "removed": diff.removed,
    })
}

fn clear_diagnostics() {
    if let Ok(mut items) = DIAGNOSTICS
        .get_or_init(|| std::sync::Mutex::new(Vec::new()))
        .lock()
    {
        items.clear();
    }
}

fn current_diagnostics() -> Vec<String> {
    DIAGNOSTICS
        .get()
        .and_then(|items| items.lock().ok().map(|items| items.clone()))
        .unwrap_or_default()
}

fn load_registry() -> Vec<Skill> {
    clear_diagnostics();
    let known_tools = tool_universe_names();
    let mut map: HashMap<String, Skill> = HashMap::new();

    // 1. Встроенный seed — всегда доступен.
    for (index, raw) in EMBEDDED_SKILL_FILES.iter().enumerate() {
        if let Some(skill) = parse_skill(raw, &known_tools, None, "embedded") {
            map.insert(skill.id.clone(), skill);
        } else {
            record_diagnostic(format!(
                "Встроенный skill #{} не прошёл минимальную валидацию",
                index + 1
            ));
        }
    }
    let embedded = map.len();

    // 2. Bundled package directory next to the binary, then configured external
    // catalog. The configured catalog has the final override precedence.
    let mut external = 0usize;
    if let Some(dir) = bundled_skills_dir().filter(|dir| dir.exists()) {
        external += load_skill_directory(&dir, "bundled", &known_tools, &mut map);
    }
    if let Some(dir) = skills_dir() {
        if dir.exists() {
            let is_bundled = bundled_skills_dir().and_then(|path| path.canonicalize().ok())
                == dir.canonicalize().ok();
            if !is_bundled {
                external += load_skill_directory(&dir, "external", &known_tools, &mut map);
            }
        } else {
            record_diagnostic(format!(
                "CRITICAL: external skills directory '{}' was not found",
                dir.display()
            ));
            tracing::info!(
                "[skills] внешний каталог '{}' не найден — только встроенные",
                dir.display()
            );
        }
    }

    let mut list: Vec<Skill> = map.into_values().collect();
    list.sort_by(|a, b| a.id.cmp(&b.id)); // детерминированный порядок
    if list.is_empty() {
        tracing::warn!("[skills] реестр пуст — чат будет только с core-инструментами");
    } else {
        tracing::info!(
            "[skills] loaded {} skills ({} embedded, {} external)",
            list.len(),
            embedded,
            external
        );
    }
    list
}

fn load_skill_directory(
    dir: &Path,
    origin: &str,
    known_tools: &HashSet<String>,
    map: &mut HashMap<String, Skill>,
) -> usize {
    let mut loaded = 0usize;
    for path in discover_skill_entries(dir) {
        match std::fs::read_to_string(&path) {
            Ok(raw) => {
                let package_root = (path.file_name().and_then(|name| name.to_str())
                    == Some("SKILL.md"))
                .then(|| path.parent().unwrap_or(dir).to_path_buf());
                match parse_skill(&raw, known_tools, package_root.as_deref(), origin) {
                    Some(skill) => {
                        if map
                            .get(&skill.id)
                            .is_some_and(|previous| previous.origin != "embedded")
                        {
                            record_diagnostic(format!(
                                "Duplicate catalog skill '{}': later package '{}' takes precedence",
                                skill.id,
                                path.display()
                            ));
                        }
                        map.insert(skill.id.clone(), skill);
                        loaded += 1;
                    }
                    None => {
                        let message = format!(
                            "Skill file was skipped after validation: {}",
                            path.display()
                        );
                        tracing::warn!("[skills] {message}");
                        record_diagnostic(message);
                    }
                }
            }
            Err(error) => {
                let message = format!("Cannot read {}: {}", path.display(), error);
                tracing::warn!("[skills] {message}");
                record_diagnostic(message);
            }
        }
    }
    loaded
}

/// Разобрать один skill-файл (frontmatter + тело). None, если нет frontmatter/id.
fn parse_skill(
    raw: &str,
    known_tools: &HashSet<String>,
    package_root: Option<&Path>,
    origin: &str,
) -> Option<Skill> {
    let (frontmatter, body) = split_frontmatter(raw);
    let frontmatter = frontmatter?;
    let manifest: SkillManifest = if package_root.is_some() {
        match serde_yaml::from_str(&frontmatter) {
            Ok(manifest) => manifest,
            Err(error) => {
                record_diagnostic(format!("Skill manifest YAML is invalid: {error}"));
                return None;
            }
        }
    } else {
        SkillManifest {
            id: parse_scalar(&frontmatter, "id")?,
            title: parse_scalar(&frontmatter, "title").unwrap_or_default(),
            description: parse_scalar(&frontmatter, "description").unwrap_or_default(),
            intents: parse_list(&frontmatter, "intents").unwrap_or_default(),
            tools: parse_list(&frontmatter, "tools").unwrap_or_default(),
            allowed_for: parse_list(&frontmatter, "allowed_for").unwrap_or_default(),
            default_for: parse_list(&frontmatter, "default_for").unwrap_or_default(),
            resources: Vec::new(),
            tasks: Vec::new(),
        }
    };
    let id = manifest.id;
    if id.trim().is_empty()
        || !id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        tracing::warn!("[skills] некорректный id '{}': файл пропущен", id);
        record_diagnostic(format!("Некорректный skill id '{}': файл пропущен", id));
        return None;
    }
    let mut compatibility_warnings = Vec::new();
    if !manifest.allowed_for.is_empty() || !manifest.default_for.is_empty() {
        let warning = format!(
            "Skill '{}': allowed_for/default_for are compatibility metadata only; runtime access comes from the specialization matrix",
            id
        );
        record_diagnostic(warning.clone());
        compatibility_warnings.push(warning);
    }
    let mut tool_names = manifest.tools;
    tool_names.retain(|t| {
        if !known_tools.contains(t) {
            tracing::warn!(
                "[skills] '{}' ссылается на неизвестный инструмент '{}': инструмент исключён",
                id,
                t
            );
            record_diagnostic(format!(
                "Skill '{}' содержит неизвестный tool '{}': tool исключён",
                id, t
            ));
            false
        } else {
            true
        }
    });
    let canonical_root = package_root.and_then(|root| match root.canonicalize() {
        Ok(root) => Some(root),
        Err(error) => {
            record_diagnostic(format!(
                "Skill '{}': package root '{}' cannot be resolved: {}",
                id,
                root.display(),
                error
            ));
            None
        }
    });
    let (resources, tasks, digest) = if let Some(root) = canonical_root.as_deref() {
        load_package_members(&id, root, manifest.resources, manifest.tasks)
    } else {
        (
            Vec::new(),
            Vec::new(),
            format!("{:x}", Sha256::digest(raw.as_bytes())),
        )
    };
    Some(Skill {
        title: if manifest.title.trim().is_empty() {
            id.clone()
        } else {
            manifest.title
        },
        description: manifest.description,
        intents: manifest.intents,
        prompt: body.trim_start().to_string(),
        tool_names,
        resources,
        tasks,
        package_root: canonical_root,
        origin: origin.to_string(),
        digest,
        compatibility_warnings,
        id,
    })
}

/// Роли из frontmatter → AgentType. Неизвестные — warn + пропуск (чтобы опечатка
/// не превратилась молча в business_analyst через `AgentType::from_str`).
fn normalize_relative_path(path: &str) -> Option<String> {
    let path = path.replace('\\', "/");
    let candidate = Path::new(&path);
    if path.trim().is_empty()
        || candidate.is_absolute()
        || candidate
            .components()
            .any(|part| !matches!(part, std::path::Component::Normal(_)))
    {
        return None;
    }
    Some(path)
}

pub(crate) fn resolve_package_file(root: &Path, relative: &str) -> Option<PathBuf> {
    let relative = normalize_relative_path(relative)?;
    let path = root.join(relative).canonicalize().ok()?;
    path.starts_with(root).then_some(path)
}

fn read_json_schema(skill_id: &str, root: &Path, relative: Option<String>) -> Option<Value> {
    let relative = relative?;
    let Some(path) = resolve_package_file(root, &relative) else {
        record_diagnostic(format!(
            "Skill '{}': input schema path '{}' is unsafe or missing",
            skill_id, relative
        ));
        return None;
    };
    match std::fs::read_to_string(&path)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
    {
        Some(schema) => Some(schema),
        None => {
            record_diagnostic(format!(
                "Skill '{}': input schema '{}' is not valid JSON",
                skill_id, relative
            ));
            None
        }
    }
}

fn collect_package_files(root: &Path, relative_dir: &str) -> Vec<String> {
    let start = root.join(relative_dir);
    if !start.is_dir() {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut stack = vec![start];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if let Ok(relative) = path.strip_prefix(root) {
                out.push(relative.to_string_lossy().replace('\\', "/"));
            }
        }
    }
    out.sort();
    out
}

fn load_package_members(
    skill_id: &str,
    root: &Path,
    declared_resources: Vec<String>,
    declared_tasks: Vec<SkillTaskManifest>,
) -> (Vec<SkillResource>, Vec<SkillTask>, String) {
    let mut resource_paths = declared_resources;
    resource_paths.extend(collect_package_files(root, "references"));
    resource_paths.extend(collect_package_files(root, "examples"));
    resource_paths.sort();
    resource_paths.dedup();
    let resources = resource_paths
        .into_iter()
        .filter_map(|relative| {
            let normalized = normalize_relative_path(&relative)?;
            let Some(path) = resolve_package_file(root, &normalized) else {
                record_diagnostic(format!(
                    "Skill '{}': resource '{}' is unsafe or missing",
                    skill_id, relative
                ));
                return None;
            };
            if path.metadata().map(|meta| meta.len()).unwrap_or(u64::MAX) > 256 * 1024 {
                record_diagnostic(format!(
                    "Skill '{}': resource '{}' exceeds the 256 KiB limit",
                    skill_id, relative
                ));
                return None;
            }
            let content = match std::fs::read_to_string(&path) {
                Ok(content) => content,
                Err(error) => {
                    record_diagnostic(format!(
                        "Skill '{}': resource '{}' is not UTF-8 text: {}",
                        skill_id, relative, error
                    ));
                    return None;
                }
            };
            Some(SkillResource {
                kind: normalized
                    .split('/')
                    .next()
                    .unwrap_or("resource")
                    .to_string(),
                path: normalized,
                content,
            })
        })
        .collect::<Vec<_>>();

    let task_manifests = if declared_tasks.is_empty() {
        collect_package_files(root, "scripts")
            .into_iter()
            .filter(|path| path.ends_with(".js") || path.ends_with(".mjs"))
            .map(|entrypoint| {
                let id = Path::new(&entrypoint)
                    .file_stem()
                    .and_then(|name| name.to_str())
                    .unwrap_or("task")
                    .replace('_', "-");
                SkillTaskManifest {
                    title: id.clone(),
                    id,
                    runtime: default_task_runtime(),
                    entrypoint,
                    export: default_task_export(),
                    mode: default_task_mode(),
                    input_schema: None,
                    capabilities: Vec::new(),
                }
            })
            .collect()
    } else {
        declared_tasks
    };
    let mut seen = HashSet::new();
    let tasks = task_manifests
        .into_iter()
        .filter_map(|task| {
            if task.id.trim().is_empty()
                || !task
                    .id
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
                || !seen.insert(task.id.clone())
            {
                record_diagnostic(format!(
                    "Skill '{}': task id '{}' is invalid or duplicated",
                    skill_id, task.id
                ));
                return None;
            }
            if task.runtime != "javascript" {
                record_diagnostic(format!(
                    "Skill '{}': task '{}' uses unsupported runtime '{}'",
                    skill_id, task.id, task.runtime
                ));
                return None;
            }
            if !matches!(task.mode.as_str(), "stable" | "development") {
                record_diagnostic(format!(
                    "Skill '{}': task '{}' uses unsupported mode '{}'",
                    skill_id, task.id, task.mode
                ));
                return None;
            }
            if task.export.trim().is_empty()
                || !task.export.chars().enumerate().all(|(index, ch)| {
                    ch.is_ascii_alphabetic()
                        || ch == '_'
                        || ch == '$'
                        || (index > 0 && ch.is_ascii_digit())
                })
            {
                record_diagnostic(format!(
                    "Skill '{}': task '{}' export '{}' is invalid",
                    skill_id, task.id, task.export
                ));
                return None;
            }
            if let Some(capability) = task.capabilities.iter().find(|capability| {
                matches!(
                    contracts::plugins::PluginCapability::parse(capability),
                    contracts::plugins::PluginCapability::Unknown(_)
                )
            }) {
                record_diagnostic(format!(
                    "Skill '{}': task '{}' declares unknown capability '{}'",
                    skill_id, task.id, capability
                ));
                return None;
            }
            let entrypoint = normalize_relative_path(&task.entrypoint)?;
            let script_path = resolve_package_file(root, &entrypoint);
            if !(entrypoint.ends_with(".js") || entrypoint.ends_with(".mjs"))
                || script_path.is_none()
            {
                record_diagnostic(format!(
                    "Skill '{}': task '{}' entrypoint '{}' is unsafe, missing, or not JavaScript",
                    skill_id, task.id, task.entrypoint
                ));
                return None;
            }
            let script_path = script_path?;
            if script_path
                .metadata()
                .map(|meta| meta.len())
                .unwrap_or(u64::MAX)
                > 512 * 1024
            {
                record_diagnostic(format!(
                    "Skill '{}': task '{}' exceeds the 512 KiB script limit",
                    skill_id, task.id
                ));
                return None;
            }
            let source = match std::fs::read_to_string(&script_path) {
                Ok(source) => source,
                Err(error) => {
                    record_diagnostic(format!(
                        "Skill '{}': task '{}' cannot be read: {}",
                        skill_id, task.id, error
                    ));
                    return None;
                }
            };
            let export_markers = [
                format!("export function {}", task.export),
                format!("export async function {}", task.export),
                format!("export const {}", task.export),
                format!("export let {}", task.export),
                format!("export var {}", task.export),
            ];
            if !export_markers.iter().any(|marker| source.contains(marker)) {
                record_diagnostic(format!(
                    "Skill '{}': task '{}' entrypoint does not declare export '{}'",
                    skill_id, task.id, task.export
                ));
                return None;
            }
            let input_schema_path = task
                .input_schema
                .as_deref()
                .and_then(normalize_relative_path);
            let input_schema = if task.input_schema.is_some() {
                match read_json_schema(skill_id, root, input_schema_path.clone()) {
                    Some(schema) => {
                        if let Err(error) = jsonschema::validator_for(&schema) {
                            record_diagnostic(format!(
                                "Skill '{}': task '{}' input schema cannot be compiled: {}",
                                skill_id, task.id, error
                            ));
                            return None;
                        }
                        Some(schema)
                    }
                    None => return None,
                }
            } else {
                None
            };
            Some(SkillTask {
                title: if task.title.trim().is_empty() {
                    task.id.clone()
                } else {
                    task.title
                },
                input_schema,
                input_schema_path,
                id: task.id,
                runtime: task.runtime,
                entrypoint,
                export: task.export,
                mode: task.mode,
                capabilities: task.capabilities,
                source,
            })
        })
        .collect::<Vec<_>>();

    let mut digest_files = vec!["SKILL.md".to_string()];
    digest_files.extend(resources.iter().map(|resource| resource.path.clone()));
    digest_files.extend(tasks.iter().map(|task| task.entrypoint.clone()));
    digest_files.extend(
        tasks
            .iter()
            .filter_map(|task| task.input_schema_path.clone()),
    );
    digest_files.sort();
    digest_files.dedup();
    let mut hasher = Sha256::new();
    for relative in digest_files {
        if let Some(path) = resolve_package_file(root, &relative) {
            hasher.update(relative.as_bytes());
            if let Ok(bytes) = std::fs::read(path) {
                hasher.update(bytes);
            }
        }
    }
    (resources, tasks, format!("{:x}", hasher.finalize()))
}

/// Имена всех инструментов «вселенной» — для валидации `tools:` в навыках.
fn tool_universe_names() -> HashSet<String> {
    tool_universe().into_iter().map(|d| d.name).collect()
}

/// Путь к внешнему каталогу навыков из конфига (None при ошибке конфига).
fn bundled_skills_dir() -> Option<PathBuf> {
    let executable = std::env::current_exe().ok()?;
    let mut root = executable.parent()?.to_path_buf();
    if root.file_name().and_then(|name| name.to_str()) == Some("deps") {
        root = root.parent()?.to_path_buf();
    }
    Some(root.join("skills"))
}

/// Путь к внешнему каталогу навыков из конфига (None при ошибке конфига).
fn skills_dir() -> Option<PathBuf> {
    match crate::shared::config::load_config() {
        Ok(cfg) => Some(crate::shared::config::get_skills_path(&cfg)),
        Err(e) => {
            record_diagnostic(format!("CRITICAL: skill configuration is unavailable: {e}"));
            tracing::warn!(
                "[skills] конфиг не загружен ({}); внешний каталог отключён",
                e
            );
            None
        }
    }
}

/// Рекурсивный сбор `*.md` (как в knowledge_base).
fn discover_skill_entries(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        let entries = match std::fs::read_dir(&current) {
            Ok(e) => e,
            Err(e) => {
                record_diagnostic(format!(
                    "{}skill directory '{}' cannot be read: {}",
                    if current == dir { "CRITICAL: " } else { "" },
                    current.display(),
                    e
                ));
                tracing::warn!(
                    "[skills] каталог '{}' не прочитан: {}",
                    current.display(),
                    e
                );
                continue;
            }
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.file_name().and_then(|x| x.to_str()) == Some("SKILL.md")
                || (path.parent() == Some(dir)
                    && path.extension().and_then(|x| x.to_str()) == Some("md"))
            {
                files.push(path);
            }
        }
    }
    // Legacy root files are loaded first; package SKILL.md overrides them by id.
    files.sort_by_key(|path| {
        (
            path.file_name().and_then(|name| name.to_str()) == Some("SKILL.md"),
            path.clone(),
        )
    });
    files
}

#[cfg(test)]
pub(crate) fn load_package_for_test(root: &Path) -> Skill {
    let raw = std::fs::read_to_string(root.join("SKILL.md")).expect("read test SKILL.md");
    parse_skill(&raw, &tool_universe_names(), Some(root), "test").expect("parse test skill package")
}

// ─── Мета-инструменты (Фаза B: progressive disclosure) ───────────────────────

/// Определения мета-инструментов навыков (входят в core).
pub fn meta_tool_definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "list_skills".into(),
            description: "Список доступных навыков (id, title, description). Вызови, если для задачи \
                          нужен набор возможностей, которого нет в текущих инструментах."
                .into(),
            parameters: json!({ "type": "object", "properties": {} }),
        },
        ToolDefinition {
            name: "use_skill".into(),
            description: "Активировать навык по id (из list_skills): добавит его инструменты и \
                          инструкции в текущий диалог. Активируй по одному навыку под текущую задачу."
                .into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "id": { "type": "string", "description": "id навыка, напр. 'plugin-authoring'." }
                },
                "required": ["id"]
            }),
        },
        ToolDefinition {
            name: "list_skill_resources".into(),
            description: "List references, examples and JavaScript tasks bundled with an active skill."
                .into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "skill_id": { "type": "string" }
                },
                "required": ["skill_id"]
            }),
        },
        ToolDefinition {
            name: "read_skill_resource".into(),
            description: "Read a declared reference/example from an active skill package.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "skill_id": { "type": "string" },
                    "path": { "type": "string" },
                    "offset": {
                        "type": "integer",
                        "minimum": 0,
                        "description": "Character offset for paged reading. Start with 0."
                    },
                    "max_chars": {
                        "type": "integer",
                        "minimum": 1,
                        "maximum": 32000,
                        "description": "Maximum characters to return; defaults to 16000."
                    }
                },
                "required": ["skill_id", "path"]
            }),
        },
        ToolDefinition {
            name: "run_skill_task".into(),
            description: "Run a declared JavaScript task from an active skill package. Pass task input as JSON."
                .into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "skill_id": { "type": "string" },
                    "task_id": { "type": "string" },
                    "input": {}
                },
                "required": ["skill_id", "task_id"]
            }),
        },
    ]
}

/// Результат `list_skills` — только разрешённые агенту навыки.
pub async fn list_skills_result(agent_type: &AgentType) -> Value {
    let policy = match super::skill_policy::accesses_for(agent_type).await {
        Ok(policy) => policy,
        Err(error) => {
            tracing::error!("[skills] policy unavailable: {}", error);
            return json!({ "error": "Политика навыков временно недоступна.", "_ok": false });
        }
    };
    list_skills_result_from(&snapshot(), &policy)
}

pub fn list_skills_result_from(
    snapshot: &SkillRegistrySnapshot,
    policy: &HashMap<String, super::skill_policy::SkillAccessLevel>,
) -> Value {
    use super::skill_policy::SkillAccessLevel;
    let to_json = |level| {
        snapshot
            .skills
            .iter()
            .filter(|s| policy.get(&s.id) == Some(&level))
            .map(|s| json!({ "id": s.id, "title": s.title, "description": s.description }))
            .collect::<Vec<_>>()
    };
    json!({
        "immediate": to_json(SkillAccessLevel::Immediate),
        "extended": to_json(SkillAccessLevel::Extended),
        "hint": "Активируй нужный навык: use_skill(\"<id>\")."
    })
}

/// Результат `use_skill` — сигнал активации (`_activate_skill`) ловится tool-циклом.
pub async fn use_skill_result(arguments_json: &str, agent_type: &AgentType) -> Value {
    let snapshot = snapshot();
    let policy = match super::skill_policy::accesses_for_snapshot(agent_type, &snapshot).await {
        Ok(policy) => policy,
        Err(error) => {
            tracing::error!("[skills] policy unavailable: {}", error);
            return json!({ "error": "Политика навыков временно недоступна.", "_ok": false });
        }
    };
    use_skill_result_from(arguments_json, &snapshot, &policy)
}

pub fn use_skill_result_from(
    arguments_json: &str,
    snapshot: &SkillRegistrySnapshot,
    policy: &HashMap<String, super::skill_policy::SkillAccessLevel>,
) -> Value {
    let id = serde_json::from_str::<Value>(arguments_json)
        .ok()
        .and_then(|v| v.get("id").and_then(|x| x.as_str()).map(str::to_string))
        .unwrap_or_default();
    let Some(skill) = skill_by_id_in(snapshot, &id) else {
        return json!({ "error": format!("Навык '{}' не найден. Вызови list_skills.", id) });
    };
    let level = policy
        .get(&skill.id)
        .copied()
        .unwrap_or(super::skill_policy::SkillAccessLevel::Denied);
    if level == super::skill_policy::SkillAccessLevel::Denied {
        return json!({ "error": format!("Навык '{}' недоступен для текущего агента.", skill.id) });
    }
    let inefficient = level == super::skill_policy::SkillAccessLevel::Extended;
    json!({
        "_activate_skill": skill.id.clone(),
        "_skill_access_level": level.as_str(),
        "_skill_inefficiency": inefficient,
        "_skill_digest": skill.digest,
        "_registry_generation": snapshot.generation,
        "activated": skill.title.clone(),
        "hint": if inefficient {
            "Расширенный навык активирован. В истории будет отмечена неэффективность специализации."
        } else {
            "Навык активирован: его инструменты и инструкции доступны со следующего шага."
        }
    })
}

// ─── Поиск/маппинг ───────────────────────────────────────────────────────────

pub fn skill_by_id(id: &str) -> Option<Skill> {
    skill_by_id_in(&snapshot(), id)
}

pub fn skill_by_id_in(snapshot: &SkillRegistrySnapshot, id: &str) -> Option<Skill> {
    snapshot.skills.iter().find(|s| s.id == id).cloned()
}

/// Имя ресурса, которым навык объявляет свой шаблон анкеты.
pub const INTAKE_TEMPLATE_RESOURCE: &str = "intake.md";

/// Шаблон анкеты от активных навыков (первый найденный).
///
/// У финансиста и маркетолога разные значимые параметры задачи, и держать их
/// перечень в коде значит фиксировать домен в бинаре. Шаблон — обычный ресурс
/// пакета навыка: правится текстом и подхватывается через reload.
pub fn intake_template_in(
    snapshot: &SkillRegistrySnapshot,
    active_skill_ids: &HashSet<String>,
) -> Option<String> {
    snapshot
        .skills
        .iter()
        .filter(|skill| active_skill_ids.contains(&skill.id))
        .find_map(|skill| {
            skill
                .resources
                .iter()
                .find(|resource| resource.path == INTAKE_TEMPLATE_RESOURCE)
                .map(|resource| resource.content.clone())
        })
}

/// Навык по интенту роутера (первое совпадение).
pub fn skill_for_intent(intent: &str) -> Option<Skill> {
    skill_for_intent_in(&snapshot(), intent)
}

pub fn skill_for_intent_in(snapshot: &SkillRegistrySnapshot, intent: &str) -> Option<Skill> {
    snapshot
        .skills
        .iter()
        .find(|s| s.intents.iter().any(|i| i == intent))
        .cloned()
}

/// Навыки сотрудника по специализации для read-only блока в карточке:
/// `core` — активны по умолчанию (default-bias), `extended` — доступны по запросу
/// (через `use_skill`; факт запроса пишется в `sys_tool_trace`). Каждый навык — {id,title,description}.
pub async fn employee_skills(agent_type: &AgentType) -> serde_json::Value {
    use super::skill_policy::SkillAccessLevel;
    let policy = match super::skill_policy::accesses_for(agent_type).await {
        Ok(policy) => policy,
        Err(error) => return json!({ "error": error.to_string(), "core": [], "extended": [] }),
    };
    let to_obj = |id: &str| match skill_by_id(id) {
        Some(s) => serde_json::json!({
            "id": s.id.clone(),
            "title": s.title.clone(),
            "description": s.description.clone(),
        }),
        None => serde_json::json!({ "id": id, "title": id, "description": "" }),
    };
    let core_json: Vec<_> = policy
        .iter()
        .filter(|(_, level)| **level == SkillAccessLevel::Immediate)
        .map(|(id, _)| to_obj(id))
        .collect();
    let extended_json: Vec<_> = policy
        .iter()
        .filter(|(_, level)| **level == SkillAccessLevel::Extended)
        .map(|(id, _)| to_obj(id))
        .collect();
    serde_json::json!({
        "agent_type": agent_type.as_str(),
        "core": core_json,
        "extended": extended_json,
    })
}

// ─── Сборка инструментов/guard ───────────────────────────────────────────────

/// «Вселенная» всех определений инструментов (core + все бандлы + мета).
/// NB: при добавлении нового бандла обнови также `tools_catalog()` (там бандлы
/// перечислены заново парами `(category, defs)`, т.к. здесь метка категории теряется).
fn tool_universe() -> Vec<ToolDefinition> {
    let mut v = Vec::new();
    v.extend(super::data_tools::tool_definitions());
    v.extend(super::tool_executor::shared_tool_definitions());
    v.extend(super::tool_executor::analyst_tool_definitions());
    v.extend(super::admin_tools::admin_tool_definitions());
    v.extend(super::kb_tools::kb_tool_definitions());
    v.extend(super::kb_admin_tools::kb_admin_tool_definitions());
    v.extend(super::plugin_tools::plugin_tool_definitions());
    v.extend(super::chart_tools::chart_tool_definitions());
    v.extend(super::table_tools::table_tool_definitions());
    v.extend(super::mail_tools::mail_tool_definitions());
    v.extend(super::schedule_tools::schedule_tool_definitions());
    v.extend(super::ticket_tools::ticket_tool_definitions());
    v.extend(super::workspace_tools::workspace_tool_definitions());
    v.extend(super::quality_tools::quality_tool_definitions());
    v.extend(super::funnel_repair_tools::funnel_repair_tool_definitions());
    v.extend(super::llm_quality_tools::llm_quality_tool_definitions());
    v.extend(super::agent_task_tools::agent_task_tool_definitions());
    v.extend(meta_tool_definitions());
    v
}

/// Множество имён активных инструментов (core ∪ инструменты активных навыков).
/// Единый источник истины для авторизации в `execute_tool_call`.
pub fn active_tool_names<S: AsRef<str>>(active_skills: &[S]) -> HashSet<String> {
    active_tool_names_in(&snapshot(), active_skills)
}

pub fn active_tool_names_in<S: AsRef<str>>(
    snapshot: &SkillRegistrySnapshot,
    active_skills: &[S],
) -> HashSet<String> {
    let mut names: HashSet<String> = CORE_TOOLS.iter().map(|s| s.to_string()).collect();
    for id in active_skills {
        if let Some(s) = skill_by_id_in(snapshot, id.as_ref()) {
            for t in &s.tool_names {
                names.insert(t.clone());
            }
        }
    }
    names
}

/// Собрать определения инструментов для передачи LLM: core + активные навыки, дедуп по имени.
pub fn assemble_tools<S: AsRef<str>>(active_skills: &[S]) -> Vec<ToolDefinition> {
    assemble_tools_in(&snapshot(), active_skills)
}

pub fn assemble_tools_in<S: AsRef<str>>(
    snapshot: &SkillRegistrySnapshot,
    active_skills: &[S],
) -> Vec<ToolDefinition> {
    let wanted = if active_skills.len() == 1
        && matches!(active_skills[0].as_ref(), "chart-builder" | "table-builder")
    {
        skill_by_id_in(snapshot, active_skills[0].as_ref())
            .map(|skill| skill.tool_names.into_iter().collect())
            .unwrap_or_default()
    } else {
        active_tool_names_in(snapshot, active_skills)
    };
    let chart_builder = active_skills.len() == 1 && active_skills[0].as_ref() == "chart-builder";
    let mut seen: HashSet<String> = HashSet::new();
    let mut out = Vec::new();
    for mut def in tool_universe() {
        if wanted.contains(def.name.as_str()) && seen.insert(def.name.clone()) {
            // В общем data-analysis preview может быть без presentation. В chart-builder
            // это другой контракт: без chart невозможно получить build_ready, и модель
            // раньше зацикливалась на успешных, но бесполезных preview.
            if chart_builder && def.name == "preview_data" {
                def.parameters["required"] = serde_json::json!(["source", "chart"]);
                def.parameters["properties"]["chart"]["description"] = serde_json::json!(
                    "Обязательный presentation-спек. Preview должен вернуть build_ready перед build_chart."
                );
            }
            out.push(def);
        }
    }
    out
}

/// Базовый системный промпт (role-agnostic).
pub fn core_prompt() -> &'static str {
    CORE_PROMPT
}

/// Full lazy context injected only when a concrete skill is activated.
pub fn activation_prompt(skill: &Skill) -> String {
    let mut text = skill.prompt.clone();
    if !skill.resources.is_empty() {
        text.push_str("\n\nДоступные материалы пакета (читайте через read_skill_resource):\n");
        for resource in &skill.resources {
            text.push_str(&format!("- {}\n", resource.path));
        }
    }
    if !skill.tasks.is_empty() {
        text.push_str("\nJavaScript-задачи пакета (запускайте через run_skill_task):\n");
        for task in &skill.tasks {
            text.push_str(&format!(
                "- {} — {} (mode: {}; input_schema: {})\n",
                task.id,
                task.title,
                task.mode,
                task.input_schema
                    .as_ref()
                    .map(Value::to_string)
                    .unwrap_or_else(|| "any JSON".to_string())
            ));
        }
    }
    text
}

/// Нефатальная диагностика каталога для административного UI.
///
/// Подробные ошибки чтения остаются в tracing; effective registry уже содержит
/// только успешно разобранные skills и известные инструменты.
pub fn diagnostics() -> Vec<String> {
    snapshot().diagnostics.clone()
}

/// Каталог содержимого навыков. Политика специализаций отдаётся отдельной матрицей.
pub fn catalog() -> Value {
    let snapshot = snapshot();
    let items: Vec<Value> = snapshot
        .skills
        .iter()
        .map(|s| {
            json!({
                "id": s.id.clone(),
                "title": s.title.clone(),
                "description": s.description.clone(),
                "intents": s.intents.clone(),
                "tools": s.tool_names.clone(),
                "tool_count": s.tool_names.len(),
                "origin": s.origin,
                "digest": s.digest,
                "diagnostic_status": if s.compatibility_warnings.is_empty() { "ok" } else { "warning" },
                "compatibility_warnings": s.compatibility_warnings,
                "resources": s.resources.iter().map(|resource| json!({
                    "path": resource.path,
                    "kind": resource.kind,
                })).collect::<Vec<_>>(),
                "tasks": s.tasks.iter().map(|task| json!({
                    "id": task.id,
                    "title": task.title,
                    "runtime": task.runtime,
                    "mode": task.mode,
                    "input_schema": task.input_schema,
                })).collect::<Vec<_>>(),
            })
        })
        .collect();
    json!({
        "core_tools": CORE_TOOLS,
        "skills": items,
        "total": items.len(),
        "generation": snapshot.generation,
        "catalog_digest": snapshot.catalog_digest,
        "loaded_at": snapshot.loaded_at,
        "diagnostics": snapshot.diagnostics,
    })
}

/// Каталог инструментов для UI/обзора: полное определение каждого инструмента
/// (name, description, JSON-схема параметров) + производные поля (category, is_core,
/// список навыков, в которых он используется). Дедуп по имени.
///
/// NB: бандлы перечислены заново парами `(category, defs)` — при добавлении нового
/// бандла в `tool_universe()` продублируй его и здесь с меткой категории.
pub fn tools_catalog() -> Value {
    let bundles: Vec<(&str, Vec<ToolDefinition>)> = vec![
        ("data", super::data_tools::tool_definitions()),
        ("shared", super::tool_executor::shared_tool_definitions()),
        ("analyst", super::tool_executor::analyst_tool_definitions()),
        ("admin", super::admin_tools::admin_tool_definitions()),
        // Порядок важен: дедуп ниже оставляет ПЕРВОЕ вхождение имени, поэтому
        // бандл поиска должен идти до kb_admin, чтобы задать свою метку категории.
        ("kb_search", super::kb_tools::kb_tool_definitions()),
        ("kb", super::kb_admin_tools::kb_admin_tool_definitions()),
        ("plugin", super::plugin_tools::plugin_tool_definitions()),
        ("chart", super::chart_tools::chart_tool_definitions()),
        ("table", super::table_tools::table_tool_definitions()),
        ("mail", super::mail_tools::mail_tool_definitions()),
        (
            "schedule",
            super::schedule_tools::schedule_tool_definitions(),
        ),
        ("ticket", super::ticket_tools::ticket_tool_definitions()),
        (
            "workspace",
            super::workspace_tools::workspace_tool_definitions(),
        ),
        ("quality", super::quality_tools::quality_tool_definitions()),
        (
            "funnel_repair",
            super::funnel_repair_tools::funnel_repair_tool_definitions(),
        ),
        (
            "llm_quality",
            super::llm_quality_tools::llm_quality_tool_definitions(),
        ),
        (
            "agent_task",
            super::agent_task_tools::agent_task_tool_definitions(),
        ),
        ("meta", meta_tool_definitions()),
    ];

    let mut seen: HashSet<String> = HashSet::new();
    let mut tools: Vec<Value> = Vec::new();
    for (category, defs) in bundles {
        for def in defs {
            if !seen.insert(def.name.clone()) {
                continue;
            }
            let is_core = CORE_TOOLS.contains(&def.name.as_str());
            let skills: Vec<String> = skills()
                .iter()
                .filter(|s| s.tool_names.iter().any(|t| t == &def.name))
                .map(|s| s.id.clone())
                .collect();
            tools.push(json!({
                "name": def.name,
                "description": def.description,
                "parameters": def.parameters,
                "category": category,
                "is_core": is_core,
                "skills": skills,
            }));
        }
    }

    let total = tools.len();
    json!({ "tools": tools, "total": total })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn core_only_has_meta_and_base() {
        let names = active_tool_names::<&str>(&[]);
        assert!(names.contains("get_architecture_overview"));
        assert!(names.contains("list_skills"));
        assert!(names.contains("use_skill"));
        assert!(!names.contains("plugin_upsert"));
        assert!(!names.contains("execute_query"));
    }

    /// Инструменты каталога лежат в core: состояние задачи нужно любому навыку.
    /// Имя, отсутствующее в `tool_universe()`, выбрасывается молча — проверяем,
    /// что все они доехали до набора, который видит модель.
    #[test]
    fn workspace_tools_are_available_without_any_skill() {
        let tools = assemble_tools::<&str>(&[]);
        let names: HashSet<_> = tools.iter().map(|t| t.name.clone()).collect();
        for expected in super::super::workspace_tools::WORKSPACE_TOOL_NAMES {
            assert!(names.contains(*expected), "потерян инструмент {expected}");
        }
    }

    #[test]
    fn plugin_skill_brings_plugin_tools() {
        let tools = assemble_tools(&["plugin-authoring"]);
        let names: HashSet<_> = tools.iter().map(|t| t.name.clone()).collect();
        assert!(names.contains("plugin_upsert"));
        assert!(names.contains("execute_query")); // из b_sql
        assert!(names.contains("use_skill")); // core всегда
    }

    #[test]
    fn support_skill_brings_ticket_tools() {
        // Цепочка регистрации инструмента рвётся тихо: имя, отсутствующее в
        // `tool_universe()`, выбрасывается при парсинге навыка без ошибки сборки.
        let tools = assemble_tools(&["support"]);
        let names: HashSet<_> = tools.iter().map(|t| t.name.clone()).collect();
        for expected in super::super::ticket_tools::TICKET_TOOL_NAMES {
            assert!(names.contains(*expected), "потерян инструмент {expected}");
        }
    }

    #[test]
    fn quality_review_skill_brings_verdict_tools() {
        // Та же тихая цепочка: без этих трёх инструментов судья не сможет ни выбрать
        // диалоги, ни записать оценку, а задача «отработает» с пустой метрикой.
        let tools = assemble_tools(&["llm-quality-review"]);
        let names: HashSet<_> = tools.iter().map(|t| t.name.clone()).collect();
        for expected in super::super::llm_quality_tools::LLM_QUALITY_TOOL_NAMES {
            assert!(names.contains(*expected), "потерян инструмент {expected}");
        }
    }

    #[test]
    fn agent_delegation_skill_brings_task_tools() {
        // Та же тихая цепочка: имя, не доехавшее до `tool_universe()`, отбрасывается
        // при разборе навыка без ошибки сборки — и делегирование «работает», молча
        // не имея ни одного своего инструмента.
        let tools = assemble_tools(&["agent-delegation"]);
        let names: HashSet<_> = tools.iter().map(|t| t.name.clone()).collect();
        for expected in super::super::agent_task_tools::AGENT_TASK_TOOL_NAMES {
            assert!(names.contains(*expected), "потерян инструмент {expected}");
        }
        // Постановка поручения тратит реальный агентный прогон — она не должна
        // быть доступна без явного use_skill.
        let core = active_tool_names::<&str>(&[]);
        assert!(!core.contains("create_agent_task"));
    }

    #[test]
    fn kb_tools_registered_end_to_end() {
        // Та же тихая цепочка: пять переходов, и каждый рвётся без ошибки сборки.
        let core = active_tool_names::<&str>(&[]);
        assert!(core.contains("search_knowledge"));
        assert!(core.contains("get_knowledge"));
        assert!(
            core.contains("kb_report_issue"),
            "пожаловаться на статью должен уметь любой агент, иначе метрика неточностей пуста"
        );
        assert!(
            !core.contains("kb_propose_article"),
            "запись статей не должна быть core — она требует явного use_skill"
        );

        let names: HashSet<_> = assemble_tools(&["kb-authoring"])
            .into_iter()
            .map(|t| t.name)
            .collect();
        for expected in super::super::kb_tools::KB_TOOL_NAMES {
            assert!(names.contains(*expected), "потерян инструмент {expected}");
        }
    }

    #[test]
    fn kb_tools_present_in_tools_catalog() {
        // Отдельный переход: `tools_catalog()` перечисляет бандлы заново, и забытый
        // здесь инструмент пропадает из /api/llm-tools, оставаясь рабочим в чате.
        let catalog = tools_catalog();
        let listed: HashSet<&str> = catalog["tools"]
            .as_array()
            .expect("tools_catalog вернул не массив")
            .iter()
            .filter_map(|t| t["name"].as_str())
            .collect();
        for expected in super::super::kb_tools::KB_TOOL_NAMES {
            assert!(
                listed.contains(expected),
                "{expected} отсутствует в tools_catalog()"
            );
        }
    }

    #[test]
    fn assemble_dedups_shared_bundle() {
        // execute_query есть и в data-analytics, и в plugin-authoring — не должно дублироваться.
        let tools = assemble_tools(&["data-analytics", "plugin-authoring"]);
        let eq = tools.iter().filter(|t| t.name == "execute_query").count();
        assert_eq!(eq, 1);
    }

    #[test]
    fn data_skill_exposes_semantic_queries_and_raw_fallback() {
        let names: HashSet<_> = assemble_tools(&["data-analytics"])
            .into_iter()
            .map(|tool| tool.name)
            .collect();
        assert!(names.contains("list_data_sources"));
        assert!(names.contains("query_data_schema"));
        assert!(names.contains("run_data_view_scalar"));
        assert!(names.contains("run_data_view_drilldown"));
        assert!(names.contains("execute_query"));
    }

    #[test]
    fn intent_maps_to_skill() {
        assert_eq!(
            skill_for_intent("plugin_dev").unwrap().id,
            "plugin-authoring"
        );
        assert_eq!(
            skill_for_intent("quality_check").unwrap().id,
            "quality-monitoring"
        );
        assert_eq!(
            skill_for_intent("quality_check_dev").unwrap().id,
            "quality-check-authoring"
        );
        assert_eq!(skill_for_intent("data_query").unwrap().id, "data-analytics");
        assert_eq!(skill_for_intent("chart_build").unwrap().id, "chart-builder");
        assert_eq!(skill_for_intent("table_build").unwrap().id, "table-builder");
        assert_eq!(
            skill_for_intent("marketing_query").unwrap().id,
            "marketing-analytics"
        );
        assert_eq!(
            skill_for_intent("finance_query").unwrap().id,
            "finance-analytics"
        );
        assert_eq!(
            skill_for_intent("sales_query").unwrap().id,
            "sales-analytics"
        );
        assert!(skill_for_intent("meta_smalltalk").is_none());
    }

    #[test]
    fn builder_skills_expose_only_the_canonical_three_tools() {
        let chart: HashSet<_> = assemble_tools(&["chart-builder"])
            .into_iter()
            .map(|tool| tool.name)
            .collect();
        assert_eq!(
            chart,
            ["find_data_sources", "preview_data", "build_chart"]
                .into_iter()
                .map(str::to_string)
                .collect()
        );
        let table: HashSet<_> = assemble_tools(&["table-builder"])
            .into_iter()
            .map(|tool| tool.name)
            .collect();
        assert_eq!(
            table,
            ["find_data_sources", "preview_data", "build_table"]
                .into_iter()
                .map(str::to_string)
                .collect()
        );
        let preview = assemble_tools(&["chart-builder"])
            .into_iter()
            .find(|tool| tool.name == "preview_data")
            .expect("chart preview definition");
        assert_eq!(
            preview.parameters["required"],
            serde_json::json!(["source", "chart"])
        );
    }

    #[test]
    fn package_discovery_ignores_reference_markdown_and_indexes_js_tasks() {
        let root =
            std::env::temp_dir().join(format!("skill-package-test-{}", uuid::Uuid::new_v4()));
        let package = root.join("marketplace-funnel-analysis");
        std::fs::create_dir_all(package.join("references")).expect("references dir");
        std::fs::create_dir_all(package.join("scripts")).expect("scripts dir");
        std::fs::write(
            package.join("SKILL.md"),
            "---\nid: marketplace-funnel-analysis\ntitle: Funnel\n---\nUse the task.",
        )
        .expect("manifest");
        std::fs::write(
            package.join("references").join("funnel-model.md"),
            "# Funnel model",
        )
        .expect("reference");
        std::fs::write(
            package.join("scripts").join("calculate_funnel.mjs"),
            "export function run(args) { return args; }",
        )
        .expect("script");

        let entries = discover_skill_entries(&root);
        assert_eq!(entries, vec![package.join("SKILL.md")]);
        let raw = std::fs::read_to_string(&entries[0]).expect("manifest text");
        let skill = parse_skill(&raw, &tool_universe_names(), Some(&package), "external")
            .expect("package skill");
        assert_eq!(skill.resources.len(), 1);
        assert_eq!(skill.resources[0].path, "references/funnel-model.md");
        assert_eq!(skill.tasks.len(), 1);
        assert_eq!(skill.tasks[0].id, "calculate-funnel");
        assert_eq!(skill.tasks[0].mode, "development");

        std::fs::write(
            package.join("scripts").join("calculate_funnel.mjs"),
            "function hidden(args) { return args; }",
        )
        .expect("invalid script");
        let invalid_task_skill =
            parse_skill(&raw, &tool_universe_names(), Some(&package), "external")
                .expect("package remains valid when one task is invalid");
        assert!(invalid_task_skill.tasks.is_empty());

        std::fs::remove_dir_all(&root).expect("cleanup test package");
    }

    #[test]
    fn reference_funnel_package_is_complete() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("skills")
            .join("marketplace-funnel-analysis");
        let skill = load_package_for_test(&root);
        assert_eq!(skill.id, "marketplace-funnel-analysis");
        assert_eq!(skill.tasks.len(), 5);
        assert!(skill.tasks.iter().all(|task| task.input_schema.is_some()));
        assert!(skill
            .resources
            .iter()
            .any(|resource| resource.path == "examples/ozon.json"));
        assert!(skill
            .resources
            .iter()
            .any(|resource| resource.path == "references/repair-workflow.md"));
        for tool in [
            "prepare_funnel_repair",
            "execute_funnel_repair",
            "get_funnel_repair_status",
        ] {
            assert!(skill.tool_names.iter().any(|name| name == tool));
        }
    }

    /// Шаблон анкеты — обычный ресурс пакета: правится текстом и приезжает
    /// в новую задачу при активации навыка, без пересборки.
    #[test]
    fn finance_skill_ships_an_intake_template() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("skills")
            .join("finance-analytics");
        let skill = load_package_for_test(&root);
        assert_eq!(skill.id, "finance-analytics");
        assert!(skill
            .resources
            .iter()
            .any(|resource| resource.path == INTAKE_TEMPLATE_RESOURCE));

        let snapshot = SkillRegistrySnapshot {
            generation: 1,
            catalog_digest: "test".into(),
            loaded_at: "test".into(),
            skills: Arc::new(vec![skill]),
            diagnostics: Vec::new(),
        };
        let active: HashSet<String> = HashSet::from(["finance-analytics".to_string()]);
        let template = intake_template_in(&snapshot, &active).expect("шаблон анкеты");
        // Ключевые поля домена: без них анкета не отвечает на вопрос «что считаем».
        assert!(template.contains("period_from"));
        assert!(template.contains("layer"));

        // Навык не активен — шаблон не подставляется.
        assert!(intake_template_in(&snapshot, &HashSet::new()).is_none());
    }

    #[test]
    fn snapshot_diff_reports_added_changed_and_removed() {
        let mut first = parse_skill(
            EMBEDDED_SKILL_FILES[0],
            &tool_universe_names(),
            None,
            "embedded",
        )
        .unwrap();
        let mut changed = first.clone();
        changed.digest = "changed".into();
        let added = parse_skill(
            EMBEDDED_SKILL_FILES[1],
            &tool_universe_names(),
            None,
            "embedded",
        )
        .unwrap();
        let old = SkillRegistrySnapshot {
            generation: 1,
            catalog_digest: "old".into(),
            loaded_at: "old".into(),
            skills: Arc::new(vec![first.clone()]),
            diagnostics: Vec::new(),
        };
        let next = SkillRegistrySnapshot {
            generation: 2,
            catalog_digest: "new".into(),
            loaded_at: "new".into(),
            skills: Arc::new(vec![changed, added.clone()]),
            diagnostics: Vec::new(),
        };
        let diff = snapshot_diff(&old, &next);
        assert_eq!(diff.changed, vec![first.id.clone()]);
        assert_eq!(diff.added, vec![added.id]);
        assert!(diff.removed.is_empty());

        first.id = "removed".into();
        let removed_snapshot = SkillRegistrySnapshot {
            generation: 1,
            catalog_digest: "old".into(),
            loaded_at: "old".into(),
            skills: Arc::new(vec![first]),
            diagnostics: Vec::new(),
        };
        assert_eq!(
            snapshot_diff(&removed_snapshot, &next).removed,
            vec!["removed".to_string()]
        );
    }
}
