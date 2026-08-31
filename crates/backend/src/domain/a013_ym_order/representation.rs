//! Представление агрегата a013_ym_order для сервиса представлений.

use std::collections::HashMap;

use contracts::domain::a013_ym_order::ENTITY_METADATA;
use contracts::general_ledger::AggregateRepresentation;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, QuerySelect};

use super::repository::{Column, Entity};
use crate::shared::data::db::get_connection;
use crate::shared::registrators::{Registrator, RegistratorMeta, RepostOption};
use crate::shared::representation::{build, chunked};
use anyhow::Result;
use async_trait::async_trait;
use uuid::Uuid;

/// Батч-резолв представлений: название типа + дата создания + номер документа.
pub async fn represent_many(ids: &[String]) -> HashMap<String, AggregateRepresentation> {
    chunked(ids, |chunk| async move {
        let rows = Entity::find()
            .select_only()
            .column(Column::Id)
            .column(Column::CreationDate)
            .column(Column::DocumentNo)
            .filter(Column::Id.is_in(chunk))
            .into_tuple::<(String, Option<String>, String)>()
            .all(get_connection())
            .await
            .unwrap_or_default();
        rows.into_iter()
            .map(|(id, date, doc_no)| {
                (
                    id,
                    build(ENTITY_METADATA.ui.element_name, date, Some(doc_no)),
                )
            })
            .collect()
    })
    .await
}

/// Регистратор `a013_ym_order` — заказы Яндекс.Маркет.
pub struct Provider;

#[async_trait]
impl Registrator for Provider {
    fn kind(&self) -> &'static str {
        "a013_ym_order"
    }

    /// Ключ этого же типа в `p904_sales_data`.
    fn aliases(&self) -> &'static [&'static str] {
        &["YM_Order"]
    }

    fn meta(&self) -> RegistratorMeta {
        RegistratorMeta {
            type_label: "Заказы Яндекс.Маркет",
            link_label: None,
            can_post: true,
            tab_key_prefix: Some("a013_ym_order_details"),
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
            label: "a013 — YM Order",
            description: "Перепроведение заказов a013_ym_order с пересборкой p900/p904/p915 и движений воронки p916 (заказы, отмены, выкупы)",
        })
    }

    async fn ids_in_period(
        &self,
        date_from: &str,
        date_to: &str,
        only_posted: bool,
    ) -> Result<Vec<String>> {
        Ok(
            super::repository::list_ids_by_creation_period(date_from, date_to, &[], only_posted)
                .await?
                .into_iter()
                .map(|(id, _)| id)
                .collect(),
        )
    }
}

/// Резолвер реквизита `marketplace_order_ref`.
///
/// Заказ Яндекс.Маркет — единственный тип заказа, на который ссылаются по
/// обобщённому имени реквизита: у WB ссылка идёт на документ, а не на заказ.
pub struct RefResolver;

#[async_trait]
impl crate::shared::representation::ReferenceResolver for RefResolver {
    fn ref_kind(&self) -> &'static str {
        "marketplace_order_ref"
    }

    async fn represent(&self, id: uuid::Uuid) -> Option<String> {
        let item = super::service::get_by_id(id).await.ok()??;
        crate::shared::representation::pick(&item.base.description, &item.header.document_no)
    }
}
