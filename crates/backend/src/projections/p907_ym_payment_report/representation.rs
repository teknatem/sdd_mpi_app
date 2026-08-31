//! Представление проекции p907_ym_payment_report для сервиса представлений.
//!
//! PK таблицы — `record_key`, но `registrator_ref` в GL = колонка `id`, поэтому
//! фильтруем по `id`. Метаданных-агрегата нет — название типа задаём явно; дата =
//! transaction_date, номер = transaction_id (или order_id).

use std::collections::HashMap;

use contracts::general_ledger::AggregateRepresentation;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, QuerySelect};

use super::repository::{Column, Entity};
use crate::shared::data::db::get_connection;
use crate::shared::registrators::{Registrator, RegistratorMeta};
use crate::shared::representation::{build, chunked};
use async_trait::async_trait;

const TYPE_NAME: &str = "YM Платёж";

/// Батч-резолв представлений: название типа + дата транзакции + transaction_id.
pub async fn represent_many(ids: &[String]) -> HashMap<String, AggregateRepresentation> {
    chunked(ids, |chunk| async move {
        let rows = Entity::find()
            .select_only()
            .column(Column::Id)
            .column(Column::TransactionDate)
            .column(Column::TransactionId)
            .column(Column::OrderId)
            .filter(Column::Id.is_in(chunk))
            .into_tuple::<(String, Option<String>, Option<String>, Option<i64>)>()
            .all(get_connection())
            .await
            .unwrap_or_default();
        rows.into_iter()
            .map(|(id, tx_date, tx_id, order_id)| {
                let doc_id = tx_id
                    .filter(|s| !s.trim().is_empty())
                    .or_else(|| order_id.map(|o| o.to_string()));
                (id, build(TYPE_NAME, tx_date, doc_id))
            })
            .collect()
    })
    .await
}

/// Регистратор `p907_ym_payment_report` — отчёт по выплатам Яндекс.Маркет.
pub struct Provider;

#[async_trait]
impl Registrator for Provider {
    fn kind(&self) -> &'static str {
        "p907_ym_payment_report"
    }

    fn meta(&self) -> RegistratorMeta {
        RegistratorMeta {
            type_label: RegistratorMeta::UNKNOWN.type_label,
            link_label: None,
            can_post: false,
            tab_key_prefix: None,
        }
    }

    async fn represent_many(&self, ids: &[String]) -> HashMap<String, AggregateRepresentation> {
        represent_many(ids).await
    }
}
