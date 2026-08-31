//! Представление ссылки на товар маркетплейса по имени реквизита.

use async_trait::async_trait;

use crate::shared::representation::{pick, ReferenceResolver};

/// Резолвер реквизита `marketplace_product_ref`.
pub struct RefResolver;

#[async_trait]
impl ReferenceResolver for RefResolver {
    fn ref_kind(&self) -> &'static str {
        "marketplace_product_ref"
    }

    async fn represent(&self, id: uuid::Uuid) -> Option<String> {
        let item = super::service::get_by_id(id).await.ok()??;
        pick(&item.base.description, &item.base.code)
    }
}
