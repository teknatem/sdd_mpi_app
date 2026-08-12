//! Глобальный гейт режима технического обслуживания.
//!
//! Пропускает только то, без чего режим не показать и не выключить:
//! статику фронтенда, статус самого режима, вход (решение принимает обработчик
//! логина) и запросы администратора. Всё остальное — 503 с телом, по которому
//! фронтенд рисует страницу-заглушку.
//!
//! Внешний API (`X-Api-Key`, 1С и Power BI) блокируется наравне с людьми: если
//! готовится подмена базы, принять от 1С документ означает записать его в файл,
//! который через минуту уедет в архив. Честный 503 с `Retry-After` даёт
//! интеграции повторить попытку, приём «в никуда» — нет.

use axum::body::Body;
use axum::extract::Request;
use axum::middleware::Next;
use axum::response::Response;

use crate::system::auth::jwt;
use crate::system::maintenance;

/// Пути, доступные при включённом режиме без проверки прав.
const ALWAYS_ALLOWED: &[&str] = &[
    // Иначе страницу-заглушку нечем наполнить.
    "/api/system/maintenance",
    // Вход нужен администратору; обычного пользователя отсекает сам обработчик.
    "/api/system/auth/login",
    "/api/system/auth/refresh",
    "/api/system/auth/logout",
    // Без этого фронтенд не узнает, что вошедший — админ, и покажет заглушку ему же.
    "/api/system/auth/me",
    "/health",
];

pub async fn maintenance_gate(req: Request<Body>, next: Next) -> Response {
    if !maintenance::is_active() {
        return next.run(req).await;
    }

    if is_always_allowed(req.uri().path()) {
        return next.run(req).await;
    }

    // Токен забираем ДО await и владеющей строкой: `axum::body::Body` не `Sync`,
    // поэтому ссылка на запрос, дожившая до точки ожидания, сделала бы future
    // не-`Send`, и middleware перестал бы подходить под `Service`.
    let token = bearer_token(&req);
    if is_admin(token).await {
        return next.run(req).await;
    }

    maintenance::unavailable_response()
}

/// Пропускать ли путь без проверки прав. Вынесено отдельно от `maintenance_gate`,
/// чтобы правило проверялось тестом, а не только глазами.
fn is_always_allowed(path: &str) -> bool {
    // Статика SPA: сама страница-заглушка приезжает этими же файлами.
    !path.starts_with("/api/") || ALWAYS_ALLOWED.contains(&path)
}

fn bearer_token(req: &Request<Body>) -> Option<String> {
    req.headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .map(ToString::to_string)
}

/// Админ проходит всегда — иначе первая же ошибка запирает администратора
/// снаружи вместе со всеми, и снять режим станет некому.
async fn is_admin(token: Option<String>) -> bool {
    let Some(token) = token else {
        return false;
    };
    jwt::validate_token(&token)
        .await
        .map(|claims| claims.is_admin)
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::StatusCode;
    use axum::routing::get;
    use axum::Router;
    use contracts::system::maintenance::MaintenanceTrigger;
    use tower::ServiceExt;

    /// Состояние режима общее на процесс — HTTP-тесты идут по одному.
    static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn app() -> Router {
        Router::new()
            .route("/api/business", get(|| async { "ok" }))
            .route("/api/system/maintenance", get(|| async { "status" }))
            .route("/index.html", get(|| async { "spa" }))
            .layer(axum::middleware::from_fn(maintenance_gate))
    }

    async fn status_of(path: &str) -> StatusCode {
        let request = Request::builder()
            .uri(path)
            .body(Body::empty())
            .expect("request");
        app().oneshot(request).await.expect("response").status()
    }

    #[tokio::test]
    async fn everything_passes_while_the_mode_is_off() {
        let _guard = SERIAL.lock().unwrap_or_else(|error| error.into_inner());
        maintenance::disable();
        assert_eq!(status_of("/api/business").await, StatusCode::OK);
    }

    #[tokio::test]
    async fn anonymous_business_request_is_rejected_with_503() {
        let _guard = SERIAL.lock().unwrap_or_else(|error| error.into_inner());
        maintenance::enable(
            "тест".to_string(),
            MaintenanceTrigger::Manual,
            "test".to_string(),
        );

        // Прикладной роут закрыт...
        assert_eq!(
            status_of("/api/business").await,
            StatusCode::SERVICE_UNAVAILABLE
        );
        // ...а статус и статика — нет, иначе заглушку нечем показать.
        assert_eq!(status_of("/api/system/maintenance").await, StatusCode::OK);
        assert_eq!(status_of("/index.html").await, StatusCode::OK);

        maintenance::disable();
    }

    #[test]
    fn static_assets_and_login_stay_reachable() {
        // Без статики нечем показать саму заглушку.
        assert!(is_always_allowed("/"));
        assert!(is_always_allowed("/index.html"));
        assert!(is_always_allowed("/static/pages/maintenance.css"));
        // Без входа администратор не сможет попасть внутрь и снять режим.
        assert!(is_always_allowed("/api/system/auth/login"));
        assert!(is_always_allowed("/api/system/auth/me"));
        // Без статуса фронтенд не узнает, что показывать.
        assert!(is_always_allowed("/api/system/maintenance"));
    }

    #[test]
    fn business_and_external_api_are_closed() {
        assert!(!is_always_allowed("/api/a015-wb-orders"));
        assert!(!is_always_allowed("/api/sys/datasets/status"));
        // Внешний API закрывается вместе со всеми: принять документ от 1С в
        // базу, которую вот-вот заменят, — значит потерять его молча.
        assert!(!is_always_allowed("/api/ext/v1/documents"));
        // Управление режимом пропускается не по пути, а по правам админа.
        assert!(!is_always_allowed("/api/system/maintenance/disable"));
    }
}
