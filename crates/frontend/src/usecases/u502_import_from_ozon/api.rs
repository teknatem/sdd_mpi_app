//! Каталог загрузок OZON.
//!
//! Семантика периода — по `backend/src/usecases/u502_import_from_ozon/executor.rs`
//! и `backend/src/shared/marketplaces/ozon/ozon_api_client.rs`.

use crate::usecases::common::{ImportOp, OpGroup, PeriodKind};

pub const OPS: &[ImportOp] = &[
    ImportOp {
        row_id: "a007",
        aggregate: "a007_marketplace_product",
        title: "Товары маркетплейса",
        group: OpGroup::Catalog,
        period: PeriodKind::None,
        period_note: "Выгрузка /v3/product/list страницами по last_id",
        details: "Загружает карточки товаров кабинета OZON: идентификаторы, артикулы и баркоды для сопоставления ассортимента.",
    },
    ImportOp {
        row_id: "a008",
        aggregate: "a008_marketplace_sales",
        title: "Продажи (фин. транзакции)",
        group: OpGroup::Finance,
        period: PeriodKind::DocDate,
        period_note: "Период = дата транзакции (filter.date)",
        details: "Загружает финансовые транзакции OZON — основу расчёта выручки и удержаний.",
    },
    ImportOp {
        row_id: "a009",
        aggregate: "a009_ozon_returns",
        title: "Возвраты OZON",
        group: OpGroup::Orders,
        period: PeriodKind::DocDate,
        period_note: "Период = дата логистического возврата (logistic_return_date)",
        details: "Загружает возвраты OZON с причинами и текущим статусом обработки.",
    },
    ImportOp {
        row_id: "a010",
        aggregate: "a010_ozon_fbs_posting",
        title: "FBS: документы продаж",
        group: OpGroup::Orders,
        period: PeriodKind::DocDate,
        period_note: "Период = дата отправления (filter.since/to)",
        details: "Загружает отправления FBS и формирует документы продаж, питающие проекцию p900.",
    },
    ImportOp {
        row_id: "a011",
        aggregate: "a011_ozon_fbo_posting",
        title: "FBO: документы продаж",
        group: OpGroup::Orders,
        period: PeriodKind::DocDate,
        period_note: "Период = дата отправления (filter.since/to)",
        details: "Загружает отправления FBO и формирует документы продаж, питающие проекцию p900.",
    },
    ImportOp {
        row_id: "a014",
        aggregate: "a014_ozon_transactions",
        title: "Транзакции OZON",
        group: OpGroup::Finance,
        period: PeriodKind::DocDate,
        period_note: "Период = дата транзакции, тип операций — все",
        details: "Загружает полный список транзакций кабинета OZON постранично.",
    },
    ImportOp {
        row_id: "p902",
        aggregate: "p902_ozon_finance_realization",
        title: "Финансы: отчёт о реализации",
        group: OpGroup::Finance,
        period: PeriodKind::Month,
        period_note: "Берётся месяц из даты «с», дата «по» игнорируется полностью",
        details: "Загружает отчёт о реализации OZON за месяц — API работает только помесячно.",
    },
];
