//! HTTP-поверхность механизма Процессов.
//!
//! Все маршруты — admin-only, и это не осторожность, а свойство предмета:
//! активация Процесса означает, что система начнёт менять данные сама, а сухой
//! прогон Этапа исполняет чужой mjs. Право на такое не выдаётся «просто
//! аутентифицированному».
//!
//! Определения живут в БД, а не в git (ADR-0011 п.6), поэтому здесь же лежит
//! то, что в обычном коде заменяет `git log` и `git diff`: история версий и
//! план активации с двухуровневым сравнением.

use axum::{
    extract::{Path, Query, State},
    Json,
};
use contracts::processes::{
    ActionInfo, ActivationPlan, DefinitionVersion, DomainEvent, DomainEventKind, EffectRecord,
    InstanceDetails, InstanceStatus, ProcessDefinition, ProcessInstance, ProcessRecord,
    StageDefinition, StageRecord, StageRun,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::processes::{definitions, effect_log, events, instances, repository, stages, steps};
use crate::shared::data::db;
use crate::shared::error::{ApiError, ApiResult};

type Db = State<sea_orm::DatabaseConnection>;

#[derive(Debug, Deserialize)]
pub struct LimitQuery {
    #[serde(default = "default_limit")]
    pub limit: u64,
}

fn default_limit() -> u64 {
    100
}

/// Паспорт вида события — каталог закрыт, поэтому UI получает его целиком.
#[derive(Debug, Serialize)]
pub struct EventKindInfo {
    pub name: &'static str,
    pub title: &'static str,
    pub correlation: Vec<&'static str>,
}

// ---------------------------------------------------------------------------
// Каталоги
// ---------------------------------------------------------------------------

/// GET /api/processes/actions
pub async fn list_actions() -> Json<Vec<&'static ActionInfo>> {
    Json(crate::processes::actions::list())
}

/// GET /api/processes/event-kinds
pub async fn list_event_kinds() -> Json<Vec<EventKindInfo>> {
    Json(
        DomainEventKind::ALL
            .into_iter()
            .map(|kind| EventKindInfo {
                name: kind.as_str(),
                title: kind.title(),
                correlation: kind.correlation_fields().to_vec(),
            })
            .collect(),
    )
}

/// GET /api/processes/events
pub async fn list_events(
    State(db): Db,
    Query(query): Query<LimitQuery>,
) -> ApiResult<Json<Vec<DomainEvent>>> {
    Ok(Json(
        events::list_recent(&db, query.limit.clamp(1, 500)).await?,
    ))
}

// ---------------------------------------------------------------------------
// Этапы
// ---------------------------------------------------------------------------

/// GET /api/processes/stages
pub async fn list_stages(State(db): Db) -> ApiResult<Json<Vec<DefinitionVersion>>> {
    Ok(Json(repository::list_stage_heads(&db).await?))
}

/// GET /api/processes/stages/full — головные версии вместе с манифестом и кодом.
///
/// Существует ради карточек: «что внутри Этапа» — это выходы, права и mjs, и
/// собирать их отдельным запросом на каждый код смысла нет.
pub async fn list_stages_full(State(db): Db) -> ApiResult<Json<Vec<StageRecord>>> {
    Ok(Json(repository::list_stage_head_records(&db).await?))
}

/// GET /api/processes/stages/:code/versions
pub async fn list_stage_versions(
    State(db): Db,
    Path(code): Path<String>,
) -> ApiResult<Json<Vec<DefinitionVersion>>> {
    Ok(Json(repository::list_stage_versions(&db, &code).await?))
}

/// GET /api/processes/stages/:code/versions/:version
pub async fn get_stage(
    State(db): Db,
    Path((code, version)): Path<(String, i32)>,
) -> ApiResult<Json<StageRecord>> {
    repository::find_stage(&db, &code, version)
        .await?
        .map(Json)
        .ok_or_else(|| ApiError::not_found(format!("версия Этапа {code} v{version} не найдена")))
}

/// POST /api/processes/stages — сохранить черновик.
pub async fn save_stage(
    State(db): Db,
    Json(definition): Json<StageDefinition>,
) -> ApiResult<Json<StageRecord>> {
    Ok(Json(definitions::save_stage(&db, definition, None).await?))
}

/// POST /api/processes/stages/:code/versions/:version/activate
pub async fn activate_stage(
    State(db): Db,
    Path((code, version)): Path<(String, i32)>,
) -> ApiResult<Json<StageRecord>> {
    Ok(Json(
        definitions::activate_stage(&db, &code, version).await?,
    ))
}

/// DELETE /api/processes/stages/:code/versions/:version — только черновик.
pub async fn delete_stage(
    State(db): Db,
    Path((code, version)): Path<(String, i32)>,
) -> ApiResult<Json<Value>> {
    repository::delete_stage_draft(&db, &code, version).await?;
    Ok(Json(serde_json::json!({ "deleted": true })))
}

#[derive(Debug, Deserialize)]
pub struct DryRunRequest {
    #[serde(default)]
    pub input: Value,
}

/// POST /api/processes/stages/:code/versions/:version/dry-run
///
/// Допуск в работу (ADR-0011 п.8): Этап исполняется по-настоящему, но все его
/// Действия идут сухим прогоном и оставляют план в журнале эффектов. Режим
/// задаётся здесь, снаружи, и внутрь не прокидывается — автор mjs не может его
/// переопределить.
pub async fn dry_run_stage(
    Path((code, version)): Path<(String, i32)>,
    body: Option<Json<DryRunRequest>>,
) -> ApiResult<Json<StageRun>> {
    // Прогон Этапа требует `'static`: обработчик Действий уходит в рантайм
    // QuickJS и переживает вызов. Это единственное место поверхности, где
    // соединение берётся мостом, а не экстрактором.
    let db = db::get_connection();
    let record = repository::find_stage(db, &code, version)
        .await?
        .ok_or_else(|| ApiError::not_found(format!("версия Этапа {code} v{version} не найдена")))?;
    let input = body.map(|Json(body)| body.input).unwrap_or(Value::Null);
    let context =
        contracts::processes::StageRunContext::manual(contracts::processes::ActionMode::DryRun);
    Ok(Json(
        stages::run(db, &record.definition, input, &context).await?,
    ))
}

// ---------------------------------------------------------------------------
// Процессы
// ---------------------------------------------------------------------------

/// GET /api/processes/definitions
pub async fn list_processes(State(db): Db) -> ApiResult<Json<Vec<DefinitionVersion>>> {
    Ok(Json(repository::list_process_heads(&db).await?))
}

/// GET /api/processes/definitions/full — головные версии вместе с графом.
pub async fn list_processes_full(State(db): Db) -> ApiResult<Json<Vec<ProcessRecord>>> {
    Ok(Json(repository::list_process_head_records(&db).await?))
}

/// GET /api/processes/definitions/:code/versions
pub async fn list_process_versions(
    State(db): Db,
    Path(code): Path<String>,
) -> ApiResult<Json<Vec<DefinitionVersion>>> {
    Ok(Json(repository::list_process_versions(&db, &code).await?))
}

/// GET /api/processes/definitions/:code/versions/:version
pub async fn get_process(
    State(db): Db,
    Path((code, version)): Path<(String, i32)>,
) -> ApiResult<Json<ProcessRecord>> {
    repository::find_process(&db, &code, version)
        .await?
        .map(Json)
        .ok_or_else(|| ApiError::not_found(format!("версия Процесса {code} v{version} не найдена")))
}

/// POST /api/processes/definitions — сохранить черновик.
pub async fn save_process(
    State(db): Db,
    Json(definition): Json<ProcessDefinition>,
) -> ApiResult<Json<ProcessRecord>> {
    Ok(Json(
        definitions::save_process(&db, definition, None).await?,
    ))
}

/// GET /api/processes/definitions/:code/versions/:version/activation-plan
///
/// То, что человек обязан увидеть до активации: двухуровневый diff, критичность
/// и список причин, по которым активации не будет.
pub async fn activation_plan(
    State(db): Db,
    Path((code, version)): Path<(String, i32)>,
) -> ApiResult<Json<ActivationPlan>> {
    Ok(Json(
        definitions::activation_plan(&db, &code, version).await?,
    ))
}

/// POST /api/processes/definitions/:code/versions/:version/activate
pub async fn activate_process(
    State(db): Db,
    Path((code, version)): Path<(String, i32)>,
) -> ApiResult<Json<ActivationPlan>> {
    Ok(Json(
        definitions::activate_process(&db, &code, version).await?,
    ))
}

/// POST /api/processes/definitions/:code/deactivate
pub async fn deactivate_process(State(db): Db, Path(code): Path<String>) -> ApiResult<Json<Value>> {
    repository::deactivate_process(&db, &code).await?;
    Ok(Json(serde_json::json!({ "active": false })))
}

/// DELETE /api/processes/definitions/:code/versions/:version — только черновик.
pub async fn delete_process(
    State(db): Db,
    Path((code, version)): Path<(String, i32)>,
) -> ApiResult<Json<Value>> {
    repository::delete_process_draft(&db, &code, version).await?;
    Ok(Json(serde_json::json!({ "deleted": true })))
}

// ---------------------------------------------------------------------------
// Экземпляры
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct InstanceQuery {
    /// `waiting` — инбокс: список тех, кто ждёт человека.
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default = "default_limit")]
    pub limit: u64,
}

/// GET /api/processes/instances
pub async fn list_instances(
    State(db): Db,
    Query(query): Query<InstanceQuery>,
) -> ApiResult<Json<Vec<ProcessInstance>>> {
    let limit = query.limit.clamp(1, 500);
    let items = match query.status.as_deref().map(InstanceStatus::from_str) {
        Some(InstanceStatus::Waiting) => instances::list_waiting(&db, limit).await?,
        Some(status) => instances::list_recent(&db, limit)
            .await?
            .into_iter()
            .filter(|instance| instance.status == status)
            .collect(),
        None => instances::list_recent(&db, limit).await?,
    };
    Ok(Json(items))
}

/// GET /api/processes/instances/:id
pub async fn get_instance(
    State(db): Db,
    Path(id): Path<String>,
) -> ApiResult<Json<InstanceDetails>> {
    let instance = instances::find(&db, &id)
        .await?
        .ok_or_else(|| ApiError::not_found(format!("экземпляр {id} не найден")))?;
    let steps = steps::list_for_instance(&db, &id, 200).await?;
    let effects = effect_log::list_recent(&db, 500)
        .await?
        .into_iter()
        .filter(|record| record.process_instance_ref.as_deref() == Some(id.as_str()))
        .collect();
    Ok(Json(InstanceDetails {
        instance,
        steps,
        effects,
    }))
}

/// POST /api/processes/instances/:id/human-done
///
/// Кнопка инбокса. Публикует `human.action.done` с ключом ожидающего
/// экземпляра — тем же, который положил в тикет `request_human_action`.
/// Экземпляр она не двигает: его двинет воркер, разобрав факт. Разница
/// принципиальна — «сделано» это факт домена, а не команда экземпляру.
pub async fn human_action_done(
    State(db): Db,
    Path(id): Path<String>,
) -> ApiResult<Json<DomainEvent>> {
    let instance = instances::find(&db, &id)
        .await?
        .ok_or_else(|| ApiError::not_found(format!("экземпляр {id} не найден")))?;
    let key = instance
        .wait
        .as_ref()
        .map(|wait| wait.token.clone())
        .unwrap_or_else(|| instance.correlation_token.clone());
    let event = events::publish(
        &db,
        DomainEventKind::HumanActionDone,
        contracts::processes::CorrelationKey::new().with("request_key", key),
        serde_json::json!({ "instance_id": id }),
        "ui",
    )
    .await?;
    Ok(Json(event))
}

// ---------------------------------------------------------------------------
// Журнал эффектов и ручной проход
// ---------------------------------------------------------------------------

/// GET /api/processes/effects
pub async fn list_effects(
    State(db): Db,
    Query(query): Query<LimitQuery>,
) -> ApiResult<Json<Vec<EffectRecord>>> {
    Ok(Json(
        effect_log::list_recent(&db, query.limit.clamp(1, 500)).await?,
    ))
}

/// POST /api/processes/tick — двинуть механизм сейчас, не дожидаясь воркера.
pub async fn tick() -> ApiResult<Json<Value>> {
    let report = crate::processes::worker::tick(db::get_connection()).await?;
    Ok(Json(serde_json::json!({
        "released": report.released,
        "started": report.started,
        "woken": report.woken,
        "stages_run": report.stages_run,
        "expired": report.expired,
        "quarantined": report.quarantined,
    })))
}
