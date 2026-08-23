//! Жизненный цикл определений: сохранить черновик → посмотреть, что меняется →
//! активировать.
//!
//! Здесь живут правила, которых нет ни в хранилище (`repository.rs` знает
//! только запросы), ни в валидаторах (`graph.rs`, `stages::validate` знают
//! только форму):
//!
//! - **Активация — это гейт, а не кнопка.** Перед ней собирается план
//!   (`ActivationPlan`): двухуровневый diff, критичность, список причин, по
//!   которым активации не будет. Активация повторяет тот же расчёт и отказывает
//!   на тех же основаниях — план нельзя «просмотреть и обойти».
//! - **Критичный Процесс не активируется без парной quality-проверки**
//!   (ADR-0011 п.4), и проверка обязана существовать, а не просто быть
//!   упомянутой: рассогласование должно быть видно нарушением, а не только
//!   экземпляром, застрявшим в середине графа.
//! - **Пины Этапов фиксируются в момент активации.** С этого мгновения
//!   известно, какой именно код побежит; публикация Этапа больше не меняет
//!   поведение работающих Процессов молча.

use std::collections::{BTreeSet, HashMap};

use anyhow::Result;
use contracts::processes::{
    ActivationPlan, DefinitionDiff, DefinitionStatus, ProcessCriticality, ProcessDefinition,
    ProcessManifest, ProcessRecord, StageDefinition, StagePin, StageRecord,
};
use sea_orm::DatabaseConnection;

use crate::processes::repository::{self, DraftInput};
use crate::processes::{graph, stages};

/// Сохранить черновик Этапа.
///
/// Определение проверяется до записи: сломанный Этап не должен доживать до
/// экземпляра, а «сохраню пока так» — это ровно тот путь, которым он туда
/// попадает. Отпечаток считается здесь же — версия Этапа опознаётся сравнением
/// строки, и считать его на чтении было бы поздно.
pub async fn save_stage(
    db: &DatabaseConnection,
    definition: StageDefinition,
    author: Option<String>,
) -> Result<StageRecord> {
    stages::validate_definition(&definition)?;
    let digest = stages::validate::digest(&definition);
    let definition = StageDefinition {
        digest: digest.clone(),
        ..definition
    };
    repository::save_stage_draft(
        db,
        DraftInput {
            definition,
            digest,
            created_by: author,
        },
    )
    .await
}

/// Активировать версию Этапа.
///
/// Проверка повторяется: между сохранением и активацией мог измениться каталог
/// Действий — Действие переименовали или убрали, и право `action:<name>` стало
/// ссылкой в пустоту.
pub async fn activate_stage(
    db: &DatabaseConnection,
    code: &str,
    version: i32,
) -> Result<StageRecord> {
    let Some(record) = repository::find_stage(db, code, version).await? else {
        anyhow::bail!("версия Этапа {code} v{version} не найдена");
    };
    stages::validate_definition(&record.definition)?;
    repository::activate_stage(db, code, version).await?;
    repository::find_stage(db, code, version)
        .await?
        .ok_or_else(|| anyhow::anyhow!("версия Этапа {code} v{version} исчезла при активации"))
}

/// Сохранить черновик Процесса.
///
/// В отличие от Этапа граф проверяется не целиком: Процесс пишется вместе со
/// своими Этапами, и требовать готовый каталог на каждом сохранении значило бы
/// запретить порядок «сначала граф, потом Этапы». Полная проверка — на
/// активации, она же публикация (ADR-0011 п.11).
pub async fn save_process(
    db: &DatabaseConnection,
    definition: ProcessDefinition,
    author: Option<String>,
) -> Result<ProcessRecord> {
    let digest = graph::digest(&definition);
    let definition = ProcessDefinition {
        digest: digest.clone(),
        ..definition
    };
    repository::save_process_draft(
        db,
        DraftInput {
            definition,
            digest,
            created_by: author,
        },
    )
    .await
}

/// Откуда брать определения Этапов графа.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StageSource {
    /// Только активные версии — то, что запинится при активации Процесса.
    Active,
    /// Активная, а при её отсутствии черновик: черновик Процесса смотрят вместе
    /// с ещё не опубликованными Этапами.
    ActiveOrDraft,
}

/// Разрешить коды Этапов графа в определения.
pub async fn resolve_stages(
    db: &DatabaseConnection,
    manifest: &ProcessManifest,
    source: StageSource,
) -> Result<HashMap<String, StageRecord>> {
    let mut resolved = HashMap::new();
    for code in manifest.stage_codes() {
        let record = match repository::active_stage(db, &code).await? {
            Some(record) => Some(record),
            None if source == StageSource::ActiveOrDraft => {
                repository::stage_draft(db, &code).await?
            }
            None => None,
        };
        if let Some(record) = record {
            resolved.insert(code, record);
        }
    }
    Ok(resolved)
}

/// Определения Этапов, запиненные версией Процесса.
///
/// То, по чему обязан идти уже стартовавший экземпляр (ADR-0011 п.7): не
/// «активные сейчас», а «активные на момент активации Процесса».
pub async fn pinned_stages(
    db: &DatabaseConnection,
    code: &str,
    version: i32,
) -> Result<HashMap<String, StageDefinition>> {
    let mut resolved = HashMap::new();
    for pin in repository::process_pins(db, code, version).await? {
        let Some(record) = repository::find_stage(db, &pin.code, pin.version).await? else {
            anyhow::bail!(
                "версия Этапа {} v{} запинена Процессом {code} v{version}, но не найдена",
                pin.code,
                pin.version
            );
        };
        resolved.insert(pin.code, record.definition);
    }
    Ok(resolved)
}

fn definitions(records: &HashMap<String, StageRecord>) -> HashMap<String, StageDefinition> {
    records
        .iter()
        .map(|(code, record)| (code.clone(), record.definition.clone()))
        .collect()
}

/// Собрать план активации: что изменится, насколько это опасно и почему
/// активации может не быть.
pub async fn activation_plan(
    db: &DatabaseConnection,
    code: &str,
    version: i32,
) -> Result<ActivationPlan> {
    let Some(candidate) = repository::find_process(db, code, version).await? else {
        anyhow::bail!("версия Процесса {code} v{version} не найдена");
    };
    let manifest = &candidate.definition.manifest;

    let resolved = resolve_stages(db, manifest, StageSource::Active).await?;
    let stage_definitions = definitions(&resolved);

    let mut problems = graph::problems(manifest, &stage_definitions);
    let criticality = graph::criticality(manifest, &stage_definitions);

    // Гейт п.4: критичность выведена из прав Этапов, а парная проверка обязана
    // существовать. Упомянутая, но не заведённая проверка — это отсутствие
    // видимости, поданное как её наличие.
    if criticality.needs_quality_check() {
        match manifest.quality_check.as_deref().map(str::trim) {
            None | Some("") => problems.push(format!(
                "Процесс критичный ({}), но парная quality-проверка не указана: \
                 без неё рассогласование не видно",
                criticality.as_str()
            )),
            Some(check) if !quality_check_exists(check) => problems.push(format!(
                "парная quality-проверка '{check}' не зарегистрирована"
            )),
            Some(_) => {}
        }
    }

    // Пины считаем только из активных версий: черновик Этапа в работу не идёт.
    let pinned_stages: Vec<StagePin> = {
        let mut pins: Vec<StagePin> = manifest
            .stage_codes()
            .into_iter()
            .filter_map(|code| resolved.get(&code))
            .map(|record| StagePin {
                code: record.code.clone(),
                version: record.version,
                digest: record.digest.clone(),
            })
            .collect();
        pins.sort_by(|left, right| left.code.cmp(&right.code));
        pins
    };

    let current = repository::active_process(db, code).await?;
    let process = process_diff(current.as_ref(), &candidate);
    let stages = stage_diffs(db, current.as_ref(), &pinned_stages).await?;

    Ok(ActivationPlan {
        process,
        stages,
        pinned_stages,
        criticality,
        problems,
    })
}

/// Активировать версию Процесса, если план это позволяет.
///
/// Тот же расчёт, что и в `activation_plan`: гейт нельзя обойти, посмотрев план
/// и нажав активацию отдельно.
pub async fn activate_process(
    db: &DatabaseConnection,
    code: &str,
    version: i32,
) -> Result<ActivationPlan> {
    let plan = activation_plan(db, code, version).await?;
    if !plan.is_allowed() {
        anyhow::bail!(
            "Процесс {code} v{version} не активирован: {}",
            plan.problems.join("; ")
        );
    }
    repository::activate_process(db, code, version, &plan.pinned_stages).await?;
    Ok(plan)
}

fn quality_check_exists(check: &str) -> bool {
    crate::quality::list_checks()
        .into_iter()
        .any(|info| info.id == check || (!info.code.is_empty() && info.code == check))
}

/// Diff по каждому Этапу: что изменится под Процессом, даже если сам граф не
/// тронут.
async fn stage_diffs(
    db: &DatabaseConnection,
    current: Option<&ProcessRecord>,
    pins: &[StagePin],
) -> Result<Vec<DefinitionDiff>> {
    let previous: HashMap<String, StagePin> = match current {
        Some(record) => repository::process_pins(db, &record.code, record.version)
            .await?
            .into_iter()
            .map(|pin| (pin.code.clone(), pin))
            .collect(),
        None => HashMap::new(),
    };

    let mut diffs = Vec::new();
    for pin in pins {
        let before = previous.get(&pin.code);
        if before.map(|pin| pin.digest.as_str()) == Some(pin.digest.as_str()) {
            continue;
        }
        let Some(to) = repository::find_stage(db, &pin.code, pin.version).await? else {
            continue;
        };
        let from = match before {
            Some(pin) => repository::find_stage(db, &pin.code, pin.version).await?,
            None => None,
        };
        diffs.push(stage_diff(from.as_ref(), &to));
    }
    Ok(diffs)
}

/// Что изменилось в Этапе между версиями.
///
/// Строки человекочитаемые, потому что читатель — человек перед активацией.
/// Отдельной строкой отмечается изменение кода: манифест может не поменяться
/// вовсе, а поведение — полностью.
pub fn stage_diff(from: Option<&StageRecord>, to: &StageRecord) -> DefinitionDiff {
    let mut changes = Vec::new();
    match from {
        None => changes.push(format!("новый Этап в графе (v{})", to.version)),
        Some(from) => {
            let (before, after) = (&from.definition.manifest, &to.definition.manifest);
            if before.title != after.title {
                changes.push(format!("заголовок: '{}' → '{}'", before.title, after.title));
            }
            for change in set_changes(
                "выход",
                before.output_names().into_iter().map(str::to_string),
                after.output_names().into_iter().map(str::to_string),
            ) {
                changes.push(change);
            }
            for change in set_changes(
                "право",
                before.capabilities.iter().cloned(),
                after.capabilities.iter().cloned(),
            ) {
                changes.push(change);
            }
            if before.input_schema != after.input_schema {
                changes.push("изменилась схема входа".to_string());
            }
            if before
                .outputs
                .iter()
                .map(|output| (&output.name, &output.data_schema))
                .ne(after
                    .outputs
                    .iter()
                    .map(|output| (&output.name, &output.data_schema)))
            {
                changes.push("изменились схемы данных выходов".to_string());
            }
            if from.definition.script != to.definition.script {
                changes.push("изменился код Этапа".to_string());
            }
        }
    }

    DefinitionDiff {
        code: to.code.clone(),
        title: to.definition.manifest.title.clone(),
        from_version: from.map(|record| record.version),
        to_version: to.version,
        changes,
    }
}

/// Что изменилось в графе Процесса между версиями.
pub fn process_diff(from: Option<&ProcessRecord>, to: &ProcessRecord) -> DefinitionDiff {
    let mut changes = Vec::new();
    match from {
        None => changes.push("Процесс активируется впервые".to_string()),
        Some(from) => {
            let (before, after) = (&from.definition.manifest, &to.definition.manifest);
            if before.title != after.title {
                changes.push(format!("заголовок: '{}' → '{}'", before.title, after.title));
            }
            if before.trigger.event != after.trigger.event {
                changes.push(format!(
                    "триггер: '{}' → '{}'",
                    before.trigger.event, after.trigger.event
                ));
            }
            if before.entry != after.entry {
                changes.push(format!(
                    "стартовый Этап: {} → {}",
                    before.entry, after.entry
                ));
            }
            if before.quality_check != after.quality_check {
                changes.push(format!(
                    "парная quality-проверка: {} → {}",
                    before.quality_check.as_deref().unwrap_or("нет"),
                    after.quality_check.as_deref().unwrap_or("нет")
                ));
            }
            for change in edge_changes(before, after) {
                changes.push(change);
            }
        }
    }

    DefinitionDiff {
        code: to.code.clone(),
        title: to.definition.manifest.title.clone(),
        from_version: from.map(|record| record.version),
        to_version: to.version,
        changes,
    }
}

/// Рёбра сравниваются по паре «Этап + выход»: это идентичность ребра, а всё
/// остальное — его содержимое. Иначе перенацеленное ребро читалось бы как
/// «одно удалили, другое добавили», и читатель не увидел бы, что изменился
/// именно маршрут.
fn edge_changes(before: &ProcessManifest, after: &ProcessManifest) -> Vec<String> {
    let mut changes = Vec::new();
    for edge in &before.edges {
        if after.edge(&edge.from, &edge.outcome).is_none() {
            changes.push(format!(
                "убрано ребро {} → '{}' → {}",
                edge.from,
                edge.outcome,
                graph::target_label(&edge.to)
            ));
        }
    }
    for edge in &after.edges {
        match before.edge(&edge.from, &edge.outcome) {
            None => changes.push(format!(
                "добавлено ребро {} → '{}' → {}",
                edge.from,
                edge.outcome,
                graph::target_label(&edge.to)
            )),
            Some(old) => {
                if old.to != edge.to {
                    changes.push(format!(
                        "ребро {} → '{}': {} → {}",
                        edge.from,
                        edge.outcome,
                        graph::target_label(&old.to),
                        graph::target_label(&edge.to)
                    ));
                }
                if old.wait != edge.wait {
                    changes.push(format!(
                        "ребро {} → '{}': изменилось ожидание",
                        edge.from, edge.outcome
                    ));
                }
            }
        }
    }
    changes
}

fn set_changes(
    what: &str,
    before: impl Iterator<Item = String>,
    after: impl Iterator<Item = String>,
) -> Vec<String> {
    let before: BTreeSet<String> = before.collect();
    let after: BTreeSet<String> = after.collect();
    let mut changes = Vec::new();
    for removed in before.difference(&after) {
        changes.push(format!("убран {what} '{removed}'"));
    }
    for added in after.difference(&before) {
        changes.push(format!("добавлен {what} '{added}'"));
    }
    changes
}

/// Состояние определения словами — для списков и сообщений.
pub fn status_label(status: DefinitionStatus) -> &'static str {
    match status {
        DefinitionStatus::Draft => "черновик",
        DefinitionStatus::Active => "активна",
        DefinitionStatus::Archived => "архив",
    }
}

/// Критичность словами — то же самое для плана активации.
pub fn criticality_label(criticality: ProcessCriticality) -> &'static str {
    match criticality {
        ProcessCriticality::ReadOnly => "только чтение",
        ProcessCriticality::Effectful => "есть эффекты",
        ProcessCriticality::Irreversible => "есть необратимые эффекты",
    }
}
