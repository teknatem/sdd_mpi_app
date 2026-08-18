//! Nomenclature Details UI Module (EditDetails MVVM Standard)
//!
//! Structure:
//! - api.rs: API functions (fetch, save, delete) + DTOs
//! - view_model.rs: ViewModel with RwSignal fields, commands, validation
//! - page.rs: Main component (thin wrapper with tab routing)
//! - tabs/: UI components for each tab
//!   - general.rs: Basic nomenclature fields
//!   - dimensions.rs: Dimension fields with autocomplete
//!   - barcodes.rs: Barcodes table
//! - dimension_input.rs: Custom dimension input with dropdown

mod api;
mod dimension_input;
mod page;
mod tabs;
mod view_model;

pub use dimension_input::DimensionInput;
pub use page::NomenclatureDetails;
pub use view_model::NomenclatureDetailsVm;
