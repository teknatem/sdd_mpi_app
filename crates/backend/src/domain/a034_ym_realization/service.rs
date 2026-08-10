use anyhow::Result;
use contracts::domain::a034_ym_realization::aggregate::YmRealization;
use contracts::domain::common::AggregateId;
use uuid::Uuid;

use super::{posting, repository};
pub use repository::{YmRealizationListQuery, YmRealizationListResult, YmRealizationListRow};

pub async fn replace_for_period(
    connection_id: &str,
    date_from: &str,
    date_to: &str,
    documents: &[YmRealization],
) -> Result<usize> {
    repository::replace_for_period(connection_id, date_from, date_to, documents).await
}

/// Замена документов периода по кабинету и одной кампании (независимо от других
/// кампаний). См. `repository::replace_for_period_campaign`.
pub async fn replace_for_period_campaign(
    connection_id: &str,
    campaign_id: &str,
    date_from: &str,
    date_to: &str,
    documents: &[YmRealization],
) -> Result<usize> {
    repository::replace_for_period_campaign(
        connection_id,
        campaign_id,
        date_from,
        date_to,
        documents,
    )
    .await
}

pub async fn upsert_document(document: &YmRealization) -> Result<()> {
    repository::upsert_document(document).await
}

/// Сохранить документ и при `is_posted=true` сразу провести в GL (как a013/a026).
pub async fn store_document_with_auto_post(document: &YmRealization) -> Result<()> {
    repository::upsert_document(document).await?;
    if document.base.metadata.is_posted || document.is_posted {
        let id = Uuid::parse_str(&document.base.id.as_string())
            .map_err(|e| anyhow::anyhow!("Invalid document id: {}", e))?;
        if let Err(error) = posting::post_document(id).await {
            tracing::error!("a034 auto-post failed for {}: {}", id, error);
            return Err(error);
        }
    }
    Ok(())
}

pub async fn get_by_id(id: Uuid) -> Result<Option<YmRealization>> {
    repository::get_by_id(id).await
}

pub async fn list_paginated(query: YmRealizationListQuery) -> Result<YmRealizationListResult> {
    repository::list_sql(query).await
}

pub async fn post_document(id: Uuid) -> Result<()> {
    posting::post_document(id).await
}

pub async fn unpost_document(id: Uuid) -> Result<()> {
    posting::unpost_document(id).await
}
