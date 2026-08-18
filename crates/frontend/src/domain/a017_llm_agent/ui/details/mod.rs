//! LLM Agent Details UI Module (MVVM Standard)
//!
//! Structure:
//! - api.rs: DTOs and API functions
//! - view_model.rs: LlmAgentDetailsVm with RwSignals
//! - page.rs: Main component LlmAgentDetails

mod api;
mod page;
mod view_model;

pub use page::LlmAgentDetails;
pub use view_model::LlmAgentDetailsVm;
