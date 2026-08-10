//! Манифест снапшота наборов данных.
//!
//! Манифест — единственный источник диагностики: по нему целевой инстанс
//! понимает, откуда приехали данные, и считает дифф, не скачивая сам бандл.
//! Хранится дважды: внутри zip (бандл самодостаточен) и отдельным объектом
//! в S3 рядом с бандлом (UI читает его дёшево).

use serde::{Deserialize, Serialize};

/// Версия формата бандла. Инкремент — только при несовместимых изменениях
/// раскладки zip или обязательных полей манифеста.
pub const DATASET_BUNDLE_FORMAT_VERSION: u32 = 1;

/// Имя манифеста внутри zip-бандла (первая запись архива).
pub const MANIFEST_ENTRY_NAME: &str = "manifest.json";

/// Префикс, под которым лежат файлы наборов внутри zip: `sets/<set_id>/<rel>`.
pub const SETS_ENTRY_PREFIX: &str = "sets";

/// Тип набора. `Database` зарезервирован под фазу 2 — БД не помещается в тот же
/// zip (нужен отдельный объект и multipart-загрузка), поэтому манифест с самого
/// начала описывает снапшот как набор объектов.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DatasetKind {
    Files,
    Database,
}

impl DatasetKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Files => "files",
            Self::Database => "database",
        }
    }
}

/// Назначение инстанса. Показывается бейджем в UI, чтобы «Тестовый» нельзя было
/// перепутать с «Рабочим» при восстановлении.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InstanceEnv {
    Production,
    Staging,
    Dev,
    Unknown,
}

impl Default for InstanceEnv {
    fn default() -> Self {
        Self::Unknown
    }
}

impl InstanceEnv {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Production => "production",
            Self::Staging => "staging",
            Self::Dev => "dev",
            Self::Unknown => "unknown",
        }
    }

    pub fn label_ru(&self) -> &'static str {
        match self {
            Self::Production => "Рабочий",
            Self::Staging => "Тестовый",
            Self::Dev => "Разработка",
            Self::Unknown => "Не задан",
        }
    }
}

impl From<&str> for InstanceEnv {
    fn from(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "production" | "prod" => Self::Production,
            "staging" | "stage" | "test" => Self::Staging,
            "dev" | "development" => Self::Dev,
            _ => Self::Unknown,
        }
    }
}

/// Откуда взялся путь набора. Часть диагностики: в UI видно, лежит ли каталог
/// под общим `[data].root` (основной режим) или уведён индивидуальным путём.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PathOrigin {
    /// Индивидуальный абсолютный путь в своей секции конфига
    /// (`[database].path`, `[llm].skills_path` и т.п.) — отклонение от общего корня.
    /// Имя варианта и строка `legacy_override` сохранены ради манифестов уже
    /// выгруженных снапшотов.
    LegacyOverride,
    /// `<[data].root>/<фиксированное имя подкаталога>`.
    DataRoot,
    /// Относительный путь, разрешённый от каталога бинарника.
    ExeRelative,
}

impl PathOrigin {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::LegacyOverride => "legacy_override",
            Self::DataRoot => "data_root",
            Self::ExeRelative => "exe_relative",
        }
    }

    pub fn label_ru(&self) -> &'static str {
        match self {
            Self::LegacyOverride => "Индивидуальный путь",
            Self::DataRoot => "Из [data].root",
            Self::ExeRelative => "Относительно бинарника",
        }
    }
}

/// Тип объекта снапшота в S3.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BundleObjectKind {
    /// Zip со всеми файловыми наборами снапшота.
    FilesZip,
    /// Снапшот БД отдельным объектом (фаза 2).
    DatabaseSnapshot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Compression {
    None,
    Deflate,
}

/// Режим восстановления набора.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RestoreMode {
    /// Файлы бандла перезаписывают одноимённые; лишние локальные остаются.
    Merge,
    /// Набор становится точной копией бандла; лишние локальные файлы удаляются
    /// (кроме путей под `exclude` дескриптора).
    Replace,
}

impl RestoreMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Merge => "merge",
            Self::Replace => "replace",
        }
    }

    pub fn label_ru(&self) -> &'static str {
        match self {
            Self::Merge => "Наложить поверх (merge)",
            Self::Replace => "Точная копия (replace)",
        }
    }
}

impl Default for RestoreMode {
    fn default() -> Self {
        Self::Merge
    }
}

/// Диагностика инстанса-источника. Пишется в манифест каждого снапшота.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceInfo {
    /// Стабильный идентификатор инстанса из `[instance].id` (или hostname).
    pub instance_id: String,
    /// Человекочитаемое имя для UI.
    pub instance_label: String,
    pub instance_env: InstanceEnv,
    /// Пишется всегда, даже при явном `instance_id`: два снапшота с одним id, но
    /// разными hostname — признак того, что config.toml скопировали и забыли
    /// поменять идентификатор.
    pub hostname: String,
    pub app_version: String,
    pub git_commit: String,
    pub build_profile: String,
    /// Версия применённой миграции БД на момент снапшота.
    pub schema_version: i64,
    pub os: String,
    pub data_root: String,
    pub database_path: String,
    pub config_path: String,
    /// Время снятия снапшота, RFC 3339.
    pub captured_at: String,
}

/// Один объект снапшота в S3.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotObject {
    pub object_index: u32,
    pub key: String,
    pub kind: BundleObjectKind,
    pub size_bytes: u64,
    pub sha256: String,
    pub compression: Compression,
    /// Загружался ли объект по частям (задел под фазу 2; сейчас всегда `false`).
    pub multipart: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntry {
    /// Путь относительно корня набора, всегда через `/`.
    pub path: String,
    pub size_bytes: u64,
    pub sha256: String,
    /// RFC 3339, если удалось прочитать mtime.
    pub modified_at: Option<String>,
}

/// Файл, не попавший в бандл. Диагностика: без этого списка «набор выгрузился,
/// но файла в нём нет» выглядит как потеря данных.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkippedEntry {
    pub path: String,
    pub reason: String,
}

/// Статистика БД для набора `database` (фаза 2).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DbSetStats {
    pub file_mb: f64,
    pub reclaimable_mb: f64,
    pub wal_mb: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetManifest {
    pub set_id: String,
    pub label_ru: String,
    pub kind: DatasetKind,
    /// Индекс объекта в `BundleManifest::objects`, где лежат данные набора.
    pub object_index: u32,
    /// Абсолютный путь на инстансе-источнике.
    pub source_path: String,
    pub path_origin: PathOrigin,
    /// Существовал ли каталог на источнике. Пустой набор — не ошибка, но повод
    /// показать предупреждение.
    pub existed: bool,
    pub file_count: u64,
    pub total_bytes: u64,
    /// Детерминированный хеш дерева: sha256 над отсортированными по `path`
    /// строками `"<path>\0<size>\0<file_sha256>\n"`. От порядка обхода не зависит.
    pub sha256: String,
    pub files: Vec<FileEntry>,
    pub skipped: Vec<SkippedEntry>,
    pub db_stats: Option<DbSetStats>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundleManifest {
    pub format_version: u32,
    /// `<YYYYMMDDTHHMMSSZ>-<8hex>` — лексикографически сортируемый и уникальный.
    pub snapshot_id: String,
    /// RFC 3339.
    pub created_at: String,
    /// `user:<id> (<login>)` либо `auto:pre-restore`.
    pub created_by: String,
    pub note: Option<String>,
    pub source: SourceInfo,
    pub objects: Vec<SnapshotObject>,
    pub sets: Vec<SetManifest>,
}

impl BundleManifest {
    pub fn total_bytes(&self) -> u64 {
        self.objects.iter().map(|object| object.size_bytes).sum()
    }

    pub fn set(&self, set_id: &str) -> Option<&SetManifest> {
        self.sets.iter().find(|set| set.set_id == set_id)
    }

    pub fn object(&self, index: u32) -> Option<&SnapshotObject> {
        self.objects
            .iter()
            .find(|object| object.object_index == index)
    }
}
