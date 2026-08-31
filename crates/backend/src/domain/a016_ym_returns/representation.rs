//! Представление агрегата a016_ym_returns для сервиса представлений.

use std::collections::HashMap;

use contracts::general_ledger::AggregateRepresentation;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, QuerySelect};

use super::repository::{Column, Entity};
use crate::shared::data::db::get_connection;
use crate::shared::registrators::{Registrator, RegistratorMeta, RepostOption};
use crate::shared::representation::{build, chunked};
use anyhow::Result;
use async_trait::async_trait;
use uuid::Uuid;

/// Название типа (зеркалит metadata element_name; generated-метаданные устарели).
const TYPE_NAME: &str = "Возврат Yandex Market";

/// Батч-резолв представлений: название типа + id возврата (return_id).
pub async fn represent_many(ids: &[String]) -> HashMap<String, AggregateRepresentation> {
    chunked(ids, |chunk| async move {
        let rows = Entity::find()
            .select_only()
            .column(Column::Id)
            .column(Column::ReturnId)
            .filter(Column::Id.is_in(chunk))
            .into_tuple::<(String, i64)>()
            .all(get_connection())
            .await
            .unwrap_or_default();
        rows.into_iter()
            .map(|(id, return_id)| (id, build(TYPE_NAME, None, Some(return_id.to_string()))))
            .collect()
    })
    .await
}

/// Регистратор `a016_ym_returns` — возвраты Яндекс.Маркет.
pub struct Provider;

#[async_trait]
impl Registrator for Provider {
    fn kind(&self) -> &'static str {
        "a016_ym_returns"
    }

    /// Ключ этого же типа в `p904_sales_data`.
    fn aliases(&self) -> &'static [&'static str] {
        &["YM_Returns"]
    }

    fn meta(&self) -> RegistratorMeta {
        RegistratorMeta {
            type_label: "Возвраты Яндекс.Маркет",
            link_label: None,
            can_post: true,
            tab_key_prefix: Some("a016_ym_returns_details"),
        }
    }

    async fn represent_many(&self, ids: &[String]) -> HashMap<String, AggregateRepresentation> {
        represent_many(ids).await
    }

    async fn post_document(&self, id: Uuid) -> Result<()> {
        super::posting::post_document(id).await
    }

    fn repost_option(&self) -> Option<RepostOption> {
        Some(RepostOption {
            label: "a016 — YM Returns",
            description: "Перепроведение возвратов a016_ym_returns с пересборкой p904 и движений возврата в воронке p916 (только return_type=RETURN)",
        })
    }

    async fn ids_in_period(
        &self,
        date_from: &str,
        date_to: &str,
        only_posted: bool,
    ) -> Result<Vec<String>> {
        let order_numbers: Vec<i64> =
            crate::domain::a013_ym_order::repository::list_ids_by_creation_period(
                date_from,
                date_to,
                &[],
                false,
            )
            .await?
            .into_iter()
            .filter_map(|(_, document_no)| document_no.parse::<i64>().ok())
            .collect();
        super::repository::list_ids_by_order_ids(&order_numbers, only_posted).await
    }
}
