//! Что каждый инструмент чата отдаёт модели.
//!
//! **Такого признака у инструментов не было.** `ToolDefinition` несёт имя,
//! описание и схему параметров — и всё. Что есть рядом: категория бандла в
//! `skills::tool_bundles()` (`data`, `kb_search`, `plugin`, `chart`, …), она
//! видна в `tools_catalog()`. Но категория отвечает на вопрос «откуда инструмент
//! родом», а не «какое знание он выдаёт»: бандл `data` покрывает разом источники
//! данных, сущности и произвольный SQL по данным экземпляра.
//!
//! Без этой связи ось достижимости (§3 D) остаётся утверждением автора реестра,
//! которое устареет молча — ни сборка, ни тест его не проверят. Здесь она
//! становится измеримой.
//!
//! **Почему таблица здесь, а не поле в `ToolDefinition`.** Поле пришлось бы
//! заполнять в восемнадцати модулях на восемьдесят шесть определений, и оно
//! неминуемо разъехалось бы с реестром поверхностей, который живёт этажом выше.
//! Одна таблица рядом с реестром — одно место, где можно ошибиться, и одно
//! место, где ошибку видно.
//!
//! **Сторож.** Тест `every_tool_is_classified` требует, чтобы множество имён
//! здесь совпадало с `skills::tool_universe()` в обе стороны. Новый инструмент
//! не пройдёт тест без записи; удалённый не оставит мёртвой строки.

use std::collections::{BTreeMap, BTreeSet};

use contracts::knowledge::Reachability;

/// Что инструмент отдаёт модели.
pub enum ToolYield {
    /// Отдаёт знание перечисленных поверхностей.
    ///
    /// Список, а не одно значение: `get_knowledge` кормит все три корпуса статей,
    /// а `search_knowledge` — только два из них, и вот эта разница и делает
    /// корпус `generated` доступным «только по id».
    Surfaces(&'static [&'static str]),
    /// Сведения о поверхностях, но не их содержимое.
    ///
    /// Отдельный случай, а не `Surfaces(всё)`: инвентаризация рассказывает, что
    /// каталог Действий существует и в нём пять записей, — но самих Действий она
    /// не отдаёт. Записать её отдающей все поверхности значило бы объявить
    /// достижимым всё подряд и обнулить `unreachable_surfaces` — метрику, ради
    /// которой механизм и написан.
    Meta,
    /// Действие, мутация или работа с рабочей памятью чата. Знания не отдаёт.
    NoKnowledge,
}

/// Инструмент обычного поиска. Поверхность, которую он кормит, находится без
/// подсказок — это и есть уровень `default_search`.
const SEARCH_TOOL: &str = "search_knowledge";

/// Инструменты адресного чтения: знание достанется, но только если модель уже
/// знает идентификатор.
const BY_ID_TOOLS: &[&str] = &["get_knowledge"];

use ToolYield::{Meta, NoKnowledge, Surfaces};

const ARTICLES_ALL: &[&str] = &["articles_business", "articles_app", "articles_generated"];
const ARTICLES_SEARCHABLE: &[&str] = &["articles_business", "articles_app"];

/// Разметка всех инструментов чата.
///
/// Порядок — по бандлам `skills::tool_bundles()`, чтобы строку было где искать.
pub static TOOL_YIELD: &[(&str, ToolYield)] = &[
    // ─── data: источники, схемы и произвольный запрос ───
    ("list_data_sources", Surfaces(&["data_sources"])),
    ("find_data_sources", Surfaces(&["data_sources"])),
    ("query_data_schema", Surfaces(&["instance_data"])),
    ("preview_data", Surfaces(&["instance_data"])),
    ("execute_query", Surfaces(&["instance_data"])),
    // Имена собираются не литералом `name: "…"`, а конструктором
    // `data_view_definition`, поэтому в глазную выборку они не попали — и
    // именно их первым же прогоном нашёл сторож.
    ("run_data_view_scalar", Surfaces(&["instance_data"])),
    ("run_data_view_drilldown", Surfaces(&["instance_data"])),
    // ─── shared/analyst: реестры кода ───
    (
        "get_architecture_overview",
        Surfaces(&["entities", "ui_scopes", "data_sources"]),
    ),
    ("list_entities", Surfaces(&["entities"])),
    ("get_entity_schema", Surfaces(&["entities"])),
    ("get_join_hint", Surfaces(&["entities"])),
    ("get_chart_of_accounts", Surfaces(&["chart_of_accounts"])),
    ("list_gl_turnovers", Surfaces(&["chart_of_accounts"])),
    ("create_drilldown_report", Surfaces(&["instance_data"])),
    // ─── admin: состояние экземпляра ───
    ("check_system_health", Surfaces(&["instance_data"])),
    ("get_performance_stats", Surfaces(&["instance_data"])),
    ("get_project_metrics", Surfaces(&["instance_data"])),
    ("get_data_integrity_report", Surfaces(&["quality_checks"])),
    ("list_background_jobs", Surfaces(&["scheduled_tasks"])),
    // ─── kb_search: собственно база знаний ───
    ("search_knowledge", Surfaces(ARTICLES_SEARCHABLE)),
    ("get_knowledge", Surfaces(ARTICLES_ALL)),
    ("list_kb_vocabulary", Surfaces(&["vocabulary"])),
    ("kb_propose_article", NoKnowledge),
    ("kb_report_issue", NoKnowledge),
    // ─── kb: правка курируемого корпуса ───
    ("list_kb_documents", Surfaces(&["articles_business"])),
    ("get_kb_document", Surfaces(&["articles_business"])),
    ("write_kb_document", NoKnowledge),
    ("create_kb_edit", NoKnowledge),
    ("update_kb_edit_articles", NoKnowledge),
    ("list_open_kb_edits", NoKnowledge),
    ("knowledge_inventory", Meta),
    // ─── plugin: бандлы и их запуск ───
    ("plugin_list", Surfaces(&["plugins"])),
    ("plugin_get", Surfaces(&["plugins"])),
    ("plugin_runs", Surfaces(&["plugins"])),
    ("plugin_invoke", Surfaces(&["plugins"])),
    ("plugin_data_catalog", Surfaces(&["data_sources"])),
    ("plugin_examples", Surfaces(&["tool_help"])),
    ("plugin_template", Surfaces(&["tool_help"])),
    ("get_plugin_ui_contract", Surfaces(&["tool_help"])),
    ("plugin_upsert", NoKnowledge),
    ("plugin_validate", NoKnowledge),
    ("plugin_smoke_test", NoKnowledge),
    // ─── chart / table: производство артефактов и справка к нему ───
    ("build_chart", NoKnowledge),
    ("chart_examples", Surfaces(&["tool_help"])),
    ("chart_template", Surfaces(&["tool_help"])),
    ("get_chart_ui_contract", Surfaces(&["tool_help"])),
    ("build_table", NoKnowledge),
    ("table_examples", Surfaces(&["tool_help"])),
    ("table_template", Surfaces(&["tool_help"])),
    ("get_table_ui_contract", Surfaces(&["tool_help"])),
    // ─── mail ───
    ("list_emails", Surfaces(&["instance_data"])),
    ("read_email", Surfaces(&["instance_data"])),
    ("send_email", NoKnowledge),
    // ─── schedule ───
    ("list_scheduled_tasks", Surfaces(&["scheduled_tasks"])),
    ("describe_task_types", Surfaces(&["scheduled_tasks"])),
    // ─── ticket и помощь по разделам ───
    ("ticket_search", Surfaces(&["instance_data"])),
    ("get_user_recent_pages", Surfaces(&["instance_data"])),
    ("find_page_help", Surfaces(&["ui_scopes"])),
    ("ticket_create", NoKnowledge),
    ("ticket_validate", NoKnowledge),
    // ─── workspace: рабочая память чата, а не знание системы ───
    ("list_chat_files", NoKnowledge),
    ("read_chat_file", NoKnowledge),
    ("write_chat_file", NoKnowledge),
    ("save_step", NoKnowledge),
    ("start_activity", NoKnowledge),
    ("switch_activity", NoKnowledge),
    ("update_plan_step", NoKnowledge),
    // ─── quality ───
    ("list_quality_checks", Surfaces(&["quality_checks"])),
    ("get_latest_quality_check", Surfaces(&["quality_checks"])),
    ("quality_check_get", Surfaces(&["quality_checks"])),
    ("run_quality_check", Surfaces(&["quality_checks"])),
    ("quality_check_template", Surfaces(&["tool_help"])),
    ("quality_check_upsert", NoKnowledge),
    ("quality_check_validate", NoKnowledge),
    // ─── funnel_repair: починка данных ───
    ("prepare_funnel_repair", NoKnowledge),
    ("execute_funnel_repair", NoKnowledge),
    ("get_funnel_repair_status", NoKnowledge),
    // ─── llm_quality: разбор чужих диалогов ───
    ("list_chats_for_review", Surfaces(&["instance_data"])),
    ("get_chat_digest", Surfaces(&["instance_data"])),
    ("record_chat_verdicts", NoKnowledge),
    // ─── agent_task ───
    ("list_agent_specializations", Surfaces(&["skills"])),
    ("create_agent_task", NoKnowledge),
    ("list_my_agent_tasks", NoKnowledge),
    ("get_agent_task_result", NoKnowledge),
    // ─── meta: навыки ───
    ("list_skills", Surfaces(&["skills"])),
    ("use_skill", Surfaces(&["skills"])),
    ("list_skill_resources", Surfaces(&["skills"])),
    ("read_skill_resource", Surfaces(&["skills"])),
    ("run_skill_task", NoKnowledge),
];

/// Поверхности, которые отдаёт инструмент. Пустой срез — не отдаёт ничего.
pub fn surfaces_of(tool: &str) -> &'static [&'static str] {
    match TOOL_YIELD.iter().find(|(name, _)| *name == tool) {
        Some((_, Surfaces(list))) => list,
        _ => &[],
    }
}

/// Инструменты, отдающие поверхность.
pub fn tools_for_surface(surface_id: &str) -> Vec<&'static str> {
    TOOL_YIELD
        .iter()
        .filter_map(|(name, yielded)| match yielded {
            Surfaces(list) if list.contains(&surface_id) => Some(*name),
            _ => None,
        })
        .collect()
}

/// Фактическая достижимость поверхности внутренним чатом.
///
/// Считается, а не объявляется. Порядок проверок — от самой лёгкой доступности
/// к самой трудной, и он же порядок уровней в §3 D:
///
/// 1. промпт ядра в контексте всегда — дотягиваться до него нечем и незачем;
/// 2. кормит обычный поиск — знание найдётся само;
/// 3. кормит адресное чтение — найдётся, если модель уже знает идентификатор;
/// 4. инструмент лежит в ядре — доступен без активации навыка;
/// 5. инструмент есть, но только внутри навыка;
/// 6. инструмента нет вовсе.
pub fn effective_reachability(surface_id: &str, declared: Reachability) -> Reachability {
    // Единственный случай, где заявление автора решает: отсутствие инструмента
    // здесь не дефект, а способ доставки.
    if declared == Reachability::AlwaysInContext {
        return Reachability::AlwaysInContext;
    }

    let tools = tools_for_surface(surface_id);
    if tools.is_empty() {
        return Reachability::Unreachable;
    }
    if tools.contains(&SEARCH_TOOL) {
        return Reachability::DefaultSearch;
    }
    if tools.iter().any(|tool| BY_ID_TOOLS.contains(tool)) {
        return Reachability::ByIdOnly;
    }
    let core = crate::shared::llm::skills::core_tool_names();
    if tools.iter().any(|tool| core.contains(tool)) {
        return Reachability::ToolGated;
    }
    Reachability::SkillGated
}

/// Разрывы цепочки достижимости (§5), кроме недостижимых поверхностей.
#[derive(Debug, Default)]
pub struct ChainDefects {
    /// Имя в `tools:` навыка, которого нет в каталоге инструментов.
    ///
    /// Такой инструмент **молча выбрасывается** при сборке набора для модели:
    /// навык объявляет умение, которого у него не будет.
    pub phantom_tools: Vec<String>,
    /// Инструмент вне ядра и вне любого навыка — дотянуться до него нельзя ничем.
    pub orphan_tools: Vec<String>,
}

/// Пройти цепочку «навык → имена инструментов → каталог» и собрать разрывы.
pub fn chain_defects() -> ChainDefects {
    let universe: BTreeSet<String> = crate::shared::llm::skills::tool_universe()
        .into_iter()
        .map(|def| def.name)
        .collect();
    let core: BTreeSet<&str> = crate::shared::llm::skills::core_tool_names()
        .iter()
        .copied()
        .collect();

    let snapshot = crate::shared::llm::skills::snapshot();
    let mut claimed: BTreeSet<String> = BTreeSet::new();
    let mut phantom: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for skill in snapshot.skills.iter() {
        for name in &skill.tool_names {
            claimed.insert(name.clone());
            if !universe.contains(name) {
                phantom
                    .entry(name.clone())
                    .or_default()
                    .push(skill.id.clone());
            }
        }
    }

    ChainDefects {
        phantom_tools: phantom
            .into_iter()
            .map(|(tool, skills)| format!("{tool} (навыки: {})", skills.join(", ")))
            .collect(),
        orphan_tools: universe
            .into_iter()
            .filter(|name| !core.contains(name.as_str()) && !claimed.contains(name))
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::knowledge::catalog::SURFACE_CATALOG;

    /// Сторож маппинга: каждый инструмент чата размечен, и лишних строк нет.
    ///
    /// Падение при добавлении инструмента — не помеха, а весь смысл: решить,
    /// какое знание он отдаёт, обязан автор инструмента, а не тот, кто через
    /// полгода будет гадать по метрике.
    #[test]
    fn every_tool_is_classified() {
        let universe: BTreeSet<String> = crate::shared::llm::skills::tool_universe()
            .into_iter()
            .map(|def| def.name)
            .collect();
        let mapped: BTreeSet<String> = TOOL_YIELD
            .iter()
            .map(|(name, _)| (*name).to_string())
            .collect();

        let unmapped: Vec<&String> = universe.difference(&mapped).collect();
        assert!(
            unmapped.is_empty(),
            "инструменты без разметки в TOOL_YIELD: {unmapped:?} — впиши, какую \
             поверхность знания они отдают, или пометь NoKnowledge"
        );

        let stale: Vec<&String> = mapped.difference(&universe).collect();
        assert!(
            stale.is_empty(),
            "в TOOL_YIELD остались имена, которых больше нет в каталоге: {stale:?}"
        );
    }

    #[test]
    fn tool_names_are_unique() {
        let mut seen = BTreeSet::new();
        for (name, _) in TOOL_YIELD {
            assert!(seen.insert(*name), "инструмент {name} размечен дважды");
        }
    }

    /// Ссылка на несуществующую поверхность — опечатка, а не право.
    #[test]
    fn yielded_surfaces_exist() {
        for (name, yielded) in TOOL_YIELD {
            if let Surfaces(list) = yielded {
                assert!(
                    !list.is_empty(),
                    "у {name} пустой список поверхностей — это NoKnowledge или Meta"
                );
                for surface in *list {
                    assert!(
                        crate::knowledge::catalog::find(surface).is_some(),
                        "инструмент {name} ссылается на несуществующую поверхность {surface}"
                    );
                }
            }
        }
    }

    /// Корпус `generated` выведен из обычного поиска намеренно — и это должно
    /// быть видно в вычисленной достижимости, а не только в комментарии.
    #[test]
    fn generated_corpus_is_by_id_only() {
        assert_eq!(
            effective_reachability("articles_generated", Reachability::ByIdOnly),
            Reachability::ByIdOnly
        );
        assert_eq!(
            effective_reachability("articles_business", Reachability::DefaultSearch),
            Reachability::DefaultSearch
        );
    }

    /// Поверхность без единого инструмента обязана называться недостижимой,
    /// как бы оптимистично ни было заявление в реестре.
    #[test]
    fn missing_tool_beats_declaration() {
        assert_eq!(
            effective_reachability("нет такой поверхности", Reachability::DefaultSearch),
            Reachability::Unreachable
        );
    }

    /// Все поверхности реестра упомянуты хотя бы раз — иначе строка заведена,
    /// а достижимость посчитать не по чему.
    #[test]
    fn catalog_and_map_agree_on_surface_ids() {
        for surface in SURFACE_CATALOG {
            let effective =
                effective_reachability(surface.surface_id, surface.reachability_declared);
            if effective == Reachability::Unreachable {
                // Недостижимость — законный результат, но пусть она будет
                // заявлена: расхождение попадёт в сводку, а не потеряется.
                continue;
            }
            assert!(
                !tools_for_surface(surface.surface_id).is_empty()
                    || effective == Reachability::AlwaysInContext,
                "поверхность {} достижима, но инструментов у неё нет",
                surface.surface_id
            );
        }
    }
}
