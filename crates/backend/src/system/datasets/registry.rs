//! Реестр наборов данных.
//!
//! Реестр живёт в коде, а не в БД: набор — это часть архитектуры приложения
//! (какие каталоги оно вообще держит), а не пользовательская настройка. Набор,
//! которого нет в этой таблице, не может появиться на диске сам по себе.

use contracts::system::datasets::{DatasetKind, RestoreMode};

use crate::shared::config::{
    self, Config, ResolvedPath, SUBDIR_ATTACHMENTS, SUBDIR_CHATS, SUBDIR_GOLDEN_SET,
    SUBDIR_KNOWLEDGE, SUBDIR_QUALITY_CHECKS, SUBDIR_SKILLS,
};

/// Какой горячий кэш нужно перезагрузить после восстановления набора.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotReload {
    None,
    KnowledgeBase,
    Skills,
    QualityChecks,
}

pub struct DatasetDescriptor {
    pub id: &'static str,
    pub label_ru: &'static str,
    pub description_ru: &'static str,
    pub kind: DatasetKind,
    /// Фиксированное имя подкаталога внутри `[data].root`.
    pub default_subdir: &'static str,
    /// Glob-подобные шаблоны исключений, применяются к пути относительно корня
    /// набора (сегменты через `/`). Поддерживаются `*` внутри сегмента и `**`
    /// как «любая глубина».
    pub exclude: &'static [&'static str],
    pub max_file_bytes: u64,
    pub selected_by_default: bool,
    pub supported_modes: &'static [RestoreMode],
    pub hot_reload: HotReload,
    /// `false` — набор виден в UI, но операции по нему пока не поддержаны.
    pub available: bool,
    pub unavailable_reason: Option<&'static str>,
}

/// 32 МиБ: файл крупнее почти наверняка попал в каталог данных по ошибке.
/// Такой файл не валит операцию — он уходит в `skipped` манифеста.
const DEFAULT_MAX_FILE_BYTES: u64 = 32 * 1024 * 1024;

/// Служебные файлы, которые не относятся к данным ни в одном наборе.
const COMMON_EXCLUDE: &[&str] = &["**/.DS_Store", "**/Thumbs.db", "**/*.tmp", "**/*.bak"];

const BOTH_MODES: &[RestoreMode] = &[RestoreMode::Merge, RestoreMode::Replace];
const REPLACE_ONLY: &[RestoreMode] = &[RestoreMode::Replace];

pub static DATASETS: &[DatasetDescriptor] = &[
    DatasetDescriptor {
        id: "knowledge",
        label_ru: "База знаний",
        description_ru: "Markdown-статьи базы знаний с YAML-frontmatter.",
        kind: DatasetKind::Files,
        default_subdir: SUBDIR_KNOWLEDGE,
        // Повторяет фильтры walk_markdown_files: эти каталоги — рабочее
        // окружение Obsidian и шаблоны, а не данные. Без исключения replace
        // снёс бы настройки редактора вместе со статьями.
        exclude: &[
            ".obsidian/**",
            ".trash/**",
            "_templates/**",
            "**/.DS_Store",
            "**/Thumbs.db",
            "**/*.tmp",
            "**/*.bak",
        ],
        max_file_bytes: DEFAULT_MAX_FILE_BYTES,
        selected_by_default: true,
        supported_modes: BOTH_MODES,
        hot_reload: HotReload::KnowledgeBase,
        available: true,
        unavailable_reason: None,
    },
    DatasetDescriptor {
        id: "skills",
        label_ru: "Навыки LLM",
        description_ru: "Внешний каталог навыков: пакеты SKILL.md и одиночные *.md.",
        kind: DatasetKind::Files,
        default_subdir: SUBDIR_SKILLS,
        exclude: COMMON_EXCLUDE,
        max_file_bytes: DEFAULT_MAX_FILE_BYTES,
        selected_by_default: true,
        supported_modes: BOTH_MODES,
        hot_reload: HotReload::Skills,
        available: true,
        unavailable_reason: None,
    },
    DatasetDescriptor {
        id: "chats",
        label_ru: "Рабочие каталоги чатов",
        description_ru: "Анкеты, планы и журнал шагов активностей LLM-чатов.",
        kind: DatasetKind::Files,
        default_subdir: SUBDIR_CHATS,
        exclude: COMMON_EXCLUDE,
        max_file_bytes: DEFAULT_MAX_FILE_BYTES,
        selected_by_default: false,
        supported_modes: BOTH_MODES,
        hot_reload: HotReload::None,
        available: true,
        unavailable_reason: None,
    },
    DatasetDescriptor {
        id: "quality_checks",
        label_ru: "Проверки качества",
        description_ru: "Внешние пакеты quality-проверок: check.json, check.mjs, schema.json.",
        kind: DatasetKind::Files,
        default_subdir: SUBDIR_QUALITY_CHECKS,
        exclude: COMMON_EXCLUDE,
        max_file_bytes: DEFAULT_MAX_FILE_BYTES,
        selected_by_default: true,
        supported_modes: BOTH_MODES,
        hot_reload: HotReload::QualityChecks,
        available: true,
        unavailable_reason: None,
    },
    DatasetDescriptor {
        id: "golden_set",
        label_ru: "Голден-сет",
        description_ru: "Эталонные вопросы для регрессии качества LLM.",
        kind: DatasetKind::Files,
        default_subdir: SUBDIR_GOLDEN_SET,
        exclude: COMMON_EXCLUDE,
        max_file_bytes: DEFAULT_MAX_FILE_BYTES,
        selected_by_default: true,
        supported_modes: BOTH_MODES,
        hot_reload: HotReload::None,
        available: true,
        unavailable_reason: None,
    },
    DatasetDescriptor {
        id: "attachments",
        label_ru: "Вложения чатов",
        description_ru: "Текстовые файлы, приложенные к сообщениям LLM-чата.",
        kind: DatasetKind::Files,
        default_subdir: SUBDIR_ATTACHMENTS,
        exclude: COMMON_EXCLUDE,
        max_file_bytes: DEFAULT_MAX_FILE_BYTES,
        // Единственный набор, растущий от действий пользователей: по умолчанию
        // не выбран и требует явного включения в [datasets].
        selected_by_default: false,
        supported_modes: BOTH_MODES,
        hot_reload: HotReload::None,
        available: true,
        unavailable_reason: None,
    },
    DatasetDescriptor {
        id: "database",
        label_ru: "База данных",
        description_ru: "Снапшот app.db. Требует VACUUM INTO и многочастной загрузки.",
        kind: DatasetKind::Database,
        default_subdir: config::SUBDIR_DB,
        exclude: &[],
        max_file_bytes: u64::MAX,
        selected_by_default: false,
        // Подмена БД частичным наложением бессмысленна: файл целостен или нет.
        supported_modes: REPLACE_ONLY,
        hot_reload: HotReload::None,
        available: false,
        unavailable_reason: Some(
            "Перенос БД появится во второй фазе: нужен снапшот через VACUUM INTO \
             и загрузка объекта по частям (сейчас S3-клиент грузит объект целиком в память).",
        ),
    },
];

pub fn lookup(set_id: &str) -> Option<&'static DatasetDescriptor> {
    DATASETS.iter().find(|set| set.id == set_id)
}

/// Наборы, доступные для операций прямо сейчас (без задела на фазу 2).
pub fn available() -> impl Iterator<Item = &'static DatasetDescriptor> {
    DATASETS.iter().filter(|set| set.available)
}

/// Резолвит корневой каталог набора по конфигу.
///
/// `config` передаётся снаружи намеренно: `load_config()` перечитывает и парсит
/// файл при каждом вызове, поэтому на семь наборов это было бы семь чтений диска.
pub fn resolve_root(config: &Config, descriptor: &DatasetDescriptor) -> Option<ResolvedPath> {
    let resolved = match descriptor.id {
        "knowledge" => config::resolve_knowledge_base_path(config),
        "skills" => config::resolve_skills_path(config),
        "chats" => config::resolve_chat_files_path(config),
        "quality_checks" => config::resolve_quality_checks_path(config),
        "golden_set" => config::resolve_golden_set_path(config),
        "attachments" => config::resolve_attachments_path(config),
        // У БД нет каталога-набора: её путь задан [database].path и
        // обрабатывается отдельным провайдером в фазе 2.
        "database" => return None,
        _ => return None,
    };
    Some(resolved)
}

/// Проверяет, что путь (относительно корня набора, сегменты через `/`) не
/// попадает ни под один шаблон исключений.
pub fn is_excluded(descriptor: &DatasetDescriptor, relative_path: &str) -> bool {
    descriptor
        .exclude
        .iter()
        .any(|pattern| matches_pattern(pattern, relative_path))
}

/// Упрощённый glob: `**` покрывает любое число сегментов, `*` — любой кусок
/// внутри одного сегмента. Полноценный glob-крейт ради шести шаблонов не нужен.
fn matches_pattern(pattern: &str, path: &str) -> bool {
    let pattern_parts: Vec<&str> = pattern.split('/').collect();
    let path_parts: Vec<&str> = path.split('/').collect();
    match_segments(&pattern_parts, &path_parts)
}

fn match_segments(pattern: &[&str], path: &[&str]) -> bool {
    match pattern.first() {
        None => path.is_empty(),
        Some(&"**") => {
            // `**` в конце шаблона покрывает любой остаток, включая пустой.
            if pattern.len() == 1 {
                return true;
            }
            (0..=path.len()).any(|skip| match_segments(&pattern[1..], &path[skip..]))
        }
        Some(segment) => match path.first() {
            None => false,
            Some(candidate) => {
                match_segment(segment, candidate) && match_segments(&pattern[1..], &path[1..])
            }
        },
    }
}

fn match_segment(pattern: &str, candidate: &str) -> bool {
    if !pattern.contains('*') {
        return pattern == candidate;
    }
    let mut rest = candidate;
    let parts: Vec<&str> = pattern.split('*').collect();
    for (index, part) in parts.iter().enumerate() {
        if part.is_empty() {
            continue;
        }
        if index == 0 {
            if !rest.starts_with(part) {
                return false;
            }
            rest = &rest[part.len()..];
            continue;
        }
        match rest.find(part) {
            Some(position) => rest = &rest[position + part.len()..],
            None => return false,
        }
    }
    // Шаблон без завершающей `*` обязан дотянуться до конца строки.
    parts.last().map_or(true, |last| {
        last.is_empty() || candidate.ends_with(last)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn dataset_ids_are_unique() {
        let mut seen = HashSet::new();
        for set in DATASETS {
            assert!(seen.insert(set.id), "duplicate dataset id '{}'", set.id);
        }
    }

    #[test]
    fn every_file_dataset_resolves_a_root() {
        let config = Config {
            database: crate::shared::config::DatabaseConfig {
                path: "C:/tmp/app.db".to_string(),
            },
            scheduled_tasks: Default::default(),
            data: Default::default(),
            instance: Default::default(),
            datasets: Default::default(),
            llm: Default::default(),
            quality: Default::default(),
            external_api: Default::default(),
            s3: Default::default(),
            mail: Default::default(),
            bitrix24: Default::default(),
        };
        for set in DATASETS.iter().filter(|s| s.kind == DatasetKind::Files) {
            assert!(
                resolve_root(&config, set).is_some(),
                "dataset '{}' has no resolved root",
                set.id
            );
        }
    }

    #[test]
    fn knowledge_excludes_obsidian_workspace() {
        let kb = lookup("knowledge").unwrap();
        assert!(is_excluded(kb, ".obsidian/workspace.json"));
        assert!(is_excluded(kb, ".obsidian/plugins/dataview/main.js"));
        assert!(is_excluded(kb, ".trash/old.md"));
        assert!(is_excluded(kb, "_templates/article.md"));
        assert!(!is_excluded(kb, "wb-funnel.md"));
        assert!(!is_excluded(kb, "marketplaces/wb/funnel.md"));
    }

    #[test]
    fn common_patterns_match_at_any_depth() {
        let skills = lookup("skills").unwrap();
        assert!(is_excluded(skills, "notes.tmp"));
        assert!(is_excluded(skills, "finance/references/data.bak"));
        assert!(is_excluded(skills, "a/b/c/Thumbs.db"));
        assert!(!is_excluded(skills, "finance/SKILL.md"));
    }

    #[test]
    fn database_set_is_visible_but_not_available() {
        let db = lookup("database").unwrap();
        assert_eq!(db.kind, DatasetKind::Database);
        assert!(!db.available);
        assert!(db.unavailable_reason.is_some());
        assert_eq!(db.supported_modes, REPLACE_ONLY);
    }
}
