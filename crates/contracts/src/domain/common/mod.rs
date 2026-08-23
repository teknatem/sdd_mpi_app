//! Common types and traits for all aggregates

pub mod aggregate_id;
pub mod aggregate_root;
pub mod base_aggregate;
pub mod entity_metadata;
pub mod origin;

// Re-exports
pub use aggregate_id::AggregateId;
pub use aggregate_root::AggregateRoot;
pub use base_aggregate::BaseAggregate;
pub use entity_metadata::EntityMetadata;
pub use origin::Origin;
