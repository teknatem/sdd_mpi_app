//! Веха 1 механизма Процессов: Действие работает и оно безопасно.
//!
//! Проверяется не «функция вызвалась», а три обещания контракта Действия
//! (ADR-0011 п.8, п.10):
//!
//! 1. повтор с тем же ключом идемпотентности не делает эффект вторым;
//! 2. сухой прогон ничего не меняет и оставляет в журнале план;
//! 3. просмотр плана не закрывает дорогу настоящему исполнению с тем же ключом.
//!
//! Действие выбрано самое дешёвое из боевых — `create_agent_task`: у него
//! настоящий эффект (строка в очереди `a042`), который видно счётчиком, и нет
//! зависимости от импортированных данных.

use backend::processes::{actions, effect_log};
use backend::shared::data::db;
use contracts::processes::{ActionActor, ActionCall, ActionMode, EffectStatus};
use sea_orm::DatabaseConnection;
use serde_json::json;

async fn database() -> DatabaseConnection {
    // Состав системы ставится composition root'ом — у интеграционного теста
    // своего `main` нет, а реестры (Действия, регистраторы) нужны те же.
    // Вызов идемпотентен, поэтому его делает каждый тест, не сговариваясь.
    backend::composition::install_all();
    db::init_test_database()
        .await
        .expect("тестовая база не поднялась")
}

fn call(key: &str, mode: ActionMode, title: &str) -> ActionCall {
    ActionCall {
        action: "create_agent_task".to_string(),
        input: json!({
            "title": title,
            "request_text": "Проверить закрытие дня за вчера",
            "target_agent_type": "financier",
        }),
        idempotency_key: key.to_string(),
        mode,
        actor: ActionActor::Manual,
    }
}

/// Повтор не должен породить второй эффект: возвращается записанный результат
/// первой попытки, и вызывающий видит это по флагу `replayed`.
///
/// Считать поручения глобальным счётчиком здесь нельзя: тесты одного бинаря
/// делят базу и идут параллельно, поэтому «стало на одно больше» — утверждение
/// не об этом Действии. Проверяется адресно: тот же id поручения в обоих
/// результатах, ровно одна боевая запись журнала на ключ и живое поручение с
/// этим id в очереди.
#[tokio::test]
async fn repeat_with_same_key_does_not_duplicate_effect() {
    let db = database().await;
    let key = "test:idempotency:поручение-1";

    let first = actions::run(&db, &call(key, ActionMode::Execute, "Первый вызов"))
        .await
        .expect("первый вызов Действия упал");
    assert_eq!(first.status, EffectStatus::Executed);
    assert!(!first.replayed, "первый вызов не может быть повтором");

    let second = actions::run(&db, &call(key, ActionMode::Execute, "Второй вызов"))
        .await
        .expect("повторный вызов Действия упал");
    assert!(
        second.replayed,
        "повтор с тем же ключом обязан вернуть записанный результат"
    );
    assert_eq!(
        first.result, second.result,
        "повтор вернул другой результат — значит, эффект был исполнен заново"
    );
    assert_eq!(
        first.effect_id, second.effect_id,
        "повтор завёл вторую запись журнала на тот же ключ"
    );

    let executed_rows = effect_log::list_recent(&db, 200)
        .await
        .expect("журнал эффектов не читается")
        .into_iter()
        .filter(|record| record.idempotency_key == key && record.mode == ActionMode::Execute)
        .count();
    assert_eq!(
        executed_rows, 1,
        "на один ключ идемпотентности должна быть ровно одна боевая запись"
    );

    // Эффект действительно состоялся — и ровно один раз.
    let task_id = first.result["task_id"]
        .as_str()
        .expect("Действие не вернуло id поручения");
    let task = backend::domain::a042_agent_task::service::get_by_id(task_id)
        .await
        .expect("поручение не читается");
    assert!(
        task.is_some(),
        "записан эффект, которого нет: поручение {task_id} не найдено"
    );
}

/// Сухой прогон обязан вернуть план и не тронуть мир.
///
/// «Мир не тронут» проверяется по журналу, а не счётчиком поручений: журнал
/// привязан к ключу идемпотентности, а счётчик глобален — соседний тест, идущий
/// параллельно в том же бинаре и той же базе, сделал бы проверку недостоверной.
/// И это не обход неудобства: раз каждый эффект проходит через журнал,
/// отсутствие боевой записи по ключу и есть отсутствие эффекта.
#[tokio::test]
async fn dry_run_records_plan_without_effect() {
    let db = database().await;
    let key = "test:dry-run:поручение-2";

    let outcome = actions::run(&db, &call(key, ActionMode::DryRun, "Только план"))
        .await
        .expect("сухой прогон упал");

    assert_eq!(outcome.status, EffectStatus::Planned);
    assert!(
        outcome.result.get("effect").is_some(),
        "план обязан описывать, что произошло бы: {}",
        outcome.result
    );

    let executed_now = effect_log::list_recent(&db, 200)
        .await
        .expect("журнал эффектов не читается")
        .into_iter()
        .filter(|record| record.idempotency_key == key && record.mode == ActionMode::Execute)
        .count();
    assert_eq!(
        executed_now, 0,
        "после сухого прогона в журнале появился боевой эффект"
    );

    // Ключ остаётся свободным: план не должен закрывать дорогу исполнению.
    let executed = actions::run(&db, &call(key, ActionMode::Execute, "Теперь по-настоящему"))
        .await
        .expect("исполнение после сухого прогона упало");
    assert_eq!(executed.status, EffectStatus::Executed);
    assert!(
        !executed.replayed,
        "план сухого прогона был засчитан за исполненный эффект"
    );

    let recent = effect_log::list_recent(&db, 50)
        .await
        .expect("журнал эффектов не читается");
    let planned = recent
        .iter()
        .filter(|record| record.idempotency_key == key && record.mode == ActionMode::DryRun)
        .count();
    let done = recent
        .iter()
        .filter(|record| record.idempotency_key == key && record.mode == ActionMode::Execute)
        .count();
    assert_eq!(planned, 1, "план сухого прогона не попал в журнал");
    assert_eq!(done, 1, "боевая запись по ключу должна быть ровно одна");
}

/// Вход проверяется по схеме Действия до всякого эффекта.
#[tokio::test]
async fn input_is_validated_before_any_effect() {
    let db = database().await;
    let mut bad = call("test:schema:поручение-3", ActionMode::Execute, "Без текста");
    bad.input = json!({ "title": "Без обязательных полей" });

    let error = actions::run(&db, &bad)
        .await
        .expect_err("вход не по схеме обязан быть отклонён");
    assert!(
        error.to_string().contains("не по схеме"),
        "ожидали отказ по схеме, получили: {error}"
    );

    let recent = effect_log::list_recent(&db, 50)
        .await
        .expect("журнал эффектов не читается");
    assert!(
        !recent
            .iter()
            .any(|record| record.idempotency_key == "test:schema:поручение-3"),
        "отклонённый по схеме вызов оставил запись в журнале эффектов"
    );
}

/// Вызов без ключа идемпотентности — ошибка контракта, а не «ну ладно».
#[tokio::test]
async fn empty_idempotency_key_is_rejected() {
    let db = database().await;
    let error = actions::run(&db, &call("   ", ActionMode::Execute, "Без ключа"))
        .await
        .expect_err("пустой ключ обязан быть отклонён");
    assert!(
        error.to_string().contains("ключа идемпотентности"),
        "ожидали отказ по ключу, получили: {error}"
    );
}

/// Каталог Действий и его паспорта — то, на что будут опираться манифесты
/// Этапов. Расхождение имени и capability обнаружится только в рантайме, поэтому
/// проверяется здесь.
#[tokio::test]
async fn catalog_is_addressable() {
    backend::composition::install_all();
    assert!(actions::exists("create_agent_task"));
    assert!(!actions::exists("нет-такого-действия"));

    let names: Vec<&str> = actions::list().iter().map(|info| info.name).collect();
    assert!(
        names.contains(&"rebuild_day_close") && names.contains(&"repost_documents"),
        "каталог Действий неполон: {names:?}"
    );
}
