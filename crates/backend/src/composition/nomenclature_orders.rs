//! Состав источников заказов для карточки номенклатуры.
//!
//! Порядок здесь не значим: список источников сортируется по дате заказа
//! целиком, уже после сборки.

use std::sync::Arc;

use crate::domain::a004_nomenclature::service::{self, NomenclatureOrderSource};

/// Установить источники заказов номенклатуры.
pub fn install() {
    service::install_order_sources(catalog());
}

fn catalog() -> Vec<Arc<dyn NomenclatureOrderSource>> {
    vec![
        Arc::new(crate::domain::a015_wb_orders::service::NomenclatureOrders)
            as Arc<dyn NomenclatureOrderSource>,
        Arc::new(crate::domain::a013_ym_order::service::NomenclatureOrders),
    ]
}
