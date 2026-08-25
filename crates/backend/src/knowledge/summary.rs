//! Сводка §6: числа и поимённые списки.
//!
//! Списки важнее чисел. «Три недостижимые поверхности» — повод пожать плечами;
//! «недостижимы Действия и маршруты внешнего API» — задача с исполнителем.
//! Поэтому всё, что можно назвать поимённо, называется поимённо, а числом
//! остаётся только то, где имён были бы сотни.
//!
//! Группа «Ценность» считается из трёх счётчиков, различающихся намеренно:
//! много поиска и мало чтения — плохой `summary`; много чтения и мало
//! цитирований — плохая статья; ноль везде — статья, которой не существует для
//! системы, чем бы она ни была для автора.

use std::collections::BTreeMap;

use contracts::knowledge::{
    InventorySummaryDto, KnowledgeUnitDto, Lifecycle, Reachability, Scope, SurfaceDto, UnitFamily,
};

/// Порог, за которым документ обязан иметь якоря разделов (`SEC-09`).
///
/// Берётся из `kb_tools`, а не задаётся здесь заново: это тот же порог, по
/// которому `get_knowledge` переключается с тела на оглавление. Две копии числа
/// разошлись бы при первой же калибровке, и проверка стала бы проверять не то,
/// что происходит на самом деле.
use crate::shared::llm::kb_tools::SECTIONED_READ_TOKENS;

pub fn build(units: &[KnowledgeUnitDto], surfaces: &[SurfaceDto]) -> InventorySummaryDto {
    let kb = crate::shared::llm::knowledge_base::kb_read();
    let mut summary = InventorySummaryDto {
        dangling_links: kb.dangling_links(),
        anchored_entities: kb.anchored_entity_count(),
        ..Default::default()
    };

    // ─── объём ───
    for unit in units {
        match unit.family {
            UnitFamily::Stored => {
                summary.stored_units += 1;
                summary.stored_bytes += unit.bytes.unwrap_or(0) as u64;
                summary.stored_tokens = summary
                    .stored_tokens
                    .saturating_add(unit.tokens.unwrap_or(0));
            }
            UnitFamily::Computed => summary.computed_units += 1,
        }
        match unit.surface_id.as_str() {
            "articles_business" => summary.articles_business += 1,
            "articles_app" => summary.articles_app += 1,
            "articles_generated" => summary.articles_generated += 1,
            _ => {}
        }
        match unit.lifecycle {
            Lifecycle::Draft => summary.drafts += 1,
            Lifecycle::Deprecated => summary.deprecated += 1,
            Lifecycle::Stale => summary.stale_articles += 1,
            _ => {}
        }
    }

    // ─── достижимость ───
    for surface in surfaces {
        if surface.reachability_effective == Reachability::Unreachable {
            summary
                .unreachable_surfaces
                .push(format!("{} ({})", surface.label, surface.surface_id));
        }
        // Расхождение заявленного с измеренным. Ловит ровно то, ради чего
        // заявленное вообще хранится: мы думали, что поверхность отдаётся так,
        // а инструмента под это нет — или наоборот, появился и не записан.
        if surface.reachability_declared != surface.reachability_effective {
            summary.reachability_mismatches.push(format!(
                "{}: заявлено «{}», фактически «{}»",
                surface.surface_id,
                surface.reachability_declared.label(),
                surface.reachability_effective.label()
            ));
        }
    }
    let defects = super::tool_map::chain_defects();
    summary.phantom_tools = defects.phantom_tools;
    summary.orphan_tools = defects.orphan_tools;

    // ─── связность ───
    for doc in kb.all_docs() {
        summary.unknown_anchors += doc.unknown_anchors.len();
        summary.unknown_tags += doc.unknown_tags.len();
        // SEC-09: документ дороже порога обязан иметь якоря у своих разделов.
        // Проверяется вычисляемым условием, а не списком файлов, — иначе список
        // пришлось бы вести руками и он бы отстал от первой же крупной правки.
        if doc.token_cost > SECTIONED_READ_TOKENS {
            let sections = crate::shared::llm::knowledge_base::outline(&doc.content);
            if !sections.is_empty() && sections.iter().any(|section| section.slug.is_none()) {
                summary.oversized_docs.push(format!(
                    "{} ({} токенов, разделы без якорей)",
                    doc.id, doc.token_cost
                ));
            }
        }
    }

    // ─── ценность ───
    for unit in units
        .iter()
        .filter(|unit| unit.unit_id.starts_with("article:"))
    {
        let touched = unit.search_hits + unit.read_hits + unit.cited_hits;
        if touched == 0 {
            summary.never_touched += 1;
        } else if unit.search_hits > 0 && unit.read_hits == 0 {
            summary.searched_not_read += 1;
        } else if unit.read_hits > 0 && unit.cited_hits == 0 {
            summary.read_not_cited += 1;
        }
    }

    // ─── гигиена ───
    let live_keys: std::collections::BTreeSet<String> =
        kb.all_docs().iter().map(|doc| doc.metrics_key()).collect();
    summary.orphaned_metrics = crate::shared::llm::kb_metrics::snapshot()
        .keys()
        .filter(|key| !live_keys.contains(*key))
        .count();

    // Одинаковый unit_id у двух единиц означает, что одна из них молча исчезла
    // из счёта. Ровно ради этого в идентификаторе обязателен префикс типа.
    let mut seen: BTreeMap<&str, usize> = BTreeMap::new();
    for unit in units {
        *seen.entry(unit.unit_id.as_str()).or_insert(0) += 1;
    }
    summary.duplicate_ids = seen
        .into_iter()
        .filter(|(_, count)| *count > 1)
        .map(|(id, count)| format!("{id} — {count} раза"))
        .collect();

    summary
}

/// Разрез по области применимости: сколько знания истинно только для этого
/// экземпляра, а сколько переезжает вместе с приложением.
pub fn by_scope(units: &[KnowledgeUnitDto]) -> BTreeMap<Scope, usize> {
    let mut counts = BTreeMap::new();
    for unit in units {
        *counts.entry(unit.scope).or_insert(0) += 1;
    }
    counts
}
