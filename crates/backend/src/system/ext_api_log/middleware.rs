use axum::body::{Body, HttpBody};
use axum::extract::{ConnectInfo, MatchedPath};
use axum::http::Request;
use axum::middleware::Next;
use axum::response::Response;
use std::net::SocketAddr;
use uuid::Uuid;

use super::{repository, service};

/// Записывает каждый входящий вызов внешнего API в `sys_ext_api_log`.
///
/// Вешается **на ext-саброутер** (`api/routes.rs`), а не глобально: так «только внешние
/// вызовы» получается по построению, без сопоставления префикса пути. Стоит **снаружи**
/// `check_api_key`, поэтому в лог попадают и 401 (неверный/отсутствующий ключ), и 503
/// (ключ не настроен) — именно они и есть контроль корректности интеграции.
pub async fn record_ext_api_call(req: Request<Body>, next: Next) -> Response {
    let start = std::time::Instant::now();

    // Всё нужное вынимаем до next.run — дальше req уходит по значению.
    let method = req.method().to_string();
    let path = req.uri().path().to_string();
    let query = req.uri().query().map(str::to_string);
    // Роутинг уже произошёл (слой висит на саброутере), поэтому паттерн доступен;
    // fallback на сырой путь оставлен на случай нештатного вызова слоя.
    let route = req
        .extensions()
        .get::<MatchedPath>()
        .map(|m| m.as_str().to_string())
        .unwrap_or_else(|| path.clone());
    let user_agent = req
        .headers()
        .get(axum::http::header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    let client_ip = extract_client_ip(&req);

    let response = next.run(req).await;

    let status = response.status().as_u16() as i32;
    // Размер берём из size_hint, а не буферизацией тела: ext-хендлеры отдают Json →
    // Full<Bytes>, точный размер известен. Глобальный request_logger уже буферизует
    // тело целиком снаружи — второй буфер здесь удвоил бы стоимость.
    let bytes_out = response.body().size_hint().exact().unwrap_or(0) as i64;
    let duration_ms = start.elapsed().as_millis() as i64;

    let row = repository::Model {
        id: Uuid::new_v4().to_string(),
        ts: service::now_ts(),
        method,
        route,
        path,
        query,
        status,
        duration_ms,
        bytes_out,
        client_ip,
        user_agent,
        // Ключ внешнего API сейчас один на всех потребителей, опознать вызывающего
        // нечем. Заполнится, когда появится многоключевость.
        client_id: None,
    };

    // Fire-and-forget: запись в SQLite не должна попадать в горячий путь ответа.
    // Внешних вызовов мало, поэтому канал/батчинг избыточны.
    tokio::spawn(async move {
        let Some(_database_activity) = crate::system::maintenance::try_begin_database_activity()
        else {
            return;
        };
        if let Err(e) = service::record(row).await {
            tracing::warn!("[ext-api-log] failed to record call: {e}");
        }
    });

    response
}

/// Реальный IP вызывающего: за прокси — первый элемент X-Forwarded-For,
/// иначе peer-адрес соединения (требует `into_make_service_with_connect_info`).
fn extract_client_ip(req: &Request<Body>) -> Option<String> {
    let forwarded = req
        .headers()
        .get("X-Forwarded-For")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(',').next())
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty());
    if forwarded.is_some() {
        return forwarded;
    }
    req.extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|ConnectInfo(addr)| addr.ip().to_string())
}
