use super::repository;
use anyhow::Result;
use contracts::domain::a016_ym_returns::aggregate::YmReturn;
use sea_orm::TransactionTrait;
use std::collections::HashMap;
use uuid::Uuid;

/// Движения возврата в воронке (p916). Строки a016 не несут ссылок на товар и когорту —
/// резолвим их здесь, где доступна БД: `shop_sku → a007/a004` и дата заказа по `order_id`.
async fn project_funnel(document: &YmReturn, id: Uuid) -> Result<()> {
    use crate::projections::p916_mp_sales_funnel_turnovers::{
        builder as funnel_builder, repository as funnel_repo,
    };

    let registrator_ref = id.to_string();

    let order_cohort_date = crate::domain::a013_ym_order::repository::order_date_by_document_no(
        &document.header.order_id.to_string(),
    )
    .await?
    .as_ref()
    .map(funnel_builder::msk_date_from_utc);

    let mut product_refs: HashMap<String, (Option<String>, Option<String>)> = HashMap::new();
    for line in &document.lines {
        if line.shop_sku.is_empty() || product_refs.contains_key(&line.shop_sku) {
            continue;
        }
        let refs = crate::domain::a007_marketplace_product::service::get_by_connection_and_sku(
            &document.header.connection_id,
            &line.shop_sku,
        )
        .await?
        .map(|product| {
            (
                Some(product.to_string_id()),
                product.nomenclature_ref.clone(),
            )
        })
        .unwrap_or((None, None));
        product_refs.insert(line.shop_sku.clone(), refs);
    }

    let rows = funnel_builder::from_ym_return(
        document,
        &registrator_ref,
        order_cohort_date,
        &product_refs,
    );

    let db = crate::shared::data::db::get_connection();
    let txn = db.begin().await?;
    funnel_repo::delete_by_registrator_with_conn(&txn, funnel_builder::REG_A016, &registrator_ref)
        .await?;
    funnel_repo::insert_many_with_conn(&txn, &rows).await?;
    txn.commit().await?;
    Ok(())
}

/// Провести документ (установить is_posted = true и создать проекции)
/// Возвраты YM со статусом REFUNDED формируют проекции в p904 (customer_out с минусом).
/// В воронку p916 попадают только возвраты после получения (`return_type = RETURN`);
/// невыкупы (`UNREDEEMED`) там не проводятся — это то же событие, что отказ из a013.
pub async fn post_document(id: Uuid) -> Result<()> {
    // Загрузить документ
    let mut document = repository::get_by_id(id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Document not found: {}", id))?;

    // Установить флаг is_posted
    document.is_posted = true;
    document.before_write();

    // Сохранить документ
    repository::upsert_document(&document).await?;

    // Удалить старые проекции (если были)
    crate::projections::p904_sales_data::repository::delete_by_registrator(&id.to_string()).await?;

    // Создать новые проекции (только для REFUNDED документов)
    crate::projections::p904_sales_data::service::project_ym_returns(&document, id).await?;

    project_funnel(&document, id).await?;

    tracing::info!(
        "Posted document a016 (YM Return): {}, refund_status: {}",
        id,
        document.state.refund_status
    );
    Ok(())
}

/// Отменить проведение документа (установить is_posted = false и удалить проекции)
pub async fn unpost_document(id: Uuid) -> Result<()> {
    // Загрузить документ
    let mut document = repository::get_by_id(id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Document not found: {}", id))?;

    // Снять флаг is_posted
    document.is_posted = false;
    document.before_write();

    // Сохранить документ
    repository::upsert_document(&document).await?;

    // Удалить проекции
    crate::projections::p904_sales_data::repository::delete_by_registrator(&id.to_string()).await?;
    crate::projections::p916_mp_sales_funnel_turnovers::repository::delete_by_registrator_ref(
        &id.to_string(),
    )
    .await?;

    tracing::info!("Unposted document a016 (YM Return): {}", id);
    Ok(())
}
