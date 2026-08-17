//! Каталог загрузок Yandex Market.
//!
//! Семантика периода — по `backend/src/usecases/u503_import_from_yandex/executor.rs`
//! и `backend/src/shared/marketplaces/yandex/yandex_api_client.rs`.

use crate::usecases::shared::{ImportOp, OpGroup, PeriodKind};

pub const OPS: &[ImportOp] = &[
    ImportOp {
        row_id: "a007",
        aggregate: "a007_marketplace_product",
        title: "Товары маркетплейса",
        group: OpGroup::Catalog,
        period: PeriodKind::None,
        period_note: "Выгрузка offer-mappings страницами по page_token",
        details: "Загружает карточки товаров кабинета Yandex Market: идентификаторы, артикулы и связки, нужные для сопоставления ассортимента.",
    },
    ImportOp {
        row_id: "a013",
        aggregate: "a013_ym_order",
        title: "Заказы Yandex Market",
        group: OpGroup::Orders,
        period: PeriodKind::DocDate,
        period_note: "Период = дата оформления заказа; окно режется на отрезки по 30 дней",
        details: "Загружает заказы по всем магазинам бизнеса. API умеет отбирать и по дате изменения (updatedAt), но ручной импорт всегда работает как полный бэкфилл по дате оформления — инкрементальный режим оставлен планировщику (task013).",
    },
    ImportOp {
        row_id: "a016",
        aggregate: "a016_ym_returns",
        title: "Возвраты и невыкупы",
        group: OpGroup::Orders,
        period: PeriodKind::DocDate,
        period_note: "Период = дата возврата (fromDate/toDate)",
        details: "Загружает возвраты и невыкупы Yandex Market за период, включая причины и состав возврата.",
    },
    ImportOp {
        row_id: "p907",
        aggregate: "p907_ym_payment_report",
        title: "Отчёт по платежам",
        group: OpGroup::Finance,
        period: PeriodKind::ReportPeriod,
        period_note: "Асинхронный отчёт: маркетплейс готовит его за указанный интервал",
        details: "Загружает отчёт по платежам Yandex Market — основу сверки выручки и удержаний по кабинету.",
    },
    ImportOp {
        row_id: "a034",
        aggregate: "a034_ym_realization",
        title: "Отчёт о реализации (слой ybuh)",
        group: OpGroup::Finance,
        period: PeriodKind::Month,
        period_note: "Отчёт строится помесячно: берутся все месяцы, попавшие в диапазон",
        details: "Загружает официальный отчёт о реализации (слой ybuh) для сверки выручки с оперативным слоем fina.",
    },
    ImportOp {
        row_id: "a041",
        aggregate: "a041_ym_shows_sales_daily",
        title: "Воронка продаж (Аналитика продаж)",
        group: OpGroup::Analytics,
        period: PeriodKind::ReportPeriod,
        period_note: "Асинхронный отчёт «Аналитика продаж» за период, разрез по дням",
        details: "Показы, клики, добавления в корзину и заказы по товарам — дневная воронка Yandex Market.",
    },
];
