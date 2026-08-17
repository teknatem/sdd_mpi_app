use anyhow::Result;
use chrono::Utc;
use contracts::shared::analytics::TurnoverLayer;
use uuid::Uuid;

use super::repository::Model;
use crate::general_ledger::repository::Model as GeneralLedgerModel;
use crate::shared::analytics::normalization::{normalize_expense, opt_nonzero};
use crate::shared::analytics::turnover_registry::get_turnover_class;

const REGISTRATOR_TYPE: &str = "p903_wb_finance_report";
const OP_PPVZ_REWARD_RU: &str = "Возмещение за выдачу и возврат товаров на ПВЗ";
const OP_SALE_RU: &str = "Продажа";
const OP_RETURN_RU: &str = "Возврат";

pub struct PostingResult {
    pub turnovers: Vec<Model>,
    pub general_ledger_entries: Vec<GeneralLedgerModel>,
}

fn now_str() -> String {
    Utc::now().to_rfc3339()
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

fn operation_name(
    entry: &crate::projections::p903_wb_finance_report::repository::Model,
) -> Option<&str> {
    entry
        .supplier_oper_name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn has_operation(
    entry: &crate::projections::p903_wb_finance_report::repository::Model,
    expected: &[&str],
) -> bool {
    let Some(actual) = operation_name(entry) else {
        return false;
    };

    expected.iter().any(|item| actual == *item)
}

fn is_return_row(entry: &crate::projections::p903_wb_finance_report::repository::Model) -> bool {
    entry.return_amount.unwrap_or(0.0).abs() > f64::EPSILON
        || entry
            .supplier_oper_name
            .as_deref()
            .is_some_and(|value| value.eq_ignore_ascii_case("return") || value == OP_RETURN_RU)
}

fn is_sale_row(entry: &crate::projections::p903_wb_finance_report::repository::Model) -> bool {
    entry.retail_amount.unwrap_or(0.0).abs() > f64::EPSILON
        || entry
            .supplier_oper_name
            .as_deref()
            .is_some_and(|value| value.eq_ignore_ascii_case("sale") || value == OP_SALE_RU)
}

fn is_sale_or_return_operation(
    entry: &crate::projections::p903_wb_finance_report::repository::Model,
) -> bool {
    has_operation(entry, &[OP_SALE_RU, OP_RETURN_RU]) || is_sale_row(entry) || is_return_row(entry)
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

pub fn source_ref_from_model(
    entry: &crate::projections::p903_wb_finance_report::repository::Model,
) -> String {
    entry.id.clone()
}

fn build_row(
    entry: &crate::projections::p903_wb_finance_report::repository::Model,
    turnover_code: &str,
    amount: Option<f64>,
    comment: Option<String>,
) -> Option<Model> {
    let amount = opt_nonzero(amount)?;
    let registrator_ref = source_ref_from_model(entry);
    let now = now_str();
    let (value_kind, agg_kind) = classifier_kinds(turnover_code);

    Some(Model {
        id: format!(
            "{}:{}:{}:{}",
            entry.connection_mp_ref,
            registrator_ref,
            turnover_code,
            TurnoverLayer::Fact.as_str()
        ),
        connection_mp_ref: entry.connection_mp_ref.clone(),
        entry_date: normalize_business_date_str(&entry.rr_dt),
        layer: TurnoverLayer::Fact.as_str().to_string(),
        turnover_code: turnover_code.to_string(),
        value_kind,
        agg_kind,
        amount,
        registrator_type: REGISTRATOR_TYPE.to_string(),
        registrator_ref,
        general_ledger_ref: None,
        nomenclature_ref: None,
        comment,
        created_at: now.clone(),
        updated_at: now,
    })
}

fn make_general_ledger_entry(row: &Model, _posting_id: &str) -> Option<GeneralLedgerModel> {
    let class = get_turnover_class(&row.turnover_code)?;
    if !class.generates_journal_entry || row.amount.abs() <= f64::EPSILON {
        return None;
    }

    Some(GeneralLedgerModel {
        id: Uuid::new_v4().to_string(),
        entry_date: row.entry_date.clone(),
        layer: row.layer.clone(),
        entity: None,
        connection_mp_ref: Some(row.connection_mp_ref.clone()),
        registrator_type: row.registrator_type.clone(),
        registrator_ref: row.registrator_ref.clone(),
        order_id: None,
        debit_account: class.debit_account.to_string(),
        credit_account: class.credit_account.to_string(),
        amount: row.amount,
        qty: None,
        turnover_code: row.turnover_code.clone(),
        resource_table: "p910_mp_unlinked_turnovers".to_string(),
        resource_field: "amount".to_string(),
        resource_sign: 1,
        created_at: now_str(),
    })
}

fn push_row(
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

pub fn from_wb_finance_row(
    entry: &crate::projections::p903_wb_finance_report::repository::Model,
    posting_id: &str,
) -> Result<PostingResult> {
    if entry
        .srid
        .as_deref()
        .is_some_and(|value| !value.trim().is_empty())
    {
        return Ok(PostingResult {
            turnovers: vec![],
            general_ledger_entries: vec![],
        });
    }

    let mut turnovers = Vec::new();
    let mut general_ledger_entries = Vec::new();

    push_row(
        &mut turnovers,
        &mut general_ledger_entries,
        build_row(
            entry,
            "mp_storage",
            normalize_expense(entry.storage_fee),
            Some("WB finance row without srid: storage_fee".to_string()),
        ),
        posting_id,
    );
    push_row(
        &mut turnovers,
        &mut general_ledger_entries,
        build_row(
            entry,
            "mp_logistics",
            normalize_expense(entry.delivery_rub),
            Some("WB finance row without srid: logistics".to_string()),
        ),
        posting_id,
    );
    push_row(
        &mut turnovers,
        &mut general_ledger_entries,
        build_row(
            entry,
            finance_turnover_code(
                entry,
                "mp_rebill_logistic_cost",
                "mp_rebill_logistic_cost_nm",
            ),
            entry.rebill_logistic_cost,
            Some("WB finance row without srid: rebill_logistic_cost".to_string()),
        ),
        posting_id,
    );
    push_row(
        &mut turnovers,
        &mut general_ledger_entries,
        build_row(
            entry,
            "acceptance",
            normalize_expense(entry.delivery_amount),
            Some("WB finance row without srid: delivery_amount".to_string()),
        ),
        posting_id,
    );
    push_row(
        &mut turnovers,
        &mut general_ledger_entries,
        build_row(
            entry,
            "mp_penalty",
            normalize_expense(entry.penalty),
            Some("WB finance row without srid: penalty".to_string()),
        ),
        posting_id,
    );
    push_row(
        &mut turnovers,
        &mut general_ledger_entries,
        build_row(
            entry,
            "mp_acquiring",
            normalize_expense(entry.acquiring_fee),
            Some("WB finance row without srid: acquiring_fee".to_string()),
        ),
        posting_id,
    );
    push_row(
        &mut turnovers,
        &mut general_ledger_entries,
        build_row(
            entry,
            if is_sale_or_return_operation(entry) {
                "mp_commission"
            } else {
                finance_turnover_code(
                    entry,
                    "mp_commission_adjustment",
                    "mp_commission_adjustment_nm",
                )
            },
            normalize_expense(if is_sale_or_return_operation(entry) {
                commission_sale_return_amount(entry)
            } else {
                commission_adjustment_amount(entry)
            }),
            Some("WB finance row without srid: commission".to_string()),
        ),
        posting_id,
    );

    push_row(
        &mut turnovers,
        &mut general_ledger_entries,
        build_row(
            entry,
            finance_turnover_code(entry, "mp_ppvz_reward", "mp_ppvz_reward_nm"),
            ppvz_reward_amount(entry),
            Some("WB finance row without srid: ppvz_reward".to_string()),
        ),
        posting_id,
    );

    if let Some(value) = entry
        .additional_payment
        .or(entry.cashback_amount)
        .filter(|value| value.abs() > f64::EPSILON)
    {
        let turnover_code = if value >= 0.0 {
            "adjustment_income"
        } else {
            "adjustment_expense"
        };
        push_row(
            &mut turnovers,
            &mut general_ledger_entries,
            build_row(
                entry,
                turnover_code,
                Some(value),
                Some("WB finance row without srid: adjustment".to_string()),
            ),
            posting_id,
        );
    }

    Ok(PostingResult {
        turnovers,
        general_ledger_entries,
    })
}

fn commission_adjustment_amount(
    entry: &crate::projections::p903_wb_finance_report::repository::Model,
) -> Option<f64> {
    opt_nonzero(Some(
        entry.ppvz_vw.unwrap_or(0.0)
            + entry.ppvz_vw_nds.unwrap_or(0.0)
            + entry.ppvz_sales_commission.unwrap_or(0.0),
    ))
}

fn commission_sale_return_amount(
    entry: &crate::projections::p903_wb_finance_report::repository::Model,
) -> Option<f64> {
    opt_nonzero(Some(
        entry.ppvz_vw.unwrap_or(0.0) + entry.ppvz_vw_nds.unwrap_or(0.0),
    ))
}

fn ppvz_reward_amount(
    entry: &crate::projections::p903_wb_finance_report::repository::Model,
) -> Option<f64> {
    let raw = entry
        .extra
        .as_deref()
        .and_then(|value| serde_json::from_str::<serde_json::Value>(value).ok())
        .and_then(|json| {
            json.get("ppvz_reward").and_then(|value| {
                value.as_f64().or_else(|| {
                    value
                        .as_str()
                        .and_then(|raw| raw.trim().parse::<f64>().ok())
                })
            })
        })
        .and_then(|value| opt_nonzero(Some(value)))?;

    if is_return_row(entry) {
        Some(-raw.abs())
    } else if is_sale_row(entry) || has_operation(entry, &[OP_SALE_RU, OP_PPVZ_REWARD_RU]) {
        Some(raw.abs())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
            srid: None,
            loaded_at_utc: "2026-03-01T00:00:00Z".to_string(),
            payload_version: 1,
            extra: None,
        }
    }

    #[test]
    fn unlinked_finance_row_with_nm_id_uses_nm_turnover_clones() {
        let mut row = base_finance_row();
        row.nm_id = Some(123456);
        row.rebill_logistic_cost = Some(120.0);
        row.ppvz_vw = Some(30.0);
        row.ppvz_vw_nds = Some(6.0);
        row.ppvz_sales_commission = Some(4.0);
        row.supplier_oper_name = Some(OP_PPVZ_REWARD_RU.to_string());
        row.extra = Some(r#"{"ppvz_reward":15.0}"#.to_string());

        let result = from_wb_finance_row(&row, "posting-1").unwrap();

        assert!(result
            .turnovers
            .iter()
            .any(|item| item.turnover_code == "mp_rebill_logistic_cost_nm"));
        assert!(result
            .turnovers
            .iter()
            .any(|item| item.turnover_code == "mp_commission_adjustment_nm"));
        assert!(result
            .turnovers
            .iter()
            .any(|item| item.turnover_code == "mp_ppvz_reward_nm"));
    }
}
