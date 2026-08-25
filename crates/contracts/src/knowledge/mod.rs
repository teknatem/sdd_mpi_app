//! DTO механизма «Инвентаризация знаний».
//!
//! Механизм отвечает на два разных вопроса и не путает их (§1 нормативного
//! документа `memory-bank/architecture/knowledge-inventory.md`):
//! **сколько знания есть** — это реестр единиц, и **сколько знания доставлено** —
//! это трасса вызовов. Здесь только первое.
//!
//! Единицы двух семейств, и складывать их нельзя: у хранимой есть размер на
//! диске и полная цена в токенах, у вычисляемой — только цена конкретного
//! ответа, которую мы не замеряли. Поэтому `tokens` у вычисляемой — `None`,
//! а не `0`: ноль сложился бы в сумму и соврал бы про бюджет контекста.

pub mod classifiers;

pub use classifiers::{
    axes, AxisDto, AxisValueDto, CodeRole, Editor, ExposureChannel, Lifecycle, Origin,
    Reachability, Scope, StorageForm, UnitFamily, CLASSIFIER_VERSION,
};

use serde::{Deserialize, Serialize};

/// Единица учёта: объект с устойчивым идентификатором и одним владельцем.
///
/// Раздел статьи единицей **не является** (решение 2026-08-25): сумма разделов
/// равна документу, и считать оба значит заложить двойной счёт в бюджет.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KnowledgeUnitDto {
    /// Всегда с префиксом типа: `article:`, `entity:`, `action:`, `skill:`,
    /// `plugin:`, `process:`, `stage:`, `check:`, `source:`, `account:`,
    /// `turnover:`, `ui_scope:`, `tool:`, `task:`, `ext_route:`, `vocabulary:`.
    ///
    /// Без префикса статья `plugins` и карта `plugins.md` схлопнулись бы в один
    /// идентификатор, и одна из них молча исчезла бы из счёта.
    pub unit_id: String,
    /// Строка реестра поверхностей, которой единица принадлежит.
    pub surface_id: String,
    pub family: UnitFamily,
    pub origin: Origin,
    pub storage_form: StorageForm,
    pub editor: Editor,
    pub reachability: Reachability,
    pub lifecycle: Lifecycle,
    pub scope: Scope,
    pub channel: ExposureChannel,
    /// `None` — исходный код к этой единице отношения не имеет (курируемая
    /// статья, строка БД). Три значения оси описывают роль **кода**, и
    /// приписывать одно из них статье в Obsidian значило бы выдумывать.
    pub code_role: Option<CodeRole>,
    pub title: String,
    /// Одна строка пояснения: summary статьи, описание Действия, имя таблицы.
    #[serde(default)]
    pub subtitle: String,
    /// Путь к файлу, имя Rust-константы или идентификатор строки БД.
    pub source_ref: Option<String>,
    /// Размер на диске. `None` у вычисляемых.
    pub bytes: Option<u32>,
    /// Цена доставки в токенах по `estimate_tokens`. `None` у вычисляемых —
    /// их единица бюджета это типичный ответ инструмента, а он не замерян.
    pub tokens: Option<u32>,
    /// «Поиск счёл релевантным».
    #[serde(default)]
    pub search_hits: i64,
    /// «Модель реально потратила токены».
    #[serde(default)]
    pub read_hits: i64,
    /// «Попало в ответ, который увидел человек».
    #[serde(default)]
    pub cited_hits: i64,
    pub updated: Option<String>,
    /// Насколько израсходован срок годности знания, %.
    pub staleness_pct: Option<u32>,
    #[serde(default)]
    pub tags: Vec<String>,
    /// Нарушения инвариантов §7 по этой единице — готовый список работ.
    #[serde(default)]
    pub issues: Vec<String>,
}

/// Строка реестра поверхностей (§2).
///
/// Реестр объявляется явно: обход диска не заметит того, чего на диске нет, а
/// на диске нет большей части системы.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SurfaceDto {
    pub surface_id: String,
    pub label: String,
    pub family: UnitFamily,
    pub origin: Origin,
    pub storage_form: StorageForm,
    pub editor: Editor,
    pub scope: Scope,
    pub channel: ExposureChannel,
    /// `None` — код к поверхности отношения не имеет.
    pub code_role: Option<CodeRole>,
    /// Достижимость, **заявленная** автором реестра.
    pub reachability_declared: Reachability,
    /// Достижимость, **вычисленная** из маппинга инструментов.
    pub reachability_effective: Reachability,
    /// Чем перечисляется — имя функции или константы.
    pub enumerated_by: String,
    #[serde(default)]
    pub note: String,
    /// Инструменты чата, отдающие эту поверхность.
    #[serde(default)]
    pub tools: Vec<String>,
    pub unit_count: usize,
    /// Сумма токенов хранимых единиц. `None` у вычисляемой поверхности.
    pub stored_tokens: Option<u32>,
}

/// Паспорт снимка.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InventorySnapshotDto {
    pub id: String,
    pub captured_at: String,
    /// `startup` | `manual`.
    pub trigger: String,
    /// Версия состава классификаторов на момент снятия.
    ///
    /// Снимки разных версий не сравниваются поразрезно: разреза, которого тогда
    /// не было, задним числом не существует.
    pub classifier_version: u16,
    pub app_version: String,
    pub unit_count: usize,
    pub surface_count: usize,
    /// Токены только хранимых единиц — вычисляемые сюда не входят по определению.
    pub stored_tokens: u32,
    pub collect_ms: i64,
    #[serde(default)]
    pub diagnostics: Vec<String>,
}

/// Значение оси со счётчиком — из этого строится фильтр таблицы.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FacetValueDto {
    pub code: String,
    pub label: String,
    pub count: usize,
}

/// Разрез по одной оси.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FacetDto {
    pub axis: String,
    pub label: String,
    pub values: Vec<FacetValueDto>,
}

/// Сводка §6. Каждое поле — либо число, либо поимённый список.
///
/// Списки важнее чисел: «3 недостижимые поверхности» — это повод пожать плечами,
/// «недостижимы Действия, план счетов и разделы UI» — это задача.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct InventorySummaryDto {
    // ─── объём ───
    pub stored_units: usize,
    pub computed_units: usize,
    pub stored_bytes: u64,
    pub stored_tokens: u32,
    pub articles_business: usize,
    pub articles_app: usize,
    pub articles_generated: usize,

    // ─── достижимость (§5) ───
    /// Поверхности, которые не отдаёт ни один инструмент.
    pub unreachable_surfaces: Vec<String>,
    /// Имена в `tools:` навыка, которых нет в каталоге инструментов, —
    /// такой инструмент молча выбрасывается при сборке.
    pub phantom_tools: Vec<String>,
    /// Инструменты вне ядра и вне любого навыка: мёртвый код.
    pub orphan_tools: Vec<String>,
    /// Поверхности, где заявленная достижимость разошлась с вычисленной.
    pub reachability_mismatches: Vec<String>,

    // ─── связность ───
    pub dangling_links: usize,
    pub unknown_anchors: usize,
    pub unknown_tags: usize,
    /// Сколько объектов системы имеют хотя бы одну привязанную статью.
    pub anchored_entities: usize,

    // ─── свежесть и цикл ───
    pub drafts: usize,
    pub deprecated: usize,
    pub stale_articles: usize,

    // ─── ценность ───
    /// Находится поиском, но не читается — плохой `summary`.
    pub searched_not_read: usize,
    /// Читается, но не цитируется — плохая статья.
    pub read_not_cited: usize,
    /// Ни поиска, ни чтения, ни цитирования.
    pub never_touched: usize,

    // ─── гигиена ───
    /// Строки статистики без живой статьи.
    pub orphaned_metrics: usize,
    /// Идентификаторы, занятые дважды.
    pub duplicate_ids: Vec<String>,
    /// Документы дороже порога чтения по разделам, но без якорей.
    pub oversized_docs: Vec<String>,
}

/// Полный ответ страницы: паспорт, сводка, оси, фасеты, реестр и все единицы.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InventoryResponseDto {
    pub snapshot: InventorySnapshotDto,
    /// Предыдущий снимок — для дельт. Сравним только при равной версии
    /// классификатора; страница обязана это показывать.
    pub previous: Option<InventorySnapshotDto>,
    pub summary: InventorySummaryDto,
    /// Описания осей: подписи фильтров приходят отсюда, а не из фронта.
    pub axes: Vec<AxisDto>,
    pub facets: Vec<FacetDto>,
    pub surfaces: Vec<SurfaceDto>,
    pub units: Vec<KnowledgeUnitDto>,
}

/// Итог ручного пересбора снимка.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InventoryCollectReportDto {
    pub snapshot_id: String,
    pub unit_count: usize,
    pub surface_count: usize,
    pub collect_ms: i64,
    #[serde(default)]
    pub diagnostics: Vec<String>,
}

/// Одна точка ряда: значение агрегата на момент снимка.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InventoryHistoryPointDto {
    pub captured_at: String,
    pub classifier_version: u16,
    pub unit_count: usize,
    pub stored_tokens: u32,
    pub unreachable_surfaces: usize,
}
