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
use contracts::processes::{EdgeTarget, ProcessRecord, StageOutput, StageRecord, WaitSpec};
use serde_json::Value;
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
///
/// Обе выборки читаются до отрисовки, а не по разделу на запрос: граф и колонка
/// «где используется» связывают Процессы с Этапами, и разложи мы эту связь по
/// двум независимым веткам, каждая отвечала бы на половину вопроса.
async fn processes_map(report: &mut GenerateReport) -> String {
    let db = crate::shared::data::db::get_connection();

    let processes = match crate::processes::repository::list_process_head_records(db).await {
        Ok(records) => {
            report.processes = records.len();
            Some(records)
        }
        Err(error) => {
            report
                .errors
                .push(format!("Процессы не прочитаны: {error}"));
            None
        }
    };
    let stages = match crate::processes::repository::list_stage_head_records(db).await {
        Ok(records) => {
            report.stages = records.len();
            Some(records)
        }
        Err(error) => {
            report.errors.push(format!("Этапы не прочитаны: {error}"));
            None
        }
    };

    render_processes_map(processes.as_deref(), stages.as_deref())
}

/// Собрать карту из уже прочитанных головных версий.
///
/// Отделено от чтения ради теста: это единственная карта корпуса, разделы
/// которой ссылаются друг на друга, и проверить её структуру, не подняв базу,
/// иначе было бы нечем.
///
/// `None` — «выборка не удалась», и это не то же самое, что пустой список:
/// «Процессов нет» и «список недоступен» ведут читателя к разным выводам.
fn render_processes_map(
    processes: Option<&[ProcessRecord]>,
    stages: Option<&[StageRecord]>,
) -> String {
    let mut out = front_matter(
        "Процессы и Этапы: что заведено в этом экземпляре",
        "Головные версии Процессов и Этапов из БД: коды, статусы, триггеры, граф переходов, \
         контракт Этапа и то, какие Процессы на него ссылаются.",
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
         каталог Действий — карта `actions`.\n\n\
         Этап лежит в общем каталоге и адресуется Процессом по коду, поэтому один и тот же\n\
         Этап может стоять в нескольких графах. Кто на него ссылается — колонка «Где\n\
         используется»; куда ведёт каждый его выход — раздел «Граф».\n\n",
    );

    processes_section(&mut out, processes);
    graph_section(&mut out, processes);
    stages_section(&mut out, stages, processes);
    stage_contract_section(&mut out, stages);

    out
}

/// Процессы: строка на головную версию.
fn processes_section(out: &mut String, processes: Option<&[ProcessRecord]>) {
    section(out, "Процессы", "processes");
    let Some(processes) = processes else {
        out.push_str("_Список недоступен._\n\n");
        return;
    };
    if processes.is_empty() {
        out.push_str("_Процессов нет._\n\n");
        return;
    }

    out.push_str(
        "| Код | Название | Статус | Версия | Триггер | Вход | Рёбер | QC |\n\
         |---|---|---|---:|---|---|---:|---|\n",
    );
    for record in processes {
        let manifest = &record.definition.manifest;
        let _ = writeln!(
            out,
            "| `{}` | {} | {} | {} | `{}` | `{}` | {} | {} |",
            manifest.code,
            one_line(&manifest.title, 80),
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

/// Граф: строка на ребро.
///
/// Раньше от графа в карте было одно число — сколько рёбер, — и ответить по
/// ней на вопрос «куда ведёт выход „расхождение“» было нельзя: у механизма,
/// который целиком и есть граф, перечислялись узлы без связей.
fn graph_section(out: &mut String, processes: Option<&[ProcessRecord]>) {
    section(out, "Граф: куда ведёт каждый выход", "graph");
    let Some(processes) = processes else {
        out.push_str("_Список недоступен._\n\n");
        return;
    };
    let edges: usize = processes
        .iter()
        .map(|record| record.definition.manifest.edges.len())
        .sum();
    if edges == 0 {
        out.push_str("_Рёбер нет._\n\n");
        return;
    }

    out.push_str(
        "Экземпляр идёт по одному ребру за раз: Этап возвращает имя выхода, и оно выбирает\n\
         следующий шаг. Вход следующего Этапа собирается из ключа корреляции экземпляра и\n\
         данных этого выхода — больше взять неоткуда.\n\n\
         Ожидание на ребре означает, что экземпляр встаёт в `waiting` и просыпается по\n\
         событию с тем же ключом корреляции. По дедлайну он уходит в запасную цель, а если\n\
         её нет — остаётся человеку, а не идёт по графу дальше сам.\n\n",
    );
    out.push_str("| Процесс | Этап | Выход | Ведёт в | Ожидание |\n|---|---|---|---|---|\n");
    for record in processes {
        let manifest = &record.definition.manifest;
        for edge in &manifest.edges {
            let _ = writeln!(
                out,
                "| `{}` | `{}` | `{}` | {} | {} |",
                manifest.code,
                edge.from,
                edge.outcome,
                target_cell(&edge.to),
                wait_cell(edge.wait.as_ref()),
            );
        }
    }
    out.push('\n');
}

/// Цель ребра одной клеткой.
///
/// Терминал показывается словом, а не прочерком: «выход завершает Процесс» и
/// «ребро забыли» — разные вещи, и вторую валидатор графа не пропускает.
fn target_cell(target: &EdgeTarget) -> String {
    match target.stage_code() {
        Some(code) => format!("`{code}`"),
        None => String::from("завершение"),
    }
}

/// Ожидание на ребре одной клеткой.
fn wait_cell(wait: Option<&WaitSpec>) -> String {
    let Some(wait) = wait else {
        return String::from("переход сразу");
    };
    let on_timeout = match &wait.on_timeout {
        Some(target) => format!("по дедлайну → {}", target_cell(target)),
        None => String::from("по дедлайну — человеку"),
    };
    format!(
        "ждёт `{}`, дедлайн {}, {on_timeout}",
        wait.event,
        deadline_label(wait.deadline_minutes)
    )
}

/// Дедлайн человеческими единицами: в манифесте он в минутах, но «1440 мин»
/// читателю карты ничего не говорит.
///
/// Единицы сокращены до «ч» и «сут» намеренно — так клетка таблицы обходится
/// без склонения по числу, которого ради одной строки заводить не стоит.
fn deadline_label(minutes: i64) -> String {
    const DAY: i64 = 24 * 60;
    if minutes >= DAY && minutes % DAY == 0 {
        format!("{} сут", minutes / DAY)
    } else if minutes >= 60 && minutes % 60 == 0 {
        format!("{} ч", minutes / 60)
    } else {
        format!("{minutes} мин")
    }
}

/// Этапы: строка на головную версию, с обратной ссылкой на Процессы.
fn stages_section(
    out: &mut String,
    stages: Option<&[StageRecord]>,
    processes: Option<&[ProcessRecord]>,
) {
    section(out, "Этапы", "stages");
    let Some(stages) = stages else {
        out.push_str("_Список недоступен._\n\n");
        return;
    };
    if stages.is_empty() {
        out.push_str("_Этапов нет._\n\n");
        return;
    }

    out.push_str(
        "| Код | Название | Статус | Версия | Выходы | Права | Где используется |\n\
         |---|---|---|---:|---|---|---|\n",
    );
    for record in stages {
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
        let usage = match processes {
            Some(processes) => {
                let usage = stage_usage(&manifest.code, processes);
                if usage.is_empty() {
                    String::from("ни один Процесс не ссылается")
                } else {
                    usage.join("; ")
                }
            }
            None => String::from("неизвестно: список Процессов недоступен"),
        };
        let _ = writeln!(
            out,
            "| `{}` | {} | {} | {} | {} | {} | {} |",
            manifest.code,
            one_line(&manifest.title, 80),
            record.status.as_str(),
            record.version,
            dash_join(&outputs),
            dash_join(&capabilities),
            usage,
        );
    }
    out.push('\n');
}

/// Процессы, ссылающиеся на Этап, с его ролью в графе каждого.
///
/// Обратной таблицы в БД нет и не нужно: Процесс адресует Этап кодом, и связь
/// выводится из графа. Ровно это же считает карточка Этапа во фронте —
/// «в каких Процессах я участвую» есть главный вопрос про Этап, раз он заявлен
/// самостоятельной единицей, а не узлом одного графа.
fn stage_usage(code: &str, processes: &[ProcessRecord]) -> Vec<String> {
    processes
        .iter()
        .filter_map(|record| {
            let manifest = &record.definition.manifest;
            let mut roles: Vec<&str> = Vec::new();
            if manifest.entry == code {
                roles.push("вход");
            }
            if manifest
                .edges
                .iter()
                .any(|edge| edge.to.stage_code() == Some(code))
            {
                roles.push("цель ребра");
            }
            if manifest.edges.iter().any(|edge| {
                edge.wait
                    .as_ref()
                    .and_then(|wait| wait.on_timeout.as_ref())
                    .and_then(EdgeTarget::stage_code)
                    == Some(code)
            }) {
                roles.push("запасной по дедлайну");
            }
            // Ребро от Этапа, до которого не дойти, — дефект графа, а не
            // участие: сказать «используется» про такой Этап значило бы скрыть
            // ошибку автора за обычной строкой.
            if roles.is_empty() && manifest.edges.iter().any(|edge| edge.from == code) {
                roles.push("источник рёбер, но недостижим");
            }
            if roles.is_empty() {
                return None;
            }
            Some(format!("`{}` ({})", manifest.code, roles.join(", ")))
        })
        .collect()
}

/// Что подаётся Этапу и что он возвращает.
///
/// В таблице выше выходы стоят голыми именами, а «`сходится`, `расхождение`»
/// не объясняет, что эти слова значат, — при том что по ним читается весь граф.
/// Описания выходов и схема входа в манифесте есть; терялись они только по
/// дороге в карту.
fn stage_contract_section(out: &mut String, stages: Option<&[StageRecord]>) {
    section(out, "Что подаётся и что возвращает", "contract");
    let Some(stages) = stages else {
        out.push_str("_Список недоступен._\n");
        return;
    };
    if stages.is_empty() {
        out.push_str("_Этапов нет._\n");
        return;
    }

    out.push_str(
        "Вход — то, что Этап требует от ключа корреляции и предыдущего выхода вместе.\n\
         Необъявленная схема означает, что вход не проверяется вовсе, а не что его нет.\n\n",
    );
    for record in stages {
        let manifest = &record.definition.manifest;
        let _ = writeln!(
            out,
            "- **`{}` — {}**",
            manifest.code,
            one_line(&manifest.title, 80)
        );
        if !manifest.description.trim().is_empty() {
            let _ = writeln!(out, "  {}", one_line(&manifest.description, 400));
        }
        let _ = writeln!(
            out,
            "  - Вход: {}",
            input_summary(manifest.input_schema.as_ref())
        );
        // Выход на строку, а не списком через разделитель: описания — обычные
        // предложения с точкой, и в одну строку они склеиваются в «нет.; ...».
        if manifest.outputs.is_empty() {
            out.push_str("  - Выходы: —\n");
        } else {
            out.push_str("  - Выходы:\n");
            for output in &manifest.outputs {
                let _ = writeln!(out, "    - {}", output_summary(output));
            }
        }
        out.push('\n');
    }
}

/// Вход Этапа одной строкой: имя поля, тип, обязательность.
///
/// Разбор намеренно мелкий — карта отвечает на вопрос «что подавать», а не
/// воспроизводит валидатор; вложенность и `oneOf` тут не разворачиваются.
fn input_summary(schema: Option<&Value>) -> String {
    let Some(schema) = schema else {
        return String::from("схема не описана — вход не проверяется");
    };
    let required: Vec<&str> = schema
        .get("required")
        .and_then(Value::as_array)
        .map(|items| items.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();

    // `required` без `properties` — законная схема: поля названы, типы нет.
    // Промолчать про них было бы хуже всего: именно они и обязательны.
    let Some(properties) = schema.get("properties").and_then(Value::as_object) else {
        if required.is_empty() {
            return String::from("схема без полей верхнего уровня");
        }
        return required
            .iter()
            .map(|name| format!("`{name}` — обязательное"))
            .collect::<Vec<_>>()
            .join("; ");
    };

    let fields: Vec<String> = properties
        .iter()
        .map(|(name, spec)| {
            let kind = spec
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or("тип не указан");
            let mark = if required.contains(&name.as_str()) {
                ", обязательное"
            } else {
                ""
            };
            format!("`{name}` — {kind}{mark}")
        })
        .collect();
    if fields.is_empty() {
        String::from("схема без полей верхнего уровня")
    } else {
        fields.join("; ")
    }
}

/// Один выход строкой: имя, что оно значит, описаны ли данные.
///
/// Наличие схемы данных отмечается не для полноты: вход следующего Этапа
/// покрывается свойствами именно этой схемы, и выход без неё покрывает только
/// ключ корреляции.
fn output_summary(output: &StageOutput) -> String {
    let meaning = if output.description.trim().is_empty() {
        String::from("без описания")
    } else {
        one_line(&output.description, 160)
    };
    let described = if output.data_schema.is_some() {
        " (данные по схеме)"
    } else {
        ""
    };
    format!("`{}` — {meaning}{described}", output.name)
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
    use contracts::processes::{
        DefinitionRecord, DefinitionStatus, ProcessDefinition, ProcessEdge, ProcessManifest,
        ProcessTrigger, StageDefinition, StageManifest,
    };
    use serde_json::json;

    fn stage(
        code: &str,
        title: &str,
        description: &str,
        input_schema: Option<Value>,
        outputs: Vec<StageOutput>,
        capabilities: &[&str],
    ) -> StageRecord {
        DefinitionRecord {
            id: format!("row-{code}"),
            code: code.to_string(),
            version: 1,
            status: DefinitionStatus::Active,
            digest: "0123456789ab".into(),
            created_at: "2026-08-27T09:00:00Z".into(),
            created_by: None,
            definition: StageDefinition {
                manifest: StageManifest {
                    code: code.into(),
                    title: title.into(),
                    description: description.into(),
                    entrypoint: "stage.mjs".into(),
                    export: "run".into(),
                    input_schema,
                    outputs,
                    capabilities: capabilities.iter().map(|c| c.to_string()).collect(),
                },
                script: "export async function run() {}".into(),
                digest: "0123456789ab".into(),
            },
        }
    }

    fn output(name: &str, description: &str, described: bool) -> StageOutput {
        StageOutput {
            name: name.into(),
            description: description.into(),
            data_schema: described.then(|| json!({ "type": "object" })),
        }
    }

    /// Пилотный граф в миниатюре: цикл через «Позвать человека» с ожиданием и
    /// терминал — то есть все виды клетки «Ведёт в» и «Ожидание» сразу. Плюс
    /// Этап, на который не ссылается никто: колонка «Где используется» обязана
    /// уметь сказать и это.
    fn pilot() -> (Vec<ProcessRecord>, Vec<StageRecord>) {
        let edges = vec![
            ProcessEdge {
                from: "st0001".into(),
                outcome: "пересчитан".into(),
                to: EdgeTarget::stage("st0002"),
                wait: None,
            },
            ProcessEdge {
                from: "st0002".into(),
                outcome: "сходится".into(),
                to: EdgeTarget::Done,
                wait: None,
            },
            ProcessEdge {
                from: "st0002".into(),
                outcome: "расхождение".into(),
                to: EdgeTarget::stage("st0004"),
                wait: None,
            },
            ProcessEdge {
                from: "st0004".into(),
                outcome: "позвали".into(),
                to: EdgeTarget::stage("st0001"),
                wait: Some(WaitSpec {
                    event: "human.action.done".into(),
                    deadline_minutes: 24 * 60,
                    on_timeout: None,
                }),
            },
        ];
        let process = DefinitionRecord {
            id: "row-pr0001".into(),
            code: "pr0001".into(),
            version: 2,
            status: DefinitionStatus::Active,
            digest: "cafebabe".into(),
            created_at: "2026-08-27T09:00:00Z".into(),
            created_by: Some("claude_dev".into()),
            definition: ProcessDefinition {
                manifest: ProcessManifest {
                    code: "pr0001".into(),
                    title: "Закрытие дня WB".into(),
                    description: String::new(),
                    trigger: ProcessTrigger::on("import.day.completed"),
                    entry: "st0001".into(),
                    edges,
                    quality_check: Some("wb_day_not_closed".into()),
                },
                digest: "cafebabe".into(),
            },
        };
        let stages = vec![
            stage(
                "st0001",
                "Пересчитать день",
                "Перестраивает снимок закрытия дня и обновляет документ.",
                Some(json!({
                    "type": "object",
                    "required": ["connection_id", "business_date"],
                    "properties": {
                        "connection_id": { "type": "string" },
                        "business_date": { "type": "string" }
                    }
                })),
                vec![output("пересчитан", "Снимок дня перестроен.", true)],
                &["action:rebuild_day_close"],
            ),
            stage(
                "st0002",
                "Сверить с ГК",
                "",
                None,
                vec![
                    output("сходится", "Расхождений с Главной книгой нет.", false),
                    output("расхождение", "", true),
                ],
                &["db:read:gl_turnover"],
            ),
            stage(
                "st0004",
                "Позвать человека",
                "",
                None,
                vec![output("позвали", "Тикет заведён, ждём человека.", true)],
                &["action:request_human_action"],
            ),
            stage(
                "st0099",
                "Ничей Этап",
                "",
                None,
                vec![output("готово", "", false)],
                &[],
            ),
        ];
        (vec![process], stages)
    }

    /// Карты обязаны соответствовать стандарту разделов `SEC-1`.
    ///
    /// Карту Процессов проверяем на фикстуре: живая БД тесту не нужна с тех пор,
    /// как чтение отделено от отрисовки, а разделов у неё больше всех.
    #[test]
    fn generated_maps_follow_the_section_standard() {
        let mut report = GenerateReport::default();
        let (processes, stages) = pilot();
        let maps = [
            ("actions", actions_map(&mut report)),
            ("skills", skills_map(&mut report)),
            ("quality-checks", quality_checks_map(&mut report)),
            ("ui-map", ui_map(&mut report)),
            (
                "processes",
                render_processes_map(Some(&processes), Some(&stages)),
            ),
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
        let (processes, stages) = pilot();
        for (name, content) in [
            ("actions", actions_map(&mut report)),
            ("skills", skills_map(&mut report)),
            ("ui-map", ui_map(&mut report)),
            (
                "processes",
                render_processes_map(Some(&processes), Some(&stages)),
            ),
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

    /// Граф обязан читаться по карте. Раньше от него попадало одно число —
    /// сколько рёбер, — и спросить «куда ведёт выход „расхождение“» было не у
    /// чего: перечислялись узлы без связей.
    #[test]
    fn processes_map_carries_the_graph() {
        let (processes, stages) = pilot();
        let map = render_processes_map(Some(&processes), Some(&stages));

        assert!(
            map.contains("| `pr0001` | `st0002` | `расхождение` | `st0004` | переход сразу |"),
            "{map}"
        );
        // Терминал — слово, а не пустая клетка: «завершение» и «ребро забыли»
        // ведут читателя к разным выводам.
        assert!(
            map.contains("| `pr0001` | `st0002` | `сходится` | завершение | переход сразу |"),
            "{map}"
        );
        assert!(
            map.contains("ждёт `human.action.done`, дедлайн 1 сут, по дедлайну — человеку"),
            "{map}"
        );
    }

    /// «Где используется» — главный вопрос про Этап, раз он заявлен единицей,
    /// переиспользуемой между Процессами.
    #[test]
    fn processes_map_says_where_each_stage_is_used() {
        let (processes, stages) = pilot();
        let map = render_processes_map(Some(&processes), Some(&stages));

        assert!(map.contains("`pr0001` (вход, цель ребра)"), "{map}");
        assert!(map.contains("ни один Процесс не ссылается"), "{map}");
    }

    /// Список Процессов может не прочитаться, и тогда колонка обязана сказать
    /// «неизвестно». «Никто не ссылается» — утверждение о графе, и делать его
    /// не глядя в граф нельзя.
    #[test]
    fn unavailable_processes_do_not_look_like_an_unused_stage() {
        let (_, stages) = pilot();
        let map = render_processes_map(None, Some(&stages));

        assert!(
            map.contains("неизвестно: список Процессов недоступен"),
            "{map}"
        );
        assert!(!map.contains("ни один Процесс не ссылается"), "{map}");
    }

    /// Контракт Этапа: описание, обязательные поля входа и что значит каждый
    /// выход. Голое имя выхода не объясняет ничего, а по именам выходов
    /// читается весь граф.
    #[test]
    fn processes_map_carries_the_stage_contract() {
        let (processes, stages) = pilot();
        let map = render_processes_map(Some(&processes), Some(&stages));

        assert!(map.contains("Перестраивает снимок закрытия дня"), "{map}");
        assert!(
            map.contains("`connection_id` — string, обязательное"),
            "{map}"
        );
        assert!(
            map.contains("схема не описана — вход не проверяется"),
            "{map}"
        );
        assert!(
            map.contains("`сходится` — Расхождений с Главной книгой нет."),
            "{map}"
        );
        assert!(
            map.contains("`расхождение` — без описания (данные по схеме)"),
            "{map}"
        );
    }

    #[test]
    fn deadline_speaks_human() {
        assert_eq!(deadline_label(24 * 60), "1 сут");
        assert_eq!(deadline_label(120), "2 ч");
        assert_eq!(deadline_label(90), "90 мин");
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
