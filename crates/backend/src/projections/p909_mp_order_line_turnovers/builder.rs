use anyhow::Result;
use chrono::Utc;
use contracts::domain::a012_wb_sales::aggregate::WbSales;
use contracts::domain::a015_wb_orders::aggregate::WbOrders;
use contracts::shared::analytics::{EventKind, TurnoverLayer};
use uuid::Uuid;

use super::repository::Model;
use crate::general_ledger::repository::Model as GeneralLedgerModel;
use crate::shared::analytics::normalization::opt_nonzero;
use crate::shared::analytics::turnover_registry::get_turnover_class;
use crate::shared::marketplaces::wildberries::datetime::wb_business_date_str;

const ORDER_REGISTRATOR_TYPE: &str = "a015_wb_orders";
const OPER_REGISTRATOR_TYPE: &str = "a012_wb_sales";
const FACT_REGISTRATOR_TYPE: &str = "p903_wb_finance_report";

pub struct PostingResult {
    pub turnovers: Vec<Model>,
    pub general_ledger_entries: Vec<GeneralLedgerModel>,
}

fn now_str() -> String {
    Utc::now().to_rfc3339()
}

fn business_date_from_datetime(value: chrono::DateTime<Utc>) -> String {
    wb_business_date_str(&value)
}

fn normalize_business_date_str(value: &str) -> String {
    if value.len() >= 10 {
        value[..10].to_string()
    } else {
        value.to_string()
    }
}

fn has_nm_id(entry: &crate::projections::p903_wb_finance_report::repository::Model) -> bool {
    entry.nm_id.is_some_and(|value| value > 0)
}

fn finance_turnover_code(
    entry: &crate::projections::p903_wb_finance_report::repository::Model,
    base_code: &'static str,
    nm_code: &'static str,
) -> &'static str {
    if has_nm_id(entry) {
        nm_code
    } else {
        base_code
    }
}

fn classifier_kinds(turnover_code: &str) -> (String, String) {
    let class = get_turnover_class(turnover_code).unwrap_or_else(|| {
        panic!("Missing turnover class for code '{}'", turnover_code);
    });

    (
        class.value_kind.as_str().to_string(),
        class.agg_kind.as_str().to_string(),
    )
}

#[allow(clippy::too_many_arguments)]
fn new_row(
    connection_mp_ref: &str,
    order_key: &str,
    line_key: &str,
    line_event_key: &str,
    event_kind: EventKind,
    entry_date: &str,
    layer: TurnoverLayer,
    turnover_code: &str,
    amount: Option<f64>,
    nomenclature_ref: Option<String>,
    marketplace_product_ref: Option<String>,
    registrator_type: &str,
    registrator_ref: String,
) -> Option<Model> {
    let amount = opt_nonzero(amount)?;
    let now = now_str();
    let (value_kind, agg_kind) = classifier_kinds(turnover_code);

    Some(Model {
        id: format!(
            "{connection_mp_ref}:{line_event_key}:{turnover_code}:{}",
            layer.as_str()
        ),
        connection_mp_ref: connection_mp_ref.to_string(),
        order_key: order_key.to_string(),
        line_key: line_key.to_string(),
        line_event_key: line_event_key.to_string(),
        event_kind: event_kind.as_str().to_string(),
        entry_date: entry_date.to_string(),
        layer: layer.as_str().to_string(),
        turnover_code: turnover_code.to_string(),
        value_kind,
        agg_kind,
        amount,
        nomenclature_ref,
        marketplace_product_ref,
        registrator_type: registrator_type.to_string(),
        registrator_ref,
        link_status: "single".to_string(),
        general_ledger_ref: None,
        created_at: now.clone(),
        updated_at: now,
    })
}

fn make_general_ledger_entry(row: &Model, _posting_id: &str) -> Option<GeneralLedgerModel> {
    let class = get_turnover_class(&row.turnover_code)?;
    if !class.generates_journal_entry || row.amount.abs() <= f64::EPSILON {
        return None;
    }

    let gl_registrator_ref = match row.registrator_type.as_str() {
        ORDER_REGISTRATOR_TYPE | OPER_REGISTRATOR_TYPE => row
            .registrator_ref
            .split_once(':')
            .map(|(_, id)| id)
            .unwrap_or(&row.registrator_ref)
            .to_string(),
        FACT_REGISTRATOR_TYPE => row.registrator_ref.clone(),
        _ => row.registrator_ref.clone(),
    };

    let order_id = match row.registrator_type.as_str() {
        OPER_REGISTRATOR_TYPE | ORDER_REGISTRATOR_TYPE => Some(row.order_key.clone()),
        FACT_REGISTRATOR_TYPE => Some(row.line_key.clone()),
        _ => None,
    };

    Some(GeneralLedgerModel {
        id: Uuid::new_v4().to_string(),
        entry_date: row.entry_date.clone(),
        layer: row.layer.clone(),
        entity: None,
        connection_mp_ref: Some(row.connection_mp_ref.clone()),
        registrator_type: row.registrator_type.clone(),
        registrator_ref: gl_registrator_ref,
        order_id,
        debit_account: class.debit_account.to_string(),
        credit_account: class.credit_account.to_string(),
        amount: row.amount,
        qty: None,
        turnover_code: row.turnover_code.clone(),
        resource_table: "p909_mp_order_line_turnovers".to_string(),
        resource_field: "amount".to_string(),
        resource_sign: 1,
        created_at: now_str(),
    })
}

fn push_row_with_journal(
    turnovers: &mut Vec<Model>,
    general_ledger_entries: &mut Vec<GeneralLedgerModel>,
    row: Option<Model>,
    posting_id: &str,
) {
    let Some(mut row) = row else {
        return;
    };

    if let Some(je) = make_general_ledger_entry(&row, posting_id) {
        row.general_ledger_ref = Some(je.id.clone());
        general_ledger_entries.push(je);
    }

    turnovers.push(row);
}

fn push_row(target: &mut Vec<Model>, row: Option<Model>) {
    if let Some(row) = row {
        target.push(row);
    }
}

pub fn order_source_ref(document_id: &str) -> String {
    format!("a015:{document_id}")
}

pub fn oper_source_ref(document_id: &str) -> String {
    format!("a012:{document_id}")
}

pub fn finance_source_ref_from_model(
    entry: &crate::projections::p903_wb_finance_report::repository::Model,
) -> String {
    entry.id.clone()
}

pub fn is_finance_row_linked(
    entry: &crate::projections::p903_wb_finance_report::repository::Model,
) -> bool {
    entry
        .srid
        .as_deref()
        .is_some_and(|value| !value.trim().is_empty())
}

pub fn from_wb_order(document: &WbOrders, document_id: &str) -> Result<Vec<Model>> {
    let registrator_ref = order_source_ref(document_id);
    let line_key = document.line.line_id.clone();
    let order_key = document.header.document_no.clone();
    let line_event_key = format!("ordered:{line_key}");
    let entry_date = business_date_from_datetime(document.state.order_dt);

    let mut rows = Vec::new();
    push_row(
        &mut rows,
        new_row(
            &document.header.connection_id,
            &order_key,
            &line_key,
            &line_event_key,
            EventKind::Ordered,
            &entry_date,
            TurnoverLayer::Oper,
            "qty_ordered",
            Some(document.line.qty),
            document.nomenclature_ref.clone(),
            document.marketplace_product_ref.clone(),
            ORDER_REGISTRATOR_TYPE,
            registrator_ref,
        ),
    );

    Ok(rows)
}

pub fn from_wb_sales(
    document: &WbSales,
    document_id: &str,
    posting_id: &str,
    prod_item_cost_total: Option<f64>,
) -> Result<PostingResult> {
    let registrator_ref = oper_source_ref(document_id);
    let line_key = document.line.line_id.clone();
    let order_key = document.header.document_no.clone();
    let is_return = document.is_customer_return;
    let event_kind = if is_return {
        EventKind::Returned
    } else {
        EventKind::Sold
    };
    let line_event_key = format!("{}:{line_key}", event_kind.as_str());
    let entry_date = business_date_from_datetime(document.state.sale_dt);
    let sign = if is_return { -1.0 } else { 1.0 };

    let qty_abs = document.line.qty.abs();
    let revenue_base = document
        .line
        .price_effective
        .or(document.line.amount_line)
        .map(|v| v.abs());

    let spp_rate = document.line.spp.unwrap_or(0.0);
    let spp_amount = if spp_rate > f64::EPSILON {
        revenue_base.map(|b| -(b * spp_rate / 100.0) * sign)
    } else {
        None
    };

    let abs_price_diff = match (document.line.finished_price, document.line.amount_line) {
        (Some(fin), Some(base)) => Some(fin.abs() - base.abs()),
        _ => None,
    };

    // Для возврата используем storno-коды: красное сторно с теми же счетами,
    // что у оригинала, но с отрицательными суммами.  Позволяет видеть
    // обороты по отгрузкам и возвратам раздельно в аналитических отчётах.
    let (qty_code, revenue_pl_code, spp_code, commission_code, coinvestment_code) = if is_return {
        (
            "qty_sold_storno",
            "customer_revenue_pl_storno",
            "spp_discount_storno",
            "mp_commission_storno",
            "wb_coinvestment_storno",
        )
    } else {
        (
            "qty_sold",
            "customer_revenue_pl",
            "spp_discount",
            "mp_commission",
            "wb_coinvestment",
        )
    };

    let mut turnovers = Vec::new();
    let mut general_ledger_entries = Vec::new();

    let mut push = |turnover_code: &str, amount: Option<f64>| {
        push_row_with_journal(
            &mut turnovers,
            &mut general_ledger_entries,
            new_row(
                &document.header.connection_id,
                &order_key,
                &line_key,
                &line_event_key,
                event_kind,
                &entry_date,
                TurnoverLayer::Oper,
                turnover_code,
                amount,
                document.nomenclature_ref.clone(),
                document.marketplace_product_ref.clone(),
                OPER_REGISTRATOR_TYPE,
                registrator_ref.clone(),
            ),
            posting_id,
        );
    };

    push(qty_code, Some(qty_abs * sign));
    push(revenue_pl_code, revenue_base.map(|v| v * sign));
    if let Some(spp) = spp_amount {
        push(spp_code, Some(spp));
    }
    match abs_price_diff {
        Some(d) if d > f64::EPSILON => push(commission_code, Some(d * sign)),
        Some(d) if d < -f64::EPSILON => push(coinvestment_code, Some(-d * sign)),
        _ => {}
    }

    // wb_extra_discount: промо-скидка WB из своего бюджета сверх СПП.
    // Формула: finishedPrice − priceWithDisc × (1 − spp/100).
    // Отрицательна когда WB снизил цену ниже расчётной по СПП (акции, Клуб WB и т.д.).
    // Порог 1 ₽ отфильтровывает погрешности округления.
    let extra_discount_code = if is_return {
        "wb_extra_discount_storno"
    } else {
        "wb_extra_discount"
    };
    if let (Some(fin), Some(price_eff)) =
        (document.line.finished_price, document.line.price_effective)
    {
        let spp_adjusted = price_eff.abs() * (1.0 - spp_rate / 100.0);
        let diff = fin.abs() - spp_adjusted;
        if diff.abs() > 1.0 {
            push(extra_discount_code, Some(diff * sign));
        }
    }

    drop(push);

    push_common_oper_rows(
        &mut turnovers,
        &mut general_ledger_entries,
        document,
        &registrator_ref,
        posting_id,
        &order_key,
        &line_key,
        &line_event_key,
        event_kind,
        &entry_date,
        sign,
        is_return,
    );
    push_prod_cost_rows(
        &mut turnovers,
        &mut general_ledger_entries,
        document,
        &registrator_ref,
        posting_id,
        &order_key,
        &line_key,
        &line_event_key,
        event_kind,
        &entry_date,
        sign,
        is_return,
        prod_item_cost_total,
    );

    Ok(PostingResult {
        turnovers,
        general_ledger_entries,
    })
}

#[allow(clippy::too_many_arguments)]
fn push_common_oper_rows(
    turnovers: &mut Vec<Model>,
    general_ledger_entries: &mut Vec<GeneralLedgerModel>,
    document: &WbSales,
    registrator_ref: &str,
    posting_id: &str,
    order_key: &str,
    line_key: &str,
    line_event_key: &str,
    event_kind: EventKind,
    entry_date: &str,
    sign: f64,
    is_return: bool,
) {
    let (acquiring_code, payout_code, cost_code) = if is_return {
        (
            "mp_acquiring_storno",
            "seller_payout_storno",
            "item_cost_storno",
        )
    } else {
        ("mp_acquiring", "seller_payout", "item_cost")
    };

    for (turnover_code, amount) in [
        // acquiring и payout: WB сам передаёт нужный знак в source-данных
        (acquiring_code, document.line.acquiring_fee_plan),
        (payout_code, document.line.supplier_payout_plan),
        (
            cost_code,
            document
                .line
                .dealer_price_ut
                .or(document.line.cost_of_production)
                .map(|price| price * document.line.qty.abs() * sign),
        ),
    ] {
        push_row_with_journal(
            turnovers,
            general_ledger_entries,
            new_row(
                &document.header.connection_id,
                order_key,
                line_key,
                line_event_key,
                event_kind,
                entry_date,
                TurnoverLayer::Oper,
                turnover_code,
                amount,
                document.nomenclature_ref.clone(),
                document.marketplace_product_ref.clone(),
                OPER_REGISTRATOR_TYPE,
                registrator_ref.to_string(),
            ),
            posting_id,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn push_prod_cost_rows(
    turnovers: &mut Vec<Model>,
    general_ledger_entries: &mut Vec<GeneralLedgerModel>,
    document: &WbSales,
    registrator_ref: &str,
    posting_id: &str,
    order_key: &str,
    line_key: &str,
    line_event_key: &str,
    event_kind: EventKind,
    entry_date: &str,
    sign: f64,
    is_return: bool,
    prod_item_cost_total: Option<f64>,
) {
    let turnover_code = if is_return {
        "item_cost_storno"
    } else {
        "item_cost"
    };
    let amount = prod_item_cost_total.map(|total| total * sign);

    push_row_with_journal(
        turnovers,
        general_ledger_entries,
        new_row(
            &document.header.connection_id,
            order_key,
            line_key,
            line_event_key,
            event_kind,
            entry_date,
            TurnoverLayer::Prod,
            turnover_code,
            amount,
            document.nomenclature_ref.clone(),
            document.marketplace_product_ref.clone(),
            OPER_REGISTRATOR_TYPE,
            registrator_ref.to_string(),
        ),
        posting_id,
    );
}

pub fn from_wb_finance_row(
    entry: &crate::projections::p903_wb_finance_report::repository::Model,
    posting_id: &str,
) -> Result<PostingResult> {
    let Some(line_key) = entry.srid.clone() else {
        return Ok(PostingResult {
            turnovers: vec![],
            general_ledger_entries: vec![],
        });
    };

    let registrator_ref = finance_source_ref_from_model(entry);
    let is_return = entry.supplier_oper_name.as_deref() == Some("Возврат")
        || entry.return_amount.unwrap_or(0.0).abs() > f64::EPSILON;
    let is_sale = entry.supplier_oper_name.as_deref() == Some("Продажа")
        || entry.retail_amount.unwrap_or(0.0).abs() > f64::EPSILON;
    let event_kind = if is_return {
        EventKind::Returned
    } else if is_sale {
        EventKind::Sold
    } else if entry.penalty.unwrap_or(0.0).abs() > f64::EPSILON
        || entry.storage_fee.unwrap_or(0.0).abs() > f64::EPSILON
        || entry.rebill_logistic_cost.unwrap_or(0.0).abs() > f64::EPSILON
    {
        EventKind::Fee
    } else {
        EventKind::Other
    };

    let line_event_key = match event_kind {
        EventKind::Sold => format!("sold:{line_key}"),
        EventKind::Returned => format!("returned:{line_key}"),
        _ => format!("fee:{line_key}:{}:{}", entry.rr_dt, entry.rrd_id),
    };
    let entry_date = normalize_business_date_str(&entry.rr_dt);

    let mut turnovers = Vec::new();
    let mut general_ledger_entries = Vec::new();

    if event_kind == EventKind::Sold {
        push_row_with_journal(
            &mut turnovers,
            &mut general_ledger_entries,
            new_row(
                &entry.connection_mp_ref,
                &line_key,
                &line_key,
                &line_event_key,
                event_kind,
                &entry_date,
                TurnoverLayer::Fact,
                "qty_sold",
                entry.quantity.map(|value| value as f64),
                None,
                None,
                FACT_REGISTRATOR_TYPE,
                registrator_ref.clone(),
            ),
            posting_id,
        );
        push_row_with_journal(
            &mut turnovers,
            &mut general_ledger_entries,
            new_row(
                &entry.connection_mp_ref,
                &line_key,
                &line_key,
                &line_event_key,
                event_kind,
                &entry_date,
                TurnoverLayer::Fact,
                "customer_revenue",
                entry.retail_amount,
                None,
                None,
                FACT_REGISTRATOR_TYPE,
                registrator_ref.clone(),
            ),
            posting_id,
        );
    }

    if event_kind == EventKind::Returned {
        let return_amount = entry.return_amount.or(entry.retail_amount);
        push_row_with_journal(
            &mut turnovers,
            &mut general_ledger_entries,
            new_row(
                &entry.connection_mp_ref,
                &line_key,
                &line_key,
                &line_event_key,
                event_kind,
                &entry_date,
                TurnoverLayer::Fact,
                "qty_returned",
                entry.quantity.map(|value| value as f64),
                None,
                None,
                FACT_REGISTRATOR_TYPE,
                registrator_ref.clone(),
            ),
            posting_id,
        );
        push_row_with_journal(
            &mut turnovers,
            &mut general_ledger_entries,
            new_row(
                &entry.connection_mp_ref,
                &line_key,
                &line_key,
                &line_event_key,
                event_kind,
                &entry_date,
                TurnoverLayer::Fact,
                "customer_return",
                return_amount,
                None,
                None,
                FACT_REGISTRATOR_TYPE,
                registrator_ref.clone(),
            ),
            posting_id,
        );
    }

    push_common_fact_rows(
        &mut turnovers,
        &mut general_ledger_entries,
        entry,
        &registrator_ref,
        posting_id,
        &line_key,
        &line_event_key,
        event_kind,
        &entry_date,
    );

    Ok(PostingResult {
        turnovers,
        general_ledger_entries,
    })
}

#[allow(clippy::too_many_arguments)]
fn push_common_fact_rows(
    rows: &mut Vec<Model>,
    general_ledger_entries: &mut Vec<GeneralLedgerModel>,
    entry: &crate::projections::p903_wb_finance_report::repository::Model,
    registrator_ref: &str,
    posting_id: &str,
    line_key: &str,
    line_event_key: &str,
    event_kind: EventKind,
    entry_date: &str,
) {
    for (turnover_code, amount) in [
        (
            "mp_commission",
            Some(entry.ppvz_vw.unwrap_or(0.0) + entry.ppvz_vw_nds.unwrap_or(0.0)),
        ),
        ("mp_acquiring", entry.acquiring_fee),
        ("mp_storage", entry.storage_fee),
        ("mp_penalty", entry.penalty),
        ("seller_payout", entry.ppvz_for_pay),
        ("commission_percent", entry.commission_percent),
    ] {
        push_row_with_journal(
            rows,
            general_ledger_entries,
            new_row(
                &entry.connection_mp_ref,
                line_key,
                line_key,
                line_event_key,
                event_kind,
                entry_date,
                TurnoverLayer::Fact,
                turnover_code,
                amount,
                None,
                None,
                FACT_REGISTRATOR_TYPE,
                registrator_ref.to_string(),
            ),
            posting_id,
        );
    }

    push_row_with_journal(
        rows,
        general_ledger_entries,
        new_row(
            &entry.connection_mp_ref,
            line_key,
            line_key,
            line_event_key,
            event_kind,
            entry_date,
            TurnoverLayer::Fact,
            finance_turnover_code(
                entry,
                "mp_rebill_logistic_cost",
                "mp_rebill_logistic_cost_nm",
            ),
            entry.rebill_logistic_cost,
            None,
            None,
            FACT_REGISTRATOR_TYPE,
            registrator_ref.to_string(),
        ),
        posting_id,
    );
}

pub fn attach_order_context(existing: &mut Model, document: &WbOrders, _document_id: &str) {
    if existing.nomenclature_ref.is_none() {
        existing.nomenclature_ref = document.nomenclature_ref.clone();
    }
    if existing.marketplace_product_ref.is_none() {
        existing.marketplace_product_ref = document.marketplace_product_ref.clone();
    }
    if existing.order_key.is_empty() {
        existing.order_key = document.header.document_no.clone();
    }
    existing.updated_at = now_str();
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use contracts::domain::a012_wb_sales::aggregate::{
        WbSales, WbSalesHeader, WbSalesId, WbSalesLine, WbSalesSourceMeta, WbSalesState,
        WbSalesWarehouse,
    };
    use contracts::domain::common::BaseAggregate;
    use uuid::Uuid;

    fn base_finance_row() -> crate::projections::p903_wb_finance_report::repository::Model {
        crate::projections::p903_wb_finance_report::repository::Model {
            id: "p903-row-1".to_string(),
            rr_dt: "2026-03-01".to_string(),
            rrd_id: 100,
            source_row_ref: "p903:100".to_string(),
            connection_mp_ref: "conn-1".to_string(),
            organization_ref: "org-1".to_string(),
            acquiring_fee: None,
            acquiring_percent: None,
            additional_payment: None,
            bonus_type_name: None,
            commission_percent: None,
            delivery_amount: None,
            delivery_rub: None,
            nm_id: None,
            a004_nomenclature_ref: None,
            marketplace_product_ref: None,
            marketplace_order_ref: None,
            penalty: None,
            ppvz_vw: None,
            ppvz_vw_nds: None,
            ppvz_sales_commission: None,
            quantity: None,
            rebill_logistic_cost: None,
            retail_amount: None,
            retail_price: None,
            retail_price_withdisc_rub: None,
            return_amount: None,
            sa_name: None,
            storage_fee: None,
            subject_name: None,
            supplier_oper_name: None,
            cashback_amount: None,
            ppvz_for_pay: None,
            ppvz_kvw_prc: None,
            ppvz_kvw_prc_base: None,
            srv_dbs: None,
            srid: Some("srid-1".to_string()),
            loaded_at_utc: "2026-03-01T00:00:00Z".to_string(),
            payload_version: 1,
            extra: None,
        }
    }

    fn base_wb_sales(is_return: bool) -> WbSales {
        let sale_dt = chrono::Utc.with_ymd_and_hms(2026, 3, 1, 12, 0, 0).unwrap();
        WbSales {
            base: BaseAggregate::new(
                WbSalesId::new(Uuid::nil()),
                "WB-1".to_string(),
                "WB sale".to_string(),
            ),
            header: WbSalesHeader {
                document_no: "SRID-1".to_string(),
                sale_id: Some("sale-1".to_string()),
                connection_id: "conn-1".to_string(),
                organization_id: "org-1".to_string(),
                marketplace_id: "mp-1".to_string(),
            },
            line: WbSalesLine {
                line_id: "line-1".to_string(),
                supplier_article: "ART-1".to_string(),
                nm_id: 1,
                barcode: "123".to_string(),
                name: "Item".to_string(),
                qty: 2.0,
                price_list: None,
                discount_total: None,
                price_effective: Some(if is_return { -100.0 } else { 100.0 }),
                amount_line: Some(if is_return { -100.0 } else { 100.0 }),
                currency_code: None,
                total_price: None,
                payment_sale_amount: None,
                discount_percent: None,
                spp: None,
                finished_price: Some(if is_return { -100.0 } else { 100.0 }),
                is_fact: Some(false),
                sell_out_plan: None,
                sell_out_fact: None,
                acquiring_fee_plan: Some(5.0),
                acquiring_fee_fact: None,
                other_fee_plan: None,
                other_fee_fact: None,
                supplier_payout_plan: Some(95.0),
                supplier_payout_fact: None,
                profit_plan: None,
                profit_fact: None,
                cost_of_production: Some(40.0),
                commission_plan: Some(0.0),
                commission_fact: None,
                dealer_price_ut: Some(40.0),
            },
            state: WbSalesState {
                event_type: if is_return {
                    "return".to_string()
                } else {
                    "sale".to_string()
                },
                status_norm: "done".to_string(),
                sale_dt,
                last_change_dt: None,
                is_supply: None,
                is_realization: None,
            },
            warehouse: WbSalesWarehouse {
                warehouse_name: None,
                warehouse_type: None,
            },
            source_meta: WbSalesSourceMeta {
                raw_payload_ref: "raw-1".to_string(),
                fetched_at: sale_dt,
                document_version: 1,
            },
            is_posted: true,
            is_customer_return: is_return,
            marketplace_product_ref: Some("mp-prod-1".to_string()),
            nomenclature_ref: Some("nom-1".to_string()),
            prod_cost_problem: false,
            prod_cost_status: Some("ok".to_string()),
            prod_cost_problem_message: None,
            prod_cost_resolved_total: Some(88.0),
        }
    }

    #[test]
    fn linked_finance_row_with_nm_id_uses_nm_rebill_turnover() {
        let mut row = base_finance_row();
        row.nm_id = Some(123456);
        row.rebill_logistic_cost = Some(120.0);
        row.supplier_oper_name = Some("Логистика".to_string());

        let result = from_wb_finance_row(&row, "posting-1").unwrap();

        assert!(result
            .turnovers
            .iter()
            .any(|item| item.turnover_code == "mp_rebill_logistic_cost_nm"));
        assert!(!result
            .turnovers
            .iter()
            .any(|item| item.turnover_code == "mp_rebill_logistic_cost"));
        assert!(result
            .general_ledger_entries
            .iter()
            .any(|item| item.turnover_code == "mp_rebill_logistic_cost_nm"));
    }

    #[test]
    fn wb_sales_adds_prod_item_cost_without_removing_oper_item_cost() {
        let document = base_wb_sales(false);
        let result = from_wb_sales(&document, "doc-1", "posting-1", Some(88.0)).unwrap();

        assert!(result
            .turnovers
            .iter()
            .any(|item| item.turnover_code == "item_cost"
                && item.layer == "oper"
                && (item.amount - 80.0).abs() < 1e-9));
        assert!(result
            .turnovers
            .iter()
            .any(|item| item.turnover_code == "item_cost"
                && item.layer == "prod"
                && (item.amount - 88.0).abs() < 1e-9));
        assert!(result
            .general_ledger_entries
            .iter()
            .any(|item| item.turnover_code == "item_cost" && item.layer == "prod"));
    }

    #[test]
    fn wb_sales_return_adds_prod_item_cost_storno() {
        let document = base_wb_sales(true);
        let result = from_wb_sales(&document, "doc-1", "posting-1", Some(88.0)).unwrap();

        assert!(result
            .turnovers
            .iter()
            .any(|item| item.turnover_code == "item_cost_storno"
                && item.layer == "prod"
                && (item.amount + 88.0).abs() < 1e-9));
        assert!(result
            .general_ledger_entries
            .iter()
            .any(|item| item.turnover_code == "item_cost_storno" && item.layer == "prod"));
    }
}
