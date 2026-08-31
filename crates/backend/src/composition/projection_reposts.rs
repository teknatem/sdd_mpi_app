//! Состав реестра пересбора проекций.
//!
//! Порядок значим: по нему строится список на странице перепроведения `u508`.

use std::sync::Arc;

use crate::usecases::u508_repost_documents::{projection_repost, ProjectionRepost};

/// Установить реестр пересбора проекций.
pub fn install() {
    projection_repost::install(catalog());
}

fn catalog() -> Vec<Arc<dyn ProjectionRepost>> {
    vec![
        Arc::new(crate::projections::p903_wb_finance_report::service::Repost)
            as Arc<dyn ProjectionRepost>,
        Arc::new(crate::projections::p904_sales_data::service::Repost),
        Arc::new(crate::projections::p907_ym_payment_report::service::Repost),
    ]
}

#[cfg(test)]
mod tests {
    use super::catalog;

    /// Ключ проекции — имя её каталога. Расхождение означало бы пункт в UI,
    /// который не находит свою реализацию.
    #[test]
    fn keys_are_catalog_names() {
        let keys: Vec<&str> = catalog()
            .iter()
            .map(|projection| projection.key())
            .collect();
        assert_eq!(
            keys,
            vec![
                "p903_wb_finance_report",
                "p904_sales_data",
                "p907_ym_payment_report",
            ]
        );
    }

    /// Пункт без описания — пустая строка в списке, где пользователь выбирает,
    /// что именно пересобрать.
    #[test]
    fn options_are_filled_in() {
        for projection in catalog() {
            let option = projection.option();
            assert!(!option.label.is_empty(), "{}", projection.key());
            assert!(!option.description.is_empty(), "{}", projection.key());
        }
    }
}
