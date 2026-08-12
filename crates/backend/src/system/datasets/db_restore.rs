//! Подмена файла БД: подготовка при восстановлении и применение при старте.
//!
//! Живой `app.db` держит открытым пул подключений, и на Windows переименовать
//! что-либо поверх открытого файла нельзя. Закрывать пул под работающими
//! обработчиками — worse: часть запросов увидела бы старую базу, часть новую.
//!
//! Поэтому восстановление разнесено во времени. Оно кладёт готовый файл рядом
//! под именем `pending_restore.db` и пишет маркер; подмена происходит при
//! следующем старте процесса — до того, как база вообще открывается. Маркер
//! отдельным файлом, а не флагом в БД: та самая база и подменяется.

use std::path::{Path, PathBuf};

use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::shared::config::{self, Config};

/// Файл, ожидающий установки на место рабочей базы.
const PENDING_DB_FILE: &str = "pending_restore.db";
/// Маркер с описанием того, что именно ждёт установки.
const PENDING_MARKER_FILE: &str = "pending_restore.json";

/// Сколько прежних баз держать в `backups/`. Каждая — гигабайты, поэтому счёт
/// идёт на единицы, в отличие от файловых архивов (их десять).
const KEEP_DB_ARCHIVES: usize = 2;

/// Что лежит в `pending_restore.db`. Пишется рядом с файлом, чтобы после
/// перезапуска было видно, откуда приехала база, — иначе подмена выглядит как
/// самопроизвольная смена данных.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingRestore {
    pub snapshot_id: String,
    pub source_instance_id: String,
    pub source_hostname: String,
    pub schema_version: i64,
    /// sha256 несжатого файла — как он записан в манифесте снапшота.
    pub sha256: String,
    pub staged_at: String,
    pub actor: Option<String>,
}

fn db_dir(config: &Config) -> anyhow::Result<PathBuf> {
    let path = config::resolve_database_path(config)?.path;
    path.parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| anyhow::anyhow!("У пути к базе данных нет родительского каталога"))
}

/// Кладёт подготовленный файл на место ожидающего и пишет маркер.
///
/// Файл переносится (а не копируется) в каталог базы: во-первых, копия ещё
/// одних гигабайт не нужна, во-вторых, `rename` при старте обязан работать —
/// а он не работает через границу томов.
pub fn stage_pending_restore(
    config: &Config,
    prepared: &Path,
    marker: PendingRestore,
) -> anyhow::Result<PathBuf> {
    let dir = db_dir(config)?;
    std::fs::create_dir_all(&dir)?;

    let target = dir.join(PENDING_DB_FILE);
    let _ = std::fs::remove_file(&target);
    move_file(prepared, &target)?;

    let marker_path = dir.join(PENDING_MARKER_FILE);
    std::fs::write(&marker_path, serde_json::to_vec_pretty(&marker)?)?;
    Ok(target)
}

/// Есть ли подготовленная подмена — показывается на странице, чтобы «данные не
/// поменялись» не выглядело как потеря восстановления.
pub fn pending(config: &Config) -> Option<PendingRestore> {
    let dir = db_dir(config).ok()?;
    if !dir.join(PENDING_DB_FILE).exists() {
        return None;
    }
    let raw = std::fs::read(dir.join(PENDING_MARKER_FILE)).ok()?;
    serde_json::from_slice(&raw).ok()
}

/// Применяет отложенную подмену. Вызывается ОДИН раз при старте — до открытия
/// пула подключений, иначе файл снова окажется занят.
///
/// Возвращает описание того, что сделано, для вывода в лог старта.
pub fn apply_pending_restore(config: &Config) -> Option<String> {
    let dir = db_dir(config).ok()?;
    let staged = dir.join(PENDING_DB_FILE);
    let marker_path = dir.join(PENDING_MARKER_FILE);
    if !staged.exists() {
        // Маркер без файла — след неудачной подготовки; убираем, чтобы он не
        // сбивал с толку на следующем старте.
        let _ = std::fs::remove_file(&marker_path);
        return None;
    }

    let marker: Option<PendingRestore> = std::fs::read(&marker_path)
        .ok()
        .and_then(|raw| serde_json::from_slice(&raw).ok());

    let live = match config::resolve_database_path(config) {
        Ok(resolved) => resolved.path,
        Err(error) => {
            return Some(format!(
                "Подмена БД отменена: не удалось определить путь к базе ({error})"
            ))
        }
    };

    // Прежняя база уезжает в backups переименованием: копировать гигабайты
    // только чтобы потом их удалить — бессмысленно, а откат при этом остаётся.
    let backups = config::get_backups_path(config);
    if let Err(error) = std::fs::create_dir_all(&backups) {
        return Some(format!(
            "Подмена БД отменена: не удалось подготовить каталог резервных копий ({error})"
        ));
    }
    let archived = backups.join(format!(
        "pre-restore-db-{}.db",
        Utc::now().format("%Y%m%dT%H%M%SZ")
    ));

    if live.exists() {
        if let Err(error) = move_file(&live, &archived) {
            return Some(format!(
                "Подмена БД отменена: не удалось убрать текущую базу в резерв ({error}). \
                 Рабочая база не тронута."
            ));
        }
    }
    // WAL и shm относятся к УЖЕ убранной базе: оставить их рядом с новой —
    // верный способ получить «database disk image is malformed».
    let _ = std::fs::remove_file(with_suffix(&live, "-wal"));
    let _ = std::fs::remove_file(with_suffix(&live, "-shm"));

    if let Err(error) = move_file(&staged, &live) {
        // Возвращаем прежнюю базу на место — без неё приложение не поднимется.
        let _ = move_file(&archived, &live);
        return Some(format!(
            "Подмена БД не удалась ({error}). Прежняя база возвращена на место."
        ));
    }

    let _ = std::fs::remove_file(&marker_path);
    rotate_db_archives(&backups);

    let source = marker
        .map(|marker| {
            format!(
                "снапшот {} с «{}» (схема {})",
                marker.snapshot_id, marker.source_hostname, marker.schema_version
            )
        })
        .unwrap_or_else(|| "подготовленный файл".to_string());
    Some(format!(
        "База данных заменена: {source}. Прежняя база сохранена в {}",
        archived.display()
    ))
}

/// `rename` в пределах тома, с откатом на copy+remove при переходе между томами
/// (индивидуальный `[database].path` может увести базу на другой диск).
fn move_file(from: &Path, to: &Path) -> anyhow::Result<()> {
    match std::fs::rename(from, to) {
        Ok(()) => Ok(()),
        Err(_) => {
            std::fs::copy(from, to)?;
            std::fs::remove_file(from)?;
            Ok(())
        }
    }
}

fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut raw = path.to_path_buf().into_os_string();
    raw.push(suffix);
    PathBuf::from(raw)
}

/// Держим последние `KEEP_DB_ARCHIVES` копий базы. Имя начинается с метки
/// времени, поэтому лексикографический порядок совпадает с хронологическим.
fn rotate_db_archives(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut archives: Vec<PathBuf> = entries
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("pre-restore-db-") && name.ends_with(".db"))
        })
        .collect();
    if archives.len() <= KEEP_DB_ARCHIVES {
        return;
    }
    archives.sort();
    for path in archives.iter().take(archives.len() - KEEP_DB_ARCHIVES) {
        let _ = std::fs::remove_file(path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wal_and_shm_names_hang_off_the_database_file() {
        let db = PathBuf::from("F:/data/db/app.db");
        assert_eq!(
            with_suffix(&db, "-wal"),
            PathBuf::from("F:/data/db/app.db-wal")
        );
        assert_eq!(
            with_suffix(&db, "-shm"),
            PathBuf::from("F:/data/db/app.db-shm")
        );
    }

    #[test]
    fn rotation_keeps_only_the_newest_database_archives() {
        let dir = std::env::temp_dir().join(format!("dsdb-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        for stamp in ["20260101T000000Z", "20260102T000000Z", "20260103T000000Z"] {
            std::fs::write(dir.join(format!("pre-restore-db-{stamp}.db")), b"x").unwrap();
        }
        // Чужие файлы ротация трогать не должна.
        std::fs::write(dir.join("pre-restore-20260101T000000Z.zip"), b"x").unwrap();

        rotate_db_archives(&dir);

        let mut left: Vec<String> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        left.sort();
        assert_eq!(
            left,
            vec![
                "pre-restore-20260101T000000Z.zip".to_string(),
                "pre-restore-db-20260102T000000Z.db".to_string(),
                "pre-restore-db-20260103T000000Z.db".to_string(),
            ]
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
