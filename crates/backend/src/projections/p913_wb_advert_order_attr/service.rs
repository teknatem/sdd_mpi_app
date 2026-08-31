use chrono::Utc;
use contracts::domain::a026_wb_advert_daily::aggregate::WbAdvertDaily;
use uuid::Uuid;

use super::repository::Model;
use crate::shared::analytics::normalization::is_significant_amount;

const REGISTRATOR_TYPE_A026: &str = "a026_wb_advert_daily";
const REGISTRATOR_TYPE_A012: &str = "a012_wb_sales";
const TURNOVER_RESERVE: &str = "advert_clicks_order_accrual";
const TURNOVER_EXPENSE: &str = "advert_clicks_order_expense";

fn now_str() -> String {
    Utc::now().to_rfc3339()
}

/// Строит reserve-строки p913 из уже вычисленных linked_orders документа a026.
///
/// Для каждого `found_order` с `is_allocated=true` создаётся одна строка.
/// Если у группы есть расход (`wb_advert_sum > 0`) но нет ни одного
/// аллоцированного заказа — создаётся строка-заглушка с `is_problem=true`.
///
/// `general_ledger_ref` — id GL-проводки advert_clicks_order_accrual, к которой привязываются
/// все построенные строки. Передаётся `None` только если документ суммарно
/// нулевой и GL-проводка не создаётся.
pub fn build_reserve_entries(
    document_id: Uuid,
    document: &WbAdvertDaily,
    general_ledger_ref: Option<&str>,
) -> Vec<Model> {
    let campaign_code = document.header.advert_id.to_string();
    let mut result = Vec::new();

    for group in &document.linked_orders {
        let allocated_orders: Vec<_> = group
            .found_orders
            .iter()
            .filter(|o| o.is_allocated && is_significant_amount(o.allocated_cost))
            .collect();

        if allocated_orders.is_empty() {
            // Нет атрибутированных заказов, но расход есть → строка-проблема.
            if is_significant_amount(group.wb_advert_sum) {
                let timestamp = now_str();
                result.push(Model {
                    id: Uuid::new_v4().to_string(),
                    connection_mp_ref: document.header.connection_id.clone(),
                    entry_date: document.header.document_date.clone(),
                    turnover_code: TURNOVER_RESERVE.to_string(),
                    amount: group.wb_advert_sum,
                    nomenclature_ref: None,
                    wb_advert_campaign_code: campaign_code.clone(),
                    order_key: String::new(),
                    registrator_type: REGISTRATOR_TYPE_A026.to_string(),
                    registrator_ref: document_id.to_string(),
                    general_ledger_ref: general_ledger_ref.map(str::to_string),
                    is_problem: true,
                    created_at: timestamp.clone(),
                    updated_at: timestamp,
                    sale_amount: 0.0,
                });
            }
        } else {
            for order in allocated_orders {
                let timestamp = now_str();
                // Сумма заказа: allocation_basis (price_with_disc / finished_price / price).
                let order_sale_amount = order.allocation_basis;
                result.push(Model {
                    id: Uuid::new_v4().to_string(),
                    connection_mp_ref: document.header.connection_id.clone(),
                    entry_date: document.header.document_date.clone(),
                    turnover_code: TURNOVER_RESERVE.to_string(),
                    amount: order.allocated_cost,
                    nomenclature_ref: order.nomenclature_ref.clone(),
                    wb_advert_campaign_code: campaign_code.clone(),
                    order_key: order.order_key.clone(),
                    registrator_type: REGISTRATOR_TYPE_A026.to_string(),
                    registrator_ref: document_id.to_string(),
                    general_ledger_ref: general_ledger_ref.map(str::to_string),
                    is_problem: false,
                    created_at: timestamp.clone(),
                    updated_at: timestamp,
                    sale_amount: order_sale_amount,
                });
            }
        }
    }

    result
}

/// Строит expense-строки p913 для Phase 2 (постинг a012).
///
/// `sale_doc_id` — UUID документа a012.
/// `order_key` — srid заказа (document.header.document_no из a012).
/// `reserve_rows` — соответствующие reserve-строки из p913.
/// `sale_finished_price` — finished_price из a012 (сумма реализации, к которой привязан расход).
/// `general_ledger_ref` — id GL-проводки advert_clicks_order_expense, к которой привязываются все expense-строки.
/// `sale_entry_date` — MSK business date реализации (wb_business_date из sale_dt a012); совпадает с GL entry_date.
pub fn build_expense_entries(
    sale_doc_id: Uuid,
    order_key: &str,
    reserve_rows: &[Model],
    sale_finished_price: f64,
    sale_entry_date: &str,
    general_ledger_ref: &str,
) -> Vec<Model> {
    let mut result = Vec::new();

    // Распределяем сумму реализации между reserve-строками пропорционально amount,
    // чтобы суммарный sale_amount по expense-строкам = sale_finished_price (с last-residual округлением).
    let total_amount: f64 = reserve_rows.iter().map(|r| r.amount).sum();
    let n = reserve_rows.len();

    let mut allocated: Vec<f64> = vec![0.0; n];
    let mut accumulated = 0.0_f64;
    for (i, reserve) in reserve_rows.iter().enumerate() {
        let raw = if total_amount > f64::EPSILON {
            sale_finished_price * (reserve.amount / total_amount)
        } else if n > 0 {
            sale_finished_price / n as f64
        } else {
            0.0
        };
        let value = if i + 1 == n {
            round_kopeyka(sale_finished_price - accumulated)
        } else {
            let rounded = round_kopeyka(raw);
            accumulated += rounded;
            rounded
        };
        allocated[i] = value;
    }

    for (i, reserve) in reserve_rows.iter().enumerate() {
        let timestamp = now_str();
        result.push(Model {
            id: Uuid::new_v4().to_string(),
            connection_mp_ref: reserve.connection_mp_ref.clone(),
            entry_date: sale_entry_date.to_string(),
            turnover_code: TURNOVER_EXPENSE.to_string(),
            amount: reserve.amount,
            nomenclature_ref: reserve.nomenclature_ref.clone(),
            wb_advert_campaign_code: reserve.wb_advert_campaign_code.clone(),
            order_key: order_key.to_string(),
            registrator_type: REGISTRATOR_TYPE_A012.to_string(),
            registrator_ref: sale_doc_id.to_string(),
            general_ledger_ref: Some(general_ledger_ref.to_string()),
            is_problem: false,
            created_at: timestamp.clone(),
            updated_at: timestamp,
            sale_amount: allocated[i],
        });
    }

    result
}

fn round_kopeyka(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}

/// Detail-строки для drill-down по ресурсу Главной книги.
pub struct GlDetail;

#[async_trait::async_trait]
impl crate::general_ledger::resource_detail::GlDetailSource for GlDetail {
    fn detail_table(&self) -> &'static str {
        "p913_wb_advert_order_attr"
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
