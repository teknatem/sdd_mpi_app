//! Б3 механизма Процессов: определения живут в БД и версионируются.
//!
//! Проверяется не хранение как таковое, а три правила, ради которых оно и
//! заведено (ADR-0011 п.6, п.7, п.4):
//!
//! 1. опубликованная версия неизменяема и неудаляема — на ней доживают
//!    экземпляры;
//! 2. активация фиксирует версии Этапов, и последующая публикация Этапа не
//!    меняет поведение работающего Процесса молча;
//! 3. критичный Процесс без парной quality-проверки не активируется.
//!
//! Коды в каждом тесте свои: тесты одного бинаря делят базу и идут
//! параллельно, а идентичность определения — это код.

use backend::processes::{definitions, repository};
use backend::shared::data::db;
use contracts::processes::{
    DefinitionStatus, EdgeTarget, ProcessCriticality, ProcessDefinition, ProcessEdge,
    ProcessManifest, ProcessTrigger, StageDefinition, StageManifest, StageOutput,
};
use sea_orm::DatabaseConnection;

async fn database() -> &'static DatabaseConnection {
    backend::composition::install_all();
    db::init_test_database()
        .await
        .expect("тестовая база не поднялась");
    db::get_connection()
}

fn stage(code: &str, outputs: &[&str]) -> StageDefinition {
    StageDefinition {
        manifest: StageManifest {
            code: code.into(),
            title: format!("Этап {code}"),
            description: String::new(),
            entrypoint: "stage.mjs".into(),
            export: "run".into(),
            input_schema: None,
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
        script: format!(
            "export async function run() {{ return {{ outcome: '{}' }}; }}",
            outputs[0]
        ),
        digest: String::new(),
    }
}

fn process(code: &str, entry: &str, edges: Vec<ProcessEdge>) -> ProcessDefinition {
    ProcessDefinition {
        manifest: ProcessManifest {
            code: code.into(),
            title: format!("Процесс {code}"),
            description: String::new(),
            trigger: ProcessTrigger::on("import.day.completed"),
            entry: entry.into(),
            edges,
            quality_check: None,
        },
        digest: String::new(),
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

/// Идентификатор любой зарегистрированной проверки: гейт п.4 требует не
/// упоминания, а существующей проверки, поэтому положительному пути нужна
/// настоящая.
fn some_quality_check() -> String {
    backend::quality::list_checks()
        .first()
        .expect("в каталоге нет ни одной quality-проверки")
        .id
        .clone()
}

/// Черновик правится по месту: история версий — это история публикаций, а не
/// нажатий «сохранить».
#[tokio::test]
async fn draft_is_edited_in_place_and_publication_starts_a_new_version() {
    let db = database().await;

    let first = definitions::save_stage(db, stage("st0101", &["готово"]), None)
        .await
        .expect("черновик не сохранён");
    assert_eq!(first.version, 1);
    assert_eq!(first.status, DefinitionStatus::Draft);
    assert!(!first.digest.is_empty(), "отпечаток не посчитан");

    let mut edited = stage("st0101", &["готово"]);
    edited.manifest.title = "Пересчитать день".into();
    let second = definitions::save_stage(db, edited, None)
        .await
        .expect("правка черновика не сохранена");
    assert_eq!(second.version, 1, "правка черновика завела новую версию");
    assert_ne!(first.digest, second.digest, "отпечаток не пересчитан");

    definitions::activate_stage(db, "st0101", 1)
        .await
        .expect("активация не прошла");

    let third = definitions::save_stage(db, stage("st0101", &["готово", "ошибка"]), None)
        .await
        .expect("новый черновик не сохранён");
    assert_eq!(third.version, 2, "публикация не открыла новую версию");

    let versions = repository::list_stage_versions(db, "st0101")
        .await
        .expect("история версий не читается");
    assert_eq!(versions.len(), 2);
    assert_eq!(versions[0].status, DefinitionStatus::Draft);
    assert_eq!(versions[1].status, DefinitionStatus::Active);
}

/// Опубликованное не удаляется: на нём могут доживать экземпляры, а его
/// прогоны уже записаны в журнале эффектов.
#[tokio::test]
async fn published_version_is_not_deletable() {
    let db = database().await;
    definitions::save_stage(db, stage("st0102", &["готово"]), None)
        .await
        .expect("черновик не сохранён");

    repository::delete_stage_draft(db, "st0102", 1)
        .await
        .expect("черновик обязан удаляться");

    // Удалённый черновик не оставляет следа: нумерация начинается заново,
    // потому что на исчезнувшую версию никто не мог сослаться.
    let restored = definitions::save_stage(db, stage("st0102", &["готово"]), None)
        .await
        .expect("черновик не сохранён повторно");
    assert_eq!(restored.version, 1);
    definitions::activate_stage(db, "st0102", 1)
        .await
        .expect("активация не прошла");

    let error = repository::delete_stage_draft(db, "st0102", 1)
        .await
        .expect_err("опубликованная версия удалилась");
    assert!(error.to_string().contains("не удаляется"), "{error}");
}

/// Активная версия на код одна — это гарантия БД, а не порядок операций:
/// активация второй уводит первую в архив.
#[tokio::test]
async fn activation_archives_the_previous_version() {
    let db = database().await;
    definitions::save_stage(db, stage("st0103", &["готово"]), None)
        .await
        .unwrap();
    definitions::activate_stage(db, "st0103", 1).await.unwrap();
    definitions::save_stage(db, stage("st0103", &["готово"]), None)
        .await
        .unwrap();
    definitions::activate_stage(db, "st0103", 2).await.unwrap();

    let active = repository::active_stage(db, "st0103")
        .await
        .unwrap()
        .expect("активной версии нет");
    assert_eq!(active.version, 2);
    let first = repository::find_stage(db, "st0103", 1)
        .await
        .unwrap()
        .expect("первая версия исчезла");
    assert_eq!(first.status, DefinitionStatus::Archived);
}

/// Главное правило Б3: активация фиксирует версии Этапов. После неё публикация
/// Этапа не меняет поведение работающего Процесса — она становится видна
/// следующим планом активации, и только там.
#[tokio::test]
async fn activation_pins_stage_versions() {
    let db = database().await;
    definitions::save_stage(db, stage("st0104", &["готово"]), None)
        .await
        .unwrap();
    definitions::activate_stage(db, "st0104", 1).await.unwrap();

    definitions::save_process(
        db,
        process(
            "pr0101",
            "st0104",
            vec![edge("st0104", "готово", EdgeTarget::Done)],
        ),
        None,
    )
    .await
    .unwrap();
    let plan = definitions::activate_process(db, "pr0101", 1)
        .await
        .expect("активация Процесса не прошла");
    assert_eq!(plan.criticality, ProcessCriticality::ReadOnly);
    assert_eq!(plan.pinned_stages.len(), 1);
    assert_eq!(plan.pinned_stages[0].version, 1);

    // Новая версия Этапа выходит в активные — пин Процесса остаётся прежним.
    let mut updated = stage("st0104", &["готово"]);
    updated.manifest.title = "Пересчитать день заново".into();
    definitions::save_stage(db, updated, None).await.unwrap();
    definitions::activate_stage(db, "st0104", 2).await.unwrap();

    let pins = repository::process_pins(db, "pr0101", 1).await.unwrap();
    assert_eq!(pins[0].version, 1, "публикация Этапа сдвинула пин молча");

    let pinned = definitions::pinned_stages(db, "pr0101", 1).await.unwrap();
    assert_eq!(pinned["st0104"].manifest.title, "Этап st0104");
}

/// Diff двухуровневый: граф не тронут, а под ним поменялся Этап — и это должно
/// быть видно человеку до активации.
#[tokio::test]
async fn plan_shows_stage_change_when_the_graph_is_untouched() {
    let db = database().await;
    definitions::save_stage(db, stage("st0105", &["готово"]), None)
        .await
        .unwrap();
    definitions::activate_stage(db, "st0105", 1).await.unwrap();
    definitions::save_process(
        db,
        process(
            "pr0102",
            "st0105",
            vec![edge("st0105", "готово", EdgeTarget::Done)],
        ),
        None,
    )
    .await
    .unwrap();
    definitions::activate_process(db, "pr0102", 1)
        .await
        .unwrap();

    let mut updated = stage("st0105", &["готово", "ошибка"]);
    updated.script.push_str("\n// правка");
    definitions::save_stage(db, updated, None).await.unwrap();
    definitions::activate_stage(db, "st0105", 2).await.unwrap();

    // Тот же граф, но с ребром для нового выхода: иначе активацию остановит
    // «выход никуда не ведёт».
    definitions::save_process(
        db,
        process(
            "pr0102",
            "st0105",
            vec![
                edge("st0105", "готово", EdgeTarget::Done),
                edge("st0105", "ошибка", EdgeTarget::Done),
            ],
        ),
        None,
    )
    .await
    .unwrap();

    let plan = definitions::activation_plan(db, "pr0102", 2).await.unwrap();
    assert!(plan.is_allowed(), "{:?}", plan.problems);
    assert_eq!(plan.stages.len(), 1, "diff по Этапу не собран");
    let stage_diff = &plan.stages[0];
    assert_eq!(stage_diff.from_version, Some(1));
    assert_eq!(stage_diff.to_version, 2);
    assert!(
        stage_diff
            .changes
            .iter()
            .any(|change| change.contains("изменился код")),
        "{:?}",
        stage_diff.changes
    );
    assert!(
        plan.process
            .changes
            .iter()
            .any(|change| change.contains("добавлено ребро")),
        "{:?}",
        plan.process.changes
    );
}

/// Гейт ADR-0011 п.4: Процесс с эффектами не активируется без парной
/// quality-проверки, и критичность выводится из прав Этапа, а не из слов
/// автора.
#[tokio::test]
async fn critical_process_needs_a_paired_quality_check() {
    let db = database().await;
    let mut acting = stage("st0106", &["готово"]);
    acting.manifest.capabilities = vec!["action:rebuild_day_close".into()];
    definitions::save_stage(db, acting, None).await.unwrap();
    definitions::activate_stage(db, "st0106", 1).await.unwrap();

    definitions::save_process(
        db,
        process(
            "pr0103",
            "st0106",
            vec![edge("st0106", "готово", EdgeTarget::Done)],
        ),
        None,
    )
    .await
    .unwrap();

    let plan = definitions::activation_plan(db, "pr0103", 1).await.unwrap();
    assert_eq!(plan.criticality, ProcessCriticality::Effectful);
    assert!(
        plan.problems
            .iter()
            .any(|problem| problem.contains("quality-проверка не указана")),
        "{:?}",
        plan.problems
    );
    let error = definitions::activate_process(db, "pr0103", 1)
        .await
        .expect_err("критичный Процесс активировался без парной проверки");
    assert!(error.to_string().contains("не активирован"), "{error}");

    // Ссылка на несуществующую проверку — тоже отсутствие видимости.
    let mut with_ghost = process(
        "pr0103",
        "st0106",
        vec![edge("st0106", "готово", EdgeTarget::Done)],
    );
    with_ghost.manifest.quality_check = Some("нет_такой_проверки".into());
    definitions::save_process(db, with_ghost, None)
        .await
        .unwrap();
    let plan = definitions::activation_plan(db, "pr0103", 1).await.unwrap();
    assert!(
        plan.problems
            .iter()
            .any(|problem| problem.contains("не зарегистрирована")),
        "{:?}",
        plan.problems
    );

    let mut with_check = process(
        "pr0103",
        "st0106",
        vec![edge("st0106", "готово", EdgeTarget::Done)],
    );
    with_check.manifest.quality_check = Some(some_quality_check());
    definitions::save_process(db, with_check, None)
        .await
        .unwrap();
    definitions::activate_process(db, "pr0103", 1)
        .await
        .expect("Процесс с парной проверкой обязан активироваться");
}

/// Граф, который не сходится, до работы не допускается: Этап без активной
/// версии — это ссылка в пустоту.
#[tokio::test]
async fn graph_problems_block_activation() {
    let db = database().await;
    definitions::save_stage(db, stage("st0107", &["готово"]), None)
        .await
        .unwrap();
    definitions::activate_stage(db, "st0107", 1).await.unwrap();
    // st0108 существует только черновиком: в работу такой не идёт.
    definitions::save_stage(db, stage("st0108", &["готово"]), None)
        .await
        .unwrap();

    definitions::save_process(
        db,
        process(
            "pr0104",
            "st0107",
            vec![
                edge("st0107", "готово", EdgeTarget::stage("st0108")),
                edge("st0108", "готово", EdgeTarget::Done),
            ],
        ),
        None,
    )
    .await
    .unwrap();

    let plan = definitions::activation_plan(db, "pr0104", 1).await.unwrap();
    assert!(
        plan.problems
            .iter()
            .any(|problem| problem.contains("не найден в каталоге")),
        "{:?}",
        plan.problems
    );
    assert!(definitions::activate_process(db, "pr0104", 1)
        .await
        .is_err());
}

/// Сломанный Этап не должен доживать до экземпляра, и «сохраню пока так» —
/// ровно тот путь, которым он туда попадает.
#[tokio::test]
async fn broken_stage_is_rejected_on_save() {
    let db = database().await;
    let mut broken = stage("st0109", &["готово"]);
    broken.manifest.capabilities = vec!["action:нет_такого".into()];
    let error = definitions::save_stage(db, broken, None)
        .await
        .expect_err("Этап с несуществующим Действием сохранился");
    assert!(
        error.to_string().contains("несуществующее Действие"),
        "{error}"
    );
}

/// Заголовок списка и полное определение обязаны выбирать **одну и ту же**
/// версию.
///
/// Разойдись они — карточка на странице «Процессы» показывала бы граф одной
/// версии, а кнопки под ней работали бы с другой. Ловушка здесь неочевидная:
/// активная версия старше свежего черновика, и «самая новая» — неправильный
/// ответ.
#[tokio::test]
async fn head_record_and_head_row_agree_on_the_version() {
    let db = database().await;

    definitions::save_stage(db, stage("st0110", &["готово"]), None)
        .await
        .unwrap();
    definitions::activate_stage(db, "st0110", 1).await.unwrap();
    // Черновик поверх активной версии: он новее, но головой не становится.
    definitions::save_stage(db, stage("st0110", &["готово", "не вышло"]), None)
        .await
        .unwrap();

    definitions::save_process(
        db,
        process(
            "pr0105",
            "st0110",
            vec![edge("st0110", "готово", EdgeTarget::Done)],
        ),
        None,
    )
    .await
    .unwrap();

    let row = repository::list_stage_heads(db)
        .await
        .unwrap()
        .into_iter()
        .find(|item| item.code == "st0110")
        .expect("Этап в заголовках списка");
    let record = repository::list_stage_head_records(db)
        .await
        .unwrap()
        .into_iter()
        .find(|item| item.code == "st0110")
        .expect("Этап в полных определениях");

    assert_eq!(record.version, row.version);
    assert_eq!(record.status, DefinitionStatus::Active);
    assert_eq!(record.version, 1, "активная версия старше черновика v2");
    // Ради определения всё и затевалось: без него карточке нечего показать.
    assert_eq!(record.definition.manifest.output_names(), vec!["готово"]);

    let process_row = repository::list_process_heads(db)
        .await
        .unwrap()
        .into_iter()
        .find(|item| item.code == "pr0105")
        .expect("Процесс в заголовках списка");
    let process_record = repository::list_process_head_records(db)
        .await
        .unwrap()
        .into_iter()
        .find(|item| item.code == "pr0105")
        .expect("Процесс в полных определениях");

    assert_eq!(process_record.version, process_row.version);
    assert_eq!(process_record.definition.manifest.entry, "st0110");
    assert_eq!(process_record.definition.manifest.edges.len(), 1);
}
