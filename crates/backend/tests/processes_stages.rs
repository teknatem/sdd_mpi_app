//! Веха 2 механизма Процессов: Этап принимает решение.
//!
//! Проверяется контракт выходов и — отдельно — разделение трёх классов исхода
//! (ADR-0011 п.10). Последнее важнее всего остального в этом файле: если дефект
//! Этапа неотличим от временного сбоя, движок будет бесконечно повторять
//! сломанный код, а если неотличим от штатного выхода — тихая неправильность
//! заменит громкое падение.

use backend::processes::stages;
use backend::shared::data::db;
use contracts::processes::{
    ActionMode, StageDefinition, StageManifest, StageOutput, StageRunContext, StageVerdict,
};
use sea_orm::DatabaseConnection;
use serde_json::{json, Value};

async fn database() -> &'static DatabaseConnection {
    db::init_test_database()
        .await
        .expect("тестовая база не поднялась");
    db::get_connection()
}

fn stage(script: &str, outputs: Vec<StageOutput>, capabilities: Vec<String>) -> StageDefinition {
    StageDefinition {
        manifest: StageManifest {
            code: "st0002".into(),
            title: "Сверить с ГК".into(),
            description: String::new(),
            entrypoint: "stage.mjs".into(),
            export: "run".into(),
            input_schema: None,
            outputs,
            capabilities,
        },
        script: script.to_string(),
        digest: String::new(),
    }
}

fn output(name: &str) -> StageOutput {
    StageOutput {
        name: name.into(),
        description: String::new(),
        data_schema: None,
    }
}

async fn run(definition: &StageDefinition, input: Value, mode: ActionMode) -> StageVerdict {
    let db = database().await;
    stages::run(db, definition, input, &StageRunContext::manual(mode))
        .await
        .expect("прогон Этапа не состоялся")
        .verdict
}

/// Штатный путь: Этап читает вход, ветвится и возвращает объявленный выход.
#[tokio::test]
async fn stage_returns_declared_outcome() {
    let definition = stage(
        r#"
        export async function run(input, host) {
          host.log.info("расхождение:", input.diff);
          if (input.diff === 0) return { outcome: "сходится" };
          return { outcome: "расхождение", data: { amount: input.diff } };
        }
        "#,
        vec![output("сходится"), output("расхождение")],
        vec![],
    );

    let verdict = run(&definition, json!({ "diff": 0 }), ActionMode::DryRun).await;
    assert_eq!(verdict.outcome_name(), Some("сходится"));

    let verdict = run(&definition, json!({ "diff": 1250 }), ActionMode::DryRun).await;
    match verdict {
        StageVerdict::Outcome(outcome) => {
            assert_eq!(outcome.outcome, "расхождение");
            assert_eq!(outcome.data["amount"], json!(1250));
        }
        other => panic!("ожидали штатный выход, получили {other:?}"),
    }
}

/// Выход, которого нет в манифесте, — дефект: граф дальше не читается.
#[tokio::test]
async fn undeclared_outcome_is_a_defect() {
    let definition = stage(
        r#"export async function run() { return { outcome: "почти сходится" }; }"#,
        vec![output("сходится")],
        vec![],
    );

    match run(&definition, json!({}), ActionMode::DryRun).await {
        StageVerdict::Defect { message } => {
            assert!(message.contains("почти сходится"), "{message}");
            assert!(
                message.contains("сходится"),
                "в сообщении нет объявленных выходов: {message}"
            );
        }
        other => panic!("ожидали дефект, получили {other:?}"),
    }
}

/// Данные выхода проверяются против схемы этого выхода уже в рантайме.
#[tokio::test]
async fn output_data_is_validated_against_schema() {
    let definition = stage(
        r#"export async function run() { return { outcome: "расхождение", data: { amount: "много" } }; }"#,
        vec![StageOutput {
            name: "расхождение".into(),
            description: String::new(),
            data_schema: Some(json!({
                "type": "object",
                "required": ["amount"],
                "properties": { "amount": { "type": "number" } }
            })),
        }],
        vec![],
    );

    match run(&definition, json!({}), ActionMode::DryRun).await {
        StageVerdict::Defect { message } => {
            assert!(message.contains("не по схеме"), "{message}")
        }
        other => panic!("ожидали дефект, получили {other:?}"),
    }
}

/// Исключение в самом Этапе — дефект, а не повод повторять.
#[tokio::test]
async fn exception_in_stage_is_a_defect() {
    let definition = stage(
        r#"export async function run() { throw new Error("автор ошибся"); }"#,
        vec![output("сходится")],
        vec![],
    );

    match run(&definition, json!({}), ActionMode::DryRun).await {
        StageVerdict::Defect { message } => assert!(message.contains("автор ошибся"), "{message}"),
        other => panic!("ожидали дефект, получили {other:?}"),
    }
}

/// Упавшее Действие — временный сбой внешнего мира: Этап повторяем, в карантин
/// не уходит. Это ровно та развилка, ради которой заведён отдельный класс.
#[tokio::test]
async fn failed_action_is_a_temporary_failure() {
    let definition = stage(
        r#"
        export async function run(input, host) {
          await host.actions.runQualityCheck({ check_id: "нет-такой-проверки" });
          return { outcome: "сходится" };
        }
        "#,
        vec![output("сходится")],
        vec!["action:run_quality_check".into()],
    );

    match run(&definition, json!({}), ActionMode::Execute).await {
        StageVerdict::TemporaryFailure { message } => {
            assert!(message.contains("run_quality_check"), "{message}")
        }
        other => panic!("ожидали временный сбой, получили {other:?}"),
    }
}

/// Действие, на которое нет права, не запрещено проверкой — его просто нет в
/// `host.actions`. Скрипт падает на обращении к несуществующему методу.
#[tokio::test]
async fn action_without_capability_is_absent_from_host() {
    let definition = stage(
        r#"
        export async function run(input, host) {
          await host.actions.createAgentTask({ title: "т", request_text: "т", target_agent_type: "financier" });
          return { outcome: "сходится" };
        }
        "#,
        vec![output("сходится")],
        // Право на чтение есть, права на эффект — нет.
        vec!["db:read:*".into()],
    );

    match run(&definition, json!({}), ActionMode::Execute).await {
        StageVerdict::Defect { .. } => {}
        other => panic!("ожидали дефект из-за отсутствия метода, получили {other:?}"),
    }
}

/// Этап действительно меняет мир — и журнал эффектов это фиксирует.
#[tokio::test]
async fn stage_effect_reaches_the_effect_log() {
    let db = database().await;
    let definition = stage(
        r#"
        export async function run(input, host) {
          const effect = await host.actions.createAgentTask(
            { title: "Разобрать расхождение", request_text: "День не сходится", target_agent_type: "financier" },
            { key: "веха-2" }
          );
          return { outcome: "поручено", data: { task_id: effect.result.task_id, replayed: effect.replayed } };
        }
        "#,
        vec![output("поручено")],
        vec!["action:create_agent_task".into()],
    );

    let run = stages::run(
        db,
        &definition,
        json!({}),
        &StageRunContext::for_instance("тест-экземпляр", 1, ActionMode::Execute),
    )
    .await
    .expect("прогон Этапа не состоялся");

    assert_eq!(run.verdict.outcome_name(), Some("поручено"));
    assert_eq!(
        run.effect_ids.len(),
        1,
        "прогон обязан перечислить свои эффекты: {:?}",
        run.effect_ids
    );

    // Ключ идемпотентности собран из контекста прогона, поэтому повтор того же
    // Этапа на том же экземпляре не создаёт второе поручение.
    let again = stages::run(
        db,
        &definition,
        json!({}),
        &StageRunContext::for_instance("тест-экземпляр", 1, ActionMode::Execute),
    )
    .await
    .expect("повторный прогон Этапа не состоялся");

    match again.verdict {
        StageVerdict::Outcome(outcome) => {
            assert_eq!(
                outcome.data["replayed"],
                json!(true),
                "повтор Этапа исполнил эффект заново"
            );
        }
        other => panic!("ожидали штатный выход, получили {other:?}"),
    }
}

/// Сухой прогон Этапа не может исполнить настоящий эффект, что бы ни написал
/// автор mjs: режим задаётся снаружи и внутрь не прокидывается.
#[tokio::test]
async fn dry_run_stage_cannot_execute_a_real_effect() {
    let definition = stage(
        r#"
        export async function run(input, host) {
          const effect = await host.actions.createAgentTask(
            { title: "Не должно появиться", request_text: "т", target_agent_type: "financier" },
            { key: "веха-2-сухой" }
          );
          return { outcome: "поручено", data: { status: effect.status } };
        }
        "#,
        vec![output("поручено")],
        vec!["action:create_agent_task".into()],
    );

    match run(&definition, json!({}), ActionMode::DryRun).await {
        StageVerdict::Outcome(outcome) => {
            assert_eq!(
                outcome.data["status"],
                json!("planned"),
                "в сухом прогоне эффект получил боевой статус"
            );
        }
        other => panic!("ожидали штатный выход, получили {other:?}"),
    }
}

/// Чтение из базы у Этапа работает и по-прежнему закрыто правами.
#[tokio::test]
async fn stage_can_read_the_database() {
    let definition = stage(
        r#"
        export async function run(input, host) {
          const rows = await host.db.query("SELECT COUNT(*) AS c FROM a042_agent_task", []);
          return { outcome: "прочитано", data: { rows: rows.length } };
        }
        "#,
        vec![output("прочитано")],
        vec!["db:read:*".into()],
    );

    match run(&definition, json!({}), ActionMode::DryRun).await {
        StageVerdict::Outcome(outcome) => assert_eq!(outcome.data["rows"], json!(1)),
        other => panic!("ожидали штатный выход, получили {other:?}"),
    }
}
