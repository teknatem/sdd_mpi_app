//! Состав реестра регистраторов.
//!
//! Порядок значим: по нему строится список агрегатов на странице
//! перепроведения `u508`. Первыми идут проводимые типы в том порядке, в каком
//! они там показывались; дальше — те, что участвуют только в представлениях.

use std::sync::Arc;

use crate::shared::registrators::{self, Registrator};

/// Установить реестр регистраторов.
pub fn install() {
    registrators::install(catalog());
}

fn catalog() -> Vec<Arc<dyn Registrator>> {
    vec![
        // --- Проводимые агрегаты, порядок = список на странице u508 ---------
        Arc::new(crate::domain::a012_wb_sales::representation::Provider) as Arc<dyn Registrator>,
        Arc::new(crate::domain::a015_wb_orders::representation::Provider),
        Arc::new(crate::domain::a021_production_output::representation::Provider),
        Arc::new(crate::domain::a023_purchase_of_goods::representation::Provider),
        Arc::new(crate::domain::a026_wb_advert_daily::representation::Provider),
        Arc::new(crate::domain::a034_ym_realization::representation::Provider),
        Arc::new(crate::domain::a013_ym_order::representation::Provider),
        Arc::new(crate::domain::a016_ym_returns::representation::Provider),
        // --- Проводятся поштучно, оптом за период — нет --------------------
        Arc::new(crate::domain::a009_ozon_returns::representation::Provider),
        Arc::new(crate::domain::a010_ozon_fbs_posting::representation::Provider),
        Arc::new(crate::domain::a011_ozon_fbo_posting::representation::Provider),
        Arc::new(crate::domain::a014_ozon_transactions::representation::Provider),
        // --- Только представление в drill-down Главной книги ----------------
        Arc::new(crate::domain::a022_kit_variant::representation::Provider),
        Arc::new(crate::domain::a028_missing_cost_registry::representation::Provider),
        Arc::new(crate::domain::a036_wb_sales_funnel_daily::representation::Provider),
        Arc::new(crate::domain::a037_wb_product_snapshot::representation::Provider),
        Arc::new(crate::domain::a040_wb_search_analytics_daily::representation::Provider),
        Arc::new(crate::projections::p903_wb_finance_report::representation::Provider),
        Arc::new(crate::projections::p907_ym_payment_report::representation::Provider),
    ]
}

/// Доступ к составу каталога для перекрёстных проверок соседних реестров.
#[cfg(test)]
pub mod test_support {
    /// Канонические ключи всех зарегистрированных регистраторов.
    pub fn catalog_keys() -> Vec<&'static str> {
        super::catalog()
            .iter()
            .map(|registrator| registrator.kind())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::catalog;
    use std::collections::HashSet;

    /// Исторические значения `registrator_type` в `p904_sales_data`.
    ///
    /// Это не список «на всякий случай», а содержимое живой колонки: заказы и
    /// возвраты, накопленные до перехода на канонические ключи. Пока в проекции
    /// лежит хоть одна такая строка, перепроведение по ней обязано находить
    /// регистратор — иначе документы молча перестанут перепроводиться.
    const P904_LEGACY_KEYS: &[&str] = &[
        "WB_Sales",
        "YM_Order",
        "OZON_FBS",
        "OZON_FBO",
        "YM_Returns",
        "OZON_Returns",
    ];

    #[test]
    fn legacy_p904_keys_are_covered() {
        let known: HashSet<&str> = catalog()
            .iter()
            .flat_map(|registrator| registrator.aliases().iter().copied())
            .collect();

        let missing: Vec<&str> = P904_LEGACY_KEYS
            .iter()
            .copied()
            .filter(|key| !known.contains(key))
            .collect();

        assert!(
            missing.is_empty(),
            "исторические ключи p904_sales_data без регистратора: {missing:?}"
        );
    }

    #[test]
    fn keys_do_not_collide() {
        let mut seen: HashSet<&str> = HashSet::new();
        for registrator in catalog() {
            for key in
                std::iter::once(registrator.kind()).chain(registrator.aliases().iter().copied())
            {
                assert!(
                    seen.insert(key),
                    "ключ '{key}' заявлен двумя регистраторами"
                );
            }
        }
    }

    /// Пункт на странице перепроведения обещает пользователю действие.
    /// Обещание без `can_post` — кнопка, которая ничего не сделает.
    #[test]
    fn repost_options_are_backed_by_posting() {
        for registrator in catalog() {
            if registrator.repost_option().is_some() {
                assert!(
                    registrator.meta().can_post,
                    "'{}' предлагается к перепроведению оптом, но не объявляет can_post",
                    registrator.kind()
                );
            }
        }
    }
}
