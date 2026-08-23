//! Журнал эффектов — репозиторий над `sys_effect_log`.
//!
//! Единственное место, где решается вопрос «это уже делали?». Идемпотентность
//! держится уникальным индексом БД, а не проверкой в коде: два воркера,
//! одновременно взявшие один экземпляр процесса, обязаны разойтись на уровне
//! базы, а не на удачном порядке чтений.

use anyhow::Result;
use chrono::Utc;
use contracts::processes::{ActionActor, ActionMode, EffectRecord, EffectStatus};
use sea_orm::entity::prelude::*;
use sea_orm::{
    ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder, QuerySelect, Set,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "sys_effect_log")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub idempotency_key: String,
    pub action_name: String,
    pub mode: String,
    pub status: String,
    pub input_json: String,
    #[sea_orm(nullable)]
    pub result_json: Option<String>,
    #[sea_orm(nullable)]
    pub error_text: Option<String>,
    pub actor: String,
    #[sea_orm(nullable)]
    pub process_instance_ref: Option<String>,
    #[sea_orm(nullable)]
    pub stage_code: Option<String>,
    pub started_at: String,
    #[sea_orm(nullable)]
    pub finished_at: Option<String>,
    #[sea_orm(nullable)]
    pub duration_ms: Option<i64>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}

impl From<Model> for EffectRecord {
    fn from(m: Model) -> Self {
        EffectRecord {
            id: m.id,
            idempotency_key: m.idempotency_key,
            action_name: m.action_name,
            mode: if m.mode == ActionMode::DryRun.as_str() {
                ActionMode::DryRun
            } else {
                ActionMode::Execute
            },
            status: EffectStatus::from_str(&m.status),
            input: serde_json::from_str(&m.input_json).unwrap_or(Value::Null),
            result: m
                .result_json
                .and_then(|raw| serde_json::from_str(&raw).ok()),
            error_text: m.error_text,
            actor: m.actor,
            process_instance_ref: m.process_instance_ref,
            stage_code: m.stage_code,
            started_at: m.started_at,
            finished_at: m.finished_at,
            duration_ms: m.duration_ms,
        }
    }
}

/// Найти боевую запись по ключу идемпотентности.
///
/// Планы сухого прогона сюда не попадают: они ключ не занимают, иначе просмотр
/// плана закрыл бы дорогу настоящему исполнению с тем же ключом.
pub async fn find_executed_by_key(
    db: &DatabaseConnection,
    idempotency_key: &str,
) -> Result<Option<Model>> {
    Ok(Entity::find()
        .filter(Column::IdempotencyKey.eq(idempotency_key))
        .filter(Column::Mode.eq(ActionMode::Execute.as_str()))
        .one(db)
        .await?)
}

/// Занять ключ: вставить строку `in_progress` ДО исполнения.
///
/// Порядок именно такой. Запись после исполнения означала бы, что падение между
/// эффектом и записью делает эффект невидимым — а повтор сделает его вторым.
pub async fn claim(
    db: &DatabaseConnection,
    action_name: &str,
    idempotency_key: &str,
    input: &Value,
    actor: &ActionActor,
) -> Result<Model> {
    let now = Utc::now().to_rfc3339();
    let model = ActiveModel {
        id: Set(Uuid::new_v4().to_string()),
        idempotency_key: Set(idempotency_key.to_string()),
        action_name: Set(action_name.to_string()),
        mode: Set(ActionMode::Execute.as_str().to_string()),
        status: Set(EffectStatus::InProgress.as_str().to_string()),
        input_json: Set(input.to_string()),
        result_json: Set(None),
        error_text: Set(None),
        actor: Set(actor.as_token()),
        process_instance_ref: Set(actor.instance_id().map(str::to_string)),
        stage_code: Set(actor.stage_code().map(str::to_string)),
        started_at: Set(now),
        finished_at: Set(None),
        duration_ms: Set(None),
    };
    Ok(model.insert(db).await?)
}

/// Перезанять ключ после неудачи: та же строка возвращается в работу.
///
/// Отдельная строка на попытку не заводится намеренно — иначе уникальный индекс
/// по ключу пришлось бы снять, а вместе с ним и саму гарантию.
pub async fn reclaim(db: &DatabaseConnection, id: &str) -> Result<()> {
    let Some(model) = Entity::find_by_id(id.to_string()).one(db).await? else {
        anyhow::bail!("запись журнала эффектов не найдена: {id}");
    };
    let mut active: ActiveModel = model.into();
    active.status = Set(EffectStatus::InProgress.as_str().to_string());
    active.started_at = Set(Utc::now().to_rfc3339());
    active.finished_at = Set(None);
    active.duration_ms = Set(None);
    active.error_text = Set(None);
    active.update(db).await?;
    Ok(())
}

/// Записать план сухого прогона. Ключ не занимает (см. `find_executed_by_key`).
pub async fn record_plan(
    db: &DatabaseConnection,
    action_name: &str,
    idempotency_key: &str,
    input: &Value,
    plan: &Value,
    actor: &ActionActor,
    duration_ms: i64,
) -> Result<Model> {
    let now = Utc::now().to_rfc3339();
    let model = ActiveModel {
        id: Set(Uuid::new_v4().to_string()),
        idempotency_key: Set(idempotency_key.to_string()),
        action_name: Set(action_name.to_string()),
        mode: Set(ActionMode::DryRun.as_str().to_string()),
        status: Set(EffectStatus::Planned.as_str().to_string()),
        input_json: Set(input.to_string()),
        result_json: Set(Some(plan.to_string())),
        error_text: Set(None),
        actor: Set(actor.as_token()),
        process_instance_ref: Set(actor.instance_id().map(str::to_string)),
        stage_code: Set(actor.stage_code().map(str::to_string)),
        started_at: Set(now.clone()),
        finished_at: Set(Some(now)),
        duration_ms: Set(Some(duration_ms)),
    };
    Ok(model.insert(db).await?)
}

/// Закрыть запись успехом.
pub async fn mark_executed(
    db: &DatabaseConnection,
    id: &str,
    result: &Value,
    duration_ms: i64,
) -> Result<()> {
    finish(
        db,
        id,
        EffectStatus::Executed,
        Some(result),
        None,
        duration_ms,
    )
    .await
}

/// Закрыть запись неудачей. Повтор с тем же ключом после этого разрешён.
pub async fn mark_failed(
    db: &DatabaseConnection,
    id: &str,
    error: &str,
    duration_ms: i64,
) -> Result<()> {
    finish(db, id, EffectStatus::Failed, None, Some(error), duration_ms).await
}

async fn finish(
    db: &DatabaseConnection,
    id: &str,
    status: EffectStatus,
    result: Option<&Value>,
    error: Option<&str>,
    duration_ms: i64,
) -> Result<()> {
    let Some(model) = Entity::find_by_id(id.to_string()).one(db).await? else {
        anyhow::bail!("запись журнала эффектов не найдена: {id}");
    };
    let mut active: ActiveModel = model.into();
    active.status = Set(status.as_str().to_string());
    active.result_json = Set(result.map(|value| value.to_string()));
    active.error_text = Set(error.map(str::to_string));
    active.finished_at = Set(Some(Utc::now().to_rfc3339()));
    active.duration_ms = Set(Some(duration_ms));
    active.update(db).await?;
    Ok(())
}

/// Последние записи журнала — для экрана разбора и для тестов.
pub async fn list_recent(db: &DatabaseConnection, limit: u64) -> Result<Vec<EffectRecord>> {
    Ok(Entity::find()
        .order_by_desc(Column::StartedAt)
        .limit(limit)
        .all(db)
        .await?
        .into_iter()
        .map(EffectRecord::from)
        .collect())
}
