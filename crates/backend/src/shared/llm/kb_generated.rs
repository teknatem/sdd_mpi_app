//! Карты, собранные из БД и рантайма, — корпус `generated/` базы знаний.
//!
//! Тот же приём, что и `ARCHITECTURE.md`, применённый к тому, чего в коде нет:
//! профиль данных живёт в боевой БД, плагины — строками в таблице `plugin`,
//! навыки и проверки качества грузятся из каталога данных на старте. Ни одно из
//! этого не найти grep'ом по репозиторию, и ни одно нельзя написать руками —
//! оно устареет к следующему импорту.
//!
//! Жанр — карта, а не статья: строка на объект, детали по ссылке. Поэтому один
//! файл на предметную область, а не документ на каждую таблицу.
//!
//! Файлы пишутся в `<knowledge>/generated/` со штампом `kind: generated`, что
//! выводит их из выдачи `search_knowledge` по умолчанию (см. `DocKind`): их
//! десятки, они машинные, и в поиске они утопили бы курируемые статьи. Достать
//! их можно явным `corpus="generated"`, по id и — главное — через якоря,
//! которые генератор проставляет сам.

use super::knowledge_base::{knowledge_base_dir, GENERATED_DOCS_SUBDIR};
use std::fmt::Write as _;
use std::path::PathBuf;

/// Итог перегенерации: что записано и сколько объектов попало в карты.
#[derive(Debug, Default, serde::Serialize)]
pub struct GenerateReport {
    pub files: Vec<String>,
    pub tables_profiled: usize,
    pub plugins: usize,
    pub skills: usize,
    pub quality_checks: usize,
    pub errors: Vec<String>,
}

/// Пересчитать профиль данных и переписать все карты корпуса `generated`.
///
/// База знаний перечитывается в конце один раз: перезагрузка на каждый файл
/// стоила бы четырёх полных обходов каталога.
pub async fn regenerate_all() -> GenerateReport {
    let mut report = GenerateReport::default();

    match crate::shared::data::data_profile::refresh_all().await {
        Ok(count) => report.tables_profiled = count,
        Err(error) => report
            .errors
            .push(format!("профиль данных не пересчитан: {error}")),
    }

    let profile = data_profile_map().await;
    let plugins = plugins_map(&mut report).await;
    let skills = skills_map(&mut report);
    let checks = quality_checks_map(&mut report);

    let maps = [
        ("data-profile.md", profile),
        ("plugins.md", plugins),
        ("skills.md", skills),
        ("quality-checks.md", checks),
    ];
    for (file_name, content) in &maps {
        write_map(&mut report, file_name, content);
    }

    // Каталог наш целиком: карта, которую перестали генерировать, иначе осталась
    // бы файлом и продолжала отвечать на поиск устаревшими цифрами. Список
    // ожидаемого берём из набора карт, а не из успешно записанных: не записалась
    // — значит осталась прежняя версия, и удалять её тем более нельзя.
    let expected: Vec<String> = maps.iter().map(|(name, _)| name.to_string()).collect();
    for stale in super::knowledge_base::prune_stale_docs(&generated_dir(), &expected) {
        tracing::info!("[kb_generated] удалена устаревшая карта '{}'", stale);
    }

    if let Err(error) = super::knowledge_base::reload_knowledge_base() {
        report
            .errors
            .push(format!("база знаний не перечитана: {error}"));
    }

    tracing::info!(
        "[kb_generated] карты обновлены: {} файлов, {} таблиц, {} плагинов, {} навыков, {} проверок",
        report.files.len(),
        report.tables_profiled,
        report.plugins,
        report.skills,
        report.quality_checks
    );
    report
}

fn generated_dir() -> PathBuf {
    knowledge_base_dir().join(GENERATED_DOCS_SUBDIR)
}

fn write_map(report: &mut GenerateReport, file_name: &str, content: &str) {
    let dir = generated_dir();
    if let Err(error) = std::fs::create_dir_all(&dir) {
        report
            .errors
            .push(format!("каталог '{}' не создан: {error}", dir.display()));
        return;
    }
    let path = dir.join(file_name);
    // Не переписываем неизменившийся файл: иначе mtime прыгает каждый запуск и
    // Obsidian показывает ложные правки.
    if std::fs::read_to_string(&path).is_ok_and(|existing| existing == content) {
        report.files.push(file_name.to_string());
        return;
    }
    match std::fs::write(&path, content) {
        Ok(_) => report.files.push(file_name.to_string()),
        Err(error) => report
            .errors
            .push(format!("карта '{file_name}' не записана: {error}")),
    }
}

/// Шапка карты. `entities` — якоря: по ним карта находится вместе со статьями
/// об этих же объектах, в том числе из `get_entity_schema`.
fn front_matter(title: &str, summary: &str, tags: &[&str], entities: &[String]) -> String {
    let mut out = String::from("---\nkind: generated\n");
    let _ = writeln!(out, "title: {title}");
    let _ = writeln!(out, "summary: {summary}");
    let _ = writeln!(out, "tags: [{}]", tags.join(", "));
    if !entities.is_empty() {
        let _ = writeln!(out, "entities: [{}]", entities.join(", "));
    }
    let _ = writeln!(out, "updated: {}", chrono::Utc::now().format("%Y-%m-%d"));
    out.push_str("---\n\n");
    out
}

fn generated_note() -> &'static str {
    "> **Сгенерировано из данных — править вручную бессмысленно.**\n\
     > Обновляется при старте приложения и по `POST /api/kb/generate`.\n\n"
}

// ─── Профиль данных ──────────────────────────────────────────────────────────

async fn data_profile_map() -> String {
    let rows = crate::shared::data::data_profile::list_all().await;
    let entities: Vec<String> = rows.iter().map(|r| r.entity_index.clone()).collect();
    // Таблица → дата документа, лежащая в JSON. Период по ней не измеряется, но
    // пустая клетка без пояснения читается как «дат в таблице нет вообще».
    let json_dates: std::collections::HashMap<&str, &str> =
        crate::shared::llm::metadata_registry::METADATA_REGISTRY
            .profile_targets()
            .into_iter()
            .filter_map(|t| Some((t.table, t.json_date_field?)))
            .collect();

    let mut out = front_matter(
        "Профиль данных: строки, периоды, заполненность",
        "Сколько строк в каждой таблице, за какой период есть данные и где не заполнены ссылки.",
        &["данные", "профиль", "таблицы", "sql"],
        &entities,
    );
    out.push_str(generated_note());
    out.push_str(
        "Отвечает на вопрос, которого нет в схеме: есть ли вообще данные и за какой период.\n\
         Пустая таблица или период вне диапазона — самая частая причина «запрос вернул ноль строк».\n\n",
    );

    if rows.is_empty() {
        out.push_str("_Профиль ещё не считался._\n");
        return out;
    }

    out.push_str("| Объект | Таблица | Строк | Период | Колонка даты | Незаполненные ссылки |\n");
    out.push_str("|---|---|---:|---|---|---|\n");
    for row in &rows {
        let period = match (&row.date_min, &row.date_max) {
            (Some(from), Some(to)) => format!("{} … {}", short_date(from), short_date(to)),
            _ => String::from("—"),
        };
        let gaps = row
            .null_shares()
            .into_iter()
            .filter(|(_, share)| *share > 0.0)
            .map(|(column, share)| format!("{column} {share}%"))
            .collect::<Vec<_>>()
            .join(", ");
        let date_column = match (
            row.date_column.as_deref(),
            json_dates.get(row.table_name.as_str()),
        ) {
            (Some(column), _) => column.to_string(),
            (None, Some(field)) => format!("{field} (в JSON, период не измеряется)"),
            (None, None) => String::from("—"),
        };
        let _ = writeln!(
            out,
            "| `{}` | `{}` | {} | {} | {} | {} |",
            row.entity_index,
            row.table_name,
            row.row_count,
            period,
            date_column,
            if gaps.is_empty() {
                String::from("—")
            } else {
                gaps
            }
        );
    }

    let empty: Vec<&str> = rows
        .iter()
        .filter(|r| r.row_count == 0)
        .map(|r| r.table_name.as_str())
        .collect();
    if !empty.is_empty() {
        let _ = write!(
            out,
            "\n## Пустые таблицы ({})\n\nЗапрос к ним вернёт ноль строк независимо от фильтров: {}\n",
            empty.len(),
            empty
                .iter()
                .map(|t| format!("`{t}`"))
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
    out
}

/// `2026-08-11T12:00:00Z` → `2026-08-11`.
fn short_date(value: &str) -> &str {
    value.split(['T', ' ']).next().unwrap_or(value)
}

// ─── Плагины ─────────────────────────────────────────────────────────────────

async fn plugins_map(report: &mut GenerateReport) -> String {
    let mut out = front_matter(
        "Плагины: что установлено в этом экземпляре",
        "Список плагинов из таблицы plugin: код, тип рантайма, статус, серверные методы.",
        &["плагины", "расширения", "runtime"],
        &[],
    );
    out.push_str(generated_note());
    out.push_str(
        "Плагины живут строками в БД, а не файлами в репозитории, — их не найти поиском по коду.\n\
         Эта карта и есть единственный способ увидеть, что установлено в экземпляре.\n\n",
    );

    let db = crate::shared::data::db::get_connection();
    let plugins = match crate::plugins::repository::list_all(db).await {
        Ok(plugins) => plugins,
        Err(error) => {
            report.errors.push(format!("плагины не прочитаны: {error}"));
            out.push_str("_Список недоступен._\n");
            return out;
        }
    };
    report.plugins = plugins.len();

    if plugins.is_empty() {
        out.push_str("_Плагинов нет._\n");
        return out;
    }

    out.push_str("| Код | Название | Рантайм | Статус | Включён | Версия | SQL-ресурсы |\n");
    out.push_str("|---|---|---|---|---|---:|---|\n");
    for plugin in &plugins {
        let manifest = &plugin.bundle.manifest;
        let resources = plugin
            .bundle
            .sql_resources
            .keys()
            .cloned()
            .collect::<Vec<_>>()
            .join(", ");
        let _ = writeln!(
            out,
            "| `{}` | {} | {:?} | {:?} | {} | {} | {} |",
            manifest.code,
            manifest.title,
            manifest.runtime,
            plugin.status,
            if plugin.is_enabled { "да" } else { "нет" },
            plugin.version,
            if resources.is_empty() {
                String::from("—")
            } else {
                resources
            }
        );
    }
    out
}

// ─── Навыки ──────────────────────────────────────────────────────────────────

fn skills_map(report: &mut GenerateReport) -> String {
    let snapshot = super::skills::snapshot();
    report.skills = snapshot.skills.len();

    let mut out = front_matter(
        "Навыки LLM: что умеет ассистент",
        "Каталог навыков: интенты, инструменты, ресурсы и назначения по умолчанию.",
        &["навыки", "llm", "инструменты"],
        &[],
    );
    out.push_str(generated_note());

    if snapshot.skills.is_empty() {
        out.push_str("_Навыков нет._\n");
        return out;
    }

    out.push_str("| Навык | Название | Интенты | Инструментов | Ресурсы | По умолчанию для |\n");
    out.push_str("|---|---|---|---:|---|---|\n");
    for skill in snapshot.skills.iter() {
        let _ = writeln!(
            out,
            "| `{}` | {} | {} | {} | {} | {} |",
            skill.id,
            skill.title,
            dash_join(&skill.intents),
            skill.tool_names.len(),
            skill.resources.len(),
            dash_join(&skill.default_for)
        );
    }

    let _ = write!(
        out,
        "\n## Инструменты по навыкам\n\n{}",
        snapshot
            .skills
            .iter()
            .map(|skill| format!(
                "- **{}**: {}\n",
                skill.id,
                if skill.tool_names.is_empty() {
                    String::from("—")
                } else {
                    skill.tool_names.join(", ")
                }
            ))
            .collect::<String>()
    );

    if !snapshot.diagnostics.is_empty() {
        let _ = write!(
            out,
            "\n## Замечания загрузки\n\n{}",
            snapshot
                .diagnostics
                .iter()
                .map(|d| format!("- {d}\n"))
                .collect::<String>()
        );
    }
    out
}

fn dash_join(items: &[String]) -> String {
    if items.is_empty() {
        String::from("—")
    } else {
        items.join(", ")
    }
}

// ─── Проверки качества ───────────────────────────────────────────────────────

fn quality_checks_map(report: &mut GenerateReport) -> String {
    let snapshot = crate::quality::registry::snapshot();
    report.quality_checks = snapshot.definitions.len();

    let mut out = front_matter(
        "Проверки качества данных",
        "Каталог quality-checks: что проверяется, к какой категории относится и чем исполняется.",
        &["качество", "проверки", "данные"],
        &[],
    );
    out.push_str(generated_note());
    out.push_str(
        "Каждая проверка измеряет пару «популяция — нарушения»: метрика без знаменателя\n\
         не говорит ничего. Запуск — `run_quality_check(code)`.\n\n",
    );

    if snapshot.definitions.is_empty() {
        out.push_str("_Проверок нет._\n");
        return out;
    }

    out.push_str("| Код | Название | Категория | Тип | Описание |\n");
    out.push_str("|---|---|---|---|---|\n");
    for definition in snapshot.definitions.iter() {
        let info = &definition.info;
        let _ = writeln!(
            out,
            "| `{}` | {} | {} | {} | {} |",
            info.code,
            info.name,
            info.category,
            definition.kind,
            info.description.replace('|', "\\|")
        );
    }
    out
}
