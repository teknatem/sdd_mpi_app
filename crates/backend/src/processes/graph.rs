//! Статическая проверка графа Процесса.
//!
//! Половина контракта, которую можно проверить до запуска (ADR-0011 п.11):
//! ошибку автора надо ловить на публикации, а не на живом экземпляре. Вторая
//! половина — рантайм: фактический выход Этапа против объявленной схемы
//! (`stages::classify_output`).
//!
//! **Что подаётся Этапу на вход** — правило, от которого зависит половина
//! проверок ниже: вход = ключ корреляции экземпляра плюс данные выхода
//! предыдущего Этапа. Ключ корреляции («кабинет», «дата») отвечает на вопрос
//! «про что этот экземпляр» и доступен на любом Этапе, включая стартовый; всё
//! остальное приходит по ребру. Поэтому «покрыт ли вход» — это вопрос про
//! объединение двух источников, а не про один только выход.
//!
//! Сравнение схем намеренно неглубокое: обязательные поля входа должны быть
//! среди свойств выхода, лишние допускаются, вложенность и `oneOf` не
//! сравниваются (ADR-0011 п.11). Более точное сравнение JSON Schema — это
//! вывод подтипов, и он либо неверен, либо запрещает законные вещи.

use std::collections::{BTreeSet, HashMap, HashSet};

use anyhow::Result;
use contracts::processes::{
    DomainEventKind, EdgeTarget, ProcessCriticality, ProcessDefinition, ProcessManifest,
    StageDefinition,
};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::processes::actions;
use crate::processes::stages::ValidationError;

/// Проверить манифест Процесса вместе с Этапами, на которые он ссылается.
///
/// `stages` — разрешённые определения Этапов графа (обычно активные версии).
/// Проверка без них невозможна: имена выходов живут в манифесте Этапа, и
/// «ребро по выходу, которого нет» иначе не поймать.
pub fn validate_definition(
    definition: &ProcessDefinition,
    stages: &HashMap<String, StageDefinition>,
) -> Result<()> {
    let problems = problems(&definition.manifest, stages);
    if problems.is_empty() {
        Ok(())
    } else {
        Err(ValidationError { problems }.into())
    }
}

/// Все нарушения манифеста разом: автор видит список целиком, а не по одному
/// за публикацию.
pub fn problems(
    manifest: &ProcessManifest,
    stages: &HashMap<String, StageDefinition>,
) -> Vec<String> {
    let mut problems = Vec::new();
    problems.extend(shape_problems(manifest));
    problems.extend(graph_problems(manifest, stages));
    problems
}

/// Проверки, которым каталог Этапов не нужен: форма кода, заполненность,
/// дубли рёбер.
fn shape_problems(manifest: &ProcessManifest) -> Vec<String> {
    let mut problems = Vec::new();

    if !is_valid_process_code(&manifest.code) {
        problems.push(format!(
            "код '{}' не по форме prNNNN (четыре цифры)",
            manifest.code
        ));
    }
    if manifest.title.trim().is_empty() {
        problems.push("пустой заголовок".to_string());
    }
    match manifest.trigger.event.trim() {
        "" => problems.push("не указано событие-триггер".to_string()),
        event if DomainEventKind::parse(event).is_none() => problems.push(format!(
            "события '{event}' нет в каталоге; заведение нового — правка Rust, а не манифеста"
        )),
        _ => {}
    }
    if manifest.entry.trim().is_empty() {
        problems.push("не указан стартовый Этап".to_string());
    }
    if manifest.edges.is_empty() {
        problems.push("в графе нет ни одного ребра".to_string());
    }

    let mut seen = HashSet::new();
    for edge in &manifest.edges {
        if !seen.insert((edge.from.as_str(), edge.outcome.as_str())) {
            problems.push(format!(
                "выход '{}' Этапа {} ведёт в два места сразу",
                edge.outcome, edge.from
            ));
        }
        if let Some(wait) = &edge.wait {
            match wait.event.trim() {
                "" => problems.push(format!(
                    "ожидание на ребре {} → '{}' без события",
                    edge.from, edge.outcome
                )),
                event if DomainEventKind::parse(event).is_none() => problems.push(format!(
                    "ожидание на ребре {} → '{}': события '{event}' нет в каталоге",
                    edge.from, edge.outcome
                )),
                _ => {}
            }
            // Ожидание без дедлайна — это тихо потерянная работа: экземпляр,
            // которого никто не разбудит, никто и не найдёт.
            if wait.deadline_minutes <= 0 {
                problems.push(format!(
                    "ожидание на ребре {} → '{}' без дедлайна",
                    edge.from, edge.outcome
                ));
            }
        }
    }

    problems
}

/// Проверки против каталога Этапов: существование, выходы, покрытие входа,
/// достижимость.
fn graph_problems(
    manifest: &ProcessManifest,
    stages: &HashMap<String, StageDefinition>,
) -> Vec<String> {
    let mut problems = Vec::new();
    // Ключ корреляции берётся из каталога событий, а не из манифеста: он —
    // свойство факта, а не подписки.
    let correlation: BTreeSet<&str> = DomainEventKind::parse(&manifest.trigger.event)
        .map(|kind| kind.correlation_fields().iter().copied().collect())
        .unwrap_or_default();

    for code in manifest.stage_codes() {
        if !stages.contains_key(&code) {
            problems.push(format!("Этап {code} не найден в каталоге"));
        }
    }

    // Вход стартового Этапа берётся только из ключа корреляции: предыдущего
    // выхода у него нет.
    if let Some(entry) = stages.get(&manifest.entry) {
        for missing in uncovered_input(entry, &correlation) {
            problems.push(format!(
                "вход стартового Этапа {}: поле '{missing}' не покрыто ключом корреляции",
                manifest.entry
            ));
        }
    }

    for edge in &manifest.edges {
        let Some(source) = stages.get(&edge.from) else {
            continue;
        };
        let Some(output) = source.manifest.output(&edge.outcome) else {
            problems.push(format!(
                "ребро от Этапа {}: выход '{}' не объявлен; объявлены: {}",
                edge.from,
                edge.outcome,
                source.manifest.output_names().join(", ")
            ));
            continue;
        };

        // Ожидание, ключ которого нечем заполнить, разбудить невозможно —
        // и узнать об этом на живом экземпляре означает потерять сутки.
        if let Some(wait) = &edge.wait {
            if let Some(kind) = DomainEventKind::parse(&wait.event) {
                let mut available = correlation.clone();
                available.extend(schema_properties(output.data_schema.as_ref()));
                // Два поля даёт рантайм: токен экземпляра и его идентификатор.
                available.insert("request_key");
                available.insert("instance_id");
                for field in kind.correlation_fields() {
                    if !available.contains(field) {
                        problems.push(format!(
                            "ожидание на ребре {} → '{}': поле ключа '{field}'                              нечем заполнить ни из выхода, ни из ключа корреляции",
                            edge.from, edge.outcome
                        ));
                    }
                }
            }
        }

        let targets = [
            Some(&edge.to),
            edge.wait.as_ref().and_then(|w| w.on_timeout.as_ref()),
        ];
        for target in targets.into_iter().flatten() {
            let Some(code) = target.stage_code() else {
                continue;
            };
            let Some(next) = stages.get(code) else {
                continue;
            };
            let mut available = correlation.clone();
            available.extend(schema_properties(output.data_schema.as_ref()));
            for missing in uncovered_input(next, &available) {
                problems.push(format!(
                    "ребро {} → '{}' → {}: обязательное поле входа '{missing}' \
                     не покрыто ни выходом, ни ключом корреляции",
                    edge.from, edge.outcome, code
                ));
            }
        }
    }

    // Выход без ребра — почти всегда забытое ребро, а не намеренный тупик:
    // терминал объявляется целью `done`, а не молчанием.
    for code in manifest.stage_codes() {
        let Some(stage) = stages.get(&code) else {
            continue;
        };
        for output in &stage.manifest.outputs {
            if manifest.edge(&code, &output.name).is_none() {
                problems.push(format!(
                    "выход '{}' Этапа {code} никуда не ведёт: объявите ребро или цель 'done'",
                    output.name
                ));
            }
        }
    }

    for code in unreachable_stages(manifest) {
        problems.push(format!("до Этапа {code} нельзя дойти от стартового"));
    }

    problems
}

/// Обязательные поля входа Этапа, которых нет среди доступных.
fn uncovered_input(stage: &StageDefinition, available: &BTreeSet<&str>) -> Vec<String> {
    required_fields(stage.manifest.input_schema.as_ref())
        .into_iter()
        .filter(|field| !available.contains(field.as_str()))
        .collect()
}

fn required_fields(schema: Option<&Value>) -> Vec<String> {
    schema
        .and_then(|schema| schema.get("required"))
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn schema_properties(schema: Option<&Value>) -> Vec<&str> {
    schema
        .and_then(|schema| schema.get("properties"))
        .and_then(Value::as_object)
        .map(|map| map.keys().map(String::as_str).collect())
        .unwrap_or_default()
}

/// Этапы графа, до которых нет пути от стартового.
fn unreachable_stages(manifest: &ProcessManifest) -> Vec<String> {
    let mut reached: HashSet<String> = HashSet::from([manifest.entry.clone()]);
    let mut frontier = vec![manifest.entry.clone()];
    while let Some(code) = frontier.pop() {
        for edge in manifest.edges.iter().filter(|edge| edge.from == code) {
            let next = [
                Some(&edge.to),
                edge.wait.as_ref().and_then(|wait| wait.on_timeout.as_ref()),
            ];
            for target in next.into_iter().flatten() {
                if let Some(code) = target.stage_code() {
                    if reached.insert(code.to_string()) {
                        frontier.push(code.to_string());
                    }
                }
            }
        }
    }
    manifest
        .stage_codes()
        .into_iter()
        .filter(|code| !reached.contains(code))
        .collect()
}

/// Критичность Процесса — выводится из прав его Этапов, а не из слов автора
/// (ADR-0011 п.4).
pub fn criticality(
    manifest: &ProcessManifest,
    stages: &HashMap<String, StageDefinition>,
) -> ProcessCriticality {
    let mut level = ProcessCriticality::ReadOnly;
    for code in manifest.stage_codes() {
        let Some(stage) = stages.get(&code) else {
            continue;
        };
        for capability in &stage.manifest.capabilities {
            let Some(name) = capability.trim().strip_prefix("action:") else {
                continue;
            };
            let name = name.trim();
            let reversible = actions::list()
                .into_iter()
                .find(|info| info.name == name)
                .map(|info| info.reversible)
                // Неизвестное Действие валидатор Этапа не пропустит, но если
                // оно всё же встретилось — считаем худшее.
                .unwrap_or(false);
            level = level.max(if reversible {
                ProcessCriticality::Effectful
            } else {
                ProcessCriticality::Irreversible
            });
        }
    }
    level
}

/// Отпечаток определения Процесса.
pub fn digest(definition: &ProcessDefinition) -> String {
    let manifest = serde_json::to_string(&definition.manifest).unwrap_or_default();
    let mut hasher = Sha256::new();
    hasher.update(manifest.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn is_valid_process_code(code: &str) -> bool {
    let code = code.trim();
    code.len() == 6
        && code.starts_with("pr")
        && code[2..]
            .chars()
            .all(|character| character.is_ascii_digit())
}

/// Цель ребра словами — для diff и для экрана экземпляра.
pub fn target_label(target: &EdgeTarget) -> String {
    match target {
        EdgeTarget::Stage { code } => code.clone(),
        EdgeTarget::Done => "завершение".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use contracts::processes::{ProcessEdge, ProcessTrigger, StageManifest, StageOutput, WaitSpec};
    use serde_json::json;

    fn stage(code: &str, outputs: &[&str], input_required: &[&str]) -> StageDefinition {
        StageDefinition {
            manifest: StageManifest {
                code: code.into(),
                title: code.into(),
                description: String::new(),
                entrypoint: "stage.mjs".into(),
                export: "run".into(),
                input_schema: (!input_required.is_empty()).then(|| {
                    json!({
                        "type": "object",
                        "required": input_required,
                    })
                }),
                outputs: outputs
                    .iter()
                    .map(|name| StageOutput {
                        name: (*name).into(),
                        description: String::new(),
                        data_schema: None,
                    })
                    .collect(),
                capabilities: vec![],
            },
            script: "export async function run() {}".into(),
            digest: String::new(),
        }
    }

    fn catalog(stages: Vec<StageDefinition>) -> HashMap<String, StageDefinition> {
        stages
            .into_iter()
            .map(|stage| (stage.manifest.code.clone(), stage))
            .collect()
    }

    fn manifest(edges: Vec<ProcessEdge>) -> ProcessManifest {
        ProcessManifest {
            code: "pr0001".into(),
            title: "Закрытие дня WB".into(),
            description: String::new(),
            trigger: ProcessTrigger::on("import.day.completed"),
            entry: "st0001".into(),
            edges,
            quality_check: None,
        }
    }

    fn edge(from: &str, outcome: &str, to: EdgeTarget) -> ProcessEdge {
        ProcessEdge {
            from: from.into(),
            outcome: outcome.into(),
            to,
            wait: None,
        }
    }

    #[test]
    fn accepts_a_closed_graph() {
        let stages = catalog(vec![
            stage("st0001", &["готово"], &["connection_id", "business_date"]),
            stage("st0002", &["сходится"], &["business_date"]),
        ]);
        let manifest = manifest(vec![
            edge("st0001", "готово", EdgeTarget::stage("st0002")),
            edge("st0002", "сходится", EdgeTarget::Done),
        ]);
        assert!(
            problems(&manifest, &stages).is_empty(),
            "{:?}",
            problems(&manifest, &stages)
        );
    }

    #[test]
    fn rejects_edge_for_undeclared_outcome() {
        let stages = catalog(vec![stage("st0001", &["готово"], &[])]);
        let manifest = manifest(vec![
            edge("st0001", "готово", EdgeTarget::Done),
            edge("st0001", "почти", EdgeTarget::Done),
        ]);
        let problems = problems(&manifest, &stages);
        assert!(
            problems.iter().any(|p| p.contains("не объявлен")),
            "{problems:?}"
        );
    }

    /// Выход без ребра — забытый переход, а не намеренный тупик.
    #[test]
    fn rejects_outcome_without_an_edge() {
        let stages = catalog(vec![stage("st0001", &["готово", "ошибка"], &[])]);
        let manifest = manifest(vec![edge("st0001", "готово", EdgeTarget::Done)]);
        let problems = problems(&manifest, &stages);
        assert!(
            problems.iter().any(|p| p.contains("никуда не ведёт")),
            "{problems:?}"
        );
    }

    #[test]
    fn rejects_input_field_covered_by_nothing() {
        let stages = catalog(vec![
            stage("st0001", &["готово"], &[]),
            stage("st0002", &["сходится"], &["amount"]),
        ]);
        let manifest = manifest(vec![
            edge("st0001", "готово", EdgeTarget::stage("st0002")),
            edge("st0002", "сходится", EdgeTarget::Done),
        ]);
        let problems = problems(&manifest, &stages);
        assert!(
            problems.iter().any(|p| p.contains("'amount'")),
            "{problems:?}"
        );
    }

    /// Поле, объявленное выходом предыдущего Этапа, вход покрывает.
    #[test]
    fn output_schema_covers_the_next_input() {
        let mut source = stage("st0001", &["готово"], &[]);
        source.manifest.outputs[0].data_schema = Some(json!({
            "type": "object",
            "properties": { "amount": { "type": "number" } }
        }));
        let stages = catalog(vec![source, stage("st0002", &["сходится"], &["amount"])]);
        let manifest = manifest(vec![
            edge("st0001", "готово", EdgeTarget::stage("st0002")),
            edge("st0002", "сходится", EdgeTarget::Done),
        ]);
        assert!(problems(&manifest, &stages).is_empty());
    }

    #[test]
    fn rejects_unreachable_stage() {
        let stages = catalog(vec![
            stage("st0001", &["готово"], &[]),
            stage("st0002", &["сходится"], &[]),
        ]);
        let manifest = manifest(vec![
            edge("st0001", "готово", EdgeTarget::Done),
            edge("st0002", "сходится", EdgeTarget::Done),
        ]);
        let problems = problems(&manifest, &stages);
        assert!(
            problems.iter().any(|p| p.contains("нельзя дойти")),
            "{problems:?}"
        );
    }

    #[test]
    fn rejects_wait_without_deadline() {
        let stages = catalog(vec![stage("st0001", &["позвали"], &[])]);
        let mut manifest = manifest(vec![edge("st0001", "позвали", EdgeTarget::Done)]);
        manifest.edges[0].wait = Some(WaitSpec {
            event: "human.action.done".into(),
            deadline_minutes: 0,
            on_timeout: None,
        });
        let problems = problems(&manifest, &stages);
        assert!(
            problems.iter().any(|p| p.contains("без дедлайна")),
            "{problems:?}"
        );
    }

    /// Каталог событий закрыт: триггер на выдуманное событие — ошибка автора,
    /// а не повод завести событие строкой в манифесте.
    #[test]
    fn rejects_trigger_outside_the_catalog() {
        let stages = catalog(vec![stage("st0001", &["готово"], &[])]);
        let mut manifest = manifest(vec![edge("st0001", "готово", EdgeTarget::Done)]);
        manifest.trigger = ProcessTrigger::on("import.day.almost");
        let problems = problems(&manifest, &stages);
        assert!(
            problems.iter().any(|p| p.contains("нет в каталоге")),
            "{problems:?}"
        );
    }

    /// Вход стартового Этапа покрывается ключом корреляции события — тем
    /// самым, который объявлен в каталоге, а не в манифесте.
    #[test]
    fn entry_input_is_covered_by_the_catalog_key() {
        let stages = catalog(vec![stage("st0001", &["готово"], &["warehouse"])]);
        let manifest = manifest(vec![edge("st0001", "готово", EdgeTarget::Done)]);
        let problems = problems(&manifest, &stages);
        assert!(
            problems
                .iter()
                .any(|p| p.contains("'warehouse'") && p.contains("ключом корреляции")),
            "{problems:?}"
        );
    }

    /// Ожидание, ключ которого нечем заполнить, — дефект графа: разбудить такой
    /// экземпляр нечем, и обнаружится это через сутки на живом прогоне.
    #[test]
    fn rejects_wait_whose_key_cannot_be_built() {
        let stages = catalog(vec![stage("st0001", &["позвали"], &[])]);
        let mut manifest = manifest(vec![edge("st0001", "позвали", EdgeTarget::Done)]);
        manifest.edges[0].wait = Some(WaitSpec {
            // document.posted требует aggregate + document_id, а ни ключ
            // корреляции дня, ни выход их не дают.
            event: "document.posted".into(),
            deadline_minutes: 60,
            on_timeout: None,
        });
        let problems = problems(&manifest, &stages);
        assert!(
            problems.iter().any(|p| p.contains("нечем заполнить")),
            "{problems:?}"
        );
    }

    /// Ожидание человека собирается всегда: `request_key` даёт рантайм.
    #[test]
    fn human_wait_key_is_always_available() {
        let stages = catalog(vec![stage("st0001", &["позвали"], &[])]);
        let mut manifest = manifest(vec![edge("st0001", "позвали", EdgeTarget::Done)]);
        manifest.edges[0].wait = Some(WaitSpec {
            event: "human.action.done".into(),
            deadline_minutes: 24 * 60,
            on_timeout: None,
        });
        assert!(problems(&manifest, &stages).is_empty());
    }

    /// Критичность выводится из прав Этапов: автор её не объявляет.
    #[test]
    fn criticality_comes_from_stage_capabilities() {
        // Критичность читает права Действий — значит, каталог должен стоять.
        crate::composition::install_all();
        let plain = catalog(vec![stage("st0001", &["готово"], &[])]);
        let manifest = manifest(vec![edge("st0001", "готово", EdgeTarget::Done)]);
        assert_eq!(criticality(&manifest, &plain), ProcessCriticality::ReadOnly);

        let mut acting = stage("st0001", &["готово"], &[]);
        acting.manifest.capabilities = vec!["action:rebuild_day_close".into()];
        assert_eq!(
            criticality(&manifest, &catalog(vec![acting])),
            ProcessCriticality::Effectful
        );

        let mut irreversible = stage("st0001", &["готово"], &[]);
        irreversible.manifest.capabilities = vec!["action:repost_documents".into()];
        assert_eq!(
            criticality(&manifest, &catalog(vec![irreversible])),
            ProcessCriticality::Irreversible
        );
    }
}
