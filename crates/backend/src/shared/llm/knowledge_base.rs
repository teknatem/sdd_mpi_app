//! База знаний для LLM: Obsidian-бизнес-слой + встроенная документация приложения.
//!
//! Сканирует бизнес-директорию `knowledge_base_path` из конфига, добавляет embedded
//! markdown-документы приложения, парсит YAML frontmatter каждого MD-файла и строит
//! in-memory индексы: `тег → [doc_id]`, обратные рёбра `related` и BM25-индекс
//! (`kb_search`) для поиска по свободному тексту.
//!
//! Инструменты LLM (см. `kb_tools`):
//! - `search_knowledge(query, tags)` — гибридный поиск, возвращает id + summary
//! - `get_knowledge(id)` — полный текст документа без frontmatter

use super::frontmatter::{parse_date, parse_list, parse_scalar, parse_u32, split_frontmatter};
use super::kb_search::{Corpus, SearchIndex};
use super::kb_vocabulary::Vocabulary;
use chrono::{DateTime, NaiveDate, Utc};
use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, RwLock};

/// Имя файла словаря тегов внутри каталога базы знаний.
pub const VOCABULARY_FILE: &str = "_vocabulary.md";

/// Подкаталог базы знаний под карты, собранные генератором из БД и рантайма.
pub const GENERATED_DOCS_SUBDIR: &str = "generated";

/// Срок годности знания по умолчанию, если в статье не указан `ttl_days`.
pub const DEFAULT_TTL_DAYS: u32 = 180;

// ─── Структуры ───────────────────────────────────────────────────────────────

/// Жизненный цикл статьи. `active` — дефолт, чтобы статьи без поля `status`
/// вели себя ровно как до введения статусов.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum KbStatus {
    /// Черновик: виден, но понижен в выдаче и помечен в UI.
    Draft,
    #[default]
    Active,
    /// Устарела: в выдачу LLM не попадает без явного запроса.
    Deprecated,
}

impl KbStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Active => "active",
            Self::Deprecated => "deprecated",
        }
    }

    pub fn from_str(raw: &str) -> Self {
        match raw.trim().to_lowercase().as_str() {
            "draft" | "черновик" => Self::Draft,
            "deprecated" | "устарела" | "archived" => Self::Deprecated,
            _ => Self::Active,
        }
    }
}

/// Кто и как создаёт документ. Определяет корпус, в котором его ищут.
///
/// Разделение нужно потому, что корпуса растут независимо: бизнес-статей будут
/// сотни (их пишут люди), а карт — десятки, и они переписываются генератором при
/// каждом запуске. Смешивать их в одной выдаче значит утопить курируемое знание
/// в машинном и обессмыслить статистику `search/read/cited`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DocKind {
    /// Знание об организации: пишется человеком и LLM, курируется, протухает.
    #[default]
    Business,
    /// Техдокументация приложения: канон в репозитории, копия в `app/`.
    App,
    /// Карта из БД и рантайма (`generated/`): переписывается генератором.
    Generated,
}

impl DocKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Business => "business",
            Self::App => "app",
            Self::Generated => "generated",
        }
    }

    pub fn from_str(raw: &str) -> Self {
        match raw.trim().to_lowercase().as_str() {
            "app" => Self::App,
            "generated" => Self::Generated,
            _ => Self::Business,
        }
    }
}

/// Один документ базы знаний (один MD-файл).
#[derive(Debug, Clone, Default)]
pub struct KnowledgeDoc {
    /// Slug-идентификатор (имя файла без `.md`)
    pub id: String,
    /// Заголовок из frontmatter `title:`
    pub title: String,
    /// Теги из frontmatter `tags: [...]` — как написано в файле
    pub tags: Vec<String>,
    /// Связанные статьи/теги/агрегаты из frontmatter `related: [...]`,
    /// дополненные `[[wiki-ссылками]]` из тела.
    pub related: Vec<String>,
    /// Явные якоря из frontmatter `entities: [...]` — как написано в файле.
    pub entities: Vec<String>,
    /// Тело MD без frontmatter-блока
    pub content: String,
    pub source_path: Option<String>,

    // ─── Авторская метадата ───
    /// Стабильный идентификатор `kb-<8 hex>`, переживающий переименование файла.
    pub uid: Option<String>,
    pub status: KbStatus,
    /// Важность/полезность 1–5. `None` трактуется как 3 (нейтрально).
    pub stars: Option<u8>,
    /// Одна строка: что даёт статья. Отдаётся в результатах поиска вместо тела.
    pub summary: String,
    /// Альтернативные формулировки для поиска.
    pub aliases: Vec<String>,
    pub updated: Option<NaiveDate>,
    /// Когда человек последний раз подтвердил, что статья соответствует реальности.
    pub verified: Option<NaiveDate>,
    /// Ожидаемый срок годности знания в днях.
    pub ttl_days: Option<u32>,
    /// Чаты, из которых родилась статья.
    pub source_chats: Vec<String>,
    /// `llm` или `human`.
    pub author: String,

    // ─── Вычисляется при загрузке ───
    /// Теги, приведённые к словарю.
    pub canonical_tags: Vec<String>,
    /// Теги, которых нет в словаре — рабочий список куратора.
    pub unknown_tags: Vec<String>,
    /// Оценка стоимости чтения статьи в токенах.
    pub token_cost: u32,
    /// Корпус документа (из frontmatter `kind:`).
    pub kind: DocKind,
    /// Коды объектов системы, к которым привязан документ: `entities` плюс те
    /// записи `related`, что резолвятся в реестр метаданных. Канонический вид —
    /// индекс сущности (`a012_wb_sales` → `a012`).
    pub anchors: Vec<String>,
    /// Записи `entities`, которых нет в реестре — рабочий список куратора.
    pub unknown_anchors: Vec<String>,
    /// Дней с даты `verified` (иначе `updated`, иначе mtime файла).
    pub age_days: Option<u32>,
}

impl KnowledgeDoc {
    /// Ключ строки статистики: `uid`, если есть, иначе id файла.
    pub fn metrics_key(&self) -> String {
        self.uid.clone().unwrap_or_else(|| self.id.clone())
    }

    /// Документ не написан руками: техдок приложения или сгенерированная карта.
    /// Такие не протухают — они переезжают вместе с кодом или перегенерируются.
    pub fn is_embedded(&self) -> bool {
        self.kind != DocKind::Business
    }

    /// Насколько статья протухла, 0–100 %. `None` — срок неприменим.
    pub fn staleness_pct(&self) -> Option<u32> {
        if self.is_embedded() {
            return None;
        }
        let age = self.age_days?;
        let ttl = self.ttl_days.unwrap_or(DEFAULT_TTL_DAYS).max(1);
        Some(((age as f32 / ttl as f32) * 100.0).min(999.0) as u32)
    }
}

/// In-memory база знаний с тег-индексом, графом связей и поисковым индексом.
pub struct KnowledgeBase {
    /// канонический тег → список doc_id
    index: HashMap<String, Vec<String>>,
    /// doc_id → документ
    docs: HashMap<String, KnowledgeDoc>,
    /// Обратные рёбра `related`: doc_id → кто на него ссылается.
    back_links: HashMap<String, Vec<String>>,
    /// Код объекта → документы всех корпусов, привязанные к нему якорем.
    anchor_index: HashMap<String, Vec<String>>,
    /// Последний сегмент id → полные id. Ссылка `general-ledger` должна вести в
    /// `app__general-ledger`: техдоки переехали в подкаталог, а записи `related`
    /// в файлах остались короткими.
    by_basename: HashMap<String, Vec<String>>,
    retrieval: SearchIndex,
    vocabulary: Vocabulary,
    loaded_at: DateTime<Utc>,
    /// Проблемы загрузки: коллизии id, теги вне словаря, ошибки разбора.
    diagnostics: Vec<String>,
    /// Ссылки, не ведущие ни в статью, ни в тег, ни в объект системы.
    dangling_links: usize,
}

// ─── Глобальный синглтон ─────────────────────────────────────────────────────

pub static KNOWLEDGE_BASE: Lazy<Arc<RwLock<KnowledgeBase>>> =
    Lazy::new(|| Arc::new(RwLock::new(KnowledgeBase::load(&knowledge_base_dir()))));

/// Чтение KB, устойчивое к poisoned-локу: KnowledgeBase — read-mostly данные,
/// частичное состояние после паники писателя безопасно читать; паниковать
/// (и валить весь tokio-таск чата) из-за этого не нужно.
pub fn kb_read() -> std::sync::RwLockReadGuard<'static, KnowledgeBase> {
    KNOWLEDGE_BASE.read().unwrap_or_else(|poisoned| {
        tracing::error!("KnowledgeBase lock poisoned; continuing with last state");
        poisoned.into_inner()
    })
}

pub fn knowledge_base_dir() -> PathBuf {
    let path = match crate::shared::config::load_config() {
        Ok(cfg) => crate::shared::config::get_knowledge_base_path(&cfg),
        Err(e) => {
            tracing::warn!(
                "KnowledgeBase: cannot load config: {}; using default path",
                e
            );
            std::path::PathBuf::from("data/knowledge")
        }
    };
    path
}

pub fn reload_knowledge_base() -> anyhow::Result<()> {
    let path = knowledge_base_dir();
    let loaded = KnowledgeBase::load(&path);
    {
        let mut guard = KNOWLEDGE_BASE
            .write()
            .map_err(|_| anyhow::anyhow!("KnowledgeBase lock poisoned"))?;
        *guard = loaded;
    }
    // `get_entity_schema` отдаёт список привязанных статей и кэшируется на весь
    // процесс — без сброса он показывал бы состав базы на момент старта.
    super::tool_executor::invalidate_metadata_tool_cache();
    Ok(())
}

pub fn write_kb_document(relative_path: &str, content: &str) -> anyhow::Result<PathBuf> {
    let relative = sanitize_relative_kb_path(relative_path)?;
    let base = knowledge_base_dir();
    let target = base.join(relative);
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&target, content)?;
    Ok(target)
}

/// Подкаталог базы знаний под техдокументацию приложения.
///
/// Канон — файлы в репозитории (список `EMBEDDED_LLM_DOCS`), они едут в
/// бинарнике. Здесь лежит их копия: база знаний должна быть полной на уровне
/// файлов, а техстатьи — того же класса, что и бизнес-статьи (те же атрибуты,
/// тот же поиск, то же дерево в UI).
pub const APP_DOCS_SUBDIR: &str = "app";

struct EmbeddedKnowledgeSource {
    id: &'static str,
    source_path: &'static str,
    raw: &'static str,
}

const EMBEDDED_LLM_DOCS: &[EmbeddedKnowledgeSource] = &[
    EmbeddedKnowledgeSource {
        id: "domain-a012_wb_sales",
        source_path: "crates/backend/src/domain/a012_wb_sales/llm.md",
        raw: include_str!("../../domain/a012_wb_sales/llm.md"),
    },
    EmbeddedKnowledgeSource {
        id: "usecase-u504_import_from_wildberries",
        source_path: "crates/backend/src/usecases/u504_import_from_wildberries/llm.md",
        raw: include_str!("../../usecases/u504_import_from_wildberries/llm.md"),
    },
    EmbeddedKnowledgeSource {
        id: "projection-p904_sales_data",
        source_path: "crates/backend/src/projections/p904_sales_data/llm.md",
        raw: include_str!("../../projections/p904_sales_data/llm.md"),
    },
    EmbeddedKnowledgeSource {
        id: "projection-p916_mp_sales_funnel_turnovers",
        source_path: "crates/backend/src/projections/p916_mp_sales_funnel_turnovers/llm.md",
        raw: include_str!("../../projections/p916_mp_sales_funnel_turnovers/llm.md"),
    },
    EmbeddedKnowledgeSource {
        id: "domain-a040_wb_search_analytics_daily",
        source_path: "crates/backend/src/domain/a040_wb_search_analytics_daily/llm.md",
        raw: include_str!("../../domain/a040_wb_search_analytics_daily/llm.md"),
    },
    EmbeddedKnowledgeSource {
        id: "data-view-dv001",
        source_path: "crates/backend/src/data_view/dv001_revenue/llm.md",
        raw: include_str!("../../data_view/dv001_revenue/llm.md"),
    },
    EmbeddedKnowledgeSource {
        id: "data-view-dv004",
        source_path: "crates/backend/src/data_view/dv004_general_ledger_turnovers/llm.md",
        raw: include_str!("../../data_view/dv004_general_ledger_turnovers/llm.md"),
    },
    EmbeddedKnowledgeSource {
        id: "general-ledger",
        source_path: "crates/backend/src/general_ledger/llm.md",
        raw: include_str!("../../general_ledger/llm.md"),
    },
    EmbeddedKnowledgeSource {
        id: "domain-a034_ym_realization",
        source_path: "crates/backend/src/domain/a034_ym_realization/llm.md",
        raw: include_str!("../../domain/a034_ym_realization/llm.md"),
    },
    EmbeddedKnowledgeSource {
        id: "domain-a024_bi_indicator",
        source_path: "crates/backend/src/domain/a024_bi_indicator/llm.md",
        raw: include_str!("../../domain/a024_bi_indicator/llm.md"),
    },
    EmbeddedKnowledgeSource {
        id: "domain-a025_bi_dashboard",
        source_path: "crates/backend/src/domain/a025_bi_dashboard/llm.md",
        raw: include_str!("../../domain/a025_bi_dashboard/llm.md"),
    },
    EmbeddedKnowledgeSource {
        id: "app-bi-indicators",
        source_path: "crates/backend/src/shared/llm/docs/bi_indicators.md",
        raw: include_str!("docs/bi_indicators.md"),
    },
    EmbeddedKnowledgeSource {
        id: "app-data-view",
        source_path: "crates/backend/src/shared/llm/docs/data_view.md",
        raw: include_str!("docs/data_view.md"),
    },
    EmbeddedKnowledgeSource {
        id: "app-dev-notes",
        source_path: "crates/backend/src/shared/llm/docs/dev_notes.md",
        raw: include_str!("docs/dev_notes.md"),
    },
    EmbeddedKnowledgeSource {
        id: "app-p909-p910-projections",
        source_path: "crates/backend/src/shared/llm/docs/p909_p910_projections.md",
        raw: include_str!("docs/p909_p910_projections.md"),
    },
    EmbeddedKnowledgeSource {
        id: "app-raw-storage",
        source_path: "crates/backend/src/shared/llm/docs/raw_storage.md",
        raw: include_str!("docs/raw_storage.md"),
    },
    EmbeddedKnowledgeSource {
        id: "app-marketing-metrics",
        source_path: "crates/backend/src/shared/llm/docs/marketing_metrics.md",
        raw: include_str!("docs/marketing_metrics.md"),
    },
    EmbeddedKnowledgeSource {
        id: "app-sales-metrics",
        source_path: "crates/backend/src/shared/llm/docs/sales_metrics.md",
        raw: include_str!("docs/sales_metrics.md"),
    },
    EmbeddedKnowledgeSource {
        id: "app-finance-metrics",
        source_path: "crates/backend/src/shared/llm/docs/finance_metrics.md",
        raw: include_str!("docs/finance_metrics.md"),
    },
    // ─── Справочник внешних API маркетплейсов ───
    // Живут рядом с клиентом соответствующего импорта: там же, где код, который
    // эти эндпоинты вызывает.
    EmbeddedKnowledgeSource {
        id: "wb-api-overview",
        source_path: "crates/backend/src/usecases/u504_import_from_wildberries/docs/wb-api-overview.md",
        raw: include_str!("../../usecases/u504_import_from_wildberries/docs/wb-api-overview.md"),
    },
    EmbeddedKnowledgeSource {
        id: "wb-api-connection",
        source_path: "crates/backend/src/usecases/u504_import_from_wildberries/docs/wb-api-connection.md",
        raw: include_str!("../../usecases/u504_import_from_wildberries/docs/wb-api-connection.md"),
    },
    EmbeddedKnowledgeSource {
        id: "wb-api-content-cards",
        source_path:
            "crates/backend/src/usecases/u504_import_from_wildberries/docs/wb-api-content-cards.md",
        raw: include_str!("../../usecases/u504_import_from_wildberries/docs/wb-api-content-cards.md"),
    },
    EmbeddedKnowledgeSource {
        id: "wb-api-statistics-sales",
        source_path:
            "crates/backend/src/usecases/u504_import_from_wildberries/docs/wb-api-statistics-sales.md",
        raw: include_str!(
            "../../usecases/u504_import_from_wildberries/docs/wb-api-statistics-sales.md"
        ),
    },
    EmbeddedKnowledgeSource {
        id: "wb-api-statistics-orders",
        source_path:
            "crates/backend/src/usecases/u504_import_from_wildberries/docs/wb-api-statistics-orders.md",
        raw: include_str!(
            "../../usecases/u504_import_from_wildberries/docs/wb-api-statistics-orders.md"
        ),
    },
    EmbeddedKnowledgeSource {
        id: "wb-api-finance-report",
        source_path:
            "crates/backend/src/usecases/u504_import_from_wildberries/docs/wb-api-finance-report.md",
        raw: include_str!("../../usecases/u504_import_from_wildberries/docs/wb-api-finance-report.md"),
    },
    EmbeddedKnowledgeSource {
        id: "wb-api-tariffs-commission",
        source_path:
            "crates/backend/src/usecases/u504_import_from_wildberries/docs/wb-api-tariffs-commission.md",
        raw: include_str!(
            "../../usecases/u504_import_from_wildberries/docs/wb-api-tariffs-commission.md"
        ),
    },
    EmbeddedKnowledgeSource {
        id: "wb-api-prices",
        source_path: "crates/backend/src/usecases/u504_import_from_wildberries/docs/wb-api-prices.md",
        raw: include_str!("../../usecases/u504_import_from_wildberries/docs/wb-api-prices.md"),
    },
    EmbeddedKnowledgeSource {
        id: "wb-api-promotions",
        source_path:
            "crates/backend/src/usecases/u504_import_from_wildberries/docs/wb-api-promotions.md",
        raw: include_str!("../../usecases/u504_import_from_wildberries/docs/wb-api-promotions.md"),
    },
    EmbeddedKnowledgeSource {
        id: "wb-api-advert-campaigns",
        source_path:
            "crates/backend/src/usecases/u504_import_from_wildberries/docs/wb-api-advert-campaigns.md",
        raw: include_str!(
            "../../usecases/u504_import_from_wildberries/docs/wb-api-advert-campaigns.md"
        ),
    },
    EmbeddedKnowledgeSource {
        id: "wb-api-advert-fullstats",
        source_path:
            "crates/backend/src/usecases/u504_import_from_wildberries/docs/wb-api-advert-fullstats.md",
        raw: include_str!(
            "../../usecases/u504_import_from_wildberries/docs/wb-api-advert-fullstats.md"
        ),
    },
    EmbeddedKnowledgeSource {
        id: "wb-api-funnel-products",
        source_path:
            "crates/backend/src/usecases/u504_import_from_wildberries/docs/wb-api-funnel-products.md",
        raw: include_str!(
            "../../usecases/u504_import_from_wildberries/docs/wb-api-funnel-products.md"
        ),
    },
    EmbeddedKnowledgeSource {
        id: "wb-api-funnel-history",
        source_path:
            "crates/backend/src/usecases/u504_import_from_wildberries/docs/wb-api-funnel-history.md",
        raw: include_str!("../../usecases/u504_import_from_wildberries/docs/wb-api-funnel-history.md"),
    },
    EmbeddedKnowledgeSource {
        id: "wb-api-funnel-detail-report",
        source_path:
            "crates/backend/src/usecases/u504_import_from_wildberries/docs/wb-api-funnel-detail-report.md",
        raw: include_str!(
            "../../usecases/u504_import_from_wildberries/docs/wb-api-funnel-detail-report.md"
        ),
    },
    EmbeddedKnowledgeSource {
        id: "wb-api-search-analytics",
        source_path:
            "crates/backend/src/usecases/u504_import_from_wildberries/docs/wb-api-search-analytics.md",
        raw: include_str!(
            "../../usecases/u504_import_from_wildberries/docs/wb-api-search-analytics.md"
        ),
    },
    EmbeddedKnowledgeSource {
        id: "wb-api-marketplace-orders",
        source_path:
            "crates/backend/src/usecases/u504_import_from_wildberries/docs/wb-api-marketplace-orders.md",
        raw: include_str!(
            "../../usecases/u504_import_from_wildberries/docs/wb-api-marketplace-orders.md"
        ),
    },
    EmbeddedKnowledgeSource {
        id: "wb-api-marketplace-supplies",
        source_path:
            "crates/backend/src/usecases/u504_import_from_wildberries/docs/wb-api-marketplace-supplies.md",
        raw: include_str!(
            "../../usecases/u504_import_from_wildberries/docs/wb-api-marketplace-supplies.md"
        ),
    },
    EmbeddedKnowledgeSource {
        id: "wb-api-documents",
        source_path:
            "crates/backend/src/usecases/u504_import_from_wildberries/docs/wb-api-documents.md",
        raw: include_str!("../../usecases/u504_import_from_wildberries/docs/wb-api-documents.md"),
    },
    EmbeddedKnowledgeSource {
        id: "wb-api-returns-claims",
        source_path:
            "crates/backend/src/usecases/u504_import_from_wildberries/docs/wb-api-returns-claims.md",
        raw: include_str!("../../usecases/u504_import_from_wildberries/docs/wb-api-returns-claims.md"),
    },
    EmbeddedKnowledgeSource {
        id: "ym-api-overview",
        source_path: "crates/backend/src/usecases/u503_import_from_yandex/docs/ym-api-overview.md",
        raw: include_str!("../../usecases/u503_import_from_yandex/docs/ym-api-overview.md"),
    },
    EmbeddedKnowledgeSource {
        id: "ym-api-campaigns",
        source_path: "crates/backend/src/usecases/u503_import_from_yandex/docs/ym-api-campaigns.md",
        raw: include_str!("../../usecases/u503_import_from_yandex/docs/ym-api-campaigns.md"),
    },
    EmbeddedKnowledgeSource {
        id: "ym-api-offer-mappings",
        source_path:
            "crates/backend/src/usecases/u503_import_from_yandex/docs/ym-api-offer-mappings.md",
        raw: include_str!("../../usecases/u503_import_from_yandex/docs/ym-api-offer-mappings.md"),
    },
    EmbeddedKnowledgeSource {
        id: "ym-api-orders",
        source_path: "crates/backend/src/usecases/u503_import_from_yandex/docs/ym-api-orders.md",
        raw: include_str!("../../usecases/u503_import_from_yandex/docs/ym-api-orders.md"),
    },
    EmbeddedKnowledgeSource {
        id: "ym-api-returns",
        source_path: "crates/backend/src/usecases/u503_import_from_yandex/docs/ym-api-returns.md",
        raw: include_str!("../../usecases/u503_import_from_yandex/docs/ym-api-returns.md"),
    },
    EmbeddedKnowledgeSource {
        id: "ym-api-united-netting",
        source_path:
            "crates/backend/src/usecases/u503_import_from_yandex/docs/ym-api-united-netting.md",
        raw: include_str!("../../usecases/u503_import_from_yandex/docs/ym-api-united-netting.md"),
    },
    EmbeddedKnowledgeSource {
        id: "ym-api-goods-realization",
        source_path:
            "crates/backend/src/usecases/u503_import_from_yandex/docs/ym-api-goods-realization.md",
        raw: include_str!("../../usecases/u503_import_from_yandex/docs/ym-api-goods-realization.md"),
    },
    EmbeddedKnowledgeSource {
        id: "ym-api-shows-sales",
        source_path: "crates/backend/src/usecases/u503_import_from_yandex/docs/ym-api-shows-sales.md",
        raw: include_str!("../../usecases/u503_import_from_yandex/docs/ym-api-shows-sales.md"),
    },
    EmbeddedKnowledgeSource {
        id: "ym-api-reports-machinery",
        source_path:
            "crates/backend/src/usecases/u503_import_from_yandex/docs/ym-api-reports-machinery.md",
        raw: include_str!("../../usecases/u503_import_from_yandex/docs/ym-api-reports-machinery.md"),
    },
];

// ─── Реализация ──────────────────────────────────────────────────────────────

impl KnowledgeBase {
    /// Загрузить словарь и все `*.md` файлы из директории `dir`, затем embedded-доки.
    pub fn load(dir: &Path) -> Self {
        let mut docs: HashMap<String, KnowledgeDoc> = HashMap::new();
        let mut diagnostics: Vec<String> = Vec::new();

        // 1. Разложить техдокументацию файлами — до обхода каталога, чтобы она
        //    попала в базу тем же путём, что и бизнес-статьи.
        materialize_app_docs(dir, &mut diagnostics);

        // 2. Словарь тегов. Отсутствует — база работает, все теги «неизвестные».
        let vocabulary = load_vocabulary(dir, &mut diagnostics);

        // 3. Статьи из каталога знаний: бизнес-база + разложенная техдокументация.
        let mut loaded = 0usize;
        if dir.exists() {
            for path in walk_markdown_files(dir) {
                let id = build_doc_id(dir, &path);
                if id.is_empty() {
                    continue;
                }
                match std::fs::read_to_string(&path) {
                    Ok(raw) => {
                        let mut doc = parse_doc(id, &raw, &vocabulary);
                        // Каталог сильнее frontmatter: `app/` и `generated/`
                        // принадлежат машине целиком, и файл, попавший туда без
                        // метки (или с чужой), не должен притворяться статьёй.
                        if let Some(kind) = kind_by_subdir(&doc.id) {
                            doc.kind = kind;
                        }
                        // У техдока показываем канонический путь в репозитории:
                        // копию в `app/` править бессмысленно — её перезапишет
                        // следующая загрузка.
                        doc.source_path = Some(match app_doc_source_path(&doc.id) {
                            Some(canonical) => canonical.to_string(),
                            None => path.display().to_string(),
                        });
                        doc.age_days = compute_age_days(&doc, Some(&path));
                        insert_doc(&mut docs, doc, &mut diagnostics);
                        loaded += 1;
                    }
                    Err(e) => {
                        diagnostics.push(format!("не читается '{}': {}", path.display(), e));
                        tracing::warn!("KnowledgeBase: cannot read '{}': {}", path.display(), e);
                    }
                }
            }
        } else {
            diagnostics.push(format!(
                "каталог базы знаний не существует: {}",
                dir.display()
            ));
            tracing::warn!(
                "KnowledgeBase: directory '{}' does not exist; only embedded docs available",
                dir.display()
            );
        }

        // 4. Производные индексы.
        let index = build_tag_index(&docs);
        let by_basename = build_basename_index(&docs);
        let back_links = build_back_links(&docs, &by_basename);
        let anchor_index = build_anchor_index(&docs);
        let retrieval = SearchIndex::build(docs.values());

        let unknown_tags: usize = docs.values().map(|d| d.unknown_tags.len()).sum();
        if unknown_tags > 0 {
            diagnostics.push(format!(
                "тегов вне словаря: {} (см. GET /api/kb/vocabulary)",
                unknown_tags
            ));
        }
        let unknown_anchors: usize = docs.values().map(|d| d.unknown_anchors.len()).sum();
        if unknown_anchors > 0 {
            diagnostics.push(format!(
                "якорей вне реестра метаданных: {} (поле entities ссылается на несуществующий объект)",
                unknown_anchors
            ));
        }
        let dangling = count_dangling_links(&docs, &by_basename, &index);
        if dangling > 0 {
            diagnostics.push(format!(
                "ссылок related/[[wiki]] в никуда: {} (не статья, не тег, не объект)",
                dangling
            ));
        }

        let count_kind = |kind: DocKind| docs.values().filter(|d| d.kind == kind).count();
        tracing::info!(
            "KnowledgeBase: loaded {} docs ({} business + {} app + {} generated) from '{}', {} vocabulary terms, {} anchored entities",
            loaded,
            count_kind(DocKind::Business),
            count_kind(DocKind::App),
            count_kind(DocKind::Generated),
            dir.display(),
            vocabulary.len(),
            anchor_index.len()
        );

        Self {
            index,
            docs,
            back_links,
            anchor_index,
            by_basename,
            retrieval,
            vocabulary,
            loaded_at: Utc::now(),
            diagnostics,
            dangling_links: dangling,
        }
    }

    /// Поиск по тегам (OR-семантика) — сохранён ради `find_page_help`,
    /// но исполняется новым движком, поэтому наследует нормализацию по словарю.
    pub fn search_by_tags(&self, tags: &[&str]) -> Vec<&KnowledgeDoc> {
        let query = super::kb_search::Query {
            tags: self.normalize_tags(tags),
            limit: super::kb_search::MAX_LIMIT,
            include_deprecated: true,
            ..Default::default()
        };
        self.search(&query)
            .into_iter()
            .map(|(doc, _)| doc)
            .collect()
    }

    /// Гибридный поиск: свободный текст + теги + граф. Возвращает документы
    /// вместе с пометкой `via` (не `None`, если статья пришла по графу).
    pub fn search(&self, query: &super::kb_search::Query) -> Vec<(&KnowledgeDoc, Option<String>)> {
        self.search_with_total(query).0
    }

    /// То же, плюс сколько статей совпало всего (до отсечки по `limit`).
    pub fn search_with_total(
        &self,
        query: &super::kb_search::Query,
    ) -> (Vec<(&KnowledgeDoc, Option<String>)>, usize) {
        let corpus = Corpus {
            docs: &self.docs,
            tag_index: &self.index,
            back_links: &self.back_links,
            anchor_index: &self.anchor_index,
            by_basename: &self.by_basename,
        };
        let outcome = super::kb_search::search(&self.retrieval, &corpus, query);
        let hits = outcome
            .hits
            .into_iter()
            .filter_map(|hit| self.docs.get(&hit.id).map(|doc| (doc, hit.via)))
            .collect();
        (hits, outcome.total_matched)
    }

    /// Привести произвольные теги к словарю. Тег вне словаря остаётся как есть:
    /// он проиндексирован и должен находиться.
    pub fn normalize_tags(&self, tags: &[&str]) -> Vec<String> {
        tags.iter()
            .map(|t| {
                self.vocabulary
                    .normalize(t)
                    .map(|c| c.to_string())
                    .unwrap_or_else(|| super::kb_vocabulary::normalize_form(t))
            })
            .filter(|t| !t.is_empty())
            .collect()
    }

    /// Получить документ по id.
    /// Статья по id. Короткое имя тоже принимается: техдоки переехали в `app/`,
    /// и ссылки вида `kb://article/general-ledger` из прошлых чатов, статистики
    /// и тикетов иначе перестали бы открываться. Неоднозначное имя не резолвим.
    pub fn get(&self, id: &str) -> Option<&KnowledgeDoc> {
        if let Some(doc) = self.docs.get(id) {
            return Some(doc);
        }
        match self.by_basename.get(id)?.as_slice() {
            [single] => self.docs.get(single),
            _ => None,
        }
    }

    /// Вернуть все документы.
    pub fn all_docs(&self) -> Vec<&KnowledgeDoc> {
        self.docs.values().collect()
    }

    pub fn vocabulary(&self) -> &Vocabulary {
        &self.vocabulary
    }

    pub fn diagnostics(&self) -> &[String] {
        &self.diagnostics
    }

    /// Ссылки в никуда — рабочий список куратора.
    pub fn dangling_links(&self) -> usize {
        self.dangling_links
    }

    /// Сколько объектов системы имеют хотя бы один привязанный документ.
    pub fn anchored_entity_count(&self) -> usize {
        self.anchor_index.len()
    }

    pub fn loaded_at(&self) -> DateTime<Utc> {
        self.loaded_at
    }

    /// Кто ссылается на этот документ.
    pub fn back_links(&self, id: &str) -> &[String] {
        self.back_links.get(id).map(|v| v.as_slice()).unwrap_or(&[])
    }

    /// Сколько статей несёт данный канонический тег.
    pub fn tag_count(&self, tag: &str) -> usize {
        self.index.get(tag).map(|v| v.len()).unwrap_or(0)
    }

    /// Документы, привязанные якорем к объекту системы. Принимает любое имя
    /// сущности (`a012`, `a012_wb_sales`) — резолвится тем же правилом, что и
    /// реестр метаданных.
    pub fn docs_for_anchor(&self, entity: &str) -> Vec<&KnowledgeDoc> {
        let Some(code) = canonical_anchor(entity) else {
            return Vec::new();
        };
        self.anchor_index
            .get(&code)
            .map(|ids| ids.iter().filter_map(|id| self.docs.get(id)).collect())
            .unwrap_or_default()
    }

    /// Подсказать теги по свободному тексту — вход в базу, когда поиск пуст.
    pub fn suggest_tags(&self, text: &str, limit: usize) -> Vec<String> {
        let terms = super::kb_search::tokenize(text);
        let mut out: Vec<String> = self
            .vocabulary
            .suggest(&terms, limit)
            .into_iter()
            .map(|s| s.to_string())
            .collect();
        if out.len() < limit {
            // Добираем самыми населёнными тегами: хоть какой-то вход в базу.
            let mut by_count: Vec<(&String, usize)> =
                self.index.iter().map(|(t, ids)| (t, ids.len())).collect();
            by_count.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));
            for (tag, _) in by_count {
                if out.len() >= limit {
                    break;
                }
                if !out.contains(tag) {
                    out.push(tag.clone());
                }
            }
        }
        out
    }

    /// Ближайшие по написанию id — ответ на промах `get_knowledge`.
    /// Раньше на промахе выдавался список ВСЕХ id, что съедало контекст.
    pub fn did_you_mean(&self, wanted: &str, limit: usize) -> Vec<&KnowledgeDoc> {
        let needle = super::kb_vocabulary::normalize_form(wanted);
        if needle.is_empty() {
            return Vec::new();
        }
        let mut scored: Vec<(usize, &KnowledgeDoc)> = self
            .docs
            .values()
            .filter_map(|doc| {
                let id = super::kb_vocabulary::normalize_form(&doc.id);
                let title = super::kb_vocabulary::normalize_form(&doc.title);
                let score = shared_prefix_len(&needle, &id)
                    .max(shared_prefix_len(&needle, &title))
                    .max(if id.contains(&needle) || needle.contains(&id) {
                        needle.chars().count()
                    } else {
                        0
                    });
                (score >= 3).then_some((score, doc))
            })
            .collect();
        scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.id.cmp(&b.1.id)));
        scored.into_iter().take(limit).map(|(_, d)| d).collect()
    }
}

fn shared_prefix_len(a: &str, b: &str) -> usize {
    a.chars().zip(b.chars()).take_while(|(x, y)| x == y).count()
}

fn load_vocabulary(dir: &Path, diagnostics: &mut Vec<String>) -> Vocabulary {
    let path = dir.join(VOCABULARY_FILE);
    match std::fs::read_to_string(&path) {
        Ok(raw) => {
            let vocabulary = Vocabulary::parse(&raw);
            if vocabulary.is_empty() {
                diagnostics.push(format!("{} пуст или не разобран", VOCABULARY_FILE));
            }
            vocabulary
        }
        Err(_) => {
            diagnostics.push(format!(
                "{} отсутствует — все теги считаются вне словаря",
                VOCABULARY_FILE
            ));
            Vocabulary::default()
        }
    }
}

fn build_tag_index(docs: &HashMap<String, KnowledgeDoc>) -> HashMap<String, Vec<String>> {
    let mut index: HashMap<String, Vec<String>> = HashMap::new();
    let mut ids: Vec<&String> = docs.keys().collect();
    ids.sort(); // детерминированный порядок внутри бакета
    for id in ids {
        let doc = &docs[id];
        for tag in &doc.canonical_tags {
            let bucket = index.entry(tag.clone()).or_default();
            if !bucket.contains(id) {
                bucket.push(id.clone());
            }
        }
    }
    index
}

/// Последний сегмент id (`app__wb-api-overview` → `wb-api-overview`).
fn id_basename(id: &str) -> &str {
    id.rsplit("__").next().unwrap_or(id)
}

/// Корпус по каталогу, в котором лежит файл (id склеен из сегментов пути).
/// `None` — файл лежит вне машинных каталогов, корпус берётся из frontmatter.
fn kind_by_subdir(doc_id: &str) -> Option<DocKind> {
    match doc_id.split_once("__")?.0 {
        APP_DOCS_SUBDIR => Some(DocKind::App),
        GENERATED_DOCS_SUBDIR => Some(DocKind::Generated),
        _ => None,
    }
}

/// Индекс коротких имён: ссылка `general-ledger` должна вести в `app__general-ledger`.
fn build_basename_index(docs: &HashMap<String, KnowledgeDoc>) -> HashMap<String, Vec<String>> {
    let mut index: HashMap<String, Vec<String>> = HashMap::new();
    let mut ids: Vec<&String> = docs.keys().collect();
    ids.sort();
    for id in ids {
        let base = id_basename(id);
        if base == id {
            continue; // короткое имя и есть id — отдельная запись не нужна
        }
        index.entry(base.to_string()).or_default().push(id.clone());
    }
    index
}

/// Код объекта → документы, привязанные к нему якорем.
fn build_anchor_index(docs: &HashMap<String, KnowledgeDoc>) -> HashMap<String, Vec<String>> {
    let mut index: HashMap<String, Vec<String>> = HashMap::new();
    let mut ids: Vec<&String> = docs.keys().collect();
    ids.sort();
    for id in ids {
        for anchor in &docs[id].anchors {
            let bucket = index.entry(anchor.clone()).or_default();
            if !bucket.contains(id) {
                bucket.push(id.clone());
            }
        }
    }
    index
}

/// Ссылки, которые не ведут никуда: ни в статью (в т.ч. по короткому имени),
/// ни в тег, ни в объект системы. Рабочий список куратора.
fn count_dangling_links(
    docs: &HashMap<String, KnowledgeDoc>,
    by_basename: &HashMap<String, Vec<String>>,
    tag_index: &HashMap<String, Vec<String>>,
) -> usize {
    docs.values()
        .flat_map(|doc| doc.related.iter())
        .filter(|entry| {
            let normalized = super::kb_vocabulary::normalize_form(entry);
            !docs.contains_key(*entry)
                && !by_basename.contains_key(*entry)
                && !tag_index.contains_key(&normalized)
                && canonical_anchor(entry).is_none()
        })
        .count()
}

/// Обратные рёбра графа: если `A.related = [B]`, то из B достижим A.
fn build_back_links(
    docs: &HashMap<String, KnowledgeDoc>,
    by_basename: &HashMap<String, Vec<String>>,
) -> HashMap<String, Vec<String>> {
    let mut back: HashMap<String, Vec<String>> = HashMap::new();
    let mut ids: Vec<&String> = docs.keys().collect();
    ids.sort();
    for id in ids {
        for target in &docs[id].related {
            // Ссылка резолвится в статью напрямую либо по короткому имени;
            // всё прочее — тег или код объекта, они разрешаются при поиске.
            let targets: Vec<String> = if docs.contains_key(target) {
                vec![target.clone()]
            } else {
                by_basename.get(target).cloned().unwrap_or_default()
            };
            for target in targets {
                let bucket = back.entry(target).or_default();
                if !bucket.contains(id) {
                    bucket.push(id.clone());
                }
            }
        }
    }
    back
}

fn compute_age_days(doc: &KnowledgeDoc, path: Option<&Path>) -> Option<u32> {
    let today = Utc::now().date_naive();
    let reference = doc.verified.or(doc.updated).or_else(|| {
        let modified = std::fs::metadata(path?).ok()?.modified().ok()?;
        let stamp: DateTime<Utc> = modified.into();
        Some(stamp.date_naive())
    })?;
    Some((today - reference).num_days().max(0) as u32)
}

fn walk_markdown_files(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut stack = vec![dir.to_path_buf()];

    while let Some(current_dir) = stack.pop() {
        let entries = match std::fs::read_dir(&current_dir) {
            Ok(entries) => entries,
            Err(error) => {
                tracing::warn!(
                    "KnowledgeBase: cannot read directory '{}': {}",
                    current_dir.display(),
                    error
                );
                continue;
            }
        };

        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();

            if path.is_dir() {
                // Служебные каталоги Obsidian и шаблоны статьями не являются.
                if matches!(name.as_str(), ".obsidian" | ".trash" | "_templates") {
                    continue;
                }
                stack.push(path);
                continue;
            }

            // Файлы на `_` зарезервированы (напр. `_vocabulary.md`) и читаются отдельно.
            if name.starts_with('_') {
                continue;
            }

            if path.extension().and_then(|ext| ext.to_str()) == Some("md") {
                files.push(path);
            }
        }
    }

    files.sort();
    files
}

fn build_doc_id(base_dir: &Path, path: &Path) -> String {
    let relative = path.strip_prefix(base_dir).unwrap_or(path);
    let segments = relative
        .iter()
        .filter_map(|part| part.to_str())
        .map(|part| part.trim_end_matches(".md"))
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();

    if segments.is_empty() {
        String::new()
    } else if segments.len() == 1 {
        segments[0].to_string()
    } else {
        segments.join("__")
    }
}

/// Канонический путь техдока в репозитории по его id в базе знаний.
///
/// id разложенного файла — `app__<id источника>`: обход каталога склеивает
/// сегменты пути через `__` (см. `build_doc_id`).
fn app_doc_source_path(doc_id: &str) -> Option<&'static str> {
    let stem = doc_id.strip_prefix(&format!("{APP_DOCS_SUBDIR}__"))?;
    EMBEDDED_LLM_DOCS
        .iter()
        .find(|source| source.id == stem)
        .map(|source| source.source_path)
}

/// Проставить `kind: app` во frontmatter копии.
///
/// Метка ставится здесь, а не в самих файлах, потому что «техдок» — это ровно
/// «состоит в `EMBEDDED_LLM_DOCS`». Автор нового файла не сможет забыть поле,
/// и никакой техдок не утечёт в базу под видом бизнес-статьи.
fn ensure_app_kind(raw: &str) -> String {
    let Some(rest) = raw
        .strip_prefix("---\n")
        .or_else(|| raw.strip_prefix("---\r\n"))
    else {
        return format!("---\nkind: {APP_DOCS_SUBDIR}\n---\n\n{}", raw.trim_start());
    };
    let Some(end) = rest.find("\n---") else {
        return format!("---\nkind: {APP_DOCS_SUBDIR}\n---\n\n{}", raw.trim_start());
    };
    if rest[..end]
        .lines()
        .any(|line| line.trim_start().starts_with("kind:"))
    {
        return raw.to_string();
    }
    format!("---\nkind: {APP_DOCS_SUBDIR}\n{rest}")
}

/// Разложить встроенную техдокументацию файлами в `<knowledge>/app/`.
///
/// Пишем только изменившееся: `reload` дёргается из UI и из фоновых задач, а
/// безусловная перезапись обнуляла бы mtime — по нему считается возраст статьи
/// у файлов без даты во frontmatter.
fn materialize_app_docs(dir: &Path, diagnostics: &mut Vec<String>) {
    let target_dir = dir.join(APP_DOCS_SUBDIR);
    if let Err(e) = std::fs::create_dir_all(&target_dir) {
        diagnostics.push(format!(
            "каталог техдокументации '{}' не создан: {e}",
            target_dir.display()
        ));
        tracing::warn!(
            "KnowledgeBase: cannot create '{}': {}",
            target_dir.display(),
            e
        );
        return;
    }

    let mut expected: Vec<String> = Vec::with_capacity(EMBEDDED_LLM_DOCS.len());
    for source in EMBEDDED_LLM_DOCS {
        let file_name = format!("{}.md", source.id);
        let path = target_dir.join(&file_name);
        expected.push(file_name);
        let content = ensure_app_kind(source.raw);
        if std::fs::read_to_string(&path).is_ok_and(|existing| existing == content) {
            continue;
        }
        if let Err(e) = std::fs::write(&path, &content) {
            diagnostics.push(format!("техдок '{}' не записан: {e}", path.display()));
            tracing::warn!("KnowledgeBase: cannot write '{}': {}", path.display(), e);
        }
    }

    for stale in prune_stale_docs(&target_dir, &expected) {
        tracing::info!("KnowledgeBase: удалён устаревший техдок '{}'", stale);
    }
}

fn sanitize_relative_kb_path(relative_path: &str) -> anyhow::Result<PathBuf> {
    let path = Path::new(relative_path);
    if path.is_absolute() {
        return Err(anyhow::anyhow!(
            "Knowledge document path must be relative: {}",
            relative_path
        ));
    }

    let mut clean = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => clean.push(part),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(anyhow::anyhow!(
                    "Knowledge document path contains forbidden component: {}",
                    relative_path
                ));
            }
        }
    }

    if clean.extension().and_then(|ext| ext.to_str()) != Some("md") {
        return Err(anyhow::anyhow!(
            "Knowledge document path must end with .md: {}",
            relative_path
        ));
    }

    // `app/` и `generated/` принадлежат машине: первый — копия техдокументации
    // (канон в репозитории), второй — карты генератора. Правка здесь молча
    // пропадёт при следующей загрузке, а статья — выпадет из обычной выдачи.
    let machine_dir = clean.components().next().and_then(|c| {
        [APP_DOCS_SUBDIR, GENERATED_DOCS_SUBDIR]
            .into_iter()
            .find(|sub| c.as_os_str().eq_ignore_ascii_case(sub))
    });
    if let Some(subdir) = machine_dir {
        return Err(anyhow::anyhow!(
            "'{subdir}/' заполняется приложением и перезаписывается при загрузке базы; \
             статью кладите в корень базы знаний или в тематический подкаталог: {relative_path}"
        ));
    }

    Ok(clean)
}

/// Убрать из машинного каталога `*.md`, которых нет в списке ожидаемых.
///
/// Каталог принадлежит генератору целиком, поэтому лишний файл — это след
/// переименования: он продолжал бы грузиться в базу и отвечать на поиск от
/// имени документа, которого больше нет.
pub fn prune_stale_docs(dir: &Path, expected: &[String]) -> Vec<String> {
    let mut removed = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return removed;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if expected.iter().any(|e| e == name) {
            continue;
        }
        match std::fs::remove_file(&path) {
            Ok(_) => removed.push(name.to_string()),
            Err(e) => tracing::warn!("KnowledgeBase: cannot remove '{}': {}", path.display(), e),
        }
    }
    removed
}

// ─── Парсинг frontmatter ─────────────────────────────────────────────────────

/// Оценка числа токенов без внешнего токенизатора.
///
/// Кириллица режется моделями заметно мельче латиницы (примерно 2.7 символа на
/// токен против 4.0), поэтому считаем по классам символов. Значение вычисляется
/// при загрузке и НЕ пишется в файл: авторская цифра протухала бы от первой же
/// правки опечатки и порождала цикл «записал → пересчитал → перезаписал».
pub fn estimate_tokens(text: &str) -> u32 {
    let (mut cyrillic, mut latin, mut other) = (0f32, 0f32, 0f32);
    for ch in text.chars() {
        if matches!(ch, 'а'..='я' | 'А'..='Я' | 'ё' | 'Ё') {
            cyrillic += 1.0;
        } else if ch.is_ascii_alphanumeric() {
            latin += 1.0;
        } else {
            other += 1.0;
        }
    }
    (cyrillic / 2.7 + latin / 4.0 + other / 3.0).ceil() as u32
}

/// Первый содержательный абзац тела — запасной `summary` для статей без поля.
fn derive_summary(content: &str) -> String {
    let paragraph = content
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#') && !line.starts_with("---"))
        .take(2)
        .collect::<Vec<_>>()
        .join(" ");
    let mut summary: String = paragraph.chars().take(200).collect();
    if paragraph.chars().count() > 200 {
        summary.push('…');
    }
    summary
}

/// Разобрать MD-файл: извлечь frontmatter и тело.
fn parse_doc(id: String, raw: &str, vocabulary: &Vocabulary) -> KnowledgeDoc {
    let (frontmatter, content) = split_frontmatter(raw);
    let fm = frontmatter.as_deref();
    let scalar = |key: &str| fm.and_then(|f| parse_scalar(f, key));
    let list = |key: &str| fm.and_then(|f| parse_list(f, key)).unwrap_or_default();

    let title = scalar("title").unwrap_or_else(|| id.replace('-', " "));
    let tags = list("tags");
    let content = content.trim_start().to_string();

    // Теги приводим к словарю; то, чего в словаре нет, остаётся как есть и
    // попадает в рабочий список куратора — отклонять теги нельзя, иначе
    // редактирование в Obsidian становится враждебным.
    let mut canonical_tags = Vec::new();
    let mut unknown_tags = Vec::new();
    for tag in &tags {
        match vocabulary.normalize(tag) {
            Some(canonical) => {
                let canonical = canonical.to_string();
                if !canonical_tags.contains(&canonical) {
                    canonical_tags.push(canonical);
                }
            }
            None => {
                let raw_form = super::kb_vocabulary::normalize_form(tag);
                if raw_form.is_empty() {
                    continue;
                }
                if !canonical_tags.contains(&raw_form) {
                    canonical_tags.push(raw_form.clone());
                }
                if !unknown_tags.contains(&raw_form) {
                    unknown_tags.push(raw_form);
                }
            }
        }
    }

    let summary = scalar("summary").unwrap_or_else(|| derive_summary(&content));
    let stars = fm
        .and_then(|f| parse_u32(f, "stars"))
        .map(|s| s.clamp(1, 5) as u8);

    // `[[wiki-ссылки]]` — те же рёбра графа, что и `related`: в Obsidian связь
    // ставят в тексте, и без этого шага она для LLM не существует.
    let mut related = list("related");
    for link in extract_wiki_links(&content) {
        if !related.contains(&link) {
            related.push(link);
        }
    }

    // Якоря: явные `entities` плюс коды объектов, уже лежащие в `related`.
    // Второе даёт рабочий индекс без единой правки файлов.
    let entities = list("entities");
    let mut anchors: Vec<String> = Vec::new();
    let mut unknown_anchors: Vec<String> = Vec::new();
    for entity in &entities {
        match canonical_anchor(entity) {
            Some(code) if !anchors.contains(&code) => anchors.push(code),
            Some(_) => {}
            None => unknown_anchors.push(entity.clone()),
        }
    }
    for entry in &related {
        if let Some(code) = canonical_anchor(entry) {
            if !anchors.contains(&code) {
                anchors.push(code);
            }
        }
    }

    KnowledgeDoc {
        token_cost: estimate_tokens(&content),
        id,
        title,
        tags,
        related,
        entities,
        anchors,
        unknown_anchors,
        content,
        source_path: None,
        uid: scalar("uid"),
        status: scalar("status")
            .map(|s| KbStatus::from_str(&s))
            .unwrap_or_default(),
        stars,
        summary,
        aliases: list("aliases"),
        updated: fm.and_then(|f| parse_date(f, "updated")),
        verified: fm.and_then(|f| parse_date(f, "verified")),
        ttl_days: fm.and_then(|f| parse_u32(f, "ttl_days")),
        source_chats: list("source_chats"),
        author: scalar("author").unwrap_or_else(|| "human".to_string()),
        canonical_tags,
        unknown_tags,
        // `kind: app` — техдокументация приложения из `app/`, `kind: generated` —
        // карта из `generated/`; всё остальное (в т.ч. файлы без поля) —
        // бизнес-знание организации.
        kind: scalar("kind")
            .map(|k| DocKind::from_str(&k))
            .unwrap_or_default(),
        age_days: None,
    }
}

/// `[[статья]]`, `[[статья|подпись]]`, `[[статья#раздел]]` → `статья`.
fn extract_wiki_links(content: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for chunk in content.split("[[").skip(1) {
        let Some((inner, _)) = chunk.split_once("]]") else {
            continue;
        };
        let target = inner
            .split(['|', '#'])
            .next()
            .unwrap_or_default()
            .trim()
            .to_string();
        if !target.is_empty() && !out.contains(&target) {
            out.push(target);
        }
    }
    out
}

/// Имя объекта системы → канонический код-якорь (`a012_wb_sales` → `a012`).
/// `None` — такого объекта в реестре метаданных нет, значит это не якорь.
pub fn canonical_anchor(reference: &str) -> Option<String> {
    let reference = reference.trim();
    if reference.is_empty() {
        return None;
    }
    super::metadata_registry::METADATA_REGISTRY
        .canonical_index(reference)
        .map(|code| code.to_string())
}

/// Вставить документ, разрешив коллизию id.
///
/// Техдокументация раскладывается файлами в `app/` и грузится общим обходом,
/// поэтому в норме коллизий уже нет. Проверка `DocKind::App` остаётся защитой:
/// док, пришедший поверх бизнес-статьи, переезжает на `app__<id>`, а не затирает
/// её молча.
fn insert_doc(
    docs: &mut HashMap<String, KnowledgeDoc>,
    mut doc: KnowledgeDoc,
    diagnostics: &mut Vec<String>,
) {
    if let Some(existing) = docs.get(&doc.id) {
        match doc.kind {
            DocKind::App if !existing.is_embedded() => {
                let renamed = format!("app__{}", doc.id);
                diagnostics.push(format!(
                    "коллизия id '{}': встроенный документ перенесён в '{}', бизнес-статья сохранена",
                    doc.id, renamed
                ));
                tracing::warn!(
                    "KnowledgeBase: embedded doc '{}' collides with a business article; re-keyed to '{}'",
                    doc.id,
                    renamed
                );
                doc.id = renamed;
            }
            _ => {
                diagnostics.push(format!(
                    "коллизия id '{}': документ перезаписан ({:?})",
                    doc.id, doc.kind
                ));
            }
        }
    }
    docs.insert(doc.id.clone(), doc);
}

// ─── Тесты ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"---
title: Акции Wildberries
tags: [a020, wildberries, акции, скидки]
related: [a006, a012]
updated: 2026-02-25
---

# Акции Wildberries

Текст статьи.
"#;

    const EXTENDED: &str = r#"---
title: Лаг выкупа
uid: kb-1a2b3c4d
status: draft
tags: [воронка, вб, выкуп]
related: [funnel-overview]
summary: Почему текущий месяц по выкупу всегда выглядит хуже, чем есть.
stars: 4
ttl_days: 90
updated: 2026-08-01
verified:
source_chats: [chat-1]
author: llm
---

# Лаг выкупа

Тело статьи.
"#;

    fn vocab() -> Vocabulary {
        Vocabulary::parse("## wildberries\n- aliases: [вб, wb]\n\n## воронка\n- group: funnel\n")
    }

    /// Собрать базу из готовых документов — минуя файловую систему.
    fn kb_from(docs: Vec<KnowledgeDoc>) -> KnowledgeBase {
        let docs: HashMap<String, KnowledgeDoc> =
            docs.into_iter().map(|d| (d.id.clone(), d)).collect();
        let by_basename = build_basename_index(&docs);
        KnowledgeBase {
            index: build_tag_index(&docs),
            back_links: build_back_links(&docs, &by_basename),
            anchor_index: build_anchor_index(&docs),
            by_basename,
            retrieval: SearchIndex::build(docs.values()),
            docs,
            vocabulary: vocab(),
            loaded_at: Utc::now(),
            diagnostics: Vec::new(),
            dangling_links: 0,
        }
    }

    #[test]
    fn anchors_come_from_related_without_touching_files() {
        // Главное свойство якорей: индекс работает на уже написанных статьях,
        // где коды объектов лежат в `related`, а поля `entities` нет вовсе.
        let doc = parse_doc("wb-promotions".to_string(), SAMPLE, &Vocabulary::default());
        assert!(doc.anchors.contains(&"a006".to_string()));
        assert!(doc.anchors.contains(&"a012".to_string()));
        assert!(doc.unknown_anchors.is_empty());

        // Полное имя таблицы — тот же якорь, что и короткий код.
        let full = parse_doc(
            "x".to_string(),
            "---\ntitle: T\nentities: [a012_wb_sales, нет-такого]\n---\n\n# T\n",
            &Vocabulary::default(),
        );
        assert_eq!(full.anchors, vec!["a012".to_string()]);
        assert_eq!(full.unknown_anchors, vec!["нет-такого".to_string()]);
    }

    #[test]
    fn wiki_links_become_graph_edges() {
        let doc = parse_doc(
            "drr".to_string(),
            "---\ntitle: ДРР\nrelated: [a008]\n---\n\n# ДРР\n\nСм. [[a012]] и [[funnel-overview|воронку]].\n",
            &Vocabulary::default(),
        );
        assert!(doc.related.contains(&"a012".to_string()));
        assert!(doc.related.contains(&"funnel-overview".to_string()));
        // Ссылка из тела тоже даёт якорь.
        assert!(doc.anchors.contains(&"a012".to_string()));
    }

    #[test]
    fn short_link_resolves_into_subdirectory_article() {
        // Техдоки переехали в `app/`, а записи `related` в файлах остались
        // короткими — без резолва по короткому имени граф рвётся.
        let kb = kb_from(vec![
            KnowledgeDoc {
                id: "app__general-ledger".into(),
                title: "GL".into(),
                kind: DocKind::App,
                ..Default::default()
            },
            KnowledgeDoc {
                id: "fina-layer".into(),
                title: "Слой fina".into(),
                related: vec!["general-ledger".into()],
                ..Default::default()
            },
        ]);
        assert_eq!(kb.back_links("app__general-ledger"), ["fina-layer"]);
    }

    #[test]
    fn generated_corpus_stays_out_of_default_search() {
        let kb = kb_from(vec![
            KnowledgeDoc {
                id: "profile".into(),
                title: "Профиль данных".into(),
                content: "воронка выкуп заказы".into(),
                kind: DocKind::Generated,
                ..Default::default()
            },
            KnowledgeDoc {
                id: "funnel".into(),
                title: "Воронка".into(),
                content: "воронка выкуп заказы".into(),
                ..Default::default()
            },
        ]);

        let found = |kinds: Vec<DocKind>| -> Vec<String> {
            kb.search(&super::super::kb_search::Query {
                text: "воронка выкуп".into(),
                kinds,
                ..Default::default()
            })
            .into_iter()
            .map(|(doc, _)| doc.id.clone())
            .collect()
        };

        assert_eq!(found(vec![DocKind::Business, DocKind::App]), ["funnel"]);
        assert_eq!(found(vec![DocKind::Generated]), ["profile"]);
        // По id сгенерированный документ читается всегда — фильтр только у поиска.
        assert!(kb.get("profile").is_some());
    }

    #[test]
    fn entity_filter_is_hard_and_works_without_text() {
        let kb = kb_from(vec![
            KnowledgeDoc {
                id: "about-a012".into(),
                title: "Продажи WB".into(),
                anchors: vec!["a012".into()],
                content: "текст про продажи".into(),
                ..Default::default()
            },
            KnowledgeDoc {
                id: "about-other".into(),
                title: "Прочее".into(),
                anchors: vec!["a013".into()],
                content: "текст про продажи".into(),
                ..Default::default()
            },
        ]);

        let hits: Vec<String> = kb
            .search(&super::super::kb_search::Query {
                entities: vec!["a012".into()],
                ..Default::default()
            })
            .into_iter()
            .map(|(doc, _)| doc.id.clone())
            .collect();
        assert_eq!(hits, ["about-a012"]);
        assert_eq!(kb.docs_for_anchor("a012").len(), 1);
    }

    #[test]
    fn test_parse_frontmatter() {
        let doc = parse_doc("wb-promotions".to_string(), SAMPLE, &Vocabulary::default());
        assert_eq!(doc.title, "Акции Wildberries");
        assert!(doc.tags.contains(&"a020".to_string()));
        assert!(doc.tags.contains(&"скидки".to_string()));
        assert!(doc.related.contains(&"a006".to_string()));
        assert!(doc.content.contains("# Акции Wildberries"));
        assert!(!doc.content.contains("---"));
    }

    #[test]
    fn legacy_article_keeps_working_defaults() {
        // Статья без новых полей обязана вести себя ровно как до правки.
        let doc = parse_doc("wb-promotions".to_string(), SAMPLE, &Vocabulary::default());
        assert_eq!(doc.status, KbStatus::Active);
        assert_eq!(doc.stars, None);
        assert_eq!(doc.author, "human");
        assert!(doc.uid.is_none());
        // `updated` раньше не парсился вообще — теперь читается.
        assert_eq!(doc.updated, NaiveDate::from_ymd_opt(2026, 2, 25));
        // summary выводится из тела, а не остаётся пустым.
        assert!(!doc.summary.is_empty());
        assert!(doc.token_cost > 0);
    }

    #[test]
    fn parses_extended_frontmatter() {
        let doc = parse_doc("funnel-buyout-lag".to_string(), EXTENDED, &vocab());
        assert_eq!(doc.uid.as_deref(), Some("kb-1a2b3c4d"));
        assert_eq!(doc.status, KbStatus::Draft);
        assert_eq!(doc.stars, Some(4));
        assert_eq!(doc.ttl_days, Some(90));
        assert_eq!(doc.author, "llm");
        assert_eq!(doc.source_chats, vec!["chat-1"]);
        assert!(doc.summary.starts_with("Почему текущий месяц"));
        // Пустое `verified:` — это «не верифицировано», а не ошибка разбора.
        assert!(doc.verified.is_none());
        // Алиас `вб` приведён к каноническому тегу.
        assert!(doc.canonical_tags.contains(&"wildberries".to_string()));
        // `выкуп` вне словаря — сохранён и помечен.
        assert!(doc.canonical_tags.contains(&"выкуп".to_string()));
        assert_eq!(doc.unknown_tags, vec!["выкуп".to_string()]);
        assert_eq!(doc.metrics_key(), "kb-1a2b3c4d");
    }

    #[test]
    fn metrics_key_falls_back_to_doc_id() {
        let doc = parse_doc("wb-promotions".to_string(), SAMPLE, &Vocabulary::default());
        assert_eq!(doc.metrics_key(), "wb-promotions");
    }

    #[test]
    fn test_search_by_tags() {
        let kb = kb_from(vec![parse_doc(
            "wb-promotions".to_string(),
            SAMPLE,
            &Vocabulary::default(),
        )]);
        let results = kb.search_by_tags(&["a020"]);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "wb-promotions");

        assert!(kb.search_by_tags(&["nonexistent"]).is_empty());
    }

    #[test]
    fn search_by_tags_resolves_aliases_through_vocabulary() {
        let kb = kb_from(vec![parse_doc(
            "funnel-buyout-lag".to_string(),
            EXTENDED,
            &vocab(),
        )]);
        // Статья тегирована `вб`; ищем каноническим `wildberries` и наоборот.
        assert_eq!(kb.search_by_tags(&["wildberries"]).len(), 1);
        assert_eq!(kb.search_by_tags(&["WB"]).len(), 1);
    }

    #[test]
    fn full_text_search_finds_article_without_matching_tag() {
        // Главный чинимый баг: статья была невидима без точного тега.
        let kb = kb_from(vec![parse_doc(
            "wb-promotions".to_string(),
            SAMPLE,
            &Vocabulary::default(),
        )]);
        let hits = kb.search(&super::super::kb_search::Query {
            text: "акции wildberries".into(),
            ..Default::default()
        });
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].0.id, "wb-promotions");
    }

    #[test]
    fn embedded_doc_does_not_clobber_business_article() {
        let mut docs = HashMap::new();
        let mut diagnostics = Vec::new();

        let mut business = parse_doc("general-ledger".into(), SAMPLE, &Vocabulary::default());
        business.kind = DocKind::Business;
        insert_doc(&mut docs, business, &mut diagnostics);

        let mut embedded = parse_doc(
            "general-ledger".into(),
            "# Техдок\n",
            &Vocabulary::default(),
        );
        embedded.kind = DocKind::App;
        insert_doc(&mut docs, embedded, &mut diagnostics);

        assert_eq!(docs["general-ledger"].title, "Акции Wildberries");
        assert!(docs.contains_key("app__general-ledger"));
        assert!(diagnostics.iter().any(|d| d.contains("коллизия id")));
    }

    #[test]
    fn kind_app_marks_doc_as_technical() {
        let business = parse_doc("wb-promotions".into(), SAMPLE, &Vocabulary::default());
        assert!(!business.is_embedded(), "статья без `kind` — бизнес-знание");

        let tech = parse_doc(
            "app__wb-api-sales".into(),
            "---\ntitle: T\nkind: app\n---\n\n# T\n",
            &Vocabulary::default(),
        );
        assert!(tech.is_embedded());
    }

    #[test]
    fn ensure_app_kind_stamps_frontmatter_once() {
        // Есть frontmatter без `kind` — поле добавляется, остальное не трогаем.
        let stamped = ensure_app_kind("---\ntitle: T\ntags: [a]\n---\n\n# T\n");
        assert!(stamped.starts_with("---\nkind: app\ntitle: T\n"));
        assert!(stamped.contains("tags: [a]"));

        // Повторный проход ничего не меняет — иначе файл переписывался бы вечно.
        assert_eq!(ensure_app_kind(&stamped), stamped);

        // Frontmatter отсутствует — создаётся минимальный.
        let bare = ensure_app_kind("# Только тело\n");
        assert_eq!(bare, "---\nkind: app\n---\n\n# Только тело\n");
        assert!(parse_doc("x".into(), &bare, &Vocabulary::default()).is_embedded());
    }

    #[test]
    fn machine_subdirs_are_not_writable_through_kb_tools() {
        assert!(sanitize_relative_kb_path("app/wb-api-sales.md").is_err());
        assert!(sanitize_relative_kb_path("generated/data-profile.md").is_err());
        assert!(sanitize_relative_kb_path("funnel-overview.md").is_ok());
        assert!(sanitize_relative_kb_path("marketplaces/wb.md").is_ok());
    }

    #[test]
    fn corpus_is_decided_by_directory_not_by_frontmatter() {
        // Файл в машинном каталоге — машинный, что бы ни было написано в шапке.
        assert_eq!(kind_by_subdir("app__wb-api-sales"), Some(DocKind::App));
        assert_eq!(
            kind_by_subdir("generated__data-profile"),
            Some(DocKind::Generated)
        );
        // Статья в корне и статья в тематическом подкаталоге — бизнес-знание.
        assert_eq!(kind_by_subdir("app"), None);
        assert_eq!(kind_by_subdir("marketplaces__wb"), None);
    }

    #[test]
    fn short_id_still_opens_the_moved_tech_doc() {
        // Техдоки переехали в `app/`, а ссылки `kb://article/<id>` из прошлых
        // чатов и строки статистики остались короткими.
        let kb = kb_from(vec![
            KnowledgeDoc {
                id: "app__general-ledger".into(),
                title: "GL".into(),
                kind: DocKind::App,
                ..Default::default()
            },
            KnowledgeDoc {
                id: "fina-layer".into(),
                title: "Слой fina".into(),
                ..Default::default()
            },
        ]);
        assert_eq!(
            kb.get("general-ledger").map(|d| d.id.as_str()),
            Some("app__general-ledger")
        );
        assert_eq!(
            kb.get("fina-layer").map(|d| d.id.as_str()),
            Some("fina-layer")
        );
        assert!(kb.get("нет-такой").is_none());
    }

    #[test]
    fn app_doc_source_path_points_at_repo_canon() {
        let first = EMBEDDED_LLM_DOCS.first().expect("список техдоков пуст");
        assert_eq!(
            app_doc_source_path(&format!("app__{}", first.id)),
            Some(first.source_path)
        );
        assert_eq!(app_doc_source_path("funnel-overview"), None);
    }

    #[test]
    fn back_links_are_built_from_related() {
        let mut child = parse_doc("child".into(), "# Ч\n", &Vocabulary::default());
        child.related = vec!["parent".into()];
        let parent = parse_doc("parent".into(), "# Р\n", &Vocabulary::default());
        let kb = kb_from(vec![child, parent]);
        assert_eq!(kb.back_links("parent"), ["child"]);
        assert!(kb.back_links("child").is_empty());
    }

    #[test]
    fn did_you_mean_replaces_dumping_every_id() {
        let kb = kb_from(vec![
            parse_doc("funnel-overview".into(), "# А\n", &Vocabulary::default()),
            parse_doc("funnel-buyout-lag".into(), "# Б\n", &Vocabulary::default()),
            parse_doc("wb-commissions".into(), "# В\n", &Vocabulary::default()),
        ]);
        let suggestions = kb.did_you_mean("funnel-overvew", 5);
        assert!(suggestions.iter().any(|d| d.id == "funnel-overview"));
        assert!(suggestions.len() < 3, "выдаётся не весь каталог");
        assert!(kb.did_you_mean("zzzzzzzz", 5).is_empty());
    }

    #[test]
    fn estimate_tokens_scales_with_alphabet() {
        assert_eq!(estimate_tokens(""), 0);
        let ru = estimate_tokens(&"комиссия".repeat(50));
        let en = estimate_tokens(&"commission".repeat(50));
        // 400 кириллических символов дороже 500 латинских.
        assert!(
            ru > en,
            "кириллица должна стоить дороже за символ: {ru} vs {en}"
        );
    }

    #[test]
    fn staleness_uses_ttl_and_skips_embedded() {
        let mut doc = parse_doc("x".into(), "# X\n", &Vocabulary::default());
        doc.age_days = Some(90);
        doc.ttl_days = Some(180);
        assert_eq!(doc.staleness_pct(), Some(50));

        doc.kind = DocKind::App;
        assert_eq!(doc.staleness_pct(), None);
    }

    #[test]
    fn test_multiline_tags() {
        let fm = "title: Test\ntags:\n  - a020\n  - wildberries\n";
        let tags = parse_list(fm, "tags").unwrap();
        assert_eq!(tags, vec!["a020", "wildberries"]);
    }

    /// Каталог знаний из config.toml рабочей копии.
    ///
    /// `knowledge_base_dir()` резолвит путь относительно каталога исполняемого
    /// файла, а тестовый бинарь лежит в `target/debug/deps` — конфиг оттуда не
    /// виден. Поэтому читаем его от корня workspace.
    fn live_kb_dir() -> Option<PathBuf> {
        let config_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()?
            .parent()?
            .join("config.toml");
        let raw = std::fs::read_to_string(config_path).ok()?;
        let scalar = |key: &str| {
            raw.lines().find_map(|line| {
                line.trim()
                    .strip_prefix(key)?
                    .trim_start()
                    .strip_prefix('=')
                    .map(|v| v.trim().trim_matches('"').to_string())
            })
        };
        // Штатно каталог выводится из общего корня данных; отдельный
        // `knowledge_base_path` — отклонение и потому побеждает.
        let dir = match scalar("knowledge_base_path") {
            Some(explicit) => PathBuf::from(explicit),
            None => PathBuf::from(scalar("root")?).join(crate::shared::config::SUBDIR_KNOWLEDGE),
        };
        dir.join(VOCABULARY_FILE).exists().then_some(dir)
    }

    /// Проверка на боевом каталоге знаний: он вне репозитория, поэтому тест
    /// пропускается, если каталог недоступен (CI, чистая машина).
    #[test]
    fn live_knowledge_dir_loads_cleanly() {
        let Some(dir) = live_kb_dir() else {
            eprintln!("пропуск: каталог знаний недоступен");
            return;
        };

        let kb = KnowledgeBase::load(&dir);
        assert!(kb.vocabulary().len() > 20, "словарь тегов не разобран");

        // Введение словаря не должно оставить существующие статьи с чужими тегами.
        let unknown: Vec<(&str, &Vec<String>)> = kb
            .all_docs()
            .iter()
            .filter(|d| !d.is_embedded() && !d.unknown_tags.is_empty())
            .map(|d| (d.id.as_str(), &d.unknown_tags))
            .collect();
        assert!(
            unknown.is_empty(),
            "теги вне словаря у бизнес-статей: {unknown:?}"
        );

        // Сам файл словаря статьёй не является.
        assert!(kb.get("_vocabulary").is_none());

        // Пилотный кластер по воронке загрузился и связан в граф.
        let overview = kb.get("funnel-overview").expect("нет funnel-overview");
        assert_eq!(overview.status, KbStatus::Active);
        assert_eq!(overview.stars, Some(5));
        assert!(overview.token_cost > 0);
        assert!(
            !kb.back_links("funnel-overview").is_empty(),
            "граф не собрался"
        );

        let ozon = kb.get("funnel-ozon-open").expect("нет funnel-ozon-open");
        assert_eq!(ozon.status, KbStatus::Draft);
        assert!(
            ozon.verified.is_none(),
            "черновик не может быть верифицирован"
        );
    }

    /// Справочник по API разложен в `app/` и находится поиском.
    ///
    /// Смысл теста — не в самом факте записи файлов, а в том, что вопрос про
    /// конкретный лимит выводит на статью этого эндпоинта, а не на обзор.
    #[test]
    fn live_api_reference_is_materialized_and_searchable() {
        let Some(dir) = live_kb_dir() else {
            eprintln!("пропуск: каталог знаний недоступен");
            return;
        };
        let kb = KnowledgeBase::load(&dir);

        for id in [
            "app__wb-api-overview",
            "app__wb-api-advert-fullstats",
            "app__ym-api-overview",
            "app__ym-api-shows-sales",
        ] {
            let doc = kb.get(id).unwrap_or_else(|| panic!("нет статьи {id}"));
            assert!(doc.is_embedded(), "{id} должна быть техдоком");
            assert!(!doc.summary.is_empty(), "{id} без summary");
            assert!(
                doc.token_cost < super::super::kb_tools::LARGE_ARTICLE_TOKENS,
                "{id} переросла порог: {} токенов",
                doc.token_cost
            );
        }

        let (hits, _) = kb.search_with_total(&super::super::kb_search::Query {
            text: "почему запросы рекламной статистики wildberries режутся по месяцам".into(),
            ..Default::default()
        });
        assert_eq!(
            hits.first().map(|(d, _)| d.id.as_str()),
            Some("app__wb-api-advert-fullstats"),
            "ожидали статью про fullstats первой, получили: {:?}",
            hits.iter().map(|(d, _)| &d.id).collect::<Vec<_>>()
        );
    }

    /// Главный сценарий приёмки: свободный текст находит нужную статью первой.
    #[test]
    fn live_search_answers_the_buyout_question() {
        let Some(dir) = live_kb_dir() else {
            eprintln!("пропуск: каталог знаний недоступен");
            return;
        };
        let kb = KnowledgeBase::load(&dir);

        let (hits, total) = kb.search_with_total(&super::super::kb_search::Query {
            text: "почему выкуп в текущем месяце всегда ниже".into(),
            ..Default::default()
        });
        assert!(total > 0, "поиск по свободному тексту ничего не нашёл");
        assert_eq!(
            hits[0].0.id,
            "funnel-buyout-lag",
            "ожидали статью про лаг выкупа первой, получили: {:?}",
            hits.iter().map(|(d, _)| &d.id).collect::<Vec<_>>()
        );

        // Алиас словаря должен работать наравне с каноническим тегом.
        assert!(
            !kb.search_by_tags(&["вб"]).is_empty(),
            "алиас 'вб' не привёл к статьям Wildberries"
        );
    }

    #[test]
    fn test_build_doc_id_for_nested_path() {
        let base = Path::new("data/knowledge");
        let nested = Path::new("data/knowledge/marketplaces/wildberries/p909-p910-projections.md");
        assert_eq!(
            build_doc_id(base, nested),
            "marketplaces__wildberries__p909-p910-projections"
        );
    }
}
