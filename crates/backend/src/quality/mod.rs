//! # Подсистема контроля качества данных
//!
//! Единый реестр read-only инвариантов качества данных для UI, LLM, ручных и
//! плановых запусков. Каждое правило имеет одну цель и небольшой набор
//! однотипных метрик над одной бизнес-популяцией.
//!
//! ## Архитектура
//!
//! ```text
//! quality/
//! ├── registry.rs  ← snapshot реестра Rust + MJS, validate/reload/authoring
//! ├── history.rs   ← единая история ручных, LLM и плановых запусков
//! ├── mod.rs       ← исполнение и стабильный внутренний интерфейс
//! └── checks/      ← сложные Rust-проверки с drill-down/fix-командами
//! ```
//!
//! ## Концепция
//!
//! Каждая **проверка** (check) отвечает за одну конкретную потенциальную проблему
//! или группу однородных проблем. Проверка:
//!
//! - имеет уникальный строковый `id` и читаемое название;
//! - запускается вручную через `POST /api/quality/checks/{id}/run`;
//! - возвращает [`CheckResult`] с набором метрик и итоговым счётчиком проблем;
//! - запускается вручную, LLM-инструментом или задачей `quality_check_run`;
//! - MJS всегда read-only; исправляющие команды остаются в Rust.
//!
//! ## Зарегистрированные проверки
//!
//! | ID | Название | Что проверяет |
//! |----|----------|---------------|
//! | `nomenclature_in_projections` | Заполненность номенклатуры в проекциях | Строки p909/p911/p913, где `nomenclature_ref IS NULL` |
//! | `gl_projection_integrity` | Целостность GL ↔ ProjectionLinked-проекции | orphan_gl / orphan_projection / amount_mismatch для p909/p910/p911/p913 |
//! | `p903_gl_integrity` | Целостность GL ↔ p903 (ExternalLinked) | orphan_gl / amount_mismatch для p903_wb_finance_report |
//!
//! Новые read-only правила добавляются внешним пакетом `check.json` +
//! `check.mjs` (+ schema) через skill `quality-check-authoring` и атомарный
//! reload без пересборки. Rust нужен только для доменных сервисов и мутаций.

pub mod checks;
pub mod history;
pub mod registry;

use contracts::quality::{
    CheckDetails, CheckResult, NipCleanupRequest, NipCleanupResult, NipGroupsResponse,
    NipProjectionRow, NipRepostRequest, NipRepostResult, QualityCheckInfo, QualityCheckOverview,
    QualityCheckSource,
};
use serde_json::Value;

const MAX_MJS_RESULT_BYTES: usize = 1024 * 1024;

/// Возвращает список всех зарегистрированных проверок.
pub fn list_checks() -> Vec<QualityCheckInfo> {
    registry::snapshot()
        .definitions
        .iter()
        .map(|definition| definition.info.clone())
        .collect()
}

/// Возвращает карточки каталога вместе с последним запуском именно текущей
/// версии определения. Результаты старого digest не маскируют изменённое правило.
pub async fn list_check_overviews() -> anyhow::Result<Vec<QualityCheckOverview>> {
    let snapshot = registry::snapshot();
    let mut latest = history::latest_runs_by_definition().await?;
    let mut items = Vec::with_capacity(snapshot.definitions.len());
    for definition in snapshot.definitions.iter() {
        items.push(QualityCheckOverview {
            info: definition.info.clone(),
            kind: definition.kind.clone(),
            definition_digest: definition.digest.clone(),
            latest_run: latest.remove(&(definition.info.id.clone(), definition.digest.clone())),
        });
    }
    Ok(items)
}

/// Запускает проверку по её ID и возвращает результат.
///
/// Возвращает `Err` с маркером `"NOT_FOUND"` в сообщении, если ID не зарегистрирован.
pub async fn run_check(id: &str) -> anyhow::Result<CheckResult> {
    Ok(
        run_check_with_input(id, Value::Object(Default::default()), "manual")
            .await?
            .result,
    )
}

async fn execute_definition(
    definition: &registry::CheckDefinition,
    input: Value,
) -> anyhow::Result<registry::CheckOutput> {
    use registry::{CheckExecutor, RustCheck};
    let output = match &definition.executor {
        CheckExecutor::Rust(kind) => match kind {
            RustCheck::NomenclatureInProjections => registry::CheckOutput {
                metrics: checks::nomenclature_in_projections::run().await?.metrics,
                breakdowns: checks::nomenclature_in_projections::breakdowns().await?,
                sources: checks::nomenclature_in_projections::list_sources(),
                ..Default::default()
            },
            RustCheck::ProjectionOrphanRegistrators => registry::CheckOutput {
                metrics: checks::projection_orphan_registrators::run().await?.metrics,
                breakdowns: checks::projection_orphan_registrators::breakdowns().await?,
                sources: checks::projection_orphan_registrators::list_sources(),
                ..Default::default()
            },
            RustCheck::GlProjectionIntegrity => {
                let result = checks::gl_projection_integrity::run().await?;
                registry::CheckOutput {
                    metrics: result.metrics,
                    violations: result.violations,
                    ..Default::default()
                }
            }
            RustCheck::P903GlIntegrity => {
                let result = checks::p903_gl_integrity::run().await?;
                registry::CheckOutput {
                    metrics: result.metrics,
                    violations: result.violations,
                    ..Default::default()
                }
            }
            RustCheck::KbIntegrity => {
                let result = checks::kb_integrity::run().await?;
                registry::CheckOutput {
                    metrics: result.metrics,
                    violations: result.violations,
                    ..Default::default()
                }
            }
        },
        CheckExecutor::Javascript(js) => {
            let (value, logs) = crate::plugins::engine::invoke_ephemeral_server_script_with_limits(
                js.source.clone(),
                js.export.clone(),
                input,
                js.capabilities.clone(),
                crate::plugins::engine::ScriptExecutionLimits::quality_check(),
            )
            .await?;
            for line in logs {
                tracing::info!(check_id=%definition.info.id, "quality.mjs: {line}");
            }
            if serde_json::to_vec(&value)?.len() > MAX_MJS_RESULT_BYTES {
                anyhow::bail!("quality check result exceeds 1 MiB");
            }
            serde_json::from_value(value)?
        }
    };
    registry::validate_output(&output)?;
    Ok(output)
}

pub async fn run_check_with_input(
    id: &str,
    input: Value,
    trigger: &str,
) -> anyhow::Result<CheckDetails> {
    let definition = registry::definition(id)
        .ok_or_else(|| anyhow::anyhow!("NOT_FOUND: Unknown check id: {id}"))?;
    let input = registry::merge_input(&definition.default_input, input);
    registry::validate_input(&definition, &input)?;
    let (run_id, started_at) = history::start_run(id, &definition.digest, &input, trigger).await?;
    let outcome = execute_definition(&definition, input).await;
    match outcome {
        Ok(output) => {
            let population_total = output
                .metrics
                .iter()
                .try_fold(0_i64, |total, metric| total.checked_add(metric.population))
                .ok_or_else(|| anyhow::anyhow!("quality check population total overflow"))?;
            let violations_total = output
                .metrics
                .iter()
                .try_fold(0_i64, |total, metric| total.checked_add(metric.violations))
                .ok_or_else(|| anyhow::anyhow!("quality check violations total overflow"))?;
            let details = CheckDetails {
                info: definition.info,
                result: CheckResult {
                    check_id: id.to_string(),
                    run_at: chrono::Utc::now(),
                    population_total,
                    violations_total,
                    metrics: output.metrics,
                    violations: output.violations,
                },
                breakdowns: output.breakdowns,
                sources: output.sources,
            };
            history::finish_success(&run_id, started_at, &details).await?;
            Ok(details)
        }
        Err(error) => {
            let _ = history::finish_failure(&run_id, started_at, &error.to_string()).await;
            Err(error)
        }
    }
}

/// Собирает полный пакет детализации правила для страницы `quality_check_details`:
/// метаданные + прогон (с популяцией и нарушениями) + разрезы + источники drill-down.
pub async fn check_details(id: &str) -> anyhow::Result<CheckDetails> {
    let definition = registry::definition(id)
        .ok_or_else(|| anyhow::anyhow!("NOT_FOUND: Unknown check id: {id}"))?;
    if let Some(mut details) =
        history::latest_success_details_for_digest(id, &definition.digest).await?
    {
        details.info = definition.info;
        return Ok(details);
    }
    run_check_with_input(id, Value::Object(Default::default()), "details").await
}

/// Возвращает список источников (проекционных таблиц) для указанной проверки.
pub fn list_check_sources(check_id: &str) -> anyhow::Result<Vec<QualityCheckSource>> {
    use registry::{CheckExecutor, RustCheck};
    let definition = registry::definition(check_id)
        .ok_or_else(|| anyhow::anyhow!("NOT_FOUND: Unknown check id: {check_id}"))?;
    match definition.executor {
        CheckExecutor::Rust(RustCheck::NomenclatureInProjections) => {
            Ok(checks::nomenclature_in_projections::list_sources())
        }
        CheckExecutor::Rust(RustCheck::ProjectionOrphanRegistrators) => {
            Ok(checks::projection_orphan_registrators::list_sources())
        }
        _ => Ok(Vec::new()),
    }
}

pub async fn list_runs(
    id: &str,
    limit: i64,
) -> anyhow::Result<Vec<contracts::quality::QualityCheckRunSummary>> {
    if registry::definition(id).is_none() {
        anyhow::bail!("NOT_FOUND: Unknown check id: {id}");
    }
    history::list_runs(id, limit).await
}

/// Возвращает страницу групп регистраторов для drill-down по проекционной таблице.
pub async fn list_check_groups(
    check_id: &str,
    projection_table: &str,
    page: i64,
    page_size: i64,
    sort_by: &str,
    sort_desc: bool,
) -> anyhow::Result<NipGroupsResponse> {
    match check_id {
        checks::nomenclature_in_projections::CHECK_ID => {
            checks::nomenclature_in_projections::list_groups(
                projection_table,
                page,
                page_size,
                sort_by,
                sort_desc,
            )
            .await
        }
        checks::projection_orphan_registrators::CHECK_ID => {
            checks::projection_orphan_registrators::list_groups(
                projection_table,
                page,
                page_size,
                sort_by,
                sort_desc,
            )
            .await
        }
        other => Err(anyhow::anyhow!("NOT_FOUND: Unknown check id: {}", other)),
    }
}

/// Возвращает строки проекции с пустым `nomenclature_ref` для одного регистратора.
pub async fn list_check_rows(
    check_id: &str,
    projection_table: &str,
    registrator_ref: &str,
) -> anyhow::Result<Vec<NipProjectionRow>> {
    match check_id {
        checks::nomenclature_in_projections::CHECK_ID => {
            checks::nomenclature_in_projections::list_rows(projection_table, registrator_ref).await
        }
        checks::projection_orphan_registrators::CHECK_ID => {
            checks::projection_orphan_registrators::list_rows(projection_table, registrator_ref)
                .await
        }
        other => Err(anyhow::anyhow!("NOT_FOUND: Unknown check id: {}", other)),
    }
}

/// Удаляет осиротевшие строки проекций по указанным регистраторам.
pub async fn cleanup_orphans(
    check_id: &str,
    request: &NipCleanupRequest,
) -> anyhow::Result<NipCleanupResult> {
    match check_id {
        checks::projection_orphan_registrators::CHECK_ID => {
            checks::projection_orphan_registrators::cleanup(
                &request.projection_table,
                &request.registrator_refs,
            )
            .await
        }
        other => Err(anyhow::anyhow!("NOT_FOUND: Unknown check id: {}", other)),
    }
}

/// Массово перепроводит указанные документы.
pub async fn bulk_repost(
    check_id: &str,
    request: &NipRepostRequest,
) -> anyhow::Result<NipRepostResult> {
    match check_id {
        checks::nomenclature_in_projections::CHECK_ID => {
            checks::nomenclature_in_projections::bulk_repost(
                &request.registrator_type,
                &request.registrator_refs,
            )
            .await
        }
        other => Err(anyhow::anyhow!("NOT_FOUND: Unknown check id: {}", other)),
    }
}
