//! Наборы данных: систематизированная раскладка файловых данных на диске плюс
//! перенос выбранных наборов между экземплярами приложения через S3.
//!
//! Модель намеренно описывает снапшот как **набор объектов**, а не как один zip:
//! файловые наборы помещаются в общий архив, а БД (фаза 2) потребует отдельного
//! объекта с multipart-загрузкой. Благодаря `SnapshotObject` + `object_index`
//! добавление БД не ломает формат манифеста.

pub mod api;
pub mod catalog;
pub mod manifest;

pub use api::*;
pub use catalog::{DatasetCatalog, SetSummary, SnapshotSummary, DATASET_CATALOG_FORMAT_VERSION};
pub use manifest::{
    BundleManifest, BundleObjectKind, Compression, DatasetKind, DbSetStats, FileEntry, InstanceEnv,
    PathOrigin, RestoreMode, SetManifest, SkippedEntry, SnapshotObject, SourceInfo,
    DATASET_BUNDLE_FORMAT_VERSION, MANIFEST_ENTRY_NAME, SETS_ENTRY_PREFIX,
};
