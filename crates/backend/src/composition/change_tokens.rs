//! Состав реестра токенов изменений.
//!
//! Токен есть у домена, чьи списки фронт держит открытыми и обновляет
//! поллингом (`GET /api/sys/change-tokens`). Имена — те же, что видит фронт;
//! менять их нельзя, не поправив клиента.

use crate::shared::change_token::{self, ChangeToken};

/// Установить реестр токенов изменений.
pub fn install() {
    change_token::install(catalog());
}

fn catalog() -> Vec<(&'static str, &'static ChangeToken)> {
    vec![
        ("sys_tasks", &crate::system::tasks::change_token::TOKEN),
        ("sys_tickets", &crate::system::tickets::change_token::TOKEN),
        ("plugins", &crate::plugins::change_token::TOKEN),
        (
            "a027_wb_documents",
            &crate::domain::a027_wb_documents::change_token::TOKEN,
        ),
        (
            "a015_wb_orders",
            &crate::domain::a015_wb_orders::change_token::TOKEN,
        ),
        (
            "a012_wb_sales",
            &crate::domain::a012_wb_sales::change_token::TOKEN,
        ),
        (
            "a013_ym_order",
            &crate::domain::a013_ym_order::change_token::TOKEN,
        ),
    ]
}
