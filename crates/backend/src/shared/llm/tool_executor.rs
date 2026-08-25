//! Исполнитель инструментов (tool calls) для LLM.
//!
//! Содержит:
//! - определения общих metadata-инструментов для передачи LLM
//! - диспетчер выполнения (`execute_tool_call`)

use super::admin_tools::{execute_admin_tool, ADMIN_TOOL_NAMES};
use super::chart_tools::{execute_chart_tool, CHART_TOOL_NAMES};
use super::data_tools::{execute_data_tool, DATA_TOOL_NAMES};
use super::funnel_repair_tools::{execute_funnel_repair_tool, FUNNEL_REPAIR_TOOL_NAMES};
use super::kb_admin_tools::execute_kb_admin_tool;
use super::mail_tools::{execute_mail_tool, MAIL_TOOL_NAMES};
use super::metadata_registry::METADATA_REGISTRY;
use super::plugin_tools::{execute_plugin_tool, PLUGIN_TOOL_NAMES};
use super::quality_tools::{execute_quality_tool, QUALITY_TOOL_NAMES};
use super::schedule_tools::{execute_schedule_tool, SCHEDULE_TOOL_NAMES};
use super::table_tools::{execute_build_table, execute_table_tool, TABLE_TOOL_NAMES};
use super::types::{ToolCall, ToolDefinition};
use super::workspace_tools::{execute_workspace_tool, WORKSPACE_TOOL_NAMES};
use contracts::domain::a017_llm_agent::aggregate::AgentType;
use once_cell::sync::Lazy;
use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

/// Инструменты «знания о системе»: результат зависит только от
/// (agent_type, name, arguments) — не от чата/состояния. Кэшируем их, чтобы LLM
/// не переоткрывал схему/каталог на каждом ходу диалога (лишние round-trip'ы и токены).
///
/// Полностью статична здесь только форма ответа: `get_entity_schema` несёт ещё
/// профиль данных и список привязанных статей, а они меняются на живом процессе —
/// см. `invalidate_metadata_tool_cache`.
const CACHEABLE_TOOLS: &[&str] = &[
    "get_architecture_overview",
    "get_chart_of_accounts",
    "get_entity_schema",
    "list_entities",
    "get_join_hint",
    "list_data_sources",
];

/// Процесс-кэш результатов инструментов «знания о системе».
static METADATA_TOOL_CACHE: Lazy<Mutex<HashMap<String, String>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

/// Сбросить кэш после того, как изменилось знание, попадающее в его ответы:
/// перечитана база знаний (меняется `docs`) или пересчитан профиль данных
/// (появляется `data_profile`). Без этого схема, взятая в первые секунды после
/// старта, оставалась бы без профиля до конца жизни процесса.
///
/// Чистим целиком: записей десятки, а точечная инвалидация требовала бы знать,
/// какие сущности задеты — знание, которого у вызывающей стороны нет.
pub fn invalidate_metadata_tool_cache() {
    if let Ok(mut cache) = METADATA_TOOL_CACHE.lock() {
        cache.clear();
    }
}

fn tool_result_ok(result: &serde_json::Value) -> bool {
    result
        .get("ok")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or_else(|| result.get("error").is_none())
}

#[cfg(test)]
mod result_status_tests {
    use super::tool_result_ok;
    use serde_json::json;

    #[test]
    fn explicit_ok_false_wins_even_without_error_field() {
        assert!(!tool_result_ok(
            &json!({ "ok": false, "failures": ["render"] })
        ));
        assert!(tool_result_ok(
            &json!({ "ok": true, "error": "diagnostic only" })
        ));
        assert!(!tool_result_ok(&json!({ "error": "failed" })));
        assert!(tool_result_ok(&json!({ "result": 1 })));
    }
}

/// Ключ кэша. Включает agent_type, т.к. часть инструментов отдаёт ошибку доступа
/// для отдельных ролей (напр. list_entities для SystemAdmin).
fn cache_key(agent_type: &AgentType, name: &str, arguments: &str) -> String {
    format!("{}\u{0}{}\u{0}{}", agent_type.as_str(), name, arguments)
}

// ─── Определения инструментов ────────────────────────────────────────────────

/// Общие инструменты для всех агентов (схемы, KB, DataView).
pub(crate) fn shared_tool_definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "get_architecture_overview".into(),
            description: "Получить КАРТУ всей системы за один вызов: список сущностей \
                          (index, table, name, tags) и их связи (related). Используй В ПЕРВУЮ \
                          ОЧЕРЕДЬ, чтобы понять структуру домена, вместо множества list_entities. \
                          Затем углубляйся через get_entity_schema(index)."
                .into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "category": {
                        "type": "string",
                        "description": "Необязательный фильтр по тегу-категории.",
                        "enum": ["wb", "ozon", "ym", "ref", "llm", "promotion", "advertising",
                                 "bi", "dashboard", "projection", "gl", "accounting", "sales", "orders", "1c"]
                    }
                }
            }),
        },
        ToolDefinition {
            name: "get_chart_of_accounts".into(),
            description: "Получить план счетов General Ledger: код, имя, тип счёта \
                          (актив/пассив), нормальное сальдо, иерархию (parent_code), раздел \
                          отчётности и описание. Используй для понимания учётной модели: какие \
                          счета дебетуются/кредитуются, как устроены взаиморасчёты с маркетплейсом \
                          (7609/76YA/76YB), выручка (9001), себестоимость (9002). \
                          Виды оборотов между счетами — в list_gl_turnovers."
                .into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {}
            }),
        },
        ToolDefinition {
            name: "get_entity_schema".into(),
            description: "Получить детальную схему таблицы: поля, SQL-типы, описания, \
                          внешние ключи (FK). Используй ПЕРЕД написанием SQL-запроса. \
                          Примеры entity_index: 'a004' (номенклатура), 'a012' (продажи WB), \
                          'a013' (заказы YM), 'a006' (подключения МП), 'a002' (организации)."
                .into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "entity_index": {
                        "type": "string",
                        "description": "Короткий индекс сущности из list_entities, например 'a012', 'a004', 'a006'. Это НЕ schema_id из list_data_sources (напр. ds03_p904_sales) и не имя таблицы — для запроса данных по безопасной схеме используй query_data_schema."
                    }
                },
                "required": ["entity_index"]
            }),
        },
    ]
}

/// Инструменты бизнес-аналитика (данные маркетплейсов, SQL, BI).
pub(crate) fn analyst_tool_definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "list_entities".into(),
            description: "Получить список таблиц базы данных с кратким описанием. \
                          ВСЕГДА передавай category — не запрашивай все таблицы без фильтра. \
                          Категории: wb=Wildberries (продажи), \
                          ozon=OZON, ym=Яндекс.Маркет, ref=справочники (организации, номенклатура), \
                          llm=чаты/агенты, bi=BI-индикаторы и дашборды, dashboard=то же что bi. \
                          Если уже знаешь entity_index — сразу вызывай get_entity_schema."
                .into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "category": {
                        "type": "string",
                        "description": "Необязательный фильтр по категории данных.",
                        "enum": ["wb", "ozon", "ym", "ref", "llm", "promotion", "bi", "dashboard", "gl", "accounting"]
                    }
                }
            }),
        },
        ToolDefinition {
            name: "get_join_hint".into(),
            description: "Получить подсказку как соединить (JOIN) две таблицы. \
                          Возвращает готовый SQL JOIN и имена FK-колонок."
                .into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "from_entity": {
                        "type": "string",
                        "description": "Индекс таблицы FROM, например 'a012'."
                    },
                    "to_entity": {
                        "type": "string",
                        "description": "Индекс таблицы для JOIN, например 'a006'."
                    }
                },
                "required": ["from_entity", "to_entity"]
            }),
        },
        ToolDefinition {
            name: "create_drilldown_report".into(),
            description: "Создать drilldown-отчёт и сохранить его в системе. \
                          Инструмент записывает сессию в базу и возвращает artifact_id — \
                          пользователь увидит карточку с кнопкой открытия отчёта прямо в чате. \
                          Используй list_data_sources(kind=\"dataview\") чтобы узнать доступные view_id, metric_id и group_by. \
                          Обязательно уточни у пользователя период (date_from, date_to) если он не указан."
                .into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "view_id": { "type": "string", "description": "ID DataView, например 'dv001_revenue'." },
                    "group_by": { "type": "string", "description": "Измерение для детализации." },
                    "metric_id": { "type": "string", "description": "Метрика: 'revenue', 'cost', 'commission', 'expenses', 'profit', 'profit_d'." },
                    "date_from": { "type": "string", "description": "Начало периода, YYYY-MM-DD." },
                    "date_to": { "type": "string", "description": "Конец периода, YYYY-MM-DD." },
                    "description": { "type": "string", "description": "Человекочитаемое название отчёта." },
                    "period2_from": { "type": "string", "description": "Начало периода сравнения (опционально)." },
                    "period2_to": { "type": "string", "description": "Конец периода сравнения (опционально)." },
                    "params": {
                        "type": "object",
                        "description": "Дополнительные параметры DataView, например {\"layer\":\"fact\",\"turnover_code\":\"mp_commission\"} для dv004."
                    },
                    "connection_mp_refs": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "UUID кабинетов МП для фильтрации (опционально, пустой = все)."
                    }
                },
                "required": ["view_id", "group_by", "metric_id", "date_from", "date_to", "description"]
            }),
        },
        ToolDefinition {
            name: "list_gl_turnovers".into(),
            description: "Получить список видов оборотов General Ledger \
                          (turnover_code, name, description, счета Дт/Кт, формулы). \
                          Используй для понимания структуры учёта: какие операции фиксируются \
                          в sys_general_ledger, какой turnover_code использовать в WHERE-условии, \
                          какой счёт дебетуется/кредитуется при продаже/возврате/комиссии."
                .into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "report_group": {
                        "type": "string",
                        "description": "Фильтр по группе отчёта: revenue, returns, commission, \
                                        acquiring, logistics, storage, penalty, advertising, \
                                        cost, quantity, ratio, adjustment, other",
                        "enum": [
                            "revenue", "returns", "payout", "commission", "acquiring",
                            "logistics", "storage", "penalty", "advertising",
                            "cost", "quantity", "ratio", "adjustment", "other"
                        ]
                    }
                }
            }),
        },
    ]
}

// ─── Диспетчер ───────────────────────────────────────────────────────────────

/// Куда уходит вызов.
///
/// Раньше маршрут выбирала лестница из пятнадцати `if`, и **порядок ветвей был
/// носителем смысла**: имя, попавшее в два набора, молча доставалось той ветви,
/// что стоит выше. Теперь наборы перечислены таблицей, а тест требует, чтобы они
/// не пересекались — коллизия падает на прогоне тестов, а не расходится в рантайме.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Route {
    SkillRuntime,
    Kb,
    KbAdmin,
    KnowledgeInventory,
    LlmQuality,
    AgentTask,
    Workspace,
    Data,
    BuildChart,
    BuildTable,
    Chart,
    Table,
    Ticket,
    Mail,
    Schedule,
    Quality,
    FunnelRepair,
    Plugin,
    Admin,
    Metadata,
}

/// Инструменты рантайма навыков: ресурсы и mjs-задачи активного навыка.
const SKILL_RUNTIME_TOOL_NAMES: &[&str] = &[
    "list_skill_resources",
    "read_skill_resource",
    "run_skill_task",
];

/// Инструменты курирования базы знаний (правки статей через a031_kb_edit).
const KB_ADMIN_TOOL_NAMES: &[&str] = &[
    "list_kb_documents",
    "get_kb_document",
    "create_kb_edit",
    "update_kb_edit_articles",
    "list_open_kb_edits",
    "write_kb_document",
];

/// Инструменты, реализованные прямо в этом файле (ветвь `Route::Metadata`).
///
/// Список ведётся руками и обязан совпадать с плечами `match` в
/// `execute_metadata_tool`. Забытое имя ловится тестом
/// `every_declared_tool_has_a_route`: объявленный, но не маршрутизированный
/// инструмент отдаёт модели «Unknown tool» — ровно так пропал `get_project_metrics`.
const METADATA_TOOL_NAMES: &[&str] = &[
    "get_architecture_overview",
    "get_chart_of_accounts",
    "list_entities",
    "list_skills",
    "use_skill",
    "get_entity_schema",
    "get_join_hint",
    "create_drilldown_report",
    "list_gl_turnovers",
];

/// Единственный источник маршрутизации — и для `route_of`, и для тестов.
/// Набор, не попавший сюда, не будет ни вызван, ни проверен.
const ROUTING_TABLE: &[(Route, &[&str])] = &[
    (Route::SkillRuntime, SKILL_RUNTIME_TOOL_NAMES),
    (Route::Kb, super::kb_tools::KB_TOOL_NAMES),
    (Route::KbAdmin, KB_ADMIN_TOOL_NAMES),
    (Route::KnowledgeInventory, &["knowledge_inventory"]),
    (
        Route::LlmQuality,
        super::llm_quality_tools::LLM_QUALITY_TOOL_NAMES,
    ),
    (
        Route::AgentTask,
        super::agent_task_tools::AGENT_TASK_TOOL_NAMES,
    ),
    (Route::Workspace, WORKSPACE_TOOL_NAMES),
    (Route::Data, DATA_TOOL_NAMES),
    (Route::BuildChart, &["build_chart"]),
    (Route::BuildTable, &["build_table"]),
    (Route::Chart, CHART_TOOL_NAMES),
    (Route::Table, TABLE_TOOL_NAMES),
    (Route::Ticket, super::ticket_tools::TICKET_TOOL_NAMES),
    (Route::Mail, MAIL_TOOL_NAMES),
    (Route::Schedule, SCHEDULE_TOOL_NAMES),
    (Route::Quality, QUALITY_TOOL_NAMES),
    (Route::FunnelRepair, FUNNEL_REPAIR_TOOL_NAMES),
    (Route::Plugin, PLUGIN_TOOL_NAMES),
    (Route::Admin, ADMIN_TOOL_NAMES),
    (Route::Metadata, METADATA_TOOL_NAMES),
];

fn route_of(name: &str) -> Option<Route> {
    ROUTING_TABLE
        .iter()
        .find(|(_, names)| names.contains(&name))
        .map(|(route, _)| *route)
}

/// Оформить результат инструмента: конверт `_tool`/`_ok` и сериализация.
///
/// Раньше эти двенадцать строк были скопированы в каждой из пятнадцати ветвей.
/// Ветвь, забывшая копию, отдавала результат **успешным по умолчанию**:
/// `tool_result_ok` возвращает `true`, когда нет ни `ok`, ни `error`.
fn finish(name: &str, result: serde_json::Value) -> String {
    let is_ok = tool_result_ok(&result);
    let mut result = result;
    if let serde_json::Value::Object(ref mut map) = result {
        map.insert(
            "_tool".to_string(),
            serde_json::Value::String(name.to_string()),
        );
        map.insert("_ok".to_string(), serde_json::Value::Bool(is_ok));
    }
    serde_json::to_string_pretty(&result)
        .unwrap_or_else(|e| format!("{{\"error\": \"Serialization error: {}\"}}", e))
}

/// Отказ до исполнения: текст уходит модели вместо результата инструмента.
fn refuse(name: &str, message: String) -> String {
    finish(name, serde_json::json!({ "error": message }))
}

/// Контекст оболочки чата — всё, что инструмент может узнать о вызывающем.
///
/// Раньше это были тринадцать позиционных параметров под
/// `#[allow(clippy::too_many_arguments)]`. Собранные в тип, они делают «тонкую
/// оболочку» видимой: вот ровно то, чем чат отличается от Этапа как вызывающего
/// (чат, агент, собеседник и матрица прав — против экземпляра, Этапа и режима).
pub struct ToolContext<'a> {
    pub chat_id: &'a str,
    pub agent_id: &'a str,
    pub agent_type: &'a AgentType,
    /// Разрешённый набор: core ∪ инструменты активных навыков.
    pub active_tools: &'a HashSet<String>,
    pub active_skill_ids: &'a HashSet<String>,
    pub skill_snapshot: &'a super::skills::SkillRegistrySnapshot,
    pub skill_access: &'a HashMap<String, super::skill_policy::SkillAccessLevel>,
    pub artifact_publish_allowed: bool,
    pub skill_script_execute_allowed: bool,
    pub skill_script_develop_allowed: bool,
    pub data_repair_execute_allowed: bool,
    /// Пользователь-собеседник; `None` у фоновых сценариев, тогда инструменты,
    /// действующие от лица человека (тикеты), отказывают.
    pub caller: Option<&'a super::types::ToolCaller>,
    /// Номер хода диалога. Входит в ключ идемпотентности эффектов — см.
    /// `chat_effects::ChatEffect::new`.
    pub turn: u32,
}

impl ToolContext<'_> {
    /// Оболочка над реестром Действий для этого хода.
    ///
    /// Провенанс собирается здесь, а не в самом Действии: кто зовёт — знает
    /// оболочка, что делать — знает Действие. Фоновый сценарий без собеседника
    /// получает `Manual`, а не отказ: эффект всё равно обязан попасть в журнал.
    pub fn effect(&self) -> super::chat_effects::ChatEffect {
        let actor = match self.caller {
            Some(caller) => contracts::processes::ActionActor::User {
                user_id: caller.user_id.clone(),
                chat_ref: (!self.chat_id.trim().is_empty()).then(|| self.chat_id.to_string()),
                agent_ref: (!self.agent_id.trim().is_empty()).then(|| self.agent_id.to_string()),
                parent_task_ref: None,
                depth: 0,
            },
            None => contracts::processes::ActionActor::Manual,
        };
        super::chat_effects::ChatEffect::new(actor, self.chat_id, self.turn)
    }
}

/// Выполнить tool call и вернуть результат в виде JSON-строки.
///
/// Вызывается в цикле `send_message`, когда LLM возвращает `tool_calls`.
pub async fn execute_tool_call(call: &ToolCall, cx: &ToolContext<'_>) -> String {
    let name = call.name.as_str();

    // ── Гарды: можно ли звать вообще ────────────────────────────────────────
    //
    // Вторая линия после `tool_guards::before_tool`: исполнитель достижим и из
    // фоновых сценариев, где цикла чата (а значит и гардов) нет.

    // Авторизация: исполняем только инструменты активного набора.
    if !cx.active_tools.contains(name) {
        return refuse(
            name,
            format!(
                "Инструмент '{name}' не активен в текущем наборе. Вызови list_skills() и \
                 use_skill(\"<id>\"), чтобы активировать нужный навык."
            ),
        );
    }
    if super::skill_policy::is_artifact_mutation(name) && !cx.artifact_publish_allowed {
        return refuse(
            name,
            "Специализация агента не имеет права artifact_publish.".to_string(),
        );
    }
    if super::skill_policy::is_data_repair_mutation(name) && !cx.data_repair_execute_allowed {
        return refuse(
            name,
            "Специализация агента не имеет права data_repair_execute.".to_string(),
        );
    }

    // Кэш идемпотентных «системных» инструментов — обслуживаем повтор без вычисления.
    let cacheable = CACHEABLE_TOOLS.contains(&name);
    if cacheable {
        let key = cache_key(cx.agent_type, name, &call.arguments);
        if let Ok(cache) = METADATA_TOOL_CACHE.lock() {
            if let Some(hit) = cache.get(&key) {
                return hit.clone();
            }
        }
    }

    // ── Маршрутизация ───────────────────────────────────────────────────────
    let Some(route) = route_of(name) else {
        return refuse(
            name,
            format!(
                "Unknown tool: '{name}'. Инструмент объявлен, но не маршрутизирован — \
                 это дефект сборки, а не отсутствие права."
            ),
        );
    };

    let result = match route {
        Route::SkillRuntime => match name {
            "list_skill_resources" => super::skill_runtime::list_resources(
                &call.arguments,
                cx.active_skill_ids,
                cx.skill_snapshot,
            ),
            "read_skill_resource" => super::skill_runtime::read_resource(
                &call.arguments,
                cx.active_skill_ids,
                cx.skill_snapshot,
            ),
            _ => {
                super::skill_runtime::run_task(
                    &call.arguments,
                    cx.active_skill_ids,
                    cx.skill_snapshot,
                    cx.skill_script_execute_allowed,
                    cx.skill_script_develop_allowed,
                )
                .await
            }
        },
        Route::Kb => {
            super::kb_tools::execute_kb_tool(name, &call.arguments, cx.chat_id, cx.agent_id).await
        }
        Route::KbAdmin => execute_kb_admin_tool(name, &call.arguments, cx.agent_id).await,
        Route::KnowledgeInventory => {
            let args = serde_json::from_str::<serde_json::Value>(&call.arguments)
                .unwrap_or_else(|_| serde_json::json!({}));
            crate::knowledge::llm_view::execute(crate::shared::data::db::get_connection(), &args)
                .await
        }
        Route::LlmQuality => {
            super::llm_quality_tools::execute_llm_quality_tool(name, &call.arguments).await
        }
        // Гарды делегирования (глубина цепочки, самоделегирование, потолки очереди)
        // считаются внутри по chat_id и agent_type, а не по аргументам модели.
        Route::AgentTask => {
            super::agent_task_tools::execute_agent_task_tool(
                name,
                &call.arguments,
                cx.chat_id,
                cx.agent_type,
                &cx.effect(),
            )
            .await
        }
        Route::Workspace => {
            // Шаблон анкеты приносит активный навык: у финансиста и маркетолога
            // значимые параметры задачи разные.
            let intake_template =
                super::skills::intake_template_in(cx.skill_snapshot, cx.active_skill_ids);
            execute_workspace_tool(
                name,
                &call.arguments,
                cx.chat_id,
                intake_template.as_deref(),
            )
            .await
        }
        Route::Data => {
            execute_data_tool(
                name,
                &call.arguments,
                cx.agent_type,
                cx.chat_id,
                cx.agent_id,
            )
            .await
        }
        // build_chart / build_table — высокоуровневые сборщики: выполняют SQL и
        // сохраняют плагин, в отличие от заготовок-инструментов рядом.
        Route::BuildChart => {
            super::chart_tools::execute_build_chart(&call.arguments, cx.chat_id, cx.agent_id).await
        }
        Route::BuildTable => execute_build_table(&call.arguments, cx.chat_id, cx.agent_id).await,
        Route::Chart => execute_chart_tool(name, &call.arguments),
        Route::Table => execute_table_tool(name, &call.arguments),
        // Тикеты действуют от лица собеседника, поэтому требуют `caller`;
        // в фоновых сценариях его нет.
        Route::Ticket => match cx.caller {
            Some(caller) => {
                super::ticket_tools::execute_ticket_tool(name, &call.arguments, cx.chat_id, caller)
                    .await
            }
            None => serde_json::json!({
                "error": "Инструменты тикетов доступны только в диалоге с пользователем: \
                          в текущей сессии автор обращения неизвестен.",
            }),
        },
        Route::Mail => execute_mail_tool(name, &call.arguments).await,
        Route::Schedule => execute_schedule_tool(name, &call.arguments).await,
        Route::Quality => execute_quality_tool(name, &call.arguments, &cx.effect()).await,
        Route::FunnelRepair => {
            execute_funnel_repair_tool(name, &call.arguments, cx.chat_id, cx.agent_id, cx.caller)
                .await
        }
        Route::Plugin => execute_plugin_tool(name, &call.arguments, cx.chat_id, cx.agent_id).await,
        Route::Admin => execute_admin_tool(name, &call.arguments).await,
        Route::Metadata => execute_metadata_tool(call, cx).await,
    };

    let output = finish(name, result);

    if cacheable {
        if let Ok(mut cache) = METADATA_TOOL_CACHE.lock() {
            cache.insert(
                cache_key(cx.agent_type, name, &call.arguments),
                output.clone(),
            );
        }
    }

    output
}

/// Инструменты «знания о системе», реализованные прямо здесь.
///
/// Плечи обязаны совпадать с `METADATA_TOOL_NAMES`; последнее плечо недостижимо,
/// пока эти два списка сходятся.
async fn execute_metadata_tool(call: &ToolCall, cx: &ToolContext<'_>) -> serde_json::Value {
    match call.name.as_str() {
        "get_architecture_overview" => {
            let category = parse_string_arg(&call.arguments, "category");
            METADATA_REGISTRY.architecture_overview(category.as_deref())
        }

        "get_chart_of_accounts" => {
            let accounts = crate::shared::analytics::account_registry::ACCOUNT_REGISTRY;
            serde_json::json!({
                "accounts": accounts,
                "count": accounts.len(),
                "hint": "План счетов GL. parent_code задаёт иерархию (группа → субсчёт). \
                         Проводки хранятся в sys_general_ledger (поля debit_account/credit_account). \
                         Какие обороты задействуют счета — см. list_gl_turnovers."
            })
        }

        "list_entities" => {
            let category = parse_string_arg(&call.arguments, "category");
            METADATA_REGISTRY.list_entities(category.as_deref())
        }

        "list_skills" => super::skills::list_skills_result_from(cx.skill_snapshot, cx.skill_access),

        "use_skill" => super::skills::use_skill_result_from(
            &call.arguments,
            cx.skill_snapshot,
            cx.skill_access,
        ),

        "get_entity_schema" => {
            let index = parse_string_arg(&call.arguments, "entity_index").unwrap_or_default();
            tracing::info!("[get_entity_schema] called with entity_index='{}'", index);
            // Метаданные описывают агрегат логически; сверяем с физической таблицей и
            // отдаём json_extract-выражения для полей, которых нет отдельными колонками.
            let result =
                crate::shared::data_access::physical_schema::annotate_with_physical_schema(
                    METADATA_REGISTRY.get_entity_schema(&index),
                )
                .await;
            let fields_count = result
                .get("fields")
                .and_then(|f| f.as_array())
                .map(|a| a.len())
                .unwrap_or(0);
            tracing::info!(
                "[get_entity_schema] entity='{}' fields_count={} has_error={}",
                index,
                fields_count,
                result.get("error").is_some()
            );
            result
        }

        "get_join_hint" => {
            let from = parse_string_arg(&call.arguments, "from_entity").unwrap_or_default();
            let to = parse_string_arg(&call.arguments, "to_entity").unwrap_or_default();
            METADATA_REGISTRY.get_join_hint(&from, &to)
        }

        "create_drilldown_report" => {
            create_drilldown_report_tool(&call.arguments, cx.chat_id, cx.agent_id).await
        }

        "list_gl_turnovers" => {
            let args =
                serde_json::from_str::<serde_json::Value>(&call.arguments).unwrap_or_default();
            let report_group = args.get("report_group").and_then(|v| v.as_str());
            let items: Vec<_> = crate::general_ledger::turnover_registry::TURNOVER_CLASSES
                .iter()
                .filter(|t| report_group.map_or(true, |g| t.report_group.as_str() == g))
                .map(|t| {
                    serde_json::json!({
                        "code": t.code,
                        "name": t.name,
                        "description": t.description,
                        "llm_description": t.llm_description,
                        "debit_account": t.debit_account,
                        "credit_account": t.credit_account,
                        "report_group": t.report_group.as_str(),
                        "generates_journal_entry": t.generates_journal_entry,
                        "formula_hint": t.formula_hint,
                    })
                })
                .collect();
            let count = items.len();
            serde_json::json!({
                "turnovers": items,
                "count": count,
                "hint": "Используй turnover_code в WHERE sys_general_ledger.turnover_code = '...' \
                         для фильтрации проводок нужного типа."
            })
        }

        orphan => serde_json::json!({
            "error": format!("'{orphan}' есть в METADATA_TOOL_NAMES, но без реализации"),
        }),
    }
}

#[cfg(test)]
mod routing_tests {
    use super::*;

    /// Каждый объявленный инструмент обязан иметь маршрут.
    ///
    /// Ради этого теста и заведена таблица: `get_project_metrics` был объявлен,
    /// реализован и стоял первым в навыке `app-health-review`, но его имя выпало
    /// из захардкоженной ветви диспетчера — и модель получала «Unknown tool» на
    /// инструмент, который система ей же и предложила.
    #[test]
    fn every_declared_tool_has_a_route() {
        let orphans: Vec<String> = super::super::skills::tool_universe()
            .into_iter()
            .filter(|tool| route_of(&tool.name).is_none())
            .map(|tool| tool.name)
            .collect();
        assert!(
            orphans.is_empty(),
            "объявлены, но не маршрутизированы: {}",
            orphans.join(", ")
        );
    }

    /// И наоборот: маршрут в никуда — мёртвая ветвь диспетчера.
    #[test]
    fn every_route_points_at_a_declared_tool() {
        let declared: HashSet<String> = super::super::skills::tool_universe()
            .into_iter()
            .map(|tool| tool.name)
            .collect();
        let dangling: Vec<&str> = ROUTING_TABLE
            .iter()
            .flat_map(|(_, names)| names.iter().copied())
            .filter(|name| !declared.contains(*name))
            .collect();
        assert!(
            dangling.is_empty(),
            "маршрутизированы, но не объявлены: {}",
            dangling.join(", ")
        );
    }

    /// Наборы не пересекаются. Пока маршрут выбирала лестница `if`, пересечение
    /// разрешалось порядком ветвей молча — и порядок приходилось охранять
    /// комментарием вместо теста.
    #[test]
    fn routing_sets_are_disjoint() {
        let mut seen: HashMap<&str, Route> = HashMap::new();
        for (route, names) in ROUTING_TABLE {
            for name in names.iter() {
                if let Some(first) = seen.insert(name, *route) {
                    panic!("имя '{name}' есть и в {first:?}, и в {route:?}");
                }
            }
        }
    }
}

// ─── create_drilldown_report implementation ───────────────────────────────────

async fn create_drilldown_report_tool(
    arguments_json: &str,
    chat_id: &str,
    agent_id: &str,
) -> serde_json::Value {
    use sea_orm::{ConnectionTrait, DatabaseBackend, Statement};
    use uuid::Uuid;

    // Parse arguments
    let args: serde_json::Value = match serde_json::from_str(arguments_json) {
        Ok(v) => v,
        Err(e) => {
            return serde_json::json!({
                "error": format!("Failed to parse tool arguments: {}", e)
            });
        }
    };

    let view_id = match args.get("view_id").and_then(|v| v.as_str()) {
        Some(v) => v.to_string(),
        None => return serde_json::json!({ "error": "Missing required parameter: view_id" }),
    };
    let group_by = match args.get("group_by").and_then(|v| v.as_str()) {
        Some(v) => v.to_string(),
        None => return serde_json::json!({ "error": "Missing required parameter: group_by" }),
    };
    let metric_id = match args.get("metric_id").and_then(|v| v.as_str()) {
        Some(v) => v.to_string(),
        None => return serde_json::json!({ "error": "Missing required parameter: metric_id" }),
    };
    let date_from = match args.get("date_from").and_then(|v| v.as_str()) {
        Some(v) => v.to_string(),
        None => return serde_json::json!({ "error": "Missing required parameter: date_from" }),
    };
    let date_to = match args.get("date_to").and_then(|v| v.as_str()) {
        Some(v) => v.to_string(),
        None => return serde_json::json!({ "error": "Missing required parameter: date_to" }),
    };
    let description = args
        .get("description")
        .and_then(|v| v.as_str())
        .unwrap_or("Drilldown отчёт")
        .to_string();

    let period2_from = args
        .get("period2_from")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());
    let period2_to = args
        .get("period2_to")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());

    let connection_mp_refs: Vec<String> = args
        .get("connection_mp_refs")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();
    let extra_params = args
        .get("params")
        .filter(|v| v.is_object())
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));

    // Validate DataView exists
    let registry = crate::data_view::DataViewRegistry::new();
    if !registry.has_view(&view_id) {
        return serde_json::json!({
            "error": format!("DataView '{}' not found. Use list_data_sources(kind=\"dataview\") to see available views.", view_id)
        });
    }

    let db = crate::shared::data::db::get_connection();

    // 1. Create sys_drilldown session
    let session_id = Uuid::new_v4().to_string();
    let params_json = serde_json::json!({
        "view_id": view_id,
        "metric_id": null,
        "metric_ids": [metric_id.clone()],
        "group_by": group_by,
        "group_by_label": "",
        "date_from": date_from,
        "date_to": date_to,
        "period2_from": period2_from,
        "period2_to": period2_to,
        "connection_mp_refs": connection_mp_refs,
        "params": extra_params
    });

    if let Err(e) = db
        .execute(Statement::from_sql_and_values(
            DatabaseBackend::Sqlite,
            "INSERT INTO sys_drilldown (id, view_id, indicator_id, indicator_name, params_json) \
             VALUES (?, ?, '', ?, ?)",
            [
                session_id.clone().into(),
                view_id.clone().into(),
                description.clone().into(),
                params_json.to_string().into(),
            ],
        ))
        .await
    {
        tracing::error!("Failed to create sys_drilldown session: {}", e);
        return serde_json::json!({
            "error": format!("Failed to create drilldown session: {}", e)
        });
    }

    // 2. Create a019_llm_artifact
    let artifact_query_params = serde_json::json!({
        "session_id": session_id,
        "view_id": view_id,
        "group_by": group_by,
        "metric_id": metric_id,
        "date_from": date_from,
        "date_to": date_to,
        "params": extra_params,
    });

    let artifact_dto = crate::domain::a019_llm_artifact::service::LlmArtifactDto {
        id: None,
        code: Some(format!("DRILLDOWN-{}", &session_id[..8].to_uppercase())),
        description: description.clone(),
        comment: Some(format!(
            "Отчёт: {} по {}, период {} — {}",
            metric_id, group_by, date_from, date_to
        )),
        chat_id: chat_id.to_string(),
        agent_id: agent_id.to_string(),
        artifact_type: Some("drilldown_report".to_string()),
        sql_query: String::new(),
        query_params: Some(artifact_query_params.to_string()),
        visualization_config: None,
    };

    match crate::domain::a019_llm_artifact::service::create(artifact_dto).await {
        Ok(artifact_uuid) => {
            tracing::info!(
                "Created drilldown artifact {} for session {}",
                artifact_uuid,
                session_id
            );
            serde_json::json!({
                "success": true,
                "artifact_id": artifact_uuid.to_string(),
                "session_id": session_id,
                "description": description,
                "hint": "Артефакт создан. Пользователь увидит карточку с кнопкой открытия отчёта в чате."
            })
        }
        Err(e) => {
            tracing::error!("Failed to create drilldown artifact: {}", e);
            // Session was created, return partial success so user can still navigate
            serde_json::json!({
                "success": false,
                "session_id": session_id,
                "error": format!("Session created but artifact save failed: {}", e)
            })
        }
    }
}

// ─── Вспомогательные ─────────────────────────────────────────────────────────

/// Извлечь строковый аргумент из JSON-строки аргументов tool call.
fn parse_string_arg(arguments_json: &str, key: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(arguments_json)
        .ok()
        .and_then(|v| v.get(key).and_then(|v| v.as_str()).map(|s| s.to_string()))
}
