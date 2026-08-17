//! Универсальный резолвер представлений ссылок (`*_ref`).
//!
//! Принимает имя реквизита (`kind`) и UUID (`id`) и возвращает человекочитаемое
//! представление объекта. Используется на детальных страницах, чтобы рядом с
//! UUID показывать наименование связанного объекта (например, имя подключения МП
//! по `connection_mp_ref`).

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
    let representation = resolve_representation(&req.kind, &req.id).await;
    Json(ResolveRefResponse {
        kind: req.kind,
        id: req.id,
        representation,
    })
}

/// Возвращает первое непустое значение, обрезая пробелы.
fn pick(primary: &str, fallback: &str) -> Option<String> {
    let primary = primary.trim();
    if !primary.is_empty() {
        return Some(primary.to_string());
    }
    let fallback = fallback.trim();
    if !fallback.is_empty() {
        return Some(fallback.to_string());
    }
    None
}

async fn resolve_representation(kind: &str, id: &str) -> Option<String> {
    let uuid = uuid::Uuid::parse_str(id).ok()?;

    match kind {
        "connection_mp_ref" => {
            let item = crate::domain::a006_connection_mp::service::get_by_id(uuid)
                .await
                .ok()??;
            pick(&item.base.description, &item.base.code)
        }
        "organization_ref" => {
            let item = crate::domain::a002_organization::service::get_by_id(
                crate::shared::data::db::get_connection(),
                uuid,
            )
            .await
            .ok()??;
            pick(&item.base.description, &item.base.code)
        }
        "nomenclature_ref" => {
            let item = crate::domain::a004_nomenclature::service::get_by_id(uuid)
                .await
                .ok()??;
            pick(&item.base.description, &item.base.code)
        }
        "marketplace_product_ref" => {
            let item = crate::domain::a007_marketplace_product::service::get_by_id(uuid)
                .await
                .ok()??;
            pick(&item.base.description, &item.base.code)
        }
        "marketplace_order_ref" => {
            let item = crate::domain::a013_ym_order::service::get_by_id(uuid)
                .await
                .ok()??;
            pick(&item.base.description, &item.header.document_no)
        }
        // Прочие виды (типы регистраторов: aXXX-документы, p903/p907 и т.п.)
        // делегируются в общий сервис представлений агрегатов.
        other => crate::shared::representation::resolve(other, id)
            .await
            .map(|rep| crate::shared::representation::to_label(&rep)),
    }
}
