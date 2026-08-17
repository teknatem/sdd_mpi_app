//! Библиотечная часть бэкенда.
//!
//! Крейт был чисто бинарным, и это структурно запрещало интеграционные тесты:
//! `crates/backend/tests/*.rs` компилируются отдельными крейтами и линкуются
//! против **библиотеки**, которой не существовало. Вся модульная структура
//! живёт здесь, `main.rs` остался тонкой обёрткой со стартовой процедурой.

#![allow(
    clippy::useless_format,
    clippy::unnecessary_map_or,
    clippy::type_complexity,
    clippy::manual_div_ceil,
    clippy::unused_enumerate_index,
    clippy::unnecessary_lazy_evaluations,
    clippy::too_many_arguments,
    clippy::if_same_then_else,
    clippy::unnecessary_cast,
    clippy::redundant_pattern_matching,
    clippy::option_as_ref_deref,
    clippy::derivable_impls
)]

pub mod api;
pub mod dashboards;
pub mod data_schemes;
pub mod data_view;
pub mod domain;
pub mod general_ledger;
pub mod plugins;
pub mod projections;
pub mod quality;
pub mod shared;
pub mod system;
pub mod usecases;

pub use shared::app_state::AppState;
pub use shared::error::{ApiError, ApiResult};
