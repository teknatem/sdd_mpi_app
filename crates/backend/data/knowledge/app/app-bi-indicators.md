---
kind: app
title: BI Индикаторы и Дашборды (a024, a025)
tags: [a024, a025, bi, индикатор, дашборд, data-view, kpi]
related: [data-view, dv001_revenue, p904]
updated: 2026-04-19
---

# BI Индикаторы (a024_bi_indicator)

BI Индикатор — единица отображения данных на дашборде. Каждый индикатор:
- привязан к источнику данных (DataView + метрика) через `data_spec_json`
- имеет настройки отображения (HTML-шаблон, форматирование числа) через `view_spec_json`
- может иметь пороговые значения (зелёный / красный)
- может быть drill-down кликабельным

Актуальный список индикаторов — всегда получай из БД:
```sql
SELECT id, code, description, data_spec_json, status
FROM a024_bi_indicator
WHERE is_deleted = 0
ORDER BY description
```

## Структура DataSpec (источник данных)

```json
{
  "view_id": "dv001_revenue",
  "metric_id": "revenue"
}
```

Доступные `view_id` и `metric_id` — получай через инструмент `list_data_sources(kind="dataview")`.

## Создание индикатора через API

```
POST /api/a024-bi-indicator
```

**Минимальный рабочий запрос:**
```json
{
  "description": "Выручка за период",
  "owner_user_id": "<UUID пользователя>",
  "data_spec": {
    "schema_id": "",
    "query_config": {
      "data_source": "", "selected_fields": [], "groupings": [],
      "filters": {}, "display_fields": [], "sort": {"field": "", "ascending": true},
      "enabled_fields": []
    },
    "view_id": "dv001_revenue",
    "metric_id": "revenue"
  },
  "params": [
    { "key": "date_from",      "param_type": "date", "label": "Начало периода",  "required": false, "global_filter_key": "date_from" },
    { "key": "date_to",        "param_type": "date", "label": "Конец периода",   "required": false, "global_filter_key": "date_to" },
    { "key": "connection_ids", "param_type": "ref",  "label": "Кабинеты МП",    "required": false, "global_filter_key": "connection_ids" }
  ],
  "view_spec": {
    "style_name": "custom",
    "custom_html": "<div class=\"kpi\"><div class=\"kpi__label\">{{title}}</div><div class=\"kpi__value\">{{value}}</div><div class=\"kpi__delta\">{{delta}}</div></div>",
    "format": { "kind": "Money", "currency": "RUB" },
    "thresholds": []
  },
  "status": "active",
  "is_public": true
}
```

## ViewSpec — отображение индикатора

### Переменные шаблона

| Переменная  | Значение                              |
|-------------|---------------------------------------|
| `{{value}}` | Текущее значение (форматированное)    |
| `{{delta}}` | Изменение к предыдущему периоду (±%)  |
| `{{title}}` | Название индикатора (description)     |

### Форматы числа (format.kind)

| kind       | Описание                      | Пример      |
|------------|-------------------------------|-------------|
| `Money`    | Денежная сумма с валютой      | 1 234 567 ₽ |
| `Integer`  | Целое число                   | 42 000      |
| `Percent`  | Процент с десятичными знаками | 23.5%       |

### Пороговые значения

```json
"thresholds": [
  { "condition": "> 25", "color": "rgb(34,197,94)", "label": "Высокая" },
  { "condition": "< 10", "color": "rgb(239,68,68)",  "label": "Низкая"  }
]
```

## Вычисление значения индикатора

```
POST /api/a024-bi-indicator/{id}/compute
{
  "date_from": "2026-01-01",
  "date_to":   "2026-01-31",
  "connection_mp_refs": []
}
```

# BI Дашборды (a025_bi_dashboard)

Дашборд компонует несколько индикаторов в сетку. 

Актуальный список дашбордов:
```sql
SELECT id, code, description, status
FROM a025_bi_dashboard
WHERE is_deleted = 0
ORDER BY description
```

## Связанные концепции

- **DataView** — источник данных для индикаторов (см. документ `data-view`)
- **p904_sales_data** — основная таблица сводных данных продаж
- **a024_bi_indicator** — таблица индикаторов в БД
- **a025_bi_dashboard** — таблица дашбордов в БД
