//! Представление агрегата a026_wb_advert_daily для сервиса представлений.

use std::collections::HashMap;

use contracts::domain::a026_wb_advert_daily::ENTITY_METADATA;
use contracts::general_ledger::AggregateRepresentation;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, QuerySelect};

use super::repository::{Column, Entity};
use crate::shared::data::db::get_connection;
use crate::shared::registrators::{Registrator, RegistratorMeta, RepostOption};
use crate::shared::representation::{build, chunked};
use anyhow::Result;
use async_trait::async_trait;
use contracts::quality::SourceColumn;
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

/// Регистратор `a026_wb_advert_daily` — ежедневная статистика рекламы WB.
pub struct Provider;

#[async_trait]
impl Registrator for Provider {
    fn kind(&self) -> &'static str {
        "a026_wb_advert_daily"
    }

    fn meta(&self) -> RegistratorMeta {
        RegistratorMeta {
            type_label: "Реклама WB (день)",
            link_label: Some("Реклама"),
            can_post: true,
            tab_key_prefix: Some("a026_wb_advert_daily_details"),
        }
    }

    async fn represent_many(&self, ids: &[String]) -> HashMap<String, AggregateRepresentation> {
        represent_many(ids).await
    }

    async fn post_document(&self, id: Uuid) -> Result<()> {
        super::posting::post_document(id).await
    }

    /// Колонки как в списке «Статистика рекламы WB»:
    /// Дата · Документ · Кампания · Кабинет · Расход.
    async fn source_columns(&self, registrator_ref: &str) -> Vec<SourceColumn> {
        source_columns(registrator_ref).await.unwrap_or_default()
    }

    fn repost_option(&self) -> Option<RepostOption> {
        Some(RepostOption {
            label: "a026 — WB Advert Daily",
            description: "Перепроведение проведённых документов a026_wb_advert_daily с пересборкой связанных проекций",
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

/// Колонки исходного документа для drill-down по нарушению quality-check,
/// смоделированные по списку «Статистика рекламы WB»:
/// Дата · Документ · Кампания · Кабинет · Расход.
async fn source_columns(registrator_ref: &str) -> anyhow::Result<Vec<SourceColumn>> {
    use sea_orm::{ConnectionTrait, Statement};

    let document_id = crate::shared::registrators::document_id(registrator_ref);
    let sql = r#"
        SELECT
            a.document_date,
            a.document_no,
            a.advert_id,
            a.total_sum,
            COALESCE(c.description, a.connection_id) AS connection_name
        FROM a026_wb_advert_daily a
        LEFT JOIN a006_connection_mp c ON a.connection_id = c.id
        WHERE a.id = ?
        LIMIT 1
    "#;
    let rows = get_connection()
        .query_all(Statement::from_sql_and_values(
            sea_orm::DatabaseBackend::Sqlite,
            sql,
            [document_id.into()],
        ))
        .await?;
    let Some(row) = rows.first() else {
        return Ok(Vec::new());
    };

    let document_date: String = row.try_get("", "document_date").unwrap_or_default();
    let document_no: String = row.try_get("", "document_no").unwrap_or_default();
    let advert_id: i64 = row.try_get("", "advert_id").unwrap_or(0);
    let total_sum: f64 = row.try_get("", "total_sum").unwrap_or(0.0);
    let connection_name: String = row.try_get("", "connection_name").unwrap_or_default();

    // Дата — только дата без времени.
    let date_only = document_date
        .split(['T', ' '])
        .next()
        .unwrap_or(&document_date)
        .to_string();
    let campaign = if advert_id > 0 {
        advert_id.to_string()
    } else {
        "—".to_string()
    };

    Ok(vec![
        SourceColumn {
            label: "Дата".to_string(),
            value: date_only,
            align_right: false,
        },
        SourceColumn {
            label: "Документ".to_string(),
            value: document_no,
            align_right: false,
        },
        SourceColumn {
            label: "Кампания".to_string(),
            value: campaign,
            align_right: true,
        },
        SourceColumn {
            label: "Кабинет".to_string(),
            value: connection_name,
            align_right: false,
        },
        SourceColumn {
            label: "Расход".to_string(),
            value: format!("{total_sum:.2}"),
            align_right: true,
        },
    ])
}
