//! Каталог операций загрузки: что грузим и что на самом деле означает период.
//!
//! Одна `ImportOp` — одна кнопка «Запустить» на странице импорта. Каталоги
//! конкретных маркетплейсов лежат рядом с их use-case (`u50X_*/ops.rs`) и
//! являются источником истины: строка UI, агрегат бэкенда и семантика дат
//! описаны в одном месте.

/// Что бэкенд делает с выбранным периодом.
///
/// Это главная колонка страницы: у трёх маркетплейсов период выглядит одинаково,
/// а работает по-разному. Цвет бейджа кодирует ожидания:
/// нейтральный — дат нет вовсе, синий — период работает «как ожидается»,
/// жёлтый — есть подвох (используется не весь диапазон или не та дата).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PeriodKind {
    /// Дат нет: полная выгрузка по курсору/страницам.
    None,
    /// Период = дата документа (заказа, транзакции, возврата).
    DocDate,
    /// Период = дата последнего изменения (`lastChangeDate` / `updatedAt`).
    ChangeDate,
    /// Период = интервал отчёта, который маркетплейс генерирует асинхронно.
    ReportPeriod,
    /// Используются только месяцы диапазона, дни игнорируются.
    Month,
    /// Используется только одна дата — срез на день.
    SnapshotDay,
    /// API без фильтра по датам: окно применяется на клиенте после выгрузки.
    ClientFilter,
}

impl PeriodKind {
    /// Подпись в колонке «Тип периода».
    pub fn label(self) -> &'static str {
        match self {
            Self::None => "Без периода",
            Self::DocDate => "Дата документа",
            Self::ChangeDate => "Дата изменения",
            Self::ReportPeriod => "Период отчёта",
            Self::Month => "Только месяц",
            Self::SnapshotDay => "Дата среза",
            Self::ClientFilter => "Фильтр на клиенте",
        }
    }

    /// Общее пояснение к типу (уточнения по конкретной загрузке — в `ImportOp::note`).
    pub fn hint(self) -> &'static str {
        match self {
            Self::None => "Полная выгрузка: даты на бэкенд не влияют",
            Self::DocDate => "Отбор по дате самого документа",
            Self::ChangeDate => "Отбор по дате изменения, а не по дате документа",
            Self::ReportPeriod => "Интервал отчёта, который маркетплейс готовит асинхронно",
            Self::Month => "Дни игнорируются, используются только месяцы",
            Self::SnapshotDay => "Используется одна дата, а не весь диапазон",
            Self::ClientFilter => "API отдаёт всё, период отсекается на нашей стороне",
        }
    }

    /// Нужен ли строке выбор периода.
    pub fn needs_period(self) -> bool {
        !matches!(self, Self::None)
    }

    pub fn badge_class(self) -> &'static str {
        match self {
            Self::None => "badge badge--neutral",
            Self::DocDate => "badge badge--primary",
            Self::ReportPeriod => "badge badge--accent",
            Self::ChangeDate | Self::Month | Self::SnapshotDay | Self::ClientFilter => {
                "badge badge--warning"
            }
        }
    }
}

/// Смысловая группа загрузки — чипы-фильтры в тулбаре и цветная полоса строки.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpGroup {
    Catalog,
    Orders,
    Finance,
    Documents,
    Analytics,
}

impl OpGroup {
    pub fn label(self) -> &'static str {
        match self {
            Self::Catalog => "Каталог",
            Self::Orders => "Заказы",
            Self::Finance => "Финансы",
            Self::Documents => "Документы",
            Self::Analytics => "Аналитика",
        }
    }

    pub fn badge_class(self) -> &'static str {
        match self {
            Self::Catalog => "badge badge--primary",
            Self::Orders => "badge badge--accent",
            Self::Finance => "badge badge--success",
            Self::Documents => "badge badge--neutral",
            Self::Analytics => "badge badge--warning",
        }
    }

    /// Цвет полосы слева (`--spec-cat`), как в списке DataView.
    pub fn stripe(self) -> &'static str {
        match self {
            Self::Catalog => "var(--color-primary)",
            Self::Orders => "var(--color-accent)",
            Self::Finance => "var(--color-success)",
            Self::Documents => "var(--color-border)",
            Self::Analytics => "var(--color-warning)",
        }
    }
}

/// Одна операция загрузки.
pub struct ImportOp {
    /// Стабильный ключ строки: под ним живут сессия и снапшот прогресса в localStorage.
    pub row_id: &'static str,
    /// Значение для `target_aggregates` в запросе на импорт.
    pub aggregate: &'static str,
    pub title: &'static str,
    pub group: OpGroup,
    pub period: PeriodKind,
    /// Уточнение: что именно бэкенд делает с датами именно в этой загрузке.
    /// Пусто — значит достаточно общего пояснения `PeriodKind::hint`.
    pub period_note: &'static str,
    /// Что загрузка делает. Видно в подробном режиме списка.
    pub details: &'static str,
}
