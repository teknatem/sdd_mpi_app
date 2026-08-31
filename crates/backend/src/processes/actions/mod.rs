//! Каталог Действий и диспетчер вызовов.
//!
//! Действие — операция ядра с побочным эффектом (ADR-0011 п.1). Оно живёт в
//! Rust, закрыто своей `capability` и умеет два режима: исполнить и записать
//! намерение. Второй режим не удобство, а условие допуска процесса в работу:
//! человек смотрит план эффектов перед активацией (ADR-0011 п.8).
//!
//! Каталог намеренно маленький и растёт поштучно: каждое новое Действие
//! расширяет то, что система вообще способна сделать с миром.

pub mod create_agent_task;
pub mod import_nomenclature;
pub mod repair_empty_nomenclature_refs;
pub mod repost_documents;
pub mod request_human_action;
pub mod run_quality_check;

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use contracts::processes::{
    ActionActor, ActionCall, ActionInfo, ActionMode, ActionOutcome, EffectStatus,
};
use sea_orm::DatabaseConnection;
use serde_json::Value;
use std::sync::{Arc, OnceLock};
use std::time::Instant;

use super::effect_log;

/// Одна операция ядра с побочным эффектом.
#[async_trait]
pub trait Action: Send + Sync {
    fn info(&self) -> &ActionInfo;

    /// Исполнить по-настоящему. Возвращает то, что стоит записать в журнал:
    /// не «ok», а идентификаторы и числа, по которым эффект можно найти.
    ///
    /// `actor` — кто просил. Он нужен не для проверки прав (право отобрано
    /// раньше, манифестом Этапа), а потому что у эффекта бывает адресат:
    /// поручение и тикет обязаны нести обратную ссылку на того, кто их
    /// породил, иначе человек, получивший просьбу, не найдёт её причину.
    async fn execute(
        &self,
        db: &DatabaseConnection,
        input: &Value,
        actor: &ActionActor,
    ) -> Result<Value>;

    /// Что произошло бы. Читать данные здесь можно и нужно — план тем полезнее,
    /// чем он конкретнее; менять нельзя ничего.
    async fn plan(
        &self,
        db: &DatabaseConnection,
        input: &Value,
        actor: &ActionActor,
    ) -> Result<Value>;
}

static CATALOG: OnceLock<Vec<Arc<dyn Action>>> = OnceLock::new();

/// Установить каталог Действий. Зовётся один раз из `composition::install_all()`.
///
/// Состав живёт в composition root, а не здесь: Действия принадлежат своим
/// срезам (`a007::action`, `a033::action`, …), и перечисляя их в ядре
/// механизма, оно зависело бы от прикладного слоя.
///
/// # Panics
/// При повторной установке и при конфликте имён: одно имя на два Действия —
/// это молча выбранное «какое-нибудь» при вызове по манифесту Этапа.
pub fn install(actions: Vec<Arc<dyn Action>>) {
    let mut names = std::collections::HashSet::new();
    for action in &actions {
        if !names.insert(action.info().name) {
            panic!("Действие '{}' заявлено дважды", action.info().name);
        }
    }
    if CATALOG.set(actions).is_err() {
        panic!("каталог Действий уже установлен");
    }
}

fn catalog() -> &'static Vec<Arc<dyn Action>> {
    CATALOG
        .get()
        .expect("каталог Действий не установлен: composition::install_all() не был вызван")
}

/// Паспорта всех Действий — для UI каталога и для проверки манифестов Этапов.
pub fn list() -> Vec<&'static ActionInfo> {
    catalog().iter().map(|action| action.info()).collect()
}

fn find(name: &str) -> Option<&'static Arc<dyn Action>> {
    catalog().iter().find(|action| action.info().name == name)
}

/// Есть ли такое Действие. Нужно валидатору манифеста Этапа: `capability`
/// `action:<name>` без существующего Действия — опечатка, а не право.
pub fn exists(name: &str) -> bool {
    find(name).is_some()
}

fn validate_input(info: &ActionInfo, input: &Value) -> Result<()> {
    jsonschema::validator_for(&info.input_schema)
        .map_err(|error| {
            anyhow!(
                "схема входа Действия '{}' не компилируется: {error}",
                info.name
            )
        })?
        .validate(input)
        .map_err(|error| anyhow!("вход Действия '{}' не по схеме: {error}", info.name))?;
    Ok(())
}

/// Вызвать Действие.
///
/// Здесь собран весь контракт безопасности эффекта: проверка входа по схеме,
/// ключ идемпотентности, запись в журнал ДО исполнения и разделение исходов.
/// Ни одно Действие не исполняется в обход этой функции — иначе гарантии
/// становятся необязательными.
pub async fn run(db: &DatabaseConnection, call: &ActionCall) -> Result<ActionOutcome> {
    let action =
        find(&call.action).ok_or_else(|| anyhow!("неизвестное Действие: {}", call.action))?;
    let info = action.info();
    validate_input(info, &call.input)?;

    if call.idempotency_key.trim().is_empty() {
        return Err(anyhow!(
            "Действие '{}' вызвано без ключа идемпотентности",
            info.name
        ));
    }

    match call.mode {
        ActionMode::DryRun => {
            let started = Instant::now();
            let plan = action.plan(db, &call.input, &call.actor).await?;
            let duration_ms = started.elapsed().as_millis() as i64;
            let record = effect_log::record_plan(
                db,
                info.name,
                &call.idempotency_key,
                &call.input,
                &plan,
                &call.actor,
                duration_ms,
            )
            .await?;
            Ok(ActionOutcome {
                effect_id: record.id,
                status: EffectStatus::Planned,
                result: plan,
                replayed: false,
                duration_ms,
            })
        }
        ActionMode::Execute => execute_once(db, action.as_ref(), call).await,
    }
}

async fn execute_once(
    db: &DatabaseConnection,
    action: &dyn Action,
    call: &ActionCall,
) -> Result<ActionOutcome> {
    let info = action.info();
    let existing = effect_log::find_executed_by_key(db, &call.idempotency_key).await?;

    let record = match existing {
        // Уже сделано — возвращаем записанный результат. Для вызывающего это
        // успех, но `replayed` он обязан различать.
        Some(model) if model.status == EffectStatus::Executed.as_str() => {
            return Ok(ActionOutcome {
                effect_id: model.id,
                status: EffectStatus::Executed,
                result: model
                    .result_json
                    .and_then(|raw| serde_json::from_str(&raw).ok())
                    .unwrap_or(Value::Null),
                replayed: true,
                duration_ms: model.duration_ms.unwrap_or(0),
            });
        }
        // Исполнение начато и не завершилось. Что произошло с миром — неизвестно,
        // и повтор здесь хуже остановки: он может сделать эффект вторым.
        // Это дефект, а не штатная ветка (ADR-0011 п.10).
        Some(model) if model.status == EffectStatus::InProgress.as_str() => {
            return Err(anyhow!(
                "эффект '{}' с ключом '{}' остался незавершённым (запись {}): \
                 требуется разбор, автоматический повтор запрещён",
                info.name,
                call.idempotency_key,
                model.id
            ));
        }
        // Прошлая попытка упала — повтор разрешён по той же строке.
        Some(model) => {
            effect_log::reclaim(db, &model.id).await?;
            model
        }
        None => {
            effect_log::claim(
                db,
                info.name,
                &call.idempotency_key,
                &call.input,
                &call.actor,
            )
            .await?
        }
    };

    let started = Instant::now();
    match action.execute(db, &call.input, &call.actor).await {
        Ok(result) => {
            let duration_ms = started.elapsed().as_millis() as i64;
            effect_log::mark_executed(db, &record.id, &result, duration_ms).await?;
            Ok(ActionOutcome {
                effect_id: record.id,
                status: EffectStatus::Executed,
                result,
                replayed: false,
                duration_ms,
            })
        }
        Err(error) => {
            let duration_ms = started.elapsed().as_millis() as i64;
            let text = error.to_string();
            effect_log::mark_failed(db, &record.id, &text, duration_ms).await?;
            Err(anyhow!("Действие '{}' упало: {text}", info.name))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Имя, метод и capability обязаны быть согласованы: по ним Действие
    /// адресуется из трёх разных мест (журнал, mjs, манифест Этапа), и
    /// расхождение обнаружится только в рантайме.
    #[test]
    fn catalog_entries_are_consistent() {
        crate::composition::install_all();
        for info in list() {
            assert_eq!(
                info.capability,
                format!("action:{}", info.name),
                "capability Действия '{}' не выведена из имени",
                info.name
            );
            assert!(
                !info.method.is_empty() && !info.title.is_empty(),
                "у Действия '{}' пустой метод или заголовок",
                info.name
            );
            jsonschema::validator_for(&info.input_schema)
                .unwrap_or_else(|error| panic!("схема '{}' не компилируется: {error}", info.name));
        }
    }

    #[test]
    fn action_names_are_unique() {
        crate::composition::install_all();
        let mut names: Vec<&str> = list().iter().map(|info| info.name).collect();
        names.sort_unstable();
        let count = names.len();
        names.dedup();
        assert_eq!(count, names.len(), "в каталоге Действий есть дубли имён");
    }
}
