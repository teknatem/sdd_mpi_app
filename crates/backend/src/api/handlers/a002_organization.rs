//! Эталонный срез Фазы 3: база берётся из [`AppState`], ошибки — [`ApiError`].
//!
//! Ни один хендлер здесь не зовёт `db::get_connection()`; соединение приходит
//! экстрактором и передаётся вниз параметром. Именно этот срез покрыт
//! интеграционным тестом `crates/backend/tests/a002_organization_api.rs`.

use axum::{
    extract::{Path, State},
    Json,
};
use serde_json::json;

use crate::domain::a002_organization;
use crate::shared::error::{ApiError, ApiResult};

/// Хендлерам этого среза нужна только база — `FromRef` избавляет их от знания
/// про остальные поля состояния.
type Db = State<sea_orm::DatabaseConnection>;

fn parse_id(id: &str) -> Result<uuid::Uuid, ApiError> {
    uuid::Uuid::parse_str(id).map_err(|_| ApiError::bad_request(format!("некорректный id: {id}")))
}

/// GET /api/organization
pub async fn list_all(
    State(db): Db,
) -> ApiResult<Json<Vec<contracts::domain::a002_organization::aggregate::Organization>>> {
    Ok(Json(a002_organization::service::list_all(&db).await?))
}

/// GET /api/organization/:id
pub async fn get_by_id(
    State(db): Db,
    Path(id): Path<String>,
) -> ApiResult<Json<contracts::domain::a002_organization::aggregate::Organization>> {
    let uuid = parse_id(&id)?;
    a002_organization::service::get_by_id(&db, uuid)
        .await?
        .map(Json)
        .ok_or_else(|| ApiError::not_found(format!("организация {id} не найдена")))
}

/// POST /api/organization
pub async fn upsert(
    State(db): Db,
    Json(dto): Json<contracts::domain::a002_organization::aggregate::OrganizationDto>,
) -> ApiResult<Json<serde_json::Value>> {
    let id = if dto.id.is_some() {
        a002_organization::service::update(&db, dto).await?;
        uuid::Uuid::nil().to_string()
    } else {
        a002_organization::service::create(&db, dto)
            .await?
            .to_string()
    };

    Ok(Json(json!({ "id": id })))
}

/// DELETE /api/organization/:id
pub async fn delete(State(db): Db, Path(id): Path<String>) -> ApiResult<()> {
    let uuid = parse_id(&id)?;
    match a002_organization::service::delete(&db, uuid).await? {
        true => Ok(()),
        false => Err(ApiError::not_found(format!("организация {id} не найдена"))),
    }
}

/// POST /api/organization/testdata
pub async fn insert_test_data(State(db): Db) -> ApiResult<()> {
    a002_organization::service::insert_test_data(&db).await?;
    Ok(())
}
