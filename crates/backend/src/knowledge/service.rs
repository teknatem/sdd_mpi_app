//! Сбор снимка и сборка ответа для страницы и инструмента чата.
//!
//! Разделение обязанностей простое: `collector` знает, где что лежит,
//! `repository` — как это хранить, а здесь склеивается снимок и решается, что
//! показать. Стоимость сбора держится в пределах десятков миллисекунд: статьи
//! уже в памяти, реестры — константы, из БД читаются три небольшие выборки.
//! Единственное дорогое место — профиль данных — здесь **не пересчитывается**,
//! а читается готовым.

use anyhow::Result;
use contracts::knowledge::{
    axes, FacetDto, FacetValueDto, InventoryCollectReportDto, InventoryResponseDto,
    InventorySnapshotDto, InventorySummaryDto, KnowledgeUnitDto, SurfaceDto, CLASSIFIER_VERSION,
};
use sea_orm::DatabaseConnection;

use super::{collector, repository};

/// Снять снимок и записать его.
pub async fn collect_and_store(
    db: &DatabaseConnection,
    trigger: &str,
) -> Result<InventoryCollectReportDto> {
    let started = std::time::Instant::now();
    let collected = collector::collect(db).await;
    let collect_ms = started.elapsed().as_millis() as i64;

    let snapshot = InventorySnapshotDto {
        id: uuid::Uuid::new_v4().to_string(),
        captured_at: chrono::Utc::now().to_rfc3339(),
        trigger: trigger.to_string(),
        classifier_version: CLASSIFIER_VERSION,
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        unit_count: collected.units.len(),
        surface_count: collected.surfaces.len(),
        stored_tokens: collected.summary.stored_tokens,
        collect_ms,
        diagnostics: collected.diagnostics.clone(),
    };

    repository::insert_snapshot(db, &snapshot, &collected.summary, &collected.units).await?;
    if let Err(error) = repository::prune(db, repository::KEEP_SNAPSHOTS).await {
        tracing::warn!("[knowledge] старые снимки не почищены: {error}");
    }

    tracing::info!(
        "[knowledge] снимок {}: {} единиц на {} поверхностях, {} токенов хранимого, {} мс",
        snapshot.trigger,
        snapshot.unit_count,
        snapshot.surface_count,
        snapshot.stored_tokens,
        collect_ms
    );

    Ok(InventoryCollectReportDto {
        snapshot_id: snapshot.id,
        unit_count: snapshot.unit_count,
        surface_count: snapshot.surface_count,
        collect_ms,
        diagnostics: collected.diagnostics,
    })
}

/// Полный ответ страницы.
///
/// Если снимка ещё нет — снимаем на месте. Иначе первый заход на страницу после
/// установки показывал бы пустоту, и человек решил бы, что механизм сломан.
pub async fn inventory(db: &DatabaseConnection) -> Result<InventoryResponseDto> {
    let stored = match repository::latest(db).await? {
        Some(found) => Some(found),
        None => {
            collect_and_store(db, "manual").await?;
            repository::latest(db).await?
        }
    };
    let Some((snapshot, summary)) = stored else {
        anyhow::bail!("снимок инвентаризации не записан");
    };

    let units = repository::units_of(db, &snapshot.id).await?;
    let previous = repository::previous(db, &snapshot.captured_at, snapshot.classifier_version)
        .await
        .unwrap_or_default();
    // Реестр поверхностей берётся из кода этой сборки, а не из снимка: он
    // описывает систему, а не её состояние, и в старом снимке был бы устаревшим.
    let surfaces = surfaces_with_counts(&units);

    Ok(InventoryResponseDto {
        snapshot,
        previous,
        summary,
        axes: axes(),
        facets: facets(&units),
        surfaces,
        units,
    })
}

/// Реестр поверхностей со счётчиками по единицам снимка.
fn surfaces_with_counts(units: &[KnowledgeUnitDto]) -> Vec<SurfaceDto> {
    use super::catalog::SURFACE_CATALOG;
    use super::tool_map;
    use contracts::knowledge::UnitFamily;

    SURFACE_CATALOG
        .iter()
        .map(|surface| {
            let own: Vec<&KnowledgeUnitDto> = units
                .iter()
                .filter(|unit| unit.surface_id == surface.surface_id)
                .collect();
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
                stored_tokens: (surface.family == UnitFamily::Stored)
                    .then(|| own.iter().filter_map(|unit| unit.tokens).sum()),
            }
        })
        .collect()
}

/// Фасеты — счётчики по каждому значению каждой оси.
///
/// Считаются по всем девяти осям сразу и отдаются с подписями: фронт строит
/// фильтр, не зная ни одного кода. Значение с нулём не выбрасывается — пустая
/// строка фильтра сообщает «такого в системе нет», и это тоже ответ.
fn facets(units: &[KnowledgeUnitDto]) -> Vec<FacetDto> {
    use std::collections::HashMap;

    let mut counts: HashMap<(&str, String), usize> = HashMap::new();
    for unit in units {
        let mut bump = |axis: &'static str, code: &str| {
            *counts.entry((axis, code.to_string())).or_insert(0) += 1;
        };
        bump("family", unit.family.as_str());
        bump("origin", unit.origin.as_str());
        bump("storage_form", unit.storage_form.as_str());
        bump("editor", unit.editor.as_str());
        bump("reachability", unit.reachability.as_str());
        bump("lifecycle", unit.lifecycle.as_str());
        bump("scope", unit.scope.as_str());
        bump("channel", unit.channel.as_str());
        if let Some(role) = unit.code_role {
            bump("code_role", role.as_str());
        }
    }

    axes()
        .into_iter()
        .map(|axis| FacetDto {
            values: axis
                .values
                .iter()
                .map(|value| FacetValueDto {
                    code: value.code.clone(),
                    label: value.label.clone(),
                    count: counts
                        .get(&(axis.axis.as_str(), value.code.clone()))
                        .copied()
                        .unwrap_or(0),
                })
                .collect(),
            axis: axis.axis,
            label: axis.label,
        })
        .collect()
}

/// Сводка последнего снимка без единиц — для инструмента чата и метрик.
pub async fn summary_only(
    db: &DatabaseConnection,
) -> Result<Option<(InventorySnapshotDto, InventorySummaryDto)>> {
    repository::latest(db).await
}
