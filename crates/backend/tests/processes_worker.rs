//! Б5 механизма Процессов: экземпляр живёт дольше запуска.
//!
//! Это веха 3 плана, и проверяется здесь именно она: событие заводит прогон,
//! прогон идёт по графу, встаёт в ожидание, просыпается от факта и доходит до
//! конца. Плюс три места, где механизм обязан вести себя не так, как «просто
//! ретраить»:
//!
//! - повторное событие про тот же день **не** заводит второй прогон;
//! - дефект Этапа уводит в карантин, а не в бесконечный повтор;
//! - цикл в графе действительно пересчитывает — ключ идемпотентности несёт
//!   номер захода, иначе второй проход вернул бы «уже делали».

use backend::processes::{definitions, effect_log, events, instances, worker};
use backend::shared::data::db;
use contracts::processes::{
    CorrelationKey, DomainEventKind, EdgeTarget, InstanceStatus, ProcessDefinition, ProcessEdge,
    ProcessManifest, ProcessTrigger, StageDefinition, StageManifest, StageOutput, WaitSpec,
};
use sea_orm::DatabaseConnection;
use serde_json::json;

/// Тесты одного бинаря делят базу, а проход воркера **глобален**: он двигает
/// все экземпляры разом. Поэтому здесь они идут по одному — иначе счётчики
/// прохода считали бы чужую работу, а чужой проход двигал бы наш экземпляр
/// между нашими же проверками.
static WORKER: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

async fn database() -> &'static DatabaseConnection {
    db::init_test_database()
        .await
        .expect("тестовая база не поднялась");
    db::get_connection()
}

/// Прогнать проход воркера. Счётчики прохода намеренно не проверяются: они
/// глобальные, а утверждать надо про свой экземпляр.
async fn tick(db: &'static DatabaseConnection) {
    worker::tick(db).await.expect("проход воркера упал");
}

fn stage(code: &str, outputs: &[&str], script: &str) -> StageDefinition {
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
        script: script.to_string(),
        digest: String::new(),
    }
}

fn returns(outcome: &str) -> String {
    format!("export async function run() {{ return {{ outcome: '{outcome}' }}; }}")
}

fn edge(from: &str, outcome: &str, to: EdgeTarget) -> ProcessEdge {
    ProcessEdge {
        from: from.into(),
        outcome: outcome.into(),
        to,
        wait: None,
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

async fn publish_stage(db: &'static DatabaseConnection, definition: StageDefinition) {
    let code = definition.manifest.code.clone();
    let record = definitions::save_stage(db, definition, None)
        .await
        .unwrap_or_else(|error| panic!("Этап {code} не сохранён: {error}"));
    definitions::activate_stage(db, &code, record.version)
        .await
        .unwrap_or_else(|error| panic!("Этап {code} не активирован: {error}"));
}

async fn publish_process(db: &'static DatabaseConnection, definition: ProcessDefinition) {
    let code = definition.manifest.code.clone();
    let record = definitions::save_process(db, definition, None)
        .await
        .expect("Процесс не сохранён");
    let plan = definitions::activate_process(db, &code, record.version)
        .await
        .unwrap_or_else(|error| panic!("Процесс {code} не активирован: {error}"));
    assert!(plan.is_allowed());
}

fn day(connection: &str) -> CorrelationKey {
    CorrelationKey::new()
        .with("connection_id", connection)
        .with("business_date", "2026-08-21")
}

async fn publish_day(db: &'static DatabaseConnection, connection: &str) {
    events::publish(
        db,
        DomainEventKind::ImportDayCompleted,
        day(connection),
        json!({}),
        "тест",
    )
    .await
    .expect("событие не опубликовано");
}

/// Найти экземпляр процесса по коду — тесты делят базу, поэтому адресуемся
/// кодом Процесса, а не «последним заведённым».
async fn instance_of(db: &'static DatabaseConnection, process_code: &str) -> ProcessInstanceView {
    let all = instances::list_recent(db, 200).await.unwrap();
    let found = all
        .into_iter()
        .find(|instance| instance.process_code == process_code)
        .unwrap_or_else(|| panic!("экземпляр {process_code} не заведён"));
    ProcessInstanceView {
        id: found.id,
        status: found.status,
        stage_code: found.stage_code,
        visit: found.visit,
        last_error: found.last_error,
        wait_event: found.wait.map(|wait| wait.event),
    }
}

struct ProcessInstanceView {
    id: String,
    status: InstanceStatus,
    stage_code: Option<String>,
    visit: i32,
    last_error: Option<String>,
    wait_event: Option<String>,
}

/// Полный проход: факт → экземпляр → Этап → следующий Этап → завершение.
#[tokio::test]
async fn event_starts_an_instance_and_the_graph_carries_it_to_the_end() {
    let _guard = WORKER.lock().await;
    let db = database().await;
    publish_stage(db, stage("st0201", &["готово"], &returns("готово"))).await;
    publish_stage(db, stage("st0202", &["сходится"], &returns("сходится"))).await;
    publish_process(
        db,
        process(
            "pr0201",
            "st0201",
            vec![
                edge("st0201", "готово", EdgeTarget::stage("st0202")),
                edge("st0202", "сходится", EdgeTarget::Done),
            ],
        ),
    )
    .await;

    publish_day(db, "worker-basic").await;

    tick(db).await;

    let after_first = instance_of(db, "pr0201").await;
    assert_eq!(after_first.status, InstanceStatus::Running);
    assert_eq!(after_first.stage_code.as_deref(), Some("st0202"));

    tick(db).await;
    let finished = instance_of(db, "pr0201").await;
    assert_eq!(finished.status, InstanceStatus::Done);
    assert_eq!(finished.stage_code, None);
}

/// Повторный факт про тот же день не заводит второй прогон: иначе эффекты
/// сделались бы дважды.
#[tokio::test]
async fn repeated_fact_does_not_start_a_second_instance() {
    let _guard = WORKER.lock().await;
    let db = database().await;
    publish_stage(
        db,
        stage(
            "st0203",
            &["ждём"],
            "export async function run() { return { outcome: 'ждём' }; }",
        ),
    )
    .await;
    let mut definition = process(
        "pr0202",
        "st0203",
        vec![edge("st0203", "ждём", EdgeTarget::Done)],
    );
    // Ожидание держит экземпляр живым, пока приходит второй такой же факт.
    definition.manifest.edges[0].wait = Some(WaitSpec {
        event: "human.action.done".into(),
        deadline_minutes: 24 * 60,
        on_timeout: None,
    });
    publish_process(db, definition).await;

    publish_day(db, "worker-dup").await;
    tick(db).await;
    let waiting = instance_of(db, "pr0202").await;
    assert_eq!(waiting.status, InstanceStatus::Waiting);

    publish_day(db, "worker-dup").await;
    tick(db).await;

    let live = instances::list_recent(db, 200)
        .await
        .unwrap()
        .into_iter()
        .filter(|instance| instance.process_code == "pr0202")
        .count();
    assert_eq!(live, 1);
}

/// Веха 3: экземпляр встал в ожидание, факт пришёл позже — и он продолжился с
/// того же узла. Перезапуск сервера здесь заменяет отдельный проход воркера:
/// состояние живёт в БД, а не в памяти.
#[tokio::test]
async fn waiting_instance_resumes_on_the_awaited_fact() {
    let _guard = WORKER.lock().await;
    let db = database().await;
    publish_stage(db, stage("st0204", &["позвали"], &returns("позвали"))).await;
    publish_stage(db, stage("st0205", &["закрыто"], &returns("закрыто"))).await;
    let mut definition = process(
        "pr0203",
        "st0204",
        vec![
            edge("st0204", "позвали", EdgeTarget::stage("st0205")),
            edge("st0205", "закрыто", EdgeTarget::Done),
        ],
    );
    definition.manifest.edges[0].wait = Some(WaitSpec {
        event: "human.action.done".into(),
        deadline_minutes: 24 * 60,
        on_timeout: None,
    });
    publish_process(db, definition).await;

    publish_day(db, "worker-wait").await;
    tick(db).await;

    let waiting = instance_of(db, "pr0203").await;
    assert_eq!(waiting.status, InstanceStatus::Waiting);
    assert_eq!(waiting.wait_event.as_deref(), Some("human.action.done"));

    // Пустой проход ничего не меняет: ожидание не «протухает» само.
    tick(db).await;
    assert_eq!(
        instance_of(db, "pr0203").await.status,
        InstanceStatus::Waiting
    );

    // Человек нажал «сделано»: ключ просьбы — токен экземпляра.
    events::publish(
        db,
        DomainEventKind::HumanActionDone,
        CorrelationKey::new().with(
            "request_key",
            "connection_id=worker-wait;business_date=2026-08-21",
        ),
        json!({ "by": "человек" }),
        "ui",
    )
    .await
    .unwrap();

    tick(db).await;
    let resumed = instance_of(db, "pr0203").await;
    assert!(
        matches!(
            resumed.status,
            InstanceStatus::Running | InstanceStatus::Done
        ),
        "{:?}",
        resumed.status
    );

    tick(db).await;
    assert_eq!(instance_of(db, "pr0203").await.status, InstanceStatus::Done);
}

/// Дефект Этапа — карантин, а не бесконечный повтор (ADR-0011 п.10).
#[tokio::test]
async fn defect_sends_the_instance_to_quarantine() {
    let _guard = WORKER.lock().await;
    let db = database().await;
    publish_stage(
        db,
        stage(
            "st0206",
            &["готово"],
            "export async function run() { throw new Error('сломался'); }",
        ),
    )
    .await;
    publish_process(
        db,
        process(
            "pr0204",
            "st0206",
            vec![edge("st0206", "готово", EdgeTarget::Done)],
        ),
    )
    .await;

    publish_day(db, "worker-defect").await;
    tick(db).await;

    let broken = instance_of(db, "pr0204").await;
    assert_eq!(broken.status, InstanceStatus::Quarantined);
    assert!(
        broken
            .last_error
            .as_deref()
            .is_some_and(|text| text.contains("сломался")),
        "{:?}",
        broken.last_error
    );

    // Карантин воркер больше не трогает: повтор бессмыслен до правки кода.
    tick(db).await;
    let still = instance_of(db, "pr0204").await;
    assert_eq!(still.status, InstanceStatus::Quarantined);
    assert_eq!(still.visit, broken.visit, "карантинный экземпляр сдвинулся");
}

/// Дедлайн ожидания публикуется фактом и уводит экземпляр по запасному ребру.
#[tokio::test]
async fn expired_wait_publishes_a_fact_and_takes_the_fallback_edge() {
    let _guard = WORKER.lock().await;
    let db = database().await;
    publish_stage(db, stage("st0207", &["позвали"], &returns("позвали"))).await;
    publish_stage(db, stage("st0208", &["добито"], &returns("добито"))).await;
    let mut definition = process(
        "pr0205",
        "st0207",
        vec![
            edge("st0207", "позвали", EdgeTarget::stage("st0208")),
            edge("st0208", "добито", EdgeTarget::Done),
        ],
    );
    definition.manifest.edges[0].wait = Some(WaitSpec {
        event: "human.action.done".into(),
        // Минимальный дедлайн: ждать в тесте нечего, время истечёт само.
        deadline_minutes: 1,
        on_timeout: Some(EdgeTarget::stage("st0208")),
    });
    publish_process(db, definition).await;

    publish_day(db, "worker-timeout").await;
    tick(db).await;
    assert_eq!(
        instance_of(db, "pr0205").await.status,
        InstanceStatus::Waiting
    );

    // Сдвигаем дедлайн в прошлое: ждать минуту в тесте нельзя, а проверяется
    // решение воркера, а не работа часов.
    let id = instance_of(db, "pr0205").await.id;
    expire_wait_now(db, &id).await;

    tick(db).await;

    let timeouts = events::list_recent(db, 50)
        .await
        .unwrap()
        .into_iter()
        .filter(|event| event.kind == DomainEventKind::ProcessInstanceTimeout)
        .filter(|event| event.correlation.get("instance_id") == Some(id.as_str()))
        .count();
    assert_eq!(timeouts, 1, "факт о дедлайне не опубликован");

    tick(db).await;
    assert_eq!(instance_of(db, "pr0205").await.status, InstanceStatus::Done);
}

/// Цикл в графе обязан **пересчитывать**. Ключ идемпотентности несёт номер
/// захода, поэтому второй проход по тому же Этапу — это новый эффект, а не
/// `replayed` (открытое место Б2, пункт 1).
#[tokio::test]
async fn a_second_visit_to_a_stage_is_not_a_replay() {
    let _guard = WORKER.lock().await;
    let db = database().await;

    // Этап с эффектом: смысловой ключ (`options.key`) у обоих заходов один и
    // тот же — именно поэтому различать их обязан номер захода.
    let mut acting = stage("st0209", &["первый", "второй"], "");
    acting.manifest.capabilities = vec!["action:create_agent_task".into()];
    acting.script = r#"
        export async function run(input, host) {
          await host.actions.createAgentTask(
            { title: "Разобрать день", request_text: "Цикл", target_agent_type: "financier" },
            { key: "цикл" }
          );
          return { outcome: input.again ? "второй" : "первый" };
        }
    "#
    .into();
    publish_stage(db, acting).await;
    publish_stage(
        db,
        stage(
            "st0210",
            &["назад"],
            "export async function run() { return { outcome: 'назад', data: { again: true } }; }",
        ),
    )
    .await;
    let mut definition = process(
        "pr0206",
        "st0209",
        vec![
            edge("st0209", "первый", EdgeTarget::stage("st0210")),
            edge("st0210", "назад", EdgeTarget::stage("st0209")),
            edge("st0209", "второй", EdgeTarget::Done),
        ],
    );
    // Процесс с эффектом критичен, а критичный не активируется без парной
    // проверки (ADR-0011 п.4) — гейт работает и в тесте.
    definition.manifest.quality_check = Some(
        backend::quality::list_checks()
            .first()
            .expect("в каталоге нет ни одной quality-проверки")
            .id
            .clone(),
    );
    publish_process(db, definition).await;

    publish_day(db, "worker-cycle").await;
    for _ in 0..3 {
        tick(db).await;
    }

    let finished = instance_of(db, "pr0206").await;
    assert_eq!(
        finished.status,
        InstanceStatus::Done,
        "цикл не дошёл до конца: {:?}",
        finished.last_error
    );
    assert!(
        finished.visit >= 3,
        "заходов меньше трёх: {}",
        finished.visit
    );

    // Мерим адресно: тесты одного бинаря делят базу, поэтому глобальный
    // счётчик эффектов был бы плавающим.
    let keys = effect_keys(db, "st0209").await;
    assert_eq!(
        keys.len(),
        2,
        "второй заход не сделал эффекта заново: {keys:?}"
    );
    assert_ne!(keys[0], keys[1], "у двух заходов совпал ключ: {keys:?}");
}

/// Ключи идемпотентности, записанные журналом эффектов для одного Этапа.
async fn effect_keys(db: &'static DatabaseConnection, stage_code: &str) -> Vec<String> {
    let mut keys: Vec<String> = effect_log::list_recent(db, 200)
        .await
        .unwrap()
        .into_iter()
        .filter(|record| record.stage_code.as_deref() == Some(stage_code))
        .map(|record| record.idempotency_key)
        .collect();
    keys.sort();
    keys.dedup();
    keys
}

/// Сдвинуть дедлайн ожидания в прошлое.
async fn expire_wait_now(db: &'static DatabaseConnection, id: &str) {
    use sea_orm::{ConnectionTrait, DatabaseBackend, Statement};
    db.execute(Statement::from_sql_and_values(
        DatabaseBackend::Sqlite,
        "UPDATE sys_process_instance SET wait_deadline_at = ? WHERE id = ?",
        vec!["2000-01-01T00:00:00Z".into(), id.into()],
    ))
    .await
    .expect("дедлайн не сдвинулся");
}
