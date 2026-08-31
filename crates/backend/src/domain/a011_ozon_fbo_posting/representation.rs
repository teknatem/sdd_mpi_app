//! Регистратор агрегата a011_ozon_fbo_posting.
//!
//! Представления у него нет — документ не показывается в drill-down Главной
//! книги; в реестре он ради перепроведения по ключам `p904_sales_data`.

use crate::shared::registrators::{Registrator, RegistratorMeta};
use anyhow::Result;
use async_trait::async_trait;
use uuid::Uuid;

/// Регистратор `a011_ozon_fbo_posting` — отправления Ozon FBO.
pub struct Provider;

#[async_trait]
impl Registrator for Provider {
    fn kind(&self) -> &'static str {
        "a011_ozon_fbo_posting"
    }

    /// Ключ этого же типа в `p904_sales_data`.
    fn aliases(&self) -> &'static [&'static str] {
        &["OZON_FBO"]
    }

    fn meta(&self) -> RegistratorMeta {
        RegistratorMeta {
            type_label: RegistratorMeta::UNKNOWN.type_label,
            link_label: None,
            can_post: true,
            tab_key_prefix: None,
        }
    }

    async fn post_document(&self, id: Uuid) -> Result<()> {
        super::posting::post_document(id).await
    }
}
