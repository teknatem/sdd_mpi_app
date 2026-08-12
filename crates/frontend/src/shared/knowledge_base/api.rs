use crate::shared::api_utils::api_base;
use gloo_net::http::Request;
use serde::Deserialize;

/// Наблюдаемая статистика статьи. Три счётчика различаются намеренно:
/// поиск счёл релевантным / модель прочитала / попало в ответ человеку.
#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
pub struct KbArticleMetrics {
    pub search_hits: i64,
    pub read_hits: i64,
    pub cited_hits: i64,
    pub open_issues: i64,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct KbArticleSummary {
    pub id: String,
    pub title: String,
    pub tags: Vec<String>,
    pub related: Vec<String>,
    pub source_path: Option<String>,
    pub display_path: String,
    pub is_embedded: bool,
    /// Корпус: `business` | `app` | `generated`.
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub status: String,
    pub stars: Option<u8>,
    pub updated: Option<String>,
    pub verified: Option<String>,
    pub ttl_days: Option<u32>,
    #[serde(default)]
    pub token_cost: u32,
    pub staleness_pct: Option<u32>,
    #[serde(default)]
    pub unknown_tags: Vec<String>,
    #[serde(default)]
    pub unknown_anchors: Vec<String>,
    #[serde(default)]
    pub metrics: KbArticleMetrics,
}

/// Бэкенд отдаёт поля сводки плоско (`#[serde(flatten)]`), поэтому здесь они
/// перечислены заново, а не вложены.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct KbArticleDetail {
    pub id: String,
    pub title: String,
    pub tags: Vec<String>,
    pub related: Vec<String>,
    pub source_path: Option<String>,
    pub display_path: String,
    pub is_embedded: bool,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub status: String,
    pub stars: Option<u8>,
    pub updated: Option<String>,
    pub verified: Option<String>,
    pub ttl_days: Option<u32>,
    #[serde(default)]
    pub token_cost: u32,
    pub staleness_pct: Option<u32>,
    #[serde(default)]
    pub unknown_tags: Vec<String>,
    #[serde(default)]
    pub unknown_anchors: Vec<String>,
    #[serde(default)]
    pub metrics: KbArticleMetrics,
    pub content: String,
    #[serde(default)]
    pub related_articles: Vec<KbRelatedArticle>,
    #[serde(default)]
    pub back_links: Vec<KbRelatedArticle>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct KbRelatedArticle {
    pub id: String,
    pub title: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct KbStatsResponse {
    pub total_articles: usize,
    pub file_articles: usize,
    pub embedded_articles: usize,
    /// Разбивка по корпусам: знание организации, техдоки, машинные карты.
    #[serde(default)]
    pub business_articles: usize,
    #[serde(default)]
    pub app_articles: usize,
    #[serde(default)]
    pub generated_articles: usize,
    /// Работа куратора: связи в никуда и якоря на несуществующие объекты.
    #[serde(default)]
    pub dangling_links: usize,
    #[serde(default)]
    pub unknown_anchor_count: usize,
    #[serde(default)]
    pub anchored_entities: usize,
    pub total_tags: usize,
    pub total_related: usize,
    pub total_folders: usize,
    pub knowledge_base_path: String,
    pub top_tags: Vec<KbCountItem>,
    #[serde(default)]
    pub drafts: usize,
    #[serde(default)]
    pub deprecated: usize,
    #[serde(default)]
    pub stale_articles: usize,
    #[serde(default)]
    pub total_token_cost: u32,
    #[serde(default)]
    pub vocabulary_terms: usize,
    #[serde(default)]
    pub unknown_tag_count: usize,
    #[serde(default)]
    pub orphaned_metrics: usize,
    #[serde(default)]
    pub open_issues: i64,
    #[serde(default)]
    pub loaded_at: String,
    #[serde(default)]
    pub diagnostics: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct KbVocabularyResponse {
    pub terms: Vec<KbVocabularyTerm>,
    pub total_terms: usize,
    pub tags_outside_vocabulary: Vec<KbCountItem>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct KbVocabularyTerm {
    pub tag: String,
    pub group: String,
    pub label: String,
    pub aliases: Vec<String>,
    pub description: String,
    pub articles: usize,
}

/// Итог пересборки карт корпуса `generated`.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct KbGenerateResponse {
    pub files: Vec<String>,
    pub tables_profiled: usize,
    pub plugins: usize,
    pub skills: usize,
    pub quality_checks: usize,
    #[serde(default)]
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct KbReloadResponse {
    pub ok: bool,
    pub total_articles: usize,
    pub file_articles: usize,
    pub drafts: usize,
    pub vocabulary_terms: usize,
    pub unknown_tag_count: usize,
    #[serde(default)]
    pub diagnostics: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct KbCountItem {
    pub name: String,
    pub count: usize,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct KbTreeResponse {
    pub roots: Vec<KbTreeNode>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct KbTreeNode {
    pub name: String,
    pub path: String,
    pub node_type: String,
    pub article: Option<KbArticleSummary>,
    pub children: Vec<KbTreeNode>,
}

pub async fn fetch_kb_stats() -> Result<KbStatsResponse, String> {
    fetch_json("/api/kb/stats").await
}

pub async fn fetch_kb_tree() -> Result<KbTreeResponse, String> {
    fetch_json("/api/kb/tree").await
}

pub async fn fetch_kb_article(id: &str) -> Result<KbArticleDetail, String> {
    fetch_json(&format!("/api/kb/articles/{}", urlencoding::encode(id))).await
}

pub async fn fetch_kb_vocabulary() -> Result<KbVocabularyResponse, String> {
    fetch_json("/api/kb/vocabulary").await
}

/// Перечитать базу с диска — после правок статей в Obsidian.
pub async fn post_kb_reload() -> Result<KbReloadResponse, String> {
    let url = format!("{}{}", api_base(), "/api/kb/reload");
    let response = Request::post(&url)
        .send()
        .await
        .map_err(|e| format!("Ошибка сети: {}", e))?;
    if !response.ok() {
        return Err(format!("Ошибка сервера: HTTP {}", response.status()));
    }
    response
        .json::<KbReloadResponse>()
        .await
        .map_err(|e| format!("Ошибка парсинга: {}", e))
}

/// Пересобрать карты из БД и рантайма. Отдельно от «Перечитать базу»: та читает
/// файлы, а эта их производит — с обходом всех таблиц каталога.
pub async fn post_kb_generate() -> Result<KbGenerateResponse, String> {
    let url = format!("{}{}", api_base(), "/api/kb/generate");
    let response = Request::post(&url)
        .send()
        .await
        .map_err(|e| format!("Ошибка сети: {}", e))?;
    if !response.ok() {
        return Err(format!("Ошибка сервера: HTTP {}", response.status()));
    }
    response
        .json::<KbGenerateResponse>()
        .await
        .map_err(|e| format!("Ошибка парсинга: {}", e))
}

async fn fetch_json<T>(path: &str) -> Result<T, String>
where
    T: for<'de> Deserialize<'de>,
{
    let url = format!("{}{}", api_base(), path);
    let response = Request::get(&url)
        .send()
        .await
        .map_err(|e| format!("Ошибка сети: {}", e))?;
    if !response.ok() {
        return Err(format!("Ошибка сервера: HTTP {}", response.status()));
    }
    response
        .json::<T>()
        .await
        .map_err(|e| format!("Ошибка парсинга: {}", e))
}
