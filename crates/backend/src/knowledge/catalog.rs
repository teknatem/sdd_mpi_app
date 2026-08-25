//! Реестр поверхностей знания — ядро метода (§2 нормативного документа).
//!
//! Список того, что вообще бывает, объявляется **явно**. Обход диска не заметит
//! того, чего на диске нет, а на диске нет большей части системы: реестры живут
//! Rust-константами, плагины и Процессы — строками в БД, планы счетов и разделы
//! UI — массивами в коде. Инвентаризация поэтому идёт от этого списка, а не от
//! каталога файлов; обратный порядок даёт правдоподобное число, молча
//! пропускающее четыре пятых системы.
//!
//! **Правило.** Новая поверхность заводится строкой здесь. Не заведена — не
//! видна, и невидимость эту ничто не обнаружит: она не сломает ни сборку, ни
//! тест. Единственное, что сломается, — новый вариант `Enumerator` без ветки в
//! `collector.rs`, и это сделано нарочно.
//!
//! Живёт в коде, а не в БД, по той же причине, что `SCOPE_CATALOG`, план счетов
//! и `METRIC_CATALOG`: состав поверхностей — часть версии приложения. Экземпляр
//! не должен уметь завести свою, иначе две базы перестанут быть сравнимыми.

use contracts::knowledge::{
    CodeRole, Editor, ExposureChannel, Origin, Reachability, Scope, StorageForm, UnitFamily,
};

/// Чем поверхность перечисляется.
///
/// Тег, а не замыкание: `collector.rs` разбирает его `match`-ем, и новый вариант
/// не соберётся без ветки. Это и есть исполнение правила §2 — забыть
/// перечислитель нельзя, о нём напомнит компилятор.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Enumerator {
    /// Статьи корпуса `business` — курируемые файлы в каталоге знаний.
    ArticlesBusiness,
    /// Статьи корпуса `app` — техдоки, вшитые в бинарь.
    ArticlesApp,
    /// Карты корпуса `generated` — собраны из БД и рантайма.
    ArticlesGenerated,
    /// Словарь тегов `_vocabulary.md`.
    Vocabulary,
    /// Навыки, их ресурсы и JS-задачи.
    Skills,
    /// Базовый системный промпт.
    CorePrompt,
    /// Определения quality-проверок.
    QualityChecks,
    /// Бандлы плагинов из таблицы `plugin`.
    Plugins,
    /// Определения Процессов и Этапов.
    Processes,
    /// Каталог Действий.
    Actions,
    /// Реестр сущностей и их метаданные.
    Entities,
    /// Источники данных `dsXX` и `dvXX`.
    DataSources,
    /// План счетов и виды оборотов.
    ChartOfAccounts,
    /// Разделы UI из каталога областей доступа.
    UiScopes,
    /// Типы регламентных заданий.
    ScheduledTasks,
    /// Каталог инструментов чата.
    ToolCatalog,
    /// Встроенная справка инструментов: примеры, шаблоны, UI-контракты.
    ToolHelp,
    /// Маршруты внешнего API.
    ExternalRoutes,
    /// Данные экземпляра: тикеты, письма, таблицы. Считается **типами**, не строками.
    InstanceData,
}

/// Строка реестра.
pub struct SurfaceDef {
    pub surface_id: &'static str,
    pub label: &'static str,
    pub family: UnitFamily,
    pub origin: Origin,
    pub storage_form: StorageForm,
    pub editor: Editor,
    pub scope: Scope,
    pub channel: ExposureChannel,
    /// `None` — исходный код к поверхности отношения не имеет.
    pub code_role: Option<CodeRole>,
    /// Достижимость, как её понимает автор реестра.
    ///
    /// Не используется в расчётах: фактическая считается из `tool_map`. Нужна
    /// ровно для одного — поймать расхождение между тем, что мы про систему
    /// думаем, и тем, что в ней есть.
    pub reachability_declared: Reachability,
    /// Человекочитаемое «чем перечислить» — имя функции или константы.
    pub enumerated_by: &'static str,
    pub enumerator: Enumerator,
    pub note: &'static str,
}

use CodeRole::{Authoritative, Extracted, Undocumented};
use Editor::{Application, Curator, Developer};
use ExposureChannel::{Both, InternalRuntime};
use Origin::{CodeEmbedded, CodeRegistry, DbGenerated, DbLive, FileCurated};
use Reachability::{AlwaysInContext, ByIdOnly, DefaultSearch, SkillGated, ToolGated};
use StorageForm::{DbRow, JsModule, Markdown, RustConst};
use UnitFamily::{Computed, Stored};

/// Все поверхности знания системы.
///
/// Порядок значим — в нём они показываются на вкладке «Поверхности»: сперва то,
/// что человек пишет и читает, потом машинные корпуса, потом реестры кода,
/// последними — данные экземпляра.
pub static SURFACE_CATALOG: &[SurfaceDef] = &[
    SurfaceDef {
        surface_id: "articles_business",
        label: "Статьи о предметной области",
        family: Stored,
        origin: FileCurated,
        storage_form: Markdown,
        editor: Curator,
        scope: Scope::Instance,
        channel: InternalRuntime,
        code_role: None,
        reachability_declared: DefaultSearch,
        enumerated_by: "knowledge_base::kb_read() → DocKind::Business",
        enumerator: Enumerator::ArticlesBusiness,
        note: "Единственный корпус, который пишет человек про свой бизнес. Живёт \
               вне репозитория, правится в Obsidian.",
    },
    SurfaceDef {
        surface_id: "articles_app",
        label: "Техдоки приложения",
        family: Stored,
        origin: CodeEmbedded,
        storage_form: Markdown,
        editor: Developer,
        scope: Scope::Application,
        channel: InternalRuntime,
        code_role: Some(Extracted),
        reachability_declared: DefaultSearch,
        enumerated_by: "knowledge_base::EMBEDDED_LLM_DOCS",
        enumerator: Enumerator::ArticlesApp,
        note: "Файлы llm.md рядом с кодом, вшитые через include_str!. Отвечают \
               «почему так», тогда как ARCHITECTURE.md отвечает «что есть».",
    },
    SurfaceDef {
        surface_id: "articles_generated",
        label: "Карты, собранные из БД",
        family: Stored,
        origin: DbGenerated,
        storage_form: Markdown,
        editor: Application,
        scope: Scope::Instance,
        channel: InternalRuntime,
        code_role: Some(Extracted),
        reachability_declared: ByIdOnly,
        enumerated_by: "kb_generated::regenerate_all",
        enumerator: Enumerator::ArticlesGenerated,
        note: "Выведены из выдачи обычного поиска намеренно: они машинные, их \
               десятки, и в поиске они утопили бы курируемые статьи.",
    },
    SurfaceDef {
        surface_id: "vocabulary",
        label: "Словарь тегов",
        family: Stored,
        origin: FileCurated,
        storage_form: Markdown,
        editor: Curator,
        scope: Scope::Instance,
        channel: InternalRuntime,
        code_role: None,
        reachability_declared: SkillGated,
        enumerated_by: "knowledge_base::kb_read().vocabulary()",
        enumerator: Enumerator::Vocabulary,
        note: "Отдаётся своим инструментом: 90 записей словаря — не разделы \
               документа, и стандарт SEC-1 к нему не применяется.",
    },
    SurfaceDef {
        surface_id: "skills",
        label: "Навыки и их ресурсы",
        family: Stored,
        origin: FileCurated,
        storage_form: Markdown,
        editor: Developer,
        scope: Scope::Mixed,
        channel: InternalRuntime,
        code_role: Some(Extracted),
        reachability_declared: ToolGated,
        enumerated_by: "skills::snapshot()",
        enumerator: Enumerator::Skills,
        note: "Якоря разделов навыкам не заводятся: навык доставляется целиком по \
               контракту, и прочитать половину собственной инструкции модель не должна.",
    },
    SurfaceDef {
        surface_id: "core_prompt",
        label: "Промпт ядра",
        family: Stored,
        origin: CodeEmbedded,
        storage_form: Markdown,
        editor: Developer,
        scope: Scope::Application,
        channel: InternalRuntime,
        code_role: Some(Extracted),
        reachability_declared: AlwaysInContext,
        enumerated_by: "skills::core_prompt()",
        enumerator: Enumerator::CorePrompt,
        note: "Единственная поверхность, до которой не нужно дотягиваться: она в \
               контексте всегда и целиком.",
    },
    SurfaceDef {
        surface_id: "quality_checks",
        label: "Проверки качества данных",
        family: Stored,
        origin: FileCurated,
        storage_form: JsModule,
        editor: Developer,
        scope: Scope::Mixed,
        channel: Both,
        code_role: Some(Authoritative),
        reachability_declared: SkillGated,
        enumerated_by: "quality::registry::snapshot()",
        enumerator: Enumerator::QualityChecks,
        note: "Пять проверок на Rust и шесть пакетов на JS в каталоге данных — \
               состав зависит от экземпляра.",
    },
    SurfaceDef {
        surface_id: "plugins",
        label: "Плагины",
        family: Stored,
        origin: DbLive,
        storage_form: DbRow,
        editor: Developer,
        scope: Scope::Instance,
        channel: Both,
        code_role: Some(Authoritative),
        reachability_declared: SkillGated,
        enumerated_by: "plugins::repository::list_all",
        enumerator: Enumerator::Plugins,
        note: "Строки таблицы plugin, а не файлы репозитория: grep их не найдёт. \
               Идентичность — manifest.code, UUID локален.",
    },
    SurfaceDef {
        surface_id: "processes",
        label: "Процессы и Этапы",
        family: Stored,
        origin: DbLive,
        storage_form: DbRow,
        editor: Developer,
        scope: Scope::Instance,
        channel: Both,
        code_role: Some(Authoritative),
        reachability_declared: Reachability::Unreachable,
        enumerated_by: "processes::repository::list_*_head_records",
        enumerator: Enumerator::Processes,
        note: "Определения живут в БД, как и плагины. Инструмента чата под них нет: \
               ни одного вызова в каталоге не размечено на эту поверхность — чат про \
               заведённые Процессы узнаёт только из карты processes.",
    },
    SurfaceDef {
        surface_id: "actions",
        label: "Действия",
        family: Computed,
        origin: CodeRegistry,
        storage_form: RustConst,
        editor: Developer,
        scope: Scope::Application,
        channel: Both,
        code_role: Some(Authoritative),
        reachability_declared: Reachability::Unreachable,
        enumerated_by: "processes::actions::list()",
        enumerator: Enumerator::Actions,
        note: "Каталог операций с побочным эффектом. Ни одной статьи про них нет, и \
               инструмента, который бы их перечислил, тоже: Действия видит Этап и \
               видит внешний API, а чат — нет.",
    },
    SurfaceDef {
        surface_id: "entities",
        label: "Сущности реестра метаданных",
        family: Computed,
        origin: CodeRegistry,
        storage_form: RustConst,
        editor: Developer,
        scope: Scope::Application,
        channel: Both,
        code_role: Some(Extracted),
        reachability_declared: ToolGated,
        enumerated_by: "contracts::shared::metadata::ALL_ENTITIES",
        enumerator: Enumerator::Entities,
        note: "Самая крупная вычисляемая поверхность и при этом самая бедная \
               статьями — про неё в корпусе не написано почти ничего.",
    },
    SurfaceDef {
        surface_id: "data_sources",
        label: "Источники данных ds/dv",
        family: Computed,
        origin: CodeRegistry,
        storage_form: RustConst,
        editor: Developer,
        scope: Scope::Application,
        channel: Both,
        code_role: Some(Extracted),
        reachability_declared: ToolGated,
        enumerated_by: "shared::data_access::list_sources",
        enumerator: Enumerator::DataSources,
        note: "Две роли в одном перечне: dsXX — гибкий ad-hoc, dvXX — курируемые \
               метрики с кэшем. Перекрытие по таблице допустимо только при разных ролях.",
    },
    SurfaceDef {
        surface_id: "chart_of_accounts",
        label: "План счетов и виды оборотов",
        family: Computed,
        origin: CodeRegistry,
        storage_form: RustConst,
        editor: Developer,
        scope: Scope::Application,
        channel: Both,
        code_role: Some(Authoritative),
        reachability_declared: SkillGated,
        enumerated_by: "ACCOUNT_REGISTRY, TURNOVER_CLASSES",
        enumerator: Enumerator::ChartOfAccounts,
        note: "Скелет финансовой модели. Слои учёта поверх него отдельной строкой \
               не заводятся — слой это свойство оборота, а не поверхность.",
    },
    SurfaceDef {
        surface_id: "ui_scopes",
        label: "Разделы интерфейса",
        family: Computed,
        origin: CodeRegistry,
        storage_form: RustConst,
        editor: Developer,
        scope: Scope::Application,
        channel: Both,
        code_role: Some(Extracted),
        reachability_declared: ToolGated,
        enumerated_by: "system::access::scope_catalog::SCOPE_CATALOG",
        enumerator: Enumerator::UiScopes,
        note: "Он же реестр прав доступа: раздел UI и область доступа — один объект.",
    },
    SurfaceDef {
        surface_id: "scheduled_tasks",
        label: "Регламентные задания",
        family: Computed,
        origin: CodeRegistry,
        storage_form: RustConst,
        editor: Developer,
        scope: Scope::Mixed,
        channel: Both,
        code_role: Some(Extracted),
        reachability_declared: SkillGated,
        enumerated_by: "system::tasks::registry",
        enumerator: Enumerator::ScheduledTasks,
        note: "Считаются типы заданий, а не запуски: запусков десятки тысяч, и они \
               данные, а не знание.",
    },
    SurfaceDef {
        surface_id: "tool_catalog",
        label: "Каталог инструментов чата",
        family: Computed,
        origin: CodeRegistry,
        storage_form: RustConst,
        editor: Developer,
        scope: Scope::Application,
        channel: InternalRuntime,
        code_role: Some(Extracted),
        reachability_declared: Reachability::Unreachable,
        enumerated_by: "skills::tool_universe()",
        enumerator: Enumerator::ToolCatalog,
        note: "Поверхность, которую не отдаёт ни один инструмент — включая её \
               собственный. Каталог доступен только через GET /api/llm-tools, то есть \
               человеку и внешнему клиенту, но не модели.",
    },
    SurfaceDef {
        surface_id: "tool_help",
        label: "Встроенная справка инструментов",
        // Вычисляемая, хотя тексты вшиты в бинарь: перечислить их можно только
        // вызвав каждый инструмент, и размер ответа мы не замеряли. Записать её
        // хранимой значило бы пообещать байты и токены, которых у нас нет.
        family: Computed,
        origin: CodeEmbedded,
        storage_form: Markdown,
        editor: Developer,
        scope: Scope::Application,
        channel: InternalRuntime,
        code_role: Some(Extracted),
        reachability_declared: SkillGated,
        enumerated_by: "tool_map::TOOL_YIELD → ToolHelp",
        enumerator: Enumerator::ToolHelp,
        note: "Строки реестра §2 у неё не было. Обнаружилась при разметке \
               инструментов: примеры, шаблоны и UI-контракты — знание, которое \
               доставляется модели и стоит токенов, но нигде не считалось.",
    },
    SurfaceDef {
        surface_id: "external_routes",
        label: "Маршруты внешнего API",
        family: Computed,
        origin: CodeRegistry,
        storage_form: RustConst,
        editor: Developer,
        scope: Scope::Application,
        channel: ExposureChannel::ExternalApi,
        code_role: Some(Authoritative),
        reachability_declared: Reachability::Unreachable,
        enumerated_by: "handlers/ext_openapi.json",
        enumerator: Enumerator::ExternalRoutes,
        note: "Единственная поверхность, раскрытая наружу и закрытая внутрь: 1С и \
               Power BI её видят, чат — нет.",
    },
    SurfaceDef {
        surface_id: "instance_data",
        label: "Данные экземпляра",
        family: Computed,
        origin: DbLive,
        storage_form: DbRow,
        editor: Application,
        scope: Scope::Instance,
        channel: Both,
        code_role: Some(Undocumented),
        reachability_declared: SkillGated,
        enumerated_by: "SQL по таблицам каталога",
        enumerator: Enumerator::InstanceData,
        note: "Тикеты, письма, проводки, заказы. Единица — таблица, а не строка: \
               иначе счёт единиц взорвётся и утопит всё остальное.",
    },
];

/// Найти поверхность по идентификатору.
pub fn find(surface_id: &str) -> Option<&'static SurfaceDef> {
    SURFACE_CATALOG
        .iter()
        .find(|surface| surface.surface_id == surface_id)
}
