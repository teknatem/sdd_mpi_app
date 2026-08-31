//! Представление агрегата a036_wb_sales_funnel_daily для сервиса представлений.

use std::collections::HashMap;

use contracts::domain::a036_wb_sales_funnel_daily::ENTITY_METADATA;
use contracts::general_ledger::AggregateRepresentation;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, QuerySelect};

use super::repository::{Column, Entity};
use crate::shared::data::db::get_connection;
use crate::shared::registrators::{Registrator, RegistratorMeta};
use crate::shared::representation::{build, chunked};
use async_trait::async_trait;

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

/// Регистратор `a036_wb_sales_funnel_daily` — воронка продаж WB за день.
pub struct Provider;

#[async_trait]
impl Registrator for Provider {
    fn kind(&self) -> &'static str {
        "a036_wb_sales_funnel_daily"
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
