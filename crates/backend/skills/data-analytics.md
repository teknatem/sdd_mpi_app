---
id: data-analytics
title: Аналитика данных
description: SQL-аналитика по маркетплейсам: продажи, заказы, остатки, выручка, обороты GL, поиск UUID в справочниках.
intents: [data_query]
tools: [list_entities, get_join_hint, list_data_sources, query_data_schema, run_data_view_scalar, run_data_view_drilldown, execute_query, get_chart_of_accounts, list_gl_turnovers]
default_for: [business_analyst, general]
---

## Навык: аналитика данных (data-analytics)

Работа с данными маркетплейсов через SQL и семантический слой.

Инструменты: `list_data_sources([kind])`, `query_data_schema(...)`, `run_data_view_scalar(...)`,
`run_data_view_drilldown(...)`, `execute_query(sql, params, description)`, `list_entities([category])`,
`get_join_hint(from, to)`, `get_chart_of_accounts()`, `list_gl_turnovers([report_group])`
(+ базовые из core: `get_architecture_overview`, `get_entity_schema`, `search_knowledge`/`get_knowledge`).

### Источники данных: три роли (выбирай осознанно)

Доступ к аналитике — три независимых движка (подробно: ADR-0010):
- **DataView (`dvXX`)** — курируемые «виртуальные таблицы»: благословлённые метрики, сравнение
  **2 периодов**, кэш. Обнаруживай через `list_data_sources("dataview")`, читай данные через
  `run_data_view_scalar` или `run_data_view_drilldown`. Это **источник истины определений**
  (выручка, себестоимость и т.п.) — НЕ переизобретай их в сыром SQL.
- **Схемы таблиц (`dsXX`, kind=base)** — декларативное описание таблиц БД (поля/типы/связи) для
  гибкого ad-hoc (группировки/фильтры/агрегаты по «сырым» полям). В UI: «Схемы таблиц» (каталог) и
  «Конструктор запросов» (построитель). Обнаруживай через `list_data_sources("base")`, читай через
  `query_data_schema`. Сюда входят безопасные metadata-проекции справочников, например `a006`
  без API-ключей.
- **Сырой SQL (`execute_query`)** — только fallback для нестандартного и разового; один SELECT/WITH,
  bind-параметры через `params`. `a006_connection_mp` доступен (можно JOIN по `marketplace`/кабинету);
  `a001_connection_1c` остаётся недоступной.

Дерево выбора: нужен благословлённый показатель / 2 периода / составная метрика → DataView; нужен
произвольный разрез одной таблицы → схема/SQL; остальное → сырой SQL. Если одна таблица доступна и как
схема, и как DataView (напр. `p904`: ds03 гибкий, dv001 курируемый) — для официальных цифр бери DataView.

Воронка продаж WB (показы/переходы/корзина/заказы/выкуп, конверсии, отмены/возвраты, разрезы по товарам
и кампаниям) — авторитет **не этот навык, а `marketplace-funnel-analysis`**. Для ЛЮБОГО WB-вопроса про
воронку, процент выкупа, отмены/возвраты или топ товаров по кабинету активируй
`use_skill("marketplace-funnel-analysis")`: он заземлён на `p916` с корректной когортной осью, каналами
paid/free и защитой от типичных ошибок (`funnel_order_count` ≠ `order_count`; заниженный `order→buyout`
в незавершённом месяце из-за лага доставки). Не считай WB-воронку через сырой SQL/DataView здесь.

Если всё же читаешь напрямую DataView `dv008_wb_sales_funnel` (напр. страница плагина «Воронка продаж WB»
уже прикреплена и нужен именно её показатель):
- **всегда** передавай `connection_mp_refs=[<uuid кабинета>]` — без фильтра DataView суммирует ВСЕ
  WB-кабинеты (частая ошибка: товары чужого кабинета попадают в топ);
- `order_count`/`buyout_count`/`buyout_pct` в dv008 — фактические (a015/a012), это не маркетинговый
  счётчик воронки (`funnel_order_count`);
- вызов: `run_data_view_drilldown(view_id="dv008_wb_sales_funnel", group_by="nm_id"|"date"|
  "connection_mp_ref", metric_ids=[...], connection_mp_refs=[...])` или `run_data_view_scalar` для сводной
  цифры. Метрики: `open_count, cart_count, order_count, order_sum, buyout_count, buyout_sum, cart_conv_pct,
  order_conv_pct, buyout_pct`. Не переизобретай json_each по `lines_json` — разбор уже в dv008.

### Правила работы с SQL

1. Для получения аналитических строк сначала используй `list_data_sources`, DataView и base-схемы.
   `get_entity_schema`/`list_entities` нужны для нестандартного Raw SQL.
   Если индекс неизвестен — `list_entities` с нужным category (wb/ozon/ym/ref/llm/bi), не без фильтра.
2. Имена таблиц и колонок должны ТОЧНО совпадать со схемой. Только SELECT (INSERT/UPDATE/DELETE запрещены).
3. Поля base-схемы (напр. `dim1` = категория, `marketplace`) — это ИЗМЕРЕНИЯ схемы, а НЕ колонки
   таблицы: они джойнятся из справочников. В `query_data_schema` используй их как `group_by`/`filters`.
   Если нужен сырой SQL — НЕ выдумывай такие колонки, а возьми готовый `generated_sql` из ответа
   `query_data_schema` (там реальные JOIN-ы и колонки) и адаптируй его.
4. Пиши SQL в блоках ```sql … ```. Давай краткое объяснение результата (2–3 предложения).
5. Если вопрос касается бизнес-метрик/терминов/методологии — вызови `search_knowledge`.
   Про конкретный объект — `search_knowledge(entities=["p904"])`; те же документы приходят
   полем `docs` в `get_entity_schema`.
6. Логические поля агрегата не всегда являются колонками таблицы (часть лежит в JSON-блобах).
   Смотри ответ `get_entity_schema`: `columns_for_sql` — реальные колонки, `json_fields` — готовые
   `json_extract(...)`-выражения. Не угадывай имя колонки: угаданное `connection_id`/`order_dt`
   даёт `no such column`.
7. Прежде чем объяснять пустой результат, сверься с блоком `data_profile` в `get_entity_schema`:
   там строки, период данных и незаполненные ссылки. `date_note` означает, что дата документа
   лежит в JSON — период по таблице не измерялся, отбирай через `json_extract`.

### Проверка перед выводом (обязательно)

Вывод о ПОЛНОТЕ и НАЛИЧИИ данных — самая дорогая ошибка: пользователь по нему решает,
перезапускать ли загрузку. Прежде чем написать «данных нет», «пропусков нет», «всё загружено»:

1. **`truncated: true` = выборка неполная.** По ней нельзя судить ни о полноте, ни о пропусках, ни о
   крайних датах. Замени построчный SELECT на агрегат (`COUNT/MIN/MAX/COUNT(DISTINCT …)`).
2. **«Данных нет» проверяется вторым запросом БЕЗ фильтров** — `SELECT MIN(дата), MAX(дата), COUNT(*)
   … GROUP BY <кабинет>`. Пустой результат с фильтрами означает лишь то, что не подошли фильтры
   (частый случай — фильтр по колонке, которой нет, или дата в другом формате).
3. **Число групп сверяй с ожидаемым.** Запрашивал 2 кабинета, а `GROUP BY` вернул 1 строку — это не
   «у второго нет данных», а сломанная группировка. В `UNION ALL` не пиши `GROUP BY 1`: порядковый
   номер укажет на первую колонку (часто это константа-метка), и обе группы схлопнутся в одну.
4. **Сверяйся с собственными прошлыми ответами в этом чате.** Если новая цифра противоречит
   названной ранее — не публикуй молча: перепроверь и напиши, какая из них верна и почему.
5. В итоге разделяй **проверено** (есть запрос и результат) и **предположение** (доменное рассуждение,
   напр. «выкупы ещё дозревают»). Не выдавай второе за первое и не пиши «идеально/всё в порядке»,
   если часть проверок не сделана.

### Термины → сущности (глоссарий)

- **товар / номенклатура / позиция / SKU / артикул / карточка** без уточнения площадки → всегда
  `a004_nomenclature` (справочник 1С:УТ; товары при `is_folder = 0`, категории-папки при `is_folder = 1`).
- **позиция/карточка на конкретном маркетплейсе** (nmId WB, offerId/shop_sku YM) → `a007_marketplace_product`;
  связь: `a004_nomenclature.id = a007_marketplace_product.nomenclature_ref`.
- Сомневаешься в терминах/связях сущности — `get_entity_schema("a004")` (там синонимы, поля и путь к МП).

### Известные схемы (без get_entity_schema)

- `a006_connection_mp`: id (UUID), code, description (магазин), `marketplace` (FK→a005, UUID; именно
  `marketplace`, не `marketplace_id`), organization_ref (FK→a002), is_used (0/1), planned_commission_percent.
  Для WB: `WHERE marketplace = (SELECT id FROM a005_marketplace WHERE code = 'mp-wb')`.
- `a005_marketplace`: id, code (mp-wb/mp-ozon/mp-ym), description.
- `a002_organization`: id, code, description.

### Поиск UUID в справочниках

а) `list_data_sources("base")` — найди безопасную схему `a006`;
б) `query_data_schema` с `fields=["id","code","description"]` и фильтром `is_used = 1`;
в) из `rows` возьми нужные id (UUID). Raw SQL к `a006_connection_mp` теперь разрешён — можно
   джойнить его напрямую (напр. по `marketplace`), не вытаскивая id отдельно.

### General Ledger

Перед SQL к `sys_general_ledger` вызови `list_gl_turnovers` (точные turnover_code) и при необходимости
`get_chart_of_accounts` (план счетов, что дебетуется/кредитуется: 7609/76YA/9001/9002 и т.д.).

Если вопрос про BI-индикаторы/дашборды или нужно СОЗДАТЬ drilldown-отчёт — активируй навык `bi-authoring`.
