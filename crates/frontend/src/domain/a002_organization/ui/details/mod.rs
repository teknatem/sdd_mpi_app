//! Organization Details UI Module
//!
//! Simplified MVVM pattern implementation:
//! - api.rs: API functions (fetch, save)
//! - view_model.rs: ViewModel with commands and state management
//! - page.rs: Leptos component (pure UI)

mod api;
mod page;
mod view_model;

pub use page::OrganizationDetails;
pub use view_model::OrganizationDetailsViewModel;
