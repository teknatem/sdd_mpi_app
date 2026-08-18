//! LLM Artifact Details UI Module (MVVM Standard)
//!
//! Structure:
//! - api.rs: DTOs and API functions
//! - view_model.rs: LlmArtifactDetailsVm with RwSignals
//! - page.rs: Main component LlmArtifactDetails

mod api;
mod page;
mod view_model;

pub use page::LlmArtifactDetails;
pub use view_model::LlmArtifactDetailsVm;
