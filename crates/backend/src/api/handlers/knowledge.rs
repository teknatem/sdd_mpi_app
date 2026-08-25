//! HTTP-доступ к инвентаризации знаний.
//!
//! База приходит экстрактором `State`, ошибки — `ApiError`: срез новый, и
//! глобальный мост к соединению в него не заводится.
//!
//! Ответ `/api/knowledge/inventory` отдаётся целиком, вместе со всеми
//! единицами. Их порядка четырёхсот, страница фильтрует и листает у себя, и
//! серверная пагинация здесь только добавила бы кругов: фасеты всё равно
//! считаются по всему снимку, а не по странице.

use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::Deserialize;

use crate::knowledge::{repository, service};
use crate::shared::error::{ApiError, ApiResult};

type Db = State<sea_orm::DatabaseConnection>;

/// GET /api/knowledge/inventory — последний снимок целиком.
pub async fn inventory(
    State(db): Db,
) -> ApiResult<Json<contracts::knowledge::InventoryResponseDto>> {
    Ok(Json(service::inventory(&db).await?))
}

/// GET /api/knowledge/inventory/surfaces — реестр поверхностей со счётчиками.
///
/// Отдельный роут, потому что реестр — это девятнадцать строк, а полный ответ —
/// четыреста: инструменту чата и виджету нужен первый, а не второй.
pub async fn surfaces(State(db): Db) -> ApiResult<Json<serde_json::Value>> {
    let full = service::inventory(&db).await?;
    Ok(Json(serde_json::json!({
        "snapshot": full.snapshot,
        "surfaces": full.surfaces,
    })))
}

/// GET /api/knowledge/inventory/unit/:id — единица и её история по снимкам.
pub async fn unit(State(db): Db, Path(id): Path<String>) -> ApiResult<Json<serde_json::Value>> {
    let Some((snapshot, _)) = service::summary_only(&db).await? else {
        return Err(ApiError::not_found("снимок инвентаризации ещё не снят"));
    };
    let units = repository::units_of(&db, &snapshot.id).await?;
    let found = units
        .into_iter()
        .find(|unit| unit.unit_id == id)
        .ok_or_else(|| {
            ApiError::not_found(format!("единица {id} не найдена в последнем снимке"))
        })?;
    Ok(Json(serde_json::json!({ "unit": found })))
}

#[derive(Debug, Deserialize)]
pub struct HistoryQuery {
    #[serde(default = "default_limit")]
    limit: u64,
}

fn default_limit() -> u64 {
    30
}

/// GET /api/knowledge/inventory/history — ряд снимков.
pub async fn history(
    State(db): Db,
    Query(query): Query<HistoryQuery>,
) -> ApiResult<Json<serde_json::Value>> {
    let items = repository::list(&db, query.limit.clamp(1, 200)).await?;
    Ok(Json(serde_json::json!({ "items": items })))
}

/// POST /api/knowledge/inventory/collect — пересобрать снимок.
pub async fn collect(
    State(db): Db,
) -> ApiResult<Json<contracts::knowledge::InventoryCollectReportDto>> {
    Ok(Json(service::collect_and_store(&db, "manual").await?))
}
