use anyhow::Result;
use chrono::NaiveDate;
use uuid::Uuid;

use super::{builder, repository};

pub async fn get_by_id(id: &str) -> Result<Option<repository::Model>> {
    repository::get_by_id(id).await
}

#[allow(clippy::too_many_arguments)]
pub async fn list_with_filters(
    date_from: Option<String>,
    date_to: Option<String>,
    connection_mp_ref: Option<String>,
    order_key: Option<String>,
    line_key: Option<String>,
    layer: Option<String>,
    turnover_code: Option<String>,
    link_status: Option<String>,
    sort_by: Option<String>,
    sort_desc: bool,
    offset: Option<u64>,
    limit: Option<u64>,
) -> Result<Vec<repository::Model>> {
    repository::list_with_filters(
        date_from,
        date_to,
        connection_mp_ref,
        order_key,
        line_key,
        layer,
        turnover_code,
        link_status,
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
    order_key: Option<String>,
    line_key: Option<String>,
    layer: Option<String>,
    turnover_code: Option<String>,
    link_status: Option<String>,
) -> Result<u64> {
    repository::count_with_filters(
        date_from,
        date_to,
        connection_mp_ref,
        order_key,
        line_key,
        layer,
        turnover_code,
        link_status,
    )
    .await
}

pub async fn project_wb_order(
    document: &contracts::domain::a015_wb_orders::aggregate::WbOrders,
    document_id: Uuid,
) -> Result<()> {
    let document_id_str = document_id.to_string();
    for entry in builder::from_wb_order(document, &document_id_str)? {
        repository::upsert_entry(&entry).await?;
    }

    let related = repository::list_by_connection_and_line_key(
        &document.header.connection_id,
        &document.line.line_id,
    )
    .await?;
    for mut entry in related {
        builder::attach_order_context(&mut entry, document, &document_id_str);
        repository::save_entry(&entry).await?;
    }

    Ok(())
}

pub async fn project_wb_sales(
    document: &contracts::domain::a012_wb_sales::aggregate::WbSales,
    document_id: Uuid,
    posting_id: &str,
    prod_item_cost_total: Option<f64>,
) -> Result<()> {
    let document_id_str = document_id.to_string();
    let result =
        builder::from_wb_sales(document, &document_id_str, posting_id, prod_item_cost_total)?;
    for entry in result.turnovers {
        repository::upsert_entry(&entry).await?;
    }
    crate::general_ledger::service::save_entries(&result.general_ledger_entries).await?;
    Ok(())
}

pub async fn project_wb_finance_entry(
    entry: &crate::projections::p903_wb_finance_report::repository::Model,
    posting_id: &str,
) -> Result<()> {
    if !builder::is_finance_row_linked(entry) {
        return Ok(());
    }

    let result = builder::from_wb_finance_row(entry, posting_id)?;
    for model in result.turnovers {
        repository::upsert_entry(&model).await?;
    }
    crate::general_ledger::service::save_entries(&result.general_ledger_entries).await?;

    Ok(())
}

pub async fn remove_by_registrator_ref(registrator_ref: &str) -> Result<()> {
    let affected_groups = repository::list_link_groups_by_registrator_ref(registrator_ref).await?;
    if affected_groups.is_empty() {
        return Ok(());
    }

    repository::delete_many_by_registrator_ref(registrator_ref).await?;

    for group in affected_groups {
        repository::refresh_group_link_status(
            &group.connection_mp_ref,
            &group.line_event_key,
            &group.turnover_code,
        )
        .await?;
    }
    Ok(())
}

pub async fn remove_order_source(source_ref: &str) -> Result<()> {
    remove_by_registrator_ref(source_ref).await
}

pub async fn remove_oper_source(source_ref: &str) -> Result<()> {
    remove_by_registrator_ref(source_ref).await
}

pub async fn remove_fact_source(source_ref: &str) -> Result<()> {
    remove_by_registrator_ref(source_ref).await
}

pub async fn rebuild_wb_range(date_from: &str, date_to: &str) -> Result<()> {
    repository::delete_by_entry_date_range(date_from, date_to).await?;

    let parsed_from = NaiveDate::parse_from_str(date_from, "%Y-%m-%d")?;
    let parsed_to = NaiveDate::parse_from_str(date_to, "%Y-%m-%d")?;

    for document in crate::domain::a015_wb_orders::service::list_by_date_range(
        Some(parsed_from),
        Some(parsed_to),
    )
    .await?
    .into_iter()
    .filter(|document| document.is_posted)
    {
        crate::domain::a015_wb_orders::posting::post_document(document.base.id.value()).await?;
    }

    for document in crate::domain::a012_wb_sales::service::list_by_sale_date_range(
        Some(parsed_from),
        Some(parsed_to),
    )
    .await?
    .into_iter()
    .filter(|document| document.is_posted)
    {
        crate::domain::a012_wb_sales::posting::post_document(document.base.id.value()).await?;
    }

    Ok(())
}

/// Detail-строки для drill-down по ресурсу Главной книги.
pub struct GlDetail;

#[async_trait::async_trait]
impl crate::general_ledger::resource_detail::GlDetailSource for GlDetail {
    fn detail_table(&self) -> &'static str {
        "p909_mp_order_line_turnovers"
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
