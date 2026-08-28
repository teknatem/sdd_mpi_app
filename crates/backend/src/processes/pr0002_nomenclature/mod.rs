//! Процесс `pr0002` «Проверка номенклатуры» — единый контур справочников.
//!
//! Посев: коды и mjs лежат в репозитории, при старте заводятся **черновиками**,
//! если такого кода в базе ещё нет. Активация — решение человека.
//!
//! Старт экземпляра — факт `process.due` (ключ `process_code`), который для
//! разработки публикует регламентное задание `task032_nomenclature_check`.

use anyhow::Result;
use contracts::processes::{
    EdgeTarget, ProcessDefinition, ProcessEdge, ProcessManifest, ProcessTrigger, StageDefinition,
    StageManifest, StageOutput, WaitSpec,
};
use sea_orm::DatabaseConnection;
use serde_json::json;

use crate::processes::{definitions, repository};

pub const PAIRED_CHECK: &str = "nomenclature_catalog_inconsistent";
pub const PROCESS_CODE: &str = "pr0002";

fn process_input_schema() -> serde_json::Value {
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "type": "object",
        "required": ["process_code"],
        "properties": {
            "process_code": { "type": "string", "minLength": 1 }
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
            input_schema: Some(process_input_schema()),
            outputs,
            capabilities,
        },
        script: script.to_string(),
        digest: String::new(),
    }
}

/// Шесть Этапов pr0002 (коды с st0005: st0001–st0004 заняты пилотом).
pub fn stages() -> Vec<StageDefinition> {
    vec![
        stage(
            "st0005",
            "Подтянуть номенклатуру 1С",
            "Импортирует a004 (и штрихкоды) по всем активным подключениям 1С.",
            include_str!("st0005_import_1c.mjs"),
            vec![output("подтянуто", "Импорт номенклатуры 1С выполнен")],
            vec![
                "db:read:a001_connection_1c_database".to_string(),
                "action:import_nomenclature".to_string(),
            ],
        ),
        stage(
            "st0006",
            "Подтянуть товары площадок",
            "Импортирует a007 по активным кабинетам WB/YM/Ozon/ЛеманаПро.",
            include_str!("st0006_import_mp.mjs"),
            vec![output("подтянуто", "Импорт товаров площадок выполнен")],
            vec![
                "db:read:a006_connection_mp".to_string(),
                "action:import_marketplace_products".to_string(),
            ],
        ),
        stage(
            "st0007",
            "Сопоставить",
            "Массово связывает a007 с a004 по артикулу (u505).",
            include_str!("st0007_match.mjs"),
            vec![output("сопоставлено", "Сопоставление завершено")],
            vec!["action:match_nomenclature".to_string()],
        ),
        stage(
            "st0008",
            "Оценить",
            "Считает несопоставленные/неоднозначные a007 и пустые ссылки в проекциях; \
             решает, куда идти дальше.",
            include_str!("st0008_assess.mjs"),
            vec![
                output("чисто", "Каталог согласован, дырок в проекциях нет"),
                output(
                    "только_проекции",
                    "Каталог согласован, но в проекциях пустой nomenclature_ref",
                ),
                output(
                    "остаток",
                    "Есть несопоставленные/неоднозначные позиции или дырки после репоста",
                ),
            ],
            vec![
                "db:read:a007_marketplace_product".to_string(),
                "db:read:a004_nomenclature".to_string(),
                "db:read:p909_mp_order_line_turnovers".to_string(),
                "db:read:p911_wb_advert_by_items".to_string(),
                "db:read:p913_wb_advert_order_attr".to_string(),
            ],
        ),
        stage(
            "st0009",
            "Починить ссылки в проекциях",
            "Точечно перепроводит регистраторы с пустым nomenclature_ref в p909/p911/p913.",
            include_str!("st0009_repair.mjs"),
            vec![output("перепроведено", "Перепроведение выполнено")],
            vec!["action:repair_empty_nomenclature_refs".to_string()],
        ),
        stage(
            "st0010",
            "Позвать человека",
            "Один тикет со сводкой и выборкой проблемных позиций. Ожидание задаёт ребро графа.",
            include_str!("st0010_call_human.mjs"),
            vec![output("позвали", "Просьба к человеку оформлена")],
            vec![
                "db:read:a007_marketplace_product".to_string(),
                "db:read:a004_nomenclature".to_string(),
                "action:request_human_action".to_string(),
            ],
        ),
    ]
}

pub fn process() -> ProcessDefinition {
    ProcessDefinition {
        manifest: ProcessManifest {
            code: PROCESS_CODE.to_string(),
            title: "Проверка номенклатуры".to_string(),
            description: "Подтягивает копии справочников 1С и площадок, сопоставляет, \
                          чинит пустые ссылки в проекциях; несопоставимое отдаёт человеку \
                          одним тикетом. В 1С и на площадки не пишет."
                .to_string(),
            trigger: ProcessTrigger::on("process.due"),
            entry: "st0005".to_string(),
            edges: vec![
                ProcessEdge {
                    from: "st0005".into(),
                    outcome: "подтянуто".into(),
                    to: EdgeTarget::stage("st0006"),
                    wait: None,
                },
                ProcessEdge {
                    from: "st0006".into(),
                    outcome: "подтянуто".into(),
                    to: EdgeTarget::stage("st0007"),
                    wait: None,
                },
                ProcessEdge {
                    from: "st0007".into(),
                    outcome: "сопоставлено".into(),
                    to: EdgeTarget::stage("st0008"),
                    wait: None,
                },
                ProcessEdge {
                    from: "st0008".into(),
                    outcome: "чисто".into(),
                    to: EdgeTarget::Done,
                    wait: None,
                },
                ProcessEdge {
                    from: "st0008".into(),
                    outcome: "только_проекции".into(),
                    to: EdgeTarget::stage("st0009"),
                    wait: None,
                },
                ProcessEdge {
                    from: "st0008".into(),
                    outcome: "остаток".into(),
                    to: EdgeTarget::stage("st0010"),
                    wait: None,
                },
                ProcessEdge {
                    from: "st0009".into(),
                    outcome: "перепроведено".into(),
                    to: EdgeTarget::stage("st0008"),
                    wait: None,
                },
                ProcessEdge {
                    from: "st0010".into(),
                    outcome: "позвали".into(),
                    to: EdgeTarget::stage("st0005"),
                    wait: Some(WaitSpec {
                        event: "human.action.done".into(),
                        deadline_minutes: 24 * 60,
                        on_timeout: None,
                    }),
                },
            ],
            quality_check: Some(PAIRED_CHECK.to_string()),
        },
        digest: String::new(),
    }
}

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

    #[test]
    fn pr0002_graph_is_valid_against_its_own_stages() {
        let catalog: HashMap<String, StageDefinition> = stages()
            .into_iter()
            .map(|stage| (stage.manifest.code.clone(), stage))
            .collect();
        let problems = graph::problems(&process().manifest, &catalog);
        assert!(problems.is_empty(), "{problems:?}");
    }

    #[test]
    fn pr0002_declares_the_paired_check_that_exists() {
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

    #[test]
    fn every_pr0002_stage_validates() {
        for definition in stages() {
            crate::processes::stages::validate_definition(&definition)
                .unwrap_or_else(|error| panic!("{}: {error}", definition.manifest.code));
        }
    }
}
