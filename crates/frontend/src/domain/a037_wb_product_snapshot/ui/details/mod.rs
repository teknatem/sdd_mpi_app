//! WB Product Snapshot Details UI Module (MVVM Standard)
//!
//! - api.rs: DTOs, formatters and API functions
//! - view_model.rs: WbProductSnapshotDetailsVm with RwSignals
//! - page.rs: Main component with Header, TabBar, TabContent
//! - tabs/: Tab components (general, lines, dynamics)

mod api;
mod page;
mod tabs;
mod view_model;

pub use page::WbProductSnapshotDetail;
