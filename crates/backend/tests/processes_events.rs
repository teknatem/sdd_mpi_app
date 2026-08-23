//! Б4 механизма Процессов: доменные события.
//!
//! Проверяется журнал фактов, а не их производители: ADR-0011 п.5 разрешает
//! публиковать только из ядра и вручную, поэтому важно, что **нельзя**
//! опубликовать (событие не из каталога, ключ не по каталогу) и что сведение
//! ожидания с фактом идёт по каноническому токену, а не по «похожести».

use backend::processes::events;
use backend::shared::data::db;
use contracts::processes::{CorrelationKey, DomainEventKind};
use sea_orm::DatabaseConnection;
use serde_json::json;

async fn database() -> &'static DatabaseConnection {
    db::init_test_database()
        .await
        .expect("тестовая база не поднялась");
    db::get_connection()
}

fn day(connection: &str, date: &str) -> CorrelationKey {
    CorrelationKey::new()
        .with("connection_id", connection)
        .with("business_date", date)
}

/// Публикация записывает факт целиком: ключ, токен и данные сверх ключа.
#[tokio::test]
async fn published_fact_keeps_its_key_and_payload() {
    let db = database().await;
    let event = events::publish(
        db,
        DomainEventKind::ImportDayCompleted,
        day("conn-a", "2026-08-21"),
        json!({ "rows": 1200 }),
        "u504",
    )
    .await
    .expect("событие не опубликовано");

    assert_eq!(event.kind, DomainEventKind::ImportDayCompleted);
    assert_eq!(
        event.correlation_token,
        "connection_id=conn-a;business_date=2026-08-21"
    );
    assert_eq!(event.payload["rows"], 1200);
    assert_eq!(event.source, "u504");
    assert!(event.seq > 0, "номер публикации не присвоен");
}

/// Ключ не по каталогу не пишется вовсе: токен, собранный не по правилу, не
/// сведётся с ожиданием, и потеря будет молчаливой.
#[tokio::test]
async fn key_outside_the_catalog_is_refused_at_publication() {
    let db = database().await;
    let incomplete = CorrelationKey::new().with("connection_id", "conn-b");
    let error = events::publish(
        db,
        DomainEventKind::ImportDayCompleted,
        incomplete,
        json!({}),
        "тест",
    )
    .await
    .expect_err("событие с неполным ключом опубликовалось");
    assert!(error.to_string().contains("не хватает"), "{error}");
}

/// Пробуждение ожидающего экземпляра: событие ищется по виду и токену, а
/// соседний кабинет с той же датой — это другой факт.
#[tokio::test]
async fn waiting_instance_finds_only_its_own_fact() {
    let db = database().await;
    let before = events::last_seq(db).await.unwrap();

    events::publish(
        db,
        DomainEventKind::ImportDayCompleted,
        day("conn-c", "2026-08-21"),
        json!({}),
        "тест",
    )
    .await
    .unwrap();
    events::publish(
        db,
        DomainEventKind::ImportDayCompleted,
        day("conn-d", "2026-08-21"),
        json!({}),
        "тест",
    )
    .await
    .unwrap();

    let mine = events::find_matching(
        db,
        DomainEventKind::ImportDayCompleted,
        "connection_id=conn-c;business_date=2026-08-21",
        before,
        10,
    )
    .await
    .unwrap();
    assert_eq!(mine.len(), 1);
    assert_eq!(mine[0].correlation.get("connection_id"), Some("conn-c"));

    // Тот же ключ, но другой вид события — не то же самое ожидание.
    let other_kind = events::find_matching(
        db,
        DomainEventKind::DocumentPosted,
        "connection_id=conn-c;business_date=2026-08-21",
        before,
        10,
    )
    .await
    .unwrap();
    assert!(other_kind.is_empty());
}

/// Курсор потребителя двигается по номеру публикации, а не по времени: время в
/// SQLite не монотонно между процессами, и на одной секунде события
/// перепутались бы местами.
#[tokio::test]
async fn cursor_advances_by_sequence() {
    let db = database().await;
    let start = events::last_seq(db).await.unwrap();

    let mut published = Vec::new();
    for index in 0..3 {
        published.push(
            events::publish(
                db,
                DomainEventKind::HumanActionDone,
                CorrelationKey::new().with("request_key", format!("курсор-{index}")),
                json!({ "index": index }),
                "ui",
            )
            .await
            .unwrap(),
        );
    }

    let batch = events::list_since(db, start, 100)
        .await
        .unwrap()
        .into_iter()
        .filter(|event| event.source == "ui")
        .collect::<Vec<_>>();
    let ours: Vec<_> = batch
        .iter()
        .filter(|event| {
            event
                .correlation
                .get("request_key")
                .is_some_and(|key| key.starts_with("курсор-"))
        })
        .collect();
    assert_eq!(ours.len(), 3);
    assert!(
        ours.windows(2).all(|pair| pair[0].seq < pair[1].seq),
        "события пришли не в порядке публикации"
    );

    // Разобрав первое, потребитель не получает его снова.
    let rest = events::list_since(db, ours[0].seq, 100)
        .await
        .unwrap()
        .into_iter()
        .filter(|event| event.id == ours[0].id)
        .count();
    assert_eq!(rest, 0, "курсор не сдвинулся");
}

/// Каталог закрыт: у каждого события есть ключ, и он разбирается обратно.
#[tokio::test]
async fn every_catalog_entry_can_be_published() {
    let db = database().await;
    for kind in DomainEventKind::ALL {
        let mut key = CorrelationKey::new();
        for field in kind.correlation_fields() {
            key = key.with(*field, format!("значение-{field}"));
        }
        let event = events::publish(db, kind, key, json!({}), "каталог")
            .await
            .unwrap_or_else(|error| panic!("событие '{}' не публикуется: {error}", kind.as_str()));
        assert_eq!(event.kind, kind);
        assert_eq!(
            DomainEventKind::parse(kind.as_str()),
            Some(kind),
            "имя события не разбирается обратно"
        );
    }
}
