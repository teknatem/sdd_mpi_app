//! YM Returns Details UI Module (Standard Tab Structure)
//!
//! Structure:
//! - api.rs: DTOs and constants
//! - page.rs: Main component with loading logic and tab navigation
//! - tabs/: Tab components (general, lines, projections, json)

mod api;
mod page;
mod tabs;
mod view_model;

pub use page::YmReturnDetail;
