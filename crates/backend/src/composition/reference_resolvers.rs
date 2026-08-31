//! Состав резолверов ссылок `/api/refs/resolve`.
//!
//! Ключ — имя реквизита, каким оно стоит в поле агрегата (`connection_mp_ref`),
//! а не имя типа: по типу документа ссылки резолвит реестр регистраторов, и
//! `resolve_reference` уходит туда, когда реквизит здесь не найден.

use std::sync::Arc;

use crate::shared::representation::{self, ReferenceResolver};

/// Установить резолверы ссылок.
pub fn install() {
    representation::install_reference_resolvers(catalog());
}

fn catalog() -> Vec<Arc<dyn ReferenceResolver>> {
    vec![
        Arc::new(crate::domain::a002_organization::representation::RefResolver)
            as Arc<dyn ReferenceResolver>,
        Arc::new(crate::domain::a004_nomenclature::representation::RefResolver),
        Arc::new(crate::domain::a006_connection_mp::representation::RefResolver),
        Arc::new(crate::domain::a007_marketplace_product::representation::RefResolver),
        Arc::new(crate::domain::a013_ym_order::representation::RefResolver),
    ]
}

#[cfg(test)]
mod tests {
    use super::catalog;

    /// Имя реквизита обязано оканчиваться на `_ref`: так они называются в
    /// полях агрегатов, и фронт запрашивает их ровно этим именем.
    #[test]
    fn kinds_look_like_reference_fields() {
        for resolver in catalog() {
            assert!(
                resolver.ref_kind().ends_with("_ref"),
                "реквизит '{}' не похож на имя ссылки",
                resolver.ref_kind()
            );
        }
    }
}
