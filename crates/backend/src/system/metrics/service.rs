//! Сборка ответа для страницы: снимок + каталог + дельта к предыдущему снимку.
//!
//! Дельта и статус считаются здесь, а не во фронте, сознательно: пороги и
//! направление «больше — лучше» живут в `catalog.rs`, и дублировать их в WASM
//! значило бы завести второй источник истины, который разъедется первым же
//! изменением порога.

use contracts::system::metrics::{DetailTable, MetricSeriesDto, MetricValueDto, ProjectMetricsDto};

use super::{catalog, repository};

/// Последний снимок со всеми значениями, сопоставленными с предыдущим.
pub async fn latest() -> anyhow::Result<ProjectMetricsDto> {
    let groups = catalog::groups();

    let Some(snapshot) = repository::latest_snapshot().await? else {
        // Первый запуск: сбор идёт в фоне и ещё не закончился. Пустой ответ
        // честнее ошибки — страница покажет «снимок готовится».
        return Ok(ProjectMetricsDto {
            snapshot: None,
            previous_captured_at: None,
            groups,
            values: Vec::new(),
            details: Vec::new(),
        });
    };

    let current = repository::values_of(&snapshot.id).await?;
    let previous = repository::previous_snapshot(&snapshot.captured_at).await?;
    let previous_values = match previous.as_ref() {
        Some(row) => repository::values_of(&row.id).await?,
        None => Default::default(),
    };

    // Порядок задаёт каталог, а не БД: он же задаёт порядок плиток на странице.
    let values: Vec<MetricValueDto> = catalog::METRIC_CATALOG
        .iter()
        .filter_map(|def| {
            let value = *current.get(def.key)?;
            let prev = previous_values.get(def.key).copied();
            Some(MetricValueDto {
                key: def.key.to_string(),
                label: def.label.to_string(),
                group: def.group.to_string(),
                unit: def.unit.to_string(),
                value,
                precision: def.precision,
                direction: def.direction,
                status: def.status(value),
                warn: def.warn,
                bad: def.bad,
                previous: prev,
                delta: prev.map(|prev| value - prev),
                hint: def.hint.map(str::to_string),
            })
        })
        .collect();

    let details: Vec<DetailTable> =
        serde_json::from_str(&snapshot.details_json).unwrap_or_default();

    Ok(ProjectMetricsDto {
        previous_captured_at: previous.map(|row| row.captured_at),
        snapshot: Some(snapshot.to_dto()),
        groups,
        values,
        details,
    })
}

/// Ряды для спарклайнов. Неизвестные каталогу ключи отсекаются здесь, чтобы
/// параметр запроса не превращался в способ читать произвольные строки таблицы.
pub async fn series(keys: &[String], limit: u64) -> anyhow::Result<Vec<MetricSeriesDto>> {
    let known: Vec<String> = keys
        .iter()
        .filter(|key| catalog::find(key).is_some())
        .cloned()
        .collect();
    repository::series(&known, limit.clamp(2, 500)).await
}
