---
id: bi-authoring
title: BI и drilldown-отчёты
description: Создание drilldown-отчётов и работа с BI-индикаторами/дашбордами (a024/a025), DataView, метриками и измерениями.
intents: [bi_authoring]
tools: [list_entities, get_join_hint, list_data_sources, run_data_view_scalar, run_data_view_drilldown, execute_query, create_drilldown_report]
default_for: []
---

## Навык: BI и drilldown-отчёты (bi-authoring)

Создание аналитических срезов: drilldown-отчёты и понимание BI-индикаторов/дашбордов.

Инструменты: `list_data_sources("dataview")`, `run_data_view_scalar(...)`,
`run_data_view_drilldown(...)`, `create_drilldown_report(...)`, плюс интроспекция БД
(`list_entities`, `get_join_hint`, `execute_query`) и базовые из core.

### DataView и BI

- **DataView** — именованное бизнес-вычисление над таблицами (семантический слой). Актуальный список
  view_id, метрик и измерений — через `list_data_sources("dataview")`. Для ответа данными вызывай
  `run_data_view_scalar`/`run_data_view_drilldown`; не воспроизводи формулу метрики сырым SQL.
- **BI Индикатор (a024)** — KPI-виджет. Методология, структура JSON и API — в базе знаний:
  `search_knowledge(entities=["a024"])`. Актуальный список — в БД:
  `execute_query("SELECT id, code, description, status FROM a024_bi_indicator WHERE is_deleted=0", "BI индикаторы")`.
- **BI Дашборд (a025)** — набор индикаторов. Список — `SELECT … FROM a025_bi_dashboard WHERE is_deleted=0`.

НЕ отвечай про индикаторы/дашборды из параметрических знаний — данные в БД всегда актуальнее.

### create_drilldown_report

`create_drilldown_report(view_id, group_by, metric_id, date_from, date_to, description,
[connection_mp_refs], [params])` — создаёт отчёт; пользователь получит карточку с кнопкой открытия.

1. Сначала `list_data_sources("dataview")` — узнать доступные view_id, metric_id и group_by.
2. Всегда уточни период (date_from/date_to), метрику и измерение. Если период не указан — спроси.
3. `connection_mp_refs` — массив UUID из безопасной base-схемы `a006` через `query_data_schema`
   (пусто = все кабинеты); Raw SQL к таблице credentials запрещён.
4. `params` — доп. параметры DataView, например `{"layer":"fact","turnover_code":"mp_commission"}` для dv004.
