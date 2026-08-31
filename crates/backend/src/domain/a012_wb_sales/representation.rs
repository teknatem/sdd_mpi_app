//! Представление агрегата a012_wb_sales для сервиса представлений.

use std::collections::HashMap;

use contracts::domain::a012_wb_sales::ENTITY_METADATA;
use contracts::general_ledger::AggregateRepresentation;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, QuerySelect};

use super::repository::{Column, Entity};
use crate::shared::data::db::get_connection;
use crate::shared::registrators::{Registrator, RegistratorMeta, RepostOption};
use crate::shared::representation::{build, chunked};
use anyhow::Result;
use async_trait::async_trait;
use uuid::Uuid;

/// Батч-резолв представлений: название типа + дата продажи + номер документа.
pub async fn represent_many(ids: &[String]) -> HashMap<String, AggregateRepresentation> {
    chunked(ids, |chunk| async move {
        let rows = Entity::find()
            .select_only()
            .column(Column::Id)
            .column(Column::SaleDate)
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

/// Регистратор `a012_wb_sales` — продажи WB, проводятся в Главную книгу.
pub struct Provider;

#[async_trait]
impl Registrator for Provider {
    fn kind(&self) -> &'static str {
        "a012_wb_sales"
    }

    /// Ключ этого же типа в `p904_sales_data`.
    fn aliases(&self) -> &'static [&'static str] {
        &["WB_Sales"]
    }

    fn meta(&self) -> RegistratorMeta {
        RegistratorMeta {
            type_label: "Продажи WB",
            link_label: Some("Продажа"),
            can_post: true,
            tab_key_prefix: Some("a012_wb_sales_details"),
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
            label: "a012 — WB Sales",
            description: "Перепроведение документов a012_wb_sales с пересборкой связанных проекций",
        })
    }
}
