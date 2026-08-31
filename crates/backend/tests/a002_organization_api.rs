//! Первый интеграционный тест проекта.
//!
//! До Фазы 3 такого файла не могло существовать по двум причинам сразу: крейт
//! был чисто бинарным (линковаться было не с чем), а соединение с базой было
//! глобальным синглтоном, инициализируемым только из `main`. Тест проверяет,
//! что оба препятствия сняты по-настоящему: поднимает **пустую базу** с
//! прогнанными миграциями, собирает **боевой роутер** и гоняет запросы через
//! `tower::ServiceExt::oneshot` — без сокета, без боевой БД, без конфига.
//!
//! Срез a002 выбран потому, что он полностью переведён на `AppState`: хендлеры
//! получают базу экстрактором, а не из моста.
//!
//! **Один файл — одна база.** `init_test_database` заполняет тот же `OnceCell`,
//! поэтому все тесты бинаря делят одну базу; порядок между ними не определён, и
//! общих строк они касаться не должны. Здесь это выдержано: тесты ниже пишут
//! свои организации и не полагаются на пустоту таблицы.

use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use backend::shared::app_state::AppState;
use backend::shared::data::db;
use serde_json::{json, Value};
use tower::ServiceExt;

/// Роутер приложения поверх тестовой базы. Собирается ровно так же, как в
/// `main.rs` — иначе тест проверял бы конструкцию, которой в бою нет.
/// Слой `check_scope` при этом остаётся на месте: запросы ниже ходят с
/// настоящим JWT, а не в обход авторизации.
async fn app() -> axum::Router {
    let db = db::init_test_database()
        .await
        .expect("тестовая база не поднялась");

    axum::Router::new()
        .merge(backend::api::configure_business_routes())
        .with_state(AppState::new(db))
}

/// Токен администратора, подписанный тем же секретом, что и в бою: секрет
/// читается из `sys_settings`, куда его кладёт подъём тестовой базы. Поэтому
/// первым делом — база: выпуск токена ходит в неё через глобальный мост.
async fn admin_token() -> String {
    backend::composition::install_all();
    db::init_test_database().await.expect("нет тестовой базы");

    backend::system::auth::jwt::generate_access_token(
        "00000000-0000-0000-0000-0000000000ad",
        "integration-admin",
        true,
        "admin",
    )
    .await
    .expect("не удалось выпустить токен")
}

async fn json_body(response: axum::response::Response) -> Value {
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    serde_json::from_slice(&bytes).unwrap_or(Value::Null)
}

fn get(token: &str, path: &str) -> Request<Body> {
    Request::builder()
        .uri(path)
        .header("Authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .expect("некорректный запрос")
}

fn post_json(token: &str, path: &str, payload: Value) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(path)
        .header("Authorization", format!("Bearer {token}"))
        .header("content-type", "application/json")
        .body(Body::from(payload.to_string()))
        .expect("некорректный запрос")
}

/// Полный путь записи и чтения через HTTP: POST создаёт организацию, GET
/// списка её видит, GET по id отдаёт ровно её.
#[tokio::test]
async fn organization_survives_a_round_trip_through_http() {
    let app = app().await;
    let token = admin_token().await;

    let created = app
        .clone()
        .oneshot(post_json(
            &token,
            "/api/organization",
            json!({
                "code": "ORG-IT-001",
                "description": "Интеграционный тест",
                "fullName": "ООО «Интеграционный тест»",
                "inn": "7700000001",
                "kpp": "770001001",
                "comment": null,
            }),
        ))
        .await
        .unwrap();
    assert_eq!(created.status(), StatusCode::OK);

    let id = json_body(created).await["id"].as_str().unwrap().to_string();

    let fetched = app
        .clone()
        .oneshot(get(&token, &format!("/api/organization/{id}")))
        .await
        .unwrap();
    assert_eq!(fetched.status(), StatusCode::OK);
    let body = json_body(fetched).await;
    assert_eq!(body["code"], "ORG-IT-001");

    let listed = app.oneshot(get(&token, "/api/organization")).await.unwrap();
    assert_eq!(listed.status(), StatusCode::OK);
    let items = json_body(listed).await;
    assert!(
        items
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["code"] == "ORG-IT-001"),
        "созданная организация не попала в список"
    );
}

/// Ошибка отдаётся телом RFC 9457, а не пустым статусом: клиенту нужен `code`,
/// по которому можно ветвиться, и `detail`, который можно показать.
#[tokio::test]
async fn missing_organization_answers_with_a_problem_document() {
    let token = admin_token().await;
    let response = app()
        .await
        .oneshot(get(
            &token,
            "/api/organization/00000000-0000-0000-0000-000000000000",
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert_eq!(
        response
            .headers()
            .get("content-type")
            .and_then(|value| value.to_str().ok()),
        Some("application/problem+json")
    );

    let body = json_body(response).await;
    assert_eq!(body["status"], 404);
    assert_eq!(body["code"], "not_found");
    assert!(body["detail"].as_str().is_some_and(|d| !d.is_empty()));
}

/// Неразбираемый id — это 400 с объяснением, а не 500 и не 404.
#[tokio::test]
async fn malformed_id_is_a_bad_request() {
    let token = admin_token().await;
    let response = app()
        .await
        .oneshot(get(&token, "/api/organization/не-uuid"))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(json_body(response).await["code"], "bad_request");
}

/// Без токена срез закрыт. Проверка стоит одного запроса и ловит худший из
/// возможных регрессов — снятый слой авторизации.
#[tokio::test]
async fn anonymous_request_is_rejected() {
    let request = Request::builder()
        .uri("/api/organization")
        .body(Body::empty())
        .unwrap();

    let response = app().await.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

/// Миграции обязаны проходить на пустой базе. Проверка стоит дёшево (база уже
/// поднята) и ловит миграцию, работоспособную только поверх боевой схемы.
#[tokio::test]
async fn migrations_apply_to_an_empty_database() {
    use sea_orm::{ConnectionTrait, DatabaseBackend, Statement};

    let db = db::init_test_database().await.unwrap();
    let applied = db
        .query_one(Statement::from_string(
            DatabaseBackend::Sqlite,
            "SELECT COUNT(*) AS n FROM _sqlx_migrations WHERE success = 1",
        ))
        .await
        .unwrap()
        .expect("таблица миграций пуста");

    let count: i32 = applied.try_get("", "n").unwrap();
    assert!(count > 200, "применено миграций: {count}");
}
