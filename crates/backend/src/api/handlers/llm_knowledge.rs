use crate::shared::error::ApiError;
use axum::{
    extract::{Path, Query},
    Json,
};
use serde::{Deserialize, Serialize};

use crate::shared::llm::knowledge_base::KnowledgeDoc;

#[derive(Debug, Deserialize)]
pub struct LlmKnowledgeListParams {
    #[serde(default)]
    pub tag: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct LlmKnowledgeListItem {
    pub id: String,
    pub title: String,
    pub tags: Vec<String>,
    pub related: Vec<String>,
    pub source_path: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct LlmKnowledgeDetailResponse {
    pub id: String,
    pub title: String,
    pub tags: Vec<String>,
    pub related: Vec<String>,
    pub source_path: Option<String>,
    pub content: String,
}

pub async fn list(Query(params): Query<LlmKnowledgeListParams>) -> Json<Vec<LlmKnowledgeListItem>> {
    let kb = crate::shared::llm::knowledge_base::kb_read();
    let docs: Vec<&KnowledgeDoc> = if params.tag.is_empty() {
        kb.all_docs()
    } else {
        let tags: Vec<&str> = params.tag.iter().map(String::as_str).collect();
        kb.search_by_tags(&tags)
    };

    let mut items: Vec<LlmKnowledgeListItem> = docs
        .into_iter()
        .map(|doc| LlmKnowledgeListItem {
            id: doc.id.clone(),
            title: doc.title.clone(),
            tags: doc.tags.clone(),
            related: doc.related.clone(),
            source_path: doc.source_path.clone(),
        })
        .collect();

    items.sort_by(|a, b| a.id.cmp(&b.id));
    Json(items)
}

pub async fn get_by_id(
    Path(id): Path<String>,
) -> Result<Json<LlmKnowledgeDetailResponse>, ApiError> {
    let kb = crate::shared::llm::knowledge_base::kb_read();
    let Some(doc) = kb.get(&id) else {
        return Err(axum::http::StatusCode::NOT_FOUND.into());
    };

    Ok(Json(LlmKnowledgeDetailResponse {
        id: doc.id.clone(),
        title: doc.title.clone(),
        tags: doc.tags.clone(),
        related: doc.related.clone(),
        source_path: doc.source_path.clone(),
        content: doc.content.clone(),
    }))
}
