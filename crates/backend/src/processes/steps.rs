//! Журнал шагов экземпляра.
//!
//! Пишется воркером после каждого прогона Этапа и только читается дальше. Это
//! не дубль журнала эффектов: там изменения мира, здесь решения. Этап, который
//! ничего не менял, в журнале эффектов не оставит ни строки — а его выход
//! определил, куда пошёл процесс, и разбирать прогон без этого невозможно.

use anyhow::Result;
use chrono::Utc;
use contracts::processes::{InstanceStep, StageRun, StageVerdict};
use sea_orm::entity::prelude::*;
use sea_orm::{
    ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder, QuerySelect, Set,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "sys_process_step")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub instance_ref: String,
    pub stage_code: String,
    pub visit: i32,
    pub verdict: String,
    #[sea_orm(nullable)]
    pub outcome: Option<String>,
    #[sea_orm(nullable)]
    pub data_json: Option<String>,
    #[sea_orm(nullable)]
    pub message: Option<String>,
    pub logs_json: String,
    pub effects_json: String,
    pub duration_ms: i64,
    pub created_at: String,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}

impl From<Model> for InstanceStep {
    fn from(row: Model) -> Self {
        InstanceStep {
            id: row.id,
            instance_ref: row.instance_ref,
            stage_code: row.stage_code,
            visit: row.visit,
            verdict: row.verdict,
            outcome: row.outcome,
            data: row
                .data_json
                .as_deref()
                .and_then(|raw| serde_json::from_str(raw).ok()),
            message: row.message,
            logs: serde_json::from_str(&row.logs_json).unwrap_or_default(),
            effect_ids: serde_json::from_str(&row.effects_json).unwrap_or_default(),
            duration_ms: row.duration_ms,
            created_at: row.created_at,
        }
    }
}

/// Записать отработавший шаг.
///
/// Класс исхода кладётся отдельной колонкой, а не выводится из наличия выхода:
/// три класса разделены механизмом (ADR-0011 п.10), и экран разбора обязан
/// показывать их так же, как их различает воркер.
pub async fn record(
    db: &DatabaseConnection,
    instance_id: &str,
    visit: i32,
    run: &StageRun,
) -> Result<InstanceStep> {
    let (verdict, outcome, data, message) = match &run.verdict {
        StageVerdict::Outcome(value) => (
            "outcome",
            Some(value.outcome.clone()),
            Some(value.data.clone()),
            None,
        ),
        StageVerdict::TemporaryFailure { message } => {
            ("temporary_failure", None, None, Some(message.clone()))
        }
        StageVerdict::Defect { message } => ("defect", None, None, Some(message.clone())),
    };

    let row = ActiveModel {
        id: Set(Uuid::new_v4().to_string()),
        instance_ref: Set(instance_id.to_string()),
        stage_code: Set(run.stage_code.clone()),
        visit: Set(visit),
        verdict: Set(verdict.to_string()),
        outcome: Set(outcome),
        data_json: Set(data.as_ref().map(Value::to_string)),
        message: Set(message),
        logs_json: Set(serde_json::to_string(&run.logs).unwrap_or_else(|_| "[]".into())),
        effects_json: Set(serde_json::to_string(&run.effect_ids).unwrap_or_else(|_| "[]".into())),
        duration_ms: Set(run.duration_ms),
        created_at: Set(Utc::now().to_rfc3339()),
    };
    Ok(row.insert(db).await?.into())
}

/// Шаги экземпляра в порядке исполнения.
pub async fn list_for_instance(
    db: &DatabaseConnection,
    instance_id: &str,
    limit: u64,
) -> Result<Vec<InstanceStep>> {
    Ok(Entity::find()
        .filter(Column::InstanceRef.eq(instance_id))
        .order_by_asc(Column::CreatedAt)
        .limit(limit)
        .all(db)
        .await?
        .into_iter()
        .map(InstanceStep::from)
        .collect())
}
