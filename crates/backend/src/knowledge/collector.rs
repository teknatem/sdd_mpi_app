//! Обход реестра поверхностей и сбор единиц учёта (§4).
//!
//! Три способа обнаружения, и ни один по отдельности не полон: обход файлов
//! видит markdown и JS, запрос БД — плагины и Процессы, обход реестров Rust —
//! Действия, сущности, источники данных, счета, обороты и разделы UI. Поэтому
//! сбор идёт **от реестра поверхностей**, а каждая строка приносит свой
//! перечислитель. Обратный порядок — обойти диск и посчитать найденное — даёт
//! правдоподобное число, молча пропускающее большую часть системы.
//!
//! Ошибка одного перечислителя не отменяет снимок: поверхность останется пустой,
//! а причина ляжет в диагностику. Терять четыреста единиц из-за недоступной
//! таблицы плагинов бессмысленно.
//!
//! **Профиль данных здесь не пересчитывается.** `kb_generated::regenerate_all`
//! обходит все таблицы каталога и стоит секунды; инвентаризация читает готовые
//! карты, а не производит их.

use std::collections::{BTreeMap, BTreeSet};

use contracts::knowledge::{
    InventorySummaryDto, KnowledgeUnitDto, Lifecycle, SurfaceDto, UnitFamily,
};
use sea_orm::DatabaseConnection;

use super::catalog::{Enumerator, SurfaceDef, SURFACE_CATALOG};
use super::tool_map;
use crate::shared::llm::knowledge_base::{estimate_tokens, DocKind};

/// Итог сбора: единицы, реестр со счётчиками, сводка и то, что не получилось.
pub struct Collected {
    pub units: Vec<KnowledgeUnitDto>,
    pub surfaces: Vec<SurfaceDto>,
    pub summary: InventorySummaryDto,
    pub diagnostics: Vec<String>,
}

/// Рабочий контекст сбора: складывает единицы и жалобы.
struct Sink {
    units: Vec<KnowledgeUnitDto>,
    diagnostics: Vec<String>,
}

impl Sink {
    /// Заготовка единицы с осями, унаследованными от поверхности.
    ///
    /// Наследование, а не повторение: девять осей на четыреста единиц никто не
    /// проставит руками, а расхождение между единицей и её поверхностью было бы
    /// не находкой, а опечаткой.
    fn unit(
        &self,
        surface: &SurfaceDef,
        id_prefix: &str,
        id: &str,
        title: &str,
    ) -> KnowledgeUnitDto {
        KnowledgeUnitDto {
            unit_id: format!("{id_prefix}:{id}"),
            surface_id: surface.surface_id.to_string(),
            family: surface.family,
            origin: surface.origin,
            storage_form: surface.storage_form,
            editor: surface.editor,
            reachability: tool_map::effective_reachability(
                surface.surface_id,
                surface.reachability_declared,
            ),
            lifecycle: Lifecycle::Active,
            scope: surface.scope,
            channel: surface.channel,
            code_role: surface.code_role,
            title: title.to_string(),
            subtitle: String::new(),
            source_ref: None,
            bytes: None,
            tokens: None,
            search_hits: 0,
            read_hits: 0,
            cited_hits: 0,
            updated: None,
            staleness_pct: None,
            tags: Vec::new(),
            issues: Vec::new(),
        }
    }

    fn complain(&mut self, surface: &SurfaceDef, error: impl std::fmt::Display) {
        self.diagnostics.push(format!(
            "поверхность '{}' не перечислена: {error}",
            surface.surface_id
        ));
    }
}

/// Собрать инвентаризацию целиком.
pub async fn collect(db: &DatabaseConnection) -> Collected {
    let mut sink = Sink {
        units: Vec::new(),
        diagnostics: Vec::new(),
    };

    for surface in SURFACE_CATALOG {
        match surface.enumerator {
            Enumerator::ArticlesBusiness => articles(&mut sink, surface, DocKind::Business),
            Enumerator::ArticlesApp => articles(&mut sink, surface, DocKind::App),
            Enumerator::ArticlesGenerated => articles(&mut sink, surface, DocKind::Generated),
            Enumerator::Vocabulary => vocabulary(&mut sink, surface),
            Enumerator::Skills => skills(&mut sink, surface),
            Enumerator::CorePrompt => core_prompt(&mut sink, surface),
            Enumerator::QualityChecks => quality_checks(&mut sink, surface),
            Enumerator::Plugins => plugins(&mut sink, surface, db).await,
            Enumerator::Processes => processes(&mut sink, surface, db).await,
            Enumerator::Actions => actions(&mut sink, surface),
            Enumerator::Entities => entities(&mut sink, surface),
            Enumerator::DataSources => data_sources(&mut sink, surface),
            Enumerator::ChartOfAccounts => chart_of_accounts(&mut sink, surface),
            Enumerator::UiScopes => ui_scopes(&mut sink, surface),
            Enumerator::ScheduledTasks => scheduled_tasks(&mut sink, surface),
            Enumerator::ToolCatalog => tool_catalog(&mut sink, surface),
            Enumerator::ToolHelp => tool_help(&mut sink, surface),
            Enumerator::ExternalRoutes => external_routes(&mut sink, surface),
            Enumerator::InstanceData => instance_data(&mut sink, surface, db).await,
        }
    }

    let surfaces = build_surfaces(&sink.units);
    let summary = super::summary::build(&sink.units, &surfaces);
    Collected {
        units: sink.units,
        surfaces,
        summary,
        diagnostics: sink.diagnostics,
    }
}

/// Реестр со счётчиками и фактической достижимостью.
fn build_surfaces(units: &[KnowledgeUnitDto]) -> Vec<SurfaceDto> {
    SURFACE_CATALOG
        .iter()
        .map(|surface| {
            let own: Vec<&KnowledgeUnitDto> = units
                .iter()
                .filter(|unit| unit.surface_id == surface.surface_id)
                .collect();
            let stored_tokens = if surface.family == UnitFamily::Stored {
                Some(own.iter().filter_map(|unit| unit.tokens).sum())
            } else {
                None
            };
            SurfaceDto {
                surface_id: surface.surface_id.to_string(),
                label: surface.label.to_string(),
                family: surface.family,
                origin: surface.origin,
                storage_form: surface.storage_form,
                editor: surface.editor,
                scope: surface.scope,
                channel: surface.channel,
                code_role: surface.code_role,
                reachability_declared: surface.reachability_declared,
                reachability_effective: tool_map::effective_reachability(
                    surface.surface_id,
                    surface.reachability_declared,
                ),
                enumerated_by: surface.enumerated_by.to_string(),
                note: surface.note.to_string(),
                tools: tool_map::tools_for_surface(surface.surface_id)
                    .into_iter()
                    .map(str::to_string)
                    .collect(),
                unit_count: own.len(),
                stored_tokens,
            }
        })
        .collect()
}

// ─── статьи и словарь ────────────────────────────────────────────────────────

/// Статьи одного корпуса.
///
/// Единственная поверхность, где почти всё уже посчитано: `KnowledgeBase` при
/// загрузке разбирает frontmatter, приводит теги к словарю, считает токены и
/// возраст. Здесь остаётся перевести это на язык осей.
fn articles(sink: &mut Sink, surface: &SurfaceDef, kind: DocKind) {
    let kb = crate::shared::llm::knowledge_base::kb_read();
    let metrics = crate::shared::llm::kb_metrics::snapshot();

    for doc in kb.all_docs().into_iter().filter(|doc| doc.kind == kind) {
        let mut unit = sink.unit(surface, "article", &doc.id, &doc.title);
        unit.subtitle = doc.summary.clone();
        unit.source_ref = doc.source_path.clone();
        unit.bytes = Some(doc.content.len() as u32);
        unit.tokens = Some(doc.token_cost);
        unit.updated = doc.updated.map(|date| date.to_string());
        unit.staleness_pct = doc.staleness_pct();
        unit.tags = doc.canonical_tags.clone();
        unit.lifecycle = article_lifecycle(doc);

        if let Some(row) = metrics.get(&doc.metrics_key()) {
            unit.search_hits = row.search_hits;
            unit.read_hits = row.read_hits;
            unit.cited_hits = row.cited_hits;
        }

        // Инварианты §7, которые до сих пор проверялись только на запись через
        // инструмент: статья, положенная в каталог руками или генератором, их
        // не проходила вовсе.
        if doc.summary.trim().is_empty() {
            unit.issues
                .push("нет summary — поиск отдаёт статью без аннотации".into());
        }
        if doc.content.trim().len() < 400 {
            unit.issues.push("тело короче 400 символов".into());
        }
        for tag in &doc.unknown_tags {
            unit.issues.push(format!("тег вне словаря: {tag}"));
        }
        for anchor in &doc.unknown_anchors {
            unit.issues
                .push(format!("якорь вне реестра объектов: {anchor}"));
        }
        for link in kb.dangling_links_of(doc) {
            unit.issues.push(format!("висячая ссылка related: {link}"));
        }

        sink.units.push(unit);
    }
}

/// Жизненный цикл статьи: статус автора, а поверх него — истёкший срок годности.
///
/// Порядок важен: `deprecated` остаётся `deprecated`, даже если протух, —
/// статью уже вывели из обращения, и требовать её обновления бессмысленно.
fn article_lifecycle(doc: &crate::shared::llm::knowledge_base::KnowledgeDoc) -> Lifecycle {
    use crate::shared::llm::knowledge_base::KbStatus;
    match doc.status {
        KbStatus::Draft => Lifecycle::Draft,
        KbStatus::Deprecated => Lifecycle::Deprecated,
        _ if doc.staleness_pct().is_some_and(|pct| pct > 100) => Lifecycle::Stale,
        _ => Lifecycle::Active,
    }
}

/// Словарь тегов — одна единица, а не девяносто.
///
/// Его `##` это записи словаря, а не разделы документа; отдаётся он целиком
/// своим инструментом, значит и единица бюджета у него одна.
fn vocabulary(sink: &mut Sink, surface: &SurfaceDef) {
    let kb = crate::shared::llm::knowledge_base::kb_read();
    let terms = kb.vocabulary().len();
    if terms == 0 {
        sink.diagnostics
            .push("словарь тегов пуст или не найден — теги статей не к чему приводить".into());
        return;
    }
    let mut unit = sink.unit(surface, "vocabulary", "_vocabulary", "Словарь тегов");
    unit.subtitle = format!("{terms} канонических тегов");
    sink.units.push(unit);
}

// ─── навыки, промпт, проверки ────────────────────────────────────────────────

/// Навыки. Единица — навык целиком, вместе с ресурсами пакета.
///
/// Дробить его на ресурсы нельзя по той же причине, по которой навыкам не
/// заводят якоря разделов: навык доставляется целиком по контракту, и половина
/// инструкции — это не половина знания, а сломанная инструкция.
fn skills(sink: &mut Sink, surface: &SurfaceDef) {
    let snapshot = crate::shared::llm::skills::snapshot();
    // Каталог инструментов собирается один раз на все навыки: внутри цикла это
    // была бы двадцатикратная пересборка восьмидесяти восьми определений.
    let universe: BTreeSet<String> = crate::shared::llm::skills::tool_universe()
        .into_iter()
        .map(|def| def.name)
        .collect();
    for skill in snapshot.skills.iter() {
        let mut unit = sink.unit(surface, "skill", &skill.id, &skill.title);
        unit.subtitle = skill.description.clone();
        unit.source_ref = skill
            .package_root
            .as_ref()
            .map(|path| path.display().to_string());

        let bytes: usize = skill.prompt.len()
            + skill
                .resources
                .iter()
                .map(|resource| resource.content.len())
                .sum::<usize>();
        let tokens: u32 = estimate_tokens(&skill.prompt)
            + skill
                .resources
                .iter()
                .map(|resource| estimate_tokens(&resource.content))
                .sum::<u32>();
        unit.bytes = Some(bytes as u32);
        unit.tokens = Some(tokens);

        // Имя инструмента, которого нет в каталоге, молча выбрасывается при
        // сборке набора для модели: навык объявляет умение, которого не будет.
        for name in &skill.tool_names {
            if !universe.contains(name) {
                unit.issues
                    .push(format!("инструмент '{name}' отсутствует в каталоге"));
            }
        }
        sink.units.push(unit);
    }
}

/// Промпт ядра — единственная поверхность, которая в контексте всегда.
fn core_prompt(sink: &mut Sink, surface: &SurfaceDef) {
    let text = crate::shared::llm::skills::core_prompt();
    let mut unit = sink.unit(surface, "prompt", "core", "Базовый системный промпт");
    unit.subtitle = "Едет в каждый запрос целиком".into();
    unit.bytes = Some(text.len() as u32);
    unit.tokens = Some(estimate_tokens(text));
    sink.units.push(unit);
}

/// Quality-проверки: определения, а не прогоны.
fn quality_checks(sink: &mut Sink, surface: &SurfaceDef) {
    let snapshot = crate::quality::registry::snapshot();
    for definition in snapshot.definitions.iter() {
        let mut unit = sink.unit(
            surface,
            "check",
            &definition.info.code,
            &definition.info.name,
        );
        unit.subtitle = definition.info.description.clone();
        unit.tags = vec![definition.info.category.clone(), definition.kind.clone()];
        sink.units.push(unit);
    }
    for message in &snapshot.diagnostics {
        sink.diagnostics
            .push(format!("реестр проверок качества: {message}"));
    }
}

// ─── плагины и Процессы: живут в БД ──────────────────────────────────────────

/// Плагины. Идентичность — `manifest.code`, а не UUID строки: при обновлении
/// базы из боевой копии UUID меняется, код остаётся.
async fn plugins(sink: &mut Sink, surface: &SurfaceDef, db: &DatabaseConnection) {
    match crate::plugins::repository::list_all(db).await {
        Ok(items) => {
            for plugin in items {
                let manifest = &plugin.bundle.manifest;
                let mut unit = sink.unit(surface, "plugin", &manifest.code, &manifest.title);
                unit.subtitle = manifest.description.clone().unwrap_or_default();
                unit.source_ref = Some(format!("plugin#{}", plugin.id));
                unit.updated = Some(plugin.updated_at.to_rfc3339());
                unit.lifecycle = if plugin.is_enabled {
                    Lifecycle::Active
                } else {
                    Lifecycle::Draft
                };
                sink.units.push(unit);
            }
        }
        Err(error) => sink.complain(surface, error),
    }
}

/// Процессы и Этапы — две разновидности единиц одной поверхности.
///
/// Разные префиксы обязательны: `pr0001` и `st0001` живут в разных таблицах и
/// имеют собственные пространства имён.
async fn processes(sink: &mut Sink, surface: &SurfaceDef, db: &DatabaseConnection) {
    match crate::processes::repository::list_process_head_records(db).await {
        Ok(items) => {
            for record in items {
                let title = record.definition.manifest.title.clone();
                let mut unit = sink.unit(surface, "process", &record.code, &title);
                unit.source_ref = Some(format!("process#{}", record.code));
                sink.units.push(unit);
            }
        }
        Err(error) => sink.complain(surface, error),
    }
    match crate::processes::repository::list_stage_head_records(db).await {
        Ok(items) => {
            for record in items {
                let title = record.definition.manifest.title.clone();
                let mut unit = sink.unit(surface, "stage", &record.code, &title);
                unit.source_ref = Some(format!("stage#{}", record.code));
                sink.units.push(unit);
            }
        }
        Err(error) => sink.complain(surface, error),
    }
}

// ─── реестры кода ────────────────────────────────────────────────────────────

/// Действия — каталог операций с побочным эффектом.
fn actions(sink: &mut Sink, surface: &SurfaceDef) {
    for info in crate::processes::actions::list() {
        let mut unit = sink.unit(surface, "action", info.name, info.title);
        unit.subtitle = info.description.to_string();
        unit.source_ref = Some(format!("processes/actions/{}.rs", info.name));
        if !info.reversible {
            unit.tags.push("необратимое".into());
        }
        sink.units.push(unit);
    }
}

/// Сущности реестра метаданных — самая крупная вычисляемая поверхность.
fn entities(sink: &mut Sink, surface: &SurfaceDef) {
    for entry in contracts::shared::metadata::ALL_ENTITIES {
        let meta = entry.meta;
        let mut unit = sink.unit(surface, "entity", meta.entity_name, meta.ui.element_name);
        unit.subtitle = format!("{} полей", entry.fields.len());
        unit.source_ref = meta.table_name.map(str::to_string);
        sink.units.push(unit);
    }
}

/// Источники данных `dsXX` и `dvXX` — один перечень, две роли.
fn data_sources(sink: &mut Sink, surface: &SurfaceDef) {
    for item in crate::shared::data_access::list_sources(None) {
        let mut unit = sink.unit(surface, "source", &item.id, &item.name);
        unit.subtitle = item.description.clone();
        unit.source_ref = item.table.clone();
        sink.units.push(unit);
    }
}

/// План счетов и виды оборотов — две разновидности одной поверхности.
fn chart_of_accounts(sink: &mut Sink, surface: &SurfaceDef) {
    for account in crate::general_ledger::ACCOUNT_REGISTRY {
        let mut unit = sink.unit(surface, "account", account.code, account.name);
        unit.subtitle = account.description.to_string();
        sink.units.push(unit);
    }
    for turnover in crate::general_ledger::turnover_registry::TURNOVER_CLASSES {
        let mut unit = sink.unit(surface, "turnover", turnover.code, turnover.name);
        unit.subtitle = turnover.description.to_string();
        sink.units.push(unit);
    }
}

/// Разделы интерфейса. Они же области доступа — объект один.
fn ui_scopes(sink: &mut Sink, surface: &SurfaceDef) {
    for scope in crate::system::access::scope_catalog::SCOPE_CATALOG {
        let mut unit = sink.unit(surface, "ui_scope", scope.scope_id, scope.label);
        unit.subtitle = scope.description.to_string();
        unit.tags = vec![scope.category.to_string()];
        sink.units.push(unit);
    }
}

/// Типы регламентных заданий — типы, а не запуски.
///
/// Реестр наполняется при старте приложения независимо от того, включён ли
/// планировщик. В тестах его нет вовсе, и это не ошибка сбора.
fn scheduled_tasks(sink: &mut Sink, surface: &SurfaceDef) {
    let Some(registry) = crate::system::tasks::registry::get_global_registry() else {
        sink.diagnostics.push(
            "реестр регламентных заданий не инициализирован (обычное дело вне рантайма)".into(),
        );
        return;
    };
    for meta in registry.list_metadata() {
        let mut unit = sink.unit(surface, "task", meta.task_type, meta.display_name);
        unit.subtitle = meta.description.to_string();
        sink.units.push(unit);
    }
}

/// Каталог инструментов — поверхность, описывающая сама себя.
///
/// Здесь же проставляется её собственный дефект: инструмент, до которого не
/// дотянуться ни ядром, ни навыком, — мёртвый код.
fn tool_catalog(sink: &mut Sink, surface: &SurfaceDef) {
    let defects = tool_map::chain_defects();
    let orphans: BTreeSet<&String> = defects.orphan_tools.iter().collect();
    for definition in crate::shared::llm::skills::tool_universe() {
        let mut unit = sink.unit(surface, "tool", &definition.name, &definition.name);
        unit.subtitle = first_line(&definition.description);
        let yielded = tool_map::surfaces_of(&definition.name);
        if yielded.is_empty() {
            unit.tags.push("без знания".into());
        } else {
            unit.tags = yielded.iter().map(|s| s.to_string()).collect();
        }
        if orphans.contains(&definition.name) {
            unit.lifecycle = Lifecycle::Orphaned;
            unit.issues
                .push("не в ядре и ни в одном навыке — дотянуться нечем".into());
        }
        sink.units.push(unit);
    }
}

/// Встроенная справка инструментов — примеры, шаблоны, UI-контракты.
///
/// Строки в реестре §2 у неё не было: она обнаружилась при разметке
/// инструментов. Знание, которое стоит токенов и нигде не считалось.
fn tool_help(sink: &mut Sink, surface: &SurfaceDef) {
    for tool in tool_map::tools_for_surface(surface.surface_id) {
        let mut unit = sink.unit(surface, "help", tool, tool);
        unit.subtitle = "Справка, отдаваемая инструментом".into();
        sink.units.push(unit);
    }
}

/// Маршруты внешнего API — единственная поверхность, закрытая для чата.
fn external_routes(sink: &mut Sink, surface: &SurfaceDef) {
    const OPENAPI: &str = include_str!("../api/handlers/ext_openapi.json");
    let parsed: serde_json::Value = match serde_json::from_str(OPENAPI) {
        Ok(value) => value,
        Err(error) => return sink.complain(surface, error),
    };
    let Some(paths) = parsed.get("paths").and_then(|value| value.as_object()) else {
        return sink.complain(surface, "в контракте нет раздела paths");
    };
    for (path, spec) in paths {
        let title = spec
            .get("get")
            .and_then(|op| op.get("summary"))
            .and_then(|value| value.as_str())
            .unwrap_or(path);
        let mut unit = sink.unit(surface, "ext_route", path, title);
        unit.subtitle = path.clone();
        sink.units.push(unit);
    }
}

/// Данные экземпляра. Единица — таблица, а не строка.
///
/// Строк в них миллионы; посчитав строками, мы получили бы число, в котором
/// утонет всё остальное, и метрику, которая растёт от каждого импорта. Профиль
/// читается готовым — пересчитывать его здесь нельзя, это обход всех таблиц.
async fn instance_data(sink: &mut Sink, surface: &SurfaceDef, db: &DatabaseConnection) {
    use sea_orm::{DatabaseBackend, FromQueryResult, Statement};

    #[derive(FromQueryResult)]
    struct ProfileRow {
        table_name: String,
        entity_index: String,
        row_count: i64,
    }

    let statement = Statement::from_string(
        DatabaseBackend::Sqlite,
        "SELECT table_name, entity_index, row_count FROM sys_data_profile ORDER BY table_name",
    );
    match ProfileRow::find_by_statement(statement).all(db).await {
        Ok(rows) => {
            for row in rows {
                let mut unit = sink.unit(surface, "table", &row.table_name, &row.table_name);
                unit.subtitle = format!("{} строк, сущность {}", row.row_count, row.entity_index);
                unit.source_ref = Some(row.table_name);
                sink.units.push(unit);
            }
        }
        Err(error) => sink.complain(surface, error),
    }
}

/// Первая содержательная строка описания — в таблицу помещается только она.
fn first_line(text: &str) -> String {
    text.lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or_default()
        .chars()
        .take(160)
        .collect()
}

/// Единицы, сгруппированные по поверхности, — для сводки и отладки.
pub fn count_by_surface(units: &[KnowledgeUnitDto]) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for unit in units {
        *counts.entry(unit.surface_id.clone()).or_insert(0) += 1;
    }
    counts
}
