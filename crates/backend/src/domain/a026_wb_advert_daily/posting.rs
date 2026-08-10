use anyhow::Result;
use chrono::Utc;
use contracts::domain::a015_wb_orders::aggregate::WbOrders;
use contracts::domain::a026_wb_advert_daily::aggregate::{
    WbAdvertDaily, WbAdvertFoundOrder, WbAdvertLinkedOrdersByNm,
};
use contracts::domain::common::AggregateId;
use contracts::shared::analytics::TurnoverLayer;
use sea_orm::TransactionTrait;
use std::collections::HashMap;
use uuid::Uuid;

use super::posting_context::AdvertPostingContext;
use super::repository;
use crate::general_ledger::repository::Model as GeneralLedgerModel;
use crate::general_ledger::turnover_registry::get_turnover_class;
use crate::shared::analytics::normalization::is_significant_amount;
use crate::shared::data::db::{acquire_sqlite_write_lock, get_connection};

const REGISTRATOR_TYPE: &str = "a026_wb_advert_daily";
const TURNOVER_CODE_RESERVE: &str = "advert_clicks_order_accrual";
const TURNOVER_CODE_DIRECT: &str = "advert_clicks_no_order";
const RESOURCE_TABLE_P913: &str = "p913_wb_advert_order_attr";
const RESOURCE_TABLE_P911: &str = "p911_wb_advert_by_items";

fn now_str() -> String {
    Utc::now().to_rfc3339()
}

fn to_general_ledger_entry_reserve(
    id: &str,
    entry_date: &str,
    connection_mp_ref: Option<String>,
    registrator_ref: &str,
    amount: f64,
) -> GeneralLedgerModel {
    let class = get_turnover_class(TURNOVER_CODE_RESERVE)
        .unwrap_or_else(|| panic!("Missing turnover class for {}", TURNOVER_CODE_RESERVE));

    GeneralLedgerModel {
        id: id.to_string(),
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
        turnover_code: TURNOVER_CODE_RESERVE.to_string(),
        resource_table: RESOURCE_TABLE_P913.to_string(),
        resource_field: "amount".to_string(),
        resource_sign: 1,
        created_at: now_str(),
    }
}

fn to_general_ledger_entry_direct(
    id: &str,
    entry_date: &str,
    connection_mp_ref: Option<String>,
    registrator_ref: &str,
    amount: f64,
) -> GeneralLedgerModel {
    let class = get_turnover_class(TURNOVER_CODE_DIRECT)
        .unwrap_or_else(|| panic!("Missing turnover class for {}", TURNOVER_CODE_DIRECT));

    GeneralLedgerModel {
        id: id.to_string(),
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
        turnover_code: TURNOVER_CODE_DIRECT.to_string(),
        resource_table: RESOURCE_TABLE_P911.to_string(),
        resource_field: "amount".to_string(),
        resource_sign: 1,
        created_at: now_str(),
    }
}

/// Округление до копейки (2 знака после запятой).
fn round_kopeyka(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}

fn allocate_costs(total_cost: f64, basis_flat: &[f64]) -> (Vec<f64>, f64, usize) {
    let total_basis: f64 = basis_flat.iter().copied().sum();
    let alloc_indices: Vec<usize> = if total_basis > f64::EPSILON {
        basis_flat
            .iter()
            .enumerate()
            .filter_map(|(i, &b)| if b > f64::EPSILON { Some(i) } else { None })
            .collect()
    } else {
        // Some WB order rows arrive from the orders endpoint without any price fields.
        // If every selected order has zero basis, keep attribution alive by splitting evenly.
        (0..basis_flat.len()).collect()
    };
    let n_with_price = alloc_indices.len();

    let mut alloc_flat: Vec<f64> = vec![0.0; basis_flat.len()];
    let mut accumulated = 0.0_f64;
    for (pos, &flat_i) in alloc_indices.iter().enumerate() {
        let basis = basis_flat[flat_i];
        let cost = if pos + 1 == n_with_price {
            round_kopeyka(total_cost - accumulated)
        } else {
            let raw_val = if total_basis > f64::EPSILON {
                total_cost * (basis / total_basis)
            } else if n_with_price > 0 {
                total_cost / n_with_price as f64
            } else {
                0.0
            };
            let rounded = round_kopeyka(raw_val);
            accumulated += rounded;
            rounded
        };
        alloc_flat[flat_i] = cost;
    }

    (alloc_flat, total_basis, n_with_price)
}

fn build_direct_p911_entries(
    document_id: Uuid,
    document: &WbAdvertDaily,
    linked_orders: &[WbAdvertLinkedOrdersByNm],
    general_ledger_ref: Option<&str>,
    product_refs: &HashMap<i64, String>,
) -> Vec<crate::projections::p911_wb_advert_by_items::repository::Model> {
    let registrator_ref = document_id.to_string();
    let campaign_code = document.header.advert_id.to_string();
    let mut result = Vec::new();

    let reserve_amount: f64 = linked_orders.iter().map(|group| group.wb_advert_sum).sum();
    let target_direct = round_kopeyka((document.totals.sum - reserve_amount).max(0.0));
    if target_direct <= f64::EPSILON {
        return result;
    }

    // Зарезервированная (привязанная к заказам) сумма per nm_id. Группы linked_orders
    // отфильтрованы (только wb_reported_orders > 0) и не совпадают позиционно со
    // строками документа, поэтому ищем по nm_id, а не через zip.
    let reserve_by_nm: HashMap<i64, f64> = linked_orders
        .iter()
        .map(|group| (group.nm_id, group.wb_advert_sum))
        .collect();

    for line in &document.lines {
        let reserve = reserve_by_nm.get(&line.nm_id).copied().unwrap_or(0.0);
        let direct_amount = round_kopeyka((line.metrics.sum - reserve).max(0.0));
        if direct_amount <= f64::EPSILON {
            continue;
        }

        let timestamp = now_str();
        result.push(
            crate::projections::p911_wb_advert_by_items::repository::Model {
                id: Uuid::new_v4().to_string(),
                connection_mp_ref: document.header.connection_id.clone(),
                entry_date: document.header.document_date.clone(),
                turnover_code: TURNOVER_CODE_DIRECT.to_string(),
                amount: direct_amount,
                nomenclature_ref: line.nomenclature_ref.clone(),
                wb_advert_campaign_code: campaign_code.clone(),
                registrator_type: REGISTRATOR_TYPE.to_string(),
                registrator_ref: registrator_ref.clone(),
                general_ledger_ref: general_ledger_ref.map(str::to_string),
                marketplace_product_ref: product_refs.get(&line.nm_id).cloned(),
                is_problem: line.nomenclature_ref.is_none(),
                created_at: timestamp.clone(),
                updated_at: timestamp,
            },
        );
    }

    let actual_direct: f64 = result.iter().map(|entry| entry.amount).sum();
    let delta = round_kopeyka(target_direct - actual_direct);
    if delta.abs() > 0.01 {
        if let Some(last) = result.last_mut() {
            last.amount = round_kopeyka(last.amount + delta);
        } else if target_direct > f64::EPSILON {
            let timestamp = now_str();
            result.push(
                crate::projections::p911_wb_advert_by_items::repository::Model {
                    id: Uuid::new_v4().to_string(),
                    connection_mp_ref: document.header.connection_id.clone(),
                    entry_date: document.header.document_date.clone(),
                    turnover_code: TURNOVER_CODE_DIRECT.to_string(),
                    amount: target_direct,
                    nomenclature_ref: None,
                    wb_advert_campaign_code: campaign_code.clone(),
                    registrator_type: REGISTRATOR_TYPE.to_string(),
                    registrator_ref,
                    general_ledger_ref: general_ledger_ref.map(str::to_string),
                    marketplace_product_ref: None,
                    is_problem: true,
                    created_at: timestamp.clone(),
                    updated_at: timestamp,
                },
            );
        }
    }

    result
}

/// Собирает связанные заказы по позициям рекламного отчёта.
///
/// Логика выборки per nm_id:
/// - кандидаты сортируются по уже начисленной атрибуции (ASC) из других
///   документов, чтобы приоритет получали заказы без накопленных начислений;
/// - берём первые N = wb_reported_orders кандидатов,
/// - если найдено меньше N — берём сколько есть,
/// - если найдено больше N — лишние показываются с is_allocated=false.
///
async fn build_linked_orders(
    document: &WbAdvertDaily,
    document_id: Uuid,
    context: &mut AdvertPostingContext,
) -> Result<(i64, Vec<WbAdvertLinkedOrdersByNm>)> {
    // ── Phase 1: fetch all raw candidates ────────────────────────────────────
    // Цены из marketplace-пейлаода дочитывает сам контекст (в предзагруженном
    // режиме — один раз на заказ, в ленивом — при выборке).
    let mut raw_per_nm: Vec<(i64, String, Option<String>, i64, Vec<WbOrders>)> = Vec::new();

    for line in &document.lines {
        let raw = if line.metrics.orders > 0 {
            context
                .orders_for(&document.header.document_date, line.nm_id)
                .await
                .unwrap_or_else(|err| {
                    tracing::warn!(
                        "a026 linked orders lookup failed for nm_id={}: {}",
                        line.nm_id,
                        err
                    );
                    Vec::new()
                })
        } else {
            Vec::new()
        };
        raw_per_nm.push((
            line.nm_id,
            line.nm_name.clone(),
            line.nomenclature_ref.clone(),
            line.metrics.orders,
            raw,
        ));
    }

    // ── Phase 3: sort candidates by existing attribution (excluding self) ─────
    // Записи текущего документа исключаем — они будут удалены при перепроведении
    // и не должны искусственно повышать «накопленную нагрузку» его заказов.
    let self_ref = document_id.to_string();
    for (_, _, _, _, raw) in &mut raw_per_nm {
        let order_keys: Vec<String> = raw.iter().map(|o| o.header.document_no.clone()).collect();
        let existing_attr = context
            .reserve_sums(&order_keys, &self_ref)
            .await
            .unwrap_or_default();

        // Стабильная сортировка: при равной сумме сохраняется хронологический порядок.
        raw.sort_by(|a, b| {
            let sa = existing_attr
                .get(&a.header.document_no)
                .copied()
                .unwrap_or(0.0);
            let sb = existing_attr
                .get(&b.header.document_no)
                .copied()
                .unwrap_or(0.0);
            sa.partial_cmp(&sb).unwrap_or(std::cmp::Ordering::Equal)
        });
    }

    // ── Phase 4: select first N per nm_id ────────────────────────────────────
    let mut per_line: Vec<(
        i64,
        String,
        Option<String>,
        i64,
        Vec<WbOrders>,
        Vec<WbOrders>,
    )> = Vec::new();
    for (nm_id, nm_name, line_nomenclature_ref, wb_reported_orders, raw) in raw_per_nm {
        let wb_reported = wb_reported_orders.max(0) as usize;
        let n_take = raw.len().min(wb_reported);
        let mut it = raw.into_iter();
        let selected: Vec<WbOrders> = it.by_ref().take(n_take).collect();
        let extras: Vec<WbOrders> = it.collect();
        per_line.push((
            nm_id,
            nm_name,
            line_nomenclature_ref,
            wb_reported_orders,
            selected,
            extras,
        ));
    }

    // ── Phase 5: allocate whole document across selected orders ──────────────
    // WB can report orders on one nm_id and spend on another, so line-level
    // allocation would leave part of the reported orders without reserve.
    let basis_flat: Vec<f64> = per_line
        .iter()
        .flat_map(|(_, _, _, _, selected, _)| selected.iter())
        .map(|order| order.line.allocation_basis())
        .collect();
    let (alloc_flat, total_basis, n_with_price) = allocate_costs(document.totals.sum, &basis_flat);

    let mut groups = Vec::new();
    let mut total_found = 0i64;
    let mut flat_index = 0usize;

    for (nm_id, nm_name, line_nomenclature_ref, wb_reported_orders, selected, extras) in per_line {
        let n = selected.len();
        let mut found_orders: Vec<WbAdvertFoundOrder> = Vec::new();

        for (i, order) in selected.iter().enumerate() {
            let current_index = flat_index + i;
            let basis = basis_flat[current_index];
            let allocated_cost = alloc_flat[current_index];
            if !is_significant_amount(allocated_cost) {
                continue;
            }
            let is_allocated = true;
            // allocation_ratio — доля цены этого заказа в ГЛОБАЛЬНОМ basis.
            let allocation_ratio = if total_basis > f64::EPSILON {
                basis / total_basis
            } else if n_with_price > 0 {
                1.0 / n_with_price as f64
            } else {
                0.0
            };
            found_orders.push(WbAdvertFoundOrder {
                order_key: order.header.document_no.clone(),
                order_id: Some(order.base.id.as_string()),
                order_date: Some(order.state.order_dt.format("%Y-%m-%d").to_string()),
                nomenclature_ref: order
                    .nomenclature_ref
                    .clone()
                    .or_else(|| line_nomenclature_ref.clone()),
                finished_price: order.line.finished_price,
                is_cancel: order.state.is_cancel,
                allocation_basis: basis,
                is_allocated,
                allocation_ratio,
                allocated_cost,
            });
        }

        // Лишние (за пределами N): видны в UI, расход не выделяется.
        for order in &extras {
            let basis = order.line.allocation_basis();
            found_orders.push(WbAdvertFoundOrder {
                order_key: order.header.document_no.clone(),
                order_id: Some(order.base.id.as_string()),
                order_date: Some(order.state.order_dt.format("%Y-%m-%d").to_string()),
                nomenclature_ref: order
                    .nomenclature_ref
                    .clone()
                    .or_else(|| line_nomenclature_ref.clone()),
                finished_price: order.line.finished_price,
                is_cancel: order.state.is_cancel,
                allocation_basis: basis,
                is_allocated: false,
                allocation_ratio: 0.0,
                allocated_cost: 0.0,
            });
        }

        let wb_advert_sum: f64 = found_orders
            .iter()
            .filter(|order| order.is_allocated)
            .map(|order| order.allocated_cost)
            .sum();
        flat_index += n;

        // Строки без заказов по WB не показываем. Позиции с orders>0 сохраняем даже без
        // кандидатов a015 — в UI видна «дыра» между метрикой WB и фактом в a015.
        if wb_reported_orders <= 0 {
            continue;
        }

        total_found += found_orders
            .iter()
            .filter(|order| order.is_allocated)
            .count() as i64;

        let wb_advert_sum = if is_significant_amount(wb_advert_sum) {
            round_kopeyka(wb_advert_sum)
        } else {
            0.0
        };

        groups.push(WbAdvertLinkedOrdersByNm {
            nm_id,
            nm_name,
            wb_reported_orders,
            wb_advert_sum,
            found_orders,
        });
    }

    Ok((total_found, groups))
}

async fn refresh_line_nomenclature_refs(
    document: &mut WbAdvertDaily,
    context: &mut AdvertPostingContext,
) -> Result<usize> {
    let mut changed = 0usize;

    for line in &mut document.lines {
        let resolved = context.nomenclature_ref(line.nm_id).await?;

        if line.nomenclature_ref != resolved {
            line.nomenclature_ref = resolved;
            changed += 1;
        }
    }

    Ok(changed)
}

/// Гарантирует наличие a007_marketplace_product для каждого nm_id документа.
///
/// Позиции рекламного отчёта могут отсутствовать в a007 — в этом случае элемент
/// создаётся автоматически (с пометкой в комментарии). Возвращает карту
/// `nm_id → a007 id` для заполнения измерения `marketplace_product_ref` в p911.
async fn ensure_marketplace_products(
    document: &WbAdvertDaily,
    document_id: Uuid,
    context: &mut AdvertPostingContext,
) -> Result<HashMap<i64, String>> {
    let mut refs: HashMap<i64, String> = HashMap::new();

    for line in &document.lines {
        if line.nm_id <= 0 || refs.contains_key(&line.nm_id) {
            continue;
        }

        let product_ref = context
            .product_ref(
                line.nm_id,
                &line.nm_name,
                &document.header.marketplace_id,
                &document.header.document_no,
                &document.header.document_date,
                document_id,
            )
            .await?;

        refs.insert(line.nm_id, product_ref);
    }

    Ok(refs)
}

/// Проведение одного документа с собственным (ленивым) контекстом.
/// Для массового проведения используй [`post_document_with_context`] с
/// предзагруженным контекстом — иначе каждый документ платит за N+1 к a015.
pub async fn post_document(id: Uuid) -> Result<()> {
    let document = repository::get_by_id(id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Document not found: {}", id))?;

    let mut context = AdvertPostingContext::lazy(document.header.connection_id.clone());
    post_document_with_context(id, &mut context).await
}

pub async fn post_document_with_context(
    id: Uuid,
    context: &mut AdvertPostingContext,
) -> Result<()> {
    let mut document = repository::get_by_id(id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Document not found: {}", id))?;

    let refreshed_lines = refresh_line_nomenclature_refs(&mut document, context).await?;
    let product_refs = ensure_marketplace_products(&document, id, context).await?;
    let (total_found, linked_orders) = build_linked_orders(&document, id, context).await?;
    document.linked_orders_count = total_found;
    document.has_linked_orders = linked_orders.iter().any(|g| g.wb_reported_orders > 0);
    document.linked_orders = linked_orders;

    document.is_posted = true;
    document.base.metadata.is_posted = true;
    document.before_write();

    tracing::info!(
        "a026 post_document: linked orders found={} refreshed_lines={} for document_id={}",
        total_found,
        refreshed_lines,
        id
    );

    let registrator_ref = id.to_string();
    let legacy_projection_ref = format!("a026:{registrator_ref}");

    // GL проводка advert_clicks_order_accrual (Phase 1 p913): Д9601/К7609.
    let reserve_amount: f64 = document
        .linked_orders
        .iter()
        .map(|group| group.wb_advert_sum)
        .sum();
    let direct_amount = round_kopeyka((document.totals.sum - reserve_amount).max(0.0));

    let gl_reserve_entry = if reserve_amount.abs() > f64::EPSILON {
        let gl_reserve_ref = Uuid::new_v4().to_string();
        Some(to_general_ledger_entry_reserve(
            &gl_reserve_ref,
            &document.header.document_date,
            Some(document.header.connection_id.clone()),
            &registrator_ref,
            reserve_amount,
        ))
    } else {
        None
    };
    let gl_direct_entry = if direct_amount.abs() > f64::EPSILON {
        let gl_direct_ref = Uuid::new_v4().to_string();
        Some(to_general_ledger_entry_direct(
            &gl_direct_ref,
            &document.header.document_date,
            Some(document.header.connection_id.clone()),
            &registrator_ref,
            direct_amount,
        ))
    } else {
        None
    };

    // p913 reserve-строки per-order. Привязываем каждую к id GL-проводки,
    // чтобы general_ledger_ref всегда был валидным.
    let p913_entries =
        crate::projections::p913_wb_advert_order_attr::service::build_reserve_entries(
            id,
            &document,
            gl_reserve_entry.as_ref().map(|entry| entry.id.as_str()),
        );
    let p911_entries = build_direct_p911_entries(
        id,
        &document,
        &document.linked_orders,
        gl_direct_entry.as_ref().map(|entry| entry.id.as_str()),
        &product_refs,
    );
    // Serialize all document JSON before acquiring the write lock/transaction.
    let prepared_document = repository::prepare_document_update(&document)?;
    let gl_entries: Vec<_> = gl_reserve_entry
        .iter()
        .chain(gl_direct_entry.iter())
        .cloned()
        .collect();
    let prepared_gl_entries =
        crate::projections::general_ledger::repository::prepare_entries(&gl_entries);
    let prepared_p913_entries =
        crate::projections::p913_wb_advert_order_attr::repository::prepare_entries(&p913_entries);
    let prepared_p911_entries =
        crate::projections::p911_wb_advert_by_items::repository::prepare_entries(&p911_entries);
    // Стадия 1 воронки p916: платные показы (show_paid_count = metrics.views).
    // Формируется при проведении (idempotent: delete-by-registrator + insert ниже),
    // registrator_ref = id документа.
    let p916_rows =
        crate::projections::p916_mp_sales_funnel_turnovers::builder::from_wb_advert_daily(
            &document,
            &registrator_ref,
        );

    let db = get_connection();
    let _write_guard = acquire_sqlite_write_lock().await;
    let transaction_started_at = std::time::Instant::now();
    let txn = db.begin().await?;

    // UPDATE is intentionally the first statement in this DEFERRED transaction.
    // It acquires the SQLite write lock immediately instead of first creating a
    // read snapshot which could later fail to upgrade with SQLITE_BUSY_SNAPSHOT.
    repository::update_prepared_document_with_conn(&txn, prepared_document).await?;

    crate::projections::general_ledger::repository::delete_by_registrator_with_conn(
        &txn,
        REGISTRATOR_TYPE,
        &registrator_ref,
    )
    .await?;
    crate::projections::p913_wb_advert_order_attr::repository::delete_by_registrator_with_conn(
        &txn,
        REGISTRATOR_TYPE,
        &registrator_ref,
    )
    .await?;
    crate::projections::p911_wb_advert_by_items::repository::delete_by_registrator_ref_with_conn(
        &txn,
        &registrator_ref,
    )
    .await?;
    crate::projections::p911_wb_advert_by_items::repository::delete_by_registrator_ref_with_conn(
        &txn,
        &legacy_projection_ref,
    )
    .await?;

    crate::projections::general_ledger::repository::insert_prepared_entries_with_conn(
        &txn,
        prepared_gl_entries,
    )
    .await?;
    crate::projections::p913_wb_advert_order_attr::repository::insert_prepared_entries_with_conn(
        &txn,
        prepared_p913_entries,
    )
    .await?;
    crate::projections::p911_wb_advert_by_items::repository::insert_prepared_entries_with_conn(
        &txn,
        prepared_p911_entries,
    )
    .await?;
    crate::projections::p916_mp_sales_funnel_turnovers::repository::delete_by_registrator_with_conn(
        &txn,
        crate::projections::p916_mp_sales_funnel_turnovers::builder::REG_A026,
        &registrator_ref,
    )
    .await?;
    crate::projections::p916_mp_sales_funnel_turnovers::repository::insert_many_with_conn(
        &txn, &p916_rows,
    )
    .await?;

    txn.commit().await?;
    tracing::info!(
        "a026 post_document: SQL transaction committed document_id={}, elapsed_ms={}",
        id,
        transaction_started_at.elapsed().as_millis()
    );

    // Только после коммита: следующий документ пачки должен видеть атрибуцию
    // этого документа так же, как увидел бы её в БД при проведении по одному.
    context.record_posted_reserve(&p913_entries);

    Ok(())
}

pub async fn unpost_document(id: Uuid) -> Result<()> {
    let mut document = repository::get_by_id(id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Document not found: {}", id))?;

    document.is_posted = false;
    document.base.metadata.is_posted = false;
    document.has_linked_orders = false;
    document.linked_orders_count = 0;
    document.linked_orders.clear();
    document.before_write();
    repository::upsert_document(&document).await?;

    let registrator_ref = id.to_string();
    let legacy_projection_ref = format!("a026:{registrator_ref}");

    let db = get_connection();
    let _write_guard = acquire_sqlite_write_lock().await;
    let txn = db.begin().await?;

    crate::projections::general_ledger::repository::delete_by_registrator_with_conn(
        &txn,
        REGISTRATOR_TYPE,
        &registrator_ref,
    )
    .await?;
    crate::projections::p913_wb_advert_order_attr::repository::delete_by_registrator_with_conn(
        &txn,
        REGISTRATOR_TYPE,
        &registrator_ref,
    )
    .await?;
    crate::projections::p911_wb_advert_by_items::repository::delete_by_registrator_ref_with_conn(
        &txn,
        &registrator_ref,
    )
    .await?;
    crate::projections::p911_wb_advert_by_items::repository::delete_by_registrator_ref_with_conn(
        &txn,
        &legacy_projection_ref,
    )
    .await?;
    crate::projections::p916_mp_sales_funnel_turnovers::repository::delete_by_registrator_with_conn(
        &txn,
        crate::projections::p916_mp_sales_funnel_turnovers::builder::REG_A026,
        &registrator_ref,
    )
    .await?;

    txn.commit().await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::allocate_costs;
    use contracts::domain::a015_wb_orders::aggregate::WbOrdersLine;

    #[test]
    fn allocation_basis_chain_price_with_disc_finished_price_price() {
        let line = |pwd, fp, p, tp| WbOrdersLine {
            line_id: "1".into(),
            supplier_article: String::new(),
            nm_id: 0,
            barcode: String::new(),
            category: None,
            subject: None,
            brand: None,
            tech_size: None,
            qty: 1.0,
            total_price: tp,
            discount_percent: None,
            spp: None,
            finished_price: fp,
            price_with_disc: pwd,
            price: p,
            sale_price: None,
            dealer_price_ut: None,
            margin_pro: None,
            currency_code: None,
            fx_rate: None,
        };
        assert_eq!(
            line(Some(80.0), Some(100.0), Some(70.0), Some(90.0)).allocation_basis(),
            80.0
        );
        assert_eq!(
            line(None, Some(100.0), Some(70.0), Some(90.0)).allocation_basis(),
            100.0
        );
        assert_eq!(
            line(None, None, Some(70.0), Some(90.0)).allocation_basis(),
            70.0
        );
        assert_eq!(line(None, None, None, Some(90.0)).allocation_basis(), 90.0);
    }

    #[test]
    fn allocate_costs_splits_evenly_when_all_basis_is_zero() {
        let (allocated, total_basis, count) = allocate_costs(11940.4, &[0.0, 0.0]);

        assert_eq!(total_basis, 0.0);
        assert_eq!(count, 2);
        assert_eq!(allocated, vec![5970.2, 5970.2]);
    }

    #[test]
    fn allocate_costs_uses_positive_basis_when_available() {
        let (allocated, total_basis, count) = allocate_costs(100.0, &[0.0, 30.0, 70.0]);

        assert_eq!(total_basis, 100.0);
        assert_eq!(count, 2);
        assert_eq!(allocated, vec![0.0, 30.0, 70.0]);
    }

    #[test]
    fn allocate_costs_splits_document_amount_between_wb_orders() {
        let (allocated, total_basis, count) = allocate_costs(1461.45, &[13675.0, 41964.0]);

        assert_eq!(total_basis, 55639.0);
        assert_eq!(count, 2);
        assert_eq!(allocated, vec![359.2, 1102.25]);
    }
}

#[cfg(test)]
mod stable_id_tests {
    use contracts::domain::a026_wb_advert_daily::aggregate::{
        WbAdvertDailyHeader, WbAdvertDailyId,
    };

    #[test]
    fn stable_document_id_is_deterministic() {
        let header = WbAdvertDailyHeader {
            document_no: "WB-ADV-123-2026-05-17".to_string(),
            document_date: "2026-05-17".to_string(),
            advert_id: 123,
            connection_id: "conn-1".to_string(),
            organization_id: "org-1".to_string(),
            marketplace_id: "mp-1".to_string(),
        };
        let a = WbAdvertDailyId::stable_for_header(&header);
        let b = WbAdvertDailyId::stable_for_header(&header);
        assert_eq!(a.value(), b.value());
    }
}
