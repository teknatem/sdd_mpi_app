//! LLM Chat Details UI Module (MVVM Standard)
//!
//! Structure:
//! - api.rs: DTOs and API functions
//! - view_model.rs: LlmChatDetailsVm with RwSignals
//! - page.rs: Main component LlmChatDetails
//! - artifact_card.rs: Component for displaying artifact cards

mod api;
mod artifact_card;
mod page;
mod prefs;
mod questions_bar;
mod settings_dialog;
mod tool_calls_trace;
mod view_model;
mod workspace_drawer;

pub use artifact_card::ArtifactCard;
pub use page::LlmChatDetails;
pub use tool_calls_trace::ToolCallsTrace;
pub use view_model::LlmChatDetailsVm;
