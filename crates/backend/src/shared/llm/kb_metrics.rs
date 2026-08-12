//! Наблюдаемая статистика по статьям базы знаний (`sys_kb_article_metrics`,
//! `sys_kb_article_issue`).
//!
//! Содержание статьи живёт в markdown-файле, а счётчики — здесь: писать их в
//! frontmatter нельзя, каждое обращение LLM переписывало бы файл и конфликтовало
//! с Obsidian.
//!
//! **Счётчики батчатся** (флаш раз в 30 с): один ход чата даёт десятки
//! инкрементов, а `UPDATE` внутри LLM-цикла — это латентность; потерять ≤30 с
//! статистики при падении процесса не страшно.
//! **Замечания пишутся синхронно** — это событие, а не счётчик, терять нельзя.
//!
//! Три счётчика различаются намеренно: `search_hits` — «поиск счёл релевантным»,
//! `read_hits` — «модель реально потратила токены», `cited_hits` — «попало в
//! ответ, который увидел человек». Их отношение и есть метрика ценности: много
//! поиска и мало чтения — плохой `summary`; много чтения и мало цитирований —
//! плохая статья.

use once_cell::sync::Lazy;
use sea_orm::{ConnectionTrait, DatabaseBackend, FromQueryResult, Statement};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

const FLUSH_INTERVAL_SECS: u64 = 30;

/// Накопленные, но ещё не записанные дельты.
#[derive(Debug, Default, Clone)]
struct Delta {
    doc_id: String,
    title: String,
    token_cost: u32,
    search_hits: i64,
    read_hits: i64,
    cited_hits: i64,
    touched_search: bool,
    touched_read: bool,
}

static PENDING: Lazy<Mutex<HashMap<String, Delta>>> = Lazy::new(|| Mutex::new(HashMap::new()));

/// Кэш прочитанных строк — чтобы ранжирование и API не ходили в БД на каждый вызов.
static SNAPSHOT: Lazy<Mutex<Arc<HashMap<String, MetricsRow>>>> =
    Lazy::new(|| Mutex::new(Arc::new(HashMap::new())));

#[derive(Debug, Clone, Default, Serialize, FromQueryResult)]
pub struct MetricsRow {
    pub article_key: String,
    pub doc_id: String,
    pub title: String,
    pub search_hits: i64,
    pub read_hits: i64,
    pub cited_hits: i64,
    pub issue_count: i64,
    pub open_issue_count: i64,
    pub token_cost_last: i64,
}

/// Идентификация статьи для статистики: стабильный ключ + снимок имени.
#[derive(Debug, Clone)]
pub struct ArticleRef {
    pub key: String,
    pub doc_id: String,
    pub title: String,
    pub token_cost: u32,
}

impl From<&super::knowledge_base::KnowledgeDoc> for ArticleRef {
    fn from(doc: &super::knowledge_base::KnowledgeDoc) -> Self {
        Self {
            key: doc.metrics_key(),
            doc_id: doc.id.clone(),
            title: doc.title.clone(),
            token_cost: doc.token_cost,
        }
    }
}

fn bump(refs: &[ArticleRef], apply: impl Fn(&mut Delta)) {
    let Ok(mut pending) = PENDING.lock() else {
        return; // статистика не стоит того, чтобы валить чат
    };
    for article in refs {
        let entry = pending.entry(article.key.clone()).or_default();
        entry.doc_id = article.doc_id.clone();
        entry.title = article.title.clone();
        entry.token_cost = article.token_cost;
        apply(entry);
    }
}

/// Статьи, попавшие в выдачу поиска (не кандидаты — именно возвращённые).
pub fn record_search(refs: &[ArticleRef]) {
    bump(refs, |d| {
        d.search_hits += 1;
        d.touched_search = true;
    });
}

/// Статья прочитана целиком через `get_knowledge`.
pub fn record_read(article: &ArticleRef) {
    bump(std::slice::from_ref(article), |d| {
        d.read_hits += 1;
        d.touched_read = true;
    });
}

/// Статья процитирована в ответе, который увидел человек.
pub fn record_citation(refs: &[ArticleRef]) {
    bump(refs, |d| d.cited_hits += 1);
}

/// Записать накопленные дельты и обновить кэш. Безопасно вызывать когда угодно.
pub async fn flush() {
    let batch: Vec<(String, Delta)> = {
        let Ok(mut pending) = PENDING.lock() else {
            return;
        };
        pending.drain().collect()
    };

    if !batch.is_empty() {
        let db = crate::shared::data::db::get_connection();
        let now = chrono::Utc::now().to_rfc3339();
        for (key, delta) in batch {
            let statement = Statement::from_sql_and_values(
                DatabaseBackend::Sqlite,
                "INSERT INTO sys_kb_article_metrics \
                 (article_key, doc_id, title, search_hits, read_hits, cited_hits, \
                  token_cost_last, last_search_at, last_read_at, created_at, updated_at) \
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) \
                 ON CONFLICT(article_key) DO UPDATE SET \
                   doc_id          = excluded.doc_id, \
                   title           = excluded.title, \
                   search_hits     = sys_kb_article_metrics.search_hits + excluded.search_hits, \
                   read_hits       = sys_kb_article_metrics.read_hits   + excluded.read_hits, \
                   cited_hits      = sys_kb_article_metrics.cited_hits  + excluded.cited_hits, \
                   token_cost_last = excluded.token_cost_last, \
                   last_search_at  = COALESCE(excluded.last_search_at, sys_kb_article_metrics.last_search_at), \
                   last_read_at    = COALESCE(excluded.last_read_at,   sys_kb_article_metrics.last_read_at), \
                   updated_at      = excluded.updated_at",
                [
                    key.clone().into(),
                    delta.doc_id.into(),
                    delta.title.into(),
                    delta.search_hits.into(),
                    delta.read_hits.into(),
                    delta.cited_hits.into(),
                    (delta.token_cost as i64).into(),
                    delta.touched_search.then(|| now.clone()).into(),
                    delta.touched_read.then(|| now.clone()).into(),
                    now.clone().into(),
                    now.clone().into(),
                ],
            );
            if let Err(error) = db.execute(statement).await {
                tracing::warn!("[kb_metrics] не записана статистика по '{key}': {error}");
            }
        }
    }

    refresh_snapshot().await;
}

/// Перечитать таблицу в процессный кэш.
pub async fn refresh_snapshot() {
    let db = crate::shared::data::db::get_connection();
    let rows = MetricsRow::find_by_statement(Statement::from_string(
        DatabaseBackend::Sqlite,
        "SELECT article_key, doc_id, title, search_hits, read_hits, cited_hits, \
                issue_count, open_issue_count, token_cost_last \
         FROM sys_kb_article_metrics",
    ))
    .all(db)
    .await;

    match rows {
        Ok(rows) => {
            let map: HashMap<String, MetricsRow> = rows
                .into_iter()
                .map(|row| (row.article_key.clone(), row))
                .collect();
            if let Ok(mut guard) = SNAPSHOT.lock() {
                *guard = Arc::new(map);
            }
        }
        Err(error) => tracing::warn!("[kb_metrics] не прочитана статистика: {error}"),
    }
}

/// Текущий снимок статистики. Дёшево: клон `Arc`.
pub fn snapshot() -> Arc<HashMap<String, MetricsRow>> {
    SNAPSHOT
        .lock()
        .map(|guard| guard.clone())
        .unwrap_or_default()
}

/// Фоновый флашер. Запускается из `main.rs`, а НЕ из планировщика: планировщик
/// может быть выключен в конфиге, а статистика должна писаться всегда.
pub fn spawn_flusher() {
    tokio::spawn(async {
        // Первичное наполнение кэша, чтобы UI не ждал первого флаша.
        if let Some(_database_activity) = crate::system::maintenance::try_begin_database_activity()
        {
            refresh_snapshot().await;
        }
        let mut ticker = tokio::time::interval(std::time::Duration::from_secs(FLUSH_INTERVAL_SECS));
        ticker.tick().await; // первый тик срабатывает немедленно
        loop {
            ticker.tick().await;
            if let Some(_database_activity) =
                crate::system::maintenance::try_begin_database_activity()
            {
                flush().await;
            }
        }
    });
}

// ─── Замечания ───────────────────────────────────────────────────────────────

pub struct NewIssue {
    pub article: ArticleRef,
    pub issue_kind: String,
    pub severity: String,
    pub body: String,
    pub quote: String,
    pub chat_id: Option<String>,
    pub agent_id: Option<String>,
    pub reported_by: String,
}

pub struct IssueOutcome {
    pub issue_id: String,
    /// Сколько открытых замечаний у статьи после записи.
    pub open_issues: i64,
}

/// Записать замечание синхронно и пересчитать денормализованные счётчики.
pub async fn record_issue(issue: NewIssue) -> anyhow::Result<IssueOutcome> {
    let db = crate::shared::data::db::get_connection();
    let now = chrono::Utc::now().to_rfc3339();
    let issue_id = uuid::Uuid::new_v4().to_string();

    db.execute(Statement::from_sql_and_values(
        DatabaseBackend::Sqlite,
        "INSERT INTO sys_kb_article_issue \
         (id, article_key, doc_id, issue_kind, severity, body, quote, chat_id, agent_id, \
          reported_by, status, created_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'open', ?)",
        [
            issue_id.clone().into(),
            issue.article.key.clone().into(),
            issue.article.doc_id.clone().into(),
            issue.issue_kind.into(),
            issue.severity.into(),
            issue.body.into(),
            issue.quote.into(),
            issue.chat_id.into(),
            issue.agent_id.into(),
            issue.reported_by.into(),
            now.clone().into(),
        ],
    ))
    .await?;

    // Счётчики замечаний считаем из таблицы событий, а не инкрементом: так они
    // остаются верными после ручного закрытия замечания в БД.
    db.execute(Statement::from_sql_and_values(
        DatabaseBackend::Sqlite,
        "INSERT INTO sys_kb_article_metrics \
         (article_key, doc_id, title, issue_count, open_issue_count, token_cost_last, \
          last_issue_at, created_at, updated_at) \
         VALUES (?, ?, ?, \
                 (SELECT COUNT(*) FROM sys_kb_article_issue WHERE article_key = ?), \
                 (SELECT COUNT(*) FROM sys_kb_article_issue WHERE article_key = ? AND status = 'open'), \
                 ?, ?, ?, ?) \
         ON CONFLICT(article_key) DO UPDATE SET \
           issue_count      = excluded.issue_count, \
           open_issue_count = excluded.open_issue_count, \
           last_issue_at    = excluded.last_issue_at, \
           updated_at       = excluded.updated_at",
        [
            issue.article.key.clone().into(),
            issue.article.doc_id.into(),
            issue.article.title.into(),
            issue.article.key.clone().into(),
            issue.article.key.clone().into(),
            (issue.article.token_cost as i64).into(),
            now.clone().into(),
            now.clone().into(),
            now.into(),
        ],
    ))
    .await?;

    let open_issues = count_open_issues(&issue.article.key).await;
    refresh_snapshot().await;

    Ok(IssueOutcome {
        issue_id,
        open_issues,
    })
}

async fn count_open_issues(article_key: &str) -> i64 {
    #[derive(FromQueryResult)]
    struct Count {
        n: i64,
    }
    let db = crate::shared::data::db::get_connection();
    Count::find_by_statement(Statement::from_sql_and_values(
        DatabaseBackend::Sqlite,
        "SELECT COUNT(*) AS n FROM sys_kb_article_issue WHERE article_key = ? AND status = 'open'",
        [article_key.into()],
    ))
    .one(db)
    .await
    .ok()
    .flatten()
    .map(|c| c.n)
    .unwrap_or(0)
}

#[derive(Debug, Clone, Serialize, FromQueryResult)]
pub struct IssueRow {
    pub id: String,
    pub article_key: String,
    pub doc_id: String,
    pub issue_kind: String,
    pub severity: String,
    pub body: String,
    pub quote: String,
    pub chat_id: Option<String>,
    pub kb_edit_id: Option<String>,
    pub status: String,
    pub created_at: String,
}

/// Открытые замечания, опционально по одной статье.
pub async fn list_open_issues(doc_id: Option<&str>, limit: u32) -> anyhow::Result<Vec<IssueRow>> {
    let db = crate::shared::data::db::get_connection();
    let statement = match doc_id {
        Some(id) => Statement::from_sql_and_values(
            DatabaseBackend::Sqlite,
            "SELECT id, article_key, doc_id, issue_kind, severity, body, quote, chat_id, \
                    kb_edit_id, status, created_at \
             FROM sys_kb_article_issue \
             WHERE status = 'open' AND doc_id = ? \
             ORDER BY created_at DESC LIMIT ?",
            [id.into(), (limit as i64).into()],
        ),
        None => Statement::from_sql_and_values(
            DatabaseBackend::Sqlite,
            "SELECT id, article_key, doc_id, issue_kind, severity, body, quote, chat_id, \
                    kb_edit_id, status, created_at \
             FROM sys_kb_article_issue \
             WHERE status = 'open' \
             ORDER BY created_at DESC LIMIT ?",
            [(limit as i64).into()],
        ),
    };
    Ok(IssueRow::find_by_statement(statement).all(db).await?)
}

/// Привязать замечание к заведённому тикету a031_kb_edit.
pub async fn link_issue_to_kb_edit(issue_id: &str, kb_edit_id: &str) {
    let db = crate::shared::data::db::get_connection();
    let result = db
        .execute(Statement::from_sql_and_values(
            DatabaseBackend::Sqlite,
            "UPDATE sys_kb_article_issue SET kb_edit_id = ? WHERE id = ?",
            [kb_edit_id.into(), issue_id.into()],
        ))
        .await;
    if let Err(error) = result {
        tracing::warn!("[kb_metrics] замечание '{issue_id}' не связано с тикетом: {error}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn article(key: &str) -> ArticleRef {
        ArticleRef {
            key: key.to_string(),
            doc_id: key.to_string(),
            title: "Т".into(),
            token_cost: 100,
        }
    }

    #[test]
    fn counters_accumulate_before_flush() {
        PENDING.lock().unwrap().clear();

        let a = article("kb-1");
        record_search(&[a.clone(), article("kb-2")]);
        record_search(&[a.clone()]);
        record_read(&a);
        record_citation(&[a.clone()]);

        let pending = PENDING.lock().unwrap();
        let one = &pending["kb-1"];
        assert_eq!(one.search_hits, 2);
        assert_eq!(one.read_hits, 1);
        assert_eq!(one.cited_hits, 1);
        assert!(one.touched_search && one.touched_read);

        // Вторая статья попала только в поиск — счётчики обязаны различаться.
        let two = &pending["kb-2"];
        assert_eq!(two.search_hits, 1);
        assert_eq!(two.read_hits, 0);
        assert!(!two.touched_read);
    }

    #[test]
    fn metrics_key_comes_from_uid_then_doc_id() {
        use super::super::knowledge_base::KnowledgeDoc;

        let mut doc = KnowledgeDoc {
            id: "funnel-overview".into(),
            title: "Обзор".into(),
            ..Default::default()
        };
        assert_eq!(ArticleRef::from(&doc).key, "funnel-overview");

        doc.uid = Some("kb-abcd1234".into());
        let r = ArticleRef::from(&doc);
        assert_eq!(r.key, "kb-abcd1234");
        // doc_id всё равно снимается — по нему опознаётся осиротевшая строка.
        assert_eq!(r.doc_id, "funnel-overview");
    }
}
