//! DTO HTTP-слоя подсистемы наборов данных (`/api/sys/datasets/*`).

use serde::{Deserialize, Serialize};

use super::catalog::SnapshotSummary;
use super::manifest::{
    BundleManifest, DatasetKind, DbSetStats, InstanceEnv, PathOrigin, RestoreMode,
};

// ---------------------------------------------------------------------------
// Локальное состояние: наборы, пути, диагностика инстанса
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstanceInfoDto {
    pub instance_id: String,
    pub instance_label: String,
    pub instance_env: InstanceEnv,
    pub hostname: String,
    pub app_version: String,
    pub git_commit: String,
    pub build_profile: String,
    pub schema_version: i64,
    pub os: String,
    pub data_root: Option<String>,
    pub database_path: String,
    pub config_path: String,
    /// Готовность S3: если `false`, каталог и выгрузка недоступны.
    pub s3_ready: bool,
    pub s3_bucket: Option<String>,
    pub s3_endpoint: Option<String>,
    /// Причина неготовности S3 — показывается пользователю как есть.
    pub s3_error: Option<String>,
}

/// Один набор в локальном состоянии.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatasetStatusEntry {
    pub set_id: String,
    pub label_ru: String,
    pub description_ru: String,
    pub kind: DatasetKind,
    pub path: String,
    pub path_origin: PathOrigin,
    pub exists: bool,
    pub file_count: u64,
    pub total_bytes: u64,
    pub skipped_count: u64,
    pub selected_by_default: bool,
    pub supported_modes: Vec<RestoreMode>,
    /// `false` для наборов, реализация которых ещё не готова (набор `database`
    /// до фазы 2). Строка видна в UI, но чекбокс задизейблен.
    pub available: bool,
    pub unavailable_reason: Option<String>,
    pub db_stats: Option<DbSetStats>,
    /// Ошибка сканирования каталога, если он есть, но не читается.
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnomalySeverity {
    Info,
    Warning,
}

/// Замеченная странность в раскладке данных: осиротевший legacy-каталог,
/// пустой набор, вложения, оставшиеся в старом месте.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatasetAnomalyDto {
    pub code: String,
    pub severity: AnomalySeverity,
    pub message: String,
    pub path: Option<String>,
    /// Что с этим делать — текстом, без автоматических действий.
    pub hint: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatasetsStatusDto {
    pub instance: InstanceInfoDto,
    pub sets: Vec<DatasetStatusEntry>,
    pub anomalies: Vec<DatasetAnomalyDto>,
}

// ---------------------------------------------------------------------------
// Создание снапшота
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateSnapshotRequest {
    pub set_ids: Vec<String>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateSnapshotResponse {
    pub snapshot: SnapshotSummary,
    /// Ключ zip-бандла в бакете.
    pub bundle_key: String,
    pub bundle_sha256: String,
    pub bundle_size_bytes: u64,
    /// Сколько старых снапшотов этого же инстанса удалила ротация.
    pub rotated_away: Vec<String>,
    /// Непустой список, если часть файлов не попала в бандл.
    pub warnings: Vec<String>,
}

// ---------------------------------------------------------------------------
// Каталог и манифест
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatasetCatalogResponse {
    pub snapshots: Vec<SnapshotSummary>,
    /// Идентификатор текущего инстанса — чтобы UI отделил свои снапшоты от чужих.
    pub local_instance_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotManifestResponse {
    pub manifest: BundleManifest,
}

// ---------------------------------------------------------------------------
// Restore: preview
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestorePreviewRequest {
    pub snapshot_id: String,
    pub set_ids: Vec<String>,
    pub mode: RestoreMode,
}

/// Класс различия для одного файла.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiffClass {
    /// Есть в бандле, нет локально.
    Added,
    /// Есть в обоих, содержимое различается.
    Modified,
    /// Совпадает по sha256.
    Identical,
    /// Есть локально, нет в бандле. При `replace` будет удалён.
    LocalOnly,
}

impl DiffClass {
    pub fn label_ru(&self) -> &'static str {
        match self {
            Self::Added => "Добавится",
            Self::Modified => "Изменится",
            Self::Identical => "Без изменений",
            Self::LocalOnly => "Только локально",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileDiffEntry {
    pub path: String,
    pub class: DiffClass,
    pub bundle_size_bytes: Option<u64>,
    pub local_size_bytes: Option<u64>,
    pub bundle_modified_at: Option<String>,
    pub local_modified_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetDiff {
    pub set_id: String,
    pub label_ru: String,
    pub local_path: String,
    pub local_exists: bool,
    pub added: u64,
    pub modified: u64,
    pub identical: u64,
    pub local_only: u64,
    pub bytes_written: u64,
    /// Сколько байт будет удалено (только при `replace`).
    pub bytes_deleted: u64,
    /// Первые `entries_limit` записей; полное дерево не отдаём.
    pub entries: Vec<FileDiffEntry>,
    pub entries_truncated: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WarningSeverity {
    Info,
    Warning,
    /// Восстановление невозможно, кнопка «Восстановить» блокируется.
    Blocking,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestoreWarning {
    pub code: String,
    pub severity: WarningSeverity,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestorePreviewDto {
    pub snapshot_id: String,
    pub mode: RestoreMode,
    pub source: InstanceInfoSummary,
    pub sets: Vec<SetDiff>,
    pub warnings: Vec<RestoreWarning>,
    /// `true`, если среди предупреждений есть `Blocking`.
    pub blocking: bool,
    /// sha256 бандла на момент preview — передаётся в `apply`, чтобы применить
    /// ровно то, что показали в диффе.
    pub bundle_sha256: String,
}

/// Урезанная карточка источника для UI поверх дифф-модалки.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstanceInfoSummary {
    pub instance_id: String,
    pub instance_label: String,
    pub instance_env: InstanceEnv,
    pub hostname: String,
    pub app_version: String,
    pub git_commit: String,
    pub schema_version: i64,
    pub captured_at: String,
}

// ---------------------------------------------------------------------------
// Restore: apply
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestoreApplyRequest {
    pub snapshot_id: String,
    pub set_ids: Vec<String>,
    pub mode: RestoreMode,
    /// sha256 из preview. Если бандл в бакете успели заменить — операция
    /// отклоняется, чтобы не применить не тот дифф, который подтвердил админ.
    pub expected_bundle_sha256: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestoreSetResult {
    pub set_id: String,
    pub label_ru: String,
    pub files_written: u64,
    pub files_deleted: u64,
    pub bytes_written: u64,
    /// Результат горячей перезагрузки кэша, если набор её требует.
    pub reload_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestoreResultDto {
    pub snapshot_id: String,
    pub mode: RestoreMode,
    pub sets: Vec<RestoreSetResult>,
    /// Путь к автоснапшоту, снятому перед применением.
    pub pre_restore_archive: Option<String>,
    /// Фаза 2: восстановление БД потребует перезапуска процесса.
    pub requires_restart: bool,
    pub warnings: Vec<String>,
}

// ---------------------------------------------------------------------------
// Журнал операций
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferLogEntryDto {
    pub id: String,
    /// `snapshot` | `restore`.
    pub operation: String,
    pub snapshot_id: String,
    pub instance_id: String,
    pub source_instance_id: Option<String>,
    pub set_ids: Vec<String>,
    pub mode: Option<String>,
    /// `ok` | `failed` | `rolled_back`.
    pub status: String,
    pub bytes: i64,
    pub files_written: i64,
    pub files_deleted: i64,
    pub pre_restore_archive: Option<String>,
    pub error: Option<String>,
    pub actor_user_id: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferLogResponse {
    pub items: Vec<TransferLogEntryDto>,
}
