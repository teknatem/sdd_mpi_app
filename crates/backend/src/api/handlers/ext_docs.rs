//! Документация внешнего API для потребителей (1С, Power BI): спека OpenAPI и
//! страница-читалка поверх неё.
//!
//! Спека — рукописный `ext_openapi.json` рядом с этим файлом, вшитый в бинарь.
//! Расхождение со списком роутов ловит тест `openapi_spec_covers_every_ext_route`
//! в `api/routes.rs`: новый эндпоинт без описания (или наоборот) валит сборку.
//!
//! Оба маршрута открыты **без** `X-Api-Key`: потребитель должен прочитать
//! документацию до того, как получит ключ, а данных она не раскрывает. Вызовы
//! всё равно проходят через рекордер и видны в журнале внешнего API.

use axum::response::Html;

/// Спека, вшитая в бинарь — отдельного файла рядом с exe не требуется.
const OPENAPI_SPEC: &str = include_str!("ext_openapi.json");

/// Читалка — Scalar с CDN. Страницу открывает внешний потребитель в браузере,
/// на WASM-бандл приложения она не влияет. Ссылка на сырую спеку в шапке —
/// на случай, когда CDN недоступен (закрытый контур).
const DOCS_PAGE: &str = r#"<!doctype html>
<html lang="ru">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>Внешний API маркетплейсов</title>
  <style>
    body { margin: 0; font-family: system-ui, sans-serif; }
    .fallback { padding: 24px; }
  </style>
</head>
<body>
  <div id="app" class="fallback">
    Если документация не отобразилась, читалка не загрузилась с CDN —
    спека доступна напрямую: <a href="/api/ext/v1/openapi.json">openapi.json</a>
  </div>
  <script src="https://cdn.jsdelivr.net/npm/@scalar/api-reference@1"></script>
  <script>
    if (window.Scalar && window.Scalar.createApiReference) {
      Scalar.createApiReference('#app', { url: '/api/ext/v1/openapi.json' });
    }
  </script>
</body>
</html>
"#;

/// GET /api/ext/v1/openapi.json — спека OpenAPI 3.1 внешнего API.
pub async fn openapi_spec() -> ([(axum::http::HeaderName, &'static str); 1], &'static str) {
    (
        [(
            axum::http::header::CONTENT_TYPE,
            "application/json; charset=utf-8",
        )],
        OPENAPI_SPEC,
    )
}

/// GET /api/ext/v1/docs — страница документации для внешних потребителей.
pub async fn docs_page() -> Html<&'static str> {
    Html(DOCS_PAGE)
}
