//! Запросы механизма Процессов. Все маршруты admin-only: не-админ получит 403,
//! и это правильный ответ — фронт лишь не показывает ему пункт меню.

use contracts::processes::{
    ActivationPlan, DefinitionVersion, DomainEvent, EffectRecord, InstanceDetails, ProcessInstance,
    ProcessRecord, StageDefinition, StageRecord,
};
use serde::Deserialize;

use crate::shared::api_utils::{get_json, post_json};

/// Паспорт вида события: каталог закрыт, поэтому приходит целиком.
#[derive(Debug, Clone, Deserialize)]
pub struct EventKindInfo {
    pub name: String,
    pub title: String,
    pub correlation: Vec<String>,
}

/// Паспорт Действия с владеющими строками.
///
/// Дубль `contracts::processes::ActionInfo` намеренный: там поля — `&'static
/// str`, потому что каталог Действий живёт константой в Rust и наружу только
/// сериализуется. Разобрать такой тип из тела ответа нельзя, и подменять в
/// контракте `&'static str` на `String` ради фронта означало бы аллокации в
/// каталоге, который их не делает ни разу.
#[derive(Debug, Clone, Deserialize)]
pub struct ActionInfo {
    pub name: String,
    /// Как называется в mjs: `host.actions.<method>`.
    pub method: String,
    pub title: String,
    pub description: String,
    /// Capability, без которой Этап не может это вызвать: `action:<name>`.
    pub capability: String,
    /// Обратимо ли Действие отдельным обратным эффектом.
    pub reversible: bool,
    /// Таблицы, в которые Действие пишет.
    #[serde(default)]
    pub write_tables: Vec<String>,
    pub input_schema: serde_json::Value,
}

/// Итог ручного прохода воркера.
#[derive(Debug, Clone, Deserialize)]
pub struct TickReport {
    pub released: u64,
    pub started: u64,
    pub woken: u64,
    pub stages_run: u64,
    pub expired: u64,
    pub quarantined: u64,
}

pub async fn list_instances(status: Option<&str>) -> Result<Vec<ProcessInstance>, String> {
    let query = status
        .map(|status| format!("?status={status}"))
        .unwrap_or_default();
    get_json(&format!("/api/processes/instances{query}")).await
}

pub async fn get_instance(id: &str) -> Result<InstanceDetails, String> {
    get_json(&format!("/api/processes/instances/{id}")).await
}

/// Кнопка инбокса. Публикует факт «человек сделал»; экземпляр двинет воркер.
pub async fn human_action_done(id: &str) -> Result<DomainEvent, String> {
    post_json(&format!("/api/processes/instances/{id}/human-done"), &()).await
}

pub async fn list_processes() -> Result<Vec<DefinitionVersion>, String> {
    get_json("/api/processes/definitions").await
}

/// Головные версии Процессов вместе с графом — то, из чего собирается карточка.
pub async fn list_processes_full() -> Result<Vec<ProcessRecord>, String> {
    get_json("/api/processes/definitions/full").await
}

pub async fn list_process_versions(code: &str) -> Result<Vec<DefinitionVersion>, String> {
    get_json(&format!("/api/processes/definitions/{code}/versions")).await
}

pub async fn get_process(code: &str, version: i32) -> Result<ProcessRecord, String> {
    get_json(&format!(
        "/api/processes/definitions/{code}/versions/{version}"
    ))
    .await
}

pub async fn activation_plan(code: &str, version: i32) -> Result<ActivationPlan, String> {
    get_json(&format!(
        "/api/processes/definitions/{code}/versions/{version}/activation-plan"
    ))
    .await
}

pub async fn activate_process(code: &str, version: i32) -> Result<ActivationPlan, String> {
    post_json(
        &format!("/api/processes/definitions/{code}/versions/{version}/activate"),
        &(),
    )
    .await
}

pub async fn deactivate_process(code: &str) -> Result<serde_json::Value, String> {
    post_json(
        &format!("/api/processes/definitions/{code}/deactivate"),
        &(),
    )
    .await
}

pub async fn list_stages() -> Result<Vec<DefinitionVersion>, String> {
    get_json("/api/processes/stages").await
}

/// Головные версии Этапов вместе с манифестом и кодом.
pub async fn list_stages_full() -> Result<Vec<StageRecord>, String> {
    get_json("/api/processes/stages/full").await
}

pub async fn list_stage_versions(code: &str) -> Result<Vec<DefinitionVersion>, String> {
    get_json(&format!("/api/processes/stages/{code}/versions")).await
}

pub async fn get_stage(code: &str, version: i32) -> Result<StageRecord, String> {
    get_json(&format!("/api/processes/stages/{code}/versions/{version}")).await
}

/// Сохранить Этап черновиком.
///
/// Черновик у кода один: повторное сохранение переписывает его, а новая версия
/// заводится только когда головная уже активна. Отпечаток считает бэкенд —
/// присылать его отсюда бессмысленно, он выводится из содержимого.
pub async fn save_stage(definition: &StageDefinition) -> Result<StageRecord, String> {
    post_json("/api/processes/stages", definition).await
}

pub async fn activate_stage(code: &str, version: i32) -> Result<StageRecord, String> {
    post_json(
        &format!("/api/processes/stages/{code}/versions/{version}/activate"),
        &(),
    )
    .await
}

/// Сухой прогон Этапа: Действия записывают план и ничего не меняют.
pub async fn dry_run_stage(
    code: &str,
    version: i32,
    input: &serde_json::Value,
) -> Result<contracts::processes::StageRun, String> {
    post_json(
        &format!("/api/processes/stages/{code}/versions/{version}/dry-run"),
        &serde_json::json!({ "input": input }),
    )
    .await
}

pub async fn list_effects(limit: u64) -> Result<Vec<EffectRecord>, String> {
    get_json(&format!("/api/processes/effects?limit={limit}")).await
}

pub async fn list_events(limit: u64) -> Result<Vec<DomainEvent>, String> {
    get_json(&format!("/api/processes/events?limit={limit}")).await
}

pub async fn list_event_kinds() -> Result<Vec<EventKindInfo>, String> {
    get_json("/api/processes/event-kinds").await
}

/// Каталог Действий: закрытый список того, чем Этап вообще может менять мир.
pub async fn list_actions() -> Result<Vec<ActionInfo>, String> {
    get_json("/api/processes/actions").await
}

/// Двинуть механизм сейчас, не дожидаясь очередного прохода воркера.
pub async fn tick() -> Result<TickReport, String> {
    post_json("/api/processes/tick", &()).await
}
