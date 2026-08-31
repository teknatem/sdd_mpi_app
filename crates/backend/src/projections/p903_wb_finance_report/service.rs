use anyhow::Result;
use chrono::NaiveDate;
use contracts::domain::common::AggregateId;
use sea_orm::{
    ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, QueryOrder, Set, TransactionTrait,
};
use serde::Serialize;
use std::collections::BTreeSet;

use crate::shared::data::db::get_connection;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReconcileResult {
    pub changed: bool,
    pub source_rows: usize,
    pub general_ledger_rows: usize,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
struct SnapshotRow {
    rr_dt: String,
    rrd_id: i64,
    connection_mp_ref: String,
    organization_ref: String,
    acquiring_fee: Option<f64>,
    acquiring_percent: Option<f64>,
    additional_payment: Option<f64>,
    bonus_type_name: Option<String>,
    commission_percent: Option<f64>,
    delivery_amount: Option<f64>,
    delivery_rub: Option<f64>,
    nm_id: Option<i64>,
    a004_nomenclature_ref: Option<String>,
    penalty: Option<f64>,
    ppvz_vw: Option<f64>,
    ppvz_vw_nds: Option<f64>,
    ppvz_sales_commission: Option<f64>,
    quantity: Option<i32>,
    rebill_logistic_cost: Option<f64>,
    retail_amount: Option<f64>,
    retail_price: Option<f64>,
    retail_price_withdisc_rub: Option<f64>,
    return_amount: Option<f64>,
    sa_name: Option<String>,
    storage_fee: Option<f64>,
    subject_name: Option<String>,
    supplier_oper_name: Option<String>,
    cashback_amount: Option<f64>,
    ppvz_for_pay: Option<f64>,
    ppvz_kvw_prc: Option<f64>,
    ppvz_kvw_prc_base: Option<f64>,
    srv_dbs: Option<i32>,
    srid: Option<String>,
}

type P903NaturalKey = i64;

fn snapshot_from_model(
    row: &crate::projections::p903_wb_finance_report::repository::Model,
) -> SnapshotRow {
    SnapshotRow {
        rr_dt: row.rr_dt.clone(),
        rrd_id: row.rrd_id,
        connection_mp_ref: row.connection_mp_ref.clone(),
        organization_ref: row.organization_ref.clone(),
        acquiring_fee: row.acquiring_fee,
        acquiring_percent: row.acquiring_percent,
        additional_payment: row.additional_payment,
        bonus_type_name: row.bonus_type_name.clone(),
        commission_percent: row.commission_percent,
        delivery_amount: row.delivery_amount,
        delivery_rub: row.delivery_rub,
        nm_id: row.nm_id,
        a004_nomenclature_ref: row.a004_nomenclature_ref.clone(),
        penalty: row.penalty,
        ppvz_vw: row.ppvz_vw,
        ppvz_vw_nds: row.ppvz_vw_nds,
        ppvz_sales_commission: row.ppvz_sales_commission,
        quantity: row.quantity,
        rebill_logistic_cost: row.rebill_logistic_cost,
        retail_amount: row.retail_amount,
        retail_price: row.retail_price,
        retail_price_withdisc_rub: row.retail_price_withdisc_rub,
        return_amount: row.return_amount,
        sa_name: row.sa_name.clone(),
        storage_fee: row.storage_fee,
        subject_name: row.subject_name.clone(),
        supplier_oper_name: row.supplier_oper_name.clone(),
        cashback_amount: row.cashback_amount,
        ppvz_for_pay: row.ppvz_for_pay,
        ppvz_kvw_prc: row.ppvz_kvw_prc,
        ppvz_kvw_prc_base: row.ppvz_kvw_prc_base,
        srv_dbs: row.srv_dbs,
        srid: row.srid.clone(),
    }
}

fn snapshot_from_entry(
    row: &crate::projections::p903_wb_finance_report::repository::WbFinanceReportEntry,
) -> SnapshotRow {
    SnapshotRow {
        rr_dt: row.rr_dt.format("%Y-%m-%d").to_string(),
        rrd_id: row.rrd_id,
        connection_mp_ref: row.connection_mp_ref.clone(),
        organization_ref: row.organization_ref.clone(),
        acquiring_fee: row.acquiring_fee,
        acquiring_percent: row.acquiring_percent,
        additional_payment: row.additional_payment,
        bonus_type_name: row.bonus_type_name.clone(),
        commission_percent: row.commission_percent,
        delivery_amount: row.delivery_amount,
        delivery_rub: row.delivery_rub,
        nm_id: row.nm_id,
        a004_nomenclature_ref: row.a004_nomenclature_ref.clone(),
        penalty: row.penalty,
        ppvz_vw: row.ppvz_vw,
        ppvz_vw_nds: row.ppvz_vw_nds,
        ppvz_sales_commission: row.ppvz_sales_commission,
        quantity: row.quantity,
        rebill_logistic_cost: row.rebill_logistic_cost,
        retail_amount: row.retail_amount,
        retail_price: row.retail_price,
        retail_price_withdisc_rub: row.retail_price_withdisc_rub,
        return_amount: row.return_amount,
        sa_name: row.sa_name.clone(),
        storage_fee: row.storage_fee,
        subject_name: row.subject_name.clone(),
        supplier_oper_name: row.supplier_oper_name.clone(),
        cashback_amount: row.cashback_amount,
        ppvz_for_pay: row.ppvz_for_pay,
        ppvz_kvw_prc: row.ppvz_kvw_prc,
        ppvz_kvw_prc_base: row.ppvz_kvw_prc_base,
        srv_dbs: row.srv_dbs,
        srid: row.srid.clone(),
    }
}

fn active_model_from_model(
    row: &crate::projections::p903_wb_finance_report::repository::Model,
) -> crate::projections::p903_wb_finance_report::repository::ActiveModel {
    crate::projections::p903_wb_finance_report::repository::ActiveModel {
        rr_dt: Set(row.rr_dt.clone()),
        rrd_id: Set(row.rrd_id),
        id: Set(row.id.clone()),
        source_row_ref: Set(row.source_row_ref.clone()),
        connection_mp_ref: Set(row.connection_mp_ref.clone()),
        organization_ref: Set(row.organization_ref.clone()),
        acquiring_fee: Set(row.acquiring_fee),
        acquiring_percent: Set(row.acquiring_percent),
        additional_payment: Set(row.additional_payment),
        bonus_type_name: Set(row.bonus_type_name.clone()),
        commission_percent: Set(row.commission_percent),
        delivery_amount: Set(row.delivery_amount),
        delivery_rub: Set(row.delivery_rub),
        nm_id: Set(row.nm_id),
        a004_nomenclature_ref: Set(row.a004_nomenclature_ref.clone()),
        marketplace_product_ref: Set(row.marketplace_product_ref.clone()),
        marketplace_order_ref: Set(row.marketplace_order_ref.clone()),
        penalty: Set(row.penalty),
        ppvz_vw: Set(row.ppvz_vw),
        ppvz_vw_nds: Set(row.ppvz_vw_nds),
        ppvz_sales_commission: Set(row.ppvz_sales_commission),
        quantity: Set(row.quantity),
        rebill_logistic_cost: Set(row.rebill_logistic_cost),
        retail_amount: Set(row.retail_amount),
        retail_price: Set(row.retail_price),
        retail_price_withdisc_rub: Set(row.retail_price_withdisc_rub),
        return_amount: Set(row.return_amount),
        sa_name: Set(row.sa_name.clone()),
        storage_fee: Set(row.storage_fee),
        subject_name: Set(row.subject_name.clone()),
        supplier_oper_name: Set(row.supplier_oper_name.clone()),
        cashback_amount: Set(row.cashback_amount),
        ppvz_for_pay: Set(row.ppvz_for_pay),
        ppvz_kvw_prc: Set(row.ppvz_kvw_prc),
        ppvz_kvw_prc_base: Set(row.ppvz_kvw_prc_base),
        srv_dbs: Set(row.srv_dbs),
        srid: Set(row.srid.clone()),
        loaded_at_utc: Set(row.loaded_at_utc.clone()),
        payload_version: Set(row.payload_version),
        extra: Set(row.extra.clone()),
    }
}

fn model_from_entry(
    row: &crate::projections::p903_wb_finance_report::repository::WbFinanceReportEntry,
    id: String,
    loaded_at_utc: &str,
) -> crate::projections::p903_wb_finance_report::repository::Model {
    crate::projections::p903_wb_finance_report::repository::Model {
        rr_dt: row.rr_dt.format("%Y-%m-%d").to_string(),
        rrd_id: row.rrd_id,
        id,
        source_row_ref: row.source_row_ref.clone(),
        connection_mp_ref: row.connection_mp_ref.clone(),
        organization_ref: row.organization_ref.clone(),
        acquiring_fee: row.acquiring_fee,
        acquiring_percent: row.acquiring_percent,
        additional_payment: row.additional_payment,
        bonus_type_name: row.bonus_type_name.clone(),
        commission_percent: row.commission_percent,
        delivery_amount: row.delivery_amount,
        delivery_rub: row.delivery_rub,
        nm_id: row.nm_id,
        a004_nomenclature_ref: row.a004_nomenclature_ref.clone(),
        marketplace_product_ref: None,
        marketplace_order_ref: None,
        penalty: row.penalty,
        ppvz_vw: row.ppvz_vw,
        ppvz_vw_nds: row.ppvz_vw_nds,
        ppvz_sales_commission: row.ppvz_sales_commission,
        quantity: row.quantity,
        rebill_logistic_cost: row.rebill_logistic_cost,
        retail_amount: row.retail_amount,
        retail_price: row.retail_price,
        retail_price_withdisc_rub: row.retail_price_withdisc_rub,
        return_amount: row.return_amount,
        sa_name: row.sa_name.clone(),
        storage_fee: row.storage_fee,
        subject_name: row.subject_name.clone(),
        supplier_oper_name: row.supplier_oper_name.clone(),
        cashback_amount: row.cashback_amount,
        ppvz_for_pay: row.ppvz_for_pay,
        ppvz_kvw_prc: row.ppvz_kvw_prc,
        ppvz_kvw_prc_base: row.ppvz_kvw_prc_base,
        srv_dbs: row.srv_dbs,
        srid: row.srid.clone(),
        loaded_at_utc: loaded_at_utc.to_string(),
        payload_version: row.payload_version,
        extra: row.extra.clone(),
    }
}

async fn load_day_models(
    connection_mp_ref: &str,
    date: NaiveDate,
) -> Result<Vec<crate::projections::p903_wb_finance_report::repository::Model>> {
    let db = get_connection();
    let date_str = date.format("%Y-%m-%d").to_string();

    Ok(
        crate::projections::p903_wb_finance_report::repository::Entity::find()
            .filter(
                crate::projections::p903_wb_finance_report::repository::Column::RrDt.eq(&date_str),
            )
            .filter(
                crate::projections::p903_wb_finance_report::repository::Column::ConnectionMpRef
                    .eq(connection_mp_ref),
            )
            .order_by_asc(crate::projections::p903_wb_finance_report::repository::Column::RrdId)
            .all(db)
            .await?,
    )
}

/// Строит и сохраняет GL-проводки и зеркальные строки p914 (слой fina) для
/// набора строк p903. GL и p914 формируются синхронно построчно, поэтому
/// гарантированно совпадают по сумме, дате транзакции и измерениям.
/// Возвращает количество сохранённых GL-проводок.
async fn save_general_ledger_and_finance_turnovers<C: ConnectionTrait>(
    db: &C,
    models: &[crate::projections::p903_wb_finance_report::repository::Model],
) -> Result<usize> {
    use crate::projections::p903_wb_finance_report::general_ledger_builder;

    let mut gl_count = 0usize;
    for model in models {
        let gl_entries = general_ledger_builder::build_general_ledger_entries(model, "")?;
        for entry in &gl_entries {
            crate::general_ledger::repository::save_entry_with_conn(db, entry).await?;
        }

        let mut finance_turnovers =
            general_ledger_builder::build_finance_turnover_entries(model, &gl_entries);
        if !finance_turnovers.is_empty() {
            enrich_wb_finance_turnovers(model, &mut finance_turnovers).await?;
        }
        for turnover in &finance_turnovers {
            crate::projections::p914_mp_finance_turnovers::repository::save_entry_with_conn(
                db, turnover,
            )
            .await?;
        }

        gl_count += gl_entries.len();
    }
    Ok(gl_count)
}

/// Уточняет `fulfillment_type` строк p914 из склада заказа a015 (точнее, чем
/// базовый srv_dbs). `marketplace_product_ref`/`order_ref` уже скопированы из
/// строки p903 в билдере, поэтому здесь не резолвятся.
async fn enrich_wb_finance_turnovers(
    model: &crate::projections::p903_wb_finance_report::repository::Model,
    turnovers: &mut [crate::projections::p914_mp_finance_turnovers::repository::Model],
) -> Result<()> {
    let Some(order_ref) = model
        .marketplace_order_ref
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
    else {
        return Ok(());
    };

    let order_uuid = match uuid::Uuid::parse_str(order_ref) {
        Ok(value) => value,
        Err(_) => return Ok(()),
    };

    let Some(order) = crate::domain::a015_wb_orders::repository::get_by_id(order_uuid).await?
    else {
        return Ok(());
    };

    let Some(warehouse_fulfillment) = order
        .warehouse
        .warehouse_type
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(|v| v.to_string())
    else {
        return Ok(());
    };

    for turnover in turnovers.iter_mut() {
        turnover.fulfillment_type = Some(warehouse_fulfillment.clone());
    }

    Ok(())
}

fn gl_registrator_aliases_from_models(
    models: &[crate::projections::p903_wb_finance_report::repository::Model],
) -> Vec<String> {
    let mut aliases = BTreeSet::new();
    for item in models {
        aliases.insert(item.id.clone());
        aliases.insert(item.source_row_ref.clone());
        aliases.insert(format!("p903:{}:{}", item.rr_dt, item.rrd_id));
    }
    aliases.into_iter().collect()
}

fn existing_id_map(
    models: &[crate::projections::p903_wb_finance_report::repository::Model],
) -> std::collections::HashMap<P903NaturalKey, String> {
    models
        .iter()
        .map(|item| (item.rrd_id, item.id.clone()))
        .collect()
}

pub async fn reconcile_day(
    connection_mp_ref: &str,
    date: NaiveDate,
    entries: &[crate::projections::p903_wb_finance_report::repository::WbFinanceReportEntry],
) -> Result<ReconcileResult> {
    let db = get_connection();
    let date_str = date.format("%Y-%m-%d").to_string();

    let existing = load_day_models(connection_mp_ref, date).await?;

    let current_snapshot: Vec<SnapshotRow> = existing.iter().map(snapshot_from_model).collect();
    let next_snapshot: Vec<SnapshotRow> = entries.iter().map(snapshot_from_entry).collect();
    if current_snapshot == next_snapshot {
        return Ok(ReconcileResult {
            changed: false,
            source_rows: existing.len(),
            general_ledger_rows: crate::general_ledger::repository::count_by_registrator_refs(
                &gl_registrator_aliases_from_models(&existing),
            )
            .await?
            .values()
            .copied()
            .sum(),
        });
    }

    let txn = db.begin().await?;

    let registrator_refs = gl_registrator_aliases_from_models(&existing);
    crate::general_ledger::repository::delete_by_registrator_refs_with_conn(
        &txn,
        &registrator_refs,
    )
    .await?;
    crate::projections::p914_mp_finance_turnovers::repository::delete_by_registrator_refs_with_conn(
        &txn,
        &registrator_refs,
    )
    .await?;

    crate::projections::p903_wb_finance_report::repository::Entity::delete_many()
        .filter(crate::projections::p903_wb_finance_report::repository::Column::RrDt.eq(&date_str))
        .filter(
            crate::projections::p903_wb_finance_report::repository::Column::ConnectionMpRef
                .eq(connection_mp_ref),
        )
        .exec(&txn)
        .await?;

    let preserved_ids = existing_id_map(&existing);
    let preserved_marketplace_refs: std::collections::HashMap<
        P903NaturalKey,
        (Option<String>, Option<String>),
    > = existing
        .iter()
        .map(|item| {
            (
                item.rrd_id,
                (
                    item.marketplace_product_ref.clone(),
                    item.marketplace_order_ref.clone(),
                ),
            )
        })
        .collect();
    let loaded_at_utc = chrono::Utc::now().to_rfc3339();
    let mut models = entries
        .iter()
        .map(|item| {
            let key = item.rrd_id;
            let id = preserved_ids.get(&key).cloned().unwrap_or_else(
                crate::projections::p903_wb_finance_report::repository::make_entry_id,
            );
            let mut model = model_from_entry(item, id, &loaded_at_utc);
            // Сохраняем уже заполненные производные ссылки при пересборке дня.
            if let Some((mp_ref, order_ref)) = preserved_marketplace_refs.get(&key) {
                model.marketplace_product_ref = mp_ref.clone();
                model.marketplace_order_ref = order_ref.clone();
            }
            model
        })
        .collect::<Vec<_>>();

    // Первый этап проведения: дозаполнить производные ссылки, если пусто.
    for model in &mut models {
        resolve_and_set_marketplace_refs(model).await?;
    }

    for model in &models {
        crate::projections::p903_wb_finance_report::repository::Entity::insert(
            active_model_from_model(model),
        )
        .exec(&txn)
        .await?;
    }

    let general_ledger_rows = save_general_ledger_and_finance_turnovers(&txn, &models).await?;

    txn.commit().await?;

    Ok(ReconcileResult {
        changed: true,
        source_rows: models.len(),
        general_ledger_rows,
    })
}

pub async fn rebuild_day_from_existing(
    connection_mp_ref: &str,
    date: NaiveDate,
) -> Result<ReconcileResult> {
    let db = get_connection();
    let mut existing = load_day_models(connection_mp_ref, date).await?;
    if existing.is_empty() {
        return Ok(ReconcileResult {
            changed: false,
            source_rows: 0,
            general_ledger_rows: 0,
        });
    }

    // Перед перерасчётом GL подтянуть актуальное a004_nomenclature_ref в строки p903
    // из текущего состояния a007 — закрывает «mismatched_document_link» при перепроведении.
    refresh_a004_links_from_a007(connection_mp_ref, &mut existing).await?;
    // Первый этап проведения: дозаполнить производные ссылки (если пусто) и сохранить.
    refresh_marketplace_refs(&mut existing).await?;

    let txn = db.begin().await?;
    let registrator_refs = gl_registrator_aliases_from_models(&existing);
    crate::general_ledger::repository::delete_by_registrator_refs_with_conn(
        &txn,
        &registrator_refs,
    )
    .await?;
    crate::projections::p914_mp_finance_turnovers::repository::delete_by_registrator_refs_with_conn(
        &txn,
        &registrator_refs,
    )
    .await?;

    let general_ledger_rows = save_general_ledger_and_finance_turnovers(&txn, &existing).await?;

    txn.commit().await?;

    Ok(ReconcileResult {
        changed: true,
        source_rows: existing.len(),
        general_ledger_rows,
    })
}

/// Для каждой строки p903 перезаписывает `a004_nomenclature_ref` актуальным значением
/// из a007 (через единый канонический резолвер). Безусловно: если a007 указывает на
/// другую номенклатуру или None — это и попадёт в строку.
async fn refresh_a004_links_from_a007(
    connection_mp_ref: &str,
    models: &mut [crate::projections::p903_wb_finance_report::repository::Model],
) -> Result<()> {
    use crate::projections::p903_wb_finance_report::repository::ActiveModel;

    let db = get_connection();
    for model in models.iter_mut() {
        let Some(nm_id) = model.nm_id else { continue };
        let new_ref =
            crate::domain::a007_marketplace_product::service::resolve_wb_nomenclature_ref(
                connection_mp_ref,
                nm_id,
                model.sa_name.as_deref(),
            )
            .await?;
        if model.a004_nomenclature_ref == new_ref {
            continue;
        }
        let am = ActiveModel {
            id: Set(model.id.clone()),
            a004_nomenclature_ref: Set(new_ref.clone()),
            ..Default::default()
        };
        <crate::projections::p903_wb_finance_report::repository::Entity as EntityTrait>::update(am)
            .exec(db)
            .await?;
        model.a004_nomenclature_ref = new_ref;
    }
    Ok(())
}

/// Резолвит и заполняет (только если ещё пусто) производные ссылки строки p903:
/// `marketplace_product_ref` (a007 по nm_id/артикулу) и `marketplace_order_ref`
/// (a015 по srid). В нормальной ситуации значения уже заполнены и копируются
/// в p914 без обращения к источникам. Возвращает `true`, если что-то заполнено.
async fn resolve_and_set_marketplace_refs(
    model: &mut crate::projections::p903_wb_finance_report::repository::Model,
) -> Result<bool> {
    let mut changed = false;

    if model.marketplace_product_ref.is_none() {
        if let Some(nm_id) = model.nm_id {
            if let Some(mp_ref) =
                crate::domain::a007_marketplace_product::service::resolve_marketplace_product_ref(
                    &model.connection_mp_ref,
                    &nm_id.to_string(),
                    model.sa_name.as_deref(),
                )
                .await?
            {
                model.marketplace_product_ref = Some(mp_ref);
                changed = true;
            }
        }
    }

    if model.marketplace_order_ref.is_none() {
        if let Some(srid) = model
            .srid
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty())
        {
            if let Some(order) =
                crate::domain::a015_wb_orders::repository::get_by_document_no(srid).await?
            {
                model.marketplace_order_ref = Some(order.base.id.as_string());
                changed = true;
            }
        }
    }

    Ok(changed)
}

/// Дозаполняет производные ссылки в существующих строках p903 и сохраняет
/// изменения в БД. Используется на этапе перепроведения.
async fn refresh_marketplace_refs(
    models: &mut [crate::projections::p903_wb_finance_report::repository::Model],
) -> Result<()> {
    use crate::projections::p903_wb_finance_report::repository::ActiveModel;

    let db = get_connection();
    for model in models.iter_mut() {
        if resolve_and_set_marketplace_refs(model).await? {
            let am = ActiveModel {
                id: Set(model.id.clone()),
                marketplace_product_ref: Set(model.marketplace_product_ref.clone()),
                marketplace_order_ref: Set(model.marketplace_order_ref.clone()),
                ..Default::default()
            };
            <crate::projections::p903_wb_finance_report::repository::Entity as EntityTrait>::update(
                am,
            )
            .exec(db)
            .await?;
        }
    }
    Ok(())
}

pub async fn rebuild_range_from_existing(date_from: &str, date_to: &str) -> Result<usize> {
    let parsed_from = NaiveDate::parse_from_str(date_from, "%Y-%m-%d")?;
    let parsed_to = NaiveDate::parse_from_str(date_to, "%Y-%m-%d")?;
    let rows = crate::projections::p903_wb_finance_report::repository::list_by_date_range(
        date_from, date_to,
    )
    .await?;

    let mut day_keys = rows
        .into_iter()
        .map(|row| (row.connection_mp_ref, row.rr_dt))
        .collect::<Vec<_>>();
    day_keys.sort();
    day_keys.dedup();

    for (connection_mp_ref, date_str) in &day_keys {
        let date = NaiveDate::parse_from_str(date_str, "%Y-%m-%d")?;
        if date < parsed_from || date > parsed_to {
            continue;
        }
        rebuild_day_from_existing(connection_mp_ref, date).await?;
    }

    Ok(day_keys.len())
}

/// Пересбор p903 за период для страницы перепроведения `u508`.
///
/// Один вызов на весь период, а не по строкам: `rebuild_range_from_existing`
/// пересобирает GL-проводки диапазона целиком, и дробить его на элементы
/// прогресса нечем — отсюда шкала из одного шага.
pub struct Repost;

#[async_trait::async_trait]
impl crate::usecases::u508_repost_documents::ProjectionRepost for Repost {
    fn key(&self) -> &'static str {
        "p903_wb_finance_report"
    }

    fn option(&self) -> crate::usecases::u508_repost_documents::ProjectionOptionInfo {
        crate::usecases::u508_repost_documents::ProjectionOptionInfo {
            label: "p903 — WB Finance Report",
            description:
                "Локальная пересборка general ledger по сохранённым строкам p903_wb_finance_report",
        }
    }

    async fn rebuild(
        &self,
        ctx: &crate::usecases::u508_repost_documents::RepostContext<'_>,
    ) -> anyhow::Result<()> {
        const STEP: &str = "Rebuilding p903 general ledger";
        ctx.tracker.set_total(ctx.session_id, 1);
        ctx.tracker
            .update_progress(ctx.session_id, 0, 0, Some(STEP.to_string()));
        rebuild_range_from_existing(ctx.date_from, ctx.date_to).await?;
        ctx.tracker
            .update_progress(ctx.session_id, 1, 1, Some(STEP.to_string()));
        Ok(())
    }
}

/// Detail-строки для drill-down по ресурсу Главной книги.
///
/// В отличие от проекций, связанных через `general_ledger_ref`, p903 —
/// внешне связанная (`ExternalLinked`): проводка ссылается на её строку
/// через `registrator_ref`, и форма ссылки исторически трёхвариантная —
/// голый id, `p903:<source_row_ref>` и `p903:<rr_dt>:<rrd_id>`.
pub struct GlDetail;

#[async_trait::async_trait]
impl crate::general_ledger::resource_detail::GlDetailSource for GlDetail {
    fn detail_table(&self) -> &'static str {
        "p903_wb_finance_report"
    }

    async fn fetch(
        &self,
        gl: &crate::general_ledger::repository::Model,
    ) -> anyhow::Result<Vec<serde_json::Value>> {
        use super::repository::{Column, Entity};
        use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};

        let conn = crate::shared::data::db::get_connection();
        let registrator_ref = gl.registrator_ref.as_str();

        if !registrator_ref.starts_with("p903:") {
            let row = Entity::find_by_id(registrator_ref.to_string())
                .into_json()
                .one(conn)
                .await?;
            return Ok(row.into_iter().collect());
        }

        let parts: Vec<&str> = registrator_ref.splitn(3, ':').collect();
        if parts.len() == 2 {
            return Ok(Entity::find()
                .filter(Column::SourceRowRef.eq(registrator_ref))
                .into_json()
                .all(conn)
                .await?);
        }

        if parts.len() == 3 {
            let rr_dt = parts[1].to_string();
            let rrd_id: i64 = parts[2].parse().map_err(|err| {
                anyhow::anyhow!("Invalid rrd_id in registrator_ref '{registrator_ref}': {err}")
            })?;
            return Ok(Entity::find()
                .filter(Column::RrDt.eq(rr_dt))
                .filter(Column::RrdId.eq(rrd_id))
                .into_json()
                .all(conn)
                .await?);
        }

        Ok(Vec::new())
    }
}
