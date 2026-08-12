//! Static data for the marketplace navigator page.
//!
//! `COLUMNS` defines the marketplace columns (one per supported marketplace).
//! `BLOCKS` defines the topical sections rendered as rows in every view.
//! Logo SVGs are embedded at compile time via `include_str!`.

use crate::navigator::shared::types::{
    EntityType, LinkScope, MarketplaceColumn, MarketplaceKind, NavBlock, NavLink,
};

const WILDBERRIES_LOGO: &str = include_str!("../../../assets/images/Wildberries.svg");
const OZON_LOGO: &str = include_str!("../../../assets/images/OZON.svg");
const YANDEX_LOGO: &str = include_str!("../../../assets/images/Yandex.svg");

pub const COLUMNS: &[MarketplaceColumn] = &[
    MarketplaceColumn {
        kind: MarketplaceKind::Wildberries,
        label: "Wildberries",
        mp_key: "wb",
        logo_svg: WILDBERRIES_LOGO,
        brand_color: "#E313BF",
        logo_height_px: 34,
    },
    MarketplaceColumn {
        kind: MarketplaceKind::Ozon,
        label: "Ozon",
        mp_key: "ozon",
        logo_svg: OZON_LOGO,
        brand_color: "#005BFF",
        logo_height_px: 44,
    },
    MarketplaceColumn {
        kind: MarketplaceKind::YandexMarket,
        label: "Яндекс Маркет",
        mp_key: "ym",
        logo_svg: YANDEX_LOGO,
        brand_color: "#FFCC00",
        logo_height_px: 34,
    },
];

const WB_ONLY: &[MarketplaceKind] = &[MarketplaceKind::Wildberries];
const OZ_ONLY: &[MarketplaceKind] = &[MarketplaceKind::Ozon];
const YM_ONLY: &[MarketplaceKind] = &[MarketplaceKind::YandexMarket];

// ───────────────────────── Заказы ─────────────────────────
const ORDERS_LINKS: &[NavLink] = &[
    NavLink {
        tab_key: "a015_wb_orders",
        label: "Заказы WB",
        annotation: "Заказы покупателей по схемам FBO и FBS",
        icon: "file-text",
        scope_id: Some("a015_wb_orders"),
        marketplaces: LinkScope::Only(WB_ONLY),
        entity_type: EntityType::Aggregate,
    },
    NavLink {
        tab_key: "a029_wb_supply",
        label: "Поставки WB (FBS)",
        annotation: "Поставки FBS Wildberries: состав, стикеры, связь с заказами",
        icon: "package",
        scope_id: Some("a029_wb_supply"),
        marketplaces: LinkScope::Only(WB_ONLY),
        entity_type: EntityType::Aggregate,
    },
    NavLink {
        tab_key: "a010_ozon_fbs_posting",
        label: "Отправления FBS",
        annotation: "Отправления Ozon по схеме FBS",
        icon: "file-text",
        scope_id: Some("a010_ozon_fbs_posting"),
        marketplaces: LinkScope::Only(OZ_ONLY),
        entity_type: EntityType::Aggregate,
    },
    NavLink {
        tab_key: "a011_ozon_fbo_posting",
        label: "Отправления FBO",
        annotation: "Отправления Ozon по схеме FBO",
        icon: "file-text",
        scope_id: Some("a011_ozon_fbo_posting"),
        marketplaces: LinkScope::Only(OZ_ONLY),
        entity_type: EntityType::Aggregate,
    },
    NavLink {
        tab_key: "a013_ym_order",
        label: "Заказы Яндекс",
        annotation: "Заказы покупателей Яндекс Маркета",
        icon: "file-text",
        scope_id: Some("a013_ym_order"),
        marketplaces: LinkScope::Only(YM_ONLY),
        entity_type: EntityType::Aggregate,
    },
];

// ───────────────────── Продажи и возвраты ─────────────────
const SALES_LINKS: &[NavLink] = &[
    NavLink {
        tab_key: "a008_marketplace_sales",
        label: "Продажи МП",
        annotation: "Сводный регистр продаж по всем маркетплейсам",
        icon: "cash",
        scope_id: Some("a008_marketplace_sales"),
        marketplaces: LinkScope::All,
        entity_type: EntityType::Aggregate,
    },
    NavLink {
        tab_key: "a012_wb_sales",
        label: "Реализация WB",
        annotation: "Документы реализации Wildberries",
        icon: "file-text",
        scope_id: Some("a012_wb_sales"),
        marketplaces: LinkScope::Only(WB_ONLY),
        entity_type: EntityType::Aggregate,
    },
    NavLink {
        tab_key: "a032_wb_returns_claims",
        label: "Заявки на возврат WB",
        annotation: "Заявки покупателей на возврат товара Wildberries",
        icon: "rotate-ccw",
        scope_id: Some("a032_wb_returns_claims"),
        marketplaces: LinkScope::Only(WB_ONLY),
        entity_type: EntityType::Aggregate,
    },
    NavLink {
        tab_key: "a009_ozon_returns",
        label: "Возвраты Ozon",
        annotation: "Возвраты товаров покупателями на Ozon",
        icon: "package-x",
        scope_id: Some("a009_ozon_returns"),
        marketplaces: LinkScope::Only(OZ_ONLY),
        entity_type: EntityType::Aggregate,
    },
    NavLink {
        tab_key: "a016_ym_returns",
        label: "Возвраты Яндекс",
        annotation: "Возвраты товаров покупателями на Яндекс Маркете",
        icon: "package-x",
        scope_id: Some("a016_ym_returns"),
        marketplaces: LinkScope::Only(YM_ONLY),
        entity_type: EntityType::Aggregate,
    },
];

// ───────────────────────── Финансы ────────────────────────
const FINANCE_LINKS: &[NavLink] = &[
    NavLink {
        tab_key: "a014_ozon_transactions",
        label: "Транзакции Ozon",
        annotation: "Финансовые транзакции по операциям Ozon",
        icon: "credit-card",
        scope_id: Some("a014_ozon_transactions"),
        marketplaces: LinkScope::Only(OZ_ONLY),
        entity_type: EntityType::Aggregate,
    },
    NavLink {
        tab_key: "p902_ozon_finance_realization",
        label: "Отчёт о реализации Ozon",
        annotation: "Реестр отчётов о реализации товаров Ozon",
        icon: "dollar-sign",
        scope_id: None,
        marketplaces: LinkScope::Only(OZ_ONLY),
        entity_type: EntityType::Projection,
    },
    NavLink {
        tab_key: "a027_wb_documents",
        label: "Финансовые документы WB",
        annotation: "Документы Wildberries: реализации, акты, штрафы",
        icon: "file-text",
        scope_id: Some("a027_wb_documents"),
        marketplaces: LinkScope::Only(WB_ONLY),
        entity_type: EntityType::Aggregate,
    },
    NavLink {
        tab_key: "a043_wb_finance_report",
        label: "Финансовые отчёты WB (новый API)",
        annotation: "Ежедневные отчёты реализации Finance API v1; без проекций",
        icon: "receipt",
        scope_id: Some("a043_wb_finance_report"),
        marketplaces: LinkScope::Only(WB_ONLY),
        entity_type: EntityType::Aggregate,
    },
    NavLink {
        tab_key: "p903_wb_finance_report",
        label: "Недельный отчёт WB",
        annotation: "Сводный финансовый отчёт Wildberries по неделям",
        icon: "dollar-sign",
        scope_id: None,
        marketplaces: LinkScope::Only(WB_ONLY),
        entity_type: EntityType::Projection,
    },
    NavLink {
        tab_key: "p907_ym_payment_report",
        label: "Отчёт по платежам Яндекс",
        annotation: "Финансовые операции и выплаты Яндекс Маркета",
        icon: "receipt",
        scope_id: None,
        marketplaces: LinkScope::Only(YM_ONLY),
        entity_type: EntityType::Projection,
    },
    NavLink {
        tab_key: "a034_ym_realization",
        label: "Отчёт о реализации Яндекс",
        annotation: "Официальная выручка YM (слой ybuh) — независимый источник для сверки",
        icon: "receipt",
        scope_id: Some("a034_ym_realization"),
        marketplaces: LinkScope::Only(YM_ONLY),
        entity_type: EntityType::Aggregate,
    },
    NavLink {
        tab_key: "ym_revenue_reconciliation",
        label: "Сверка выручки YM",
        annotation: "Сверка нетто-выручки: слой fina (p907) против слоя ybuh (реализация)",
        icon: "table",
        scope_id: Some("a034_ym_realization"),
        marketplaces: LinkScope::Only(YM_ONLY),
        entity_type: EntityType::Projection,
    },
    NavLink {
        tab_key: "a035_ym_settlement_recon",
        label: "Сверка перечислений YM",
        annotation: "Банковские ордера YM: обороты против факта (bank_sum) по каждому перечислению",
        icon: "table",
        scope_id: Some("a035_ym_settlement_recon"),
        marketplaces: LinkScope::Only(YM_ONLY),
        entity_type: EntityType::Aggregate,
    },
    NavLink {
        tab_key: "p914_mp_finance_turnovers",
        label: "Финансовые обороты (fina)",
        annotation: "Обороты слоя fina — зеркало проводок GL из финотчётов WB и YM (p914)",
        icon: "layers",
        scope_id: None,
        marketplaces: LinkScope::All,
        entity_type: EntityType::Projection,
    },
];

// ─────────────────── Реклама и продвижение ────────────────
const ADVERT_LINKS: &[NavLink] = &[
    NavLink {
        tab_key: "a020_wb_promotion",
        label: "Акции WB",
        annotation: "Календарные акции Wildberries и участие товаров",
        icon: "tag",
        scope_id: Some("a020_wb_promotion"),
        marketplaces: LinkScope::Only(WB_ONLY),
        entity_type: EntityType::Aggregate,
    },
    NavLink {
        tab_key: "a030_wb_advert_campaign",
        label: "Рекламные кампании WB",
        annotation: "Карточки рекламных кампаний Wildberries",
        icon: "megaphone",
        scope_id: Some("a030_wb_advert_campaign"),
        marketplaces: LinkScope::Only(WB_ONLY),
        entity_type: EntityType::Aggregate,
    },
    NavLink {
        tab_key: "a026_wb_advert_daily",
        label: "Статистика рекламы по дням",
        annotation: "Дневные показатели и расходы рекламных кампаний WB",
        icon: "activity",
        scope_id: Some("a026_wb_advert_daily"),
        marketplaces: LinkScope::Only(WB_ONLY),
        entity_type: EntityType::Aggregate,
    },
    NavLink {
        tab_key: "a036_wb_sales_funnel_daily",
        label: "Воронка продаж по дням",
        annotation: "Переходы, корзины, заказы и выкупы по товарам WB",
        icon: "filter",
        scope_id: Some("a036_wb_sales_funnel_daily"),
        marketplaces: LinkScope::Only(WB_ONLY),
        entity_type: EntityType::Aggregate,
    },
    NavLink {
        tab_key: "a037_wb_product_snapshot",
        label: "Данные по товарам по дням",
        annotation: "Остатки и рейтинги товаров WB с динамикой по дням",
        icon: "package",
        scope_id: Some("a037_wb_product_snapshot"),
        marketplaces: LinkScope::Only(WB_ONLY),
        entity_type: EntityType::Aggregate,
    },
    NavLink {
        tab_key: "a040_wb_search_analytics_daily",
        label: "Поисковая аналитика по дням",
        annotation: "Показы, позиции в выдаче и поисковые запросы товаров WB",
        icon: "search",
        scope_id: Some("a040_wb_search_analytics_daily"),
        marketplaces: LinkScope::Only(WB_ONLY),
        entity_type: EntityType::Aggregate,
    },
    NavLink {
        tab_key: "p911_wb_advert_by_items",
        label: "Реклама в разрезе товаров",
        annotation: "Эффективность рекламы Wildberries по позициям",
        icon: "trending-up",
        scope_id: None,
        marketplaces: LinkScope::Only(WB_ONLY),
        entity_type: EntityType::Projection,
    },
    NavLink {
        tab_key: "d404_wb_advert_report",
        label: "Отчет по рекламе WB",
        annotation: "Дерево начислений, списаний и расходов без заказа по рекламе WB",
        icon: "bar-chart-3",
        scope_id: None,
        marketplaces: LinkScope::Only(WB_ONLY),
        entity_type: EntityType::Projection,
    },
    NavLink {
        tab_key: "p913_wb_advert_order_attr",
        label: "Атрибуция рекламных расходов",
        annotation: "Резервирование и списание рекламных расходов по заказам (p913)",
        icon: "layers",
        scope_id: None,
        marketplaces: LinkScope::Only(WB_ONLY),
        entity_type: EntityType::Projection,
    },
];

// ──────────────────── Цены и каталог ──────────────────────
const PRICES_LINKS: &[NavLink] = &[
    NavLink {
        tab_key: "a007_marketplace_product",
        label: "Товары маркетплейсов",
        annotation: "Карточки товаров на маркетплейсах и связь с номенклатурой",
        icon: "package",
        scope_id: Some("a007_marketplace_product"),
        marketplaces: LinkScope::All,
        entity_type: EntityType::Aggregate,
    },
    NavLink {
        tab_key: "p908_wb_goods_prices",
        label: "Цены товаров WB",
        annotation: "История розничных и закупочных цен товаров Wildberries",
        icon: "tag",
        scope_id: None,
        marketplaces: LinkScope::Only(WB_ONLY),
        entity_type: EntityType::Projection,
    },
    NavLink {
        tab_key: "p905_commission_history",
        label: "История комиссий WB",
        annotation: "Динамика комиссий маркетплейса по категориям",
        icon: "percent",
        scope_id: None,
        marketplaces: LinkScope::Only(WB_ONLY),
        entity_type: EntityType::Projection,
    },
    NavLink {
        tab_key: "p906_nomenclature_prices",
        label: "Дилерские цены",
        annotation: "Прайс-лист дилерских цен из 1С:УТ",
        icon: "dollar-sign",
        scope_id: None,
        marketplaces: LinkScope::All,
        entity_type: EntityType::Projection,
    },
];

// ────────────────── Настройки и интеграция ────────────────
const SETTINGS_LINKS: &[NavLink] = &[
    NavLink {
        tab_key: "a005_marketplace",
        label: "Справочник маркетплейсов",
        annotation: "Реестр поддерживаемых маркетплейсов",
        icon: "store",
        scope_id: Some("a005_marketplace"),
        marketplaces: LinkScope::All,
        entity_type: EntityType::Aggregate,
    },
    NavLink {
        tab_key: "a006_connection_mp",
        label: "Подключения к МП",
        annotation: "Учётные записи и API-ключи продавцов",
        icon: "plug",
        scope_id: Some("a006_connection_mp"),
        marketplaces: LinkScope::All,
        entity_type: EntityType::Aggregate,
    },
    NavLink {
        tab_key: "u504_import_from_wildberries",
        label: "Импорт WB",
        annotation: "Регламентированный импорт данных из Wildberries",
        icon: "import",
        scope_id: None,
        marketplaces: LinkScope::Only(WB_ONLY),
        entity_type: EntityType::UseCase,
    },
    NavLink {
        tab_key: "u502_import_from_ozon",
        label: "Импорт Ozon",
        annotation: "Регламентированный импорт данных из Ozon",
        icon: "import",
        scope_id: None,
        marketplaces: LinkScope::Only(OZ_ONLY),
        entity_type: EntityType::UseCase,
    },
    NavLink {
        tab_key: "u503_import_from_yandex",
        label: "Импорт Яндекс",
        annotation: "Регламентированный импорт данных из Яндекс Маркета",
        icon: "import",
        scope_id: None,
        marketplaces: LinkScope::Only(YM_ONLY),
        entity_type: EntityType::UseCase,
    },
];

pub const BLOCKS: &[NavBlock] = &[
    NavBlock {
        id: "orders",
        label: "Заказы",
        icon: "shopping-cart",
        links: ORDERS_LINKS,
    },
    NavBlock {
        id: "sales",
        label: "Продажи и возвраты",
        icon: "cash",
        links: SALES_LINKS,
    },
    NavBlock {
        id: "finance",
        label: "Финансы",
        icon: "dollar-sign",
        links: FINANCE_LINKS,
    },
    NavBlock {
        id: "advert",
        label: "Реклама и продвижение",
        icon: "megaphone",
        links: ADVERT_LINKS,
    },
    NavBlock {
        id: "prices",
        label: "Цены и каталог",
        icon: "tag",
        links: PRICES_LINKS,
    },
    NavBlock {
        id: "settings",
        label: "Настройки и интеграция",
        icon: "settings",
        links: SETTINGS_LINKS,
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wb_finance_report_is_listed_in_finance_block_only() {
        let containing_blocks: Vec<&str> = BLOCKS
            .iter()
            .filter(|block| {
                block
                    .links
                    .iter()
                    .any(|link| link.tab_key == "a043_wb_finance_report")
            })
            .map(|block| block.id)
            .collect();

        assert_eq!(containing_blocks, vec!["finance"]);
    }
}
