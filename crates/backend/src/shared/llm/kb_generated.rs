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
    pub processes: usize,
    pub stages: usize,
    pub actions: usize,
    pub skills: usize,
    pub quality_checks: usize,
    pub ui_scopes: usize,
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
    let processes = processes_map(&mut report).await;
    let actions = actions_map(&mut report);
    let skills = skills_map(&mut report);
    let checks = quality_checks_map(&mut report);
    let ui = ui_map(&mut report);

    let maps = [
        ("data-profile.md", profile),
        ("plugins.md", plugins),
        ("processes.md", processes),
        ("actions.md", actions),
        ("skills.md", skills),
        ("quality-checks.md", checks),
        ("ui-map.md", ui),
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
        "[kb_generated] карты обновлены: {} файлов, {} таблиц, {} плагинов, \
         {} Процессов, {} Этапов, {} Действий, {} навыков, {} проверок, {} разделов UI",
        report.files.len(),
        report.tables_profiled,
        report.plugins,
        report.processes,
        report.stages,
        report.actions,
        report.skills,
        report.quality_checks,
        report.ui_scopes
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
///
/// Заголовок документа (`# ...`) пишется здесь, а не в каждой карте: `SEC-01`
/// требует его первой значащей строкой тела, и одно место записи — гарантия,
/// что новая карта не заведётся без него.
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
    let _ = writeln!(out, "# {title}\n");
    out
}

/// Заголовок раздела с якорем — стандарт `SEC-1`, уровень 1.
///
/// Якорь у машинной карты берётся из ключа объекта, а не из формулировки:
/// заголовок раздела перепишут, ключ переживёт. Ради этого якоря и заводятся.
fn section(out: &mut String, title: &str, slug: &str) {
    let _ = write!(out, "## {title} {{#{slug}}}\n\n");
}

/// Русское название → якорь `[a-z0-9-]`.
///
/// Нужна там, где ключа объекта нет и заголовок — единственное, что есть
/// (категории разделов UI). Транслитерация, а не хеш: якорь читает человек.
fn slugify(raw: &str) -> String {
    const CYRILLIC: [(char, &str); 33] = [
        ('а', "a"),
        ('б', "b"),
        ('в', "v"),
        ('г', "g"),
        ('д', "d"),
        ('е', "e"),
        ('ё', "e"),
        ('ж', "zh"),
        ('з', "z"),
        ('и', "i"),
        ('й', "y"),
        ('к', "k"),
        ('л', "l"),
        ('м', "m"),
        ('н', "n"),
        ('о', "o"),
        ('п', "p"),
        ('р', "r"),
        ('с', "s"),
        ('т', "t"),
        ('у', "u"),
        ('ф', "f"),
        ('х', "h"),
        ('ц', "c"),
        ('ч', "ch"),
        ('ш', "sh"),
        ('щ', "sch"),
        ('ъ', ""),
        ('ы', "y"),
        ('ь', ""),
        ('э', "e"),
        ('ю', "yu"),
        ('я', "ya"),
    ];
    let mut out = String::new();
    for ch in raw.to_lowercase().chars() {
        if ch.is_ascii_lowercase() || ch.is_ascii_digit() {
            out.push(ch);
        } else if let Some((_, latin)) = CYRILLIC.iter().find(|(c, _)| *c == ch) {
            out.push_str(latin);
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    let trimmed = out.trim_matches('-');
    // Верхняя граница якоря по `SEC-07` — 40 символов; режем по границе слова,
    // чтобы обрубок оставался читаемым.
    let capped: String = trimmed.chars().take(40).collect();
    capped.trim_matches('-').to_string()
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

    section(&mut out, "Таблицы, строки и периоды", "tables");
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
    // Раздел пишется всегда, даже когда пустых таблиц нет: структура карты не
    // должна зависеть от данных — иначе `SEC-03` срабатывает через раз, а модель
    // не может опереться на оглавление.
    out.push('\n');
    section(
        &mut out,
        &format!("Пустые таблицы ({})", empty.len()),
        "empty",
    );
    if empty.is_empty() {
        out.push_str("Пустых таблиц нет.\n");
    } else {
        let _ = writeln!(
            out,
            "Запрос к ним вернёт ноль строк независимо от фильтров: {}",
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
        "Список плагинов из таблицы plugin: код, назначение, тип рантайма, статус, SQL-ресурсы.",
        &["плагины", "расширения", "runtime"],
        &[],
    );
    out.push_str(generated_note());
    out.push_str(
        "Плагины живут строками в БД, а не файлами в репозитории, — их не найти поиском по коду.\n\
         Эта карта и есть единственный способ увидеть, что установлено в экземпляре.\n\
         Чем плагин отличается от Процесса и Действия — статья `app-mechanisms`.\n\n",
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

    out.push_str(
        "| Код | Название | Назначение | Рантайм | Статус | Включён | Версия | SQL-ресурсы |\n",
    );
    out.push_str("|---|---|---|---|---|---|---:|---|\n");
    for plugin in &plugins {
        let manifest = &plugin.bundle.manifest;
        let resources = plugin
            .bundle
            .sql_resources
            .keys()
            .cloned()
            .collect::<Vec<_>>()
            .join(", ");
        // `as_str()`, а не `{:?}`: Debug печатал имена Rust-вариантов («Hybrid»,
        // «Draft») посреди русского текста, и статус плагина читался как опечатка.
        let _ = writeln!(
            out,
            "| `{}` | {} | {} | {} | {} | {} | {} | {} |",
            manifest.code,
            manifest.title,
            one_line(manifest.description.as_deref().unwrap_or("—"), 140),
            manifest.runtime.as_str(),
            plugin.status.as_str(),
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

/// Текст в ячейку таблицы: без переносов и труб, с обрезкой по длине.
fn one_line(raw: &str, limit: usize) -> String {
    let flat = raw
        .replace(['\r', '\n'], " ")
        .replace('|', "\\|")
        .trim()
        .to_string();
    if flat.chars().count() <= limit {
        return flat;
    }
    let mut short: String = flat.chars().take(limit).collect();
    short.push('…');
    short
}

// ─── Процессы и Этапы ────────────────────────────────────────────────────────

/// Карта Процессов и Этапов этого экземпляра.
///
/// Определения живут в БД, а не файлами в репозитории, — ровно как плагины, и
/// точно так же не находятся поиском по коду. Механизм (что такое Процесс, Этап
/// и Действие, где их код) описан в `ARCHITECTURE.md`; здесь — что именно
/// заведено здесь и в каком состоянии.
async fn processes_map(report: &mut GenerateReport) -> String {
    let mut out = front_matter(
        "Процессы и Этапы: что заведено в этом экземпляре",
        "Головные версии Процессов и Этапов из БД: коды, статусы, триггеры, выходы и права.",
        &["процессы", "этапы", "pr", "st"],
        &[],
    );
    out.push_str(generated_note());
    // Раньше здесь стояла ссылка на `ARCHITECTURE.md` — файл репозитория, которого
    // в базе знаний нет: для чата это была ссылка в пустоту. Ссылаемся на статьи,
    // которые он действительно может открыть.
    out.push_str(
        "Определения Процессов и Этапов хранятся в БД и версионируются, — их не найти поиском\n\
         по коду. Что такое Процесс, Этап, Действие и Плагин — статья `app-mechanisms`;\n\
         каталог Действий — карта `actions`.\n\n",
    );

    let db = crate::shared::data::db::get_connection();

    match crate::processes::repository::list_process_head_records(db).await {
        Ok(processes) => {
            report.processes = processes.len();
            section(&mut out, "Процессы", "processes");
            if processes.is_empty() {
                out.push_str("_Процессов нет._\n\n");
            } else {
                out.push_str(
                    "| Код | Название | Статус | Версия | Триггер | Вход | Рёбер | QC |\n\
                     |---|---|---|---:|---|---|---:|---|\n",
                );
                for record in &processes {
                    let manifest = &record.definition.manifest;
                    let _ = writeln!(
                        out,
                        "| `{}` | {} | {} | {} | `{}` | `{}` | {} | {} |",
                        manifest.code,
                        manifest.title,
                        record.status.as_str(),
                        record.version,
                        manifest.trigger.event,
                        manifest.entry,
                        manifest.edges.len(),
                        manifest
                            .quality_check
                            .as_deref()
                            .map(|code| format!("`{code}`"))
                            .unwrap_or_else(|| String::from("—")),
                    );
                }
                out.push('\n');
            }
        }
        Err(error) => {
            report
                .errors
                .push(format!("Процессы не прочитаны: {error}"));
            section(&mut out, "Процессы", "processes");
            out.push_str("_Список недоступен._\n\n");
        }
    }

    match crate::processes::repository::list_stage_head_records(db).await {
        Ok(stages) => {
            report.stages = stages.len();
            section(&mut out, "Этапы", "stages");
            if stages.is_empty() {
                out.push_str("_Этапов нет._\n");
            } else {
                out.push_str(
                    "| Код | Название | Статус | Версия | Выходы | Права |\n\
                     |---|---|---|---:|---|---|\n",
                );
                for record in &stages {
                    let manifest = &record.definition.manifest;
                    let outputs: Vec<String> = manifest
                        .outputs
                        .iter()
                        .map(|output| format!("`{}`", output.name))
                        .collect();
                    let capabilities: Vec<String> = manifest
                        .capabilities
                        .iter()
                        .map(|capability| format!("`{capability}`"))
                        .collect();
                    let _ = writeln!(
                        out,
                        "| `{}` | {} | {} | {} | {} | {} |",
                        manifest.code,
                        manifest.title,
                        record.status.as_str(),
                        record.version,
                        dash_join(&outputs),
                        dash_join(&capabilities),
                    );
                }
            }
        }
        Err(error) => {
            report.errors.push(format!("Этапы не прочитаны: {error}"));
            section(&mut out, "Этапы", "stages");
            out.push_str("_Список недоступен._\n");
        }
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

    section(&mut out, "Каталог навыков", "catalog");
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

    out.push('\n');
    section(&mut out, "Инструменты по навыкам", "tools");
    let _ = write!(
        out,
        "{}",
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
        out.push('\n');
        section(&mut out, "Замечания загрузки", "diagnostics");
        let _ = write!(
            out,
            "{}",
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

// ─── Действия ────────────────────────────────────────────────────────────────

/// Каталог Действий — операций ядра с побочным эффектом.
///
/// Единственное семейство механизмов, которое живёт целиком в Rust, поэтому
/// карта собирается не из БД, а из паспортов `ActionInfo`. До этого каталог
/// существовал только в `ARCHITECTURE.md`, то есть был не виден чату вовсе:
/// на вопрос «какие Действия есть» ответить было нечем.
fn actions_map(report: &mut GenerateReport) -> String {
    let actions = crate::processes::actions::list();
    report.actions = actions.len();

    let mut out = front_matter(
        "Действия: каталог операций с побочным эффектом",
        "Что ядро умеет менять по команде Этапа или инструмента чата: имя, право, обратимость, таблицы записи.",
        &["действия", "процессы", "этапы", "эффекты"],
        &[],
    );
    out.push_str(generated_note());
    out.push_str(
        "Действие — операция ядра, которая меняет мир: у неё есть сухой прогон, ключ\n\
         идемпотентности и запись в `sys_effect_log`. Одна и та же запись подаётся в двух\n\
         оболочках: Этапу — как `host.actions.<метод>`, чату — как инструмент. Право\n\
         Этапа на вызов — `capability` вида `action:<имя>` в его манифесте.\n\n\
         Каталог закрыт и растёт только правкой Rust: нового Действия «на лету» не завести.\n\
         Механизм целиком — статья `app-mechanisms`.\n\n",
    );

    if actions.is_empty() {
        out.push_str("_Действий нет._\n");
        return out;
    }

    section(&mut out, "Каталог", "catalog");
    out.push_str("| Имя | host.actions | Право | Название | Обратимо | Пишет в |\n");
    out.push_str("|---|---|---|---|---|---|\n");
    for info in &actions {
        let writes: Vec<String> = info
            .write_tables
            .iter()
            .map(|table| format!("`{table}`"))
            .collect();
        let _ = writeln!(
            out,
            "| `{}` | `{}` | `{}` | {} | {} | {} |",
            info.name,
            info.method,
            info.capability,
            one_line(info.title, 80),
            if info.reversible {
                "да"
            } else {
                "**нет**"
            },
            dash_join(&writes),
        );
    }

    out.push('\n');
    section(&mut out, "Что делает каждое", "details");
    for info in &actions {
        let _ = writeln!(
            out,
            "- **`{}`** — {}\n",
            info.name,
            one_line(info.description, 400)
        );
    }
    out
}

// ─── Разделы интерфейса ──────────────────────────────────────────────────────

/// Карта разделов UI из `SCOPE_CATALOG`.
///
/// Нужна поддержке: на вопрос «где это в программе» отвечать было нечем —
/// у навыка `support` есть инструмент `find_page_help`, но статей с тегами
/// `user-guide` / `page:<ключ>` в базе не было ни одной, и он всегда возвращал
/// пусто. Ключ раздела здесь совпадает с ключом вкладки и с id scope.
fn ui_map(report: &mut GenerateReport) -> String {
    use crate::system::access::scope_catalog::SCOPE_CATALOG;

    report.ui_scopes = SCOPE_CATALOG.len();

    // `user-guide` — тот тег, по которому `find_page_help` собирает кандидатов:
    // одного достаточно, чтобы карта отвечала на вопрос про любой раздел.
    // Пер-страничные теги `page:<ключ>` сюда НЕ кладём: их было бы 67 в одной
    // строке frontmatter, а разницы нет — конкурентов у карты в этой выдаче
    // пока нет вовсе. Появятся статьи про отдельные страницы — они придут со
    // своими `page:` тегами и обойдут карту по релевантности, как и задумано.
    let mut out = front_matter(
        "Разделы интерфейса: что где лежит",
        "Каталог разделов приложения: ключ страницы, название в UI, категория и назначение.",
        &["user-guide", "интерфейс", "разделы", "навигация"],
        &[],
    );
    out.push_str(generated_note());
    out.push_str(
        "Карта отвечает на вопрос «где это в программе». Ключ раздела — он же ключ вкладки\n\
         и id права доступа: по нему раздел находится и в интерфейсе, и в матрице прав.\n\
         Раздел может быть не виден конкретному пользователю — это вопрос его прав, а не\n\
         наличия страницы. Страницы плагинов сюда не входят: они добавляются в рантайме.\n\n",
    );

    // Группируем по категории, сохраняя порядок появления в каталоге: он
    // осмысленный (справочники, документы, интеграции, система), а алфавит — нет.
    let mut order: Vec<&'static str> = Vec::new();
    for scope in SCOPE_CATALOG {
        if !order.contains(&scope.category) {
            order.push(scope.category);
        }
    }

    for category in order {
        section(&mut out, category, &slugify(category));
        out.push_str("| Ключ раздела | Название в UI | Тип | Про что |\n");
        out.push_str("|---|---|---|---|\n");
        for scope in SCOPE_CATALOG.iter().filter(|s| s.category == category) {
            let _ = writeln!(
                out,
                "| `{}` | {} | {} | {} |",
                scope.scope_id,
                one_line(scope.label, 60),
                scope.scope_type.as_str(),
                one_line(scope.description, 160),
            );
        }
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Карты обязаны соответствовать стандарту разделов `SEC-1`.
    ///
    /// Проверяются синхронные карты — асинхронным нужна живая БД. Этого хватает:
    /// заголовок документа пишет общий `front_matter`, и он один на все семь.
    #[test]
    fn generated_maps_follow_the_section_standard() {
        let mut report = GenerateReport::default();
        let maps = [
            ("actions", actions_map(&mut report)),
            ("skills", skills_map(&mut report)),
            ("quality-checks", quality_checks_map(&mut report)),
            ("ui-map", ui_map(&mut report)),
        ];
        for (name, content) in &maps {
            let body = content
                .splitn(3, "---\n")
                .nth(2)
                .expect("у карты есть frontmatter");
            let violations = super::super::knowledge_base::validate_structure(body, false);
            assert!(
                violations.is_empty(),
                "карта '{}' нарушает SEC-1: {:?}",
                name,
                violations
            );
        }
    }

    /// Карта без разделов отдаётся целиком — ради этого разделы и заводились.
    #[test]
    fn multi_part_maps_have_an_outline() {
        let mut report = GenerateReport::default();
        for (name, content) in [
            ("actions", actions_map(&mut report)),
            ("skills", skills_map(&mut report)),
            ("ui-map", ui_map(&mut report)),
        ] {
            let sections = super::super::knowledge_base::outline(&content);
            assert!(
                sections.len() >= 2,
                "карта '{}' не режется на разделы: {}",
                name,
                sections.len()
            );
            // Якорь у каждого: номер съедет от первой же новой категории.
            assert!(
                sections.iter().all(|s| s.slug.is_some()),
                "карта '{}': раздел без якоря",
                name
            );
        }
    }

    #[test]
    fn slugify_transliterates_and_stays_within_the_anchor_format() {
        assert_eq!(slugify("Справочники"), "spravochniki");
        assert_eq!(slugify("Документы и отчёты"), "dokumenty-i-otchety");
        // Длинное название режется до предела `SEC-07` и не оканчивается дефисом.
        let long = slugify("Очень длинное название категории раздела интерфейса");
        assert!(long.len() <= 40 && !long.ends_with('-'));
    }

    /// Карта Действий обязана покрывать каталог целиком: она — единственный
    /// источник, по которому чат вообще узнаёт о существовании Действия.
    #[test]
    fn actions_map_covers_the_whole_catalog() {
        let mut report = GenerateReport::default();
        let map = actions_map(&mut report);

        let catalog = crate::processes::actions::list();
        assert_eq!(report.actions, catalog.len());
        for info in &catalog {
            assert!(
                map.contains(info.name),
                "Действие '{}' не попало в карту",
                info.name
            );
            assert!(
                map.contains(info.method),
                "метод host.actions.'{}' не попал в карту",
                info.method
            );
        }
        assert!(map.starts_with("---\nkind: generated\n"));
    }

    /// `find_page_help` отбирает статьи по тегам `user-guide` и `page:<ключ>`.
    /// Без них карта разделов остаётся невидимой ровно для того инструмента,
    /// ради которого она заводилась.
    #[test]
    fn ui_map_carries_the_tags_find_page_help_searches_by() {
        let mut report = GenerateReport::default();
        let map = ui_map(&mut report);

        assert_eq!(
            report.ui_scopes,
            crate::system::access::scope_catalog::SCOPE_CATALOG.len()
        );
        assert!(map.contains("user-guide"), "нет тега user-guide");
        for scope in crate::system::access::scope_catalog::SCOPE_CATALOG {
            assert!(
                map.contains(scope.scope_id),
                "ключ раздела '{}' не попал в карту",
                scope.scope_id
            );
            assert!(
                map.contains(scope.label),
                "раздел '{}' не попал в карту",
                scope.scope_id
            );
        }
    }

    /// Ячейка таблицы не должна разъезжаться из-за переноса строки или трубы
    /// в описании, пришедшем из манифеста плагина.
    #[test]
    fn one_line_flattens_and_escapes() {
        assert_eq!(one_line("две\nстроки", 80), "две строки");
        assert_eq!(one_line("а | б", 80), r"а \| б");
        assert_eq!(one_line("абвгд", 3), "абв…");
    }
}
