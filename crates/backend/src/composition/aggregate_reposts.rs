//! Состав реестра собственных стратегий перепроведения оптом.
//!
//! Крюк необязательный: агрегат попадает сюда, только если общий путь движка
//! (`Registrator::ids_in_period` плюс конкурентный прогон) ему объективно не
//! годится. Сейчас такой один.

use std::sync::Arc;

use crate::usecases::u508_repost_documents::{aggregate_repost, AggregateBulkRepost};

/// Установить реестр стратегий перепроведения.
pub fn install() {
    aggregate_repost::install(catalog());
}

fn catalog() -> Vec<Arc<dyn AggregateBulkRepost>> {
    vec![Arc::new(crate::domain::a012_wb_sales::service::BulkRepost) as Arc<dyn AggregateBulkRepost>]
}

#[cfg(test)]
mod tests {
    use super::catalog;

    /// Стратегия обязана называться ключом агрегата, иначе движок её не найдёт
    /// и молча уйдёт на общий путь — то есть потеряет ускорение, ничего не
    /// сломав. Такое не замечают месяцами.
    #[test]
    fn strategy_keys_match_their_aggregates() {
        let keys: Vec<&str> = catalog().iter().map(|strategy| strategy.key()).collect();
        assert_eq!(keys, vec!["a012_wb_sales"]);
    }

    /// Каждая стратегия должна принадлежать существующему регистратору:
    /// перепроведение оптом начинается с `repost_option` на странице u508.
    #[test]
    fn strategies_belong_to_registered_aggregates() {
        let registrators = super::super::registrators::test_support::catalog_keys();
        for strategy in catalog() {
            assert!(
                registrators.contains(&strategy.key()),
                "стратегия '{}' не имеет регистратора",
                strategy.key()
            );
        }
    }
}
