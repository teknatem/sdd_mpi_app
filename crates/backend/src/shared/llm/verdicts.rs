//! Вердикты о качестве работы агентов (`sys_llm_verdict`) и сводная статистика
//! для дашборда качества LLM.
//!
//! Мотивация: `sys_tool_trace` отвечает на вопрос «что вызывалось и не упало ли
//! технически», но не на вопрос «помог ли ответ». Пока второго нет, каждое
//! изменение промпта или навыка вносится вслепую — новых механизмов становится
//! больше, а понять, стало ли лучше, нечем.
//!
//! Вердикты ставит LLM-судья (`task027_llm_judge`) по реальным диалогам и прогон
//! эталонного набора (`task028_llm_golden_set`). Запись автоматическая: оценка —
//! наблюдение, а не мутация боевых данных.

use crate::shared::data_access::row_json::{fetch_json_rows, JsonBind};
use serde_json::Value;

/// Значения `verdict`, которые принимаются на запись. Всё прочее — ошибка вызова:
/// молча подставленный дефолт исказил бы метрику, ради которой таблица и заводилась.
pub const VERDICTS: &[&str] = &["solved", "partial", "failed"];

/// Классификация провала. Список закрытый, чтобы группировка на дашборде не
/// рассыпалась на синонимы, которые придумывает модель.
pub const FAILURE_KINDS: &[&str] = &[
    "sql_error",
    "tool_loop",
    "wrong_data",
    "missing_context",
    "no_answer",
    "refused",
    "other",
];

pub const SOURCE_AUDIT: &str = "audit";
pub const SOURCE_GOLDEN: &str = "golden";

/// Одна оценка, готовая к записи.
#[derive(Debug, Clone, Default)]
pub struct NewVerdict {
    pub source: String,
    pub chat_id: String,
    pub message_id: Option<String>,
    pub case_id: Option<String>,
    pub agent_type: Option<String>,
    pub skill_id: Option<String>,
    pub intent: Option<String>,
    pub model: Option<String>,
    pub verdict: String,
    pub failure_kind: Option<String>,
    pub reason: String,
    pub tool_calls: i64,
    pub tool_failures: i64,
    pub judge_session_id: Option<String>,
    pub judge_model: Option<String>,
}

fn opt_bind(value: &Option<String>) -> JsonBind {
    match value {
        Some(text) if !text.trim().is_empty() => JsonBind::Text(text.trim().to_string()),
        _ => JsonBind::Null,
    }
}

/// Записать вердикты пачкой. Возвращает (записано, пропущено_как_дубликат).
///
/// Дубликат — не ошибка: судья может повторно попасть на уже оценённый чат, если
/// прогон пересёкся с предыдущим по окну. Уникальный индекс это гасит, а вызывающий
/// получает честный счёт вместо исключения.
pub async fn insert_batch(items: &[NewVerdict]) -> Result<(usize, usize), String> {
    let mut inserted = 0usize;
    let mut skipped = 0usize;
    let now = chrono::Utc::now().to_rfc3339();

    for item in items {
        if !VERDICTS.contains(&item.verdict.as_str()) {
            return Err(format!(
                "Недопустимый verdict '{}'. Допустимые: {}",
                item.verdict,
                VERDICTS.join(", ")
            ));
        }
        if item.chat_id.trim().is_empty() {
            return Err("chat_id обязателен".to_string());
        }
        if let Some(kind) = item.failure_kind.as_deref() {
            if !kind.trim().is_empty() && !FAILURE_KINDS.contains(&kind) {
                return Err(format!(
                    "Недопустимый failure_kind '{}'. Допустимые: {}",
                    kind,
                    FAILURE_KINDS.join(", ")
                ));
            }
        }

        let binds = vec![
            JsonBind::Text(uuid::Uuid::new_v4().to_string()),
            JsonBind::Text(if item.source.trim().is_empty() {
                SOURCE_AUDIT.to_string()
            } else {
                item.source.trim().to_string()
            }),
            JsonBind::Text(item.chat_id.trim().to_string()),
            opt_bind(&item.message_id),
            opt_bind(&item.case_id),
            opt_bind(&item.agent_type),
            opt_bind(&item.skill_id),
            opt_bind(&item.intent),
            opt_bind(&item.model),
            JsonBind::Text(item.verdict.clone()),
            opt_bind(&item.failure_kind),
            JsonBind::Text(item.reason.trim().to_string()),
            JsonBind::Int(item.tool_calls.max(0)),
            JsonBind::Int(item.tool_failures.max(0)),
            opt_bind(&item.judge_session_id),
            opt_bind(&item.judge_model),
            JsonBind::Text(now.clone()),
        ];

        // INSERT OR IGNORE + RETURNING: строка вернётся только если реально вставилась,
        // поэтому пустой результат и есть «дубликат», без отдельного SELECT.
        let (rows, _) = fetch_json_rows(
            "INSERT OR IGNORE INTO sys_llm_verdict \
             (id, source, chat_id, message_id, case_id, agent_type, skill_id, intent, model, \
              verdict, failure_kind, reason, tool_calls, tool_failures, judge_session_id, \
              judge_model, created_at) \
             VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?) RETURNING id",
            binds,
        )
        .await?;

        if rows.is_empty() {
            skipped += 1;
        } else {
            inserted += 1;
        }
    }

    Ok((inserted, skipped))
}

/// Сколько вердиктов записал прогон задачи. Нужно, чтобы задача отчитывалась
/// по строкам в таблице, а не по тексту финального сообщения модели.
pub async fn verdicts_by_session(session_id: &str) -> Result<i64, String> {
    let (rows, _) = fetch_json_rows(
        "SELECT COUNT(*) AS n FROM sys_llm_verdict WHERE judge_session_id = ?",
        vec![JsonBind::Text(session_id.to_string())],
    )
    .await?;
    Ok(rows
        .first()
        .and_then(|row| row.get("n"))
        .and_then(Value::as_i64)
        .unwrap_or(0))
}

/// Чаты за окно, у которых ещё нет вердикта, вместе со сводкой по трассе инструментов.
///
/// Отдаётся судье готовым, вместо того чтобы просить его написать SQL по `a018_*`:
/// сгенерированный запрос — самая частая точка отказа фоновых LLM-задач, а выборка
/// кандидатов детерминирована и не нуждается в модели.
pub async fn chats_awaiting_verdict(lookback_days: i64, limit: i64) -> Result<Vec<Value>, String> {
    let lookback = lookback_days.clamp(1, 90);
    let limit = limit.clamp(1, 50);

    let sql = format!(
        "SELECT c.id AS chat_id, \
                c.description AS chat_title, \
                c.code AS chat_code, \
                c.model_name AS model, \
                a.agent_type AS agent_type, \
                COUNT(m.id) AS message_count, \
                MAX(m.created_at) AS last_message_at \
         FROM a018_llm_chat c \
         JOIN a018_llm_chat_message m ON m.chat_id = c.id \
         LEFT JOIN a017_llm_agent a ON a.id = c.agent_id \
         WHERE c.is_deleted = 0 \
           AND m.created_at >= datetime('now', '-{lookback} days') \
           AND NOT EXISTS ( \
                 SELECT 1 FROM sys_llm_verdict v \
                 WHERE v.chat_id = c.id AND v.source = 'audit') \
           AND c.code NOT LIKE 'KB-ANALYZE-%' \
           AND c.code NOT LIKE 'KB-POST-%' \
           AND c.code NOT LIKE 'KB-INTAKE-%' \
           AND c.code NOT LIKE 'LLM-JUDGE-%' \
           AND c.code NOT LIKE 'GOLDEN-%' \
         GROUP BY c.id \
         HAVING COUNT(m.id) >= 2 \
         ORDER BY last_message_at DESC \
         LIMIT {limit}"
    );

    let (rows, _) = fetch_json_rows(&sql, Vec::new()).await?;
    Ok(rows)
}

/// Полная выписка по диалогу для оценки: сообщения + сводка вызовов инструментов.
///
/// Тексты подрезаются: судье нужен смысл ответа, а не полный дамп таблицы,
/// который вытеснит из контекста остальные диалоги окна.
pub async fn chat_digest(chat_id: &str, max_chars: usize) -> Result<Value, String> {
    let max_chars = max_chars.clamp(200, 4000);

    let (messages, _) = fetch_json_rows(
        "SELECT id, role, substr(content, 1, ?) AS content, model_name, intent, \
                skill_trace_json, duration_ms, created_at \
         FROM a018_llm_chat_message \
         WHERE chat_id = ? \
         ORDER BY created_at ASC \
         LIMIT 40",
        vec![
            JsonBind::Int(max_chars as i64),
            JsonBind::Text(chat_id.to_string()),
        ],
    )
    .await?;

    let (tools, _) = fetch_json_rows(
        "SELECT tool, \
                COUNT(*) AS calls, \
                SUM(CASE WHEN ok = 0 THEN 1 ELSE 0 END) AS failures, \
                MAX(ms) AS max_ms \
         FROM sys_tool_trace \
         WHERE chat_id = ? \
         GROUP BY tool \
         ORDER BY failures DESC, calls DESC",
        vec![JsonBind::Text(chat_id.to_string())],
    )
    .await?;

    // Тексты ошибок инструментов — самый информативный вход для судьи: по ним
    // отличается «модель ошиблась» от «инструмент сломан».
    let (errors, _) = fetch_json_rows(
        "SELECT tool, substr(COALESCE(summary, output_json), 1, 400) AS detail, created_at \
         FROM sys_tool_trace \
         WHERE chat_id = ? AND ok = 0 \
         ORDER BY created_at ASC \
         LIMIT 15",
        vec![JsonBind::Text(chat_id.to_string())],
    )
    .await?;

    Ok(serde_json::json!({
        "chat_id": chat_id,
        "messages": messages,
        "tool_summary": tools,
        "tool_errors": errors,
    }))
}

/// Сводная статистика для дашборда качества LLM за окно в днях.
pub async fn quality_overview(days: i64) -> Result<Value, String> {
    let days = days.clamp(1, 365);

    // Инструменты: частота, доля отказов, латентность. Ради этого в 0155 и заведён
    // индекс по `tool`, у которого до сих пор не было потребителя.
    let (tools, _) = fetch_json_rows(
        &format!(
            "SELECT tool, \
                    COUNT(*) AS calls, \
                    SUM(CASE WHEN ok = 0 THEN 1 ELSE 0 END) AS failures, \
                    ROUND(AVG(ms), 0) AS avg_ms, \
                    MAX(ms) AS max_ms \
             FROM sys_tool_trace \
             WHERE created_at >= datetime('now', '-{days} days') \
             GROUP BY tool \
             ORDER BY failures DESC, calls DESC \
             LIMIT 60"
        ),
        Vec::new(),
    )
    .await?;

    // Итерации на ответ: приближение к «модель ходила по кругу». Потолок цикла — 40,
    // близкие к нему значения означают, что ответ дался почти на пределе.
    let (iterations, _) = fetch_json_rows(
        &format!(
            // COALESCE обязателен: на пустом окне агрегаты дают NULL, а типизация
            // ответа на границе API ждёт числа и упала бы на «нет данных».
            "SELECT COALESCE(ROUND(AVG(iters), 2), 0.0) AS avg_iterations, \
                    COALESCE(MAX(iters), 0) AS max_iterations, \
                    COALESCE(SUM(CASE WHEN iters >= 20 THEN 1 ELSE 0 END), 0) AS heavy_answers, \
                    COUNT(*) AS answers \
             FROM (SELECT message_id, MAX(iteration) + 1 AS iters \
                   FROM sys_tool_trace \
                   WHERE created_at >= datetime('now', '-{days} days') \
                   GROUP BY message_id)"
        ),
        Vec::new(),
    )
    .await?;

    let (verdicts, _) = fetch_json_rows(
        &format!(
            "SELECT source, verdict, COUNT(*) AS n \
             FROM sys_llm_verdict \
             WHERE created_at >= datetime('now', '-{days} days') \
             GROUP BY source, verdict"
        ),
        Vec::new(),
    )
    .await?;

    let (failures, _) = fetch_json_rows(
        &format!(
            "SELECT COALESCE(failure_kind, 'other') AS failure_kind, \
                    COUNT(*) AS n \
             FROM sys_llm_verdict \
             WHERE verdict != 'solved' \
               AND created_at >= datetime('now', '-{days} days') \
             GROUP BY failure_kind \
             ORDER BY n DESC"
        ),
        Vec::new(),
    )
    .await?;

    let (by_skill, _) = fetch_json_rows(
        &format!(
            "SELECT COALESCE(NULLIF(skill_id, ''), '(без навыка)') AS skill_id, \
                    COUNT(*) AS total, \
                    SUM(CASE WHEN verdict = 'solved' THEN 1 ELSE 0 END) AS solved, \
                    SUM(CASE WHEN verdict = 'failed' THEN 1 ELSE 0 END) AS failed \
             FROM sys_llm_verdict \
             WHERE created_at >= datetime('now', '-{days} days') \
             GROUP BY skill_id \
             ORDER BY total DESC \
             LIMIT 30"
        ),
        Vec::new(),
    )
    .await?;

    // Расхождение быстрого (rule-based) интента и того, что реально произошло,
    // видно только по факту активации навыка — обе величины пишутся в сообщение.
    let (intents, _) = fetch_json_rows(
        &format!(
            "SELECT COALESCE(NULLIF(intent, ''), '(нет)') AS intent, COUNT(*) AS n \
             FROM a018_llm_chat_message \
             WHERE role = 'user' \
               AND created_at >= datetime('now', '-{days} days') \
             GROUP BY intent \
             ORDER BY n DESC \
             LIMIT 25"
        ),
        Vec::new(),
    )
    .await?;

    // Ценность статей KB: много поиска и мало чтений — плохой summary;
    // много чтений и мало цитирований — плохая статья.
    let (kb, _) = fetch_json_rows(
        "SELECT doc_id, title, search_hits, read_hits, cited_hits, open_issue_count \
         FROM sys_kb_article_metrics \
         WHERE search_hits > 0 OR read_hits > 0 OR open_issue_count > 0 \
         ORDER BY (search_hits + read_hits) DESC \
         LIMIT 30",
        Vec::new(),
    )
    .await?;

    let (recent, _) = fetch_json_rows(
        &format!(
            "SELECT v.chat_id, v.source, v.verdict, v.failure_kind, v.reason, \
                    v.skill_id, v.agent_type, v.model, v.case_id, v.created_at, \
                    c.description AS chat_title \
             FROM sys_llm_verdict v \
             LEFT JOIN a018_llm_chat c ON c.id = v.chat_id \
             WHERE v.created_at >= datetime('now', '-{days} days') \
             ORDER BY v.created_at DESC \
             LIMIT 50"
        ),
        Vec::new(),
    )
    .await?;

    Ok(serde_json::json!({
        "days": days,
        "tools": tools,
        // Пустой объект, а не null: у потребителя поля имеют дефолты, а null
        // сломал бы типизацию на границе API.
        "iterations": iterations
            .into_iter()
            .next()
            .unwrap_or_else(|| serde_json::json!({})),
        "verdicts": verdicts,
        "failure_kinds": failures,
        "by_skill": by_skill,
        "intents": intents,
        "kb_articles": kb,
        "recent_verdicts": recent,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Схема из самой миграции: тест обязан ломаться, если её правка разойдётся
    /// с тем, на что рассчитывает код записи.
    const MIGRATION: &str = include_str!("../../../../../migrations/0204_sys_llm_verdict.sql");

    /// Дедупликация вердиктов держится на двух допущениях о SQLite: уникальный
    /// индекс по ВЫРАЖЕНИЮ (`COALESCE(case_id,'')`) и `INSERT OR IGNORE … RETURNING`,
    /// который на подавленной вставке возвращает пусто. Оба проверяем на живом
    /// движке, а не на вере: иначе судья молча писал бы дубликаты.
    #[tokio::test]
    async fn duplicate_audit_verdict_is_suppressed() {
        use sqlx::sqlite::SqlitePoolOptions;

        let pool = SqlitePoolOptions::new()
            .connect("sqlite::memory:")
            .await
            .unwrap();
        // Комментарии убираем до разбиения: иначе хвостовой блок `--` становится
        // отдельным «оператором» и SQLite отвечает «incomplete input».
        let sql: String = MIGRATION
            .lines()
            .filter(|line| !line.trim_start().starts_with("--"))
            .collect::<Vec<_>>()
            .join("\n");
        for statement in sql.split(';') {
            if statement.trim().is_empty() {
                continue;
            }
            sqlx::query(statement).execute(&pool).await.unwrap();
        }

        let insert = "INSERT OR IGNORE INTO sys_llm_verdict \
                      (id, source, chat_id, verdict, reason, created_at) \
                      VALUES (?,?,?,?,?,?) RETURNING id";

        let first = sqlx::query(insert)
            .bind("id-1")
            .bind("audit")
            .bind("chat-1")
            .bind("solved")
            .bind("ok")
            .bind("2026-08-08T00:00:00Z")
            .fetch_all(&pool)
            .await
            .unwrap();
        assert_eq!(first.len(), 1, "первая вставка должна вернуть строку");

        let second = sqlx::query(insert)
            .bind("id-2")
            .bind("audit")
            .bind("chat-1")
            .bind("failed")
            .bind("повтор")
            .bind("2026-08-08T01:00:00Z")
            .fetch_all(&pool)
            .await
            .unwrap();
        assert!(
            second.is_empty(),
            "повторная оценка того же чата должна подавляться, а не дублироваться"
        );

        // Голден-прогон того же чата — другой источник, и он проходить обязан.
        let golden = sqlx::query(insert)
            .bind("id-3")
            .bind("golden")
            .bind("chat-1")
            .bind("solved")
            .bind("эталон")
            .bind("2026-08-08T02:00:00Z")
            .fetch_all(&pool)
            .await
            .unwrap();
        assert_eq!(golden.len(), 1, "источник golden не должен конфликтовать с audit");
    }

    /// Опечатка в вердикте не должна тихо превратиться в валидную строку:
    /// метрика, в которую можно записать что угодно, ничего не измеряет.
    #[tokio::test]
    async fn rejects_unknown_verdict() {
        let items = vec![NewVerdict {
            source: SOURCE_AUDIT.into(),
            chat_id: "chat-1".into(),
            verdict: "ok".into(),
            ..Default::default()
        }];
        let error = insert_batch(&items).await.unwrap_err();
        assert!(error.contains("verdict"), "unexpected error: {error}");
    }

    #[tokio::test]
    async fn rejects_unknown_failure_kind() {
        let items = vec![NewVerdict {
            source: SOURCE_AUDIT.into(),
            chat_id: "chat-1".into(),
            verdict: "failed".into(),
            failure_kind: Some("hallucination".into()),
            ..Default::default()
        }];
        let error = insert_batch(&items).await.unwrap_err();
        assert!(error.contains("failure_kind"), "unexpected error: {error}");
    }
}
