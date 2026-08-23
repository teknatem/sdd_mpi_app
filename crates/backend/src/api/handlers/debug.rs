//! Debug endpoints — только для разработки.
//!
//! GET  /api/debug/tool-test?tool=get_entity_schema&entity_index=a006
//! GET  /api/debug/tool-test?tool=list_entities&category=ref
//! GET  /api/debug/tool-test?tool=execute_query&sql=SELECT+1&description=test

use crate::shared::llm::types::ToolCall;
use axum::{extract::Query, response::IntoResponse, Json};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct ToolTestParams {
    /// Имя инструмента: list_entities / get_entity_schema / execute_query / ...
    pub tool: String,
    // Параметры конкретных инструментов
    pub entity_index: Option<String>,
    pub category: Option<String>,
    pub sql: Option<String>,
    pub description: Option<String>,
    pub from_entity: Option<String>,
    pub to_entity: Option<String>,
    pub tags: Option<String>,
    pub id: Option<String>,
    pub check_id: Option<String>,
}

/// GET /api/debug/tool-test
///
/// Напрямую вызывает execute_tool_call без участия LLM.
/// Позволяет убедиться что инструмент работает и что именно он возвращает.
pub async fn tool_test(Query(params): Query<ToolTestParams>) -> impl IntoResponse {
    // Собираем аргументы в JSON в зависимости от инструмента
    let args = build_args(&params);

    let call = ToolCall {
        id: "debug-test".to_string(),
        name: params.tool.clone(),
        arguments: args.to_string(),
    };

    tracing::info!(
        "[debug/tool-test] tool='{}' args={}",
        params.tool,
        call.arguments
    );

    // Для debug-эндпоинта активируем все навыки General, чтобы любой инструмент проходил guard.
    let specialization = contracts::domain::a017_llm_agent::aggregate::AgentType::CoordinatorAdmin;
    let skill_snapshot = crate::shared::llm::skills::snapshot();
    let skill_access = match crate::shared::llm::skill_policy::accesses_for_snapshot(
        &specialization,
        &skill_snapshot,
    )
    .await
    {
        Ok(access) => access,
        Err(error) => {
            tracing::error!("[debug/tool-test] failed to load skill policy: {error:#}");
            return Json(serde_json::json!({
                "tool": params.tool,
                "error": "failed to load effective skill policy",
                "ok": false,
            }));
        }
    };
    let active_skills = skill_access
        .iter()
        .into_iter()
        .filter_map(|(skill_id, level)| {
            (*level != crate::shared::llm::skill_policy::SkillAccessLevel::Denied)
                .then_some(skill_id.clone())
        })
        .collect::<Vec<_>>();
    let active_tools =
        crate::shared::llm::skills::active_tool_names_in(&skill_snapshot, &active_skills);
    let active_skill_ids = active_skills
        .iter()
        .cloned()
        .collect::<std::collections::HashSet<_>>();
    let artifact_publish_allowed = match crate::shared::llm::skill_policy::capability_allowed(
        &specialization,
        crate::shared::llm::skill_policy::ARTIFACT_PUBLISH,
    )
    .await
    {
        Ok(allowed) => allowed,
        Err(error) => {
            tracing::error!("[debug/tool-test] failed to load capability policy: {error:#}");
            return Json(serde_json::json!({
                "tool": params.tool,
                "error": "failed to load effective capability policy",
                "ok": false,
            }));
        }
    };
    let skill_script_execute_allowed = crate::shared::llm::skill_policy::capability_allowed(
        &specialization,
        crate::shared::llm::skill_policy::SKILL_SCRIPT_EXECUTE,
    )
    .await
    .unwrap_or(false);
    let skill_script_develop_allowed = crate::shared::llm::skill_policy::capability_allowed(
        &specialization,
        crate::shared::llm::skill_policy::SKILL_SCRIPT_DEVELOP,
    )
    .await
    .unwrap_or(false);
    let data_repair_execute_allowed = crate::shared::llm::skill_policy::capability_allowed(
        &specialization,
        crate::shared::llm::skill_policy::DATA_REPAIR_EXECUTE,
    )
    .await
    .unwrap_or(false);

    let raw_result = crate::shared::llm::execute_tool_call(
        &call,
        &crate::shared::llm::ToolContext {
            chat_id: "debug-chat-id",
            agent_id: "debug-agent-id",
            agent_type: &specialization,
            active_tools: &active_tools,
            active_skill_ids: &active_skill_ids,
            skill_snapshot: &skill_snapshot,
            skill_access: &skill_access,
            artifact_publish_allowed,
            skill_script_execute_allowed,
            skill_script_develop_allowed,
            data_repair_execute_allowed,
            // Собеседника у debug-вызова нет: инструменты «от лица пользователя» откажут.
            caller: None,
            turn: 1,
        },
    )
    .await;

    // Резать по символам, а не по байтам: результаты почти всегда кириллические, и срез
    // по байту 500 попадал внутрь символа → паника прямо в обработчике.
    tracing::info!(
        "[debug/tool-test] result preview: {}",
        raw_result.chars().take(500).collect::<String>()
    );

    let parsed: serde_json::Value = serde_json::from_str(&raw_result)
        .unwrap_or_else(|e| serde_json::json!({ "raw": raw_result, "parse_error": e.to_string() }));

    Json(serde_json::json!({
        "tool":   params.tool,
        "args":   serde_json::from_str::<serde_json::Value>(&call.arguments).ok(),
        "result": parsed,
        "ok":     parsed.get("error").is_none(),
    }))
}

fn build_args(p: &ToolTestParams) -> serde_json::Value {
    match p.tool.as_str() {
        "get_entity_schema" => serde_json::json!({
            "entity_index": p.entity_index.as_deref().unwrap_or("a006")
        }),
        "list_entities" => serde_json::json!({
            "category": p.category
        }),
        "execute_query" => serde_json::json!({
            "sql": p.sql.as_deref().unwrap_or("SELECT 1"),
            "description": p.description.as_deref().unwrap_or("debug test")
        }),
        "get_join_hint" => serde_json::json!({
            "from_entity": p.from_entity.as_deref().unwrap_or("a012"),
            "to_entity":   p.to_entity.as_deref().unwrap_or("a006")
        }),
        "search_knowledge" => serde_json::json!({
            "tags": p.tags.as_ref().map(|t| t.split(',').collect::<Vec<_>>()).unwrap_or_default()
        }),
        "get_knowledge" => serde_json::json!({
            "id": p.id.as_deref().unwrap_or("")
        }),
        "run_quality_check" => serde_json::json!({
            "check_id": p.check_id.as_deref().unwrap_or("")
        }),
        _ => serde_json::json!({}),
    }
}
