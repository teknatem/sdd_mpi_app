//! Сканирование каталога набора: дерево файлов, размеры, хеши, диагностика.
//!
//! Скан — общий фундамент трёх операций: он даёт содержимое для бандла, счётчики
//! для страницы состояния и левую часть диффа при восстановлении. Поэтому здесь
//! же считаются sha256 каждого файла — на объёмах этих наборов (единицы мегабайт)
//! это дешевле, чем изобретать отдельный быстрый путь, а сравнение по содержимому
//! единственное надёжное: mtime после копирования каталогов ничего не значит.
//!
//! Фаза 2 (набор `database`) не пойдёт этим путём: снапшот БД снимается через
//! `VACUUM INTO` и остаётся одним файлом. Диспетчеризация — по
//! `DatasetDescriptor::kind` у вызывающего.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use contracts::system::datasets::{FileEntry, SkippedEntry};
use sha2::{Digest, Sha256};

use super::registry::{is_excluded, DatasetDescriptor};

/// Потолок числа файлов в наборе. Защита от «кто-то положил в каталог знаний
/// выгрузку на сто тысяч файлов»: без него скан молча съел бы всю память.
const MAX_FILES_PER_SET: usize = 50_000;

/// Максимальная глубина обхода — страховка от циклов по симлинкам.
const MAX_DEPTH: usize = 32;

#[derive(Debug, Clone)]
pub struct ScannedFile {
    /// Путь относительно корня набора, сегменты через `/`.
    pub relative_path: String,
    pub absolute_path: PathBuf,
    pub size_bytes: u64,
    pub sha256: String,
    pub modified_at: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct DatasetScan {
    pub root: PathBuf,
    pub existed: bool,
    /// Отсортировано по `relative_path` — от этого зависит воспроизводимость хеша.
    pub files: Vec<ScannedFile>,
    pub skipped: Vec<SkippedEntry>,
    pub total_bytes: u64,
}

impl DatasetScan {
    pub fn file_count(&self) -> u64 {
        self.files.len() as u64
    }

    /// Детерминированный хеш дерева: не зависит ни от порядка обхода файловой
    /// системы, ни от времени модификации — только от имён и содержимого.
    pub fn tree_sha256(&self) -> String {
        let mut hasher = Sha256::new();
        for file in &self.files {
            hasher.update(file.relative_path.as_bytes());
            hasher.update([0]);
            hasher.update(file.size_bytes.to_string().as_bytes());
            hasher.update([0]);
            hasher.update(file.sha256.as_bytes());
            hasher.update([b'\n']);
        }
        hex_lower(&hasher.finalize())
    }

    pub fn manifest_entries(&self) -> Vec<FileEntry> {
        self.files
            .iter()
            .map(|file| FileEntry {
                path: file.relative_path.clone(),
                size_bytes: file.size_bytes,
                sha256: file.sha256.clone(),
                modified_at: file.modified_at.clone(),
            })
            .collect()
    }

    /// Карта «путь → (размер, хеш)» для сравнения с содержимым бандла.
    pub fn index(&self) -> BTreeMap<String, (u64, String)> {
        self.files
            .iter()
            .map(|file| {
                (
                    file.relative_path.clone(),
                    (file.size_bytes, file.sha256.clone()),
                )
            })
            .collect()
    }
}

fn hex_lower(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    hex_lower(&Sha256::digest(bytes))
}

/// Сканирует каталог набора. Отсутствующий каталог — не ошибка: это законное
/// состояние (голден-сет ещё не заведён), которое манифест фиксирует как
/// `existed: false`, а UI показывает предупреждением.
pub fn scan_directory(descriptor: &DatasetDescriptor, root: &Path) -> anyhow::Result<DatasetScan> {
    let mut scan = DatasetScan {
        root: root.to_path_buf(),
        existed: root.is_dir(),
        ..Default::default()
    };
    if !scan.existed {
        return Ok(scan);
    }

    walk(descriptor, root, root, 0, &mut scan)?;
    scan.files
        .sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
    scan.skipped.sort_by(|left, right| left.path.cmp(&right.path));
    scan.total_bytes = scan.files.iter().map(|file| file.size_bytes).sum();
    Ok(scan)
}

fn walk(
    descriptor: &DatasetDescriptor,
    root: &Path,
    dir: &Path,
    depth: usize,
    scan: &mut DatasetScan,
) -> anyhow::Result<()> {
    if depth > MAX_DEPTH {
        scan.skipped.push(SkippedEntry {
            path: relative_of(root, dir),
            reason: format!("превышена глубина вложенности ({MAX_DEPTH})"),
        });
        return Ok(());
    }

    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) => {
            scan.skipped.push(SkippedEntry {
                path: relative_of(root, dir),
                reason: format!("каталог не читается: {error}"),
            });
            return Ok(());
        }
    };

    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                scan.skipped.push(SkippedEntry {
                    path: relative_of(root, dir),
                    reason: format!("запись каталога не читается: {error}"),
                });
                continue;
            }
        };
        let path = entry.path();
        let relative = relative_of(root, &path);
        if relative.is_empty() {
            continue;
        }
        if is_excluded(descriptor, &relative) {
            continue;
        }

        let metadata = match entry.metadata() {
            Ok(metadata) => metadata,
            Err(error) => {
                scan.skipped.push(SkippedEntry {
                    path: relative,
                    reason: format!("метаданные не читаются: {error}"),
                });
                continue;
            }
        };

        if metadata.is_dir() {
            walk(descriptor, root, &path, depth + 1, scan)?;
            continue;
        }
        if !metadata.is_file() {
            // Симлинки и прочие спецфайлы переносить нечем: на принимающей
            // стороне цель ссылки может не существовать.
            scan.skipped.push(SkippedEntry {
                path: relative,
                reason: "не обычный файл (ссылка или спецфайл)".to_string(),
            });
            continue;
        }

        if scan.files.len() >= MAX_FILES_PER_SET {
            scan.skipped.push(SkippedEntry {
                path: relative,
                reason: format!("превышен лимит в {MAX_FILES_PER_SET} файлов на набор"),
            });
            continue;
        }

        let size = metadata.len();
        if size > descriptor.max_file_bytes {
            scan.skipped.push(SkippedEntry {
                path: relative,
                reason: format!(
                    "файл больше лимита: {:.1} МБ > {:.1} МБ",
                    size as f64 / 1024.0 / 1024.0,
                    descriptor.max_file_bytes as f64 / 1024.0 / 1024.0
                ),
            });
            continue;
        }

        let bytes = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) => {
                scan.skipped.push(SkippedEntry {
                    path: relative,
                    reason: format!("файл не читается: {error}"),
                });
                continue;
            }
        };

        scan.files.push(ScannedFile {
            relative_path: relative,
            absolute_path: path,
            size_bytes: size,
            sha256: sha256_hex(&bytes),
            modified_at: modified_at(&metadata),
        });
    }

    Ok(())
}

fn modified_at(metadata: &std::fs::Metadata) -> Option<String> {
    let modified = metadata.modified().ok()?;
    let datetime: chrono::DateTime<chrono::Utc> = modified.into();
    Some(datetime.to_rfc3339())
}

/// Путь относительно корня набора, всегда через `/`: ключи манифеста должны
/// совпадать на Windows и Linux, иначе дифф на другой платформе покажет полное
/// расхождение там, где файлы идентичны.
fn relative_of(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .components()
        .map(|component| component.as_os_str().to_string_lossy().to_string())
        .collect::<Vec<_>>()
        .join("/")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::system::datasets::registry::lookup;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("mpi-scan-{name}-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write(root: &Path, relative: &str, content: &str) {
        let path = root.join(relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, content).unwrap();
    }

    #[test]
    fn missing_directory_is_an_empty_scan_not_an_error() {
        let descriptor = lookup("golden_set").unwrap();
        let scan = scan_directory(descriptor, Path::new("F:/definitely/not/here")).unwrap();
        assert!(!scan.existed);
        assert_eq!(scan.file_count(), 0);
    }

    #[test]
    fn excluded_paths_never_reach_the_manifest() {
        let root = temp_dir("exclude");
        write(&root, "article.md", "# hello");
        write(&root, "nested/deep.md", "# deep");
        write(&root, ".obsidian/workspace.json", "{}");
        write(&root, "_templates/base.md", "tpl");
        write(&root, "scratch.tmp", "junk");

        let scan = scan_directory(lookup("knowledge").unwrap(), &root).unwrap();
        let paths: Vec<&str> = scan
            .files
            .iter()
            .map(|file| file.relative_path.as_str())
            .collect();
        assert_eq!(paths, vec!["article.md", "nested/deep.md"]);

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn tree_hash_ignores_traversal_order() {
        let first = temp_dir("hash-a");
        let second = temp_dir("hash-b");
        // Одинаковое содержимое, разный порядок создания файлов.
        write(&first, "a.md", "alpha");
        write(&first, "b/c.md", "gamma");
        write(&second, "b/c.md", "gamma");
        write(&second, "a.md", "alpha");

        let descriptor = lookup("knowledge").unwrap();
        let left = scan_directory(descriptor, &first).unwrap();
        let right = scan_directory(descriptor, &second).unwrap();
        assert_eq!(left.tree_sha256(), right.tree_sha256());

        // А изменение содержимого хеш меняет.
        write(&second, "a.md", "alpha changed");
        let changed = scan_directory(descriptor, &second).unwrap();
        assert_ne!(left.tree_sha256(), changed.tree_sha256());

        std::fs::remove_dir_all(&first).ok();
        std::fs::remove_dir_all(&second).ok();
    }

    #[test]
    fn oversized_files_are_skipped_with_a_reason() {
        let root = temp_dir("oversize");
        write(&root, "small.md", "ok");
        write(&root, "big.md", &"x".repeat(2048));

        // Урезанный дескриптор: лимит в 1 КиБ вместо штатных 32 МиБ.
        let descriptor = DatasetDescriptor {
            max_file_bytes: 1024,
            ..copy_of(lookup("knowledge").unwrap())
        };
        let scan = scan_directory(&descriptor, &root).unwrap();

        assert_eq!(scan.file_count(), 1);
        assert_eq!(scan.files[0].relative_path, "small.md");
        assert_eq!(scan.skipped.len(), 1);
        assert_eq!(scan.skipped[0].path, "big.md");
        assert!(scan.skipped[0].reason.contains("больше лимита"));

        std::fs::remove_dir_all(&root).ok();
    }

    fn copy_of(descriptor: &'static DatasetDescriptor) -> DatasetDescriptor {
        DatasetDescriptor {
            id: descriptor.id,
            label_ru: descriptor.label_ru,
            description_ru: descriptor.description_ru,
            kind: descriptor.kind,
            default_subdir: descriptor.default_subdir,
            exclude: descriptor.exclude,
            max_file_bytes: descriptor.max_file_bytes,
            selected_by_default: descriptor.selected_by_default,
            supported_modes: descriptor.supported_modes,
            hot_reload: descriptor.hot_reload,
            available: descriptor.available,
            unavailable_reason: descriptor.unavailable_reason,
        }
    }
}
