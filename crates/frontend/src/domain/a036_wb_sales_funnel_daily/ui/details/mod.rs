//! WB Sales Funnel Daily Details UI Module (MVVM Standard)
//!
//! Structure:
//! - api.rs: DTOs, formatters and API functions
//! - view_model.rs: WbSalesFunnelDailyDetailsVm with RwSignals
//! - page.rs: Main component with Header, TabBar, TabContent
//! - tabs/: Tab components (general, lines)

pub mod api;
mod page;
mod tabs;
mod view_model;

pub use page::WbSalesFunnelDailyDetail;
