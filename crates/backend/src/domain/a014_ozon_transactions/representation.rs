//! Представление агрегата a014_ozon_transactions для сервиса представлений.

use std::collections::HashMap;

use contracts::general_ledger::AggregateRepresentation;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, QuerySelect};

use super::repository::{Column, Entity};
use crate::shared::data::db::get_connection;
use crate::shared::registrators::{Registrator, RegistratorMeta};
use crate::shared::representation::{build, chunked};
use anyhow::Result;
use async_trait::async_trait;
use uuid::Uuid;

/// Название типа (зеркалит metadata element_name; generated-метаданные устарели).
const TYPE_NAME: &str = "Транзакция OZON";

/// Батч-резолв представлений: название типа + номер отправления (posting_number).
pub async fn represent_many(ids: &[String]) -> HashMap<String, AggregateRepresentation> {
    chunked(ids, |chunk| async move {
        let rows = Entity::find()
            .select_only()
            .column(Column::Id)
            .column(Column::PostingNumber)
            .filter(Column::Id.is_in(chunk))
            .into_tuple::<(String, String)>()
            .all(get_connection())
            .await
            .unwrap_or_default();
        rows.into_iter()
            .map(|(id, posting_number)| (id, build(TYPE_NAME, None, Some(posting_number))))
            .collect()
    })
    .await
}

/// Регистратор `a014_ozon_transactions` — транзакции Ozon.
pub struct Provider;

#[async_trait]
impl Registrator for Provider {
    fn kind(&self) -> &'static str {
        "a014_ozon_transactions"
    }

    /// Ключ этого же типа в `p904_sales_data`.
    fn aliases(&self) -> &'static [&'static str] {
        &["OZON_Transactions"]
    }

    fn meta(&self) -> RegistratorMeta {
        RegistratorMeta {
            type_label: "Транзакции Ozon",
            link_label: None,
            can_post: true,
            tab_key_prefix: Some("a014_ozon_transactions_details"),
        }
    }

    async fn represent_many(&self, ids: &[String]) -> HashMap<String, AggregateRepresentation> {
        represent_many(ids).await
    }

    async fn post_document(&self, id: Uuid) -> Result<()> {
        super::posting::post_document(id).await
    }
}
