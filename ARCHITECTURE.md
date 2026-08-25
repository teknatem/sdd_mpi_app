# ARCHITECTURE

> **GENERATED file - do not edit by hand.** Source of truth is the code.
> Regenerate: `powershell -File tools/gen_architecture.ps1`
> Project object map (aggregates, projections, use-cases, chart of accounts, turnovers, API).

## Mechanisms

Четыре механизма, которыми систему меняют **без пересборки**. Их легко перепутать,
поэтому здесь — что чем является и когда что брать. Что именно заведено в конкретном
экземпляре, лежит в картах `processes`, `plugins` и `actions`, а не здесь.

| Механизм | Что это | Где живёт |
|---|---|---|
| **Процесс** (`pr0001`) | Граф Этапов с курсором и ожиданием по доменному событию: «после какого исхода куда идём». Стартует по триггеру, живёт дольше одного запуска. | Определение — в БД, версионируется. Код механизма — `backend/src/processes/` (graph, worker, instances). |
| **Этап** (`st0001`) | Узел графа: mjs-модуль `(input, host) -> { outcome, data }`. Читает данные, при необходимости зовёт Действия, возвращает один из объявленных выходов. | Определение — в БД, версионируется. Код — `backend/src/processes/stages/`. |
| **Действие** | Операция ядра с побочным эффектом: сухой прогон, ключ идемпотентности, запись в `sys_effect_log`. Кода не имеет — адресуется именем и своей `capability`. | Только в Rust: `backend/src/processes/actions/`. В версионировании определений не участвует. |
| **Плагин** | Самодостаточный JS-артефакт (клиентский и серверный скрипт, стили, SQL-ресурсы), исполняемый в том же QuickJS. Ветки эффектов не видит. | Строкой в таблице `plugin`, адресуется по `manifest.code`. Код движка — `backend/src/plugins/`. |

### Как они соотносятся

Процесс состоит из Этапов; на Этапах вызываются Действия. Это единственная связь
внутри тройки — Действие ничего не знает про Процесс, а Этап переиспользуется между
Процессами, потому что лежит в глобальном каталоге со своими версиями.

**Плагин — сосед, а не часть механизма Процессов.** Разница в назначении: плагин
доставляет функционал пользователю (экран плюс серверные методы), Процесс делает
системную работу внутри конвейера и человеку показывается только журналом и просьбами.
Общий у них рантайм исполнения (QuickJS), а не подсистема.

### Действие и инструмент чата — одна запись, две оболочки

То же самое Действие, поданное в LLM-чат, называется **инструментом**. Реестр
вызываемых операций один (`processes::actions`), оболочек две: оболочка Этапа
(`processes::stages`) и оболочка чата (`shared/llm/chat_effects.rs`). Обе зовут
`actions::run`, где и собран весь контракт безопасности эффекта.

Отсюда практическое следствие: **второй реализации бизнес-логики не заводят**. Операция
с эффектом появляется в `processes/actions/`, инструмент над ней — адаптер. Кто именно
просил (экземпляр и Этап либо диалог, агент и заказчик), едет **актором**, а не входом
Действия — поэтому схема входа одна на обе оболочки.

### Один движок на четыре хоста

Все четыре mjs-хоста — плагины, quality-проверки, задачи навыков и Этапы — исполняются
одним движком `plugins::engine`. Базовая поверхность `host` одинакова: `db.query`,
`db.queryResource`, `log.*`, `context`. Ветка `host.actions` выдаётся **только Этапу** и
только поимённо, по манифесту: у плагинов, проверок и навыков её нет вовсе — право не
отбирают проверкой, его просто не выдают.

Чтение данных у всех mjs-хостов ограничено гардом `sql_guard`: ровно один `SELECT`,
без комментариев, без `*` рядом с таблицей, где лежат креды.

### Что выбрать

- Нужен **экран с данными** для пользователя — плагин.
- Нужна **работа по факту** («день импортирован» → пересчитать, сверить, позвать
  человека), которая переживает перезапуск и умеет ждать, — Процесс.
- Нужна **новая операция с эффектом**, которой ещё нет ни у Этапа, ни у чата, — Действие
  в Rust; это единственный из четырёх, который требует пересборки, и добавляется поштучно.

Термины — `CONTEXT.md`, раздел «Механизм процессов». Почему механизм устроен именно так и
где грабли — ADR-0011 и `backend/src/processes/llm.md`.

## Actions (5)

Операции ядра с побочным эффектом. В mjs Этапа — `host.actions.<method>`, право — `action:<name>` в манифесте Этапа. В LLM-чате те же записи подаются как инструменты.

| Name | host.actions | Title | Reversible | Writes |
|------|--------------|-------|------------|--------|
| `create_agent_task` | `createAgentTask` | Поставить поручение AI-сотруднику | true | `a042_agent_task` |
| `rebuild_day_close` | `rebuildDayClose` | Пересобрать закрытие дня WB | true | `a033_wb_day_close` |
| `repost_documents` | `repostDocuments` | Перепровести документы агрегата | false | `sys_general_ledger`, `p903_wb_finance_report`, `p904_sales_data`, `p907_ym_payment_report`, `p909_mp_order_line_turnovers` |
| `request_human_action` | `requestHumanAction` | Позвать человека | true | `sys_ticket` |
| `run_quality_check` | `runQualityCheck` | Прогнать quality-проверку | true | `sys_quality_check_runs` |

## Aggregates (a0XX)

| Index | Entity | Table | Description | Related | Docs |
|-------|--------|-------|-------------|---------|------|
| `a001` | Подключение 1С | `a001_connection_1c_database` | Настройки подключения к базе данных 1С:Управление торговлей. Используется для импорта справочников (номенклатура, организации, контрагенты)… | a002_organization, a003_counterparty, a004_nomenclature |  |
| `a002` | Организация | `a002_organization` | Юридические лица и ИП, от имени которых ведётся торговля на маркетплейсах. Импортируются из 1С:УТ. Используются для группировки продаж и фин… | a001_connection_1c, a006_connection_mp, a012_wb_sales |  |
| `a003` | Контрагент | `a003_counterparty` | Контрагенты (поставщики, покупатели, партнёры), импортируемые из 1С:Управление торговлей. Поддерживает иерархическую структуру (папки). Соде… | a001_connection_1c, a023_purchase_of_goods |  |
| `a004` | Номенклатура | `a004_nomenclature` | Справочник товаров и категорий из 1С:УТ. Синонимы: товар, номенклатура, позиция, SKU, артикул, карточка товара — упоминание любого из них БЕ… | a001_connection_1c, a007_marketplace_product, a012_wb_sales, a013_ym_order |  |
| `a005` | Маркетплейс | `a005_marketplace` | Справочник торговых площадок: Wildberries, Ozon, Яндекс.Маркет. Системные записи, создаются при инициализации. Используется как справочник т… | a006_connection_mp |  |
| `a006` | Подключение маркетплейса | `a006_connection_mp` | Подключения к торговым площадкам — один магазин на WB, Ozon или Яндекс.Маркет. Содержит API-ключи и идентификаторы магазинов. Используется к… | a002_organization, a005_marketplace, a012_wb_sales, a013_ym_order |  |
| `a007` | Товар маркетплейса | `a007_marketplace_product` | A007 — канонический регистр сопоставления позиции маркетплейса с номенклатурой 1С. Ключ идентификации всегда рассматривается в пределах conn… | a004_nomenclature, a005_marketplace, a006_connection_mp, a008_marketplace_sales |  |
| `a008` | Продажа маркетплейса | `a008_marketplace_sales` | Запись о продаже товара на маркетплейсе: дата начисления, количество, выручка и тип операции (продажа, возврат, комиссия). Основной источник… | a006_connection_mp, a007_marketplace_product, a005_marketplace, a002_organization |  |
| `a009` | Возврат OZON | `a009_ozon_returns` | Возврат товара с OZON. Содержит информацию о дате возврата, причине, типе (полный/частичный), идентификаторах заказа и отправления, а также… | a006_connection_mp, a005_marketplace, a002_organization |  |
| `a010` | Документ OZON FBS | `a010_ozon_fbs_posting` | Отправление OZON по схеме FBS (Fulfillment by Seller — продавец хранит и доставляет сам). Содержит номер отправления, строки товаров, статус… | a006_connection_mp, a005_marketplace, a014_ozon_transactions |  |
| `a011` | Документ OZON FBO | `a011_ozon_fbo_posting` | Отправление OZON по схеме FBO (Fulfillment by OZON — OZON хранит и доставляет). Содержит номер отправления, строки товаров, статус и временн… | a006_connection_mp, a005_marketplace, a014_ozon_transactions |  |
| `a012` | WB Sales | `a012_wb_sales` | Продажи и возвраты с Wildberries. Каждая запись — одна транзакция из отчёта WB (sale или return). Содержит финансовые показатели: выручку, к… | a004_nomenclature, a006_connection_mp, a002_organization | ✓ |
| `a013` | Заказ Яндекс.Маркет | `a013_ym_order` | Заказы с Яндекс.Маркет (YM). Каждая запись — один заказ, может содержать несколько товарных позиций (строк). Данные загружаются через YM API… | a004_nomenclature, a006_connection_mp, a002_organization |  |
| `a014` | Транзакция OZON | `a014_ozon_transactions` | Финансовая транзакция OZON из раздела финансов. Содержит тип операции, суммы начислений, комиссий и доставки. Является основным источником д… | a006_connection_mp, a005_marketplace, a010_ozon_fbs_posting, a011_ozon_fbo_posting |  |
| `a015` | Документ WB Заказы | `a015_wb_orders` | Заказ Wildberries (один заказ = одна строка). Содержит артикул продавца, nmId, штрихкод, категорию, цены со скидками, статус, дату заказа и… | a006_connection_mp, a005_marketplace, a007_marketplace_product, a004_nomenclature |  |
| `a016` | Возврат Yandex Market | `a016_ym_returns` | Возврат товара с Yandex Market. Содержит ID возврата и заказа, тип операции (RETURN или UNREDEEMED — невыкуп), статус возврата денег, строки… | a006_connection_mp, a005_marketplace, a013_ym_order |  |
| `a017` | AI-сотрудник | `a017_llm_agent` | Виртуальный сотрудник (AI-агент): имя, аватар, почта, специализация (agent_type — определяет навыки), должностные обязанности (system_prompt… | a038_llm_connection, a018_llm_chat, a019_llm_artifact |  |
| `a018` | Чат LLM | `a018_llm_chat` | Сессии чатов с LLM агентами. Содержит историю диалогов с языковыми моделями, включая сообщения пользователя и ответы ассистента. Каждый чат… | a017_llm_agent, a019_llm_artifact |  |
| `a019` | Артефакт LLM | `a019_llm_artifact` | SQL-запросы и другие артефакты, созданные LLM агентами в процессе работы с чатами. Каждый артефакт связан с конкретным чатом и агентом, соде… | a017_llm_agent, a018_llm_chat |  |
| `a020` | Акция WB | `a020_wb_promotion` | Календарные акции Wildberries. Каждая запись — одна акция из WB Calendar API с датами проведения и списком товаров (nmId). Данные загружаютс… | a006_connection_mp, a002_organization, a007_marketplace_product |  |
| `a021` | Выпуск продукции | `a021_production_output` | Документ Выпуск продукции из 1С:Управление торговлей. Содержит номер и дату документа, артикул и количество произведённой продукции, сумму с… | a001_connection_1c, a004_nomenclature |  |
| `a022` | Вариант комплектации | `a022_kit_variant` | Вариант комплектации номенклатуры из 1С:Управление торговлей. Описывает состав набора (kit) — какая номенклатура и в каком количестве входит… | a004_nomenclature, a001_connection_1c, a021_production_output |  |
| `a023` | Приобретение товаров | `a023_purchase_of_goods` | Документ Приобретение товаров и услуг из 1С:Управление торговлей. Содержит номер и дату документа, контрагента-поставщика и строки с товарам… | a001_connection_1c, a003_counterparty, a004_nomenclature |  |
| `a024` | BI Индикатор | `a024_bi_indicator` | Индикаторы BI-дашбордов. Каждый индикатор содержит спецификацию источника данных (DataSpec), типизированные параметры (Params), настройки от… | a019_llm_artifact | ✓ |
| `a025` | BI Дашборд | `a025_bi_dashboard` | BI-дашборды. Каждый дашборд объединяет набор BI-индикаторов (a024), сгруппированных по категориям в дерево. Содержит глобальные фильтры, оце… | a024_bi_indicator | ✓ |
| `a026` | Статистика рекламы WB | `a026_wb_advert_daily` | Ежедневная статистика рекламных кампаний Wildberries. Одна запись — один кабинет WB, одна дата и один advert_id. Содержит показы, клики, зак… | a006_connection_mp, a002_organization, a030_wb_advert_campaign, p911_wb_advert_by_items | ✓ |
| `a027` | Документ WB | `a027_wb_documents` | Заголовки отчетных документов Wildberries из API documents/list. Содержит serviceName, категорию, доступные форматы, время создания и призна… | a006_connection_mp, a002_organization, a005_marketplace |  |
| `a028` | Регистр отсутствующих себестоимостей | `a028_missing_cost_registry` | Документ-регистр номенклатуры, для которой не найдена себестоимость при расчёте продаж. Строки хранятся в lines_json; служит рабочим списком… | a004_nomenclature, p912_nomenclature_costs, p904_sales_data |  |
| `a029` | Поставка WB | `a029_wb_supply` | Документ поставки Wildberries: партия заказов, передаваемая на склад WB. Хранит данные API поставки (info_json), состав заказов (supply_orde… | a006_connection_mp, a002_organization, a005_marketplace, a015_wb_orders |  |
| `a030` | Рекламная кампания WB | `a030_wb_advert_campaign` | Справочник рекламных кампаний Wildberries. advert_id — native-идентификатор кампании в WB, по нему рекламные расходы a026 раскладываются в p… | a006_connection_mp, a002_organization, a005_marketplace, a026_wb_advert_daily, p911_wb_advert_by_items, p913_wb_advert_order_attr |  |
| `a031` | Правка базы знаний | `a031_kb_edit` | Заявка на правку базы знаний, подготовленная LLM-агентом по итогам чата: тип правки, статус обработки, целевые и применённые статьи, ссылки… | a017_llm_agent, a018_llm_chat, a019_llm_artifact |  |
| `a032` | Заявка на возврат WB | `a032_wb_returns_claims` | Заявка покупателя на возврат товара WB. Загружается из feedbacks-api.wildberries.ru/api/v1/claims. Содержит ID заявки, nmId, название товара… | a006_connection_mp, a005_marketplace, a012_wb_sales, a015_wb_orders |  |
| `a033` | Закрытие дня WB | `a033_wb_day_close` | Документ закрытия дня по кабинету Wildberries: пересчёт строк дня, выявленные проблемы и сверка с Главной книгой. Строки и итоги хранятся в… | a006_connection_mp, a026_wb_advert_daily, p911_wb_advert_by_items, p913_wb_advert_order_attr, general_ledger | ✓ |
| `a034` | Реализация YM | `a034_ym_realization` | Официальный «Отчёт о реализации» Yandex Market, импортируемый как суточный документ (один кабинет, одна дата). Содержит выручку по покупател… | a006_connection_mp, a002_organization, p907_ym_payment_report | ✓ |
| `a035` | Сверка перечислений YM | `a035_ym_settlement_recon` | Документ-сверка одного банковского ордера YM (bank_order_id из p907_ym_payment_report). Таблица операций ордера сгруппирована по нашим оборо… | p907_ym_payment_report, a006_connection_mp, a002_organization |  |
| `a036` | Воронка продаж WB | `a036_wb_sales_funnel_daily` | Ежедневная воронка продаж Wildberries в разрезе номенклатуры. Одна запись — один кабинет WB и одна дата; JSON детализация по товарам (nm_id)… | a006_connection_mp, a002_organization, a007_marketplace_product, a026_wb_advert_daily |  |
| `a037` | Данные по товарам WB | `a037_wb_product_snapshot` | Ежедневные снимки состояния товаров Wildberries в разрезе номенклатуры: остатки на складах WB и продавца, сумма остатков, рейтинг карточки и… | a006_connection_mp, a002_organization, a007_marketplace_product, a036_wb_sales_funnel_daily |  |
| `a038` | Подключение LLM | `a038_llm_connection` | Техническое подключение к провайдеру LLM (OpenAI, OpenRouter, DeepSeek). Содержит API-ключ, эндпоинт, параметры модели (temperature, max_tok… | a018_llm_chat, a019_llm_artifact |  |
| `a039` | Письмо | `a039_mail_message` | Журнал входящих и исходящих писем почтового конвейера. Одна запись = одно письмо (кратко): направление, отправитель/получатель, тема, статус… | a018_llm_chat, a038_llm_connection, a019_llm_artifact |  |
| `a040` | Поисковая аналитика WB | `a040_wb_search_analytics_daily` | Ежедневные снимки поисковой аналитики Wildberries в разрезе номенклатуры (search-report / «Товары по контенту», подписка «Джем»): видимость… | a006_connection_mp, a002_organization, a007_marketplace_product, a036_wb_sales_funnel_daily, a037_wb_product_snapshot | ✓ |
| `a041` | Воронка продаж YM | `a041_ym_shows_sales_daily` | Ежедневная воронка продаж Яндекс.Маркета в разрезе товаров. Одна запись — один кабинет YM и одна дата; JSON-детализация по товарам (offer_id… | a006_connection_mp, a002_organization, a007_marketplace_product, a013_ym_order, a016_ym_returns |  |
| `a042` | Поручение AI-сотруднику | `a042_agent_task` | Очередь поручений между AI-сотрудниками: один агент ставит задачу специалисту другой специализации, регламентное задание task029 исполняет е… | a017_llm_agent, a018_llm_chat, a019_llm_artifact |  |
| `a043` | Финансовый отчёт WB | `a043_wb_finance_report` | Ежедневные финансовые отчёты реализации Wildberries из Finance API v1. Один документ соответствует одному reportId и содержит исходную шапку… | a006_connection_mp, p903_wb_finance_report |  |

## Projections (p9XX)

| Code | Name | Docs |
|------|------|------|
| `p900` | mp sales register |  |
| `p901` | nomenclature barcodes |  |
| `p902` | ozon finance realization |  |
| `p903` | wb finance report |  |
| `p904` | sales data | ✓ |
| `p905` | wb commission history |  |
| `p906` | nomenclature prices |  |
| `p907` | ym payment report |  |
| `p908` | wb goods prices |  |
| `p909` | mp order line turnovers |  |
| `p910` | mp unlinked turnovers |  |
| `p911` | wb advert by items |  |
| `p912` | nomenclature costs |  |
| `p913` | wb advert order attr |  |
| `p914` | mp finance turnovers |  |
| `p915` | mp order events |  |
| `p916` | mp sales funnel turnovers | ✓ |

## Use-cases (u5XX)

| Code | Name | Docs |
|------|------|------|
| `u501` | import from ut |  |
| `u502` | import from ozon |  |
| `u503` | import from yandex |  |
| `u504` | import from wildberries | ✓ |
| `u505` | match nomenclature |  |
| `u506` | import from lemanapro |  |
| `u507` | import from erp |  |
| `u508` | repost documents |  |

## Data schemes (dsXX)

| Code | Name |
|------|------|
| `ds01` | wb finance report |
| `ds02` | mp sales register |
| `ds03` | p904 sales |

## Scheduled tasks (task0XX)

| Code | Name |
|------|------|
| `task001` | wb orders fbs polling |
| `task002` | wb orders stats hourly |
| `task003` | wb products |
| `task004` | wb sales |
| `task005` | wb supplies |
| `task006` | wb finance |
| `task007` | wb commissions |
| `task008` | wb prices |
| `task009` | wb promotions |
| `task010` | wb documents |
| `task011` | wb advert |
| `task012` | wb advert campaigns |
| `task013` | ym orders polling |
| `task014` | kb analyze |
| `task015` | kb post |
| `task016` | kb intake |
| `task017` | wb returns claims |
| `task018` | ym returns |
| `task019` | ym payment report |
| `task020` | wb product snapshot |
| `task021` | mail intake |
| `task022` | mail reply |
| `task023` | wb sales funnel daily |
| `task024` | wb search analytics daily |
| `task025` | bitrix ticket sync |
| `task026` | ym shows sales daily |
| `task027` | llm judge |
| `task028` | llm golden set |
| `task029` | agent task runner |
| `task030` | wb finance reports |

## Dashboards (d4XX)

| Code | Name | Backend | Frontend |
|------|------|---------|----------|
| `d400` | monthly summary | + | + |
| `d401` | wb finance |  | + |
| `d402` | wb order flow |  | + |
| `d403` | ym order flow |  | + |
| `d404` | wb advert report |  | + |
| `d405` | metadata dashboard |  | + |
| `d406` | wb sales funnel |  | + |
| `d407` | llm quality |  | + |

## DataView (dvXXX)

| Code | Description | Docs |
|------|-------------|------|
| `dv001_revenue` | dv001 — DataView: Продажи (2 периода) | ✓ |
| `dv002_wb_advert_by_items` | dv002 - DataView: WB рекламные расходы по номенклатуре (2 периода) |  |
| `dv003_mp_order_line_turnovers` | dv003 - DataView: MP order line KPI (2 periods) |  |
| `dv004_general_ledger_turnovers` | dv004 - DataView: General ledger turnovers KPI (2 periods) | ✓ |
| `dv005_gl_account_view_total` | dv005 - DataView: GL account view totals (2 periods) |  |
| `dv006_indicator_ratio_percent` | dv006 - DataView: ratio of two BI indicators. |  |
| `dv007_gl_turnover_ratio_percent` | dv007 - DataView: ratio of two GL turnover formulas, expressed in percent. |  |
| `dv008_wb_sales_funnel` | dv008 - DataView: WB sales funnel (2 periods) |  |

## Quality checks

| Code | Description |
|------|-------------|
| `gl_projection_integrity` | Проверка: целостность GL ↔ ProjectionLinked-проекции |
| `kb_integrity` | Проверка: целостность базы знаний |
| `marketplace_product_ref_required` | Checks that marketplace_product_ref is filled in active marketplace tables. |
| `nomenclature_in_projections` | Проверка: заполненность `nomenclature_ref` в проекциях |
| `p903_gl_integrity` | Проверка: целостность GL ↔ p903_wb_finance_report (ExternalLinked) |
| `p907_gl_coverage` | Проверка: полнота проведения p907_ym_payment_report → GL |
| `projection_orphan_registrators` | Проверка: строки проекций без исходных регистраторов |

## Chart of accounts (account_registry)

| Account | Name | Parent | Section |
|---------|------|--------|---------|
| `62` | Расчёты с покупателями |  | BalanceSheet |
| `44` | Расходы на продажу |  | ProfitLoss |
| `4401` | Расходы на продажу — маркетплейс | 44 | ProfitLoss |
| `41` | Товары |  | BalanceSheet |
| `90` | Продажи |  | ProfitLoss |
| `9001` | Выручка от продаж | 90 | ProfitLoss |
| `9002` | Себестоимость продаж | 90 | ProfitLoss |
| `91` | Прочие доходы и расходы |  | ProfitLoss |
| `76` | Расчёты с прочими дебиторами и кредиторами |  | BalanceSheet |
| `7609` | Расчёты с маркетплейсом | 76 | BalanceSheet |
| `76YB` | Баланс баллов/промо (Яндекс.Маркет) | 76 | BalanceSheet |
| `76YA` | Деньги покупателей у Я.Маркет (предоплаты в пути) | 76 | BalanceSheet |
| `51` | Расчётный счёт |  | BalanceSheet |

## Turnover classes (turnover_registry)

| Code | Name | Debit | Credit | Entry |
|------|------|-------|--------|-------|
| `qty_ordered` | Количество заказано |  |  |  |
| `qty_sold` | Количество продано |  |  |  |
| `qty_returned` | Количество возвращено |  |  |  |
| `customer_revenue` | Выручка от покупателя | 7609 | 9001 | ✓ |
| `customer_revenue_pl` | Выручка по прайслисту | 7609 | 9001 | ✓ |
| `customer_return` | Возврат покупателя | 7609 | 9001 | ✓ |
| `seller_payout` | Выплата продавцу |  |  |  |
| `mp_commission` | Комиссия маркетплейса | 4401 | 7609 | ✓ |
| `mp_commission_adjustment` | Корректировка комиссии WB | 4402 | 7609 | ✓ |
| `mp_commission_adjustment_nm` | Корректировка комиссии WB (с номенклатурой) | 4402 | 7609 | ✓ |
| `mp_acquiring` | Эквайринг маркетплейса | 4403 | 7609 | ✓ |
| `mp_logistics` | Логистика маркетплейса | 4404 | 7609 | ✓ |
| `mp_rebill_logistic_cost` | Возмещение расходов по перевозке | 4404 | 7609 | ✓ |
| `mp_rebill_logistic_cost_nm` | Возмещение расходов по перевозке (с номенклатурой) | 4404 | 7609 | ✓ |
| `mp_ppvz_reward` | Возмещение за выдачу и возврат товаров на ПВЗ | 4404 | 7609 | ✓ |
| `mp_ppvz_reward_nm` | Возмещение за выдачу и возврат товаров на ПВЗ (с номенклатурой) | 4404 | 7609 | ✓ |
| `mp_storage` | Хранение маркетплейса | 4404 | 7609 | ✓ |
| `mp_penalty` | Штраф маркетплейса | 9102 | 7609 | ✓ |
| `mp_penalty_storno` | Штраф маркетплейса (сторно) | 9102 | 7609 | ✓ |
| `mp_rebill_logistic_cost_legacy` | Возмещение издержек по перевозке и складским операциям | 4404 | 7609 | ✓ |
| `item_cost` | Себестоимость | 9002 | 41 | ✓ |
| `spp_discount` | Скидка SPP (продажа) | 7609 | 9001 | ✓ |
| `spp_discount_storno` | Скидка SPP (возврат) | 7609 | 9001 | ✓ |
| `wb_extra_discount` | Доп. скидка WB сверх СПП (продажа) | 7609 | 9001 | ✓ |
| `wb_extra_discount_storno` | Доп. скидка WB сверх СПП (сторно возврат) | 7609 | 9001 | ✓ |
| `wb_coinvestment` | Соинвестирование WB (продажа) | 7609 | 91 | ✓ |
| `wb_coinvestment_storno` | Соинвестирование WB (возврат) | 7609 | 91 | ✓ |
| `advert_clicks_no_order` | Рекламные расходы по номенклатуре | 9102 | 7609 | ✓ |
| `advert_clicks_order_accrual` | Резерв рекламных расходов по заказу | 9601 | 7609 | ✓ |
| `advert_clicks_order_expense` | Рекламные расходы при реализации | 9102 | 9601 | ✓ |
| `advertising` | Реклама |  |  |  |
| `acceptance` | Приемка | 4401 | 7609 | ✓ |
| `adjustment_income` | Корректировка дохода |  |  |  |
| `voluntary_return_compensation` | Добровольная компенсация при возврате | 7609 | 91 | ✓ |
| `adjustment_expense` | Корректировка расхода |  |  |  |
| `other_income` | Прочие доходы | 7609 | 91 | ✓ |
| `other_expense` | Прочие расходы | 7609 | 9102 | ✓ |
| `ym_settlement` | Перечисление на расчётный счёт (Я.Маркет) | 51 | 7609 | ✓ |
| `prepayment` | Предоплата покупателя (получение) | 76YA | 62 | ✓ |
| `prepayment_storno` | Предоплата покупателя (сторно возврата) | 76YA | 62 | ✓ |
| `prepayment_settle` | Зачёт предоплаты на отгрузке | 7609 | 76YA | ✓ |
| `prepayment_settle_storno` | Зачёт предоплаты на отгрузке (сторно возврата) | 7609 | 76YA | ✓ |
| `qty_sold_storno` | Количество продано (сторно возврат) |  |  |  |
| `customer_revenue_storno` | Выручка от покупателя (сторно возврат) | 7609 | 9001 |  |
| `customer_revenue_pl_storno` | Выручка по прайслисту (сторно возврат) | 7609 | 9001 | ✓ |
| `mp_commission_storno` | Комиссия маркетплейса (сторно возврат) | 4401 | 7609 | ✓ |
| `mp_acquiring_storno` | Эквайринг маркетплейса (сторно возврат) | 4403 | 7609 | ✓ |
| `seller_payout_storno` | Выплата продавцу (сторно возврат) |  |  |  |
| `item_cost_storno` | Себестоимость (сторно возврат) | 9002 | 41 | ✓ |
| `commission_percent` | Процент комиссии |  |  |  |

## UI scopes (67)

| Scope | Type | Category | Label | Description |
|-------|------|----------|-------|-------------|
| `a001_connection_1c` | Aggregate | references | Подключения 1С | Настройки подключения к системам 1С для синхронизации данных |
| `a002_organization` | Aggregate | references | Организации | Справочник организаций компании |
| `a003_counterparty` | Aggregate | references | Контрагенты | Справочник контрагентов (поставщики, покупатели) |
| `a004_nomenclature` | Aggregate | references | Номенклатура | Справочник товаров и продуктов |
| `a005_marketplace` | Aggregate | references | Маркетплейсы | Справочник маркетплейсов (WB, Ozon, Яндекс Маркет) |
| `a006_connection_mp` | Aggregate | references | Подключения к маркетплейсам | API-токены и настройки подключения к кабинетам маркетплейсов |
| `a007_marketplace_product` | Aggregate | references | Товары на маркетплейсах | Карточки товаров, привязанных к кабинетам маркетплейсов |
| `a008_marketplace_sales` | Aggregate | marketplace_data | Продажи маркетплейсов | Сводные данные о продажах по всем маркетплейсам |
| `a009_ozon_returns` | Aggregate | marketplace_data | Возвраты Ozon | Данные о возвратах товаров на Ozon |
| `a010_ozon_fbs_posting` | Aggregate | marketplace_data | Отправления Ozon FBS | Отправления по схеме FBS (Fulfillment by Seller) на Ozon |
| `a011_ozon_fbo_posting` | Aggregate | marketplace_data | Отправления Ozon FBO | Отправления по схеме FBO (Fulfillment by Ozon) |
| `a012_wb_sales` | Aggregate | marketplace_data | Продажи Wildberries | Данные о продажах товаров на Wildberries |
| `a013_ym_order` | Aggregate | marketplace_data | Заказы Яндекс Маркет | Данные о заказах на Яндекс Маркет |
| `a014_ozon_transactions` | Aggregate | marketplace_data | Транзакции Ozon | Финансовые транзакции по кабинетам Ozon |
| `a015_wb_orders` | Aggregate | marketplace_data | Заказы Wildberries | Данные о заказах на Wildberries |
| `a016_ym_returns` | Aggregate | marketplace_data | Возвраты Яндекс Маркет | Данные о возвратах товаров на Яндекс Маркет |
| `a020_wb_promotion` | Aggregate | marketplace_data | Акции Wildberries | Рекламные акции и скидки на Wildberries |
| `a026_wb_advert_daily` | Aggregate | marketplace_data | Реклама Wildberries (daily) | Ежедневная статистика рекламных кампаний Wildberries |
| `a036_wb_sales_funnel_daily` | Aggregate | marketplace_data | Воронка продаж WB | Ежедневная воронка продаж Wildberries в разрезе номенклатуры |
| `a037_wb_product_snapshot` | Aggregate | marketplace_data | Данные по товарам WB | Ежедневные данные по остаткам и рейтингам товаров Wildberries |
| `a040_wb_search_analytics_daily` | Aggregate | marketplace_data | Поисковая аналитика WB | Показы, позиции в выдаче и поисковые запросы товаров Wildberries |
| `a041_ym_shows_sales_daily` | Aggregate | marketplace_data | Воронка продаж Yandex Market | Показы, клики, корзины и заказы по товарам Yandex Market |
| `a043_wb_finance_report` | Aggregate | marketplace_data | Финансовые отчёты WB (новый API) | Ежедневные отчёты реализации WB Finance API v1 без проекций |
| `a034_ym_realization` | Aggregate | marketplace_data | Реализация YM (Отчёт о реализации) | Официальный отчёт о реализации Yandex Market, слой ybuh |
| `a027_wb_documents` | Aggregate | marketplace_data | Документы Wildberries | Документы поставок и логистики Wildberries |
| `a029_wb_supply` | Aggregate | marketplace_data | Поставки Wildberries | Поставки товаров на склады Wildberries |
| `a030_wb_advert_campaign` | Aggregate | marketplace_data | Кампании рекламы Wildberries | Справочник рекламных кампаний Wildberries и их свойств |
| `a021_production_output` | Aggregate | production | Выпуск продукции | Данные о выпуске продукции на производстве |
| `a022_kit_variant` | Aggregate | production | Варианты комплектов | Конфигурации комплектов и наборов товаров |
| `a023_purchase_of_goods` | Aggregate | production | Закупки товаров | Данные о закупках товаров у поставщиков |
| `a028_missing_cost_registry` | Aggregate | production | Реестр незакрытых себестоимостей | Позиции без рассчитанной себестоимости |
| `a024_bi_indicator` | Aggregate | analytics | BI Индикаторы | Индикаторы BI-дашбордов — конфигурации источников данных и отображения |
| `a025_bi_dashboard` | Aggregate | analytics | BI Дашборды | Аналитические дашборды с наборами индикаторов и фильтрами |
| `bi_timeline` | System | analytics | BI Timeline | Проверка динамики BI-индикаторов по дневным рядам и двум периодам |
| `a017_llm_agent` | Aggregate | ai | LLM Агенты | Конфигурации AI-агентов на базе языковых моделей |
| `a018_llm_chat` | Aggregate | ai | LLM Чаты | Сессии чатов с языковыми моделями |
| `a019_llm_artifact` | Aggregate | ai | LLM Артефакты | Артефакты, созданные языковыми моделями |
| `a038_llm_connection` | Aggregate | ai | Подключения LLM | Настройки подключений к провайдерам языковых моделей |
| `a039_mail_message` | Aggregate | ai | Письма (журнал) | Журнал входящих и исходящих писем почтового конвейера LLM-агентов |
| `a031_kb_edit` | Aggregate | ai | Редактирование базы знаний | Тикеты администратора базы знаний: предложения, обсуждения и публикация статей |
| `a042_agent_task` | Aggregate | ai | Поручения AI-сотрудникам | Очередь задач, которые агенты передают друг другу: постановка, статус, результат |
| `a032_wb_returns_claims` | Aggregate | wildberries | Заявки на возврат WB | Заявки покупателей на возврат товара Wildberries (feedbacks-api /api/v1/claims) |
| `a033_wb_day_close` | Aggregate | wildberries | Закрытие дня WB | Документ-снапшот итогов дня WB-кабинета: 10 колонок по заказам, проверка рекламной атрибуции |
| `knowledge_base` | System | ai | Инвентаризация знаний | Полный учёт источников знания: статьи, карты, навыки, проверки, \ плагины, Процессы, Действия, сущности реестра, источни… |
| `p900_mp_sales_register` | Projection | analytics | Реестр продаж | Сводный аналитический реестр всех продаж по маркетплейсам |
| `p901_nomenclature_barcodes` | Projection | references | Штрихкоды номенклатуры | Справочник штрихкодов, привязанных к номенклатуре |
| `p902_ozon_finance_realization` | Projection | analytics | Финансовая реализация Ozon | Данные финансовой реализации по кабинетам Ozon |
| `p903_wb_finance_report` | Projection | analytics | Финансовый отчёт Wildberries | Отчёты о реализации и комиссиях Wildberries |
| `p904_sales_data` | Projection | analytics | Данные продаж | Агрегированные данные продаж для аналитики |
| `p905_wb_commission_history` | Projection | analytics | История комиссий Wildberries | Исторические данные о ставках комиссий Wildberries |
| `p906_nomenclature_prices` | Projection | analytics | Цены номенклатуры | История цен на товары по периодам |
| `p907_ym_payment_report` | Projection | analytics | Отчёт по выплатам Яндекс Маркет | Отчёты о выплатах по кабинетам Яндекс Маркет |
| `p908_wb_goods_prices` | Projection | analytics | Цены товаров Wildberries | Текущие цены товаров в каталоге Wildberries |
| `p912_nomenclature_costs` | Projection | analytics | Себестоимость номенклатуры | Рассчитанная себестоимость товаров |
| `u501_import_from_ut` | Usecase | imports | Импорт из 1С | Загрузка справочников и документов из системы 1С:Управление торговлей |
| `u502_import_from_ozon` | Usecase | imports | Импорт из Ozon | Загрузка заказов, транзакций и товаров с маркетплейса Ozon |
| `u503_import_from_yandex` | Usecase | imports | Импорт из Яндекс Маркет | Загрузка заказов и финансовых данных с Яндекс Маркет |
| `u504_import_from_wildberries` | Usecase | imports | Импорт из Wildberries | Загрузка заказов, поставок, остатков и рекламных данных с Wildberries |
| `u505_match_nomenclature` | Usecase | imports | Сопоставление номенклатуры | Автоматическое сопоставление товаров маркетплейсов с внутренней номенклатурой |
| `u506_import_from_lemanapro` | Usecase | imports | Импорт из Leroy Merlin | Загрузка данных с маркетплейса Leroy Merlin (LemanaПРО) |
| `u507_import_from_erp` | Usecase | imports | Импорт из ERP | Загрузка данных из корпоративной ERP-системы |
| `u508_repost_documents` | Usecase | imports | Перепроведение документов | Массовое перепроведение агрегатов и проекций после изменения данных |
| `general_ledger` | System | system | Общий журнал операций | Аналитический журнал всех хозяйственных операций и оборотов по счетам |
| `data_view` | System | system | Просмотры данных (Data Views) | Конфигурируемые представления данных для BI-индикаторов |
| `dashboard` | System | system | Системные дашборды | Служебные аналитические дашборды (ежемесячная сводка и пр.) |
| `sys_s3_files` | System | system | S3 файлы | Управление файлами в Yandex Object Storage |
| `sys_datasets` | System | system | Наборы данных и перенос | Выгрузка наборов данных в S3 и восстановление на другом экземпляре |

## UI sidebar (13 groups)

> Tab key = page identity: it is also the scope id and the key in `layout/tabs/registry.rs`.
> Plugin pages are added at runtime from the `plugin` table and are not listed here.

### `navigator` Навигация

| Tab key | Label | Scope | Component |
|---------|-------|-------|-----------|
| `navigator_marketplace` | Все по маркетплейсам |  | MarketplaceNavigator |

### `dashboards` Дашборды

| Tab key | Label | Scope | Component |
|---------|-------|-------|-----------|
| `a024_bi_indicator` | BI Индикатор | `a024_bi_indicator` | BiIndicatorList |
| `a025_bi_dashboard` | BI Дашборд | `a025_bi_dashboard` | BiDashboardList |
| `bi_timeline` | BI Timeline | `bi_timeline` | BiTimelinePage |
| `d402_wb_order_flow` | WB История заказов |  | WbOrderFlowDashboard |
| `d403_ym_order_flow` | YM История заказов |  | YmOrderFlowDashboard |
| `d406_wb_sales_funnel` | Воронка продаж |  | WbSalesFunnelDashboard |

### `knowledge_base` Знания

| Tab key | Label | Scope | Component |
|---------|-------|-------|-----------|
| `knowledge_base` | Инвентаризация знаний | `knowledge_base` | KnowledgeInventoryPage |
| `a031_kb_edit` | Редактирование базы знаний | `a031_kb_edit` | KbEditList |

### `references` Справочники

| Tab key | Label | Scope | Component |
|---------|-------|-------|-----------|
| `a002_organization` | Организация | `a002_organization` | OrganizationList |
| `a003_counterparty` | Контрагенты | `a003_counterparty` | CounterpartyTree |
| `a004_nomenclature` | Номенклатура | `a004_nomenclature` | NomenclatureTree |
| `a004_nomenclature_list` | Номенклатура (список) | `a004_nomenclature` | NomenclatureList |
| `a005_marketplace` | Маркетплейс | `a005_marketplace` | MarketplaceList |
| `a007_marketplace_product` | Товары маркетплейсов | `a007_marketplace_product` | MarketplaceProductList |
| `a030_wb_advert_campaign` | Рекламные кампании WB | `a030_wb_advert_campaign` | WbAdvertCampaignList |

### `documents` Документы

| Tab key | Label | Scope | Component |
|---------|-------|-------|-----------|
| `a015_wb_orders` | WB Orders | `a015_wb_orders` | WbOrdersList |
| `a026_wb_advert_daily` | Статистика рекламы WB | `a026_wb_advert_daily` | WbAdvertDailyList |
| `a036_wb_sales_funnel_daily` | Воронка продаж WB | `a036_wb_sales_funnel_daily` | WbSalesFunnelDailyList |
| `a037_wb_product_snapshot` | Данные по товарам WB | `a037_wb_product_snapshot` | WbProductSnapshotList |
| `a040_wb_search_analytics_daily` | Поисковая аналитика WB | `a040_wb_search_analytics_daily` | WbSearchAnalyticsList |
| `a041_ym_shows_sales_daily` | Воронка продаж Yandex Market | `a041_ym_shows_sales_daily` | YmShowsSalesDailyList |
| `a033_wb_day_close` | Закрытие дня WB | `a033_wb_day_close` | WbDayCloseList |
| `a027_wb_documents` | Документы WB | `a027_wb_documents` | WbDocumentsList |
| `a043_wb_finance_report` | Финансовые отчёты WB (новый API) | `a043_wb_finance_report` | WbFinanceReportsList |
| `a021_production_output` | Выпуск продукции | `a021_production_output` | ProductionOutputList |
| `a022_kit_variant` | Варианты комплектации | `a022_kit_variant` | KitVariantList |
| `a023_purchase_of_goods` | Приобретение товаров | `a023_purchase_of_goods` | PurchaseOfGoodsList |
| `a028_missing_cost_registry` | Реестр отсутствующих цен | `a028_missing_cost_registry` | MissingCostRegistryList |
| `a029_wb_supply` | Поставки WB (FBS) | `a029_wb_supply` | WbSupplyList |
| `a032_wb_returns_claims` | Заявки на возврат WB | `a032_wb_returns_claims` | WbReturnsClaimsList |
| `a020_wb_promotion` | Акция WB | `a020_wb_promotion` | WbPromotionList |
| `a013_ym_order` | Заказ Яндекс.Маркет | `a013_ym_order` | YmOrderList |
| `a010_ozon_fbs_posting` | OZON FBS Posting | `a010_ozon_fbs_posting` | OzonFbsPostingList |
| `a011_ozon_fbo_posting` | OZON FBO Posting | `a011_ozon_fbo_posting` | OzonFboPostingList |
| `a012_wb_sales` | WB Sales | `a012_wb_sales` | WbSalesList |
| `a009_ozon_returns` | Возвраты OZON | `a009_ozon_returns` | OzonReturnsList |
| `a016_ym_returns` | Возвраты Yandex | `a016_ym_returns` | YmReturnsList |
| `a008_marketplace_sales` | Продажи МП | `a008_marketplace_sales` | MarketplaceSalesList |
| `a014_ozon_transactions` | Транзакции OZON | `a014_ozon_transactions` | OzonTransactionsList |

### `integrations` Интеграции

| Tab key | Label | Scope | Component |
|---------|-------|-------|-----------|
| `a001_connection_1c` | Подключение 1С | `a001_connection_1c` | Connection1CList |
| `a006_connection_mp` | Подключение маркетплейса | `a006_connection_mp` | ConnectionMPList |
| `u501_import_from_ut` | Импорт из УТ 11 |  | ImportWidget |
| `u502_import_from_ozon` | Импорт из OZON |  | ImportWidget |
| `u503_import_from_yandex` | Импорт из Yandex |  | ImportWidget |
| `u504_import_from_wildberries` | Импорт из Wildberries |  | ImportWidget |
| `u506_import_from_lemanapro` | Импорт из ЛеманаПро |  | ImportWidget |
| `u507_import_from_erp` | Импорт из ERP |  | ImportWidget |
| `u508_repost_documents` | Перепроведение документов |  | RepostDocumentsWidget |

### `operations` Финансы (admin only)

| Tab key | Label | Scope | Component |
|---------|-------|-------|-----------|
| `general_ledger` | Главная книга |  | GeneralLedgerPage |
| `general_ledger_turnovers` | Обороты GL |  | GeneralLedgerTurnoversPage |
| `general_ledger_dimensions` | Измерения GL |  | GeneralLedgerDimensionsPage |
| `general_ledger_layers` | Слои GL |  | GeneralLedgerLayersPage |
| `general_ledger_entities` | Субъекты GL |  | GeneralLedgerEntitiesPage |
| `supplier_balance` | Баланс к перечислению (YM) |  | SupplierBalancePage |
| `general_ledger_matrix` | Матрица Слой/Оборот |  | GeneralLedgerLayerTurnoverMatrixPage |
| `u505_match_nomenclature` | Сопоставление |  | MatchNomenclatureView |

### `llm` Чаты LLM

| Tab key | Label | Scope | Component |
|---------|-------|-------|-----------|
| `a018_llm_chat` | Чат LLM | `a018_llm_chat` | LlmChatList |
| `a019_llm_artifact` | Артефакт LLM | `a019_llm_artifact` | LlmArtifactList |
| `a017_llm_agent` | AI-сотрудник | `a017_llm_agent` | LlmAgentList |
| `a038_llm_connection` | Подключение LLM | `a038_llm_connection` | LlmConnectionList |
| `a039_mail_message` | Письма (журнал) | `a039_mail_message` | MailMessageList |
| `a042_agent_task` | Поручения AI-сотрудникам | `a042_agent_task` | AgentTaskList |
| `llm_skills` | Навыки LLM |  | LlmSkillList |
| `llm_tools` | Инструменты LLM |  | LlmToolList |
| `d407_llm_quality` | Качество агентов |  | LlmQualityDashboard |

### `reports` Отчеты (admin only)

| Tab key | Label | Scope | Component |
|---------|-------|-------|-----------|
| `general_ledger_report` | Отчёт GL |  | GeneralLedgerReportPage |
| `gl_account_view__7609` | Ведомость по кабинетам |  | GlAccountViewPage |
| `wb_weekly_reconciliation` | Сверка weekly WB и GL 7609 |  | WbWeeklyReconciliationPage |
| `ym_revenue_reconciliation` | Сверка выручки YM (fina vs ybuh) |  | YmRevenueReconciliationPage |
| `report_a026_wb_advert_daily` | Реклама WB — выгрузка CSV | `a026_wb_advert_daily` | WbAdvertDailyList |

### `information` Информация

| Tab key | Label | Scope | Component |
|---------|-------|-------|-----------|
| `p900_sales_register` | Регистр продаж |  | SalesRegisterList |
| `p901_barcodes` | Штрихкоды номенклатуры |  | BarcodesList |
| `p902_ozon_finance_realization` | OZON Finance Realization |  | OzonFinanceRealizationList |
| `p903_wb_finance_report` | WB Finance Report |  | WbFinanceReportList |
| `p904_sales_data` | Sales Data |  | SalesDataList |
| `p905_commission_history` | WB Commission History |  | CommissionHistoryList |
| `p906_nomenclature_prices` | Дилерские цены (УТ) |  | NomenclaturePricesList |
| `p907_ym_payment_report` | YM Отчёт по платежам |  | YmPaymentReportList |
| `a034_ym_realization` | Реализация YM |  | YmRealizationList |
| `a035_ym_settlement_recon` | Сверка перечислений YM |  | YmSettlementReconList |
| `p908_wb_goods_prices` | WB Цены товаров |  | WbGoodsPricesList |
| `p913_wb_advert_order_attr` | Атрибуция расходов WB |  | WbAdvertOrderAttrList |
| `p914_mp_finance_turnovers` | Финансовые обороты (fina) |  | MpFinanceTurnoverList |
| `a032_wb_returns_claims` | Заявки на возврат WB |  | WbReturnsClaimsList |

### `support` Техподдержка

| Tab key | Label | Scope | Component |
|---------|-------|-------|-----------|
| `sys_tickets` | Тикеты |  | TicketsListPage |

### `settings` Настройки (admin only)

| Tab key | Label | Scope | Component |
|---------|-------|-------|-----------|
| `data_view` | DataView |  | DataViewList |
| `universal_dashboard` | Конструктор запросов |  | UniversalDashboard |
| `schema_browser` | Схемы таблиц |  | SchemaBrowser |
| `drilldown__new` | Детализация |  | DrilldownReportPage |
| `all_reports` | Все отчеты |  | AllReportsList |
| `filter_registry` | Реестр фильтров |  | FilterRegistryPage |
| `d400_monthly_summary` | Сводка за месяц |  | MonthlySummaryDashboard |
| `d405_metadata_dashboard` | Метаданные |  | MetadataDashboard |

### `administration` Система (admin only)

| Tab key | Label | Scope | Component |
|---------|-------|-------|-----------|
| `sys_metrics` | Метрики проекта |  | ProjectMetricsPage |
| `sys_processes` | Процессы |  | ProcessesPage |
| `sys_users` | Пользователи |  | UsersListPage |
| `sys_roles` | Роли |  | RolesListPage |
| `sys_roles_matrix` | Матрица ролей |  | RoleMatrixPage |
| `sys_audit` | Аудит доступа |  | AuditPage |
| `sys_s3_files` | S3 файлы |  | S3FilesPage |
| `sys_datasets` | Наборы данных и перенос |  | DatasetsPage |
| `sys_raw_storage` | Настройка raw JSON |  | RawStoragePage |
| `quality_checks` | Контроль качества данных |  | QualityCheckList |
| `sys_tasks` | Регламентные задания |  | ScheduledTaskList |
| `sys_task_type_registry` | Реестр типов заданий |  | TaskTypeRegistryPage |
| `sys_thaw_test` | Тест Thaw UI |  | ThawTestPage |
| `sys_style_guide` | Гид по стилям |  | StyleGuidePage |

## API routes (476)

### `/a004`
- `GET` /api/a004/nomenclature

### `/a007`
- `GET` /api/a007/marketplace-product

### `/a009`
- `POST` /api/a009/ozon-returns/:id/post
- `POST` /api/a009/ozon-returns/:id/unpost

### `/a010`
- `GET` /api/a010/ozon-fbs-posting
- `GET` /api/a010/ozon-fbs-posting/:id
- `POST` /api/a010/ozon-fbs-posting/:id/post
- `POST` /api/a010/ozon-fbs-posting/:id/unpost
- `POST` /api/a010/ozon-fbs-posting/post-period
- `GET` /api/a010/raw/:ref_id

### `/a011`
- `GET` /api/a011/ozon-fbo-posting
- `GET` /api/a011/ozon-fbo-posting/:id
- `POST` /api/a011/ozon-fbo-posting/:id/post
- `POST` /api/a011/ozon-fbo-posting/:id/unpost
- `POST` /api/a011/ozon-fbo-posting/post-period

### `/a012`
- `GET` /api/a012/raw/:ref_id
- `GET` /api/a012/wb-sales
- `GET` /api/a012/wb-sales/:id
- `GET` /api/a012/wb-sales/:id/advert-attribution
- `GET` /api/a012/wb-sales/:id/journal
- `POST` /api/a012/wb-sales/:id/post
- `GET` /api/a012/wb-sales/:id/projections
- `POST` /api/a012/wb-sales/:id/refresh-dealer-price
- `POST` /api/a012/wb-sales/:id/unpost
- `POST` /api/a012/wb-sales/batch-post
- `POST` /api/a012/wb-sales/batch-unpost
- `POST` /api/a012/wb-sales/migrate-sale-id
- `POST` /api/a012/wb-sales/post-period
- `GET` /api/a012/wb-sales/search-by-srid

### `/a013`
- `GET` /api/a013/raw/:ref_id
- `GET` /api/a013/ym-order
- `GET` /api/a013/ym-order/:id
- `POST` /api/a013/ym-order/:id/post
- `GET` /api/a013/ym-order/:id/projections
- `POST` /api/a013/ym-order/:id/unpost
- `POST` /api/a013/ym-order/batch-post
- `POST` /api/a013/ym-order/batch-unpost
- `GET` /api/a013/ym-order/list
- `POST` /api/a013/ym-order/post-period

### `/a014`
- `POST` /api/a014/ozon-transactions/:id/post
- `GET` /api/a014/ozon-transactions/:id/projections
- `POST` /api/a014/ozon-transactions/:id/unpost

### `/a015`
- `GET` /api/a015/raw/:ref_id
- `GET` /api/a015/wb-orders
- `GET` /api/a015/wb-orders/:id
- `POST` /api/a015/wb-orders/:id/delete
- `POST` /api/a015/wb-orders/:id/post
- `GET` /api/a015/wb-orders/:id/projections
- `POST` /api/a015/wb-orders/:id/unpost
- `GET` /api/a015/wb-orders/search-by-srid

### `/a016`
- `GET` /api/a016/raw/:ref_id
- `GET` /api/a016/ym-returns
- `GET` /api/a016/ym-returns/:id
- `POST` /api/a016/ym-returns/:id/post
- `GET` /api/a016/ym-returns/:id/projections
- `POST` /api/a016/ym-returns/:id/unpost
- `POST` /api/a016/ym-returns/batch-post
- `POST` /api/a016/ym-returns/batch-unpost
- `POST` /api/a016/ym-returns/post-period
- `GET` /api/a016/ym-returns/source-order/:order_no

### `/a017-llm-agent`
- `GET POST` /api/a017-llm-agent
- `GET DELETE` /api/a017-llm-agent/:id
- `POST` /api/a017-llm-agent/:id/fetch-models
- `POST` /api/a017-llm-agent/:id/test
- `GET` /api/a017-llm-agent/list
- `GET` /api/a017-llm-agent/primary
- `GET` /api/a017-llm-agent/skills

### `/a018-llm-chat`
- `GET POST` /api/a018-llm-chat
- `GET DELETE` /api/a018-llm-chat/:chat_id/attachments/:attachment_id
- `GET DELETE` /api/a018-llm-chat/:id
- `GET POST` /api/a018-llm-chat/:id/context
- `GET POST` /api/a018-llm-chat/:id/messages
- `POST` /api/a018-llm-chat/:id/model
- `POST` /api/a018-llm-chat/:id/rating
- `POST` /api/a018-llm-chat/:id/shared
- `POST` /api/a018-llm-chat/:id/upload
- `GET` /api/a018-llm-chat/:id/workspace
- `POST` /api/a018-llm-chat/:id/workspace/active
- `POST` /api/a018-llm-chat/:id/workspace/answer
- `GET PUT` /api/a018-llm-chat/:id/workspace/file/*path
- `GET` /api/a018-llm-chat/jobs/:job_id
- `POST` /api/a018-llm-chat/jobs/:job_id/cancel
- `GET` /api/a018-llm-chat/jobs/:job_id/stream
- `GET` /api/a018-llm-chat/list
- `GET` /api/a018-llm-chat/message/:message_id/tool-trace
- `GET` /api/a018-llm-chat/with-stats

### `/a018-llm-chat-context`
- `GET` /api/a018-llm-chat-context/:id

### `/a019-llm-artifact`
- `GET POST` /api/a019-llm-artifact
- `GET DELETE` /api/a019-llm-artifact/:id
- `GET` /api/a019-llm-artifact/chat/:chat_id
- `GET` /api/a019-llm-artifact/list

### `/a020`
- `GET` /api/a020/raw/:ref_id
- `GET` /api/a020/wb-promotions
- `GET` /api/a020/wb-promotions/:id
- `POST` /api/a020/wb-promotions/:id/post
- `POST` /api/a020/wb-promotions/:id/unpost

### `/a021`
- `GET` /api/a021/production-output/:id
- `POST` /api/a021/production-output/:id/post
- `POST` /api/a021/production-output/:id/unpost
- `GET` /api/a021/production-output/list

### `/a022`
- `GET` /api/a022/kit-variant/:id
- `GET` /api/a022/kit-variant/list

### `/a023`
- `GET` /api/a023/purchase-of-goods/:id
- `POST` /api/a023/purchase-of-goods/:id/post
- `POST` /api/a023/purchase-of-goods/:id/unpost
- `GET` /api/a023/purchase-of-goods/list

### `/a024-bi-indicator`
- `GET POST` /api/a024-bi-indicator
- `GET DELETE` /api/a024-bi-indicator/:id
- `POST` /api/a024-bi-indicator/:id/compute
- `GET` /api/a024-bi-indicator/:id/drilldown
- `POST` /api/a024-bi-indicator/compute-batch
- `POST` /api/a024-bi-indicator/generate-view
- `GET` /api/a024-bi-indicator/list
- `GET` /api/a024-bi-indicator/owner/:user_id
- `GET` /api/a024-bi-indicator/public
- `POST` /api/a024-bi-indicator/resolve-batch
- `POST` /api/a024-bi-indicator/testdata
- `POST` /api/a024-bi-indicator/upsert

### `/a025-bi-dashboard`
- `GET POST` /api/a025-bi-dashboard
- `GET DELETE` /api/a025-bi-dashboard/:id
- `GET` /api/a025-bi-dashboard/list
- `GET` /api/a025-bi-dashboard/owner/:user_id
- `GET` /api/a025-bi-dashboard/public
- `POST` /api/a025-bi-dashboard/testdata
- `POST` /api/a025-bi-dashboard/upsert

### `/a026`
- `GET` /api/a026/wb-advert-daily/:id
- `GET` /api/a026/wb-advert-daily/:id/journal
- `POST` /api/a026/wb-advert-daily/:id/post
- `GET` /api/a026/wb-advert-daily/:id/projections
- `POST` /api/a026/wb-advert-daily/:id/unpost
- `GET` /api/a026/wb-advert-daily/list
- `GET` /api/a026/wb-advert-daily/report.csv

### `/a027`
- `GET` /api/a027/wb-documents/:id
- `GET` /api/a027/wb-documents/:id/download/:extension
- `POST` /api/a027/wb-documents/:id/extract-weekly-report
- `PUT` /api/a027/wb-documents/:id/manual
- `POST` /api/a027/wb-documents/:id/post
- `GET` /api/a027/wb-documents/list

### `/a028`
- `GET PUT` /api/a028/missing-cost-registry/:id
- `POST` /api/a028/missing-cost-registry/:id/post
- `POST` /api/a028/missing-cost-registry/:id/unpost
- `GET` /api/a028/missing-cost-registry/list

### `/a029`
- `GET` /api/a029/raw/:ref_id
- `GET` /api/a029/wb-supply
- `GET` /api/a029/wb-supply/:id
- `POST` /api/a029/wb-supply/:id/delete
- `GET` /api/a029/wb-supply/:id/orders
- `GET` /api/a029/wb-supply/:id/stickers
- `GET` /api/a029/wb-supply/by-order/:order_id
- `GET` /api/a029/wb-supply/by-wb-id/:wb_id

### `/a030`
- `GET` /api/a030/wb-advert-campaign/:id
- `GET` /api/a030/wb-advert-campaign/:id/advert-stats
- `GET` /api/a030/wb-advert-campaign/:id/nm-positions
- `GET` /api/a030/wb-advert-campaign/list

### `/a031-kb-edit`
- `GET POST` /api/a031-kb-edit
- `GET PUT DELETE` /api/a031-kb-edit/:id
- `POST` /api/a031-kb-edit/:id/approve
- `POST` /api/a031-kb-edit/:id/cancel
- `GET` /api/a031-kb-edit/list

### `/a032`
- `GET` /api/a032/wb-returns-claims
- `GET` /api/a032/wb-returns-claims/:id

### `/a033`
- `GET POST` /api/a033/wb-day-close
- `GET` /api/a033/wb-day-close/:id
- `GET` /api/a033/wb-day-close/:id/advert-live
- `POST` /api/a033/wb-day-close/:id/archive-and-recreate
- `POST` /api/a033/wb-day-close/:id/recalculate
- `POST` /api/a033/wb-day-close/:id/repost-problematic-a012
- `GET` /api/a033/wb-day-close/by-day/:connection_id/:business_date
- `POST` /api/a033/wb-day-close/compare

### `/a034`
- `GET` /api/a034/ym-realization/:id
- `GET` /api/a034/ym-realization/:id/delivery-orders
- `POST` /api/a034/ym-realization/:id/fetch-missing-orders
- `GET` /api/a034/ym-realization/:id/journal
- `GET` /api/a034/ym-realization/:id/payment-detail
- `POST` /api/a034/ym-realization/:id/post
- `GET` /api/a034/ym-realization/:id/reconciliation-returns
- `GET` /api/a034/ym-realization/:id/reconciliation-sales
- `GET` /api/a034/ym-realization/:id/reconciliation-summary
- `POST` /api/a034/ym-realization/:id/unpost
- `GET` /api/a034/ym-realization/list

### `/a035`
- `GET` /api/a035/ym-settlement-recon/:id
- `POST` /api/a035/ym-settlement-recon/:id/post
- `POST` /api/a035/ym-settlement-recon/:id/recompute
- `POST` /api/a035/ym-settlement-recon/:id/unpost
- `POST` /api/a035/ym-settlement-recon/generate
- `GET` /api/a035/ym-settlement-recon/list

### `/a036`
- `GET` /api/a036/wb-sales-funnel/:id
- `POST` /api/a036/wb-sales-funnel/:id/post
- `GET` /api/a036/wb-sales-funnel/:id/projections
- `GET` /api/a036/wb-sales-funnel/export-lines
- `GET` /api/a036/wb-sales-funnel/list
- `GET` /api/a036/wb-sales-funnel/product-metrics
- `POST` /api/a036/wb-sales-funnel/rebuild-funnel-projection

### `/a037`
- `GET` /api/a037/wb-product-snapshot/:id
- `GET` /api/a037/wb-product-snapshot/list
- `GET` /api/a037/wb-product-snapshot/rating-changes
- `GET` /api/a037/wb-product-snapshot/series

### `/a038-llm-connection`
- `GET POST` /api/a038-llm-connection
- `GET DELETE` /api/a038-llm-connection/:id
- `POST` /api/a038-llm-connection/:id/fetch-models
- `POST` /api/a038-llm-connection/:id/test
- `GET` /api/a038-llm-connection/list
- `GET` /api/a038-llm-connection/primary

### `/a039-mail-message`
- `GET` /api/a039-mail-message
- `GET DELETE` /api/a039-mail-message/:id
- `GET` /api/a039-mail-message/list

### `/a040`
- `GET` /api/a040/wb-search-analytics/:id
- `GET` /api/a040/wb-search-analytics/list

### `/a041`
- `GET` /api/a041/ym-shows-sales/:id
- `GET` /api/a041/ym-shows-sales/list

### `/a042-agent-task`
- `GET` /api/a042-agent-task
- `GET DELETE` /api/a042-agent-task/:id
- `POST` /api/a042-agent-task/:id/cancel
- `POST` /api/a042-agent-task/:id/requeue
- `GET` /api/a042-agent-task/list

### `/a043`
- `GET` /api/a043/wb-finance-reports/:id
- `GET` /api/a043/wb-finance-reports/:id/lines
- `GET` /api/a043/wb-finance-reports/list

### `/bi-timeline`
- `GET` /api/bi-timeline/indicators
- `POST` /api/bi-timeline/series

### `/connection_1c`
- `GET POST` /api/connection_1c
- `GET DELETE` /api/connection_1c/:id
- `GET` /api/connection_1c/list
- `POST` /api/connection_1c/test
- `POST` /api/connection_1c/testdata

### `/connection_mp`
- `GET POST` /api/connection_mp
- `GET DELETE` /api/connection_mp/:id
- `POST` /api/connection_mp/seller_info
- `POST` /api/connection_mp/test

### `/counterparty`
- `GET POST` /api/counterparty
- `GET DELETE` /api/counterparty/:id

### `/d400`
- `GET` /api/d400/monthly_summary
- `GET` /api/d400/periods

### `/d401`
- `GET POST` /api/d401/configs
- `GET PUT DELETE` /api/d401/configs/:id
- `POST` /api/d401/execute
- `POST` /api/d401/generate-sql
- `GET` /api/d401/schemas
- `GET` /api/d401/schemas/:id
- `GET` /api/d401/schemas/:schema_id/fields/:field_id/values

### `/dashboards`
- `GET POST` /api/dashboards/d402/configs
- `GET PUT DELETE` /api/dashboards/d402/configs/:id
- `POST` /api/dashboards/d402/execute
- `POST` /api/dashboards/d402/generate-sql
- `GET` /api/dashboards/d402/schemas
- `GET` /api/dashboards/d402/schemas/:id
- `GET` /api/dashboards/d402/schemas/:schema_id/fields/:field_id/values
- `GET` /api/dashboards/wb-advert-report
- `GET` /api/dashboards/wb-order-flow
- `GET` /api/dashboards/wb-sales-funnel
- `GET` /api/dashboards/wb-sales-funnel/orders
- `GET` /api/dashboards/ym-order-flow

### `/data-view`
- `GET` /api/data-view
- `GET` /api/data-view/:id
- `POST` /api/data-view/:id/compute
- `POST` /api/data-view/:id/drilldown
- `POST` /api/data-view/:id/drilldown-capabilities
- `GET` /api/data-view/:id/filters
- `GET` /api/data-view/filters

### `/debug`
- `GET` /api/debug/tool-test

### `/drilldown`
- `POST` /api/drilldown/execute

### `/ds01`
- `GET POST` /api/ds01/configs
- `GET PUT DELETE` /api/ds01/configs/:id
- `POST` /api/ds01/execute
- `POST` /api/ds01/generate-sql
- `GET` /api/ds01/schemas
- `GET` /api/ds01/schemas/:id
- `GET` /api/ds01/schemas/:schema_id/fields/:field_id/values

### `/ds02`
- `GET POST` /api/ds02/configs
- `GET PUT DELETE` /api/ds02/configs/:id
- `POST` /api/ds02/execute
- `POST` /api/ds02/generate-sql
- `GET` /api/ds02/schemas
- `GET` /api/ds02/schemas/:id
- `GET` /api/ds02/schemas/:schema_id/fields/:field_id/values

### `/ext`
- `GET` /api/ext/v1/docs
- `GET` /api/ext/v1/openapi.json
- `GET` /api/ext/v1/wb-advert-daily
- `GET` /api/ext/v1/wb-finance-report
- `GET` /api/ext/v1/wb-sales-funnel
- `GET` /api/ext/v1/wb-stocks
- `GET` /api/ext/v1/wb-supplies
- `GET` /api/ext/v1/wb-supplies/:id
- `GET` /api/ext/v1/ym-payment-report
- `GET` /api/ext/v1/ym-sales-funnel

### `/general-ledger`
- `GET` /api/general-ledger
- `GET` /api/general-ledger/:id
- `GET` /api/general-ledger/:id/resource-details
- `POST` /api/general-ledger/account-view
- `GET` /api/general-ledger/dimensions
- `POST` /api/general-ledger/drilldown
- `GET` /api/general-ledger/drilldown/:id
- `GET` /api/general-ledger/drilldown/:id/data
- `GET` /api/general-ledger/entities
- `GET` /api/general-ledger/layers
- `GET` /api/general-ledger/layer-turnover-matrix
- `POST` /api/general-ledger/report
- `GET` /api/general-ledger/report/dimensions
- `POST` /api/general-ledger/report/drilldown
- `POST` /api/general-ledger/supplier-balance
- `GET` /api/general-ledger/turnovers
- `GET` /api/general-ledger/turnovers/:code

### `/kb`
- `GET` /api/kb/articles/:id
- `POST` /api/kb/generate
- `GET` /api/kb/issues
- `POST` /api/kb/reload
- `GET` /api/kb/stats
- `GET` /api/kb/tree
- `GET` /api/kb/vocabulary

### `/knowledge`
- `GET` /api/knowledge/inventory
- `POST` /api/knowledge/inventory/collect
- `GET` /api/knowledge/inventory/history
- `GET` /api/knowledge/inventory/surfaces
- `GET` /api/knowledge/inventory/unit/:id

### `/llm-knowledge`
- `GET` /api/llm-knowledge
- `GET` /api/llm-knowledge/:id

### `/llm-quality`
- `GET` /api/llm-quality/overview

### `/llm-skills`
- `GET` /api/llm-skills
- `GET PUT` /api/llm-skills/access-matrix
- `POST` /api/llm-skills/reload

### `/llm-tools`
- `GET` /api/llm-tools

### `/marketplace`
- `GET POST` /api/marketplace
- `GET DELETE` /api/marketplace/:id
- `POST` /api/marketplace/testdata

### `/marketplace_product`
- `GET POST` /api/marketplace_product
- `GET DELETE` /api/marketplace_product/:id
- `POST` /api/marketplace_product/testdata

### `/marketplace_sales`
- `GET POST` /api/marketplace_sales
- `GET DELETE` /api/marketplace_sales/:id

### `/nomenclature`
- `GET POST` /api/nomenclature
- `GET DELETE` /api/nomenclature/:id
- `GET` /api/nomenclature/:id/orders
- `GET` /api/nomenclature/dimensions
- `POST` /api/nomenclature/import-excel
- `GET` /api/nomenclature/search
- `GET` /api/nomenclature/search-by-barcode

### `/organization`
- `GET POST` /api/organization
- `GET DELETE` /api/organization/:id
- `POST` /api/organization/testdata

### `/ozon_returns`
- `GET POST` /api/ozon_returns
- `GET DELETE` /api/ozon_returns/:id

### `/ozon_transactions`
- `GET` /api/ozon_transactions
- `GET DELETE` /api/ozon_transactions/:id
- `GET` /api/ozon_transactions/by-posting/:posting_number

### `/p900`
- `POST` /api/p900/backfill-product-refs
- `GET` /api/p900/sales-register
- `GET` /api/p900/sales-register/:marketplace/:document_no/:line_id
- `GET` /api/p900/stats/by-date
- `GET` /api/p900/stats/by-marketplace

### `/p901`
- `GET` /api/p901/barcode/:barcode
- `GET` /api/p901/barcodes
- `GET` /api/p901/nomenclature/:nomenclature_ref/barcodes

### `/p902`
- `GET` /api/p902/finance-realization
- `GET` /api/p902/finance-realization/:posting_number/:sku/:operation_type
- `GET` /api/p902/stats

### `/p903`
- `GET` /api/p903/finance-report
- `GET` /api/p903/finance-report/by-id/:id
- `POST` /api/p903/finance-report/by-id/:id/post
- `GET` /api/p903/finance-report/by-id/:id/raw
- `GET` /api/p903/finance-report/export
- `GET` /api/p903/finance-report/operation-kinds
- `GET` /api/p903/finance-report/search-by-srid

### `/p904`
- `GET` /api/p904/sales-data

### `/p905-commission`
- `POST` /api/p905-commission
- `GET PUT DELETE` /api/p905-commission/:id
- `GET` /api/p905-commission/list
- `POST` /api/p905-commission/sync

### `/p906`
- `POST` /api/p906/import-excel
- `GET` /api/p906/nomenclature-prices
- `GET` /api/p906/periods

### `/p907`
- `GET` /api/p907/payment-report
- `GET` /api/p907/payment-report/:id
- `GET` /api/p907/payment-report/:id/finance-turnovers
- `POST` /api/p907/payment-report/:id/post
- `GET` /api/p907/payment-report/filter-options
- `POST` /api/p907/payment-report/migrate-keys
- `POST` /api/p907/payment-report/repost-all

### `/p908`
- `GET` /api/p908/goods-prices
- `GET` /api/p908/goods-prices/:nm_id

### `/p912`
- `GET` /api/p912/nomenclature-costs

### `/p913`
- `GET` /api/p913/wb-advert-order-attr

### `/p914`
- `GET` /api/p914/mp-finance-turnovers

### `/p915`
- `GET` /api/p915/order-events
- `GET` /api/p915/order-events/by-order/:order_id

### `/plugin`
- `POST` /api/plugin
- `GET` /api/plugin
- `DELETE` /api/plugin/:id
- `GET` /api/plugin/:id
- `POST` /api/plugin/:id/apply-update
- `POST` /api/plugin/:id/data
- `POST` /api/plugin/:id/dev-invoke
- `GET` /api/plugin/:id/export
- `POST` /api/plugin/:id/invoke
- `POST` /api/plugin/:id/publish
- `POST` /api/plugin/:id/rating
- `GET` /api/plugin/:id/stats
- `GET` /api/plugin/all
- `GET` /api/plugin/catalog
- `POST` /api/plugin/catalog/:code/install
- `POST` /api/plugin/import
- `GET` /api/plugin/migration-version
- `GET` /api/plugin/runs/summary
- `POST` /api/plugin/smoke-test
- `POST` /api/plugin/testdata
- `GET` /api/plugin/updates
- `POST` /api/plugin/validate

### `/processes`
- `GET` /api/processes/actions
- `GET POST` /api/processes/definitions
- `POST` /api/processes/definitions/:code/deactivate
- `GET` /api/processes/definitions/:code/versions
- `GET DELETE` /api/processes/definitions/:code/versions/:version
- `POST` /api/processes/definitions/:code/versions/:version/activate
- `GET` /api/processes/definitions/:code/versions/:version/activation-plan
- `GET` /api/processes/definitions/full
- `GET` /api/processes/effects
- `GET` /api/processes/event-kinds
- `GET` /api/processes/events
- `GET` /api/processes/instances
- `GET` /api/processes/instances/:id
- `POST` /api/processes/instances/:id/human-done
- `GET POST` /api/processes/stages
- `GET` /api/processes/stages/:code/versions
- `GET DELETE` /api/processes/stages/:code/versions/:version
- `POST` /api/processes/stages/:code/versions/:version/activate
- `POST` /api/processes/stages/:code/versions/:version/dry-run
- `GET` /api/processes/stages/full
- `POST` /api/processes/tick

### `/projections`
- `GET` /api/projections/p900/:registrator_ref

### `/quality`
- `GET` /api/quality/checks
- `POST` /api/quality/checks/:id/cleanup
- `GET` /api/quality/checks/:id/details
- `GET` /api/quality/checks/:id/groups
- `POST` /api/quality/checks/:id/repost
- `GET` /api/quality/checks/:id/rows
- `POST` /api/quality/checks/:id/run
- `GET` /api/quality/checks/:id/runs
- `GET` /api/quality/checks/:id/sources
- `GET` /api/quality/checks/overview
- `POST` /api/quality/checks/reload

### `/refs`
- `GET` /api/refs/resolve

### `/reports`
- `GET` /api/reports/wb-weekly-reconciliation
- `GET` /api/reports/ym-revenue-reconciliation

### `/sys-drilldown`
- `POST` /api/sys-drilldown
- `GET` /api/sys-drilldown/:id
- `GET` /api/sys-drilldown/:id/data

### `/u501`
- `GET` /api/u501/import/:session_id/progress
- `POST` /api/u501/import/start

### `/u502`
- `GET` /api/u502/import/:session_id/progress
- `POST` /api/u502/import/start

### `/u503`
- `GET` /api/u503/import/:session_id/progress
- `POST` /api/u503/import/start

### `/u504`
- `GET` /api/u504/import/:session_id/progress
- `POST` /api/u504/import/start

### `/u505`
- `GET` /api/u505/match/:session_id/progress
- `POST` /api/u505/match/start

### `/u506`
- `GET` /api/u506/import/:session_id/progress
- `POST` /api/u506/import/start

### `/u507`
- `GET` /api/u507/import/:session_id/progress
- `POST` /api/u507/import/start

### `/u508`
- `GET` /api/u508/repost/:session_id/progress
- `POST` /api/u508/repost/aggregate/start
- `GET` /api/u508/repost/aggregates
- `GET` /api/u508/repost/funnel/diagnostics
- `POST` /api/u508/repost/funnel/start
- `GET` /api/u508/repost/projections
- `POST` /api/u508/repost/start

### `/universal-dashboard`
- `GET POST` /api/universal-dashboard/configs
- `GET PUT DELETE` /api/universal-dashboard/configs/:id
- `POST` /api/universal-dashboard/execute
- `POST` /api/universal-dashboard/generate-sql
- `GET` /api/universal-dashboard/schemas
- `GET` /api/universal-dashboard/schemas/:id
- `POST` /api/universal-dashboard/schemas/:id/validate
- `GET` /api/universal-dashboard/schemas/:schema_id/fields/:field_id/values
- `POST` /api/universal-dashboard/schemas/validate-all

### `/ym`
- `POST` /api/ym/consolidate-connections

