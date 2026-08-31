//! Состав источников detail-строк для drill-down по ресурсу Главной книги.
//!
//! Ключ — таблица ресурса из проводки (`resource_table`). Пять проекций
//! связаны с проводкой через `general_ledger_ref` и пользуются общим отбором;
//! p903 связана внешне и загружает строки по-своему.

use std::sync::Arc;

use crate::general_ledger::resource_detail::{self, GlDetailSource};

/// Установить источники detail-строк.
pub fn install() {
    resource_detail::install_detail_sources(catalog());
}

fn catalog() -> Vec<Arc<dyn GlDetailSource>> {
    vec![
        Arc::new(crate::projections::p909_mp_order_line_turnovers::service::GlDetail)
            as Arc<dyn GlDetailSource>,
        Arc::new(crate::projections::p910_mp_unlinked_turnovers::service::GlDetail),
        Arc::new(crate::projections::p911_wb_advert_by_items::service::GlDetail),
        Arc::new(crate::projections::p913_wb_advert_order_attr::service::GlDetail),
        Arc::new(crate::projections::p914_mp_finance_turnovers::service::GlDetail),
        Arc::new(crate::projections::p903_wb_finance_report::service::GlDetail),
    ]
}
