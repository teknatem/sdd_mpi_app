//! Локальное состояние: где лежат наборы, сколько в них файлов и что в раскладке
//! выглядит подозрительно.
//!
//! Аномалии — это именно наблюдения, а не автоматические действия. Подсистема
//! никогда не удаляет и не переносит то, что ей не принадлежит: осиротевший
//! каталог может оказаться единственной копией чьих-то данных.

use contracts::system::datasets::{
    AnomalySeverity, DatasetAnomalyDto, DatasetStatusEntry, DatasetsStatusDto, InstanceInfoDto,
    PathOrigin,
};

use super::registry::{self, DATASETS};
use super::scan;
use crate::shared::config::{self, Config};

/// Историческое имя каталога чатов. После переезда на `chats` каталог с этим
/// именем остаётся на диске, но приложение в него больше не смотрит.
const LEGACY_CHATS_DIRNAME: &str = "chat_files";

/// Историческое расположение вложений — относительно рабочего каталога процесса.
const LEGACY_ATTACHMENTS_PATH: &str = "uploads/chat_attachments";

pub async fn local_status() -> anyhow::Result<DatasetsStatusDto> {
    let config = config::load_config()?;
    let instance = instance_info(&config).await;
    let sets = scan_all(&config);
    let anomalies = detect_anomalies(&config, &sets);

    Ok(DatasetsStatusDto {
        instance,
        sets,
        anomalies,
    })
}

async fn instance_info(config: &Config) -> InstanceInfoDto {
    let source = super::source_info(config).await;
    let s3_error = config.s3.validate_ready().err().map(|error| error.to_string());

    InstanceInfoDto {
        instance_id: source.instance_id,
        instance_label: source.instance_label,
        instance_env: source.instance_env,
        hostname: source.hostname,
        app_version: source.app_version,
        git_commit: source.git_commit,
        build_profile: source.build_profile,
        schema_version: source.schema_version,
        os: source.os,
        data_root: config::get_data_root(config).map(|root| root.display().to_string()),
        database_path: source.database_path,
        config_path: source.config_path,
        s3_ready: s3_error.is_none(),
        s3_bucket: Some(config.s3.bucket.clone()).filter(|bucket| !bucket.is_empty()),
        s3_endpoint: Some(config.s3.endpoint.clone()).filter(|endpoint| !endpoint.is_empty()),
        s3_error,
    }
}

/// Сканирует все наборы реестра. `config` читается один раз и передаётся вниз:
/// `load_config()` каждый вызов заново читает и парсит файл с диска.
pub fn scan_all(config: &Config) -> Vec<DatasetStatusEntry> {
    DATASETS
        .iter()
        .map(|descriptor| {
            let Some(root) = registry::resolve_root(config, descriptor) else {
                // Набор без файлового корня — сейчас это только БД.
                let database = config::resolve_database_path(config).ok();
                return DatasetStatusEntry {
                    set_id: descriptor.id.to_string(),
                    label_ru: descriptor.label_ru.to_string(),
                    description_ru: descriptor.description_ru.to_string(),
                    kind: descriptor.kind,
                    path: database
                        .as_ref()
                        .map(|resolved| resolved.display_path())
                        .unwrap_or_default(),
                    path_origin: database
                        .as_ref()
                        .map(|resolved| resolved.origin)
                        .unwrap_or(contracts::system::datasets::PathOrigin::ExeRelative),
                    exists: false,
                    file_count: 0,
                    total_bytes: 0,
                    skipped_count: 0,
                    selected_by_default: descriptor.selected_by_default,
                    supported_modes: descriptor.supported_modes.to_vec(),
                    available: descriptor.available,
                    unavailable_reason: descriptor
                        .unavailable_reason
                        .map(|reason| reason.to_string()),
                    db_stats: None,
                    error: None,
                };
            };

            let (scan, error) = match scan::scan_directory(descriptor, &root.path) {
                Ok(scan) => (Some(scan), None),
                Err(error) => (None, Some(error.to_string())),
            };

            DatasetStatusEntry {
                set_id: descriptor.id.to_string(),
                label_ru: descriptor.label_ru.to_string(),
                description_ru: descriptor.description_ru.to_string(),
                kind: descriptor.kind,
                path: root.display_path(),
                path_origin: root.origin,
                exists: scan.as_ref().map(|scan| scan.existed).unwrap_or(false),
                file_count: scan.as_ref().map(|scan| scan.file_count()).unwrap_or(0),
                total_bytes: scan.as_ref().map(|scan| scan.total_bytes).unwrap_or(0),
                skipped_count: scan
                    .as_ref()
                    .map(|scan| scan.skipped.len() as u64)
                    .unwrap_or(0),
                selected_by_default: descriptor.selected_by_default
                    && (descriptor.id != "attachments" || config.datasets.attachments_enabled),
                supported_modes: descriptor.supported_modes.to_vec(),
                available: descriptor.available,
                unavailable_reason: descriptor
                    .unavailable_reason
                    .map(|reason| reason.to_string()),
                db_stats: None,
                error,
            }
        })
        .collect()
}

fn detect_anomalies(config: &Config, sets: &[DatasetStatusEntry]) -> Vec<DatasetAnomalyDto> {
    let mut anomalies = Vec::new();

    // 1. Осиротевший каталог чатов рядом с действующим.
    let chats = config::resolve_chat_files_path(config);
    if let Some(parent) = chats.path.parent() {
        let legacy = parent.join(LEGACY_CHATS_DIRNAME);
        if legacy.is_dir() && legacy != chats.path {
            anomalies.push(DatasetAnomalyDto {
                code: "orphaned_chat_files".to_string(),
                severity: AnomalySeverity::Warning,
                message: format!(
                    "Рядом с рабочим каталогом чатов найден каталог «{LEGACY_CHATS_DIRNAME}» — \
                     исторический путь по умолчанию. Приложение в него не смотрит, \
                     в снапшоты он не попадает."
                ),
                path: Some(legacy.display().to_string()),
                hint: Some(
                    "Проверьте содержимое и удалите вручную либо перенесите нужное в каталог chats."
                        .to_string(),
                ),
            });
        }
    }

    // 2. Вложения, оставшиеся в старом месте (относительно рабочего каталога).
    let legacy_attachments = std::path::Path::new(LEGACY_ATTACHMENTS_PATH);
    let attachments = config::resolve_attachments_path(config);
    if legacy_attachments.is_dir()
        && legacy_attachments.canonicalize().ok() != attachments.path.canonicalize().ok()
    {
        anomalies.push(DatasetAnomalyDto {
            code: "legacy_attachments".to_string(),
            severity: AnomalySeverity::Warning,
            message: "Вложения чатов остались в каталоге, зависящем от рабочей директории \
                      процесса. Перенос выполняется при старте бэкенда, когда задан [data].root."
                .to_string(),
            path: legacy_attachments
                .canonicalize()
                .ok()
                .map(|path| path.display().to_string())
                .or_else(|| Some(LEGACY_ATTACHMENTS_PATH.to_string())),
            hint: Some(
                "Задайте [data].root в config.toml и перезапустите бэкенд — файлы переедут \
                 в <data_root>/attachments."
                    .to_string(),
            ),
        });
    }

    // 3. Пустые или отсутствующие наборы.
    for set in sets.iter().filter(|set| set.available) {
        if !set.exists {
            anomalies.push(DatasetAnomalyDto {
                code: "missing_directory".to_string(),
                severity: AnomalySeverity::Info,
                message: format!("Каталог набора «{}» не существует.", set.label_ru),
                path: Some(set.path.clone()),
                hint: Some(
                    "Это не ошибка: каталог создастся при восстановлении или первой записи."
                        .to_string(),
                ),
            });
        } else if set.file_count == 0 {
            anomalies.push(DatasetAnomalyDto {
                code: "empty_dataset".to_string(),
                severity: AnomalySeverity::Info,
                message: format!("Набор «{}» пуст.", set.label_ru),
                path: Some(set.path.clone()),
                hint: None,
            });
        }
    }

    // 4. Не задан общий корень данных — раскладка остаётся разнородной.
    if config::get_data_root(config).is_none() {
        anomalies.push(DatasetAnomalyDto {
            code: "no_data_root".to_string(),
            severity: AnomalySeverity::Warning,
            message: "Не задан [data].root: каталоги данных разрешаются каждый по-своему \
                      (абсолютный путь из своей секции либо путь относительно бинарника)."
                .to_string(),
            path: None,
            hint: Some(
                "Укажите [data].root в config.toml — наборы без явного абсолютного пути \
                 переедут под общий корень."
                    .to_string(),
            ),
        });
    } else {
        // 5. Каталоги, уведённые индивидуальным путём из-под общего корня.
        // Это разрешённое отклонение, но оно должно быть видно: такой каталог
        // выпадает из единой раскладки и из переноса наборов между инстансами.
        for (name, resolved) in config::resolved_data_paths(config)
            .into_iter()
            .filter(|(_, resolved)| resolved.origin != PathOrigin::DataRoot)
        {
            anomalies.push(DatasetAnomalyDto {
                code: "path_override".to_string(),
                severity: AnomalySeverity::Info,
                message: format!(
                    "Каталог «{name}» задан индивидуальным путём и лежит вне [data].root ({}).",
                    resolved.origin.label_ru()
                ),
                path: Some(resolved.display_path()),
                hint: Some(
                    "Если отклонение не нужно — уберите путь из своей секции config.toml \
                     и перенесите содержимое: каталог вернётся под общий корень."
                        .to_string(),
                ),
            });
        }
    }

    anomalies
}
