//! Универсальный резолвер представлений ссылок (`*_ref`).
//!
//! Принимает имя реквизита (`kind`) и UUID (`id`) и возвращает человекочитаемое
//! представление объекта. Используется на детальных страницах, чтобы рядом с
//! UUID показывать наименование связанного объекта (например, имя подключения МП
//! по `connection_mp_ref`).
//!
//! Какой реквизит какому срезу принадлежит, знает реестр
//! (`shared::representation`), а не этот хендлер: здесь остался только разбор
//! запроса и форма ответа.

use axum::{extract::Query, Json};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub struct ResolveRefQuery {
    /// Имя реквизита, например `connection_mp_ref`, `organization_ref`, ...
    pub kind: String,
    /// UUID связанного объекта.
    pub id: String,
}

#[derive(Debug, Serialize)]
pub struct ResolveRefResponse {
    pub kind: String,
    pub id: String,
    /// Человекочитаемое представление; `None`, если объект не найден или
    /// `kind` не поддерживается.
    pub representation: Option<String>,
}

/// GET /api/refs/resolve?kind=connection_mp_ref&id=<uuid>
pub async fn resolve(Query(req): Query<ResolveRefQuery>) -> Json<ResolveRefResponse> {
    let representation = crate::shared::representation::resolve_reference(&req.kind, &req.id).await;
    Json(ResolveRefResponse {
        kind: req.kind,
        id: req.id,
        representation,
    })
}
