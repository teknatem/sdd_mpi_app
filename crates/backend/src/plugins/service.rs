//! Business logic for plugin CRUD, invocation, validation, import/export, and data bindings.

use super::repository;
use chrono::Utc;
use contracts::plugins::{
    PluginBundle, PluginDataMode, PluginDefinition, PluginError, PluginInvokeRequest,
    PluginSmokeFailure, PluginSmokeMethod, PluginSmokeReport, PluginSmokeRequest, PluginStatus,
    PluginUpsert, PluginValidateReport, WidgetKind,
};
use sea_orm::TransactionTrait;
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

fn db() -> &'static sea_orm::DatabaseConnection {
    crate::shared::data::db::get_connection()
}

pub async fn validate(bundle: &PluginBundle) -> PluginValidateReport {
    if let Err(message) = bundle.validate() {
        return PluginValidateReport {
            ok: false,
            errors: vec![PluginError::new("manifest", message)],
            ..Default::default()
        };
    }

    let mut report = PluginValidateReport {
        ok: true,
        ..Default::default()
    };

    // Серверный модуль: компиляция + перечень экспортов.
    if let Some(script) = &bundle.server_script {
        let server = super::engine::validate_server_script(script).await;
        report.server_exports = server.server_exports;
        report.errors.extend(server.errors);
        report.ok &= server.ok;
    }

    // Клиентский модуль (UI iframe): компиляция + проверка экспорта `mount`.
    if bundle.manifest.runtime.runs_on_client() {
        if let Some(script) = &bundle.client_script {
            let client = super::engine::validate_client_script(script).await;
            report.client_exports = client.client_exports;
            report.errors.extend(client.errors);
            report.ok &= client.ok;
        }
    }

    // SQL-ресурсы: тот же гард, что и при исполнении (`inspect_read_query`), — ловим
    // запрещённые конструкции (комментарии `--`/`/* */`, несколько стейтментов, не-SELECT,
    // `SELECT *` / `alias.*` при обращении к a006) ДО сохранения, а не при первом вызове
    // метода в рантайме. Гард идентичен исполнению, поэтому не отвергает рабочие запросы.
    let mut sql_names: Vec<&String> = bundle.sql_resources.keys().collect();
    sql_names.sort();
    for name in sql_names {
        let sql = &bundle.sql_resources[name];
        if let Err(message) = crate::shared::data_access::sql_guard::inspect_read_query(sql.trim())
        {
            report.errors.push(PluginError::new(
                "sql",
                format!("SQL-ресурс '{name}': {message}"),
            ));
            report.ok = false;
        }
    }

    report
}

pub(crate) struct PluginArtifactOrigin<'a> {
    pub chat_id: &'a str,
    pub agent_id: &'a str,
}

pub(crate) struct PluginUpsertResult {
    pub id: String,
    pub version: i32,
    pub artifact_id: Option<String>,
}

pub async fn upsert(dto: PluginUpsert) -> anyhow::Result<String> {
    Ok(upsert_prepared(dto, None, None, None, None).await?.id)
}

/// Transactional upsert with an optional result already resolved by a trusted
/// builder. This prevents executing the same source twice and guarantees that
/// the persisted snapshot is exactly the data that passed presentation checks.
pub(crate) async fn upsert_prepared(
    dto: PluginUpsert,
    prepared_snapshot: Option<serde_json::Value>,
    artifact_origin: Option<PluginArtifactOrigin<'_>>,
    prepared_validation: Option<PluginValidateReport>,
    // Явно задать итоговый номер версии (при обновлении/установке с сервера —
    // номер из каталога S3). `None` — обычное поведение: +1 при обновлении, 1 при вставке.
    version_override: Option<i32>,
) -> anyhow::Result<PluginUpsertResult> {
    let report = match prepared_validation {
        Some(report) => report,
        None => validate(&dto.bundle).await,
    };
    if !report.ok {
        return Err(anyhow::anyhow!(
            "Validation failed: {}",
            report
                .errors
                .first()
                .map(|error| error.to_string())
                .unwrap_or_else(|| "unknown plugin validation error".to_string())
        ));
    }
    let revision_bundle = dto.bundle.clone();
    let revision_agent_id = dto.created_by_agent_id.clone();
    let status = dto
        .status
        .as_deref()
        .map(PluginStatus::from_str)
        .unwrap_or(PluginStatus::Draft);
    let capture_snapshot = dto.capture_snapshot
        || (revision_bundle.data.source.is_some()
            && status == PluginStatus::Active
            && !dto.allow_live_only);
    let snapshot_payload = if capture_snapshot {
        match if let Some(payload) = prepared_snapshot {
            Ok(payload)
        } else {
            resolve_declarative_data(&dto.bundle, None).await
        } {
            Ok(payload) => Some(payload),
            Err(error) if dto.allow_live_only => {
                tracing::warn!("Declarative snapshot skipped: {error}");
                None
            }
            Err(error) => return Err(anyhow::anyhow!(error)),
        }
    } else {
        None
    };

    let txn = db().begin().await?;
    let id = match dto.id.as_deref() {
        Some(id) => update_existing(&txn, id.to_string(), dto, status, version_override).await,
        None => insert_new(&txn, dto, status, version_override).await,
    }?;
    let saved = repository::find_by_id(&txn, &id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("plugin disappeared during transactional upsert"))?;

    let source_json = saved
        .bundle
        .data
        .source
        .as_ref()
        .map(serde_json::to_string)
        .transpose()?;
    let source_hash = saved
        .bundle
        .data
        .source
        .as_ref()
        .map(super::data::source_hash);
    let mut snapshot_meta = None;
    if let Some(payload) = snapshot_payload.as_ref() {
        let row_limit = snapshot_row_limit(&saved.bundle);
        let (row_count, size_bytes) = super::data::validate_snapshot_payload(payload, row_limit)
            .map_err(anyhow::Error::msg)?;
        let hash = source_hash
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("declarative snapshot requires data.source"))?;
        snapshot_meta = Some(
            repository::upsert_snapshot(
                &txn,
                &id,
                saved.version,
                "data",
                payload,
                row_count,
                size_bytes,
                hash,
            )
            .await?,
        );
    }
    let snapshot_meta_json = snapshot_meta
        .as_ref()
        .map(serde_json::to_string)
        .transpose()?;
    let smoke_report = serde_json::json!({
        "ok": true,
        "validation": "passed",
        "source_resolved": saved.bundle.data.source.is_some(),
        "snapshot_created": snapshot_meta.is_some(),
    })
    .to_string();
    let (artifact_id, origin_json) = if let Some(origin) = artifact_origin {
        let (artifact_id, source_message_id) =
            crate::domain::a019_llm_artifact::repository::upsert_plugin_artifact(
                &txn,
                &id,
                &saved.bundle.manifest.code,
                &saved.bundle.manifest.title,
                origin.chat_id,
                origin.agent_id,
            )
            .await?;
        (
            Some(artifact_id.clone()),
            Some(
                serde_json::json!({
                    "chat_id": origin.chat_id,
                    "source_message_id": source_message_id,
                    "artifact_id": artifact_id,
                })
                .to_string(),
            ),
        )
    } else {
        (None, None)
    };
    repository::insert_revision(
        &txn,
        &id,
        saved.version,
        &revision_bundle,
        &report,
        Some(&smoke_report),
        revision_agent_id.as_deref(),
        source_json.as_deref(),
        source_hash.as_deref(),
        snapshot_meta_json.as_deref(),
        origin_json.as_deref(),
    )
    .await?;
    txn.commit().await?;
    super::change_token::TOKEN.bump();
    Ok(PluginUpsertResult {
        id,
        version: saved.version,
        artifact_id,
    })
}

async fn update_existing<C: sea_orm::ConnectionTrait>(
    conn: &C,
    id: String,
    dto: PluginUpsert,
    status: PluginStatus,
    version_override: Option<i32>,
) -> anyhow::Result<String> {
    let mut existing = repository::find_by_id(conn, &id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Plugin not found: {id}"))?;

    if let Some(expected) = dto.version {
        if expected != existing.version {
            return Err(anyhow::anyhow!(
                "Version conflict for plugin {id}: expected {expected}, actual {}",
                existing.version
            ));
        }
    }

    existing.bundle = dto.bundle;
    existing.status = status;
    if let Some(enabled) = dto.is_enabled {
        existing.is_enabled = enabled;
    }
    if dto.owner_user_id.is_some() {
        existing.owner_user_id = dto.owner_user_id;
    }
    if dto.created_by_agent_id.is_some() {
        existing.created_by_agent_id = dto.created_by_agent_id;
    }
    // При обновлении с сервера версия становится номером установленной версии из
    // каталога, иначе — монотонный инкремент.
    existing.version = version_override.unwrap_or(existing.version + 1);
    existing.updated_at = Utc::now();

    repository::update(conn, &existing).await?;
    Ok(id)
}

async fn insert_new<C: sea_orm::ConnectionTrait>(
    conn: &C,
    dto: PluginUpsert,
    status: PluginStatus,
    version_override: Option<i32>,
) -> anyhow::Result<String> {
    let id = Uuid::new_v4().to_string();
    let now = Utc::now();
    let def = PluginDefinition {
        id: id.clone(),
        bundle: dto.bundle,
        status,
        is_enabled: dto.is_enabled.unwrap_or(true),
        owner_user_id: dto.owner_user_id,
        created_by_agent_id: dto.created_by_agent_id,
        // Свежая установка из каталога получает номер версии каталога, иначе — 1.
        version: version_override.unwrap_or(1),
        created_at: now,
        updated_at: now,
        rating: None,
        snapshot: None,
        s3_published_version: None,
        s3_published_at: None,
    };
    repository::insert(conn, &def).await?;
    Ok(id)
}

pub async fn get_by_id(id: &str) -> anyhow::Result<Option<PluginDefinition>> {
    let Some(mut definition) = repository::find_by_id(db(), id).await? else {
        return Ok(None);
    };
    definition.snapshot = repository::find_snapshot(db(), id, definition.version, "data")
        .await?
        .map(|(meta, _)| meta);
    Ok(Some(definition))
}

pub async fn get_by_code(code: &str) -> anyhow::Result<Option<PluginDefinition>> {
    Ok(repository::find_by_code(db(), code).await?)
}

pub async fn list_all() -> anyhow::Result<Vec<PluginDefinition>> {
    Ok(repository::list_all(db()).await?)
}

pub async fn list_enabled() -> anyhow::Result<Vec<PluginDefinition>> {
    Ok(repository::list_enabled(db()).await?)
}

pub async fn delete(id: &str) -> anyhow::Result<()> {
    repository::soft_delete(db(), id).await?;
    super::change_token::TOKEN.bump();
    Ok(())
}

/// Установить пользовательскую оценку плагина (1..5; None — снять оценку).
pub async fn set_rating(id: &str, rating: Option<i32>) -> anyhow::Result<()> {
    if let Some(r) = rating {
        if !(1..=5).contains(&r) {
            return Err(anyhow::anyhow!("Rating must be between 1 and 5"));
        }
    }

    let mut def = repository::find_by_id(db(), id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Plugin not found: {id}"))?;
    def.rating = rating;
    repository::update(db(), &def).await?;
    super::change_token::TOKEN.bump();
    Ok(())
}

fn ensure_public_runnable(def: &PluginDefinition) -> anyhow::Result<()> {
    if !def.is_enabled || def.status != PluginStatus::Active {
        return Err(anyhow::anyhow!("Plugin is not active"));
    }
    Ok(())
}

pub async fn invoke(
    id: &str,
    request: PluginInvokeRequest,
) -> anyhow::Result<(serde_json::Value, Vec<String>)> {
    let def = repository::find_by_id(db(), id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Plugin not found: {id}"))?;
    ensure_public_runnable(&def)?;
    if request.method == "data" && def.bundle.data.source.is_some() {
        return invoke_declarative_data(id, def, request, Some("public")).await;
    }
    invoke_definition(id, def, request, Some("public")).await
}

pub async fn dev_invoke(
    id: &str,
    request: PluginInvokeRequest,
) -> anyhow::Result<(serde_json::Value, Vec<String>)> {
    let def = repository::find_by_id(db(), id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Plugin not found: {id}"))?;
    if request.method == "data" && def.bundle.data.source.is_some() {
        return invoke_declarative_data(id, def, request, Some("dev")).await;
    }
    invoke_definition(id, def, request, Some("dev")).await
}

fn snapshot_row_limit(bundle: &PluginBundle) -> usize {
    if bundle
        .view_spec
        .widgets
        .iter()
        .any(|widget| widget.kind == WidgetKind::Table)
    {
        super::data::TABLE_ROW_LIMIT
    } else {
        super::data::CHART_ROW_LIMIT
    }
}

async fn resolve_declarative_data(
    bundle: &PluginBundle,
    context: Option<&contracts::plugins::PluginRunContext>,
) -> Result<serde_json::Value, String> {
    let source = bundle
        .data
        .source
        .as_ref()
        .ok_or_else(|| "plugin has no declarative data.source".to_string())?;
    let result =
        super::data::execute_source_with_context(source, snapshot_row_limit(bundle) + 1, context)
            .await?;
    if result.truncated || result.rows.len() > snapshot_row_limit(bundle) {
        return Err(format!(
            "snapshot_limit_exceeded: source returned more than {} rows",
            snapshot_row_limit(bundle)
        ));
    }
    Ok(serde_json::Value::Array(result.rows))
}

async fn invoke_declarative_data(
    id: &str,
    def: PluginDefinition,
    request: PluginInvokeRequest,
    triggered_by: Option<&str>,
) -> anyhow::Result<(serde_json::Value, Vec<String>)> {
    let started = std::time::Instant::now();
    let mode = request.data_mode;
    let mut source_context = request.context.clone();
    if let Some(args) = request.args.as_object() {
        if let Some(value) = args.get("date_from").and_then(serde_json::Value::as_str) {
            source_context.date_from = Some(value.to_string());
        }
        if let Some(value) = args.get("date_to").and_then(serde_json::Value::as_str) {
            source_context.date_to = Some(value.to_string());
        }
        if let Some(params) = args.get("params").and_then(serde_json::Value::as_object) {
            source_context
                .params
                .extend(params.iter().filter_map(|(key, value)| {
                    value.as_str().map(|value| (key.clone(), value.to_string()))
                }));
        }
    }
    let result = match mode {
        PluginDataMode::Live => match resolve_declarative_data(&def.bundle, Some(&source_context))
            .await
        {
            Ok(value) => Ok(value),
            Err(error) => {
                let snapshot_available = repository::find_snapshot(db(), id, def.version, "data")
                    .await?
                    .is_some();
                Err(anyhow::anyhow!(
                    "live_data_failed: {error}; snapshot_available={snapshot_available}. \
                     Переключитесь на «Сохраненные данные», если нужен последний опубликованный снимок."
                ))
            }
        },
        PluginDataMode::Snapshot => repository::find_snapshot(db(), id, def.version, "data")
            .await?
            .map(|(_, payload)| payload)
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "Snapshot is not available for plugin version {}",
                    def.version
                )
            }),
    };
    let duration_ms = started.elapsed().as_millis() as i64;
    let (status, error_stage, row_count) = match &result {
        Ok(value) => ("ok", None, value.as_array().map(|rows| rows.len() as i64)),
        Err(_) => ("error", Some("data_source"), None),
    };
    super::runs::record(
        id,
        &def.bundle.manifest.code,
        "data",
        duration_ms,
        status,
        error_stage,
        row_count,
        triggered_by,
        mode,
    )
    .await;
    result.map(|value| (value, Vec::new()))
}

async fn invoke_definition(
    id: &str,
    def: PluginDefinition,
    request: PluginInvokeRequest,
    triggered_by: Option<&str>,
) -> anyhow::Result<(serde_json::Value, Vec<String>)> {
    let code = def.bundle.manifest.code.clone();
    let method = request.method.clone();
    let data_mode = request.data_mode;
    let started = std::time::Instant::now();
    let result = super::engine::invoke_server_method(def, request).await;
    let duration_ms = started.elapsed().as_millis() as i64;

    let (status, error_stage, row_count) = match &result {
        Ok((value, _logs)) => ("ok", None, value.as_array().map(|a| a.len() as i64)),
        Err(error) => {
            let stage = error.downcast_ref::<PluginError>().map(|e| e.stage.clone());
            let status = if stage.as_deref() == Some("timeout") {
                "timeout"
            } else {
                "error"
            };
            (status, stage, None)
        }
    };
    super::runs::record(
        id,
        &code,
        &method,
        duration_ms,
        status,
        error_stage.as_deref(),
        row_count,
        triggered_by,
        data_mode,
    )
    .await;

    result
}

fn smoke_failure(
    stage: impl Into<String>,
    file_hint: Option<String>,
    message: impl Into<String>,
    stack: Option<String>,
) -> PluginSmokeFailure {
    PluginSmokeFailure {
        stage: stage.into(),
        file_hint,
        message: message.into(),
        stack,
    }
}

fn file_hint_for_stage(stage: &str) -> Option<String> {
    if stage.starts_with("client_") {
        Some("client_script".to_string())
    } else if matches!(
        stage,
        "module_eval" | "missing_export" | "invoke" | "runtime" | "deserialize" | "timeout"
    ) {
        Some("server_script".to_string())
    } else if stage == "manifest" {
        Some("manifest".to_string())
    } else {
        None
    }
}

fn extract_client_invokes(script: Option<&String>) -> Vec<String> {
    let Some(script) = script else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for marker in ["host.invoke(\"", "host.invoke('"] {
        let quote = marker.chars().last().unwrap_or('"');
        let mut rest = script.as_str();
        while let Some(pos) = rest.find(marker) {
            let after = &rest[pos + marker.len()..];
            if let Some(end) = after.find(quote) {
                let method = after[..end].trim();
                if !method.is_empty() && !out.iter().any(|item| item == method) {
                    out.push(method.to_string());
                }
                rest = &after[end + 1..];
            } else {
                break;
            }
        }
    }
    out.sort();
    out
}

fn temp_definition(bundle: PluginBundle) -> PluginDefinition {
    let now = Utc::now();
    PluginDefinition {
        id: "smoke-bundle".to_string(),
        bundle,
        status: PluginStatus::Draft,
        is_enabled: false,
        owner_user_id: None,
        created_by_agent_id: None,
        version: 0,
        created_at: now,
        updated_at: now,
        rating: None,
        snapshot: None,
        s3_published_version: None,
        s3_published_at: None,
    }
}

fn suggested_next_step(failures: &[PluginSmokeFailure]) -> Option<String> {
    let first = failures.first()?;
    let hint = match first.stage.as_str() {
        "manifest" => "Fix the plugin manifest or bundle shape, then run plugin_validate again.",
        "client_missing_export" => "Export async function mount(root, host) from client_script.",
        "client_missing_server_export" => {
            "Make every host.invoke(\"method\") call match an exported server_script function."
        }
        "module_eval" | "client_module_eval" => {
            "Fix JavaScript syntax/top-level module code, then rerun smoke test."
        }
        "missing_export" => "Export the requested server method or update the invoke method name.",
        "runtime" | "invoke" | "deserialize" | "timeout" => {
            "Use the stage/message/stack to fix server_script runtime behavior."
        }
        "sql" | "database" => "Fix SQL resources or manifest db:read capabilities.",
        "table_spec_parse" => {
            "Keep the table spec as a JSON literal assigned to `const spec = ...`, then rerun plugin_smoke_test."
        }
        "table_spec" | "table_data" => {
            "Fix table spec columns/sort/totals/conditionalFormat so they match the data rows returned by server_script."
        }
        _ => "Fix the first reported failure, then rerun plugin_smoke_test.",
    };
    Some(hint.to_string())
}

fn extract_table_spec(script: Option<&String>) -> Result<Option<serde_json::Value>, String> {
    let Some(script) = script else {
        return Ok(None);
    };
    if !script.contains("PluginTables.render") {
        return Ok(None);
    }

    for marker in ["const spec =", "let spec =", "var spec ="] {
        if let Some(pos) = script.find(marker) {
            let after = pos + marker.len();
            let Some(rel_start) = script[after..].find(|c: char| matches!(c, '{' | '[')) else {
                return Err(
                    "Found table spec assignment, but no JSON object/array follows it".into(),
                );
            };
            let start = after + rel_start;
            let literal = extract_balanced_json_literal(script, start)?;
            let value = serde_json::from_str::<serde_json::Value>(literal)
                .map_err(|error| format!("Table spec must be strict JSON: {error}"))?;
            return Ok(Some(value));
        }
    }

    Err("PluginTables.render is used, but smoke test could not find `const spec = {...}`".into())
}

fn extract_balanced_json_literal(script: &str, start: usize) -> Result<&str, String> {
    let mut stack: Vec<char> = Vec::new();
    let mut in_string = false;
    let mut escape = false;

    for (offset, ch) in script[start..].char_indices() {
        if in_string {
            if escape {
                escape = false;
            } else if ch == '\\' {
                escape = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }

        match ch {
            '"' => in_string = true,
            '{' => stack.push('}'),
            '[' => stack.push(']'),
            '}' | ']' => {
                if stack.pop() != Some(ch) {
                    return Err("Table spec JSON has mismatched brackets".into());
                }
                if stack.is_empty() {
                    let end = start + offset + ch.len_utf8();
                    return Ok(&script[start..end]);
                }
            }
            _ => {}
        }
    }

    Err("Table spec JSON is not closed".into())
}

fn table_spec_failures(
    spec: &serde_json::Value,
    rows: Option<&serde_json::Value>,
) -> Vec<PluginSmokeFailure> {
    let mut failures = Vec::new();
    let mut fail = |message: String| {
        failures.push(smoke_failure(
            "table_spec",
            Some("client_script".to_string()),
            message,
            None,
        ));
    };

    let Some(columns) = spec.get("columns").and_then(|v| v.as_array()) else {
        fail("table spec requires columns: [...]".into());
        return failures;
    };
    if columns.is_empty() {
        fail("table spec must contain at least one column".into());
    }

    let allowed_types = ["text", "number", "int", "money", "percent", "date"];
    let numeric_types = ["number", "int", "money", "percent"];
    let mut keys: HashSet<String> = HashSet::new();
    let mut types: HashMap<String, String> = HashMap::new();

    for (idx, col) in columns.iter().enumerate() {
        let Some(key) = col
            .get("key")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
        else {
            fail(format!("columns[{idx}].key is required"));
            continue;
        };
        if !keys.insert(key.to_string()) {
            fail(format!("duplicate column key '{key}'"));
        }
        let col_type = col.get("type").and_then(|v| v.as_str()).unwrap_or("text");
        if !allowed_types.contains(&col_type) {
            fail(format!("column '{key}' has unsupported type '{col_type}'"));
        }
        types.insert(key.to_string(), col_type.to_string());
    }

    if let Some(sort_key) = spec
        .get("sort")
        .and_then(|v| v.get("key"))
        .and_then(|v| v.as_str())
    {
        if !keys.contains(sort_key) {
            fail(format!("sort.key '{sort_key}' is not present in columns"));
        }
    }

    if let Some(agg) = spec
        .get("totals")
        .and_then(|v| v.get("agg"))
        .and_then(|v| v.as_object())
    {
        let allowed_agg = ["sum", "avg", "count", "min", "max"];
        for (key, fn_value) in agg {
            if !keys.contains(key) {
                fail(format!("totals.agg references unknown column '{key}'"));
            }
            let Some(fn_name) = fn_value.as_str() else {
                fail(format!("totals.agg['{key}'] must be a string"));
                continue;
            };
            if !allowed_agg.contains(&fn_name) {
                fail(format!(
                    "totals.agg['{key}'] has unsupported function '{fn_name}'"
                ));
            }
        }
    }

    if let Some(items) = spec.get("conditionalFormat").and_then(|v| v.as_array()) {
        let allowed_kinds = ["threshold", "dataBar", "heatmap"];
        let allowed_ops = [">", "<", ">=", "<=", "=", "!="];
        for (idx, item) in items.iter().enumerate() {
            let column = item.get("column").and_then(|v| v.as_str()).unwrap_or("");
            if !keys.contains(column) {
                fail(format!(
                    "conditionalFormat[{idx}] references unknown column '{column}'"
                ));
            }
            let kind = item.get("kind").and_then(|v| v.as_str()).unwrap_or("");
            if !allowed_kinds.contains(&kind) {
                fail(format!(
                    "conditionalFormat[{idx}] has unsupported kind '{kind}'"
                ));
            }
            if let Some(col_type) = types.get(column) {
                if !numeric_types.contains(&col_type.as_str()) {
                    fail(format!(
                        "conditionalFormat[{idx}] column '{column}' must be numeric, got '{col_type}'"
                    ));
                }
            }
            if kind == "threshold" {
                let Some(rules) = item.get("rules").and_then(|v| v.as_array()) else {
                    fail(format!("conditionalFormat[{idx}] threshold requires rules"));
                    continue;
                };
                for (rule_idx, rule) in rules.iter().enumerate() {
                    let op = rule.get("op").and_then(|v| v.as_str()).unwrap_or("");
                    if !allowed_ops.contains(&op) {
                        fail(format!(
                            "conditionalFormat[{idx}].rules[{rule_idx}] has unsupported op '{op}'"
                        ));
                    }
                    if rule.get("value").is_none() {
                        fail(format!(
                            "conditionalFormat[{idx}].rules[{rule_idx}] requires value"
                        ));
                    }
                }
            }
        }
    }

    if let Some(page_size) = spec
        .get("pagination")
        .and_then(|v| v.get("pageSize"))
        .and_then(|v| v.as_i64())
    {
        if !(1..=1000).contains(&page_size) {
            fail(format!(
                "pagination.pageSize must be in 1..=1000, got {page_size}"
            ));
        }
    }

    if let Some(rows) = rows {
        let Some(rows_array) = rows.as_array() else {
            failures.push(smoke_failure(
                "table_data",
                Some("server_script".to_string()),
                "PluginTables.render expects data() to return an array of row objects",
                None,
            ));
            return failures;
        };
        if let Some(sample) = rows_array.first().and_then(|v| v.as_object()) {
            for key in &keys {
                if !sample.contains_key(key) {
                    fail(format!(
                        "column.key '{key}' is absent in the first data row; check SELECT aliases"
                    ));
                }
            }
        }
    }

    failures
}

pub async fn smoke_test(request: PluginSmokeRequest) -> anyhow::Result<PluginSmokeReport> {
    let def = if let Some(bundle) = request.bundle {
        temp_definition(bundle)
    } else if let Some(id) = request.id.as_deref() {
        repository::find_by_id(db(), id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Plugin not found: {id}"))?
    } else {
        return Err(anyhow::anyhow!("PluginSmokeRequest requires id or bundle"));
    };

    let validate = validate(&def.bundle).await;
    let mut failures: Vec<PluginSmokeFailure> = validate
        .errors
        .iter()
        .map(|error| {
            smoke_failure(
                error.stage.clone(),
                file_hint_for_stage(&error.stage),
                error.message.clone(),
                error.stack.clone(),
            )
        })
        .collect();
    let server_exports = validate.server_exports.clone();
    let client_exports = validate.client_exports.clone();
    let client_invokes = extract_client_invokes(def.bundle.client_script.as_ref());
    let uses_plugin_tables = def
        .bundle
        .client_script
        .as_ref()
        .is_some_and(|script| script.contains("PluginTables.render"));

    for method in &client_invokes {
        if !server_exports.iter().any(|export| export == method) {
            failures.push(smoke_failure(
                "client_missing_server_export",
                Some("client_script".to_string()),
                format!("client_script calls host.invoke(\"{method}\"), but server_script does not export it"),
                None,
            ));
        }
    }

    let mut methods = request.methods;
    if methods.is_empty() {
        methods = server_exports
            .iter()
            .map(|method| PluginSmokeMethod {
                method: method.clone(),
                args: serde_json::Value::Null,
            })
            .collect();
    }
    if request.render {
        for method in &client_invokes {
            if !methods.iter().any(|item| item.method == *method) {
                methods.push(PluginSmokeMethod {
                    method: method.clone(),
                    args: serde_json::Value::Null,
                });
            }
        }
    }

    let mut method_results: HashMap<String, serde_json::Value> = HashMap::new();
    if validate.ok {
        for method in methods {
            if method.method.trim().is_empty() {
                continue;
            }
            let invoke = PluginInvokeRequest {
                method: method.method.clone(),
                args: method.args,
                context: request.context.clone(),
                data_mode: PluginDataMode::Live,
            };
            match super::engine::invoke_server_method(def.clone(), invoke).await {
                Ok((value, _logs)) => {
                    method_results.insert(method.method.clone(), value);
                }
                Err(error) => {
                    if let Some(detail) = error.downcast_ref::<PluginError>() {
                        failures.push(smoke_failure(
                            detail.stage.clone(),
                            file_hint_for_stage(&detail.stage),
                            detail.message.clone(),
                            detail.stack.clone(),
                        ));
                    } else {
                        failures.push(smoke_failure(
                            "invoke",
                            Some("server_script".to_string()),
                            error.to_string(),
                            None,
                        ));
                    }
                }
            }
        }
    }

    if request.render
        && def.bundle.manifest.runtime.runs_on_client()
        && !client_exports.iter().any(|export| export == "mount")
    {
        failures.push(smoke_failure(
            "client_missing_export",
            Some("client_script".to_string()),
            "render smoke requires client_script to export mount",
            None,
        ));
    }

    if request.render && uses_plugin_tables {
        match extract_table_spec(def.bundle.client_script.as_ref()) {
            Ok(Some(spec)) => {
                let rows = client_invokes
                    .iter()
                    .filter_map(|method| method_results.get(method))
                    .find(|value| value.is_array())
                    .or_else(|| method_results.get("data"))
                    .or_else(|| method_results.values().find(|value| value.is_array()));
                failures.extend(table_spec_failures(&spec, rows));
            }
            Ok(None) => {}
            Err(message) => failures.push(smoke_failure(
                "table_spec_parse",
                Some("client_script".to_string()),
                message,
                None,
            )),
        }
    }

    let ok = validate.ok && failures.is_empty();
    let suggested_next_step = if ok {
        None
    } else {
        suggested_next_step(&failures)
    };

    Ok(PluginSmokeReport {
        ok,
        validate,
        server_exports,
        client_exports,
        client_invokes,
        failures,
        suggested_next_step,
    })
}

pub async fn stats(id: &str, days: i64) -> anyhow::Result<contracts::plugins::PluginStats> {
    super::runs::stats(id, days, 50).await
}

pub async fn runs_summary(days: i64) -> anyhow::Result<Vec<contracts::plugins::PluginRunBrief>> {
    super::runs::summary_all(days).await
}

pub async fn export(id: &str) -> anyhow::Result<(String, Vec<u8>)> {
    let def = repository::find_by_id(db(), id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Plugin not found: {id}"))?;
    let snapshot = repository::find_snapshot(db(), id, def.version, "data")
        .await?
        .map(|(_, payload)| payload);
    let bytes = super::package::export_bundle_with_snapshot(&def.bundle, snapshot.as_ref())?;
    let filename = super::package::archive_filename(&def.bundle.manifest.code);
    Ok((filename, bytes))
}

pub struct ImportOutcome {
    pub id: Option<String>,
    pub code: String,
    pub report: PluginValidateReport,
}

pub async fn import(bytes: &[u8]) -> anyhow::Result<ImportOutcome> {
    let (bundle, imported_snapshot) = super::package::import_archive_with_snapshot(bytes)?;
    import_bundle_onto(None, bundle, imported_snapshot, None).await
}

/// Общая часть импорта: валидирует бандл и апсертит его либо на конкретный
/// `id_hint` (используется публикацией/обновлением из S3 — плагин уже известен),
/// либо ищет существующий плагин по `bundle.manifest.code` и создаёт новый,
/// если такого кода ещё нет (используется загрузкой .zip через `import`).
pub(crate) async fn import_bundle_onto(
    id_hint: Option<&str>,
    bundle: PluginBundle,
    imported_snapshot: Option<serde_json::Value>,
    // Итоговый номер версии (для установки/обновления с сервера — номер каталога S3).
    // `None` — обычный импорт .zip: +1 к существующей или 1 для новой записи.
    version_override: Option<i32>,
) -> anyhow::Result<ImportOutcome> {
    let code = bundle.manifest.code.clone();
    let capture_snapshot = bundle.data.source.is_some();

    let report = validate(&bundle).await;
    if !report.ok {
        return Ok(ImportOutcome {
            id: None,
            code,
            report,
        });
    }

    let existing = match id_hint {
        Some(id) => repository::find_by_id(db(), id).await?,
        None => get_by_code(&code).await?,
    };
    let dto = match &existing {
        Some(current) => PluginUpsert {
            id: Some(current.id.clone()),
            bundle,
            status: Some(current.status.as_str().to_string()),
            is_enabled: Some(current.is_enabled),
            owner_user_id: None,
            created_by_agent_id: None,
            version: Some(current.version),
            capture_snapshot,
            allow_live_only: false,
        },
        None => PluginUpsert {
            id: None,
            bundle,
            status: Some(PluginStatus::Draft.as_str().to_string()),
            is_enabled: Some(false),
            owner_user_id: None,
            created_by_agent_id: None,
            version: None,
            capture_snapshot,
            allow_live_only: false,
        },
    };

    let id = upsert_prepared(
        dto,
        imported_snapshot,
        None,
        Some(report.clone()),
        version_override,
    )
    .await?
    .id;
    Ok(ImportOutcome {
        id: Some(id),
        code,
        report,
    })
}

pub async fn run_data(
    id: &str,
    ctx: &contracts::plugins::PluginRunContext,
) -> anyhow::Result<contracts::shared::drilldown::DrilldownResponse> {
    use crate::data_view::DataViewRegistry;
    use contracts::shared::data_view::ViewContext;

    let def = repository::find_by_id(db(), id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Plugin not found: {id}"))?;
    ensure_public_runnable(&def)?;

    let view_id = def
        .bundle
        .data
        .view_id
        .clone()
        .ok_or_else(|| anyhow::anyhow!("Plugin {id} has no data.view_id binding"))?;
    let group_by = ctx
        .group_by
        .clone()
        .or_else(|| def.bundle.data.group_by.clone())
        .ok_or_else(|| anyhow::anyhow!("Plugin {id} has no group_by"))?;

    let mut params = ctx.params.clone();
    if let Some(metric) = &def.bundle.data.metric_id {
        params.entry("metric".to_string()).or_insert(metric.clone());
    }

    let view_ctx = ViewContext {
        date_from: ctx.date_from.clone().unwrap_or_default(),
        date_to: ctx.date_to.clone().unwrap_or_default(),
        period2_from: None,
        period2_to: None,
        connection_mp_refs: ctx.connection_mp_refs.clone(),
        params,
    };

    DataViewRegistry::new()
        .compute_drilldown(&view_id, &view_ctx, &group_by, &[])
        .await
        .map_err(|error| anyhow::anyhow!("DataView '{view_id}' drilldown error: {error}"))
}

pub async fn insert_test_data() -> anyhow::Result<()> {
    super::demo::insert_test_data().await?;
    super::funnel::insert_funnel_plugin().await
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn extracts_strict_json_table_spec_from_template_client_script() {
        let script = r#"
export async function mount(root, host) {
  const rows = await host.invoke("data", {});
  const spec = {"title":"T","columns":[{"key":"name","type":"text"}]};
  PluginTables.render(root, spec, rows);
}
"#
        .to_string();

        let spec = extract_table_spec(Some(&script))
            .expect("parse result")
            .expect("table spec");
        assert_eq!(spec["columns"][0]["key"], json!("name"));
    }

    #[test]
    fn table_spec_validation_reports_missing_select_alias() {
        let spec = json!({
            "columns": [
                { "key": "article", "type": "text" },
                { "key": "revenue", "type": "money" }
            ],
            "sort": { "key": "revenue", "dir": "desc" }
        });
        let rows = json!([{ "article": "A-1", "amount": 1200 }]);

        let failures = table_spec_failures(&spec, Some(&rows));
        assert!(failures.iter().any(|f| {
            f.stage == "table_spec" && f.message.contains("column.key 'revenue' is absent")
        }));
    }
}
