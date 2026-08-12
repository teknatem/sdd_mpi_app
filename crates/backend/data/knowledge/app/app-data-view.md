---
kind: app
title: DataView — семантический слой аналитики
tags: [data-view, dv001, bi, аналитика, метрики, drilldown, dv001_revenue]
related: [a024, a025, p904]
updated: 2026-03-12
---

# DataView — семантический слой аналитики

DataView — именованное бизнес-вычисление, которое инкапсулирует:
- источник данных (SQL-таблица)
- логику агрегации (метрики)
- доступные срезы (измерения для drill-down)
- период сравнения (P1 vs P2)

DataView вызывается через `POST /api/data-view/{id}/compute` или `POST /api/data-view/{id}/drilldown`.

## Список доступных DataView

### dv001_revenue — Продажи (2 периода)

**Источник:** `p904_sales_data`

Вычисляет финансовые метрики за выбранный период с автоматическим сравнением с предыдущим.
Возвращает скалярное значение + sparkline по дням.

**Как использовать при создании BI-индикатора:**
- `view_id`: `"dv001_revenue"`
- `metric_id`: одно из значений ниже

### Доступные метрики (metric_id)

| metric_id    | Название          | Формула SQL                                          |
|--------------|-------------------|------------------------------------------------------|
| `revenue`    | Выручка           | `customer_in + customer_out`                         |
| `cost`       | Себестоимость     | `cost`                                               |
| `commission` | Комиссия МП       | `commission_out`                                     |
| `expenses`   | Расходы           | `acquiring_out + penalty_out + logistics_out`        |
| `profit`     | Прибыль продавца  | `-seller_out`                                        |

### Доступные измерения для drill-down (group_by)

| id                  | Название              | Описание                                      |
|---------------------|-----------------------|-----------------------------------------------|
| `date`              | По дням               | Группировка по дате YYYY-MM-DD                |
| `article`           | По артикулу           | Артикул продавца                              |
| `registrator_type`  | По типу операции      | Продажа, возврат и т.д.                       |
| `connection_mp_ref` | По кабинету МП        | JOIN → a006_connection_mp.description         |
| `marketplace`       | По маркетплейсу       | WB / OZON / Yandex Market                     |
| `nomenclature_ref`  | По номенклатуре       | JOIN → a004_nomenclature.description          |
| `dim1`              | Измерение 1 (категория) | dim1_category из a004_nomenclature          |
| `dim2`              | Измерение 2 (линейка)   | dim2_line из a004_nomenclature              |
| `dim3`              | Измерение 3 (модель)    | dim3_model из a004_nomenclature             |
| `dim4`              | Измерение 4 (формат)    | dim4_format из a004_nomenclature            |
| `dim5`              | Измерение 5 (назначение)| dim5_sink из a004_nomenclature              |
| `dim6`              | Измерение 6 (размер)    | dim6_size из a004_nomenclature              |

### Фильтры (ViewContext)

| Параметр              | Обязателен | Описание                            |
|-----------------------|------------|-------------------------------------|
| `date_from`           | Да         | Период 1, начало (YYYY-MM-DD)       |
| `date_to`             | Да         | Период 1, конец (YYYY-MM-DD)        |
| `period2_from`        | Нет        | Период 2, начало (если нет — авто -1 мес.) |
| `period2_to`          | Нет        | Период 2, конец                     |
| `connection_mp_refs`  | Нет        | Список UUID кабинетов МП            |

## Примеры запросов

### Получить список DataView
```
GET /api/data-view
```

### Вычислить выручку за период
```json
POST /api/data-view/dv001_revenue/compute
{
  "date_from": "2026-01-01",
  "date_to": "2026-01-31",
  "params": { "metric": "revenue" }
}
```

### Drilldown по маркетплейсам
```json
POST /api/data-view/dv001_revenue/drilldown
{
  "date_from": "2026-01-01",
  "date_to": "2026-01-31",
  "params": { "metric": "profit" },
  "group_by": "marketplace"
}
```

## Связанные концепции

- **BI Индикатор (a024)** — использует DataView как источник данных через `view_id` + `metric_id`
- **BI Дашборд (a025)** — компонует несколько индикаторов на одном экране
- **p904_sales_data** — сводная таблица продаж всех маркетплейсов
