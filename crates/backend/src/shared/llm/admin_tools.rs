//! Инструменты для агента-администратора системы.
//!
//! Мониторинг производительности, безопасность, состояние фоновых задач.
//! Работают с системными таблицами (sys_task_runs, a018_llm_chat_message)
//! без изменений бизнес-данных.

use super::types::ToolDefinition;
use sea_orm::{DatabaseBackend, FromQueryResult, Statement};

// ─── Определения инструментов ────────────────────────────────────────────────

/// Набор инструментов для SystemAdmin агента.
pub fn admin_tool_definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "check_system_health".into(),
            description: "Проверить состояние системы: последние запуски задач, ошибки, \
                          общая статистика за указанный период. \
                          Показывает статусы задач (Completed/Failed/Running), \
                          суммарное количество обработанных записей и ошибок. \
                          Используй для быстрого диагностики системы."
                .into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "hours": {
                        "type": "integer",
                        "description": "Период в часах для анализа (по умолчанию 24). Максимум 168 (7 дней).",
                        "default": 24
                    }
                }
            }),
        },
        ToolDefinition {
            name: "get_performance_stats".into(),
            description: "Статистика производительности LLM-агентов: медленные ответы, \
                          высокое потребление токенов, распределение по моделям. \
                          Помогает выявить проблемы с производительностью агентов и чатов. \
                          Возвращает топ медленных запросов и сводку по моделям."
                .into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "hours": {
                        "type": "integer",
                        "description": "Период анализа в часах (по умолчанию 24).",
                        "default": 24
                    },
                    "slow_threshold_ms": {
                        "type": "integer",
                        "description": "Порог медленного ответа в миллисекундах (по умолчанию 10000).",
                        "default": 10000
                    }
                }
            }),
        },
        ToolDefinition {
            name: "list_background_jobs".into(),
            description: "Список фоновых задач и история их запусков. \
                          Показывает задачи из планировщика (sys_tasks), \
                          их последние запуски (статус, длительность, ошибки). \
                          Используй для мониторинга регламентных заданий: \
                          импорт данных WB, синхронизация, обновление индикаторов."
                .into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "status_filter": {
                        "type": "string",
                        "description": "Фильтр по статусу: 'Failed', 'Running', 'Completed' или пусто для всех.",
                        "enum": ["Failed", "Running", "Completed"]
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Максимальное количество записей (по умолчанию 20, максимум 100).",
                        "default": 20
                    }
                }
            }),
        },
        ToolDefinition {
            name: "get_project_metrics".into(),
            description: "Метрики проекта: последний снимок состояния кодовой базы и экземпляра \
                          с порогами, дельтами к предыдущему запуску и направлением \
                          («больше — лучше» или наоборот). Покрывает размер кода, стоимость \
                          сборки, домен, границы и контракты, тесты, UI-стандарт, базу данных, \
                          качество данных, целостность доступа и активность. \
                          Используй для разбора «на что обратить внимание» и «что ухудшилось». \
                          Метрики кода вшиты в сборку pre-commit хуком и обновляются только с \
                          новым коммитом — их неизменность не означает застой работы."
                .into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "keys": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Ключи метрик (например 'code.lines.total'). Пусто — все."
                    },
                    "history": {
                        "type": "integer",
                        "description": "Сколько последних снимков вернуть рядом со значением. \
                                        Требует непустой keys. 0 или пусто — только текущий срез. \
                                        Максимум 200."
                    }
                }
            }),
        },
        ToolDefinition {
            name: "get_data_integrity_report".into(),
            description: "Отчёт о целостности данных: количество записей в основных таблицах, \
                          наличие \"осиротевших\" связей, дубликаты ключей. \
                          Помогает выявить аномалии после импорта или миграций. \
                          Проверяет таблицы: продажи WB, заказы YM, номенклатуру, кабинеты."
                .into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {}
            }),
        },
    ]
}

// ─── Обработчики инструментов ────────────────────────────────────────────────

pub async fn execute_admin_tool(name: &str, arguments: &str) -> serde_json::Value {
    let args: serde_json::Value =
        serde_json::from_str(arguments).unwrap_or(serde_json::Value::Object(Default::default()));

    match name {
        "check_system_health" => {
            let hours = args
                .get("hours")
                .and_then(|v| v.as_i64())
                .unwrap_or(24)
                .min(168) as u64;
            check_system_health(hours).await
        }
        "get_performance_stats" => {
            let hours = args
                .get("hours")
                .and_then(|v| v.as_i64())
                .unwrap_or(24)
                .min(168) as u64;
            let threshold = args
                .get("slow_threshold_ms")
                .and_then(|v| v.as_i64())
                .unwrap_or(10000) as u64;
            get_performance_stats(hours, threshold).await
        }
        "list_background_jobs" => {
            let status_filter = args
                .get("status_filter")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let limit = args
                .get("limit")
                .and_then(|v| v.as_i64())
                .unwrap_or(20)
                .min(100) as u64;
            list_background_jobs(status_filter, limit).await
        }
        "get_data_integrity_report" => get_data_integrity_report().await,
        "get_project_metrics" => {
            let keys: Vec<String> = args
                .get("keys")
                .and_then(|v| v.as_array())
                .map(|items| {
                    items
                        .iter()
                        .filter_map(|v| v.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default();
            let history = args
                .get("history")
                .and_then(|v| v.as_i64())
                .unwrap_or(0)
                .clamp(0, 200) as u64;
            get_project_metrics(keys, history).await
        }
        _ => serde_json::json!({ "error": format!("Unknown admin tool: '{}'", name) }),
    }
}

/// Снимок метрик проекта, при необходимости — с историей по выбранным ключам.
///
/// Срез собирается тем же кодом, что и контекст страницы `sys_metrics`
/// (`system::metrics::llm_view`): иначе модель получала бы разные числа в
/// зависимости от того, пришли метрики контекстом или запросом инструмента.
async fn get_project_metrics(keys: Vec<String>, history: u64) -> serde_json::Value {
    let data = match crate::system::metrics::latest().await {
        Ok(data) => data,
        Err(e) => return serde_json::json!({ "error": format!("Не удалось прочитать метрики: {e}") }),
    };

    let mut result = crate::system::metrics::llm_view::summary(&data);

    // Фильтр по ключам применяем к срезу, а не к запросу: пороги и паспорт
    // нужны в любом случае, иначе отобранные значения не с чем сопоставить.
    if !keys.is_empty() {
        if let Some(values) = result.get_mut("values").and_then(|v| v.as_array_mut()) {
            values.retain(|item| {
                item.get("key")
                    .and_then(|k| k.as_str())
                    .map(|k| keys.iter().any(|want| want == k))
                    .unwrap_or(false)
            });
        }
    }

    if history > 0 && !keys.is_empty() {
        match crate::system::metrics::series(&keys, history).await {
            Ok(rows) => result["history"] = serde_json::json!(rows),
            Err(e) => result["history_error"] = serde_json::json!(e.to_string()),
        }
    } else if history > 0 {
        result["history_error"] =
            serde_json::json!("История требует непустой список keys: ряды по всем метрикам сразу не отдаются.");
    }

    result
}

async fn check_system_health(hours: u64) -> serde_json::Value {
    let db = crate::shared::data::db::get_connection();

    let sql = format!(
        "SELECT \
           COUNT(*) as total_runs, \
           SUM(CASE WHEN status = 'Completed' THEN 1 ELSE 0 END) as completed, \
           SUM(CASE WHEN status = 'Failed' THEN 1 ELSE 0 END) as failed, \
           SUM(CASE WHEN status = 'Running' THEN 1 ELSE 0 END) as running, \
           SUM(COALESCE(total_processed, 0)) as total_processed, \
           SUM(COALESCE(total_errors, 0)) as total_errors, \
           AVG(CASE WHEN duration_ms IS NOT NULL THEN duration_ms END) as avg_duration_ms, \
           MAX(duration_ms) as max_duration_ms \
         FROM sys_task_runs \
         WHERE started_at >= datetime('now', '-{} hours')",
        hours
    );

    let rows =
        serde_json::Value::find_by_statement(Statement::from_string(DatabaseBackend::Sqlite, sql))
            .all(db)
            .await;

    let summary = match rows {
        Ok(r) => r.into_iter().next().unwrap_or(serde_json::Value::Null),
        Err(e) => return serde_json::json!({ "error": format!("DB error: {}", e) }),
    };

    // Recent failures
    let failures_sql = format!(
        "SELECT r.id, r.task_id, t.code as task_code, t.description as task_description, \
                r.started_at, r.finished_at, r.duration_ms, r.status, \
                r.total_errors, r.error_message \
         FROM sys_task_runs r \
         LEFT JOIN sys_tasks t ON t.id = r.task_id \
         WHERE r.status = 'Failed' \
           AND r.started_at >= datetime('now', '-{} hours') \
         ORDER BY r.started_at DESC \
         LIMIT 10",
        hours
    );

    let failures = serde_json::Value::find_by_statement(Statement::from_string(
        DatabaseBackend::Sqlite,
        failures_sql,
    ))
    .all(db)
    .await
    .unwrap_or_default();

    serde_json::json!({
        "period_hours": hours,
        "summary": summary,
        "recent_failures": failures,
        "health_status": if failures.is_empty() { "OK" } else { "WARN — есть ошибки задач" },
        "hint": "Используй list_background_jobs для детализации по конкретной задаче."
    })
}

async fn get_performance_stats(hours: u64, slow_threshold_ms: u64) -> serde_json::Value {
    let db = crate::shared::data::db::get_connection();

    // Stats per model
    let model_sql = format!(
        "SELECT \
           model_name, \
           COUNT(*) as message_count, \
           AVG(COALESCE(tokens_used, 0)) as avg_tokens, \
           MAX(COALESCE(tokens_used, 0)) as max_tokens, \
           AVG(duration_ms) as avg_duration_ms, \
           MAX(duration_ms) as max_duration_ms, \
           SUM(CASE WHEN duration_ms > {} THEN 1 ELSE 0 END) as slow_count \
         FROM a018_llm_chat_message \
         WHERE role = 'assistant' \
           AND created_at >= datetime('now', '-{} hours') \
         GROUP BY model_name \
         ORDER BY avg_duration_ms DESC NULLS LAST",
        slow_threshold_ms, hours
    );

    let model_stats = serde_json::Value::find_by_statement(Statement::from_string(
        DatabaseBackend::Sqlite,
        model_sql,
    ))
    .all(db)
    .await
    .unwrap_or_default();

    // Slowest responses
    let slow_sql = format!(
        "SELECT m.id, m.chat_id, m.model_name, m.duration_ms, m.tokens_used, \
                LEFT(m.content, 200) as content_preview, m.created_at \
         FROM a018_llm_chat_message m \
         WHERE m.role = 'assistant' \
           AND m.duration_ms > {} \
           AND m.created_at >= datetime('now', '-{} hours') \
         ORDER BY m.duration_ms DESC \
         LIMIT 10",
        slow_threshold_ms, hours
    );

    let slow_responses = serde_json::Value::find_by_statement(Statement::from_string(
        DatabaseBackend::Sqlite,
        slow_sql,
    ))
    .all(db)
    .await
    .unwrap_or_default();

    // High token usage
    let high_tokens_sql = format!(
        "SELECT m.chat_id, m.model_name, m.tokens_used, m.duration_ms, m.created_at \
         FROM a018_llm_chat_message m \
         WHERE m.role = 'assistant' \
           AND m.tokens_used > 3000 \
           AND m.created_at >= datetime('now', '-{} hours') \
         ORDER BY m.tokens_used DESC \
         LIMIT 10",
        hours
    );

    let high_token_msgs = serde_json::Value::find_by_statement(Statement::from_string(
        DatabaseBackend::Sqlite,
        high_tokens_sql,
    ))
    .all(db)
    .await
    .unwrap_or_default();

    serde_json::json!({
        "period_hours": hours,
        "slow_threshold_ms": slow_threshold_ms,
        "stats_by_model": model_stats,
        "slowest_responses": slow_responses,
        "high_token_messages": high_token_msgs,
        "hint": "Медленные ответы (>10s) и высокое потребление токенов (>3000) \
                 требуют оптимизации system_prompt или уменьшения MAX_TOOL_ITERATIONS."
    })
}

async fn list_background_jobs(status_filter: Option<String>, limit: u64) -> serde_json::Value {
    let db = crate::shared::data::db::get_connection();

    let where_clause = match &status_filter {
        Some(s) => format!("WHERE r.status = '{}'", s.replace('\'', "''")),
        None => String::new(),
    };

    let sql = format!(
        "SELECT r.id, r.task_id, t.code as task_code, t.description as task_description, \
                r.triggered_by, r.started_at, r.finished_at, r.duration_ms, \
                r.status, r.total_processed, r.total_inserted, r.total_updated, \
                r.total_errors, r.error_message \
         FROM sys_task_runs r \
         LEFT JOIN sys_tasks t ON t.id = r.task_id \
         {} \
         ORDER BY r.started_at DESC \
         LIMIT {}",
        where_clause, limit
    );

    let runs =
        serde_json::Value::find_by_statement(Statement::from_string(DatabaseBackend::Sqlite, sql))
            .all(db)
            .await
            .unwrap_or_default();

    // Summary per task
    let summary_sql = "SELECT t.code, t.description, t.task_type, \
           COUNT(r.id) as total_runs, \
           SUM(CASE WHEN r.status = 'Completed' THEN 1 ELSE 0 END) as completed, \
           SUM(CASE WHEN r.status = 'Failed' THEN 1 ELSE 0 END) as failed, \
           MAX(r.started_at) as last_run_at, \
           MAX(CASE WHEN r.status = 'Failed' THEN r.started_at END) as last_failure_at \
         FROM sys_tasks t \
         LEFT JOIN sys_task_runs r ON r.task_id = t.id \
         WHERE t.is_deleted = 0 \
         GROUP BY t.id, t.code, t.description, t.task_type \
         ORDER BY last_run_at DESC NULLS LAST";

    let task_summary = serde_json::Value::find_by_statement(Statement::from_string(
        DatabaseBackend::Sqlite,
        summary_sql.to_string(),
    ))
    .all(db)
    .await
    .unwrap_or_default();

    serde_json::json!({
        "status_filter": status_filter,
        "task_summary": task_summary,
        "recent_runs": runs,
        "hint": "Задачи с failed > 0 требуют внимания. \
                 Используй check_system_health для агрегированной картины по периоду."
    })
}

async fn get_data_integrity_report() -> serde_json::Value {
    let db = crate::shared::data::db::get_connection();

    let counts_sql = "SELECT \
       (SELECT COUNT(*) FROM a004_nomenclature WHERE is_deleted = 0) as nomenclature_count, \
       (SELECT COUNT(*) FROM a006_connection_mp WHERE is_deleted = 0) as connections_count, \
       (SELECT COUNT(*) FROM a012_wb_sale WHERE is_deleted = 0) as wb_sales_count, \
       (SELECT COUNT(*) FROM a017_llm_agent WHERE is_deleted = 0) as llm_agents_count, \
       (SELECT COUNT(*) FROM a018_llm_chat WHERE is_deleted = 0) as llm_chats_count, \
       (SELECT COUNT(*) FROM sys_task_runs WHERE status = 'Running') as running_tasks, \
       (SELECT COUNT(*) FROM sys_task_runs WHERE status = 'Failed' \
         AND started_at >= datetime('now', '-24 hours')) as failed_last_24h";

    let counts = serde_json::Value::find_by_statement(Statement::from_string(
        DatabaseBackend::Sqlite,
        counts_sql.to_string(),
    ))
    .all(db)
    .await
    .unwrap_or_default();

    // Orphaned sales (no connection reference)
    let orphan_sql = "SELECT COUNT(*) as orphaned_sales \
         FROM a012_wb_sale s \
         WHERE s.is_deleted = 0 \
           AND s.connection_mp_ref NOT IN \
               (SELECT id FROM a006_connection_mp WHERE is_deleted = 0)";

    let orphan = serde_json::Value::find_by_statement(Statement::from_string(
        DatabaseBackend::Sqlite,
        orphan_sql.to_string(),
    ))
    .all(db)
    .await
    .unwrap_or_default();

    serde_json::json!({
        "table_counts": counts.into_iter().next().unwrap_or(serde_json::Value::Null),
        "orphaned_records": orphan.into_iter().next().unwrap_or(serde_json::Value::Null),
        "hint": "orphaned_sales > 0 означает продажи без привязки к кабинету МП — \
                 нужна перепривязка или повторный импорт."
    })
}
