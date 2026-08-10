//! Каталог снапшотов в S3 (`datasets/catalog.json`).
//!
//! Единственный способ для целевого инстанса узнать, какие снапшоты существуют:
//! таблица `sys_files_s3` локальна для инстанса-донора, а листинга бакета в
//! S3-клиенте нет. Каталог обновляется read-modify-write — как `plugins/catalog.json`.
//!
//! В каталог намеренно НЕ попадают деревья файлов (`SetManifest::files`): иначе
//! объект разрастётся до мегабайтов за десяток снапшотов. Дерево лежит в
//! отдельном `manifest.json` рядом с бандлом.

use serde::{Deserialize, Serialize};

use super::manifest::{DatasetKind, SnapshotObject, SourceInfo};

pub const DATASET_CATALOG_FORMAT_VERSION: u32 = 1;

/// Сводка по набору внутри снапшота — без списка файлов.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetSummary {
    pub set_id: String,
    pub label_ru: String,
    pub kind: DatasetKind,
    pub file_count: u64,
    pub total_bytes: u64,
    pub sha256: String,
    pub existed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotSummary {
    pub snapshot_id: String,
    /// RFC 3339.
    pub created_at: String,
    pub created_by: String,
    pub note: Option<String>,
    pub source: SourceInfo,
    pub sets: Vec<SetSummary>,
    pub objects: Vec<SnapshotObject>,
}

impl SnapshotSummary {
    pub fn total_bytes(&self) -> u64 {
        self.objects.iter().map(|object| object.size_bytes).sum()
    }

    pub fn file_count(&self) -> u64 {
        self.sets.iter().map(|set| set.file_count).sum()
    }

    pub fn set_ids(&self) -> Vec<String> {
        self.sets.iter().map(|set| set.set_id.clone()).collect()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatasetCatalog {
    pub format_version: u32,
    /// Отсортированы по `created_at` по убыванию.
    pub snapshots: Vec<SnapshotSummary>,
}

impl Default for DatasetCatalog {
    fn default() -> Self {
        Self {
            format_version: DATASET_CATALOG_FORMAT_VERSION,
            snapshots: Vec::new(),
        }
    }
}

impl DatasetCatalog {
    pub fn find(&self, snapshot_id: &str) -> Option<&SnapshotSummary> {
        self.snapshots
            .iter()
            .find(|snapshot| snapshot.snapshot_id == snapshot_id)
    }

    /// Снапшоты конкретного инстанса, свежие первыми. Используется ротацией:
    /// keep-N применяется только к своим снапшотам, чужие не трогаем никогда —
    /// иначе тестовый инстанс снесёт бэкапы рабочего.
    pub fn by_instance(&self, instance_id: &str) -> Vec<&SnapshotSummary> {
        self.snapshots
            .iter()
            .filter(|snapshot| snapshot.source.instance_id == instance_id)
            .collect()
    }
}
