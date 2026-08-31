use anyhow::Result;

use super::{builder, repository};

pub async fn get_by_id(id: &str) -> Result<Option<repository::Model>> {
    repository::get_by_id(id).await
}

#[allow(clippy::too_many_arguments)]
pub async fn list_with_filters(
    date_from: Option<String>,
    date_to: Option<String>,
    connection_mp_ref: Option<String>,
    layer: Option<String>,
    turnover_code: Option<String>,
    registrator_type: Option<String>,
    sort_by: Option<String>,
    sort_desc: bool,
    offset: Option<u64>,
    limit: Option<u64>,
) -> Result<Vec<repository::Model>> {
    repository::list_with_filters(
        date_from,
        date_to,
        connection_mp_ref,
        layer,
        turnover_code,
        registrator_type,
        sort_by,
        sort_desc,
        offset,
        limit,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub async fn count_with_filters(
    date_from: Option<String>,
    date_to: Option<String>,
    connection_mp_ref: Option<String>,
    layer: Option<String>,
    turnover_code: Option<String>,
    registrator_type: Option<String>,
) -> Result<u64> {
    repository::count_with_filters(
        date_from,
        date_to,
        connection_mp_ref,
        layer,
        turnover_code,
        registrator_type,
    )
    .await
}

pub async fn project_wb_finance_entry(
    entry: &crate::projections::p903_wb_finance_report::repository::Model,
    posting_id: &str,
) -> Result<()> {
    let result = builder::from_wb_finance_row(entry, posting_id)?;
    for model in result.turnovers {
        repository::upsert_entry(&model).await?;
    }
    crate::general_ledger::service::save_entries(&result.general_ledger_entries).await?;
    Ok(())
}

pub async fn remove_by_registrator_ref(registrator_ref: &str) -> Result<()> {
    repository::delete_by_registrator_ref(registrator_ref).await?;
    Ok(())
}

pub async fn rebuild_wb_range(date_from: &str, date_to: &str) -> Result<()> {
    repository::delete_by_date_range(date_from, date_to).await?;
    for finance_row in crate::projections::p903_wb_finance_report::repository::list_by_date_range(
        date_from, date_to,
    )
    .await?
    {
        let source_ref = builder::source_ref_from_model(&finance_row);
        crate::general_ledger::service::remove_by_registrator_ref(&source_ref).await?;
        remove_by_registrator_ref(&source_ref).await?;
        let posting_id = uuid::Uuid::new_v4().to_string();
        project_wb_finance_entry(&finance_row, &posting_id).await?;
    }
    Ok(())
}

/// Detail-строки для drill-down по ресурсу Главной книги.
pub struct GlDetail;

#[async_trait::async_trait]
impl crate::general_ledger::resource_detail::GlDetailSource for GlDetail {
    fn detail_table(&self) -> &'static str {
        "p910_mp_unlinked_turnovers"
    }

    async fn fetch(
        &self,
        gl: &crate::general_ledger::repository::Model,
    ) -> anyhow::Result<Vec<serde_json::Value>> {
        use super::repository::{Column, Entity};

        crate::general_ledger::resource_detail::fetch_linked_rows::<Entity>(
            gl,
            Column::GeneralLedgerRef,
            Column::RegistratorType,
            Column::RegistratorRef,
            Column::TurnoverCode,
        )
        .await
    }
}
