//! Представление проекции p903_wb_finance_report для сервиса представлений.
//!
//! Метаданных-агрегата у проекции нет — название типа задаём явно; дата = rr_dt,
//! номер = rrd_id.

use std::collections::HashMap;

use contracts::general_ledger::AggregateRepresentation;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, QuerySelect};

use super::repository::{Column, Entity};
use crate::shared::data::db::get_connection;
use crate::shared::registrators::{Registrator, RegistratorMeta};
use crate::shared::representation::{build, chunked};
use async_trait::async_trait;

const TYPE_NAME: &str = "WB Финотчёт";

/// Батч-резолв представлений: название типа + дата rr_dt + номер rrd_id.
pub async fn represent_many(ids: &[String]) -> HashMap<String, AggregateRepresentation> {
    chunked(ids, |chunk| async move {
        let rows = Entity::find()
            .select_only()
            .column(Column::Id)
            .column(Column::RrDt)
            .column(Column::RrdId)
            .filter(Column::Id.is_in(chunk))
            .into_tuple::<(String, String, i64)>()
            .all(get_connection())
            .await
            .unwrap_or_default();
        rows.into_iter()
            .map(|(id, rr_dt, rrd_id)| {
                (id, build(TYPE_NAME, Some(rr_dt), Some(rrd_id.to_string())))
            })
            .collect()
    })
    .await
}

/// Регистратор `p903_wb_finance_report` — финансовый отчёт WB.
pub struct Provider;

#[async_trait]
impl Registrator for Provider {
    fn kind(&self) -> &'static str {
        "p903_wb_finance_report"
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
