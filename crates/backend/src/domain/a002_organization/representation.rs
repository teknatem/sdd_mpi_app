//! Представление ссылки на организация по имени реквизита.

use async_trait::async_trait;

use crate::shared::representation::{pick, ReferenceResolver};

/// Резолвер реквизита `organization_ref`.
pub struct RefResolver;

#[async_trait]
impl ReferenceResolver for RefResolver {
    fn ref_kind(&self) -> &'static str {
        "organization_ref"
    }

    async fn represent(&self, id: uuid::Uuid) -> Option<String> {
        let item = crate::domain::a002_organization::service::get_by_id(
            crate::shared::data::db::get_connection(),
            id,
        )
        .await
        .ok()??;
        pick(&item.base.description, &item.base.code)
    }
}
