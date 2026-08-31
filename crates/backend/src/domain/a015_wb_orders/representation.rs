//! Представление агрегата a015_wb_orders для сервиса представлений.

use std::collections::HashMap;

use contracts::domain::a015_wb_orders::ENTITY_METADATA;
use contracts::general_ledger::AggregateRepresentation;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, QuerySelect};

use super::repository::{Column, Entity};
use crate::shared::data::db::get_connection;
use crate::shared::registrators::{Registrator, RegistratorMeta, RepostOption};
use crate::shared::representation::{build, chunked};
use anyhow::Result;
use async_trait::async_trait;
use uuid::Uuid;

/// Батч-резолв представлений: название типа + дата заказа + номер документа.
pub async fn represent_many(ids: &[String]) -> HashMap<String, AggregateRepresentation> {
    chunked(ids, |chunk| async move {
        let rows = Entity::find()
            .select_only()
            .column(Column::Id)
            .column(Column::DocumentDate)
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

/// Регистратор `a015_wb_orders` — заказы Wildberries.
pub struct Provider;

#[async_trait]
impl Registrator for Provider {
    fn kind(&self) -> &'static str {
        "a015_wb_orders"
    }

    fn meta(&self) -> RegistratorMeta {
        RegistratorMeta {
            type_label: "Заказы Wildberries",
            link_label: Some("Заказ"),
            can_post: true,
            tab_key_prefix: Some("a015_wb_orders_details"),
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
            label: "a015 — WB Orders",
            description: "Перепроведение документов a015_wb_orders с пересборкой строк p909",
        })
    }

    async fn ids_in_period(
        &self,
        date_from: &str,
        date_to: &str,
        only_posted: bool,
    ) -> Result<Vec<String>> {
        super::repository::list_ids_by_date_range(date_from, date_to, only_posted).await
    }
}
