use super::repository;
use anyhow::Result;
use chrono::Utc;
use contracts::shared::analytics::TurnoverLayer;
use sea_orm::TransactionTrait;
use std::collections::HashSet;
use uuid::Uuid;

use crate::general_ledger::repository::Model as GeneralLedgerModel;
use crate::general_ledger::turnover_registry::get_turnover_class;
use crate::shared::data::db::get_connection;
use crate::shared::marketplaces::wildberries::datetime::wb_business_date_str;

const REGISTRATOR_TYPE: &str = "a012_wb_sales";
const TURNOVER_CODE_EXPENSE: &str = "advert_clicks_order_expense";
const RESOURCE_TABLE_P913: &str = "p913_wb_advert_order_attr";

fn now_str() -> String {
    Utc::now().to_rfc3339()
}

fn to_gl_advert_expense(
    gl_id: &str,
    entry_date: &str,
    connection_mp_ref: Option<String>,
    registrator_ref: &str,
    amount: f64,
) -> GeneralLedgerModel {
    let class = get_turnover_class(TURNOVER_CODE_EXPENSE)
        .unwrap_or_else(|| panic!("Missing turnover class for {}", TURNOVER_CODE_EXPENSE));

    GeneralLedgerModel {
        id: gl_id.to_string(),
        entry_date: entry_date.to_string(),
        layer: TurnoverLayer::Oper.as_str().to_string(),
        entity: None,
        connection_mp_ref,
        registrator_type: REGISTRATOR_TYPE.to_string(),
        registrator_ref: registrator_ref.to_string(),
        order_id: None,
        debit_account: class.debit_account.to_string(),
        credit_account: class.credit_account.to_string(),
        amount,
        qty: None,
        turnover_code: TURNOVER_CODE_EXPENSE.to_string(),
        resource_table: RESOURCE_TABLE_P913.to_string(),
        resource_field: "amount".to_string(),
        resource_sign: 1,
        created_at: now_str(),
    }
}

pub async fn post_document(id: Uuid) -> Result<()> {
    let mut cache = super::service::PostingPreparationCache::default();
    post_document_with_cache(id, &mut cache).await
}

pub async fn post_document_with_cache(
    id: Uuid,
    cache: &mut super::service::PostingPreparationCache,
) -> Result<()> {
    let mut document = repository::get_by_id(id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Document not found: {}", id))?;

    let prepare_changed =
        super::service::prepare_document_for_posting_cached(&mut document, cache).await?;

    let next_is_customer_return = document.state.event_type.eq_ignore_ascii_case("return")
        || document.line.finished_price.unwrap_or(0.0) < 0.0;
    let mut should_persist_document =
        prepare_changed || !document.is_posted || !document.base.metadata.is_posted;

    if document.is_customer_return != next_is_customer_return {
        document.is_customer_return = next_is_customer_return;
        should_persist_document = true;
    }
    if !document.is_posted {
        document.is_posted = true;
        should_persist_document = true;
    }
    if !document.base.metadata.is_posted {
        document.base.metadata.is_posted = true;
        should_persist_document = true;
    }

    let prod_cost_resolution = super::service::resolve_prod_cost_cached(&document, cache).await?;
    should_persist_document |=
        super::service::apply_prod_cost_diagnostics(&mut document, &prod_cost_resolution);

    let prod_item_cost_total = prod_cost_resolution.total;

    let registrator_ref = id.to_string();
    let p909_registrator_ref = format!("a012:{id}");
    let id_str = id.to_string();

    // === ФАЗА 1: подготовка (только чтения), вне транзакции ===
    // Все проекционные строки собираются в памяти ДО открытия транзакции, чтобы транзакция
    // держала write-lock SQLite минимально — только на время самих записей.
    if should_persist_document {
        document.before_write();
    }

    let p900_entry =
        crate::projections::p900_mp_sales_register::builder::from_wb_sales(&document, &id_str)
            .await?;
    let p904_entries =
        crate::projections::p904_sales_data::builder::from_wb_sales_lines(&document, &id_str)
            .await?;
    let p909_result = crate::projections::p909_mp_order_line_turnovers::builder::from_wb_sales(
        &document,
        &id_str,
        &registrator_ref,
        prod_item_cost_total,
    )?;

    // Дата заказа по srid (a015) для когортной привязки выкупа/возврата в p916 — чтение вне
    // транзакции. None → воронка фолбэком возьмёт дату продажи (заказ не найден в a015).
    let funnel_order_cohort_date =
        crate::domain::a015_wb_orders::repository::order_date_by_srid(&document.header.document_no)
            .await?
            .map(|dt| {
                crate::projections::p916_mp_sales_funnel_turnovers::builder::msk_date_from_utc(&dt)
            });

    // Группы p909 для пересчёта link_status = (группы, имевшие строки до перепроведения)
    // ∪ (группы, вставляемые сейчас). Первое читаем до удаления, чтобы у оставшихся строк
    // групп, которые в этот раз не создаются, статус тоже корректно пересчитался.
    let mut p909_groups: HashSet<(String, String, String)> = HashSet::new();
    for group in
        crate::projections::p909_mp_order_line_turnovers::repository::list_link_groups_by_registrator_ref(
            &p909_registrator_ref,
        )
        .await?
    {
        p909_groups.insert((
            group.connection_mp_ref,
            group.line_event_key,
            group.turnover_code,
        ));
    }
    for turnover in &p909_result.turnovers {
        p909_groups.insert((
            turnover.connection_mp_ref.clone(),
            turnover.line_event_key.clone(),
            turnover.turnover_code.clone(),
        ));
    }

    // Phase 2 p913 (advert expense): только при реализации (не возврат). Резерв читаем здесь.
    let mut p913_gl_entry: Option<GeneralLedgerModel> = None;
    let mut p913_expense_entries: Vec<
        crate::projections::p913_wb_advert_order_attr::repository::Model,
    > = Vec::new();
    if !document.is_customer_return {
        let srid = &document.header.document_no;
        let reserve_rows =
            crate::projections::p913_wb_advert_order_attr::repository::list_by_order_key_and_turnover(
                srid, "advert_clicks_order_accrual",
            )
            .await?;
        if !reserve_rows.is_empty() {
            let total_amount: f64 = reserve_rows.iter().map(|r| r.amount).sum();
            let entry_date = wb_business_date_str(&document.state.sale_dt);
            let gl_id = Uuid::new_v4().to_string();
            p913_gl_entry = Some(to_gl_advert_expense(
                &gl_id,
                &entry_date,
                Some(document.header.connection_id.clone()),
                &registrator_ref,
                total_amount,
            ));
            let sale_finished_price = document.line.finished_price.unwrap_or(0.0);
            p913_expense_entries =
                crate::projections::p913_wb_advert_order_attr::service::build_expense_entries(
                    id,
                    srid,
                    &reserve_rows,
                    sale_finished_price,
                    &entry_date,
                    &gl_id,
                );
        }
    }

    // === ФАЗА 2: запись (одна транзакция) ===
    let txn = get_connection().begin().await?;

    if should_persist_document {
        repository::upsert_document_knowing_existence_with_conn(&txn, &document, Some(id)).await?;
    }

    // Удаляем прежний след документа во всех проекциях.
    crate::projections::p900_mp_sales_register::repository::delete_by_registrator_with_conn(
        &txn,
        &registrator_ref,
    )
    .await?;
    crate::projections::p904_sales_data::repository::delete_by_registrator_with_conn(
        &txn,
        &registrator_ref,
    )
    .await?;
    crate::projections::p909_mp_order_line_turnovers::repository::delete_many_by_registrator_ref_with_conn(
        &txn,
        &p909_registrator_ref,
    )
    .await?;
    crate::general_ledger::repository::delete_by_registrator_with_conn(
        &txn,
        "a012_wb_sales",
        &registrator_ref,
    )
    .await?;
    crate::projections::p913_wb_advert_order_attr::repository::delete_by_registrator_with_conn(
        &txn,
        REGISTRATOR_TYPE,
        &registrator_ref,
    )
    .await?;
    crate::projections::p916_mp_sales_funnel_turnovers::repository::delete_by_registrator_with_conn(
        &txn,
        crate::projections::p916_mp_sales_funnel_turnovers::builder::REG_A012,
        &registrator_ref,
    )
    .await?;

    // Вставляем заново собранные строки.
    crate::projections::p900_mp_sales_register::repository::upsert_entry_with_conn(
        &txn,
        &p900_entry,
    )
    .await?;
    for entry in &p904_entries {
        crate::projections::p904_sales_data::repository::upsert_entry_with_conn(&txn, entry)
            .await?;
    }
    for turnover in &p909_result.turnovers {
        crate::projections::p909_mp_order_line_turnovers::repository::insert_entry_raw_with_conn(
            &txn, turnover,
        )
        .await?;
    }
    crate::general_ledger::repository::insert_entries_bulk_with_conn(
        &txn,
        &p909_result.general_ledger_entries,
    )
    .await?;
    for (connection_mp_ref, line_event_key, turnover_code) in &p909_groups {
        crate::projections::p909_mp_order_line_turnovers::repository::refresh_group_link_status_with_conn(
            &txn,
            connection_mp_ref,
            line_event_key,
            turnover_code,
        )
        .await?;
    }

    if let Some(gl_entry) = &p913_gl_entry {
        crate::general_ledger::repository::save_entry_with_conn(&txn, gl_entry).await?;
    }
    for entry in &p913_expense_entries {
        crate::projections::p913_wb_advert_order_attr::repository::save_entry_with_conn(
            &txn, entry,
        )
        .await?;
    }

    // Стадия 2 воронки p916: выкуп/возврат из a012. Когорта — по дате заказа (a015 по srid),
    // фолбэк на дату продажи, если заказ не найден.
    let p916_rows = crate::projections::p916_mp_sales_funnel_turnovers::builder::from_wb_sales(
        &document,
        &registrator_ref,
        funnel_order_cohort_date,
    );
    crate::projections::p916_mp_sales_funnel_turnovers::repository::insert_many_with_conn(
        &txn, &p916_rows,
    )
    .await?;

    txn.commit().await?;

    // Сигнал клиентам обновить открытые списки a012. Бьём внутри _with_cache, чтобы
    // покрыть все пути проведения (одиночное, batch, repost u508, day-close a033).
    super::change_token::TOKEN.bump();

    Ok(())
}

pub async fn unpost_document(id: Uuid) -> Result<()> {
    let mut document = repository::get_by_id(id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Document not found: {}", id))?;

    // Вариант b: на unpost принудительно обновляем все ссылочные реквизиты, чтобы непроведённый
    // документ оставался корректным для отображения, а инвариант unpost + post = полное
    // обновление сохранялся (на последующем post реквизиты уже заполнены и пропускаются).
    let mut cache = super::service::PostingPreparationCache::default();
    super::service::force_refresh_requisites_cached(&mut document, &mut cache).await?;
    let prod_cost_resolution =
        super::service::resolve_prod_cost_cached(&document, &mut cache).await?;
    super::service::apply_prod_cost_diagnostics(&mut document, &prod_cost_resolution);

    document.is_posted = false;
    document.base.metadata.is_posted = false;
    document.before_write();

    let registrator_ref = id.to_string();
    let p909_registrator_ref = format!("a012:{id}");

    // Группы p909, у которых исчезнут строки этого документа — читаем ДО удаления,
    // чтобы потом пересчитать link_status у оставшихся строк этих групп.
    let mut p909_groups: HashSet<(String, String, String)> = HashSet::new();
    for group in
        crate::projections::p909_mp_order_line_turnovers::repository::list_link_groups_by_registrator_ref(
            &p909_registrator_ref,
        )
        .await?
    {
        p909_groups.insert((
            group.connection_mp_ref,
            group.line_event_key,
            group.turnover_code,
        ));
    }

    // Одна транзакция: обновление документа + снятие всех проекций + пересчёт групп.
    let txn = get_connection().begin().await?;

    repository::upsert_document_knowing_existence_with_conn(&txn, &document, Some(id)).await?;

    crate::projections::p900_mp_sales_register::repository::delete_by_registrator_with_conn(
        &txn,
        &registrator_ref,
    )
    .await?;
    crate::projections::p904_sales_data::repository::delete_by_registrator_with_conn(
        &txn,
        &registrator_ref,
    )
    .await?;
    crate::projections::p909_mp_order_line_turnovers::repository::delete_many_by_registrator_ref_with_conn(
        &txn,
        &p909_registrator_ref,
    )
    .await?;
    crate::general_ledger::repository::delete_by_registrator_with_conn(
        &txn,
        "a012_wb_sales",
        &registrator_ref,
    )
    .await?;
    crate::projections::p913_wb_advert_order_attr::repository::delete_by_registrator_with_conn(
        &txn,
        REGISTRATOR_TYPE,
        &registrator_ref,
    )
    .await?;
    crate::projections::p916_mp_sales_funnel_turnovers::repository::delete_by_registrator_with_conn(
        &txn,
        crate::projections::p916_mp_sales_funnel_turnovers::builder::REG_A012,
        &registrator_ref,
    )
    .await?;

    for (connection_mp_ref, line_event_key, turnover_code) in &p909_groups {
        crate::projections::p909_mp_order_line_turnovers::repository::refresh_group_link_status_with_conn(
            &txn,
            connection_mp_ref,
            line_event_key,
            turnover_code,
        )
        .await?;
    }

    txn.commit().await?;

    // Сигнал клиентам обновить открытые списки a012.
    super::change_token::TOKEN.bump();

    Ok(())
}
