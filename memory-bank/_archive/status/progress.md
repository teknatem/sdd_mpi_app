# Progress Tracker

_Последнее обновление: 2026-05-11_

## ✅ Реализовано и работает

### Aggregates (Domain Entities)

- ✅ **a001_connection_1c** - Подключения к 1С:УТ11
  - CRUD операции
  - Тестирование подключения OData
  - Primary connection flag
- ✅ **a002_organization** - Организации
  - Импорт из 1С
  - Связь с маркетплейсами
  - CRUD операции
- ✅ **a003_counterparty** - Контрагенты
  - Импорт из 1С
  - Tree view в UI
  - Связь с организациями
- ✅ **a004_nomenclature** - Номенклатура
  - Полная структура с nullable полями
  - Импорт из 1С
  - Excel импорт
  - Tree view и picker
- ✅ **a005_marketplace** - Маркетплейсы
  - Справочник маркетплейсов (WB, Ozon, Yandex)
  - CRUD операции
  - Picker компонент
- ✅ **a006_connection_mp** - Подключения к маркетплейсам
  - Хранение токенов и ключей
  - Связь с маркетплейсами
  - Тестирование подключения
  - Для Яндекс.Маркет поддержаны `API Key` и `OAuth 2.0` заголовки; кнопка "Информация" показывает магазины/кабинет и API-Key token info
  - Thaw Table с сортировкой
- ✅ **a007_marketplace_product** - Продукты маркетплейсов
  - Товары на маркетплейсах
  - Связь с номенклатурой
- ✅ **a008_marketplace_sales** - Продажи маркетплейсов
  - Сводные данные по продажам
  - UI для просмотра
- ✅ **a009_ozon_returns** - Возвраты Ozon
  - Обработка возвратов Ozon
  - Posting функционал
- ✅ **a010_ozon_fbs_posting** - FBS отгрузки Ozon
  - Fulfillment by Seller
  - Posting функционал
- ✅ **a011_ozon_fbo_posting** - FBO отгрузки Ozon
  - Fulfillment by Ozon
  - Posting функционал
- ✅ **a012_wb_sales** - Продажи Wildberries
  - Данные по продажам WB
  - Posting функционал
  - State management
- ✅ **a013_ym_order** - Заказы Яндекс.Маркет
  - Обработка заказов ЯМ
  - Posting функционал
  - State management
- ✅ **a014_ozon_transactions** - Транзакции Ozon
  - Полная структура транзакций
  - UI для просмотра и фильтрации
  - Posting/Unposting функционал
  - State management
- ✅ **a015_wb_orders** - Заказы Wildberries
  - Импорт заказов
  - Детальная информация
  - UI для просмотра
  - Карточка заказа показывает связанную поставку WB из `a029_wb_supply` или "Поставка не найдена"
- ✅ **a016_ym_returns** - Возвраты Яндекс.Маркет
  - Обработка возвратов ЯМ
  - Posting функционал
  - State management
- ✅ **a017_llm_agent** - LLM-агенты
  - CRUD для агентов (model, system_prompt, температура)
  - UI список и детали
- ✅ **a018_llm_chat** - LLM-чаты
  - Чат с LLM через встроенный интерфейс
  - Tool calls: list_entities, get_entity_schema, get_join_hint, search_knowledge, get_knowledge, list_data_views
  - История сообщений, артефакты
- ✅ **a019_llm_artifact** - LLM-артефакты
  - Хранение артефактов чата (SQL-запросы и пр.)
- ✅ **a020_wb_promotion** - WB-продвижение
- ✅ **a030_wb_advert_campaign** - Рекламные кампании WB
  - Хранит `advert_id`, тип, статус, `changeTime`
  - Хранит сырые свойства кампаний WB Advert API в `info_json`
  - Обновляется регламентным заданием `task012_wb_advert_campaigns` ежедневно
  - `task012` соблюдает локальные паузы между WB Advert API запросами и останавливает дальнейшие `info_json` чанки при первом `429`
  - Используется `task011_wb_advert` как источник `advert_id` для загрузки `fullstats`
  - UI: список и детальная карточка с JSON свойств, пункт в сайдбаре `Справочники`
  - Акции Wildberries
  - Интеграция с WB Promotion API
- ✅ **a021_production_output** - Выпуск продукции
- ✅ **a022_kit_variant** - Варианты комплектов
- ✅ **a023_purchase_of_goods** - Закупки товаров
- ✅ **a024_bi_indicator** - BI Индикаторы
  - CRUD операции, пагинация
  - DataSpec с приоритетной цепочкой: view_id → data_source_config → schema_query → schema_id
  - ViewSpec: custom_html шаблоны с {{value}}/{{delta}}/{{title}}, форматы (Money/Integer/Percent), пороги
  - Вычисление через DataView (`compute_indicator`) и drilldown (`get_indicator_drilldown`)
  - 5 тестовых индикаторов (IND-REVENUE-WB, IND-MARGIN, IND-ORDERS и др.)
  - Метаданные в LLM MetadataRegistry (category: bi/dashboard)
- ✅ **a025_bi_dashboard** - BI Дашборды
  - CRUD операции
  - Метаданные в LLM MetadataRegistry (category: bi/dashboard)
- ✅ **a026_wb_advert_daily** - Реклама Wildberries по дням
  - Ежедневная статистика рекламных кампаний WB
  - UI список и детали
- ✅ **sys_scheduled_task** - Регламентные задания
  - Хранение расписаний (Cron) и параметров (JSON)
  - Статус последнего запуска и ссылка на сессию
  - CRUD операции в UI
  - Фоновый запуск по расписанию
  - `task013_ym_orders_polling`: адаптивный поллер заказов Яндекс.Маркета для `a013_ym_order`; seed-миграция создаёт две отключённые задачи для двух YM-кабинетов

### UseCases (Operations)

- ✅ **u501_import_from_ut** - Импорт из 1С:УТ11
  - OData client для 1С
  - Импорт организаций
  - Импорт номенклатуры
  - Импорт контрагентов
  - Progress tracking
  - UI виджет с мониторингом
  - ✅ Рефакторинг: вынос OData моделей, поддержка программного вызова
- ✅ **u502_import_from_ozon** - Импорт из Ozon
  - Ozon API client
  - Импорт транзакций
  - Импорт отгрузок (FBS/FBO)
  - Импорт возвратов
  - Pagination для больших датасетов
  - Progress tracking
  - ✅ Рефакторинг: поддержка программного вызова (Scheduled Tasks)
- ✅ **u503_import_from_yandex** - Импорт из Яндекс.Маркет
  - Yandex API client
  - Импорт заказов
  - Импорт возвратов
  - Progress tracking
  - ✅ Рефакторинг: поддержка программного вызова (Scheduled Tasks)
  - `task013_ym_orders_polling` запускает u503 в фоне только для `a013_ym_order` с FAST/EXTENDED окном по аналогии с WB `task001`
- ✅ **u504_import_from_wildberries** - Импорт из Wildberries
  - Wildberries API client
  - Импорт продаж
  - Импорт заказов
  - Импорт финансовых отчетов
  - Импорт истории комиссий
  - Pagination для больших датасетов
  - Diagnostic tools
  - ✅ Рефакторинг: поддержка программного вызова (Scheduled Tasks)
- ✅ **u505_match_nomenclature** - Сопоставление номенклатуры
  - Автоматическое сопоставление
  - Matching logic
  - Progress tracking
- ✅ **u506_import_from_lemanapro** - Импорт из LemanaPro
  - LemanaPro API client
  - Базовая структура
  - Progress tracking
- ✅ **u507_import_from_erp** - Импорт из ERP
- ✅ **u508_repost_documents** - Перепроведение документов

### Projections (Analytics)

- ✅ **p900_mp_sales_register** - Регистр продаж маркетплейсов
  - Consolidated sales data
  - Cross-marketplace view
  - Projection builder
  - Backfill функционал
- ✅ **p901_nomenclature_barcodes** - Штрих-коды номенклатуры
  - Связь номенклатуры со штрих-кодами
  - Импорт из 1С
  - UI для просмотра
- ✅ **p902_ozon_finance_realization** - Финансовая реализация Ozon
  - Финансовые данные Ozon
  - UI для просмотра
- ✅ **p903_wb_finance_report** - Финансовый отчет Wildberries
  - ppvz_sales_commission поле
  - Детальные финансовые показатели WB
  - UI для просмотра
- ✅ **p904_sales_data** - Аналитика продаж
  - Период фильтрация
  - Cabinet фильтрация (с persistence)
  - Сортировка по всем полям
  - State management (state.rs)
  - Projection builder
  - Улучшенный UI
- ✅ **p905_wb_commission_history** - История комиссий Wildberries
  - Импорт данных
  - UI для просмотра
  - Детальная информация по кабинетам
- ✅ **p906_nomenclature_prices** - Цены номенклатуры
  - Импорт цен из 1С
  - История изменения цен
  - UI для просмотра
- ✅ **p907_ym_payment_report** - Финансовый отчёт Яндекс.Маркет
- ✅ **p908_wb_goods_prices** - Цены товаров Wildberries
- ✅ **p909_mp_order_line_turnovers** - Обороты по строкам заказов маркетплейсов
- ✅ **p910_mp_unlinked_turnovers** - Несвязанные обороты маркетплейсов
- ✅ **p911_wb_advert_by_items** - Реклама WB в разрезе товаров

### General Ledger (независимая система)

- ✅ **General Ledger** — бухгалтерский журнал проводок
  - Хранение проводок (entry_date, layer, debit/credit account, amount, qty, turnover_code, cabinet_mp)
  - Реестр видов оборотов (27 turnover codes) + реестр счетов (план счетов)
  - API: list с фильтрами (date, layer, account, turnover, cabinet_mp), get by id, list turnovers
  - Frontend: список с Select-фильтрами, детали записи, страница Обороты GL
  - Расположение: `crates/*/src/general_ledger/` (НЕ в domain/)

### DataView (аналитический слой)

- ✅ **dv001_revenue** — DataView выручки/продаж
  - 12 измерений с SQL-полями (DimensionMeta)
  - Самодостаточен (не зависит от SchemaRegistry)
  - Интегрирован с LLM (инструмент `list_data_views`)

### Roles & Permissions (sys_roles)

- ✅ **Система ролей** — управление доступом на основе ролей
  - Роли: CRUD + матрица прав (`sys_roles_matrix`)
  - Табы: `sys_roles`, `sys_roles_matrix`, `sys_role_details_{id}`
  - Frontend: `crates/frontend/src/system/roles/ui/`

### System Infrastructure

- ✅ **Field Metadata System**
  - Декларативное описание агрегатов в `metadata.json`
  - Rust types: `EntityMetadataInfo`, `FieldMetadata` с `'static` lifetimes
  - JSON Schema для валидации и IDE автодополнения
  - `build.rs` генератор: JSON → `metadata_gen.rs`
  - AggregateRoot trait extension: `entity_metadata_info()`, `field_metadata()`
  - AI контекст: description, questions, related для LLM чата
  - POC: `a001_connection_1c` агрегат полностью интегрирован

- ✅ **Scheduled Tasks System**
  - Task Manager / Executor паттерн
  - Background Worker (tokio loop)
  - File-based logging (GUID-based)
  - Registry для динамического поиска обработчиков
  - Backend API для мониторинга и управления
  - Frontend UI (список, детали, логи)

- ✅ **LLM Chat System (a017-a019)**
  - Встроенный чат-интерфейс для аналитики данных
  - Tool-calling инфраструктура: list_entities, get_entity_schema, get_join_hint, search_knowledge, get_knowledge
  - Агенты (a017) с настраиваемым system_prompt
  - История чатов (a018) и артефакты (a019)
  - System prompt: `crates/backend/src/domain/a018_llm_chat/prompts/default_agent.md`
- ✅ **Read-only Knowledge Base UI**
  - Backend API: `/api/kb/stats`, `/api/kb/tree`, `/api/kb/articles/:id`
  - Frontend: отдельный раздел “База знаний” со статистикой, деревом статей и просмотром markdown как безопасного текста
  - Внутренние ссылки `kb://article/{id}` открывают вкладку статьи из чата и KB-контента
  - Access scope: `knowledge_base`
  - KB разделена на два слоя: Obsidian (`data/knowledge/`) хранит только бизнес-знания организации, embedded-документы приложения хранят технический контекст (DataView, BI, projections, tool notes)


### Database

- ✅ **SQLite schema**
  - 40+ таблиц для aggregates, projections, system
  - Индексы для производительности
  - Soft delete support
- ✅ **Формальная система миграций (2026-02-18)**
  - `migrations/0001_baseline_schema.sql` — полная исходная схема
  - `migration_runner.rs` — автозапуск `sqlx::migrate::Migrator` при старте
  - Трекинг в `_sqlx_migrations`: версия, описание, checksum, дата
  - Поддержка fresh install и idempotent повторного запуска
  - Старые `migrate_*.sql` заархивированы в `migrations/archive/`

## 🔨 В процессе разработки

- 🔄 Field Metadata System — POC на a001, расширение на остальные агрегаты планируется
- 🔄 Новые DataView (dv002+)
- 🔄 Экспорт данных (CSV, Excel)

## 📋 Планируется (Backlog)

### High Priority

- [ ] Добавление метаданных для всех агрегатов (a002-a016)
- [ ] Интеграция метаданных с Frontend (автогенерация форм)
- [ ] Полная документация API endpoints
- [ ] Automated testing setup

### Medium Priority

- [ ] Оптимизация производительности при больших объемах
- [ ] Расширенная фильтрация и поиск
- [ ] Export функционал (CSV, Excel)
- [ ] Улучшенная error handling и user feedback

### Low Priority

- [ ] Дополнительные интеграции маркетплейсов
- [ ] Расширенная аналитика и dashboards
- [ ] User preferences и settings
- [ ] Локализация (если нужно)

## 🐛 Известные проблемы

### Critical

_Нет критических проблем на данный момент_

### Minor

- ⚠️ **Frontend hot reload**: Иногда требует полной перезагрузки страницы
- ⚠️ **Large datasets**: Pagination работает, но UI может тормозить на > 10k строк в таблице

