use super::{builder, repository};
use anyhow::Result;
use contracts::domain::a009_ozon_returns::aggregate::OzonReturns;
use contracts::domain::a010_ozon_fbs_posting::aggregate::OzonFbsPosting;
use contracts::domain::a011_ozon_fbo_posting::aggregate::OzonFboPosting;
use contracts::domain::a013_ym_order::aggregate::YmOrder;
use contracts::domain::a014_ozon_transactions::aggregate::OzonTransactions;
use contracts::domain::a016_ym_returns::aggregate::YmReturn;
use uuid::Uuid;

pub async fn list(limit: Option<u64>) -> Result<Vec<repository::Model>> {
    repository::list(limit).await
}

pub async fn list_with_filters(
    date_from: Option<String>,
    date_to: Option<String>,
    connection_mp_ref: Option<String>,
    limit: Option<u64>,
) -> Result<Vec<repository::ModelWithCabinet>> {
    repository::list_with_filters(date_from, date_to, connection_mp_ref, limit).await
}

/// Проецировать OZON Transactions в Sales Data (P904)
pub async fn project_ozon_transactions(
    document: &OzonTransactions,
    document_id: Uuid,
) -> Result<()> {
    let entries = builder::from_ozon_transactions(document, &document_id.to_string()).await?;

    for entry in entries {
        repository::upsert_entry(&entry).await?;
    }

    tracing::info!(
        "Projected OZON Transactions document {} into Sales Data P904",
        document.header.operation_id
    );

    Ok(())
}

/// Проецировать YM Order в Sales Data (P904)
/// Только документы со статусом DELIVERED формируют проекции
pub async fn project_ym_order(document: &YmOrder, document_id: Uuid) -> Result<()> {
    let entries = builder::from_ym_order(document, &document_id.to_string()).await?;

    let entries_count = entries.len();
    for entry in entries {
        repository::upsert_entry(&entry).await?;
    }

    if entries_count > 0 {
        tracing::info!(
            "Projected YM Order document {} into Sales Data P904 ({} entries)",
            document.header.document_no,
            entries_count
        );
    }

    Ok(())
}

/// Проецировать YM Returns в Sales Data (P904)
/// Только документы со статусом REFUNDED формируют проекции
/// Заполняется только customer_out (с минусом)
pub async fn project_ym_returns(document: &YmReturn, document_id: Uuid) -> Result<()> {
    let entries = builder::from_ym_returns(document, &document_id.to_string()).await?;

    let entries_count = entries.len();
    for entry in entries {
        repository::upsert_entry(&entry).await?;
    }

    if entries_count > 0 {
        tracing::info!(
            "Projected YM Return document {} into Sales Data P904 ({} entries)",
            document.header.return_id,
            entries_count
        );
    }

    Ok(())
}

/// Проецировать OZON FBS Posting в Sales Data (P904)
/// Только документы со статусом DELIVERED формируют проекции
pub async fn project_ozon_fbs(document: &OzonFbsPosting, document_id: Uuid) -> Result<()> {
    let entries = builder::from_ozon_fbs(document, &document_id.to_string()).await?;

    let entries_count = entries.len();
    for entry in entries {
        repository::upsert_entry(&entry).await?;
    }

    if entries_count > 0 {
        tracing::info!(
            "Projected OZON FBS Posting document {} into Sales Data P904 ({} entries)",
            document.header.document_no,
            entries_count
        );
    }

    Ok(())
}

/// Проецировать OZON FBO Posting в Sales Data (P904)
pub async fn project_ozon_fbo(document: &OzonFboPosting, document_id: Uuid) -> Result<()> {
    let entries = builder::from_ozon_fbo(document, &document_id.to_string()).await?;

    let entries_count = entries.len();
    for entry in entries {
        repository::upsert_entry(&entry).await?;
    }

    if entries_count > 0 {
        tracing::info!(
            "Projected OZON FBO Posting document {} into Sales Data P904 ({} entries)",
            document.header.document_no,
            entries_count
        );
    }

    Ok(())
}

/// Проецировать OZON Returns в Sales Data (P904)
/// Возвраты формируют проекции с отрицательным customer_out
pub async fn project_ozon_returns(document: &OzonReturns, document_id: Uuid) -> Result<()> {
    let entries = builder::from_ozon_returns(document, &document_id.to_string()).await?;

    let entries_count = entries.len();
    for entry in entries {
        repository::upsert_entry(&entry).await?;
    }

    if entries_count > 0 {
        tracing::info!(
            "Projected OZON Return document {} into Sales Data P904 ({} entries)",
            document.return_id,
            entries_count
        );
    }

    Ok(())
}

/// Пересбор p904 за период для страницы перепроведения `u508`.
///
/// Сама проекция не пересобирается: перепроводятся документы-регистраторы,
/// на которые она ссылается, и строки появляются заново как побочный эффект
/// проведения. Типы приходят историческими (`WB_Sales`, `OZON_FBS`) —
/// их резолвит реестр регистраторов через `aliases`.
pub struct Repost;

#[async_trait::async_trait]
impl crate::usecases::u508_repost_documents::ProjectionRepost for Repost {
    fn key(&self) -> &'static str {
        "p904_sales_data"
    }

    fn option(&self) -> crate::usecases::u508_repost_documents::ProjectionOptionInfo {
        crate::usecases::u508_repost_documents::ProjectionOptionInfo {
            label: "p904 — Sales Data",
            description: "Перепроведение документов по registrator_ref из p904_sales_data",
        }
    }

    async fn rebuild(
        &self,
        ctx: &crate::usecases::u508_repost_documents::RepostContext<'_>,
    ) -> anyhow::Result<()> {
        let registrators: Vec<(String, String)> =
            repository::list_registrators_by_period(ctx.date_from, ctx.date_to)
                .await?
                .into_iter()
                .map(|row| (row.registrator_type, row.registrator_ref))
                .collect();
        ctx.repost_registrators(&registrators).await
    }
}
