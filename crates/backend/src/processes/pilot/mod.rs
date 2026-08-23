//! Пилот `pr0001` «Закрытие дня WB» — первый настоящий процесс компании.
//!
//! Определения живут в БД (ADR-0011 п.6), но взяться там они должны откуда-то в
//! первый раз. Отсюда посев: коды и mjs лежат в репозитории, а при старте
//! заводятся **черновиками**, если такого кода в базе ещё нет.
//!
//! Три свойства посева, каждое намеренное:
//!
//! - **Только черновики.** Активация — решение человека с просмотром плана
//!   эффектов (ADR-0011 п.8); посев, включающий процесс сам, обходил бы гейт,
//!   ради которого гейт и заведён.
//! - **Только при отсутствии кода.** Правку в базе посев не трогает: после
//!   первого запуска источник истины — БД, а файлы здесь остаются образцом, с
//!   которого всё начиналось.
//! - **Молча пропускает существующее.** Второй старт сервера не должен ни
//!   падать, ни плодить версии.
//!
//! Граф пилота:
//!
//! ```text
//!   import.day.completed(connection_id, business_date)
//!        │
//!   st0001 «Пересчитать день» ──пересчитан──▶ st0002 «Сверить день»
//!                                              ├─сходится────▶ st0003 «Закрыть день»
//!                                              │                 ├─закрыт──────▶ готово
//!                                              │                 └─не закрыт──┐
//!                                              └─расхождение──────────────────┴▶ st0004
//!   st0004 «Позвать человека» ──позвали──▶ ЖДЁТ human.action.done (24 ч) ──▶ st0001
//! ```
//!
//! Дедлайн ожидания без запасного ребра: не дождались за сутки — экземпляр
//! остаётся человеку, а не идёт по кругу сам.

use anyhow::Result;
use contracts::processes::{
    EdgeTarget, ProcessDefinition, ProcessEdge, ProcessManifest, ProcessTrigger, StageDefinition,
    StageManifest, StageOutput, WaitSpec,
};
use sea_orm::DatabaseConnection;
use serde_json::json;

use crate::processes::{definitions, repository};

/// Код парной quality-проверки. Критичный Процесс без неё не активируется
/// (ADR-0011 п.4), поэтому имя должно совпадать с `quality_checks/`.
pub const PAIRED_CHECK: &str = "wb_day_not_closed";

pub const PROCESS_CODE: &str = "pr0001";

/// Вход у всех Этапов пилота один и тот же — ключ корреляции события.
fn day_input_schema() -> serde_json::Value {
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "type": "object",
        "required": ["connection_id", "business_date"],
        "properties": {
            "connection_id": { "type": "string", "minLength": 1 },
            "business_date": { "type": "string", "pattern": "^\\d{4}-\\d{2}-\\d{2}$" }
        }
    })
}

fn output(name: &str, description: &str) -> StageOutput {
    StageOutput {
        name: name.to_string(),
        description: description.to_string(),
        data_schema: None,
    }
}

fn stage(
    code: &str,
    title: &str,
    description: &str,
    script: &str,
    outputs: Vec<StageOutput>,
    capabilities: Vec<String>,
) -> StageDefinition {
    StageDefinition {
        manifest: StageManifest {
            code: code.to_string(),
            title: title.to_string(),
            description: description.to_string(),
            entrypoint: "stage.mjs".to_string(),
            export: "run".to_string(),
            input_schema: Some(day_input_schema()),
            outputs,
            capabilities,
        },
        script: script.to_string(),
        digest: String::new(),
    }
}

/// Четыре Этапа пилота.
pub fn stages() -> Vec<StageDefinition> {
    vec![
        stage(
            "st0001",
            "Пересчитать день",
            "Пересобирает снимок закрытия дня a033 из проекций. Единственный Этап пилота \
             с эффектом над данными учёта.",
            include_str!("st0001_rebuild_day.mjs"),
            vec![output("пересчитан", "Снимок дня пересобран")],
            vec![
                "db:read:a033_wb_day_close".to_string(),
                "action:rebuild_day_close".to_string(),
            ],
        ),
        stage(
            "st0002",
            "Сверить день",
            "Смотрит блокирующие проблемы снимка a033: его детекторы и есть сверка дня. \
             Эффектов нет — Этап только решает, куда пойдёт процесс.",
            include_str!("st0002_reconcile.mjs"),
            vec![
                output("сходится", "Блокирующих проблем нет"),
                output("расхождение", "Есть блокирующие проблемы либо снимка нет"),
            ],
            vec!["db:read:a033_wb_day_close".to_string()],
        ),
        stage(
            "st0003",
            "Закрыть день",
            "Проверяет само утверждение «день закрыт»: снимок есть, пересчитан и без \
             блокирующих проблем. Отдельного состояния закрытия у a033 нет намеренно.",
            include_str!("st0003_close_day.mjs"),
            vec![
                output("закрыт", "Снимок свежий и чистый"),
                output(
                    "не закрыт",
                    "Снимка нет, он не пересчитан или в нём есть блокирующие проблемы",
                ),
            ],
            vec!["db:read:a033_wb_day_close".to_string()],
        ),
        stage(
            "st0004",
            "Позвать человека",
            "Заводит тикет с просьбой разобрать день. Ожидание задаёт ребро графа, а не \
             этот Этап.",
            include_str!("st0004_call_human.mjs"),
            vec![output("позвали", "Просьба к человеку оформлена")],
            vec!["action:request_human_action".to_string()],
        ),
    ]
}

/// Определение Процесса.
pub fn process() -> ProcessDefinition {
    ProcessDefinition {
        manifest: ProcessManifest {
            code: PROCESS_CODE.to_string(),
            title: "Закрытие дня WB".to_string(),
            description: "Пересчитывает снимок дня, сверяет его и закрывает; при расхождении \
                          зовёт человека и после его ответа начинает заново."
                .to_string(),
            trigger: ProcessTrigger::on("import.day.completed"),
            entry: "st0001".to_string(),
            edges: vec![
                ProcessEdge {
                    from: "st0001".into(),
                    outcome: "пересчитан".into(),
                    to: EdgeTarget::stage("st0002"),
                    wait: None,
                },
                ProcessEdge {
                    from: "st0002".into(),
                    outcome: "сходится".into(),
                    to: EdgeTarget::stage("st0003"),
                    wait: None,
                },
                ProcessEdge {
                    from: "st0002".into(),
                    outcome: "расхождение".into(),
                    to: EdgeTarget::stage("st0004"),
                    wait: None,
                },
                ProcessEdge {
                    from: "st0003".into(),
                    outcome: "закрыт".into(),
                    to: EdgeTarget::Done,
                    wait: None,
                },
                ProcessEdge {
                    from: "st0003".into(),
                    outcome: "не закрыт".into(),
                    to: EdgeTarget::stage("st0004"),
                    wait: None,
                },
                ProcessEdge {
                    from: "st0004".into(),
                    outcome: "позвали".into(),
                    to: EdgeTarget::stage("st0001"),
                    wait: Some(WaitSpec {
                        event: "human.action.done".into(),
                        deadline_minutes: 24 * 60,
                        // Запасного ребра нет: не дождались за сутки —
                        // экземпляр остаётся человеку, а не идёт по кругу сам.
                        on_timeout: None,
                    }),
                },
            ],
            quality_check: Some(PAIRED_CHECK.to_string()),
        },
        digest: String::new(),
    }
}

/// Что сделал посев — для лога старта.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct SeedReport {
    pub stages_created: Vec<String>,
    pub process_created: bool,
}

impl SeedReport {
    pub fn is_empty(&self) -> bool {
        self.stages_created.is_empty() && !self.process_created
    }
}

/// Завести определения пилота черновиками, если их кодов ещё нет.
pub async fn seed(db: &DatabaseConnection) -> Result<SeedReport> {
    let mut report = SeedReport::default();

    for definition in stages() {
        let code = definition.manifest.code.clone();
        if !repository::list_stage_versions(db, &code).await?.is_empty() {
            continue;
        }
        definitions::save_stage(db, definition, Some("seed".to_string())).await?;
        report.stages_created.push(code);
    }

    if repository::list_process_versions(db, PROCESS_CODE)
        .await?
        .is_empty()
    {
        definitions::save_process(db, process(), Some("seed".to_string())).await?;
        report.process_created = true;
    }

    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    use crate::processes::graph;

    /// Граф пилота обязан проходить ту же статическую проверку, что и любой
    /// другой: образец, который не проходит собственный валидатор, хуже, чем
    /// отсутствие образца.
    #[test]
    fn pilot_graph_is_valid_against_its_own_stages() {
        let catalog: HashMap<String, StageDefinition> = stages()
            .into_iter()
            .map(|stage| (stage.manifest.code.clone(), stage))
            .collect();
        let problems = graph::problems(&process().manifest, &catalog);
        assert!(problems.is_empty(), "{problems:?}");
    }

    /// Пилот критичен (есть эффекты), поэтому парная проверка обязана быть
    /// указана — и указана та, что реально зарегистрирована.
    #[test]
    fn pilot_declares_the_paired_check_that_exists() {
        assert_eq!(
            process().manifest.quality_check.as_deref(),
            Some(PAIRED_CHECK)
        );
        assert!(
            crate::quality::list_checks()
                .into_iter()
                .any(|info| info.id == PAIRED_CHECK),
            "парная проверка '{PAIRED_CHECK}' не зарегистрирована"
        );
    }

    /// Каждый Этап пилота — корректное определение сам по себе.
    #[test]
    fn every_pilot_stage_validates() {
        for definition in stages() {
            crate::processes::stages::validate_definition(&definition)
                .unwrap_or_else(|error| panic!("{}: {error}", definition.manifest.code));
        }
    }
}
