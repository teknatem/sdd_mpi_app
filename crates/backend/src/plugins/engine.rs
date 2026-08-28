//! Server-side JavaScript runtime for plugins.
//!
//! A plugin server script is an ES module. Exported async functions are invoked with
//! `(args, host)`, where `host.db.query(sql, params)` provides parameterized read access
//! to the application database and `host.log.*` writes to the invocation log.

use contracts::plugins::{
    PluginCapability, PluginDefinition, PluginError, PluginInvokeRequest, PluginValidateReport,
};
use rquickjs::{
    prelude::{Async, Func},
    promise::MaybePromise,
    AsyncContext, AsyncRuntime, CatchResultExt, CaughtError, Function, Module, Object, Value,
};
use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::shared::data_access::row_json::{fetch_json_rows, JsonBind};
use crate::shared::data_access::sql_guard::{inspect_read_query, wrap_limited_sql};

const READ_ROW_LIMIT: usize = 5_000;

/// Жёсткий лимит времени исполнения одного вызова плагина.
const EXEC_TIMEOUT: Duration = Duration::from_secs(5);
/// Лимит памяти JS-рантайма (дефолтный аллокатор QuickJS — лимит действует).
const MEMORY_LIMIT_BYTES: usize = 64 * 1024 * 1024;
/// Лимит стека JS-рантайма (защита от бесконечной рекурсии).
const MAX_STACK_SIZE: usize = 1024 * 1024;

#[derive(Debug, Clone, Copy)]
pub struct ScriptExecutionLimits {
    pub timeout: Duration,
    pub memory_limit_bytes: usize,
    pub max_stack_size: usize,
}

impl Default for ScriptExecutionLimits {
    fn default() -> Self {
        Self {
            timeout: EXEC_TIMEOUT,
            memory_limit_bytes: MEMORY_LIMIT_BYTES,
            max_stack_size: MAX_STACK_SIZE,
        }
    }
}

impl ScriptExecutionLimits {
    pub fn quality_check() -> Self {
        Self {
            timeout: Duration::from_secs(30),
            ..Self::default()
        }
    }
}

/// Построить ошибку плагина из пойманного JS-исключения, сохранив stack.
fn js_error(stage: &str, caught: &CaughtError) -> PluginError {
    match caught {
        CaughtError::Exception(exception) => PluginError::new(
            stage,
            exception.message().unwrap_or_else(|| exception.to_string()),
        )
        .with_stack(exception.stack()),
        other => PluginError::new(stage, other.to_string()),
    }
}

/// Если исполнение было прервано по таймауту — переразметить этап ошибки в `timeout`.
fn relabel_timeout(deadline: Instant, timeout: Duration, mut error: PluginError) -> PluginError {
    if Instant::now() >= deadline {
        error.stage = "timeout".to_string();
        error.message = format!(
            "Превышен лимит времени исполнения плагина ({} с)",
            timeout.as_secs()
        );
        error.stack = None;
    }
    error
}

/// Создать JS-рантайм с лимитами времени, памяти и стека.
///
/// Возвращает рантайм и дедлайн (для пост-классификации ошибки как `timeout`).
async fn limited_runtime(limits: ScriptExecutionLimits) -> anyhow::Result<(AsyncRuntime, Instant)> {
    let runtime = AsyncRuntime::new()
        .map_err(|error| anyhow::anyhow!("Failed to create JavaScript runtime: {error}"))?;
    let deadline = Instant::now() + limits.timeout;
    runtime.set_memory_limit(limits.memory_limit_bytes).await;
    runtime.set_max_stack_size(limits.max_stack_size).await;
    runtime
        .set_interrupt_handler(Some(Box::new(move || Instant::now() >= deadline)))
        .await;
    Ok((runtime, deadline))
}

fn json_param_to_bind(value: serde_json::Value) -> Result<JsonBind, String> {
    match value {
        serde_json::Value::Null => Ok(JsonBind::Null),
        serde_json::Value::Bool(value) => Ok(JsonBind::Bool(value)),
        serde_json::Value::Number(value) => {
            if let Some(value) = value.as_i64() {
                Ok(JsonBind::Int(value))
            } else if let Some(value) = value.as_f64() {
                Ok(JsonBind::Float(value))
            } else {
                Err("Unsupported numeric SQL parameter".to_string())
            }
        }
        serde_json::Value::String(value) => Ok(JsonBind::Text(value)),
        serde_json::Value::Array(_) | serde_json::Value::Object(_) => {
            Err("SQL parameters must be scalar JSON values".to_string())
        }
    }
}

fn table_scopes(table: &str) -> Vec<String> {
    let table = table.to_ascii_lowercase();
    let mut scopes = vec![table.clone()];
    let tags: &[&str] =
        if table.starts_with("a017") || table.starts_with("a018") || table.starts_with("a019") {
            &["llm"]
        } else if table.starts_with("a024") || table.starts_with("a025") {
            &["bi", "dashboard"]
        } else if table.starts_with("sys_general_ledger") {
            &["gl", "accounting"]
        } else if table.starts_with("a013") {
            &["ym", "sales"]
        } else if table.starts_with("a002")
            || table.starts_with("a004")
            || table.starts_with("a005")
            || table.starts_with("a006")
        {
            &["ref"]
        } else if table.starts_with("a012")
            || table.starts_with("a015")
            || table.starts_with("a020")
            || table.starts_with("a026")
            || table.starts_with("p9")
        {
            &["wb", "projection"]
        } else {
            &[]
        };
    scopes.extend(tags.iter().map(|tag| tag.to_string()));
    scopes
}

fn capability_allows_table(capabilities: &[PluginCapability], table: &str) -> bool {
    let scopes = table_scopes(table);
    capabilities.iter().any(|capability| match capability {
        PluginCapability::DbReadAll => true,
        PluginCapability::DbRead(scope) => scopes.iter().any(|item| item == scope),
        _ => false,
    })
}

fn enforce_sql_capabilities(sql: &str, capabilities: &[PluginCapability]) -> Result<(), String> {
    let tables = inspect_read_query(sql)?.tables;
    if tables.is_empty() {
        return Ok(());
    }
    let blocked: Vec<String> = tables
        .into_iter()
        .filter(|table| !capability_allows_table(capabilities, table))
        .collect();
    if blocked.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "Plugin manifest capabilities do not allow reading table(s): {}. Add db:read:<table>, db:read:<tag>, or db:read:*.",
            blocked.join(", ")
        ))
    }
}

fn limit_read_sql(sql: &str) -> String {
    wrap_limited_sql(sql, READ_ROW_LIMIT, "plugin_limited_result")
}

async fn read_sql(
    sql: &str,
    params: Vec<serde_json::Value>,
    capabilities: &[PluginCapability],
) -> Result<Vec<serde_json::Value>, String> {
    let trimmed = sql.trim();
    enforce_sql_capabilities(trimmed, capabilities)?;

    let binds = params
        .into_iter()
        .map(json_param_to_bind)
        .collect::<Result<Vec<_>, _>>()?;
    let limited = limit_read_sql(trimmed);
    // Materialize via the sqlx runtime-type decoder (not SeaORM `find_by_statement`,
    // which silently drops untyped computed columns — SUM/COUNT/CAST — on SQLite).
    let (rows, _columns) = fetch_json_rows(&limited, binds).await?;

    Ok(rows)
}

async fn host_db_query(
    sql: String,
    params_json: String,
    capabilities_json: String,
) -> rquickjs::Result<String> {
    let params: Vec<serde_json::Value> = serde_json::from_str(&params_json).map_err(|error| {
        rquickjs::Error::new_from_js_message(
            "JSON",
            "SQL parameters",
            format!("Invalid parameter array: {error}"),
        )
    })?;
    let capabilities: Vec<PluginCapability> =
        serde_json::from_str(&capabilities_json).map_err(|error| {
            rquickjs::Error::new_from_js_message(
                "JSON",
                "plugin capabilities",
                format!("Invalid capability array: {error}"),
            )
        })?;
    let rows = read_sql(&sql, params, &capabilities)
        .await
        .map_err(|error| rquickjs::Error::new_into_js_message("database", "JavaScript", error))?;
    serde_json::to_string(&rows).map_err(|error| {
        rquickjs::Error::new_into_js_message("database result", "JSON", error.to_string())
    })
}

const HOST_FACTORY: &str = r#"
(() => {
  const host = {
    db: Object.freeze({
      query: async (sql, params = []) => {
        const json = await __hostDbQuery(String(sql), JSON.stringify(params), __hostCapabilitiesJson);
        return JSON.parse(json);
      },
      queryResource: async (name, params = []) => {
        const key = String(name);
        const sql = __hostSqlResources[key];
        if (typeof sql !== "string") {
          throw new Error(`SQL resource '${key}' is not defined`);
        }
        const json = await __hostDbQuery(sql, JSON.stringify(params), __hostCapabilitiesJson);
        return JSON.parse(json);
      }
    }),
    log: Object.freeze({
      info: (...values) => __hostLog(values.map(formatLogValue).join(" ")),
      warn: (...values) => __hostLog("[warn] " + values.map(formatLogValue).join(" ")),
      error: (...values) => __hostLog("[error] " + values.map(formatLogValue).join(" "))
    })
  };
  // Ветка эффектов появляется только там, где её выдал хост. У плагинов и
  // навыков этих глобалов нет — значит, нет и самой возможности что-то
  // изменить: право на эффект не отбирается проверкой, его просто не выдают.
  if (typeof __hostActionCatalog !== "undefined") {
    const actions = {};
    for (const entry of __hostActionCatalog) {
      actions[entry.method] = async (input, options) => {
        const json = await __hostActionCall(
          entry.name,
          JSON.stringify(input || {}),
          JSON.stringify(options || {})
        );
        return JSON.parse(json);
      };
    }
    host.actions = Object.freeze(actions);
  }
  return host;
})()

function formatLogValue(value) {
  if (typeof value === "string") return value;
  try { return JSON.stringify(value); } catch (_) { return String(value); }
}
"#;

/// Метод, который появится в `host.actions` у скрипта.
///
/// Смысла Действия и режима исполнения движок не трактует: он заводит метод и
/// передаёт вызов наружу. Что такое Действие, знает вызывающая сторона —
/// механизм Процессов; плагины про него по-прежнему не знают ничего.
///
/// Единственное, что движок делает с именем, — сверяет его с этим списком на
/// вызове: `__hostActionCall` остаётся достижимым глобалом, и без сверки право
/// держал бы только сахар `host.actions`, который скрипт обходит одной строкой.
#[derive(Debug, Clone)]
pub struct HostActionEntry {
    pub name: String,
    pub method: String,
}

/// Обработчик вызова эффекта: `(имя, вход, опции) -> результат | текст ошибки`.
pub type HostActionHandler = Arc<
    dyn Fn(
            String,
            serde_json::Value,
            serde_json::Value,
        ) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = Result<serde_json::Value, String>> + Send>,
        > + Send
        + Sync,
>;

/// Чем расширен `host` сверх базовой поверхности (чтение и журнал).
#[derive(Default, Clone)]
pub struct HostExtensions {
    pub actions: Option<(Vec<HostActionEntry>, HostActionHandler)>,
}

/// Invoke one exported server function and return its JSON result plus captured log lines.
pub async fn invoke_server_method(
    def: PluginDefinition,
    request: PluginInvokeRequest,
) -> anyhow::Result<(serde_json::Value, Vec<String>)> {
    invoke_server_method_with_limits(def, request, ScriptExecutionLimits::default()).await
}

pub async fn invoke_server_method_with_limits(
    def: PluginDefinition,
    request: PluginInvokeRequest,
    limits: ScriptExecutionLimits,
) -> anyhow::Result<(serde_json::Value, Vec<String>)> {
    invoke_server_method_with_extensions(def, request, limits, HostExtensions::default()).await
}

/// То же, но с расширенным `host`. Отдельный вход, а не флаг у прежнего:
/// расширение поверхности — решение вызывающего, и оно должно быть видно в
/// месте вызова.
pub async fn invoke_server_method_with_extensions(
    def: PluginDefinition,
    request: PluginInvokeRequest,
    limits: ScriptExecutionLimits,
    extensions: HostExtensions,
) -> anyhow::Result<(serde_json::Value, Vec<String>)> {
    let script = def
        .bundle
        .server_script
        .ok_or_else(|| anyhow::anyhow!("Plugin has no server_script"))?;
    let sql_resources = def.bundle.sql_resources;
    let capabilities = def.bundle.manifest.parsed_capabilities();
    if request.method.trim().is_empty() {
        return Err(anyhow::anyhow!("Plugin method must not be empty"));
    }

    let method = request.method;
    let args = request.args;
    let context = request.context;
    let logs = Arc::new(Mutex::new(Vec::<String>::new()));
    let logs_for_runtime = logs.clone();

    let (runtime, deadline) = limited_runtime(limits).await?;
    let js_context = AsyncContext::full(&runtime)
        .await
        .map_err(|error| anyhow::anyhow!("Failed to create JavaScript context: {error}"))?;

    let result: Result<serde_json::Value, PluginError> = js_context
        .async_with(async move |ctx| {
            let globals = ctx.globals();
            globals
                .set("__hostDbQuery", Func::from(Async(host_db_query)))
                .catch(&ctx)
                .map_err(|error| js_error("module_eval", &error))?;
            let sql_resources_value = rquickjs_serde::to_value(ctx.clone(), sql_resources)
                .map_err(|error| PluginError::new("module_eval", error.to_string()))?;
            globals
                .set("__hostSqlResources", sql_resources_value)
                .catch(&ctx)
                .map_err(|error| js_error("module_eval", &error))?;
            let capabilities_json = serde_json::to_string(&capabilities)
                .map_err(|error| PluginError::new("module_eval", error.to_string()))?;
            globals
                .set("__hostCapabilitiesJson", capabilities_json)
                .catch(&ctx)
                .map_err(|error| js_error("module_eval", &error))?;

            let log_fn = move |message: String| {
                logs_for_runtime.lock().unwrap().push(message);
            };
            globals
                .set("__hostLog", Func::from(log_fn))
                .catch(&ctx)
                .map_err(|error| js_error("module_eval", &error))?;

            if let Some((entries, handler)) = extensions.actions {
                let catalog: Vec<serde_json::Value> = entries
                    .iter()
                    .map(|entry| serde_json::json!({ "name": entry.name, "method": entry.method }))
                    .collect();
                let catalog_value = rquickjs_serde::to_value(ctx.clone(), catalog)
                    .map_err(|error| PluginError::new("module_eval", error.to_string()))?;
                globals
                    .set("__hostActionCatalog", catalog_value)
                    .catch(&ctx)
                    .map_err(|error| js_error("module_eval", &error))?;

                // Выданные имена держим отдельным множеством и проверяем на вызове.
                // Каталог задаёт только сахар `host.actions`, а `__hostActionCall`
                // остаётся достижимым глобалом: без этой проверки скрипт звал бы
                // мимо сахара любое Действие каталога — право, которого ему не
                // выдавали.
                let granted: Arc<HashSet<String>> =
                    Arc::new(entries.iter().map(|entry| entry.name.clone()).collect());

                // Обработчик захвачен замыканием, а не прочитан из глобала:
                // права и контекст прогона остаются на стороне Rust и не могут
                // быть подменены присваиванием из скрипта.
                let action_fn = move |name: String, input_json: String, options_json: String| {
                    let handler = handler.clone();
                    let granted = granted.clone();
                    async move {
                        if !granted.contains(&name) {
                            let mut allowed: Vec<&str> =
                                granted.iter().map(String::as_str).collect();
                            allowed.sort_unstable();
                            return Err(rquickjs::Error::new_into_js_message(
                                "action",
                                "JavaScript",
                                format!(
                                    "Действие '{name}' не выдано этому скрипту; разрешены: {}",
                                    allowed.join(", ")
                                ),
                            ));
                        }
                        let input: serde_json::Value =
                            serde_json::from_str(&input_json).map_err(|error| {
                                rquickjs::Error::new_from_js_message(
                                    "JSON",
                                    "action input",
                                    format!("Invalid action input: {error}"),
                                )
                            })?;
                        let options: serde_json::Value = serde_json::from_str(&options_json)
                            .map_err(|error| {
                                rquickjs::Error::new_from_js_message(
                                    "JSON",
                                    "action options",
                                    format!("Invalid action options: {error}"),
                                )
                            })?;
                        let result = handler(name, input, options).await.map_err(|error| {
                            rquickjs::Error::new_into_js_message("action", "JavaScript", error)
                        })?;
                        serde_json::to_string(&result).map_err(|error| {
                            rquickjs::Error::new_into_js_message(
                                "action result",
                                "JSON",
                                error.to_string(),
                            )
                        })
                    }
                };
                globals
                    .set("__hostActionCall", Func::from(Async(action_fn)))
                    .catch(&ctx)
                    .map_err(|error| js_error("module_eval", &error))?;
            }

            let declared = Module::declare(ctx.clone(), "plugin-server.js", script)
                .catch(&ctx)
                .map_err(|error| js_error("module_eval", &error))?;
            let (module, evaluation) = declared
                .eval()
                .catch(&ctx)
                .map_err(|error| js_error("module_eval", &error))?;
            evaluation
                .into_future::<()>()
                .await
                .catch(&ctx)
                .map_err(|error| js_error("module_eval", &error))?;

            let function: Function = module.get(method.as_str()).catch(&ctx).map_err(|_| {
                PluginError::new(
                    "missing_export",
                    format!("Server method '{method}' is not exported"),
                )
            })?;
            let args_value = rquickjs_serde::to_value(ctx.clone(), args)
                .map_err(|error| PluginError::new("invoke", error.to_string()))?;
            let context_value = rquickjs_serde::to_value(ctx.clone(), context)
                .map_err(|error| PluginError::new("invoke", error.to_string()))?;
            let host: Object = ctx
                .eval(HOST_FACTORY)
                .catch(&ctx)
                .map_err(|error| js_error("invoke", &error))?;
            host.set("context", context_value)
                .catch(&ctx)
                .map_err(|error| js_error("invoke", &error))?;

            let promise: MaybePromise = function
                .call((args_value, host))
                .catch(&ctx)
                .map_err(|error| js_error("invoke", &error))?;
            let value: Value = promise
                .into_future()
                .await
                .catch(&ctx)
                .map_err(|error| js_error("runtime", &error))?;
            rquickjs_serde::from_value(value)
                .map_err(|error| PluginError::new("deserialize", error.to_string()))
        })
        .await;

    runtime.idle().await;
    let captured = logs.lock().unwrap().clone();
    result
        .map(|value| (value, captured))
        .map_err(|error| anyhow::Error::new(relabel_timeout(deadline, limits.timeout, error)))
}

/// Execute an in-memory ES module with the same limits and host surface as a
/// server plugin, without persisting a PluginDefinition. Skill packages use
/// this path for fast, reloadable computational tasks.
pub async fn invoke_ephemeral_server_script(
    script: String,
    method: String,
    args: serde_json::Value,
    capabilities: Vec<String>,
) -> anyhow::Result<(serde_json::Value, Vec<String>)> {
    invoke_ephemeral_server_script_with_limits(
        script,
        method,
        args,
        capabilities,
        ScriptExecutionLimits::default(),
    )
    .await
}

pub async fn invoke_ephemeral_server_script_with_limits(
    script: String,
    method: String,
    args: serde_json::Value,
    capabilities: Vec<String>,
    limits: ScriptExecutionLimits,
) -> anyhow::Result<(serde_json::Value, Vec<String>)> {
    invoke_ephemeral_server_script_with_extensions(
        script,
        method,
        args,
        capabilities,
        limits,
        HostExtensions::default(),
    )
    .await
}

/// Тот же одноразовый модуль, но с расширенным `host`. По этому пути идут Этапы
/// механизма Процессов: им нужна ветка `host.actions`, которой у навыков и
/// плагинов нет.
pub async fn invoke_ephemeral_server_script_with_extensions(
    script: String,
    method: String,
    args: serde_json::Value,
    capabilities: Vec<String>,
    limits: ScriptExecutionLimits,
    extensions: HostExtensions,
) -> anyhow::Result<(serde_json::Value, Vec<String>)> {
    use chrono::Utc;
    use contracts::plugins::{
        DataBinding, PluginBundle, PluginDataMode, PluginManifest, PluginRunContext, PluginRuntime,
        PluginStatus, ViewSpec,
    };
    let now = Utc::now();
    let definition = PluginDefinition {
        id: "ephemeral-skill-task".to_string(),
        bundle: PluginBundle {
            manifest: PluginManifest {
                code: "ephemeral-skill-task".to_string(),
                title: "Ephemeral skill task".to_string(),
                runtime: PluginRuntime::Server,
                api_version: "2".to_string(),
                description: None,
                capabilities,
                client_kits: Some(vec![]),
                built_for_migration: None,
            },
            params: Vec::new(),
            data: DataBinding::default(),
            client_script: None,
            server_script: Some(script),
            view_spec: ViewSpec::default(),
            styles: None,
            sql_resources: Default::default(),
            assets: Default::default(),
        },
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
    };
    invoke_server_method_with_extensions(
        definition,
        PluginInvokeRequest {
            method,
            args,
            context: PluginRunContext::default(),
            data_mode: PluginDataMode::Live,
        },
        limits,
        extensions,
    )
    .await
}

/// Скомпилировать ES-модуль и перечислить его экспорты **без вызова** функций.
///
/// `stage_prefix` подставляется в `stage` ошибок (пусто для серверного модуля,
/// `client_` для клиентского), чтобы агент отличал, какой скрипт не собрался.
/// Stub-глобалы (`__hostDbQuery`/`__hostLog`) позволяют исполнить верхний уровень
/// модуля без доступа к БД и журналу; DOM не мокается — обращение к нему на верхнем
/// уровне справедливо считается ошибкой.
async fn compile_module_exports(
    script: &str,
    stage_prefix: &str,
) -> Result<Vec<String>, PluginError> {
    let stage = format!("{stage_prefix}module_eval");
    let limits = ScriptExecutionLimits::default();
    let (runtime, deadline) = limited_runtime(limits)
        .await
        .map_err(|error| PluginError::new(stage.clone(), error.to_string()))?;
    let js_context = AsyncContext::full(&runtime)
        .await
        .map_err(|error| PluginError::new(stage.clone(), error.to_string()))?;

    let script = script.to_string();
    let stage_for_block = stage.clone();
    let result: Result<Vec<String>, PluginError> = js_context
        .async_with(async move |ctx| {
            let stage = stage_for_block.as_str();
            let globals = ctx.globals();
            globals
                .set(
                    "__hostDbQuery",
                    Func::from(|_: String, _: String| "[]".to_string()),
                )
                .catch(&ctx)
                .map_err(|error| js_error(stage, &error))?;
            globals
                .set("__hostLog", Func::from(|_: String| {}))
                .catch(&ctx)
                .map_err(|error| js_error(stage, &error))?;

            let declared = Module::declare(ctx.clone(), "plugin-module.js", script)
                .catch(&ctx)
                .map_err(|error| js_error(stage, &error))?;
            let (module, evaluation) = declared
                .eval()
                .catch(&ctx)
                .map_err(|error| js_error(stage, &error))?;
            evaluation
                .into_future::<()>()
                .await
                .catch(&ctx)
                .map_err(|error| js_error(stage, &error))?;

            let namespace = module
                .namespace()
                .catch(&ctx)
                .map_err(|error| js_error(stage, &error))?;
            let mut exports = namespace
                .keys::<String>()
                .collect::<rquickjs::Result<Vec<String>>>()
                .catch(&ctx)
                .map_err(|error| js_error(stage, &error))?;
            exports.sort();
            Ok(exports)
        })
        .await;

    runtime.idle().await;
    result.map_err(|error| relabel_timeout(deadline, limits.timeout, error))
}

/// Скомпилировать серверный ES-модуль и перечислить экспортированные функции
/// **без вызова** какой-либо из них. Используется `POST /api/plugin/validate`
/// для быстрой петли обратной связи (в т.ч. при доработке плагина из чата).
pub async fn validate_server_script(script: &str) -> PluginValidateReport {
    match compile_module_exports(script, "").await {
        Ok(server_exports) => PluginValidateReport {
            ok: true,
            server_exports,
            ..Default::default()
        },
        Err(error) => PluginValidateReport {
            ok: false,
            errors: vec![error],
            ..Default::default()
        },
    }
}

/// Скомпилировать клиентский ES-модуль (UI iframe) и убедиться, что он экспортирует
/// `mount`. Реального рендера нет (в QuickJS нет DOM) — это статическая проверка
/// контракта для самопроверки агента до передачи плагина пользователю.
pub async fn validate_client_script(script: &str) -> PluginValidateReport {
    match compile_module_exports(script, "client_").await {
        Ok(client_exports) => {
            if client_exports.iter().any(|name| name == "mount") {
                PluginValidateReport {
                    ok: true,
                    client_exports,
                    ..Default::default()
                }
            } else {
                PluginValidateReport {
                    ok: false,
                    errors: vec![PluginError::new(
                        "client_missing_export",
                        "client_script должен экспортировать async function mount(root, host)",
                    )],
                    client_exports,
                    ..Default::default()
                }
            }
        }
        Err(error) => PluginValidateReport {
            ok: false,
            errors: vec![error],
            ..Default::default()
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use contracts::plugins::{
        DataBinding, PluginBundle, PluginManifest, PluginRunContext, PluginRuntime, PluginStatus,
        ViewSpec,
    };

    fn test_plugin(script: &str) -> PluginDefinition {
        PluginDefinition {
            id: "test".to_string(),
            bundle: PluginBundle {
                manifest: PluginManifest {
                    code: "TEST".to_string(),
                    title: "Test".to_string(),
                    runtime: PluginRuntime::Server,
                    api_version: "2".to_string(),
                    description: None,
                    capabilities: vec!["db:read:*".into()],
                    client_kits: Some(vec![]),
                    built_for_migration: None,
                },
                params: vec![],
                data: DataBinding::default(),
                client_script: None,
                server_script: Some(script.to_string()),
                view_spec: ViewSpec::default(),
                styles: None,
                sql_resources: Default::default(),
                assets: Default::default(),
            },
            status: PluginStatus::Active,
            is_enabled: true,
            owner_user_id: None,
            created_by_agent_id: None,
            version: 1,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            rating: None,
            snapshot: None,
            s3_published_version: None,
            s3_published_at: None,
        }
    }

    #[tokio::test]
    async fn invokes_async_export_and_captures_log() {
        let def = test_plugin(
            r#"
export async function echo(args, host) {
  host.log.info("echo", args.value);
  return { value: args.value, contextValue: host.context.params.test };
}
"#,
        );
        let request = PluginInvokeRequest {
            method: "echo".to_string(),
            args: serde_json::json!({ "value": 42 }),
            context: PluginRunContext {
                params: [("test".to_string(), "ok".to_string())]
                    .into_iter()
                    .collect(),
                ..Default::default()
            },
            data_mode: contracts::plugins::PluginDataMode::Live,
        };

        let (result, logs) = invoke_server_method(def, request).await.unwrap();
        assert_eq!(
            result,
            serde_json::json!({ "value": 42, "contextValue": "ok" })
        );
        assert_eq!(logs, vec!["echo 42"]);
    }

    #[tokio::test]
    async fn reports_unknown_sql_resource() {
        let def = test_plugin(
            r#"
export async function run(_args, host) {
  return await host.db.queryResource("missing");
}
"#,
        );
        let request = PluginInvokeRequest {
            method: "run".to_string(),
            args: serde_json::Value::Null,
            context: PluginRunContext::default(),
            data_mode: contracts::plugins::PluginDataMode::Live,
        };

        let error = invoke_server_method(def, request).await.unwrap_err();
        assert!(error.to_string().contains("SQL resource 'missing'"));
    }

    #[tokio::test]
    async fn times_out_on_infinite_loop() {
        let def = test_plugin("export function spin() { while (true) {} }");
        let request = PluginInvokeRequest {
            method: "spin".to_string(),
            args: serde_json::Value::Null,
            context: PluginRunContext::default(),
            data_mode: contracts::plugins::PluginDataMode::Live,
        };

        let error = invoke_server_method(def, request).await.unwrap_err();
        let detail = error
            .downcast_ref::<PluginError>()
            .expect("error should carry PluginError");
        assert_eq!(detail.stage, "timeout");
    }

    #[tokio::test]
    async fn invoke_error_has_stage_and_stack() {
        let def = test_plugin(
            r#"
export function boom() {
  throw new Error("kaboom");
}
"#,
        );
        let request = PluginInvokeRequest {
            method: "boom".to_string(),
            args: serde_json::Value::Null,
            context: PluginRunContext::default(),
            data_mode: contracts::plugins::PluginDataMode::Live,
        };

        let error = invoke_server_method(def, request).await.unwrap_err();
        let detail = error
            .downcast_ref::<PluginError>()
            .expect("error should carry PluginError");
        assert_eq!(detail.stage, "invoke");
        assert!(detail.message.contains("kaboom"));
        assert!(detail.stack.is_some(), "expected a JS stack trace");
    }

    #[tokio::test]
    async fn validate_lists_exports() {
        let report = validate_server_script(
            r#"
export async function alpha(args, host) { return 1; }
export function beta() { return 2; }
"#,
        )
        .await;
        assert!(report.ok, "errors: {:?}", report.errors);
        assert_eq!(report.server_exports, vec!["alpha", "beta"]);
    }

    #[tokio::test]
    async fn validate_reports_syntax_error() {
        let report = validate_server_script("export async function broken( {").await;
        assert!(!report.ok);
        assert_eq!(
            report.errors.first().map(|e| e.stage.as_str()),
            Some("module_eval")
        );
    }

    #[tokio::test]
    async fn validate_client_lists_exports_and_accepts_mount() {
        let report = validate_client_script(
            r#"
export async function mount(root, host) {
  const rows = await host.invoke("load");
  root.textContent = JSON.stringify(rows);
}
export function unmount() {}
"#,
        )
        .await;
        assert!(report.ok, "errors: {:?}", report.errors);
        assert_eq!(report.client_exports, vec!["mount", "unmount"]);
    }

    #[tokio::test]
    async fn validate_client_requires_mount_export() {
        let report = validate_client_script("export function render() {}").await;
        assert!(!report.ok);
        assert_eq!(
            report.errors.first().map(|e| e.stage.as_str()),
            Some("client_missing_export")
        );
    }

    #[tokio::test]
    async fn validate_client_reports_syntax_error_with_prefix() {
        let report = validate_client_script("export async function mount( {").await;
        assert!(!report.ok);
        assert_eq!(
            report.errors.first().map(|e| e.stage.as_str()),
            Some("client_module_eval")
        );
    }

    #[tokio::test]
    async fn ephemeral_skill_script_uses_limited_quickjs_runtime() {
        let (result, logs) = invoke_ephemeral_server_script(
            r#"
export async function run(args, host) {
  host.log.info("skill-task", args.value);
  return { doubled: args.value * 2 };
}
"#
            .to_string(),
            "run".to_string(),
            serde_json::json!({ "value": 21 }),
            vec!["network:none".to_string()],
        )
        .await
        .expect("ephemeral script");
        assert_eq!(result["doubled"], 42);
        assert_eq!(logs, vec!["skill-task 21"]);
    }

    #[test]
    fn extracts_table_refs_for_capability_checks() {
        assert_eq!(
            inspect_read_query("SELECT * FROM a004_nomenclature n JOIN p900_x x ON 1=1")
                .unwrap()
                .tables,
            vec!["a004_nomenclature".to_string(), "p900_x".to_string()]
        );
    }

    #[test]
    fn capability_blocks_unauthorized_tables() {
        let caps = vec![PluginCapability::DbRead("ref".into())];
        assert!(enforce_sql_capabilities("SELECT * FROM a004_nomenclature", &caps).is_ok());
        let error = enforce_sql_capabilities("SELECT * FROM plugin", &caps).unwrap_err();
        assert!(error.contains("plugin"), "got: {error}");
    }

    #[test]
    fn read_sql_is_wrapped_with_hard_limit() {
        assert_eq!(
            limit_read_sql("SELECT 1 AS value;"),
            format!(
                "SELECT * FROM (SELECT 1 AS value) AS plugin_limited_result LIMIT {READ_ROW_LIMIT}"
            )
        );
    }

    /// Право на Действие держится проверкой, а не отсутствием метода.
    ///
    /// Сахар `host.actions` заводит только выданные методы, но сырой
    /// `__hostActionCall` остаётся достижимым глобалом — и до этой проверки
    /// Этап, которому выдали одно Действие, мог позвать любое из каталога.
    #[tokio::test]
    async fn ungranted_action_is_rejected_before_the_handler() {
        let seen: Arc<Mutex<Vec<String>>> = Arc::default();
        let recorder = seen.clone();
        let handler: HostActionHandler = Arc::new(move |name: String, _input, _options| {
            let recorder = recorder.clone();
            Box::pin(async move {
                recorder.lock().unwrap().push(name);
                Ok(serde_json::json!({ "done": true }))
            })
        });

        let error = invoke_ephemeral_server_script_with_extensions(
            r#"
export async function run(_args, _host) {
  // Мимо сахара: имени `repostDocuments` в `host.actions` нет, но глобал есть.
  return await __hostActionCall("repost_documents", "{}", "{}");
}
"#
            .to_string(),
            "run".to_string(),
            serde_json::json!({}),
            vec!["action:request_human_action".to_string()],
            ScriptExecutionLimits::default(),
            HostExtensions {
                actions: Some((
                    vec![HostActionEntry {
                        name: "request_human_action".to_string(),
                        method: "requestHumanAction".to_string(),
                    }],
                    handler,
                )),
            },
        )
        .await
        .expect_err("невыданное Действие обязано быть отклонено");

        assert!(
            error.to_string().contains("repost_documents"),
            "ошибка должна называть отклонённое Действие: {error}"
        );
        // Главное: обработчик не позвали вовсе. Отказ обязан случиться до
        // `actions::run`, иначе эффект успел бы записаться в журнал.
        assert!(
            seen.lock().unwrap().is_empty(),
            "обработчик эффекта не должен вызываться для невыданного Действия"
        );
    }

    /// А выданное — проходит насквозь.
    #[tokio::test]
    async fn granted_action_reaches_the_handler() {
        let seen: Arc<Mutex<Vec<String>>> = Arc::default();
        let recorder = seen.clone();
        let handler: HostActionHandler = Arc::new(move |name: String, _input, _options| {
            let recorder = recorder.clone();
            Box::pin(async move {
                recorder.lock().unwrap().push(name);
                Ok(serde_json::json!({ "done": true }))
            })
        });

        let (result, _logs) = invoke_ephemeral_server_script_with_extensions(
            r#"
export async function run(_args, host) {
  return await host.actions.requestHumanAction({ title: "t" }, { key: "k" });
}
"#
            .to_string(),
            "run".to_string(),
            serde_json::json!({}),
            vec!["action:request_human_action".to_string()],
            ScriptExecutionLimits::default(),
            HostExtensions {
                actions: Some((
                    vec![HostActionEntry {
                        name: "request_human_action".to_string(),
                        method: "requestHumanAction".to_string(),
                    }],
                    handler,
                )),
            },
        )
        .await
        .expect("выданное Действие обязано пройти");

        assert_eq!(result.get("done").and_then(|v| v.as_bool()), Some(true));
        assert_eq!(seen.lock().unwrap().as_slice(), ["request_human_action"]);
    }
}
