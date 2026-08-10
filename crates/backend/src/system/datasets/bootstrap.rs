//! Приведение раскладки на диске в порядок при старте приложения.
//!
//! Обе процедуры идемпотентны и не считаются критичными: если что-то не вышло,
//! приложение поднимается как раньше, а расхождение попадает в аномалии на
//! странице наборов данных.

use std::path::Path;

use crate::shared::config::{self, Config};

/// Историческое расположение вложений — относительно рабочего каталога процесса.
const LEGACY_ATTACHMENTS_PATH: &str = "uploads/chat_attachments";

/// Создаёт подкаталоги `[data].root`, если корень задан.
///
/// Без общего корня ничего не создаём: каталоги в этом режиме разрешаются
/// относительно бинарника, и плодить их в `target/debug` незачем.
pub fn ensure_data_root(config: &Config) {
    let Some(root) = config::get_data_root(config) else {
        return;
    };
    for subdir in [
        config::SUBDIR_DB,
        config::SUBDIR_KNOWLEDGE,
        config::SUBDIR_SKILLS,
        config::SUBDIR_CHATS,
        config::SUBDIR_GOLDEN_SET,
        config::SUBDIR_QUALITY_CHECKS,
        config::SUBDIR_ATTACHMENTS,
        config::SUBDIR_BACKUPS,
        config::SUBDIR_TMP,
    ] {
        let path = root.join(subdir);
        if let Err(error) = std::fs::create_dir_all(&path) {
            tracing::warn!(
                "datasets: не удалось создать каталог {}: {error}",
                path.display()
            );
        }
    }
}

/// Переносит вложения чатов из `./uploads/chat_attachments` в настроенный корень.
///
/// Выполняется только когда целевой путь стабилен — то есть задан `[data].root`
/// или явный абсолютный путь. Иначе вложения переехали бы в каталог рядом с
/// бинарником, который сносится первым же `cargo clean`; это было бы хуже
/// исходной проблемы.
pub fn migrate_legacy_attachments(config: &Config) {
    let target = config::resolve_attachments_path(config);
    if matches!(
        target.origin,
        contracts::system::datasets::PathOrigin::ExeRelative
    ) {
        return;
    }

    let legacy = Path::new(LEGACY_ATTACHMENTS_PATH);
    if !legacy.is_dir() {
        return;
    }
    if legacy.canonicalize().ok() == target.path.canonicalize().ok() {
        return;
    }

    if let Err(error) = std::fs::create_dir_all(&target.path) {
        tracing::warn!(
            "datasets: не удалось создать каталог вложений {}: {error}",
            target.path.display()
        );
        return;
    }

    let mut moved = 0_u64;
    let mut conflicts = 0_u64;
    match move_tree(legacy, &target.path, &mut moved, &mut conflicts) {
        Ok(()) => {
            if moved > 0 || conflicts > 0 {
                tracing::info!(
                    "datasets: перенесено вложений: {moved}, пропущено из-за конфликта имён: {conflicts} → {}",
                    target.path.display()
                );
            }
            // Пустые каталоги убираем, непустые (остались конфликты) — оставляем.
            let _ = remove_empty_tree(legacy);
        }
        Err(error) => tracing::warn!("datasets: перенос вложений прерван: {error}"),
    }
}

fn move_tree(
    source: &Path,
    destination: &Path,
    moved: &mut u64,
    conflicts: &mut u64,
) -> std::io::Result<()> {
    for entry in std::fs::read_dir(source)? {
        let entry = entry?;
        let from = entry.path();
        let to = destination.join(entry.file_name());
        if from.is_dir() {
            std::fs::create_dir_all(&to)?;
            move_tree(&from, &to, moved, conflicts)?;
            continue;
        }
        if to.exists() {
            // Файл с таким именем уже есть: молча перезаписывать чужие данные
            // нельзя, а имя здесь — uuid, так что совпадение почти наверняка
            // означает, что перенос уже выполнялся.
            *conflicts += 1;
            continue;
        }
        // rename не работает через границы томов — тогда копируем и удаляем.
        match std::fs::rename(&from, &to) {
            Ok(()) => *moved += 1,
            Err(_) => {
                std::fs::copy(&from, &to)?;
                std::fs::remove_file(&from)?;
                *moved += 1;
            }
        }
    }
    Ok(())
}

fn remove_empty_tree(dir: &Path) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        if entry.path().is_dir() {
            let _ = remove_empty_tree(&entry.path());
        }
    }
    std::fs::remove_dir(dir)
}
