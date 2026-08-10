//! Выгрузка снапшота наборов в S3 и работа с каталогом снапшотов.
//!
//! Раскладка ключей:
//!
//! ```text
//! datasets/catalog.json
//! datasets/<instance_id>/<snapshot_id>/manifest.json
//! datasets/<instance_id>/<snapshot_id>/bundle.zip
//! ```
//!
//! Известное ограничение (унаследовано от `plugins::publish`): обновление
//! каталога — read-modify-write без условной записи по ETag. Одновременная
//! публикация с двух инстансов может потерять одну запись. Для админской
//! операции, выполняемой руками, это приемлемо.

use bytes::Bytes;
use chrono::Utc;
use contracts::system::datasets::{
    BundleManifest, BundleObjectKind, Compression, CreateSnapshotRequest, CreateSnapshotResponse,
    DatasetCatalog, SetManifest, SetSummary, SnapshotObject, SnapshotSummary,
    DATASET_BUNDLE_FORMAT_VERSION, DATASET_CATALOG_FORMAT_VERSION,
};

use super::registry::{self, DatasetDescriptor};
use super::repository::{self, TransferLogEntry, OPERATION_SNAPSHOT, STATUS_FAILED, STATUS_OK};
use super::scan::{self, DatasetScan};
use super::{bundle, ActorInfo};
use crate::shared::config::{self, Config, S3Config};
use crate::system::s3::client;
use crate::system::s3::service as s3_service;

pub const CATALOG_KEY: &str = "datasets/catalog.json";

pub fn snapshot_prefix(instance_id: &str, snapshot_id: &str) -> String {
    format!("datasets/{instance_id}/{snapshot_id}")
}

pub fn bundle_key(instance_id: &str, snapshot_id: &str) -> String {
    format!("{}/bundle.zip", snapshot_prefix(instance_id, snapshot_id))
}

pub fn manifest_key(instance_id: &str, snapshot_id: &str) -> String {
    format!("{}/manifest.json", snapshot_prefix(instance_id, snapshot_id))
}

// ---------------------------------------------------------------------------
// Каталог
// ---------------------------------------------------------------------------

async fn read_catalog(cfg: &S3Config) -> anyhow::Result<DatasetCatalog> {
    match client::get_object_opt(cfg, CATALOG_KEY).await? {
        None => Ok(DatasetCatalog::default()),
        Some(object) if object.bytes.is_empty() => Ok(DatasetCatalog::default()),
        Some(object) => Ok(serde_json::from_slice(&object.bytes).unwrap_or_default()),
    }
}

async fn write_catalog(cfg: &S3Config, catalog: &DatasetCatalog) -> anyhow::Result<()> {
    let bytes = serde_json::to_vec_pretty(catalog)?;
    client::put_object(
        cfg,
        CATALOG_KEY,
        Some("application/json"),
        Bytes::from(bytes),
    )
    .await?;
    Ok(())
}

pub async fn get_catalog() -> anyhow::Result<DatasetCatalog> {
    let cfg = s3_service::s3_config()?;
    read_catalog(&cfg).await
}

/// Полный манифест снапшота (с деревьями файлов) — отдельным объектом, чтобы
/// UI мог посчитать дифф, не скачивая бандл целиком.
pub async fn get_manifest(snapshot_id: &str) -> anyhow::Result<BundleManifest> {
    let cfg = s3_service::s3_config()?;
    let catalog = read_catalog(&cfg).await?;
    let summary = catalog
        .find(snapshot_id)
        .ok_or_else(|| anyhow::anyhow!("Снапшот '{snapshot_id}' не найден в каталоге"))?;
    let key = manifest_key(&summary.source.instance_id, snapshot_id);
    let object = client::get_object(&cfg, &key).await?;
    Ok(serde_json::from_slice(&object.bytes)?)
}

/// Скачивает zip-бандл снапшота. Возвращает имя файла и байты.
pub async fn download_bundle(snapshot_id: &str) -> anyhow::Result<(String, Bytes)> {
    let cfg = s3_service::s3_config()?;
    let catalog = read_catalog(&cfg).await?;
    let summary = catalog
        .find(snapshot_id)
        .ok_or_else(|| anyhow::anyhow!("Снапшот '{snapshot_id}' не найден в каталоге"))?;
    let key = bundle_key(&summary.source.instance_id, snapshot_id);
    let object = client::get_object(&cfg, &key).await?;
    Ok((format!("{snapshot_id}.zip"), object.bytes))
}

/// Скачивает бандл и сверяет его sha256 с записью каталога.
///
/// Проверка до распаковки — обязательная: содержимое архива подставляется в
/// файловые операции на принимающей стороне, и применять надо ровно тот бандл,
/// который был показан в диффе.
pub(super) async fn fetch_verified_bundle(
    cfg: &S3Config,
    summary: &SnapshotSummary,
    expected_sha256: Option<&str>,
) -> anyhow::Result<Bytes> {
    let files_object = summary
        .objects
        .iter()
        .find(|object| object.kind == BundleObjectKind::FilesZip)
        .ok_or_else(|| anyhow::anyhow!("В снапшоте нет файлового архива"))?;

    if let Some(expected) = expected_sha256 {
        if expected != files_object.sha256 {
            anyhow::bail!(
                "Снапшот в бакете изменился с момента предпросмотра: \
                 ожидался архив {expected}, в каталоге {}. Повторите предпросмотр.",
                files_object.sha256
            );
        }
    }

    let object = client::get_object(cfg, &files_object.key).await?;
    let actual = client::sha256_hex(&object.bytes);
    if actual != files_object.sha256 {
        anyhow::bail!(
            "Контрольная сумма архива не совпала с каталогом (ожидалась {}, получена {actual}). \
             Архив повреждён или подменён.",
            files_object.sha256
        );
    }
    Ok(object.bytes)
}

// ---------------------------------------------------------------------------
// Создание снапшота
// ---------------------------------------------------------------------------

/// Сканирует выбранные наборы. Возвращает сканы в порядке реестра.
pub(super) fn scan_selected(
    config: &Config,
    set_ids: &[String],
) -> anyhow::Result<Vec<(&'static DatasetDescriptor, DatasetScan)>> {
    let mut result = Vec::new();
    for set_id in set_ids {
        let descriptor = registry::lookup(set_id)
            .ok_or_else(|| anyhow::anyhow!("Неизвестный набор данных '{set_id}'"))?;
        if !descriptor.available {
            anyhow::bail!(
                "Набор «{}» пока недоступен: {}",
                descriptor.label_ru,
                descriptor.unavailable_reason.unwrap_or("причина не указана")
            );
        }
        let root = registry::resolve_root(config, descriptor)
            .ok_or_else(|| anyhow::anyhow!("Для набора '{set_id}' не удалось определить путь"))?;
        let scan = scan::scan_directory(descriptor, &root.path)?;
        result.push((descriptor, scan));
    }
    Ok(result)
}

/// Собирает манифест и zip по результатам сканов. Общая часть выгрузки в S3 и
/// автоснапшота перед восстановлением — форматы обязаны совпадать, иначе откат
/// нельзя сделать штатным восстановлением.
pub(super) async fn build_snapshot(
    config: &Config,
    snapshot_id: &str,
    set_ids: &[String],
    note: Option<String>,
    actor: &ActorInfo,
    object_key: String,
) -> anyhow::Result<(BundleManifest, Vec<u8>)> {
    let scans = scan_selected(config, set_ids)?;
    let source = super::source_info(config).await;

    let total_raw: u64 = scans.iter().map(|(_, scan)| scan.total_bytes).sum();
    let limit = config.datasets.max_bundle_bytes(&config.s3);
    if total_raw > limit {
        let heaviest = heaviest_files(&scans, 5);
        anyhow::bail!(
            "Суммарный объём выбранных наборов {:.1} МБ превышает лимит {:.1} МБ. \
             Самые тяжёлые файлы: {}",
            total_raw as f64 / 1024.0 / 1024.0,
            limit as f64 / 1024.0 / 1024.0,
            heaviest.join("; ")
        );
    }

    let mut sets = Vec::new();
    for (descriptor, scan) in &scans {
        let root = registry::resolve_root(config, descriptor)
            .ok_or_else(|| anyhow::anyhow!("Для набора '{}' нет пути", descriptor.id))?;
        sets.push(SetManifest {
            set_id: descriptor.id.to_string(),
            label_ru: descriptor.label_ru.to_string(),
            kind: descriptor.kind,
            object_index: 0,
            source_path: root.display_path(),
            path_origin: root.origin,
            existed: scan.existed,
            file_count: scan.file_count(),
            total_bytes: scan.total_bytes,
            sha256: scan.tree_sha256(),
            files: scan.manifest_entries(),
            skipped: scan.skipped.clone(),
            db_stats: None,
        });
    }

    let mut manifest = BundleManifest {
        format_version: DATASET_BUNDLE_FORMAT_VERSION,
        snapshot_id: snapshot_id.to_string(),
        created_at: Utc::now().to_rfc3339(),
        created_by: actor.created_by(),
        note,
        source,
        objects: vec![SnapshotObject {
            object_index: 0,
            key: object_key,
            kind: BundleObjectKind::FilesZip,
            size_bytes: 0,
            sha256: String::new(),
            compression: Compression::Deflate,
            multipart: false,
        }],
        sets,
    };

    // Архив не может содержать собственный хеш: манифест лежит внутри него.
    // Поэтому у копии манифеста ВНУТРИ zip размер и sha256 своего объекта
    // остаются нулевыми, а заполненные значения несёт внешний manifest.json
    // рядом с бандлом и запись каталога — именно по ним идёт проверка перед
    // распаковкой на принимающей стороне.
    let scan_refs: Vec<(String, &DatasetScan)> = scans
        .iter()
        .map(|(descriptor, scan)| (descriptor.id.to_string(), scan))
        .collect();
    let bytes = bundle::build_bundle(&manifest, &scan_refs)?;
    manifest.objects[0].size_bytes = bytes.len() as u64;
    manifest.objects[0].sha256 = client::sha256_hex(&bytes);

    Ok((manifest, bytes))
}

fn heaviest_files(scans: &[(&'static DatasetDescriptor, DatasetScan)], take: usize) -> Vec<String> {
    let mut files: Vec<(u64, String)> = scans
        .iter()
        .flat_map(|(descriptor, scan)| {
            scan.files.iter().map(move |file| {
                (
                    file.size_bytes,
                    format!("{}/{}", descriptor.id, file.relative_path),
                )
            })
        })
        .collect();
    files.sort_by(|left, right| right.0.cmp(&left.0));
    files
        .into_iter()
        .take(take)
        .map(|(size, path)| format!("{path} ({:.1} МБ)", size as f64 / 1024.0 / 1024.0))
        .collect()
}

fn summarize(manifest: &BundleManifest) -> SnapshotSummary {
    SnapshotSummary {
        snapshot_id: manifest.snapshot_id.clone(),
        created_at: manifest.created_at.clone(),
        created_by: manifest.created_by.clone(),
        note: manifest.note.clone(),
        source: manifest.source.clone(),
        sets: manifest
            .sets
            .iter()
            .map(|set| SetSummary {
                set_id: set.set_id.clone(),
                label_ru: set.label_ru.clone(),
                kind: set.kind,
                file_count: set.file_count,
                total_bytes: set.total_bytes,
                sha256: set.sha256.clone(),
                existed: set.existed,
            })
            .collect(),
        objects: manifest.objects.clone(),
    }
}

/// Снимает снапшот выбранных наборов и выгружает его в S3.
pub async fn create_snapshot(
    request: CreateSnapshotRequest,
    actor: ActorInfo,
) -> anyhow::Result<CreateSnapshotResponse> {
    if request.set_ids.is_empty() {
        anyhow::bail!("Не выбрано ни одного набора данных");
    }
    let config = config::load_config()?;
    let cfg = s3_service::s3_config()?;
    let identity = config::instance_identity(&config);
    let snapshot_id = super::new_snapshot_id();
    let key = bundle_key(&identity.id, &snapshot_id);

    let outcome = create_snapshot_inner(
        &config,
        &cfg,
        &snapshot_id,
        &identity.id,
        key.clone(),
        &request,
        &actor,
    )
    .await;

    match outcome {
        Ok(response) => {
            let _ = repository::insert(&TransferLogEntry {
                operation: OPERATION_SNAPSHOT,
                snapshot_id: snapshot_id.clone(),
                instance_id: identity.id.clone(),
                source_instance_id: Some(identity.id.clone()),
                set_ids: request.set_ids.clone(),
                mode: None,
                status: STATUS_OK,
                bytes: response.bundle_size_bytes as i64,
                files_written: response.snapshot.file_count() as i64,
                files_deleted: 0,
                pre_restore_archive: None,
                manifest_json: serde_json::to_string(&response.snapshot).ok(),
                error: None,
                actor_user_id: actor.user_id.clone(),
            })
            .await;
            Ok(response)
        }
        Err(error) => {
            let _ = repository::insert(&TransferLogEntry {
                operation: OPERATION_SNAPSHOT,
                snapshot_id,
                instance_id: identity.id.clone(),
                source_instance_id: Some(identity.id),
                set_ids: request.set_ids.clone(),
                mode: None,
                status: STATUS_FAILED,
                bytes: 0,
                files_written: 0,
                files_deleted: 0,
                pre_restore_archive: None,
                manifest_json: None,
                error: Some(error.to_string()),
                actor_user_id: actor.user_id.clone(),
            })
            .await;
            Err(error)
        }
    }
}

async fn create_snapshot_inner(
    config: &Config,
    cfg: &S3Config,
    snapshot_id: &str,
    instance_id: &str,
    key: String,
    request: &CreateSnapshotRequest,
    actor: &ActorInfo,
) -> anyhow::Result<CreateSnapshotResponse> {
    let (manifest, bytes) = build_snapshot(
        config,
        snapshot_id,
        &request.set_ids,
        request.note.clone(),
        actor,
        key.clone(),
    )
    .await?;

    let size_bytes = bytes.len() as u64;
    let sha256 = manifest.objects[0].sha256.clone();

    client::put_object(cfg, &key, Some("application/zip"), Bytes::from(bytes)).await?;

    // Манифест кладём вторым объектом. Если этот шаг не пройдёт, оставшийся в
    // бакете архив не попадёт в каталог и останется недоступным — убираем его,
    // чтобы не копить мусор (S3 и запись каталога не в одной транзакции).
    let manifest_object_key = manifest_key(instance_id, snapshot_id);
    let manifest_bytes = serde_json::to_vec_pretty(&manifest)?;
    if let Err(error) = client::put_object(
        cfg,
        &manifest_object_key,
        Some("application/json"),
        Bytes::from(manifest_bytes),
    )
    .await
    {
        let _ = client::delete_object(cfg, &key).await;
        return Err(error);
    }

    let summary = summarize(&manifest);
    let mut catalog = read_catalog(cfg).await?;
    catalog.format_version = DATASET_CATALOG_FORMAT_VERSION;
    catalog
        .snapshots
        .retain(|existing| existing.snapshot_id != summary.snapshot_id);
    catalog.snapshots.push(summary.clone());
    catalog
        .snapshots
        .sort_by(|left, right| right.created_at.cmp(&left.created_at));

    let rotated = plan_rotation(&catalog, instance_id, config.datasets.keep_per_instance);
    for snapshot in &rotated {
        for object in &snapshot.objects {
            let _ = client::delete_object(cfg, &object.key).await;
        }
        let _ = client::delete_object(cfg, &manifest_key(instance_id, &snapshot.snapshot_id)).await;
    }
    let rotated_ids: Vec<String> = rotated
        .iter()
        .map(|snapshot| snapshot.snapshot_id.clone())
        .collect();
    catalog
        .snapshots
        .retain(|snapshot| !rotated_ids.contains(&snapshot.snapshot_id));

    if let Err(error) = write_catalog(cfg, &catalog).await {
        // Каталог — единственный указатель на снапшот. Не записался — снапшота
        // фактически нет, и оставлять недостижимые объекты незачем.
        let _ = client::delete_object(cfg, &key).await;
        let _ = client::delete_object(cfg, &manifest_object_key).await;
        return Err(error);
    }

    let warnings = manifest
        .sets
        .iter()
        .filter(|set| !set.skipped.is_empty())
        .map(|set| {
            format!(
                "«{}»: не попало в архив файлов — {} (первое: {})",
                set.label_ru,
                set.skipped.len(),
                set.skipped
                    .first()
                    .map(|entry| format!("{} — {}", entry.path, entry.reason))
                    .unwrap_or_default()
            )
        })
        .collect();

    Ok(CreateSnapshotResponse {
        snapshot: summary,
        bundle_key: key,
        bundle_sha256: sha256,
        bundle_size_bytes: size_bytes,
        rotated_away: rotated_ids,
        warnings,
    })
}

/// Какие снапшоты удалить, чтобы у инстанса осталось не больше `keep` штук.
///
/// Чистая функция — тестируется без сети. Снапшоты ЧУЖИХ инстансов не попадают
/// в выборку никогда: иначе тестовый экземпляр при первой же выгрузке снёс бы
/// бэкапы рабочего.
pub fn plan_rotation(
    catalog: &DatasetCatalog,
    instance_id: &str,
    keep: usize,
) -> Vec<SnapshotSummary> {
    if keep == 0 {
        return Vec::new();
    }
    let mut own: Vec<&SnapshotSummary> = catalog.by_instance(instance_id);
    own.sort_by(|left, right| right.created_at.cmp(&left.created_at));
    own.into_iter()
        .skip(keep)
        .cloned()
        .collect()
}

/// Удаляет снапшот из бакета и каталога.
pub async fn delete_snapshot(snapshot_id: &str) -> anyhow::Result<()> {
    let cfg = s3_service::s3_config()?;
    let mut catalog = read_catalog(&cfg).await?;
    let summary = catalog
        .find(snapshot_id)
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("Снапшот '{snapshot_id}' не найден в каталоге"))?;

    for object in &summary.objects {
        let _ = client::delete_object(&cfg, &object.key).await;
    }
    let _ = client::delete_object(
        &cfg,
        &manifest_key(&summary.source.instance_id, snapshot_id),
    )
    .await;

    catalog
        .snapshots
        .retain(|snapshot| snapshot.snapshot_id != snapshot_id);
    write_catalog(&cfg, &catalog).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use contracts::system::datasets::{InstanceEnv, SourceInfo};

    fn summary(instance_id: &str, snapshot_id: &str, created_at: &str) -> SnapshotSummary {
        SnapshotSummary {
            snapshot_id: snapshot_id.to_string(),
            created_at: created_at.to_string(),
            created_by: "user:1".to_string(),
            note: None,
            source: SourceInfo {
                instance_id: instance_id.to_string(),
                instance_label: instance_id.to_string(),
                instance_env: InstanceEnv::Unknown,
                hostname: "host".to_string(),
                app_version: "0.1.0".to_string(),
                git_commit: "abc".to_string(),
                build_profile: "debug".to_string(),
                schema_version: 1,
                os: "windows".to_string(),
                data_root: String::new(),
                database_path: String::new(),
                config_path: String::new(),
                captured_at: created_at.to_string(),
            },
            sets: Vec::new(),
            objects: Vec::new(),
        }
    }

    fn catalog_with(entries: &[(&str, &str, &str)]) -> DatasetCatalog {
        DatasetCatalog {
            format_version: DATASET_CATALOG_FORMAT_VERSION,
            snapshots: entries
                .iter()
                .map(|(instance, id, at)| summary(instance, id, at))
                .collect(),
        }
    }

    #[test]
    fn rotation_keeps_the_newest_n_of_its_own_instance() {
        let catalog = catalog_with(&[
            ("dev-a", "s3", "2026-08-03T00:00:00Z"),
            ("dev-a", "s1", "2026-08-01T00:00:00Z"),
            ("dev-a", "s2", "2026-08-02T00:00:00Z"),
        ]);
        let rotated = plan_rotation(&catalog, "dev-a", 2);
        assert_eq!(rotated.len(), 1);
        assert_eq!(rotated[0].snapshot_id, "s1");
    }

    #[test]
    fn rotation_never_touches_other_instances() {
        let catalog = catalog_with(&[
            ("dev-b", "b1", "2026-08-01T00:00:00Z"),
            ("dev-b", "b2", "2026-08-02T00:00:00Z"),
            ("dev-b", "b3", "2026-08-03T00:00:00Z"),
            ("dev-a", "a1", "2026-08-04T00:00:00Z"),
        ]);
        // У dev-a всего один снапшот — удалять нечего, чужие три не в счёт.
        assert!(plan_rotation(&catalog, "dev-a", 1).is_empty());
    }

    #[test]
    fn rotation_disabled_when_keep_is_zero() {
        let catalog = catalog_with(&[("dev-a", "a1", "2026-08-01T00:00:00Z")]);
        assert!(plan_rotation(&catalog, "dev-a", 0).is_empty());
    }

    #[test]
    fn object_keys_are_scoped_by_instance_and_snapshot() {
        assert_eq!(
            bundle_key("dev-a", "20260810T120000Z-abcdef12"),
            "datasets/dev-a/20260810T120000Z-abcdef12/bundle.zip"
        );
        assert_eq!(
            manifest_key("dev-a", "20260810T120000Z-abcdef12"),
            "datasets/dev-a/20260810T120000Z-abcdef12/manifest.json"
        );
    }
}
