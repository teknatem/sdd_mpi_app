//! Типизированная ошибка HTTP-слоя.
//!
//! **Что было.** 377 сигнатур вида `Result<Json<T>, StatusCode>`. Голый статус —
//! это ошибка, у которой нет ни причины, ни машиночитаемого кода, ни тела: на
//! фронт приходил пустой 500, и различать «нет такой записи» от «база
//! недоступна» приходилось по подстроке в тексте (`"HTTP error: 404"`).
//!
//! **Что теперь.** `ApiError` — перечисление с явными видами отказа. Тело
//! ответа — RFC 9457 (`application/problem+json`):
//!
//! ```json
//! { "type": "about:blank", "title": "Not Found", "status": 404,
//!   "detail": "организация 5f1c… не найдена", "code": "not_found" }
//! ```
//!
//! `type` намеренно `about:blank`: RFC разрешает это значение, когда у типа
//! проблемы нет отдельной документируемой страницы, и требует тогда, чтобы
//! `title` совпадал с фразой статуса. Машиночитаемую специфику несёт `code` —
//! заводить URI, который отдаёт 404, было бы хуже, чем не заводить вовсе.
//!
//! **Что наружу не уходит.** Текст `DbErr` пишется в лог и заменяется общей
//! фразой: сообщения SQLite содержат имена таблиц и куски запроса. Сообщения
//! варианта `Internal` написаны нами и отдаются как есть — иначе 500 снова
//! становится неотлаживаемым.
//!
//! **Мост миграции.** `From<StatusCode>` существует ровно затем, чтобы смена
//! типа ошибки в сигнатуре не требовала переписывать тело хендлера: привычные
//! `.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?` и `.ok_or(StatusCode::NOT_FOUND)?`
//! продолжают работать. В новом коде пользуйтесь конструкторами
//! (`ApiError::not_found("…")`) — они несут причину, а статус её не несёт.

use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};

/// Результат хендлера. Короткая запись для `Result<T, ApiError>`.
pub type ApiResult<T> = Result<T, ApiError>;

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    /// Запрос не проходит валидацию — неразбираемый id, отсутствующий параметр.
    #[error("{0}")]
    BadRequest(String),

    /// Нет действительной сессии.
    #[error("{0}")]
    Unauthorized(String),

    /// Сессия есть, прав недостаточно.
    #[error("{0}")]
    Forbidden(String),

    /// Запрошенного объекта не существует.
    #[error("{0}")]
    NotFound(String),

    /// Состояние объекта не позволяет операцию (повторное проведение, дубль).
    #[error("{0}")]
    Conflict(String),

    /// Синтаксис верен, но данные противоречивы.
    #[error("{0}")]
    UnprocessableEntity(String),

    /// Отказ на нашей стороне; текст авторский и отдаётся клиенту.
    #[error("{0}")]
    Internal(String),

    /// Отказ базы. Текст логируется, наружу не уходит.
    #[error(transparent)]
    Database(#[from] sea_orm::DbErr),

    /// Мост миграции: статус без причины. Новый код так писать не должен.
    #[error("HTTP {0}")]
    Status(StatusCode),
}

impl ApiError {
    pub fn bad_request(detail: impl std::fmt::Display) -> Self {
        Self::BadRequest(detail.to_string())
    }

    pub fn unauthorized(detail: impl std::fmt::Display) -> Self {
        Self::Unauthorized(detail.to_string())
    }

    pub fn forbidden(detail: impl std::fmt::Display) -> Self {
        Self::Forbidden(detail.to_string())
    }

    pub fn not_found(detail: impl std::fmt::Display) -> Self {
        Self::NotFound(detail.to_string())
    }

    pub fn conflict(detail: impl std::fmt::Display) -> Self {
        Self::Conflict(detail.to_string())
    }

    pub fn unprocessable(detail: impl std::fmt::Display) -> Self {
        Self::UnprocessableEntity(detail.to_string())
    }

    pub fn internal(detail: impl std::fmt::Display) -> Self {
        Self::Internal(detail.to_string())
    }

    /// Статус ответа.
    pub fn status(&self) -> StatusCode {
        match self {
            Self::BadRequest(_) => StatusCode::BAD_REQUEST,
            Self::Unauthorized(_) => StatusCode::UNAUTHORIZED,
            Self::Forbidden(_) => StatusCode::FORBIDDEN,
            Self::NotFound(_) => StatusCode::NOT_FOUND,
            Self::Conflict(_) => StatusCode::CONFLICT,
            Self::UnprocessableEntity(_) => StatusCode::UNPROCESSABLE_ENTITY,
            Self::Internal(_) | Self::Database(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::Status(status) => *status,
        }
    }

    /// Машиночитаемый код вида проблемы — то, по чему клиенту разрешено
    /// ветвиться (в отличие от `detail`, который меняется свободно).
    pub fn code(&self) -> &'static str {
        match self {
            Self::BadRequest(_) => "bad_request",
            Self::Unauthorized(_) => "unauthorized",
            Self::Forbidden(_) => "forbidden",
            Self::NotFound(_) => "not_found",
            Self::Conflict(_) => "conflict",
            Self::UnprocessableEntity(_) => "unprocessable_entity",
            Self::Internal(_) => "internal_error",
            Self::Database(_) => "database_error",
            Self::Status(_) => "unspecified",
        }
    }

    /// Человекочитаемая причина для клиента. `None` — когда причина есть, но
    /// показывать её нельзя (ошибки базы) или её просто нет (мост-статус).
    fn client_detail(&self) -> Option<String> {
        match self {
            Self::BadRequest(detail)
            | Self::Unauthorized(detail)
            | Self::Forbidden(detail)
            | Self::NotFound(detail)
            | Self::Conflict(detail)
            | Self::UnprocessableEntity(detail)
            | Self::Internal(detail) => Some(detail.clone()),
            Self::Database(_) | Self::Status(_) => None,
        }
    }
}

impl From<StatusCode> for ApiError {
    fn from(status: StatusCode) -> Self {
        // Известные статусы поднимаются до именованных вариантов: после
        // миграции хендлера остаётся дописать причину, а не менять вариант.
        match status {
            StatusCode::BAD_REQUEST => Self::BadRequest(String::new()),
            StatusCode::UNAUTHORIZED => Self::Unauthorized(String::new()),
            StatusCode::FORBIDDEN => Self::Forbidden(String::new()),
            StatusCode::NOT_FOUND => Self::NotFound(String::new()),
            StatusCode::CONFLICT => Self::Conflict(String::new()),
            StatusCode::UNPROCESSABLE_ENTITY => Self::UnprocessableEntity(String::new()),
            StatusCode::INTERNAL_SERVER_ERROR => Self::Internal(String::new()),
            other => Self::Status(other),
        }
    }
}

impl From<anyhow::Error> for ApiError {
    fn from(error: anyhow::Error) -> Self {
        Self::Internal(error.to_string())
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = self.status();

        // Всё, что клиенту не покажут, обязано остаться в логе — иначе 500
        // превращается в «что-то сломалось» без следа.
        if status.is_server_error() {
            tracing::error!(code = self.code(), "api error: {self}");
        }

        let detail = self.client_detail().filter(|text| !text.is_empty());
        let body = serde_json::json!({
            "type": "about:blank",
            "title": status.canonical_reason().unwrap_or("Error"),
            "status": status.as_u16(),
            "detail": detail,
            "code": self.code(),
        });

        (
            status,
            [(header::CONTENT_TYPE, "application/problem+json")],
            serde_json::to_string(&body).unwrap_or_else(|_| "{}".to_string()),
        )
            .into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::to_bytes;

    async fn body_of(error: ApiError) -> (StatusCode, serde_json::Value) {
        let response = error.into_response();
        let status = response.status();
        let bytes = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("тело ответа не читается");
        (
            status,
            serde_json::from_slice(&bytes).expect("тело ответа — не JSON"),
        )
    }

    #[tokio::test]
    async fn problem_body_follows_rfc_9457() {
        let (status, body) = body_of(ApiError::not_found("организация не найдена")).await;

        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(body["status"], 404);
        assert_eq!(body["title"], "Not Found");
        assert_eq!(body["code"], "not_found");
        assert_eq!(body["detail"], "организация не найдена");
    }

    /// Текст ошибки базы содержит имена таблиц и фрагменты запроса — наружу он
    /// не уходит ни при каких обстоятельствах.
    #[tokio::test]
    async fn database_detail_is_not_leaked() {
        let error = ApiError::Database(sea_orm::DbErr::Custom(
            "no such column: a002_organization.secret".to_string(),
        ));
        let (status, body) = body_of(error).await;

        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(body["code"], "database_error");
        assert!(body["detail"].is_null());
    }

    /// Мост миграции: старый `Err(StatusCode::…)` обязан давать тот же статус,
    /// иначе перевод сигнатур молча меняет поведение API.
    #[test]
    fn status_bridge_preserves_the_code() {
        for status in [
            StatusCode::BAD_REQUEST,
            StatusCode::UNAUTHORIZED,
            StatusCode::FORBIDDEN,
            StatusCode::NOT_FOUND,
            StatusCode::CONFLICT,
            StatusCode::UNPROCESSABLE_ENTITY,
            StatusCode::INTERNAL_SERVER_ERROR,
            StatusCode::SERVICE_UNAVAILABLE,
            StatusCode::TOO_MANY_REQUESTS,
        ] {
            assert_eq!(ApiError::from(status).status(), status);
        }
    }
}
