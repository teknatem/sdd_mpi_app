//! Инструмент чата над инвентаризацией.
//!
//! **По умолчанию отдаются агрегаты, детали — по явному запросу.** Урок
//! `ARCHITECTURE.md` записан кровью: восемнадцать тысяч токенов одним куском
//! съедают контекст и не отвечают ни на один вопрос. Полный снимок здесь — это
//! четыреста единиц; в свёрнутом виде он укладывается в полторы тысячи токенов
//! и говорит ровно то, ради чего инвентаризация затевалась: чего в системе
//! много, чего мало и до чего не дотянуться.
//!
//! Фильтры повторяют оси классификации один в один — это не совпадение, а
//! смысл: если ось нельзя выбрать фильтром, она не ось, а комментарий.

use serde_json::{json, Value};

use crate::shared::llm::types::ToolDefinition;

pub fn tool_definitions() -> Vec<ToolDefinition> {
    vec![ToolDefinition {
        name: "knowledge_inventory".into(),
        description: "Инвентаризация знаний системы: сколько знания есть и сколько из него \
                      достижимо чату. Считает НЕ ТОЛЬКО статьи, но и сущности реестра, \
                      источники данных, план счетов, разделы UI, навыки, проверки качества, \
                      плагины, Процессы, Действия и каталог инструментов. \
                      БЕЗ ПАРАМЕТРОВ отдаёт сводку: объём по осям, недостижимые поверхности, \
                      мёртвые инструменты, проблемные статьи. Детали — только по явному \
                      фильтру, иначе ответ не поместится в контекст. \
                      Отвечает на вопросы вида «что вообще есть в системе», «почему чат не \
                      видит X», «какие статьи никто не читает», «сколько токенов стоит корпус»."
            .into(),
        parameters: json!({
            "type": "object",
            "properties": {
                "surface": {
                    "type": "string",
                    "description": "Поверхность знания: articles_business, articles_app, \
                                    articles_generated, vocabulary, skills, core_prompt, \
                                    quality_checks, plugins, processes, actions, entities, \
                                    data_sources, chart_of_accounts, ui_scopes, scheduled_tasks, \
                                    tool_catalog, tool_help, external_routes, instance_data. \
                                    Полный список с описаниями — в ответе без параметров."
                },
                "family": {
                    "type": "string",
                    "enum": ["stored", "computed"],
                    "description": "Хранимая единица (есть размер и цена в токенах) или \
                                    вычисляемая (цена только у конкретного ответа)."
                },
                "origin": {
                    "type": "string",
                    "enum": ["code_embedded", "code_registry", "db_generated", "file_curated",
                             "db_live", "external_api"],
                    "description": "Где источник правды."
                },
                "scope": {
                    "type": "string",
                    "enum": ["application", "instance", "mixed", "external"],
                    "description": "Для какого контекста знание истинно: одинаково для всех \
                                    экземпляров приложения или относится к этой базе."
                },
                "reachability": {
                    "type": "string",
                    "enum": ["default_search", "by_id_only", "tool_gated", "skill_gated",
                             "always_in_context", "unreachable"],
                    "description": "Как до знания добирается чат. 'unreachable' — не добирается \
                                    никак: это дефект, а не свойство."
                },
                "lifecycle": {
                    "type": "string",
                    "enum": ["active", "draft", "deprecated", "stale", "orphaned"],
                    "description": "Состояние единицы. 'stale' — истёк срок годности знания."
                },
                "has_issues": {
                    "type": "boolean",
                    "description": "Только единицы с нарушениями инвариантов — готовый список работ."
                },
                "limit": {
                    "type": "integer",
                    "default": 40,
                    "description": "Сколько единиц вернуть при фильтре. Максимум 200."
                }
            },
            "required": []
        }),
    }]
}

/// Исполнить вызов инструмента.
pub async fn execute(db: &sea_orm::DatabaseConnection, args: &Value) -> Value {
    let filtered = args
        .as_object()
        .is_some_and(|map| map.keys().any(|key| key != "limit"));

    let inventory = match super::service::inventory(db).await {
        Ok(value) => value,
        Err(error) => return json!({ "error": format!("инвентаризация недоступна: {error}") }),
    };

    if !filtered {
        return overview(&inventory);
    }

    let limit = args
        .get("limit")
        .and_then(Value::as_u64)
        .unwrap_or(40)
        .clamp(1, 200) as usize;

    let matches: Vec<&contracts::knowledge::KnowledgeUnitDto> = inventory
        .units
        .iter()
        .filter(|unit| {
            matches_str(args, "surface", &unit.surface_id)
                && matches_str(args, "family", unit.family.as_str())
                && matches_str(args, "origin", unit.origin.as_str())
                && matches_str(args, "scope", unit.scope.as_str())
                && matches_str(args, "reachability", unit.reachability.as_str())
                && matches_str(args, "lifecycle", unit.lifecycle.as_str())
                && match args.get("has_issues").and_then(Value::as_bool) {
                    Some(true) => !unit.issues.is_empty(),
                    Some(false) => unit.issues.is_empty(),
                    None => true,
                }
        })
        .collect();

    let total = matches.len();
    json!({
        "total": total,
        "shown": matches.len().min(limit),
        "units": matches.iter().take(limit).map(|unit| json!({
            "unit_id": unit.unit_id,
            "title": unit.title,
            "surface": unit.surface_id,
            "reachability": unit.reachability.as_str(),
            "lifecycle": unit.lifecycle.as_str(),
            "tokens": unit.tokens,
            "read_hits": unit.read_hits,
            "issues": unit.issues,
        })).collect::<Vec<_>>(),
        "note": if total > limit {
            format!("показаны первые {limit} из {total}; сузь фильтр или подними limit")
        } else {
            String::new()
        },
    })
}

fn matches_str(args: &Value, key: &str, actual: &str) -> bool {
    match args.get(key).and_then(Value::as_str) {
        Some(wanted) => wanted == actual,
        None => true,
    }
}

/// Сводка — то, что отдаётся без параметров.
///
/// Здесь важно, чего в ней НЕТ: списка единиц. Модель, получившая четыреста
/// строк, потратит контекст и всё равно спросит уточнение; модель, получившая
/// сводку, задаст точный фильтр следующим вызовом.
fn overview(inventory: &contracts::knowledge::InventoryResponseDto) -> Value {
    let summary = &inventory.summary;
    json!({
        "snapshot": {
            "captured_at": inventory.snapshot.captured_at,
            "classifier_version": inventory.snapshot.classifier_version,
            "units": inventory.snapshot.unit_count,
        },
        "volume": {
            "stored_units": summary.stored_units,
            "computed_units": summary.computed_units,
            "stored_tokens": summary.stored_tokens,
            "articles": {
                "business": summary.articles_business,
                "app": summary.articles_app,
                "generated": summary.articles_generated,
            },
            "note": "Токены есть только у хранимых единиц. У вычисляемых единица бюджета — \
                     типичный ответ инструмента, и он не замерян; складывать их нельзя.",
        },
        "reachability": {
            "unreachable_surfaces": summary.unreachable_surfaces,
            "phantom_tools": summary.phantom_tools,
            "orphan_tools": summary.orphan_tools,
            "declared_vs_actual": summary.reachability_mismatches,
        },
        "health": {
            "dangling_links": summary.dangling_links,
            "unknown_anchors": summary.unknown_anchors,
            "unknown_tags": summary.unknown_tags,
            "stale_articles": summary.stale_articles,
            "drafts": summary.drafts,
            "deprecated": summary.deprecated,
            "orphaned_metrics": summary.orphaned_metrics,
            "duplicate_ids": summary.duplicate_ids,
            "oversized_docs_without_anchors": summary.oversized_docs,
        },
        "value": {
            "never_touched": summary.never_touched,
            "searched_not_read": summary.searched_not_read,
            "read_not_cited": summary.read_not_cited,
            "note": "Много поиска и мало чтения — плохой summary. Много чтения и мало \
                     цитирований — плохая статья.",
        },
        "surfaces": inventory.surfaces.iter().map(|surface| json!({
            "id": surface.surface_id,
            "label": surface.label,
            "family": surface.family.as_str(),
            "units": surface.unit_count,
            "tokens": surface.stored_tokens,
            "reachability": surface.reachability_effective.as_str(),
        })).collect::<Vec<_>>(),
        "next": "Детали — повторный вызов с фильтром: surface, reachability, lifecycle, \
                 has_issues. Без фильтра список единиц не отдаётся намеренно.",
    })
}
