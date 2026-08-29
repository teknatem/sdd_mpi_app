//! Страница «Процессы» (`sys_processes`), admin-only.
//!
//! Слой зеркалит бэкендовый `processes/`: механизм Процессов, Этапов и
//! Действий (ADR-0011). Экранов два: `sys_processes` — разбор и допуск
//! (экземпляры, определения, каталог, журналы), и `sys_stage_details_<code>` —
//! страница одного Этапа с редакторами манифеста и mjs. Процессы по-прежнему
//! правятся через API: у графа своего редактора нет.

pub mod api;
pub mod ui;

pub use ui::stage_details::StageDetailsPage;
pub use ui::ProcessesPage;
