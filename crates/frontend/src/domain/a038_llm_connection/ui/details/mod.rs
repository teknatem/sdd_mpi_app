//! LLM Connection Details UI Module (MVVM Standard)
//!
//! Structure:
//! - api.rs: DTOs and API functions
//! - view_model.rs: LlmConnectionDetailsVm with RwSignals
//! - page.rs: Main component LlmConnectionDetails

mod api;
mod page;
mod view_model;

pub use page::LlmConnectionDetails;
pub use view_model::LlmConnectionDetailsVm;
