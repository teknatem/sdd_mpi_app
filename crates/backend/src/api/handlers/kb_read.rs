use crate::shared::error::ApiError;
use axum::{extract::Path, http::StatusCode, Json};
use once_cell::sync::Lazy;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path as FsPath, PathBuf};

use crate::shared::llm::knowledge_base::{knowledge_base_dir, DocKind as KbDocKind, KnowledgeDoc};

// Compile-time metadata for known DataView modules — used to enrich tree segment names.
static DV_LABELS: Lazy<HashMap<String, String>> = Lazy::new(|| {
    let entries: &[(&str, &str)] = &[
        (
            "dv001_revenue",
            include_str!("../../data_view/dv001_revenue/metadata.json"),
        ),
        (
            "dv004_general_ledger_turnovers",
            include_str!("../../data_view/dv004_general_ledger_turnovers/metadata.json"),
        ),
    ];
    entries
        .iter()
        .filter_map(|(id, json_str)| {
            let v: serde_json::Value = serde_json::from_str(json_str).ok()?;
            let name = v.get("name")?.as_str()?;
            Some((id.to_string(), format!("{} {}", id, name)))
        })
        .collect()
});

/// Наблюдаемая статистика статьи. `None`, если по статье ещё не было обращений.
#[derive(Debug, Clone, Default, Serialize)]
pub struct KbArticleMetrics {
    pub search_hits: i64,
    pub read_hits: i64,
    pub cited_hits: i64,
    pub open_issues: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct KbArticleSummary {
    pub id: String,
    pub title: String,
    pub tags: Vec<String>,
    pub related: Vec<String>,
    pub source_path: Option<String>,
    pub display_path: String,
    /// Не написан руками: техдок приложения или сгенерированная карта.
    pub is_embedded: bool,
    /// Корпус: `business` | `app` | `generated`.
    pub kind: String,
    // ─── ценность и актуальность ───
    pub summary: String,
    pub status: String,
    pub stars: Option<u8>,
    pub updated: Option<String>,
    pub verified: Option<String>,
    pub ttl_days: Option<u32>,
    pub token_cost: u32,
    /// Насколько израсходован срок годности знания, %.
    pub staleness_pct: Option<u32>,
    pub unknown_tags: Vec<String>,
    /// Записи `entities`, не найденные в реестре объектов — куратору видно, какие.
    pub unknown_anchors: Vec<String>,
    pub metrics: KbArticleMetrics,
}

#[derive(Debug, Serialize)]
pub struct KbArticleDetail {
    #[serde(flatten)]
    pub summary_fields: KbArticleSummary,
    pub content: String,
    /// Связи, ведущие на реальные статьи — кликабельны в UI.
    pub related_articles: Vec<KbRelatedArticle>,
    /// Кто ссылается на эту статью.
    pub back_links: Vec<KbRelatedArticle>,
}

#[derive(Debug, Clone, Serialize)]
pub struct KbRelatedArticle {
    pub id: String,
    pub title: String,
}

#[derive(Debug, Serialize)]
pub struct KbStatsResponse {
    pub total_articles: usize,
    pub file_articles: usize,
    pub embedded_articles: usize,
    /// Разбивка по корпусам: курируемое знание, техдоки, сгенерированные карты.
    pub business_articles: usize,
    pub app_articles: usize,
    pub generated_articles: usize,
    /// Ссылки `related`/`[[wiki]]`, не ведущие ни в статью, ни в тег, ни в объект.
    pub dangling_links: usize,
    /// Записи `entities`, которых нет в реестре метаданных.
    pub unknown_anchor_count: usize,
    /// Сколько объектов системы имеют хотя бы один привязанный документ.
    pub anchored_entities: usize,
    pub total_tags: usize,
    pub total_related: usize,
    pub total_folders: usize,
    pub knowledge_base_path: String,
    pub top_tags: Vec<KbCountItem>,
    // ─── здоровье базы ───
    pub drafts: usize,
    pub deprecated: usize,
    /// Статьи, у которых срок годности знания истёк.
    pub stale_articles: usize,
    pub total_token_cost: u32,
    pub vocabulary_terms: usize,
    pub unknown_tag_count: usize,
    /// Строки статистики, не сопоставленные ни с одной живой статьёй
    /// (обычно — след переименования файла).
    pub orphaned_metrics: usize,
    pub open_issues: i64,
    pub loaded_at: String,
    pub diagnostics: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct KbVocabularyResponse {
    pub terms: Vec<KbVocabularyTerm>,
    pub total_terms: usize,
    /// Теги, использованные в статьях, но отсутствующие в словаре — работа куратору.
    pub tags_outside_vocabulary: Vec<KbCountItem>,
}

#[derive(Debug, Serialize)]
pub struct KbVocabularyTerm {
    pub tag: String,
    pub group: String,
    pub label: String,
    pub aliases: Vec<String>,
    pub description: String,
    pub articles: usize,
}

#[derive(Debug, Serialize)]
pub struct KbReloadResponse {
    pub ok: bool,
    pub total_articles: usize,
    pub file_articles: usize,
    pub embedded_articles: usize,
    pub drafts: usize,
    pub vocabulary_terms: usize,
    pub unknown_tag_count: usize,
    pub loaded_at: String,
    pub diagnostics: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct KbCountItem {
    pub name: String,
    pub count: usize,
}

#[derive(Debug, Serialize)]
pub struct KbTreeResponse {
    pub roots: Vec<KbTreeNode>,
}

#[derive(Debug, Clone, Serialize)]
pub struct KbTreeNode {
    pub name: String,
    pub path: String,
    pub node_type: String,
    pub article: Option<KbArticleSummary>,
    pub children: Vec<KbTreeNode>,
}

#[derive(Debug, Default)]
struct MutableTreeNode {
    name: String,
    path: String,
    article: Option<KbArticleSummary>,
    children: BTreeMap<String, MutableTreeNode>,
}

pub async fn stats() -> Json<KbStatsResponse> {
    let kb_dir = knowledge_base_dir();
    let kb = crate::shared::llm::knowledge_base::kb_read();
    let metrics = crate::shared::llm::kb_metrics::snapshot();
    let docs = kb.all_docs();
    let mut tags = BTreeMap::<String, usize>::new();
    let mut related = BTreeSet::<String>::new();
    let mut folders = BTreeSet::<String>::new();
    let mut unknown = BTreeSet::<String>::new();
    let mut live_keys = BTreeSet::<String>::new();
    let mut file_articles = 0usize;
    let mut embedded_articles = 0usize;
    let mut business_articles = 0usize;
    let mut app_articles = 0usize;
    let mut generated_articles = 0usize;
    let mut unknown_anchor_count = 0usize;
    let mut drafts = 0usize;
    let mut deprecated = 0usize;
    let mut stale_articles = 0usize;
    let mut total_token_cost = 0u32;

    for doc in docs.iter() {
        let summary = article_summary(doc, &kb_dir, &metrics);
        if summary.is_embedded {
            embedded_articles += 1;
        } else {
            file_articles += 1;
        }
        match doc.kind {
            KbDocKind::Business => business_articles += 1,
            KbDocKind::App => app_articles += 1,
            KbDocKind::Generated => generated_articles += 1,
        }
        unknown_anchor_count += doc.unknown_anchors.len();
        match summary.status.as_str() {
            "draft" => drafts += 1,
            "deprecated" => deprecated += 1,
            _ => {}
        }
        if summary.staleness_pct.is_some_and(|pct| pct > 100) {
            stale_articles += 1;
        }
        total_token_cost = total_token_cost.saturating_add(summary.token_cost);
        live_keys.insert(doc.metrics_key());
        for tag in &summary.tags {
            *tags.entry(tag.clone()).or_insert(0) += 1;
        }
        for tag in &summary.unknown_tags {
            unknown.insert(tag.clone());
        }
        for item in &summary.related {
            related.insert(item.clone());
        }
        let segments = path_segments(&summary.display_path);
        for depth in 1..segments.len() {
            folders.insert(segments[..depth].join("/"));
        }
    }

    let total_tags = tags.len();
    let mut top_tags = tags
        .into_iter()
        .map(|(name, count)| KbCountItem { name, count })
        .collect::<Vec<_>>();
    top_tags.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.name.cmp(&b.name)));
    top_tags.truncate(20);

    // Осиротевшая строка статистики — след переименования файла у статьи без `uid`.
    // Показываем её, чтобы потеря счётчиков была видна, а не загадочна.
    let orphaned_metrics = metrics
        .keys()
        .filter(|key| !live_keys.contains(*key))
        .count();
    let open_issues = metrics.values().map(|row| row.open_issue_count).sum();

    Json(KbStatsResponse {
        total_articles: docs.len(),
        file_articles,
        embedded_articles,
        business_articles,
        app_articles,
        generated_articles,
        dangling_links: kb.dangling_links(),
        unknown_anchor_count,
        anchored_entities: kb.anchored_entity_count(),
        total_tags,
        total_related: related.len(),
        total_folders: folders.len(),
        knowledge_base_path: kb_dir.display().to_string(),
        top_tags,
        drafts,
        deprecated,
        stale_articles,
        total_token_cost,
        vocabulary_terms: kb.vocabulary().len(),
        unknown_tag_count: unknown.len(),
        orphaned_metrics,
        open_issues,
        loaded_at: kb.loaded_at().to_rfc3339(),
        diagnostics: kb.diagnostics().to_vec(),
    })
}

pub async fn vocabulary() -> Json<KbVocabularyResponse> {
    let kb = crate::shared::llm::knowledge_base::kb_read();
    let terms: Vec<KbVocabularyTerm> = kb
        .vocabulary()
        .terms()
        .map(|term| KbVocabularyTerm {
            articles: kb.tag_count(&term.tag),
            tag: term.tag.clone(),
            group: term.group.clone(),
            label: term.label.clone(),
            aliases: term.aliases.clone(),
            description: term.description.clone(),
        })
        .collect();

    let mut outside = BTreeMap::<String, usize>::new();
    for doc in kb.all_docs() {
        for tag in &doc.unknown_tags {
            *outside.entry(tag.clone()).or_insert(0) += 1;
        }
    }
    let mut tags_outside_vocabulary: Vec<KbCountItem> = outside
        .into_iter()
        .map(|(name, count)| KbCountItem { name, count })
        .collect();
    tags_outside_vocabulary.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.name.cmp(&b.name)));

    Json(KbVocabularyResponse {
        total_terms: terms.len(),
        terms,
        tags_outside_vocabulary,
    })
}

pub async fn issues() -> Json<serde_json::Value> {
    match crate::shared::llm::kb_metrics::list_open_issues(None, 200).await {
        Ok(items) => Json(serde_json::json!({ "total": items.len(), "items": items })),
        Err(error) => {
            Json(serde_json::json!({ "total": 0, "items": [], "error": error.to_string() }))
        }
    }
}

/// Перечитать базу знаний с диска.
///
/// До этого такого роута не существовало (хотя документация на него ссылалась),
/// и правки статей в Obsidian требовали рестарта бэкенда.
pub async fn reload() -> Result<Json<KbReloadResponse>, (StatusCode, String)> {
    crate::shared::llm::kb_metrics::flush().await;
    if let Err(error) = crate::shared::llm::knowledge_base::reload_knowledge_base() {
        return Err((StatusCode::INTERNAL_SERVER_ERROR, error.to_string()));
    }

    let kb = crate::shared::llm::knowledge_base::kb_read();
    let docs = kb.all_docs();
    let embedded_articles = docs.iter().filter(|d| d.is_embedded()).count();
    let drafts = docs
        .iter()
        .filter(|d| d.status == crate::shared::llm::knowledge_base::KbStatus::Draft)
        .count();
    let unknown: BTreeSet<&String> = docs.iter().flat_map(|d| d.unknown_tags.iter()).collect();

    Ok(Json(KbReloadResponse {
        ok: true,
        total_articles: docs.len(),
        file_articles: docs.len() - embedded_articles,
        embedded_articles,
        drafts,
        vocabulary_terms: kb.vocabulary().len(),
        unknown_tag_count: unknown.len(),
        loaded_at: kb.loaded_at().to_rfc3339(),
        diagnostics: kb.diagnostics().to_vec(),
    }))
}

/// Пересчитать профиль данных и переписать сгенерированные карты.
///
/// Отдельно от `reload`: тот перечитывает файлы, а этот их производит — и стоит
/// на порядок дороже (обход всех таблиц каталога).
pub async fn generate() -> Json<crate::shared::llm::kb_generated::GenerateReport> {
    Json(crate::shared::llm::kb_generated::regenerate_all().await)
}

pub async fn tree() -> Json<KbTreeResponse> {
    let kb_dir = knowledge_base_dir();
    let kb = crate::shared::llm::knowledge_base::kb_read();
    let metrics = crate::shared::llm::kb_metrics::snapshot();
    let mut root = MutableTreeNode::default();

    let mut docs = kb.all_docs();
    docs.sort_by(|a, b| a.id.cmp(&b.id));
    for doc in docs {
        insert_article(&mut root, article_summary(doc, &kb_dir, &metrics));
    }

    Json(KbTreeResponse {
        roots: root
            .children
            .into_values()
            .map(MutableTreeNode::into_tree_node)
            .collect(),
    })
}

pub async fn get_article(Path(id): Path<String>) -> Result<Json<KbArticleDetail>, ApiError> {
    let kb_dir = knowledge_base_dir();
    let kb = crate::shared::llm::knowledge_base::kb_read();
    let metrics = crate::shared::llm::kb_metrics::snapshot();
    let Some(doc) = kb.get(&id) else {
        return Err(axum::http::StatusCode::NOT_FOUND.into());
    };

    // Связи, ведущие на реальные статьи, — видимое лицо графа в UI.
    let related_articles = doc
        .related
        .iter()
        .filter_map(|r| kb.get(r))
        .map(|d| KbRelatedArticle {
            id: d.id.clone(),
            title: d.title.clone(),
        })
        .collect();
    let back_links = kb
        .back_links(&id)
        .iter()
        .filter_map(|r| kb.get(r))
        .map(|d| KbRelatedArticle {
            id: d.id.clone(),
            title: d.title.clone(),
        })
        .collect();

    Ok(Json(KbArticleDetail {
        summary_fields: article_summary(doc, &kb_dir, &metrics),
        content: doc.content.clone(),
        related_articles,
        back_links,
    }))
}

impl MutableTreeNode {
    fn folder(name: String, path: String) -> Self {
        Self {
            name,
            path,
            article: None,
            children: BTreeMap::new(),
        }
    }

    fn into_tree_node(self) -> KbTreeNode {
        KbTreeNode {
            name: self.name,
            path: self.path,
            node_type: if self.article.is_some() {
                "article".to_string()
            } else {
                "folder".to_string()
            },
            article: self.article,
            children: self
                .children
                .into_values()
                .map(MutableTreeNode::into_tree_node)
                .collect(),
        }
    }
}

fn insert_article(root: &mut MutableTreeNode, article: KbArticleSummary) {
    let mut segments = path_segments(&article.display_path);
    if segments.is_empty() {
        segments.push(article.id.clone());
    }

    let mut current = root;
    let mut path_acc = Vec::<String>::new();
    for segment in segments.iter().take(segments.len().saturating_sub(1)) {
        path_acc.push(segment.clone());
        let path = path_acc.join("/");
        // Enrich known DataView folder segments with their human-readable name.
        let display_name = DV_LABELS
            .get(segment.as_str())
            .cloned()
            .unwrap_or_else(|| segment.clone());
        current = current
            .children
            .entry(segment.clone())
            .or_insert_with(|| MutableTreeNode::folder(display_name, path));
    }

    // Use the article title as the leaf node display name for embedded docs;
    // for Obsidian files keep the raw segment (matches the filename).
    let key = segments
        .last()
        .cloned()
        .unwrap_or_else(|| article.id.clone());
    let leaf_name = if article.is_embedded {
        article.title.clone()
    } else {
        key.clone()
    };
    let path = segments.join("/");
    current.children.insert(
        key,
        MutableTreeNode {
            name: leaf_name,
            path,
            article: Some(article),
            children: BTreeMap::new(),
        },
    );
}

fn article_summary(
    doc: &KnowledgeDoc,
    kb_dir: &FsPath,
    metrics: &HashMap<String, crate::shared::llm::kb_metrics::MetricsRow>,
) -> KbArticleSummary {
    let source_path = doc.source_path.clone();
    let display_path = source_path
        .as_deref()
        .map(|path| display_path(path, kb_dir))
        .unwrap_or_else(|| format!("embedded/{}.md", doc.id));

    let row = metrics.get(&doc.metrics_key());
    KbArticleSummary {
        id: doc.id.clone(),
        title: doc.title.clone(),
        tags: doc.canonical_tags.clone(),
        related: doc.related.clone(),
        source_path,
        display_path,
        // Признак теперь вычисляется при загрузке — повторно резать путь не нужно.
        is_embedded: doc.is_embedded(),
        kind: doc.kind.as_str().to_string(),
        summary: doc.summary.clone(),
        status: doc.status.as_str().to_string(),
        stars: doc.stars,
        updated: doc.updated.map(|d| d.to_string()),
        verified: doc.verified.map(|d| d.to_string()),
        ttl_days: doc.ttl_days,
        token_cost: doc.token_cost,
        staleness_pct: doc.staleness_pct(),
        unknown_tags: doc.unknown_tags.clone(),
        unknown_anchors: doc.unknown_anchors.clone(),
        metrics: KbArticleMetrics {
            search_hits: row.map(|r| r.search_hits).unwrap_or(0),
            read_hits: row.map(|r| r.read_hits).unwrap_or(0),
            cited_hits: row.map(|r| r.cited_hits).unwrap_or(0),
            open_issues: row.map(|r| r.open_issue_count).unwrap_or(0),
        },
    }
}

/// Returns a user-friendly display path.
///
/// - Obsidian business files: path relative to kb_dir.
/// - Embedded app-docs: strip leading `crates/backend/src/` (first 3 segments) so the tree
///   starts directly from `domain/`, `data_view/`, etc.
fn display_path(source_path: &str, kb_dir: &FsPath) -> String {
    let path = PathBuf::from(source_path);
    if let Ok(relative) = path.strip_prefix(kb_dir) {
        return normalize_path(relative.display().to_string());
    }
    let normalized = normalize_path(source_path.to_string());
    // Strip the first 3 segments (crates/backend/src) for embedded app docs.
    let segments: Vec<&str> = normalized.split('/').filter(|s| !s.is_empty()).collect();
    if segments.len() > 3 {
        segments[3..].join("/")
    } else {
        normalized
    }
}

fn normalize_path(path: String) -> String {
    path.replace('\\', "/")
}

fn path_segments(path: &str) -> Vec<String> {
    normalize_path(path.to_string())
        .split('/')
        .map(str::trim)
        .filter(|segment| !segment.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}
