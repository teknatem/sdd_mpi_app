//! Tab labels - единый источник правды для заголовков табов.
//!
//! Для агрегатов с metadata_gen.rs используются константы из contracts.
//! Для остальных (проекции, юзкейсы, системные) - хардкод.

use contracts::domain::a001_connection_1c::ENTITY_METADATA as A001;
use contracts::domain::a002_organization::ENTITY_METADATA as A002;
use contracts::domain::a004_nomenclature::ENTITY_METADATA as A004;
use contracts::domain::a005_marketplace::ENTITY_METADATA as A005;
use contracts::domain::a006_connection_mp::ENTITY_METADATA as A006;
use contracts::domain::a012_wb_sales::ENTITY_METADATA as A012;
use contracts::domain::a013_ym_order::ENTITY_METADATA as A013;
use contracts::domain::a017_llm_agent::ENTITY_METADATA as A017;
use contracts::domain::a018_llm_chat::ENTITY_METADATA as A018;
use contracts::domain::a019_llm_artifact::ENTITY_METADATA as A019;
use contracts::domain::a020_wb_promotion::ENTITY_METADATA as A020;
use contracts::domain::a024_bi_indicator::ENTITY_METADATA as A024;
use contracts::domain::a025_bi_dashboard::ENTITY_METADATA as A025;
use contracts::domain::a027_wb_documents::ENTITY_METADATA as A027;
use contracts::domain::a038_llm_connection::ENTITY_METADATA as A038;
use contracts::domain::a039_mail_message::ENTITY_METADATA as A039;
use contracts::domain::a042_agent_task::ENTITY_METADATA as A042;

/// Возвращает читаемый заголовок таба для данного ключа.
///
/// Для агрегатов с metadata_gen берет `list_name` из contracts.
/// Для остальных - хардкод. Fallback: сам ключ.
pub fn tab_label_for_key(key: &str) -> &'static str {
    match key {
        "sys_s3_files" => "S3 файлы",
        "a001_connection_1c" => A001.ui.list_name,
        "a002_organization" => A002.ui.list_name,
        "a004_nomenclature" => A004.ui.list_name,
        "a004_nomenclature_list" => "Номенклатура (список)",
        "a005_marketplace" => A005.ui.list_name,
        "a006_connection_mp" => A006.ui.list_name,
        "a012_wb_sales" => A012.ui.list_name,
        "a013_ym_order" => A013.ui.list_name,
        "a017_llm_agent" => A017.ui.list_name,
        "a038_llm_connection" => A038.ui.list_name,
        "a039_mail_message" => A039.ui.list_name,
        "a042_agent_task" => A042.ui.list_name,
        k if k.starts_with("a042_agent_task_details_") => A042.ui.element_name,
        "a018_llm_chat" => A018.ui.list_name,
        "llm_skills" => "Навыки LLM",
        "llm_tools" => "Инструменты LLM",
        "a019_llm_artifact" => A019.ui.list_name,
        "a020_wb_promotion" => A020.ui.list_name,
        "a024_bi_indicator" => A024.ui.list_name,
        "a025_bi_dashboard" => A025.ui.list_name,
        "bi_timeline" => "BI Timeline",
        "a027_wb_documents" => A027.ui.list_name,

        "a003_counterparty" => "Контрагенты",
        "a007_marketplace_product" => "Товары маркетплейсов",
        "a008_marketplace_sales" => "Продажи МП",
        "a009_ozon_returns" => "Возвраты OZON",
        "a010_ozon_fbs_posting" => "OZON FBS Posting",
        "a011_ozon_fbo_posting" => "OZON FBO Posting",
        "a014_ozon_transactions" => "Транзакции OZON",
        "a015_wb_orders" => "WB Orders",
        "a026_wb_advert_daily" => "Статистика рекламы WB",
        "report_a026_wb_advert_daily" => "Реклама WB — выгрузка CSV",
        "a036_wb_sales_funnel_daily" => "Воронка продаж WB",
        "a037_wb_product_snapshot" => "Данные по товарам WB",
        "a040_wb_search_analytics_daily" => "Поисковая аналитика WB",
        "a041_ym_shows_sales_daily" => "Воронка продаж Yandex Market",
        "a043_wb_finance_report" => "Финансовые отчёты WB (новый API)",
        "a016_ym_returns" => "Возвраты Yandex",

        "u501_import_from_ut" => "Импорт из УТ 11",
        "u502_import_from_ozon" => "Импорт из OZON",
        "u503_import_from_yandex" => "Импорт из Yandex",
        "u504_import_from_wildberries" => "Импорт из Wildberries",
        "u505_match_nomenclature" => "Сопоставление",
        "u506_import_from_lemanapro" => "Импорт из ЛеманаПро",
        "u507_import_from_erp" => "Импорт из ERP",
        "u508_repost_documents" => "Перепроведение документов",
        "general_ledger" => "Главная книга",
        "general_ledger_turnovers" => "Обороты GL",
        "general_ledger_dimensions" => "Измерения GL",
        "general_ledger_layers" => "Слои GL",
        "general_ledger_entities" => "Субъекты GL",
        "supplier_balance" => "Баланс к перечислению (YM)",
        "general_ledger_matrix" => "Матрица Слой/Оборот",
        "general_ledger_report" => "Отчёт GL",
        "gl_account_view__7609" => "Ведомость по кабинетам",
        "wb_weekly_reconciliation" => "Сверка weekly WB и GL 7609",
        "ym_revenue_reconciliation" => "Сверка выручки YM (fina vs ybuh)",
        "a034_ym_realization" => "Реализация YM",
        "a035_ym_settlement_recon" => "Сверка перечислений YM",
        k if k.starts_with("gl_drilldown__") => "Детализация GL",
        k if k.starts_with("bi_timeline__") => "BI Timeline",
        k if k.starts_with("general_ledger_details_") => "Главная книга",
        k if k.starts_with("general_ledger_turnover_details_") => "Оборот GL",
        k if k.starts_with("general_ledger_dimensions__") => "Измерения GL",
        "a021_production_output" => "Выпуск продукции",
        "a022_kit_variant" => "Варианты комплектации",
        "a023_purchase_of_goods" => "Приобретение товаров",
        "a028_missing_cost_registry" => "Реестр отсутствующих цен",
        "a029_wb_supply" => "Поставки WB (FBS)",
        k if k.starts_with("a029_wb_supply_details_") => "Поставка WB",
        "a030_wb_advert_campaign" => "Рекламные кампании WB",
        k if k.starts_with("a030_wb_advert_campaign_details_") => "Рекламная кампания WB",
        "a031_kb_edit" => "Редактирование базы знаний",
        k if k.starts_with("a031_kb_edit_details_") => "Редактирование KB",
        "a032_wb_returns_claims" => "Заявки на возврат WB",
        k if k.starts_with("a032_wb_returns_claims_details_") => "Заявка на возврат WB",
        "a033_wb_day_close" => "Закрытие дня WB",
        k if k.starts_with("a033_wb_day_close_details_") => "Закрытие дня WB",
        "knowledge_base" => "Инвентаризация знаний",
        k if k.starts_with("kb_article_") => "Статья KB",
        // Без этих правил вкладка чата, восстановленная из `?active=`, получает
        // заголовком свой сырой ключ (`a018_llm_chat_details_<uuid>`).
        k if k.starts_with("a018_llm_chat_details_") => "AI чат",
        k if k.starts_with("a018_llm_context_details_") => "Контекст чата",

        "p900_sales_register" => "Регистр продаж",
        "p901_barcodes" => "Штрихкоды номенклатуры",
        "p902_ozon_finance_realization" => "OZON Finance Realization",
        "p903_wb_finance_report" => "WB Finance Report",
        "p904_sales_data" => "Sales Data",
        "p909_mp_order_line_turnovers" => "MP Order Line Turnovers",
        "p910_mp_unlinked_turnovers" => "MP Unlinked Turnovers",
        "p911_wb_advert_by_items" => "WB Advert By Items",
        "p913_wb_advert_order_attr" => "Атрибуция расходов WB",
        "p914_mp_finance_turnovers" => "Финансовые обороты (fina)",
        "p905_commission_history" => "WB Commission History",
        "p906_nomenclature_prices" => "Дилерские цены (УТ)",
        "p907_ym_payment_report" => "YM Отчёт по платежам",
        "p908_wb_goods_prices" => "WB Цены товаров",

        "d400_monthly_summary" => "Сводка за месяц",
        "d405_metadata_dashboard" => "Метаданные",
        "d406_wb_sales_funnel" => "Воронка продаж",
        "d407_llm_quality" => "Качество агентов",
        "d401_wb_finance" => "WB Finance",
        "d402_wb_order_flow" => "WB История заказов",
        k if k.starts_with("d402_wb_order_flow_srid_") => "Вся история",
        "d403_ym_order_flow" => "YM История заказов",
        k if k.starts_with("d403_ym_order_flow_order_") => "Вся история",

        "sys_users" => "Пользователи",
        k if k.starts_with("sys_user_details_") => "Пользователь",
        "sys_tickets" => "Тикеты",
        "sys_ticket_new" => "Новый тикет",
        k if k.starts_with("sys_ticket_details_") => "Тикет",
        "sys_roles" => "Роли",
        "sys_roles_matrix" => "Матрица ролей",
        "sys_audit" => "Аудит доступа",
        "sys_metrics" => "Метрики проекта",
        "sys_processes" => "Процессы",
        k if k.starts_with("sys_stage_details_") => "Этап",
        "sys_datasets" => "Наборы данных и перенос",
        "sys_raw_storage" => "Настройка raw JSON",
        "sys_tasks" => "Регламентные задания",
        "sys_task_details" => "Новая задача",
        k if k.starts_with("sys_task_details_") => "Задача",
        "sys_task_type_registry" => "Реестр типов заданий",
        "sys_thaw_test" => "Тест Thaw UI",
        "sys_style_guide" => "Гид по стилям",
        "dom_inspector" => "DOM Inspector",

        "universal_dashboard" => "Конструктор запросов",
        "all_reports" => "Все отчеты",
        "schema_browser" => "Схемы таблиц",

        "data_view" => "DataView",
        "filter_registry" => "Реестр фильтров",
        "drilldown__new" => "Детализация",

        "navigator_marketplace" => "Все по маркетплейсам",

        "quality_checks" => "Контроль качества данных",

        _ => "",
    }
}

/// Возвращает первый непустой идентификатор из цепочки fallback.
///
/// Порядок приоритета: document_no -> article -> description -> id
pub fn pick_identifier<'a>(
    document_no: Option<&'a str>,
    article: Option<&'a str>,
    description: Option<&'a str>,
    id: &'a str,
) -> &'a str {
    [document_no, article, description]
        .into_iter()
        .flatten()
        .find(|s| !s.is_empty())
        .unwrap_or(id)
}

/// Формирует заголовок detail-таба: "<entity> · <identifier>".
pub fn detail_tab_label(entity_label: &'static str, identifier: &str) -> String {
    format!("{} · {}", entity_label, identifier)
}

/// Возвращает element_name для агрегата по ключу.
pub fn entity_element_name(aggregate_key: &str) -> &'static str {
    match aggregate_key {
        "a001_connection_1c" => A001.ui.element_name,
        "a002_organization" => A002.ui.element_name,
        "a004_nomenclature" => A004.ui.element_name,
        "a005_marketplace" => A005.ui.element_name,
        "a006_connection_mp" => A006.ui.element_name,
        "a012_wb_sales" => A012.ui.element_name,
        "a013_ym_order" => A013.ui.element_name,
        "a017_llm_agent" => A017.ui.element_name,
        "a038_llm_connection" => A038.ui.element_name,
        "a018_llm_chat" => A018.ui.element_name,
        "a019_llm_artifact" => A019.ui.element_name,
        "a020_wb_promotion" => A020.ui.element_name,
        "a024_bi_indicator" => A024.ui.element_name,
        "a025_bi_dashboard" => A025.ui.element_name,
        _ => "",
    }
}
