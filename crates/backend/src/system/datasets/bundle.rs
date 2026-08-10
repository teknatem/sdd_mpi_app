//! Сборка и разбор zip-бандла снапшота.
//!
//! Раскладка архива:
//!
//! ```text
//! manifest.json               # первая запись — читается без распаковки остального
//! sets/<set_id>/<rel_path>    # файлы наборов
//! ```
//!
//! Архив собирается целиком в памяти: суммарный объём файловых наборов —
//! единицы мегабайт, а атомарность («отдаём только целиком собранный архив»)
//! важнее экономии. Ограничение зафиксировано лимитом `max_bundle_bytes`,
//! который проверяется ДО сборки, по результатам скана.
//!
//! Набор `database` (фаза 2) в этот архив не попадёт: 3 ГБ нужно грузить
//! отдельным объектом и по частям. Поэтому манифест ссылается на объекты по
//! `object_index`, а не подразумевает, что бандл ровно один.

use std::collections::BTreeMap;
use std::io::{Cursor, Read, Write};

use contracts::system::datasets::{BundleManifest, MANIFEST_ENTRY_NAME, SETS_ENTRY_PREFIX};
use zip::{write::SimpleFileOptions, CompressionMethod, ZipArchive, ZipWriter};

use super::scan::DatasetScan;

/// Потолок на распакованный объём — защита от zip-бомбы во входящем архиве.
const MAX_UNPACKED_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const MAX_ENTRIES: usize = 200_000;

/// Содержимое одного набора внутри разобранного бандла.
pub struct BundleSetFiles {
    /// Путь относительно корня набора → содержимое файла.
    pub files: BTreeMap<String, Vec<u8>>,
}

pub struct ParsedBundle {
    pub manifest: BundleManifest,
    pub sets: BTreeMap<String, BundleSetFiles>,
}

impl ParsedBundle {
    pub fn set(&self, set_id: &str) -> Option<&BundleSetFiles> {
        self.sets.get(set_id)
    }
}

/// Собирает архив из уже выполненных сканов. Манифест пишется первой записью.
pub fn build_bundle(
    manifest: &BundleManifest,
    scans: &[(String, &DatasetScan)],
) -> anyhow::Result<Vec<u8>> {
    let mut zip = ZipWriter::new(Cursor::new(Vec::<u8>::new()));
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);

    zip.start_file(MANIFEST_ENTRY_NAME, options)?;
    zip.write_all(serde_json::to_string_pretty(manifest)?.as_bytes())?;

    for (set_id, scan) in scans {
        for file in &scan.files {
            let entry_name = format!("{SETS_ENTRY_PREFIX}/{set_id}/{}", file.relative_path);
            let bytes = std::fs::read(&file.absolute_path).map_err(|error| {
                anyhow::anyhow!(
                    "не удалось прочитать {}: {error}",
                    file.absolute_path.display()
                )
            })?;
            zip.start_file(&entry_name, options)?;
            zip.write_all(&bytes)?;
        }
    }

    Ok(zip.finish()?.into_inner())
}

/// Читает только манифест — не распаковывая файлы наборов.
pub fn read_manifest(bytes: &[u8]) -> anyhow::Result<BundleManifest> {
    let mut archive = ZipArchive::new(Cursor::new(bytes))
        .map_err(|error| anyhow::anyhow!("не удалось открыть архив: {error}"))?;
    let mut entry = archive
        .by_name(MANIFEST_ENTRY_NAME)
        .map_err(|_| anyhow::anyhow!("в архиве нет {MANIFEST_ENTRY_NAME}"))?;
    let mut text = String::new();
    entry.read_to_string(&mut text)?;
    Ok(serde_json::from_str(&text)?)
}

/// Полный разбор архива: манифест + содержимое наборов.
pub fn read_bundle(bytes: &[u8]) -> anyhow::Result<ParsedBundle> {
    let mut archive = ZipArchive::new(Cursor::new(bytes))
        .map_err(|error| anyhow::anyhow!("не удалось открыть архив: {error}"))?;
    if archive.len() > MAX_ENTRIES {
        anyhow::bail!("в архиве слишком много записей: {}", archive.len());
    }

    let mut manifest: Option<BundleManifest> = None;
    let mut sets: BTreeMap<String, BundleSetFiles> = BTreeMap::new();
    let mut unpacked: u64 = 0;

    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
        if entry.is_dir() {
            continue;
        }
        let name = entry.name().to_string();

        unpacked = unpacked.saturating_add(entry.size());
        if unpacked > MAX_UNPACKED_BYTES {
            anyhow::bail!("распакованный размер архива превышает допустимый");
        }

        if name == MANIFEST_ENTRY_NAME {
            let mut text = String::new();
            entry.read_to_string(&mut text)?;
            manifest = Some(serde_json::from_str(&text)?);
            continue;
        }

        let Some((set_id, relative)) = split_set_entry(&name)? else {
            // Неизвестная запись верхнего уровня — не наши данные, пропускаем,
            // но и не падаем: формат должен быть расширяемым вперёд.
            continue;
        };

        let mut buffer = Vec::with_capacity(entry.size() as usize);
        entry.read_to_end(&mut buffer)?;
        sets.entry(set_id)
            .or_insert_with(|| BundleSetFiles {
                files: BTreeMap::new(),
            })
            .files
            .insert(relative, buffer);
    }

    let manifest =
        manifest.ok_or_else(|| anyhow::anyhow!("в архиве нет {MANIFEST_ENTRY_NAME}"))?;
    Ok(ParsedBundle { manifest, sets })
}

/// Разбирает `sets/<set_id>/<rel>` и заодно валидирует путь.
///
/// Это главный барьер против zip-slip: имена внутри архива приходят извне и
/// подставляются в файловые операции, поэтому `..`, абсолютные пути, буквы
/// дисков и обратные слеши отсекаются здесь, до любой записи на диск.
fn split_set_entry(name: &str) -> anyhow::Result<Option<(String, String)>> {
    let Some(rest) = name.strip_prefix(&format!("{SETS_ENTRY_PREFIX}/")) else {
        return Ok(None);
    };
    let Some((set_id, relative)) = rest.split_once('/') else {
        return Ok(None);
    };
    validate_set_id(set_id)?;
    validate_relative_path(relative).map_err(|error| {
        anyhow::anyhow!("небезопасный путь в архиве '{name}': {error}")
    })?;
    Ok(Some((set_id.to_string(), relative.to_string())))
}

fn validate_set_id(set_id: &str) -> anyhow::Result<()> {
    if set_id.is_empty()
        || !set_id
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-')
    {
        anyhow::bail!("недопустимый идентификатор набора '{set_id}'");
    }
    Ok(())
}

/// Проверка пути, извлечённого из архива, перед записью на диск.
pub fn validate_relative_path(relative: &str) -> Result<(), String> {
    if relative.is_empty() {
        return Err("пустой путь".to_string());
    }
    if relative.starts_with('/') {
        return Err("абсолютный путь".to_string());
    }
    if relative.contains('\\') {
        return Err("обратный слеш в пути".to_string());
    }
    if relative.contains('\0') {
        return Err("нулевой байт в пути".to_string());
    }
    // C:\… и \\server\share — на Windows такой путь увёл бы запись за пределы
    // каталога набора, даже без единой `..`.
    if relative.len() >= 2 && relative.as_bytes()[1] == b':' {
        return Err("путь с буквой диска".to_string());
    }
    for segment in relative.split('/') {
        if segment.is_empty() {
            return Err("пустой сегмент пути".to_string());
        }
        if segment == "." || segment == ".." {
            return Err("относительный сегмент '.' или '..'".to_string());
        }
        if segment.ends_with(' ') || segment.ends_with('.') {
            // Windows молча обрезает такие имена — «a.txt.» и «a.txt» станут
            // одним файлом, что ломает соответствие манифесту.
            return Err(format!("недопустимое имя сегмента '{segment}'"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use contracts::system::datasets::{
        BundleObjectKind, Compression, DatasetKind, InstanceEnv, PathOrigin, SetManifest,
        SnapshotObject, SourceInfo, DATASET_BUNDLE_FORMAT_VERSION,
    };

    fn sample_manifest() -> BundleManifest {
        BundleManifest {
            format_version: DATASET_BUNDLE_FORMAT_VERSION,
            snapshot_id: "20260810T120000Z-abcdef12".to_string(),
            created_at: "2026-08-10T12:00:00Z".to_string(),
            created_by: "user:1 (admin)".to_string(),
            note: None,
            source: SourceInfo {
                instance_id: "dev-a".to_string(),
                instance_label: "Рабочий".to_string(),
                instance_env: InstanceEnv::Production,
                hostname: "HOST".to_string(),
                app_version: "0.1.0".to_string(),
                git_commit: "deadbee".to_string(),
                build_profile: "debug".to_string(),
                schema_version: 207,
                os: "windows/x86_64".to_string(),
                data_root: "F:/data/app".to_string(),
                database_path: "F:/data/app/app.db".to_string(),
                config_path: "F:/app/config.toml".to_string(),
                captured_at: "2026-08-10T12:00:00Z".to_string(),
            },
            objects: vec![SnapshotObject {
                object_index: 0,
                key: "datasets/dev-a/20260810T120000Z-abcdef12/bundle.zip".to_string(),
                kind: BundleObjectKind::FilesZip,
                size_bytes: 0,
                sha256: String::new(),
                compression: Compression::Deflate,
                multipart: false,
            }],
            sets: vec![SetManifest {
                set_id: "knowledge".to_string(),
                label_ru: "База знаний".to_string(),
                kind: DatasetKind::Files,
                object_index: 0,
                source_path: "F:/data/app/knowledge".to_string(),
                path_origin: PathOrigin::DataRoot,
                existed: true,
                file_count: 2,
                total_bytes: 10,
                sha256: "0".repeat(64),
                files: Vec::new(),
                skipped: Vec::new(),
                db_stats: None,
            }],
        }
    }

    #[test]
    fn round_trip_preserves_manifest_and_files() {
        let root = std::env::temp_dir().join(format!("mpi-bundle-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(root.join("nested")).unwrap();
        std::fs::write(root.join("a.md"), "alpha").unwrap();
        std::fs::write(root.join("nested/b.md"), "beta").unwrap();

        let descriptor = crate::system::datasets::registry::lookup("knowledge").unwrap();
        let scan = crate::system::datasets::scan::scan_directory(descriptor, &root).unwrap();
        let manifest = sample_manifest();

        let bytes = build_bundle(&manifest, &[("knowledge".to_string(), &scan)]).unwrap();

        let only_manifest = read_manifest(&bytes).unwrap();
        assert_eq!(only_manifest.snapshot_id, manifest.snapshot_id);

        let parsed = read_bundle(&bytes).unwrap();
        let files = &parsed.set("knowledge").unwrap().files;
        assert_eq!(files.len(), 2);
        assert_eq!(files.get("a.md").unwrap(), b"alpha");
        assert_eq!(files.get("nested/b.md").unwrap(), b"beta");

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn zip_slip_paths_are_rejected() {
        for evil in [
            "../evil.md",
            "a/../../evil.md",
            "/etc/passwd",
            "C:/windows/system32/evil.dll",
            "a\\b.md",
            "a//b.md",
        ] {
            assert!(
                validate_relative_path(evil).is_err(),
                "path '{evil}' must be rejected"
            );
        }
        for safe in ["a.md", "nested/b.md", "a/b/c/d.json"] {
            assert!(
                validate_relative_path(safe).is_ok(),
                "path '{safe}' must be accepted"
            );
        }
    }

    #[test]
    fn entry_names_outside_sets_prefix_are_ignored() {
        assert!(split_set_entry("readme.txt").unwrap().is_none());
        assert!(split_set_entry("sets/knowledge").unwrap().is_none());
        assert_eq!(
            split_set_entry("sets/knowledge/a.md").unwrap(),
            Some(("knowledge".to_string(), "a.md".to_string()))
        );
        assert!(split_set_entry("sets/knowledge/../../evil").is_err());
    }
}
