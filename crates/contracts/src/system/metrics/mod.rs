//! DTO страницы «Метрики проекта» (`sys_metrics`).
//!
//! Зеркалит `backend/src/system/metrics/` — только сериализуемые типы.
//!
//! Фронт намеренно не знает ни одного `metric_key`: подписи, единицы, группы,
//! порядок и цвет приходят с бэкенда из `METRIC_CATALOG`. Добавление метрики —
//! правка каталога на бэкенде, страница подхватит её сама.

use serde::{Deserialize, Serialize};

/// Куда смотрит метрика: рост — это хорошо, плохо или нейтрально.
/// Определяет знак и цвет дельты, а не сам факт её показа.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MetricDirection {
    /// Больше — лучше (тесты, покрытие документацией).
    Higher,
    /// Меньше — лучше (мёртвые классы, `.unwrap()`, освобождаемое место).
    Lower,
    /// Ни то ни другое (число агрегатов, размер кода) — дельта без окраски.
    Neutral,
}

/// Оценка значения по порогам каталога.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MetricStatus {
    Ok,
    Warn,
    Bad,
    /// Порогов нет — метрика справочная.
    Neutral,
}

/// Одно значение в снимке, уже сопоставленное с предыдущим снимком.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricValueDto {
    pub key: String,
    pub label: String,
    /// Код группы (блока страницы), см. `MetricGroupDto`.
    pub group: String,
    /// Единица для подписи: "", "строк", "МБ", "%", "шт".
    pub unit: String,
    pub value: f64,
    /// Сколько знаков после запятой показывать.
    pub precision: u8,
    pub direction: MetricDirection,
    pub status: MetricStatus,
    /// Порог «жёлтого». Едет на фронт не ради дублирования `status`, а ради
    /// шкалы: цвет говорит «плохо», а метр — насколько далеко до порога.
    pub warn: Option<f64>,
    /// Порог «красного».
    pub bad: Option<f64>,
    /// Значение в предыдущем снимке; `None` — метрика появилась только сейчас.
    pub previous: Option<f64>,
    /// `value - previous`, посчитанное на бэкенде, чтобы фронт не повторял логику.
    pub delta: Option<f64>,
    /// Короткое пояснение, откуда цифра.
    pub hint: Option<String>,
}

/// Группа метрик = блок на странице.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricGroupDto {
    pub code: String,
    pub label: String,
    pub order: u16,
}

/// Паспорт снимка.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricSnapshotDto {
    pub id: String,
    pub captured_at: String,
    pub trigger: String,
    pub app_version: String,
    pub git_commit: Option<String>,
    pub build_profile: String,
    pub schema_version: i64,
    pub code_generated_at: Option<String>,
    pub collect_ms: i64,
}

/// Строка таблицы-детализации (топ файлов, топ таблиц, quality-проверки).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DetailRow {
    pub name: String,
    pub value: f64,
    /// Вторая колонка, если у таблицы она есть (доля, дата, статус).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extra: Option<String>,
}

/// Именованная таблица-детализация внутри снимка.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DetailTable {
    pub code: String,
    pub label: String,
    /// Заголовок числовой колонки.
    pub value_label: String,
    pub rows: Vec<DetailRow>,
}

/// Всё, что нужно странице за один запрос.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectMetricsDto {
    /// `None` — сбор ещё ни разу не отработал (первый запуск, снимок в пути).
    pub snapshot: Option<MetricSnapshotDto>,
    pub previous_captured_at: Option<String>,
    pub groups: Vec<MetricGroupDto>,
    pub values: Vec<MetricValueDto>,
    pub details: Vec<DetailTable>,
}

/// Точка ряда для спарклайна.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricPoint {
    pub captured_at: String,
    pub value: f64,
}

/// Ряд одной метрики по снимкам, от старых к новым.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricSeriesDto {
    pub key: String,
    pub points: Vec<MetricPoint>,
}

/// Ответ `POST /api/system/metrics/collect`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectResultDto {
    pub snapshot_id: String,
    pub collect_ms: i64,
    pub metrics_written: usize,
}
