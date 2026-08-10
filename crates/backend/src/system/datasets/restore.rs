//! Восстановление наборов из снапшота.
//!
//! Схема применения повторяет `quality::registry::upsert_authoring_bundle`,
//! поднятую с уровня одного пакета на уровень каталога набора: содержимое
//! собирается в соседнем staging-каталоге, затем два `rename` меняют его
//! местами с текущим, и только после успешной перезагрузки кэшей старая версия
//! удаляется. Любая ошибка — хоть на файловой операции, хоть на reload —
//! возвращает все затронутые наборы в исходное состояние.
//!
//! Staging создаётся именно рядом с целевым каталогом, а не во временной папке:
//! `rename` не работает через границы томов, а при legacy-override наборы могут
//! лежать на разных дисках.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use bytes::Bytes;
use contracts::system::datasets::{
    BundleManifest, DiffClass, FileDiffEntry, InstanceInfoSummary, RestoreApplyRequest,
    RestoreMode, RestorePreviewDto, RestorePreviewRequest, RestoreResultDto, RestoreSetResult,
    RestoreWarning, SetDiff, WarningSeverity, DATASET_BUNDLE_FORMAT_VERSION,
};

use super::registry::{self, DatasetDescriptor, HotReload};
use super::repository::{
    self, TransferLogEntry, OPERATION_RESTORE, STATUS_FAILED, STATUS_OK, STATUS_ROLLED_BACK,
};
use super::scan;
use super::{bundle, publish, ActorInfo};
use crate::shared::config::{self, Config};
use crate::system::s3::service as s3_service;

/// Сколько записей дерева отдавать в предпросмотр. Полное дерево может быть в
/// десятки тысяч файлов — в диалоге его всё равно не прочитать.
const DIFF_ENTRIES_LIMIT: usize = 200;

// ---------------------------------------------------------------------------
// Предпросмотр
// ---------------------------------------------------------------------------

pub async fn preview(request: RestorePreviewRequest) -> anyhow::Result<RestorePreviewDto> {
    let config = config::load_config()?;
    let cfg = s3_service::s3_config()?;
    let catalog = publish::get_catalog().await?;
    let summary = catalog
        .find(&request.snapshot_id)
        .ok_or_else(|| anyhow::anyhow!("Снапшот '{}' не найден", request.snapshot_id))?
        .clone();

    // Манифест лежит отдельным объектом — дифф считается без скачивания бандла.
    let manifest = publish::get_manifest(&request.snapshot_id).await?;
    let _ = &cfg;

    let bundle_sha256 = manifest
        .objects
        .first()
        .map(|object| object.sha256.clone())
        .unwrap_or_default();
    let bundle_sha256 = if bundle_sha256.is_empty() {
        summary
            .objects
            .first()
            .map(|object| object.sha256.clone())
            .unwrap_or_default()
    } else {
        bundle_sha256
    };

    let mut sets = Vec::new();
    let mut warnings = collect_warnings(&config, &manifest, &request.set_ids).await;

    for set_id in &request.set_ids {
        let Some(set_manifest) = manifest.set(set_id) else {
            warnings.push(RestoreWarning {
                code: "set_missing_in_snapshot".to_string(),
                severity: WarningSeverity::Warning,
                message: format!("В снапшоте нет набора '{set_id}' — он будет пропущен."),
            });
            continue;
        };
        let Some(descriptor) = registry::lookup(set_id) else {
            warnings.push(RestoreWarning {
                code: "unknown_set".to_string(),
                severity: WarningSeverity::Warning,
                message: format!(
                    "Набор '{set_id}' неизвестен этой версии приложения — он будет пропущен."
                ),
            });
            continue;
        };
        if !descriptor.supported_modes.contains(&request.mode) {
            warnings.push(RestoreWarning {
                code: "mode_unsupported".to_string(),
                severity: WarningSeverity::Blocking,
                message: format!(
                    "Набор «{}» не поддерживает режим «{}».",
                    descriptor.label_ru,
                    request.mode.label_ru()
                ),
            });
            continue;
        }

        sets.push(diff_set(&config, descriptor, set_manifest, request.mode)?);
    }

    let blocking = warnings
        .iter()
        .any(|warning| warning.severity == WarningSeverity::Blocking);

    Ok(RestorePreviewDto {
        snapshot_id: request.snapshot_id,
        mode: request.mode,
        source: InstanceInfoSummary {
            instance_id: manifest.source.instance_id.clone(),
            instance_label: manifest.source.instance_label.clone(),
            instance_env: manifest.source.instance_env,
            hostname: manifest.source.hostname.clone(),
            app_version: manifest.source.app_version.clone(),
            git_commit: manifest.source.git_commit.clone(),
            schema_version: manifest.source.schema_version,
            captured_at: manifest.source.captured_at.clone(),
        },
        sets,
        warnings,
        blocking,
        bundle_sha256,
    })
}

async fn collect_warnings(
    config: &Config,
    manifest: &BundleManifest,
    set_ids: &[String],
) -> Vec<RestoreWarning> {
    let mut warnings = Vec::new();

    if manifest.format_version > DATASET_BUNDLE_FORMAT_VERSION {
        warnings.push(RestoreWarning {
            code: "format_too_new".to_string(),
            severity: WarningSeverity::Blocking,
            message: format!(
                "Снапшот записан в формате версии {}, приложение понимает {}. \
                 Обновите приложение на этом инстансе.",
                manifest.format_version, DATASET_BUNDLE_FORMAT_VERSION
            ),
        });
    }

    let local = super::source_info(config).await;
    if manifest.source.schema_version != local.schema_version {
        warnings.push(RestoreWarning {
            code: "schema_mismatch".to_string(),
            // Для файловых наборов это не блокер: база знаний и навыки не
            // завязаны на схему БД. Для набора «База данных» (фаза 2) то же
            // расхождение станет блокирующим.
            severity: WarningSeverity::Warning,
            message: format!(
                "Версия схемы БД источника ({}) отличается от локальной ({}).",
                manifest.source.schema_version, local.schema_version
            ),
        });
    }
    if manifest.source.instance_id == local.instance_id {
        warnings.push(RestoreWarning {
            code: "same_instance".to_string(),
            severity: WarningSeverity::Info,
            message: "Это снапшот текущего инстанса — восстановление откатит данные \
                      к состоянию на момент снятия."
                .to_string(),
        });
    }
    if manifest.source.hostname != local.hostname
        && manifest.source.instance_id == local.instance_id
    {
        warnings.push(RestoreWarning {
            code: "instance_id_collision".to_string(),
            severity: WarningSeverity::Warning,
            message: format!(
                "Совпадает идентификатор инстанса ('{}'), но машины разные \
                 (снапшот с «{}», локально «{}»). Похоже, config.toml скопировали \
                 без правки [instance].id.",
                local.instance_id, manifest.source.hostname, local.hostname
            ),
        });
    }
    if set_ids.is_empty() {
        warnings.push(RestoreWarning {
            code: "no_sets_selected".to_string(),
            severity: WarningSeverity::Blocking,
            message: "Не выбрано ни одного набора.".to_string(),
        });
    }

    warnings
}

fn diff_set(
    config: &Config,
    descriptor: &DatasetDescriptor,
    set_manifest: &contracts::system::datasets::SetManifest,
    mode: RestoreMode,
) -> anyhow::Result<SetDiff> {
    let root = registry::resolve_root(config, descriptor)
        .ok_or_else(|| anyhow::anyhow!("Для набора '{}' нет пути", descriptor.id))?;
    let local = scan::scan_directory(descriptor, &root.path)?;
    let local_index = local.index();

    let bundle_index: BTreeMap<&str, &contracts::system::datasets::FileEntry> = set_manifest
        .files
        .iter()
        .map(|file| (file.path.as_str(), file))
        .collect();

    let mut entries = Vec::new();
    let mut added = 0_u64;
    let mut modified = 0_u64;
    let mut identical = 0_u64;
    let mut local_only = 0_u64;
    let mut bytes_written = 0_u64;
    let mut bytes_deleted = 0_u64;

    for (path, file) in &bundle_index {
        match local_index.get(*path) {
            None => {
                added += 1;
                bytes_written += file.size_bytes;
                entries.push(FileDiffEntry {
                    path: (*path).to_string(),
                    class: DiffClass::Added,
                    bundle_size_bytes: Some(file.size_bytes),
                    local_size_bytes: None,
                    bundle_modified_at: file.modified_at.clone(),
                    local_modified_at: None,
                });
            }
            // Сравнение по содержимому: mtime после копирования каталогов ничего
            // не говорит, а размер совпадает у слишком многих правок.
            Some((local_size, local_hash)) if *local_hash == file.sha256 => {
                identical += 1;
                entries.push(FileDiffEntry {
                    path: (*path).to_string(),
                    class: DiffClass::Identical,
                    bundle_size_bytes: Some(file.size_bytes),
                    local_size_bytes: Some(*local_size),
                    bundle_modified_at: file.modified_at.clone(),
                    local_modified_at: None,
                });
            }
            Some((local_size, _)) => {
                modified += 1;
                bytes_written += file.size_bytes;
                entries.push(FileDiffEntry {
                    path: (*path).to_string(),
                    class: DiffClass::Modified,
                    bundle_size_bytes: Some(file.size_bytes),
                    local_size_bytes: Some(*local_size),
                    bundle_modified_at: file.modified_at.clone(),
                    local_modified_at: None,
                });
            }
        }
    }

    for (path, (size, _)) in &local_index {
        if bundle_index.contains_key(path.as_str()) {
            continue;
        }
        local_only += 1;
        if mode == RestoreMode::Replace {
            bytes_deleted += size;
        }
        entries.push(FileDiffEntry {
            path: path.clone(),
            class: DiffClass::LocalOnly,
            bundle_size_bytes: None,
            local_size_bytes: Some(*size),
            bundle_modified_at: None,
            local_modified_at: None,
        });
    }

    entries.sort_by(|left, right| {
        diff_order(left.class)
            .cmp(&diff_order(right.class))
            .then_with(|| left.path.cmp(&right.path))
    });
    let entries_truncated = entries.len() > DIFF_ENTRIES_LIMIT;
    entries.truncate(DIFF_ENTRIES_LIMIT);

    Ok(SetDiff {
        set_id: descriptor.id.to_string(),
        label_ru: descriptor.label_ru.to_string(),
        local_path: root.display_path(),
        local_exists: local.existed,
        added,
        modified,
        identical,
        local_only,
        bytes_written,
        bytes_deleted,
        entries,
        entries_truncated,
    })
}

/// Порядок в диалоге: сначала то, что меняется, «без изменений» — в конце.
fn diff_order(class: DiffClass) -> u8 {
    match class {
        DiffClass::Modified => 0,
        DiffClass::Added => 1,
        DiffClass::LocalOnly => 2,
        DiffClass::Identical => 3,
    }
}

// ---------------------------------------------------------------------------
// Применение
// ---------------------------------------------------------------------------

pub async fn apply(
    request: RestoreApplyRequest,
    actor: ActorInfo,
) -> anyhow::Result<RestoreResultDto> {
    let cfg = s3_service::s3_config()?;
    let catalog = publish::get_catalog().await?;
    let summary = catalog
        .find(&request.snapshot_id)
        .ok_or_else(|| anyhow::anyhow!("Снапшот '{}' не найден", request.snapshot_id))?
        .clone();

    let bytes = publish::fetch_verified_bundle(
        &cfg,
        &summary,
        request.expected_bundle_sha256.as_deref(),
    )
    .await?;

    apply_from_bytes(bytes, request, actor).await
}

/// Применение из готовых байтов архива — общий путь для восстановления из S3 и
/// офлайн-импорта загруженного файла (в том числе автоснапшота pre-restore).
pub async fn apply_from_bytes(
    bytes: Bytes,
    request: RestoreApplyRequest,
    actor: ActorInfo,
) -> anyhow::Result<RestoreResultDto> {
    let config = config::load_config()?;
    let identity = config::instance_identity(&config);
    let parsed = bundle::read_bundle(&bytes)?;

    if parsed.manifest.format_version > DATASET_BUNDLE_FORMAT_VERSION {
        anyhow::bail!(
            "Снапшот записан в формате версии {}, приложение понимает {}",
            parsed.manifest.format_version,
            DATASET_BUNDLE_FORMAT_VERSION
        );
    }

    let mut targets = Vec::new();
    for set_id in &request.set_ids {
        let Some(descriptor) = registry::lookup(set_id) else {
            continue;
        };
        if !descriptor.available || !descriptor.supported_modes.contains(&request.mode) {
            continue;
        }
        if parsed.manifest.set(set_id).is_none() {
            continue;
        }
        let root = registry::resolve_root(&config, descriptor)
            .ok_or_else(|| anyhow::anyhow!("Для набора '{set_id}' нет пути"))?;
        targets.push((descriptor, root.path));
    }
    if targets.is_empty() {
        anyhow::bail!("Ни один из выбранных наборов не может быть восстановлен");
    }

    // Автоснапшот текущего состояния — единственный путь назад, если replace
    // удалит нужное. Формат совпадает с обычным бандлом, поэтому откат делается
    // штатным восстановлением из файла.
    let pre_restore = match snapshot_before_restore(&config, &targets).await {
        Ok(path) => Some(path),
        Err(error) => {
            tracing::warn!("datasets: не удалось снять автоснапшот перед восстановлением: {error}");
            return Err(anyhow::anyhow!(
                "Не удалось снять резервную копию текущего состояния, восстановление отменено: {error}"
            ));
        }
    };

    let outcome = apply_targets(&parsed, &targets, request.mode, &actor).await;

    let source_instance = parsed.manifest.source.instance_id.clone();
    match outcome {
        Ok((sets, warnings)) => {
            let _ = repository::insert(&TransferLogEntry {
                operation: OPERATION_RESTORE,
                snapshot_id: request.snapshot_id.clone(),
                instance_id: identity.id.clone(),
                source_instance_id: Some(source_instance),
                set_ids: request.set_ids.clone(),
                mode: Some(request.mode.as_str().to_string()),
                status: STATUS_OK,
                bytes: bytes.len() as i64,
                files_written: sets.iter().map(|set| set.files_written as i64).sum(),
                files_deleted: sets.iter().map(|set| set.files_deleted as i64).sum(),
                pre_restore_archive: pre_restore
                    .as_ref()
                    .map(|path| path.display().to_string()),
                manifest_json: serde_json::to_string(&parsed.manifest.source).ok(),
                error: None,
                actor_user_id: actor.user_id.clone(),
            })
            .await;

            Ok(RestoreResultDto {
                snapshot_id: request.snapshot_id,
                mode: request.mode,
                sets,
                pre_restore_archive: pre_restore.map(|path| path.display().to_string()),
                // Файловые наборы применяются на живом приложении: кэши
                // перезагружаются здесь же. Перезапуск понадобится только
                // восстановлению БД (фаза 2).
                requires_restart: false,
                warnings,
            })
        }
        Err((error, rolled_back)) => {
            let _ = repository::insert(&TransferLogEntry {
                operation: OPERATION_RESTORE,
                snapshot_id: request.snapshot_id.clone(),
                instance_id: identity.id,
                source_instance_id: Some(source_instance),
                set_ids: request.set_ids.clone(),
                mode: Some(request.mode.as_str().to_string()),
                status: if rolled_back {
                    STATUS_ROLLED_BACK
                } else {
                    STATUS_FAILED
                },
                bytes: 0,
                files_written: 0,
                files_deleted: 0,
                pre_restore_archive: pre_restore
                    .as_ref()
                    .map(|path| path.display().to_string()),
                manifest_json: None,
                error: Some(error.to_string()),
                actor_user_id: actor.user_id,
            })
            .await;
            Err(error)
        }
    }
}

async fn snapshot_before_restore(
    config: &Config,
    targets: &[(&'static DatasetDescriptor, PathBuf)],
) -> anyhow::Result<PathBuf> {
    let set_ids: Vec<String> = targets
        .iter()
        .map(|(descriptor, _)| descriptor.id.to_string())
        .collect();
    let snapshot_id = super::new_snapshot_id();
    let (_, bytes) = publish::build_snapshot(
        config,
        &snapshot_id,
        &set_ids,
        Some("Автоснапшот перед восстановлением".to_string()),
        &ActorInfo::system("pre-restore"),
        format!("local/pre-restore-{snapshot_id}.zip"),
    )
    .await?;

    let dir = config::get_backups_path(config);
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(format!("pre-restore-{snapshot_id}.zip"));
    std::fs::write(&path, &bytes)?;
    rotate_local_archives(&dir);
    Ok(path)
}

/// Локальная ротация автоснапшотов: держим последние 10.
fn rotate_local_archives(dir: &Path) {
    const KEEP: usize = 10;
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut archives: Vec<PathBuf> = entries
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("pre-restore-") && name.ends_with(".zip"))
        })
        .collect();
    if archives.len() <= KEEP {
        return;
    }
    // Имя начинается с временной метки — лексикографическая сортировка
    // совпадает с хронологической.
    archives.sort();
    for path in archives.iter().take(archives.len() - KEEP) {
        let _ = std::fs::remove_file(path);
    }
}

/// Результат применения: либо наборы и предупреждения, либо ошибка вместе с
/// признаком «состояние на диске возвращено к исходному».
type ApplyOutcome = Result<(Vec<RestoreSetResult>, Vec<String>), (anyhow::Error, bool)>;

async fn apply_targets(
    parsed: &bundle::ParsedBundle,
    targets: &[(&'static DatasetDescriptor, PathBuf)],
    mode: RestoreMode,
    actor: &ActorInfo,
) -> ApplyOutcome {
    let nonce = uuid::Uuid::new_v4();
    // Каждый успешный swap кладёт сюда пару «цель → бэкап», чтобы откат вернул
    // ВСЕ наборы, а не только тот, на котором произошла ошибка.
    let mut swapped: Vec<(PathBuf, Option<PathBuf>)> = Vec::new();
    let mut results = Vec::new();
    let mut warnings = Vec::new();

    for (descriptor, target) in targets {
        let empty = bundle::BundleSetFiles {
            files: BTreeMap::new(),
        };
        let files = parsed
            .set(descriptor.id)
            .map(|set| &set.files)
            .unwrap_or(&empty.files);

        match stage_and_swap(descriptor, target, files, mode, nonce) {
            Ok((backup, written, deleted, bytes)) => {
                swapped.push((target.clone(), backup));
                results.push(RestoreSetResult {
                    set_id: descriptor.id.to_string(),
                    label_ru: descriptor.label_ru.to_string(),
                    files_written: written,
                    files_deleted: deleted,
                    bytes_written: bytes,
                    reload_message: None,
                });
            }
            Err(error) => {
                rollback(&swapped);
                return Err((
                    anyhow::anyhow!(
                        "Набор «{}»: {error}. Изменения отменены, данные вернулись к исходным.",
                        descriptor.label_ru
                    ),
                    true,
                ));
            }
        }
    }

    // Кэши перезагружаем только после того, как ВСЕ наборы легли на место:
    // иначе реестр качества увидел бы половину обновления.
    for (descriptor, _) in targets {
        match reload_cache(descriptor, actor).await {
            Ok(message) => {
                if let Some(result) = results
                    .iter_mut()
                    .find(|result| result.set_id == descriptor.id)
                {
                    result.reload_message = message;
                }
            }
            Err(error) => {
                rollback(&swapped);
                // Данные вернулись — но кэши уже могли частично перезагрузиться
                // с новым содержимым, поэтому возвращаем их к состоянию на диске.
                for (descriptor, _) in targets {
                    let _ = reload_cache(descriptor, actor).await;
                }
                return Err((
                    anyhow::anyhow!(
                        "Набор «{}» не прошёл перезагрузку: {error}. \
                         Изменения отменены, данные вернулись к исходным.",
                        descriptor.label_ru
                    ),
                    true,
                ));
            }
        }
    }

    // Всё удалось — убираем бэкапы.
    for (_, backup) in &swapped {
        if let Some(backup) = backup {
            let _ = std::fs::remove_dir_all(backup);
        }
    }

    for (descriptor, _) in targets {
        if parsed.manifest.set(descriptor.id).is_none() {
            warnings.push(format!(
                "Набора «{}» не было в снапшоте — он не изменялся.",
                descriptor.label_ru
            ));
        }
    }

    Ok((results, warnings))
}

/// Наполняет staging и меняет его местами с целевым каталогом.
///
/// Возвращает путь к бэкапу (если целевой каталог существовал) и счётчики.
fn stage_and_swap(
    descriptor: &DatasetDescriptor,
    target: &Path,
    files: &BTreeMap<String, Vec<u8>>,
    mode: RestoreMode,
    nonce: uuid::Uuid,
) -> anyhow::Result<(Option<PathBuf>, u64, u64, u64)> {
    let parent = target
        .parent()
        .ok_or_else(|| anyhow::anyhow!("у каталога набора нет родителя: {}", target.display()))?;
    std::fs::create_dir_all(parent)?;

    let stage = parent.join(format!(".{}.{nonce}.tmp", descriptor.id));
    let backup = parent.join(format!(".{}.{nonce}.bak", descriptor.id));
    let _ = std::fs::remove_dir_all(&stage);
    std::fs::create_dir(&stage)?;

    let build = (|| -> anyhow::Result<(u64, u64, u64)> {
        let had_target = target.is_dir();
        let mut deleted = 0_u64;

        if mode == RestoreMode::Merge && had_target {
            // merge начинается с копии текущего состояния: всё, чего нет в
            // бандле, должно уцелеть.
            copy_tree(target, &stage)?;
        } else if mode == RestoreMode::Replace && had_target {
            // replace обязан сохранить исключённое: .obsidian и прочее рабочее
            // окружение никогда не попадало в снапшот, и удалять его нельзя.
            deleted = copy_excluded_and_count_removed(descriptor, target, &stage, files)?;
        }

        let mut written = 0_u64;
        let mut bytes = 0_u64;
        for (relative, content) in files {
            bundle::validate_relative_path(relative)
                .map_err(|error| anyhow::anyhow!("небезопасный путь '{relative}': {error}"))?;
            let path = stage.join(relative);
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir)?;
            }
            std::fs::write(&path, content)?;
            written += 1;
            bytes += content.len() as u64;
        }
        Ok((written, deleted, bytes))
    })();

    let (written, deleted, bytes) = match build {
        Ok(counters) => counters,
        Err(error) => {
            let _ = std::fs::remove_dir_all(&stage);
            return Err(error);
        }
    };

    let had_target = target.is_dir();
    if had_target {
        std::fs::rename(target, &backup).map_err(|error| {
            let _ = std::fs::remove_dir_all(&stage);
            anyhow::anyhow!(
                "не удалось освободить каталог {} ({error}). \
                 Возможно, файлы открыты другой программой.",
                target.display()
            )
        })?;
    }
    if let Err(error) = std::fs::rename(&stage, target) {
        if had_target {
            let _ = std::fs::rename(&backup, target);
        }
        let _ = std::fs::remove_dir_all(&stage);
        return Err(anyhow::anyhow!(
            "не удалось активировать каталог {}: {error}",
            target.display()
        ));
    }

    Ok((
        had_target.then_some(backup),
        written,
        deleted,
        bytes,
    ))
}

/// Возвращает наборы к состоянию до применения.
fn rollback(swapped: &[(PathBuf, Option<PathBuf>)]) {
    for (target, backup) in swapped {
        match backup {
            Some(backup) => {
                let _ = std::fs::remove_dir_all(target);
                if let Err(error) = std::fs::rename(backup, target) {
                    tracing::error!(
                        "datasets: не удалось вернуть {} из резервной копии {}: {error}",
                        target.display(),
                        backup.display()
                    );
                }
            }
            // Каталога не было до восстановления — убираем созданный.
            None => {
                let _ = std::fs::remove_dir_all(target);
            }
        }
    }
}

fn copy_tree(source: &Path, destination: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(destination)?;
    for entry in std::fs::read_dir(source)? {
        let entry = entry?;
        let from = entry.path();
        let to = destination.join(entry.file_name());
        if from.is_dir() {
            copy_tree(&from, &to)?;
        } else if from.is_file() {
            std::fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

/// Для `replace`: переносит в staging только исключённые из набора пути и
/// считает, сколько локальных файлов будет удалено.
fn copy_excluded_and_count_removed(
    descriptor: &DatasetDescriptor,
    target: &Path,
    stage: &Path,
    files: &BTreeMap<String, Vec<u8>>,
) -> anyhow::Result<u64> {
    let mut removed = 0_u64;
    walk_for_replace(descriptor, target, target, stage, files, &mut removed)?;
    Ok(removed)
}

fn walk_for_replace(
    descriptor: &DatasetDescriptor,
    root: &Path,
    dir: &Path,
    stage: &Path,
    files: &BTreeMap<String, Vec<u8>>,
    removed: &mut u64,
) -> anyhow::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let relative = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .components()
            .map(|component| component.as_os_str().to_string_lossy().to_string())
            .collect::<Vec<_>>()
            .join("/");
        if relative.is_empty() {
            continue;
        }

        if registry::is_excluded(descriptor, &relative) {
            let destination = stage.join(&relative);
            if path.is_dir() {
                copy_tree(&path, &destination)?;
            } else if path.is_file() {
                if let Some(parent) = destination.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::copy(&path, &destination)?;
            }
            continue;
        }

        if path.is_dir() {
            walk_for_replace(descriptor, root, &path, stage, files, removed)?;
        } else if path.is_file() && !files.contains_key(&relative) {
            *removed += 1;
        }
    }
    Ok(())
}

async fn reload_cache(
    descriptor: &DatasetDescriptor,
    actor: &ActorInfo,
) -> anyhow::Result<Option<String>> {
    match descriptor.hot_reload {
        HotReload::None => Ok(None),
        HotReload::KnowledgeBase => {
            crate::shared::llm::knowledge_base::reload_knowledge_base()?;
            Ok(Some("База знаний перечитана".to_string()))
        }
        HotReload::Skills => {
            let report = crate::shared::llm::skills::reload(actor.user_id.clone()).await;
            // reload отдаёт JSON; провал распознаём по явному признаку ошибки.
            if report
                .get("ok")
                .and_then(|value| value.as_bool())
                .is_some_and(|ok| !ok)
            {
                anyhow::bail!("реестр навыков не принял каталог: {report}");
            }
            if let Some(error) = report.get("error").and_then(|value| value.as_str()) {
                anyhow::bail!("реестр навыков не принял каталог: {error}");
            }
            Ok(Some(format!(
                "Навыки перечитаны: {}",
                report
                    .get("total")
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "готово".to_string())
            )))
        }
        HotReload::QualityChecks => {
            let report = crate::quality::registry::reload().await;
            if !report.ok {
                anyhow::bail!("реестр проверок отверг каталог: {}", report.diagnostics.join("; "));
            }
            Ok(Some("Проверки качества перечитаны".to_string()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::system::datasets::registry::lookup;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("mpi-restore-{name}-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn files(pairs: &[(&str, &str)]) -> BTreeMap<String, Vec<u8>> {
        pairs
            .iter()
            .map(|(path, content)| (path.to_string(), content.as_bytes().to_vec()))
            .collect()
    }

    fn read(root: &Path, relative: &str) -> Option<String> {
        std::fs::read_to_string(root.join(relative)).ok()
    }

    #[test]
    fn merge_keeps_local_only_files() {
        let parent = temp_dir("merge");
        let target = parent.join("knowledge");
        std::fs::create_dir_all(&target).unwrap();
        std::fs::write(target.join("mine.md"), "local").unwrap();
        std::fs::write(target.join("shared.md"), "old").unwrap();

        let descriptor = lookup("knowledge").unwrap();
        let incoming = files(&[("shared.md", "new"), ("fresh.md", "added")]);
        let (backup, written, deleted, _) = stage_and_swap(
            descriptor,
            &target,
            &incoming,
            RestoreMode::Merge,
            uuid::Uuid::new_v4(),
        )
        .unwrap();

        assert_eq!(written, 2);
        assert_eq!(deleted, 0);
        assert_eq!(read(&target, "mine.md").as_deref(), Some("local"));
        assert_eq!(read(&target, "shared.md").as_deref(), Some("new"));
        assert_eq!(read(&target, "fresh.md").as_deref(), Some("added"));

        if let Some(backup) = backup {
            std::fs::remove_dir_all(backup).ok();
        }
        std::fs::remove_dir_all(&parent).ok();
    }

    #[test]
    fn replace_drops_local_only_files_but_keeps_excluded() {
        let parent = temp_dir("replace");
        let target = parent.join("knowledge");
        std::fs::create_dir_all(target.join(".obsidian")).unwrap();
        std::fs::write(target.join("mine.md"), "local").unwrap();
        std::fs::write(target.join("shared.md"), "old").unwrap();
        std::fs::write(target.join(".obsidian/workspace.json"), "{}").unwrap();

        let descriptor = lookup("knowledge").unwrap();
        let incoming = files(&[("shared.md", "new")]);
        let (backup, written, deleted, _) = stage_and_swap(
            descriptor,
            &target,
            &incoming,
            RestoreMode::Replace,
            uuid::Uuid::new_v4(),
        )
        .unwrap();

        assert_eq!(written, 1);
        assert_eq!(deleted, 1, "mine.md должен быть посчитан как удаляемый");
        assert_eq!(read(&target, "shared.md").as_deref(), Some("new"));
        assert!(read(&target, "mine.md").is_none());
        // Рабочее окружение Obsidian не относится к данным и пережило replace.
        assert_eq!(read(&target, ".obsidian/workspace.json").as_deref(), Some("{}"));

        if let Some(backup) = backup {
            std::fs::remove_dir_all(backup).ok();
        }
        std::fs::remove_dir_all(&parent).ok();
    }

    #[test]
    fn rollback_restores_the_previous_directory() {
        let parent = temp_dir("rollback");
        let target = parent.join("knowledge");
        std::fs::create_dir_all(&target).unwrap();
        std::fs::write(target.join("original.md"), "before").unwrap();

        let descriptor = lookup("knowledge").unwrap();
        let incoming = files(&[("replacement.md", "after")]);
        let (backup, _, _, _) = stage_and_swap(
            descriptor,
            &target,
            &incoming,
            RestoreMode::Replace,
            uuid::Uuid::new_v4(),
        )
        .unwrap();
        assert!(read(&target, "replacement.md").is_some());

        rollback(&[(target.clone(), backup)]);

        assert_eq!(read(&target, "original.md").as_deref(), Some("before"));
        assert!(read(&target, "replacement.md").is_none());

        std::fs::remove_dir_all(&parent).ok();
    }

    #[test]
    fn restoring_into_a_missing_directory_creates_it_and_rollback_removes_it() {
        let parent = temp_dir("fresh");
        let target = parent.join("golden_set");

        let descriptor = lookup("golden_set").unwrap();
        let incoming = files(&[("case-01.md", "question")]);
        let (backup, written, _, _) = stage_and_swap(
            descriptor,
            &target,
            &incoming,
            RestoreMode::Merge,
            uuid::Uuid::new_v4(),
        )
        .unwrap();

        assert_eq!(written, 1);
        assert!(backup.is_none(), "нечего резервировать: каталога не было");
        assert!(target.is_dir());

        rollback(&[(target.clone(), None)]);
        assert!(!target.exists(), "откат должен убрать созданный каталог");

        std::fs::remove_dir_all(&parent).ok();
    }

    #[test]
    fn unsafe_paths_from_the_bundle_are_refused() {
        let parent = temp_dir("unsafe");
        let target = parent.join("knowledge");
        std::fs::create_dir_all(&target).unwrap();

        let descriptor = lookup("knowledge").unwrap();
        let incoming = files(&[("../escape.md", "nope")]);
        let result = stage_and_swap(
            descriptor,
            &target,
            &incoming,
            RestoreMode::Merge,
            uuid::Uuid::new_v4(),
        );

        assert!(result.is_err());
        assert!(!parent.join("escape.md").exists());

        std::fs::remove_dir_all(&parent).ok();
    }
}
