//! Инструменты для агента-разработчика плагинов (`AgentType::PluginAdmin`).
//!
//! Позволяют создавать, читать, валидировать, сохранять и тестировать JS-плагины
//! прямо из чата в рантайме. Единица переноса плагина между экземплярами — `bundle`
//! (ключ идентичности — `manifest.code`); локальное состояние (id/version/status)
//! не входит в переносимый артефакт. Любое сохранение атомарно: сначала полная
//! валидация бандла, затем единичный upsert.

use super::types::ToolDefinition;
use crate::plugins::service;
use contracts::plugins::{
    PluginBundle, PluginError, PluginInvokeRequest, PluginSmokeRequest, PluginUpsert,
};
use serde_json::{json, Value};

/// Имена инструментов разработчика плагинов (для guard'а в диспетчере).
pub const PLUGIN_TOOL_NAMES: &[&str] = &[
    "plugin_list",
    "plugin_get",
    "plugin_validate",
    "plugin_smoke_test",
    "plugin_upsert",
    "plugin_invoke",
    "plugin_template",
    "plugin_examples",
    "get_plugin_ui_contract",
    "get_flow_ui_contract",
    "plugin_data_catalog",
    "plugin_runs",
];

/// Краткое описание формы bundle для всех инструментов, принимающих его на вход.
fn bundle_schema() -> Value {
    json!({
        "type": "object",
        "description": "Самодостаточный переносимый артефакт плагина. Ключ идентичности — manifest.code.",
        "properties": {
            "manifest": {
                "type": "object",
                "properties": {
                    "code": { "type": "string", "description": "Бизнес-код плагина (стабильный идентификатор переноса)." },
                    "title": { "type": "string" },
                    "runtime": { "type": "string", "enum": ["client", "server", "hybrid"] },
                    "api_version": { "type": "string", "description": "Версия API движка плагинов, обычно \"2\"." },
                    "description": { "type": "string" },
                    "capabilities": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Runtime access scopes. Use db:read:* for prototypes, or db:read:<table/tag> for least privilege."
                    }
                },
                "required": ["code", "title", "runtime"]
            },
            "params": {
                "type": "array",
                "description": "Optional user parameters surfaced in PluginRunContext.params.",
                "items": {
                    "type": "object",
                    "properties": {
                        "key": { "type": "string" },
                        "param_type": { "type": "string", "enum": ["date", "date_range", "string", "integer", "float", "boolean", "ref"] },
                        "label": { "type": "string" },
                        "default_value": { "type": "string" },
                        "required": { "type": "boolean" }
                    },
                    "required": ["key", "param_type", "label"]
                }
            },
            "client_script": { "type": "string", "description": "ES-модуль iframe; export async function mount(root, host)." },
            "server_script": { "type": "string", "description": "ES-модуль QuickJS; экспортированные async-функции (args, host)." },
            "sql_resources": {
                "type": "object",
                "description": "Имя → SQL (ТОЛЬКО SELECT/WITH). Доступ: host.db.queryResource(name, params).",
                "additionalProperties": { "type": "string" }
            },
            "styles": { "type": "string", "description": "CSS внутри iframe." }
        },
        "required": ["manifest"]
    })
}

/// Определения инструментов для PluginAdmin.
pub fn plugin_tool_definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "plugin_list".into(),
            description: "Список плагинов платформы: id, code, title, runtime, status, enabled, version."
                .into(),
            parameters: json!({ "type": "object", "properties": {} }),
        },
        ToolDefinition {
            name: "plugin_get".into(),
            description: "Получить полное определение плагина по id ИЛИ code. Поле `bundle` — \
                          переносимый артефакт (его и правь), `local` — состояние экземпляра \
                          (id/version/status/is_enabled)."
                .into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "id": { "type": "string", "description": "UUID плагина." },
                    "code": { "type": "string", "description": "Бизнес-код (manifest.code). Альтернатива id." }
                }
            }),
        },
        ToolDefinition {
            name: "plugin_validate".into(),
            description: "Проверить bundle БЕЗ сохранения: компиляция серверного И клиентского \
                          модулей, перечень экспортов, проверка SQL. Для client/hybrid также \
                          проверяется, что client_script экспортирует mount. Возвращает \
                          { ok, server_exports, client_exports, errors:[{stage,message,stack}] }; \
                          стадии client-ошибок: client_module_eval, client_missing_export. \
                          Вызывай перед plugin_upsert."
                .into(),
            parameters: json!({
                "type": "object",
                "properties": { "bundle": bundle_schema() },
                "required": ["bundle"]
            }),
        },
        ToolDefinition {
            name: "plugin_smoke_test".into(),
            description: "One-call preflight for LLM plugin authoring: validates bundle/id, checks client host.invoke names against server exports, runs server methods with context, and returns structured failures plus suggested_next_step."
                .into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "id": { "type": "string", "description": "Existing plugin UUID. Alternative to bundle." },
                    "bundle": bundle_schema(),
                    "context": { "type": "object", "description": "PluginRunContext: date_from/date_to/connection_mp_refs/group_by/params." },
                    "methods": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "method": { "type": "string" },
                                "args": { "type": "object" }
                            },
                            "required": ["method"]
                        }
                    },
                    "render": { "type": "boolean", "description": "When true, requires client mount and checks host.invoke wiring." }
                }
            }),
        },
        ToolDefinition {
            name: "plugin_upsert".into(),
            description: "Создать или обновить плагин. Если id не задан, идентичность берётся по \
                          manifest.code (idempotent upsert-by-code). Бандл валидируется ПЕРЕД \
                          сохранением (server + client, включая mount) — невалидный плагин не \
                          сохраняется. По умолчанию статус draft. Создаёт в чате карточку-превью \
                          (кнопки «Превью»/«Редактор»). Возвращает { ok, id, version, validate, artifact_id }."
                .into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "bundle": bundle_schema(),
                    "id": { "type": "string", "description": "UUID для обновления конкретной записи (необязательно)." },
                    "status": { "type": "string", "enum": ["draft", "active", "disabled"], "description": "По умолчанию draft при создании." },
                    "is_enabled": { "type": "boolean" }
                },
                "required": ["bundle"]
            }),
        },
        ToolDefinition {
            name: "plugin_invoke".into(),
            description: "Запустить экспортированный серверный метод плагина для теста (работает и \
                          для draft/disabled — это dev-вызов). Возвращает { result, logs } либо \
                          { error, error_detail:{ stage, message, stack } }."
                .into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "id": { "type": "string", "description": "UUID плагина." },
                    "method": { "type": "string", "description": "Имя экспортированной функции server_script." },
                    "args": { "type": "object", "description": "Аргументы метода (первый параметр функции)." }
                },
                "required": ["id", "method"]
            }),
        },
        ToolDefinition {
            name: "plugin_template".into(),
            description: "Получить минимальный ВАЛИДНЫЙ скелет bundle для выбранного runtime \
                          (client | server | hybrid). Начинай новый плагин с шаблона, затем \
                          заполняй логику и SQL. Возвращает { bundle, hint }."
                .into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "runtime": { "type": "string", "enum": ["client", "server", "hybrid"], "description": "Тип плагина. По умолчанию hybrid." }
                }
            }),
        },
        ToolDefinition {
            name: "plugin_examples".into(),
            description: "Получить готовый рабочий ПРИМЕР плагина (hybrid: server_script + \
                          client_script + sql_resources + styles), оформленный по UI-контракту. \
                          Используй как образец структуры и стиля перед написанием своего. \
                          Возвращает { examples:[{ title, bundle }] }."
                .into(),
            parameters: json!({ "type": "object", "properties": {} }),
        },
        ToolDefinition {
            name: "get_plugin_ui_contract".into(),
            description: "Получить UI-контракт iframe: готовые CSS-классы/компоненты \
                          (.card, .table-wrap > table.data-table, .num, .stat*, .btn*, .badge*, \
                          .status*) и правила (DOM только внутри mount, тема подхватывается). \
                          Рендери UI этим китом, свой CSS — по минимуму."
                .into(),
            parameters: json!({ "type": "object", "properties": {} }),
        },
        ToolDefinition {
            name: "get_flow_ui_contract".into(),
            description: "Контракт кита flow (граф/схема): манифест client_kits, вызовы \
                          PluginFlow.render/getFlow/setFlow/autoLayout, форма узлов и рёбер, \
                          и правила host.loadDocument/saveDocument для редактируемого поля."
                .into(),
            parameters: json!({ "type": "object", "properties": {} }),
        },
        ToolDefinition {
            name: "plugin_data_catalog".into(),
            description: "Safe data-source catalog for miniapp plugins: common tags/tables, required db:read capabilities, and SQL starter snippets."
                .into(),
            parameters: json!({ "type": "object", "properties": {} }),
        },
        ToolDefinition {
            name: "plugin_runs".into(),
            description: "Журнал запусков плагина за последние дни: сводка (total, errors, \
                          error_rate, avg_ms, health) и последние запуски (method, status, \
                          error_stage, duration). Используй для самокоррекции: проверь, не падает \
                          ли серверный метод после plugin_invoke/в проде."
                .into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "id": { "type": "string", "description": "UUID плагина." },
                    "days": { "type": "integer", "description": "Окно в днях (по умолчанию 7)." }
                },
                "required": ["id"]
            }),
        },
    ]
}

/// Диспетчер инструментов разработчика плагинов.
pub async fn execute_plugin_tool(
    name: &str,
    arguments: &str,
    chat_id: &str,
    agent_id: &str,
) -> Value {
    let args: Value = serde_json::from_str(arguments).unwrap_or_else(|_| json!({}));
    match name {
        "plugin_list" => plugin_list().await,
        "plugin_get" => plugin_get(&args).await,
        "plugin_validate" => plugin_validate(&args).await,
        "plugin_smoke_test" => plugin_smoke_test(&args).await,
        "plugin_upsert" => plugin_upsert(&args, chat_id, agent_id).await,
        "plugin_invoke" => plugin_invoke(&args).await,
        "plugin_template" => plugin_template(&args),
        "plugin_examples" => plugin_examples(),
        "get_plugin_ui_contract" => plugin_ui_contract(),
        "get_flow_ui_contract" => flow_ui_contract(),
        "plugin_data_catalog" => plugin_data_catalog(),
        "plugin_runs" => plugin_runs(&args).await,
        _ => json!({ "error": format!("Unknown plugin tool: '{}'", name) }),
    }
}

async fn plugin_list() -> Value {
    match service::list_all().await {
        Ok(defs) => json!({
            "plugins": defs.iter().map(|d| json!({
                "id": d.id,
                "code": d.bundle.manifest.code,
                "title": d.bundle.manifest.title,
                "runtime": d.bundle.manifest.runtime.as_str(),
                "status": d.status.as_str(),
                "is_enabled": d.is_enabled,
                "version": d.version,
            })).collect::<Vec<_>>()
        }),
        Err(e) => json!({ "error": e.to_string() }),
    }
}

async fn plugin_get(args: &Value) -> Value {
    let found = if let Some(id) = args.get("id").and_then(Value::as_str) {
        service::get_by_id(id).await
    } else if let Some(code) = args.get("code").and_then(Value::as_str) {
        service::get_by_code(code).await
    } else {
        return json!({ "error": "Укажите id или code" });
    };

    match found {
        Ok(Some(def)) => json!({
            "bundle": def.bundle,
            "local": {
                "id": def.id,
                "version": def.version,
                "status": def.status.as_str(),
                "is_enabled": def.is_enabled,
                "created_by_agent_id": def.created_by_agent_id,
                "updated_at": def.updated_at,
            },
            "hint": "Единица переноса — поле `bundle` (ключ идентичности manifest.code). \
                     Локальное состояние в `local` не переносится между экземплярами."
        }),
        Ok(None) => json!({ "error": "Плагин не найден" }),
        Err(e) => json!({ "error": e.to_string() }),
    }
}

fn parse_bundle(args: &Value) -> Result<PluginBundle, Value> {
    let raw = args
        .get("bundle")
        .ok_or_else(|| json!({ "error": "Отсутствует поле bundle" }))?;
    serde_json::from_value::<PluginBundle>(raw.clone())
        .map_err(|e| json!({ "error": format!("Некорректный bundle: {e}") }))
}

async fn plugin_validate(args: &Value) -> Value {
    let bundle = match parse_bundle(args) {
        Ok(b) => b,
        Err(e) => return e,
    };
    json!({ "validate": service::validate(&bundle).await })
}

async fn plugin_smoke_test(args: &Value) -> Value {
    let request = match serde_json::from_value::<PluginSmokeRequest>(args.clone()) {
        Ok(request) => request,
        Err(e) => return json!({ "ok": false, "error": format!("Invalid smoke request: {e}") }),
    };
    match service::smoke_test(request).await {
        Ok(report) => json!({ "ok": report.ok, "smoke": report }),
        Err(e) => json!({ "ok": false, "error": e.to_string() }),
    }
}

async fn plugin_upsert(args: &Value, chat_id: &str, agent_id: &str) -> Value {
    let bundle = match parse_bundle(args) {
        Ok(b) => b,
        Err(e) => return e,
    };
    upsert_bundle(
        bundle,
        args.get("id").and_then(Value::as_str).map(str::to_string),
        args.get("status")
            .and_then(Value::as_str)
            .map(str::to_string),
        args.get("is_enabled").and_then(Value::as_bool),
        chat_id,
        agent_id,
    )
    .await
}

/// Атомарно сохранить готовый bundle: валидация (server+client+SQL) → upsert-by-code →
/// карточка-превью в чате. Реюзается и `plugin_upsert`, и высокоуровневыми сборщиками
/// артефактов (напр. `build_chart`), чтобы поведение сохранения было единым.
/// Возвращает `{ ok, id, version, validate, artifact_id }` либо `{ ok:false, error, .. }`.
pub(crate) async fn upsert_bundle(
    bundle: PluginBundle,
    id: Option<String>,
    status: Option<String>,
    is_enabled: Option<bool>,
    chat_id: &str,
    agent_id: &str,
) -> Value {
    let capture_snapshot = bundle.data.source.is_some();
    upsert_bundle_with_snapshot(
        bundle,
        id,
        status,
        is_enabled,
        chat_id,
        agent_id,
        None,
        capture_snapshot,
        false,
    )
    .await
}

pub(crate) async fn upsert_bundle_with_snapshot(
    bundle: PluginBundle,
    id: Option<String>,
    status: Option<String>,
    is_enabled: Option<bool>,
    chat_id: &str,
    agent_id: &str,
    prepared_snapshot: Option<Value>,
    capture_snapshot: bool,
    allow_live_only: bool,
) -> Value {
    // Атомарность: невалидный плагин не сохраняется.
    let report = service::validate(&bundle).await;
    if !report.ok {
        return json!({ "ok": false, "error": "Валидация не пройдена", "validate": report });
    }

    // Идентичность по code: если id не задан, но плагин с таким code уже есть — обновляем его.
    let mut id = id;
    let mut version = None;
    if id.is_none() {
        if let Ok(Some(existing)) = service::get_by_code(&bundle.manifest.code).await {
            id = Some(existing.id);
            version = Some(existing.version);
        }
    }

    let dto = PluginUpsert {
        id,
        bundle,
        status,
        is_enabled,
        owner_user_id: None,
        created_by_agent_id: Some(agent_id.to_string()),
        version,
        capture_snapshot,
        allow_live_only,
    };

    let retry_dto = dto.clone();
    let retry_snapshot = prepared_snapshot.clone();
    let mut saved_result = service::upsert_prepared(
        dto,
        prepared_snapshot,
        Some(service::PluginArtifactOrigin { chat_id, agent_id }),
        Some(report.clone()),
        None,
    )
    .await;
    if saved_result.is_err() && retry_dto.id.is_none() {
        // Concurrent builders with the same stable code may both observe "not found".
        // The unique code constraint chooses a winner; the loser retries as an update.
        if let Ok(Some(existing)) = service::get_by_code(&retry_dto.bundle.manifest.code).await {
            let mut update = retry_dto;
            update.id = Some(existing.id);
            update.version = Some(existing.version);
            saved_result = service::upsert_prepared(
                update,
                retry_snapshot,
                Some(service::PluginArtifactOrigin { chat_id, agent_id }),
                Some(report.clone()),
                None,
            )
            .await;
        }
    }
    match saved_result {
        Ok(saved) => {
            let mut result = json!({
                "ok": true,
                "id": saved.id,
                "version": saved.version,
                "validate": report,
            });
            if let Some(id) = saved.artifact_id {
                result["artifact_id"] = Value::String(id);
            }
            result
        }
        Err(e) => json!({ "ok": false, "error": e.to_string() }),
    }
}

async fn plugin_invoke(args: &Value) -> Value {
    let Some(id) = args.get("id").and_then(Value::as_str) else {
        return json!({ "error": "Отсутствует id" });
    };
    let Some(method) = args.get("method").and_then(Value::as_str) else {
        return json!({ "error": "Отсутствует method" });
    };

    // Dev-вызов: тестируем плагин даже в статусе draft/disabled (минуя гейт активности).
    let _def = match service::get_by_id(id).await {
        Ok(Some(def)) => def,
        Ok(None) => return json!({ "error": "Плагин не найден" }),
        Err(e) => return json!({ "error": e.to_string() }),
    };

    let request = PluginInvokeRequest {
        method: method.to_string(),
        args: args.get("args").cloned().unwrap_or(Value::Null),
        context: args
            .get("context")
            .cloned()
            .and_then(|value| serde_json::from_value(value).ok())
            .unwrap_or_default(),
        data_mode: args
            .get("data_mode")
            .and_then(Value::as_str)
            .map(|mode| {
                if mode == "snapshot" {
                    contracts::plugins::PluginDataMode::Snapshot
                } else {
                    contracts::plugins::PluginDataMode::Live
                }
            })
            .unwrap_or_default(),
    };

    match service::dev_invoke(id, request).await {
        Ok((result, logs)) => json!({ "ok": true, "result": result, "logs": logs }),
        Err(e) => {
            let detail = e.downcast_ref::<PluginError>().cloned();
            json!({ "ok": false, "error": e.to_string(), "error_detail": detail })
        }
    }
}

// ─── Шаблоны и пример (few-shot для генерации) ───────────────────────────────

const TPL_CLIENT: &str = r##"export async function mount(root, host) {
  root.innerHTML = `<div class="card">Привет из плагина</div>`;
}
"##;

const TPL_SERVER: &str = r##"export async function run(args, host) {
  return await host.db.queryResource("rows", []);
}
"##;

const HYBRID_CLIENT: &str = r##"export async function mount(root, host) {
  root.innerHTML = `<div class="card"><div class="status">Загрузка…</div></div>`;
  try {
    const rows = await host.invoke("loadRows", {});
    root.innerHTML = `
      <div class="table-wrap"><table class="data-table">
        <thead><tr><th>Артикул</th><th class="num">Значение</th></tr></thead>
        <tbody>${rows.map(r => `<tr><td>${r.article}</td><td class="num">${r.value}</td></tr>`).join("")}</tbody>
      </table></div>`;
  } catch (e) {
    root.innerHTML = `<div class="status status--error">${e.message}</div>`;
  }
}
"##;

const HYBRID_SERVER: &str = r##"export async function loadRows(args, host) {
  host.log.info("loadRows called");
  return await host.db.queryResource("rows", []);
}
"##;

/// Демо-SELECT без зависимости от схемы — пример сразу запускается через plugin_invoke.
const EXAMPLE_SQL: &str = "SELECT 'ABC-1' AS article, 1200 AS value UNION ALL SELECT 'ABC-2', 980";

const EXAMPLE_STYLES: &str = ".card { padding: 16px; }";

/// Минимальный валидный скелет bundle по runtime.
fn plugin_template(args: &Value) -> Value {
    let runtime = args
        .get("runtime")
        .and_then(Value::as_str)
        .unwrap_or("hybrid");
    let bundle = match runtime {
        "client" => json!({
            "manifest": { "code": "MY-PLUGIN", "title": "Мой плагин", "runtime": "client", "api_version": "2" },
            "client_script": TPL_CLIENT,
        }),
        "server" => json!({
            "manifest": { "code": "MY-PLUGIN", "title": "Мой плагин", "runtime": "server", "api_version": "2" },
            "server_script": TPL_SERVER,
            "sql_resources": { "rows": "SELECT 1 AS value" },
        }),
        _ => json!({
            "manifest": { "code": "MY-PLUGIN", "title": "Мой плагин", "runtime": "hybrid", "api_version": "2" },
            "client_script": HYBRID_CLIENT,
            "server_script": HYBRID_SERVER,
            "sql_resources": { "rows": "SELECT 1 AS value" },
        }),
    };
    json!({
        "bundle": bundle,
        "runtime": runtime,
        "hint": "Минимальный валидный скелет. Замени code/title, впиши логику и реальный SELECT \
                 в sql_resources, затем plugin_validate → plugin_invoke."
    })
}

/// Готовый рабочий пример (hybrid) — образец структуры и стиля.
fn plugin_examples() -> Value {
    let bundle = json!({
        "manifest": {
            "code": "EXAMPLE-MARGIN-TABLE",
            "title": "Пример: таблица из серверного метода",
            "runtime": "hybrid",
            "api_version": "2",
            "description": "Канонический hybrid-плагин: server_script тянет данные, client_script рисует таблицу по UI-контракту."
        },
        "client_script": HYBRID_CLIENT,
        "server_script": HYBRID_SERVER,
        "sql_resources": { "rows": EXAMPLE_SQL },
        "styles": EXAMPLE_STYLES,
    });
    json!({
        "examples": [
            { "title": "Hybrid: таблица из серверного метода", "bundle": bundle }
        ],
        "hint": "Скопируй структуру, замени SQL на реальный (проверь через execute_query). \
                 Имена в host.invoke(\"X\") должны совпадать с export-функциями server_script."
    })
}

/// UI-контракт iframe: CSS-кит и правила рендера.
fn plugin_ui_contract() -> Value {
    json!({
        "entry": "export async function mount(root, host) { … } — единственная точка входа; DOM трогай только ВНУТРИ mount (на верхнем уровне модуля DOM нет).",
        "data": "Данные с сервера: const rows = await host.invoke(\"methodName\", { … }).",
        "theme": "Тема (свет/тёмная) подхватывается автоматически — не хардкодь цвета.",
        "components": {
            "card":   ".card — контейнер-карточка.",
            "table":  ".table-wrap > table.data-table; числовые ячейки (th и td) — класс .num.",
            "stat":   ".stat / .stat__label / .stat__value — плитка-метрика; модификаторы .stat--ok / .stat--bad.",
            "button": ".btn / .btn--secondary / .btn--ghost.",
            "badge":  ".badge / .badge--success / .badge--error.",
            "status": ".status / .status--ok / .status--error — строка статуса и вывод ошибок."
        },
        "hint": "Рендери этим китом, свой CSS — по минимуму. Ошибки показывай в .status--error."
    })
}

/// Контракт кита `flow` — редактор графов и запись документа.
fn flow_ui_contract() -> Value {
    json!({
        "manifest": "Объяви kit: manifest.client_kits = [\"flow\"]. Хост грузит киты по объявлению, \
                     так что без этого window.PluginFlow в iframe не появится. Лишние киты не объявляй \
                     — каждый парсится заново в каждом открытом iframe.",
        "render": "const flow = PluginFlow.render(container, { nodes, edges }, options); \
                   Контейнеру нужна ЯВНАЯ высота (height: 100%), min-height ReactFlow не хватает.",
        "options": {
            "editable": "bool (по умолчанию true) — перетаскивание, соединение, удаление, переименование.",
            "onDirtyChange": "(dirty) => host.setDirty(dirty) — срабатывает на переходе, а не на каждое движение.",
            "autoLayout": "\"missing\" (по умолчанию) | \"always\" | \"never\" — раскладка dagre, когда координат нет.",
            "minimap": "bool, toolbar: bool — выключить встроенные элементы."
        },
        "controller": {
            "getFlow": "() => { nodes, edges } — то, что нужно сохранить.",
            "setFlow": "(spec) => подменить граф целиком; снимает грязный флаг.",
            "markSaved": "() => после успешного сохранения.",
            "autoLayout": "() => разложить dagre.",
            "fitView": "() => вписать в экран.",
            "destroy": "() => зови в unmount()."
        },
        "shape": {
            "node": "{ id: string, position: { x, y }, data: { label, kind? } } — position можно не задавать: разложит сам.",
            "edge": "{ id?, source, target, label? }"
        },
        "document": {
            "load": "const { content, version } = await host.loadDocument();",
            "save": "const { version } = await host.saveDocument(flow.getFlow());",
            "authority": "Адрес поля задаёт ХОСТ и в аргументы не передаётся — плагин не выбирает, куда писать.",
            "errors": "Отказ приходит при: плагин открыт без привязки к документу; режим \"Снимок\"; \
                       расхождение версий. При конфликте версий НЕ повторяй запись вслепую — \
                       перечитай loadDocument и покажи пользователю, что документ изменился."
        },
        "validation": "PluginFlow.validateSpec(spec) => { ok, errors } — только структура (дубли id, \
                       рёбра в несуществующие узлы). Доменных правил «что с чем соединяется» в ките нет.",
        "theme": "Цвета берутся из токенов приложения через --xy-*; не хардкодь цвета и не зови applyTheme."
    })
}

/// Журнал запусков плагина — серверная наблюдаемость для самокоррекции.
fn plugin_data_catalog() -> Value {
    json!({
        "capabilities": {
            "prototype": ["db:read:*", "network:none"],
            "least_privilege_examples": [
                { "need": "reference data", "capabilities": ["db:read:ref", "network:none"] },
                { "need": "Wildberries sales/orders/projections", "capabilities": ["db:read:wb", "network:none"] },
                { "need": "BI dashboards and indicators", "capabilities": ["db:read:bi", "network:none"] },
                { "need": "general ledger", "capabilities": ["db:read:gl", "network:none"] }
            ]
        },
        "safe_sources": [
            {
                "tag": "ref",
                "tables": ["a002_organization", "a004_nomenclature", "a005_marketplace", "a006_connection_mp"],
                "starter_sql": "SELECT id, code, description FROM a004_nomenclature WHERE is_deleted = 0 LIMIT 50"
            },
            {
                "tag": "wb",
                "tables": ["a012_wb_sales", "a015_wb_orders", "a020_wb_promotion", "a026_wb_advert_daily", "p909_mp_order_line_turnovers", "p914_mp_finance_turnovers"],
                "starter_sql": "SELECT sale_date, supplier_article, qty, total_price FROM a012_wb_sales WHERE is_deleted = 0 ORDER BY sale_date DESC LIMIT 50"
            },
            {
                "tag": "gl",
                "tables": ["sys_general_ledger"],
                "starter_sql": "SELECT business_date, turnover_code, debit_account, credit_account, amount FROM sys_general_ledger ORDER BY business_date DESC LIMIT 50"
            },
            {
                "tag": "bi",
                "tables": ["a024_bi_indicator", "a025_bi_dashboard"],
                "starter_sql": "SELECT code, description FROM a024_bi_indicator WHERE is_deleted = 0 LIMIT 50"
            }
        ],
        "hint": "Start with execute_query to prove SQL, then put the SELECT into sql_resources and include matching manifest.capabilities."
    })
}

async fn plugin_runs(args: &Value) -> Value {
    let Some(id) = args.get("id").and_then(Value::as_str) else {
        return json!({ "error": "Отсутствует id" });
    };
    let days = args
        .get("days")
        .and_then(Value::as_i64)
        .unwrap_or(7)
        .clamp(1, 90);
    match service::stats(id, days).await {
        Ok(stats) => json!({ "ok": true, "stats": stats }),
        Err(e) => json!({ "error": e.to_string() }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Атомарность: невалидный server_script отклоняется до обращения к БД
    /// (валидация раньше upsert), плагин не сохраняется.
    #[tokio::test]
    async fn upsert_rejects_invalid_bundle_before_persist() {
        let args = json!({
            "bundle": {
                "manifest": { "code": "T-BROKEN", "title": "T", "runtime": "server", "api_version": "2" },
                "server_script": "export async function broken( {"
            }
        });
        let result =
            execute_plugin_tool("plugin_upsert", &args.to_string(), "chat-1", "agent-1").await;
        assert_eq!(result["ok"], json!(false));
        assert_eq!(result["validate"]["ok"], json!(false));
        assert!(
            result.get("id").is_none(),
            "битый плагин не должен сохраняться"
        );
    }

    /// Скелеты из plugin_template должны парситься и проходить базовую валидацию.
    #[test]
    fn templates_are_valid_bundles() {
        for rt in ["client", "server", "hybrid"] {
            let out = plugin_template(&json!({ "runtime": rt }));
            let bundle: PluginBundle = serde_json::from_value(out["bundle"].clone())
                .unwrap_or_else(|e| panic!("template {rt} bundle parse: {e}"));
            bundle
                .validate()
                .unwrap_or_else(|e| panic!("template {rt} invalid: {e}"));
        }
    }

    /// Образец из plugin_examples должен быть валидным bundle.
    #[test]
    fn example_is_valid_bundle() {
        let out = plugin_examples();
        let bundle: PluginBundle = serde_json::from_value(out["examples"][0]["bundle"].clone())
            .expect("example bundle parse");
        bundle.validate().expect("example invalid");
    }

    #[tokio::test]
    async fn smoke_test_reports_missing_server_export_for_client_invoke() {
        let args = json!({
            "render": true,
            "bundle": {
                "manifest": {
                    "code": "T-SMOKE",
                    "title": "Smoke",
                    "runtime": "hybrid",
                    "api_version": "2",
                    "capabilities": ["network:none"]
                },
                "client_script": "export async function mount(root, host) { await host.invoke(\"missingMethod\", {}); }",
                "server_script": "export function loadRows() { return []; }"
            },
            "methods": []
        });
        let result = plugin_smoke_test(&args).await;
        assert_eq!(result["ok"], json!(false));
        let failures = result["smoke"]["failures"].as_array().unwrap();
        assert!(failures
            .iter()
            .any(|failure| { failure["stage"] == json!("client_missing_server_export") }));
    }
}
