//! Страница «Процессы» (`sys_processes`), admin-only.
//!
//! Слой зеркалит бэкендовый `processes/`: механизм Процессов, Этапов и
//! Действий (ADR-0011). UI здесь один — экран разбора и допуска; редактор
//! определений в первой версии не заводим, определения пишет LLM и правит
//! администратор через API.

pub mod api;
pub mod ui;

pub use ui::stage_details::StageDetailsPage;
pub use ui::ProcessesPage;
