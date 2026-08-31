//! Представление ссылки на подключение к маркетплейсу по имени реквизита.

use async_trait::async_trait;

use crate::shared::representation::{pick, ReferenceResolver};

/// Резолвер реквизита `connection_mp_ref`.
pub struct RefResolver;

#[async_trait]
impl ReferenceResolver for RefResolver {
    fn ref_kind(&self) -> &'static str {
        "connection_mp_ref"
    }

    async fn represent(&self, id: uuid::Uuid) -> Option<String> {
        let item = super::service::get_by_id(id).await.ok()??;
        pick(&item.base.description, &item.base.code)
    }
}
