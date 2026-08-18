//! WB Advert Daily Details UI Module (MVVM Standard)
//!
//! Structure:
//! - api.rs: DTOs, formatters and API functions
//! - view_model.rs: WbAdvertDailyDetailsVm with RwSignals
//! - page.rs: Main component with Header, TabBar, TabContent
//! - tabs/: Tab components (general, lines, attribution, journal, projections)

mod api;
mod page;
mod tabs;
mod view_model;

pub use page::WbAdvertDailyDetail;
