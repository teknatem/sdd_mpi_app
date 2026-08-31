//! Представление агрегата a034_ym_realization для сервиса представлений.

use std::collections::HashMap;

use contracts::domain::a034_ym_realization::ENTITY_METADATA;
use contracts::general_ledger::AggregateRepresentation;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, QuerySelect};

use super::repository::{Column, Entity};
use crate::shared::data::db::get_connection;
use crate::shared::registrators::{Registrator, RegistratorMeta, RepostOption};
use crate::shared::representation::{build, chunked};
use anyhow::Result;
use async_trait::async_trait;
use uuid::Uuid;

/// Батч-резолв представлений: название типа + дата документа + номер документа.
pub async fn represent_many(ids: &[String]) -> HashMap<String, AggregateRepresentation> {
    chunked(ids, |chunk| async move {
        let rows = Entity::find()
            .select_only()
            .column(Column::Id)
            .column(Column::DocumentDate)
            .column(Column::DocumentNo)
            .filter(Column::Id.is_in(chunk))
            .into_tuple::<(String, String, String)>()
            .all(get_connection())
            .await
            .unwrap_or_default();
        rows.into_iter()
            .map(|(id, date, doc_no)| {
                (
                    id,
                    build(ENTITY_METADATA.ui.element_name, Some(date), Some(doc_no)),
                )
            })
            .collect()
    })
    .await
}

/// Регистратор `a034_ym_realization` — отчёт о реализации Яндекс.Маркет.
pub struct Provider;

#[async_trait]
impl Registrator for Provider {
    fn kind(&self) -> &'static str {
        "a034_ym_realization"
    }

    fn meta(&self) -> RegistratorMeta {
        RegistratorMeta {
            type_label: RegistratorMeta::UNKNOWN.type_label,
            link_label: None,
            can_post: true,
            tab_key_prefix: None,
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
            label: "a034 — YM Realization",
            description: "Перепроведение документов a034_ym_realization с пересборкой GL-проводок слоя ybuh и событий реализации/возврата p915",
        })
    }

    async fn ids_in_period(
        &self,
        date_from: &str,
        date_to: &str,
        only_posted: bool,
    ) -> Result<Vec<String>> {
        super::repository::list_ids_by_period(date_from, date_to, only_posted).await
    }
}
