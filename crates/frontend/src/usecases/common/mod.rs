//! Общая страница загрузок с маркетплейсов (u502 / u503 / u504).
//!
//! Каталог операций каждого маркетплейса лежит в его `ops.rs`, всё остальное —
//! разметка, состояние строк, HTTP и опрос прогресса — здесь.

pub mod catalog;
pub mod client;
pub mod page;
pub mod progress;

pub use catalog::{ImportOp, OpGroup, PeriodKind};
pub use client::ImportUseCase;
pub use page::ImportPage;
