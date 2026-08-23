//! Действие `run_quality_check` — прогнать проверку качества данных.
//!
//! Пограничный случай: сам прогон ничего в мире не меняет, он только читает и
//! пишет свой результат в историю. Действием он сделан не ради эффекта, а ради
//! места в графе: Процесс должен уметь спросить «сходится ли» и разойтись по
//! именованным выходам Этапа, а результат этого вопроса обязан попасть в тот же
//! журнал, что и остальные шаги прогона.

use anyhow::Result;
use async_trait::async_trait;
use contracts::processes::{ActionActor, ActionInfo};
use sea_orm::DatabaseConnection;
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::OnceLock;

use super::Action;

#[derive(Debug, Deserialize)]
struct Input {
    check_id: String,
    #[serde(default)]
    input: Value,
}

pub struct RunQualityCheck;

fn info() -> &'static ActionInfo {
    static INFO: OnceLock<ActionInfo> = OnceLock::new();
    INFO.get_or_init(|| ActionInfo {
        name: "run_quality_check",
        method: "runQualityCheck",
        title: "Прогнать quality-проверку",
        description: "Запускает зарегистрированную проверку качества данных по id \
                      и возвращает популяцию, число нарушений и долю.",
        capability: "action:run_quality_check",
        reversible: true,
        write_tables: &["sys_quality_check_runs"],
        input_schema: json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "type": "object",
            "required": ["check_id"],
            "additionalProperties": false,
            "properties": {
                "check_id": { "type": "string", "minLength": 1 },
                "input": { "type": "object", "default": {} }
            }
        }),
    })
}

#[async_trait]
impl Action for RunQualityCheck {
    fn info(&self) -> &ActionInfo {
        info()
    }

    async fn execute(
        &self,
        _db: &DatabaseConnection,
        input: &Value,
        actor: &ActionActor,
    ) -> Result<Value> {
        let input: Input = serde_json::from_value(input.clone())?;
        let source = match actor {
            ActionActor::Process { .. } => "process:action",
            ActionActor::User { .. } => "llm",
            ActionActor::Manual => "manual",
        };
        let details =
            crate::quality::run_check_with_input(&input.check_id, input.input, source).await?;
        // Отдаём полный `details`, а не два числа. Урезанный результат годился,
        // пока единственным вызывающим был Этап — ему нужно только «сходится ли».
        // Оболочке чата нужны нарушения: модель обязана объяснить человеку, что
        // именно не сошлось. Форма записи в журнале при этом одна на обоих.
        Ok(json!({
            "check_id": input.check_id,
            "population_total": details.result.population_total,
            "violations_total": details.result.violations_total,
            "details": details,
        }))
    }

    async fn plan(
        &self,
        _db: &DatabaseConnection,
        input: &Value,
        _actor: &ActionActor,
    ) -> Result<Value> {
        let input: Input = serde_json::from_value(input.clone())?;
        Ok(json!({
            "effect": "прогон quality-проверки (данные не меняются)",
            "check_id": input.check_id,
            "input": input.input,
        }))
    }
}
