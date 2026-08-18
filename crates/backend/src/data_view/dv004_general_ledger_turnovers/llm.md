---
title: DV004 General Ledger Turnovers
tags: [dv004, general_ledger, gl, bi, dataview, turnover]
related: [sys_general_ledger, a024_bi_indicator, p909_mp_order_line_turnovers, p911_wb_advert_by_items]
updated: 2026-05-10
---

# Summary

`dv004_general_ledger_turnovers` — универсальный DataView для KPI на базе проводок General Ledger (`sys_general_ledger`).
Используй его, когда пользователь спрашивает про сумму оборота GL: комиссию, рекламу, логистику, штрафы,
выручку, возвраты или другие `turnover_code`.

# Data Source

- Основной источник: `sys_general_ledger`.
- Детализация по номенклатуре возможна, если GL-строки указывают на detail projection через `resource_table` и `general_ledger_ref`.
- Перед выбором `turnover_code` вызывай `list_gl_turnovers([report_group])`.

# Required Params

`dv004` требует дополнительные параметры в `params`:

- `layer` — слой GL: `oper`, `fact` или `plan`.
- `turnover_code` — один код оборота, например `mp_commission`.
- `turnover_items` — альтернативно формула из нескольких оборотов через запятую/точку с запятой. Можно ставить знак: `+customer_revenue,-customer_return`.

Если указан `turnover_items`, `turnover_code` не нужен.

# Metrics

- `amount` — подписанная сумма оборота.
- `entry_count` — количество GL-строк по выбранной формуле.

# Dimensions

Базовые измерения:

- `entry_date` — по дням.
- `connection_mp_ref` — по кабинету маркетплейса.
- `registrator_type` — по типу документа.
- `registrator_ref` — по документу.
- `layer` — по слою.

Номенклатурные измерения доступны только для оборотов, где GL связан с detail projection:

- `nomenclature`
- `dim1_category`
- `dim2_line`
- `dim3_model`
- `dim4_format`
- `dim5_sink`
- `dim6_size`

# Common Turnovers

Для точного списка всегда используй `list_gl_turnovers`.

- Комиссия маркетплейса: `report_group=commission`, обычно `turnover_code=mp_commission`, `layer=fact`.
- Реклама WB: `report_group=advertising`, обычно `turnover_code=advert_clicks_no_order`, `layer=fact`.
- Логистика: `report_group=logistics`, обычно `turnover_code=mp_logistics`, `layer=fact`.
- Штрафы: `report_group=penalty`, обычно `turnover_code=mp_penalty`, `layer=fact`.
- Выручка: `report_group=revenue`, обычно `turnover_code=customer_revenue`, слой зависит от вопроса.

# Drilldown Examples

Комиссия WB по дням:

```json
{
  "view_id": "dv004_general_ledger_turnovers",
  "group_by": "entry_date",
  "metric_id": "amount",
  "date_from": "2026-04-01",
  "date_to": "2026-04-30",
  "description": "Комиссия WB по дням за апрель 2026",
  "connection_mp_refs": ["<a006_connection_mp.id>"],
  "params": {
    "layer": "fact",
    "turnover_code": "mp_commission"
  }
}
```

Рекламные расходы WB по кабинетам:

```json
{
  "view_id": "dv004_general_ledger_turnovers",
  "group_by": "connection_mp_ref",
  "metric_id": "amount",
  "date_from": "2026-04-01",
  "date_to": "2026-04-30",
  "description": "Рекламные расходы WB по кабинетам за апрель 2026",
  "params": {
    "layer": "fact",
    "turnover_code": "advert_clicks_no_order"
  }
}
```

Составная формула оборотов:

```json
{
  "view_id": "dv004_general_ledger_turnovers",
  "group_by": "entry_date",
  "metric_id": "amount",
  "date_from": "2026-04-01",
  "date_to": "2026-04-30",
  "description": "Чистая выручка по дням",
  "params": {
    "layer": "fact",
    "turnover_items": "+customer_revenue,-customer_return"
  }
}
```

# SQL Fallback

Если нужно быстро ответить числом без drilldown-артефакта, используй `execute_query`:

```sql
SELECT SUM(amount) AS total_amount
FROM sys_general_ledger
WHERE layer = 'fact'
  AND turnover_code = 'mp_commission'
  AND entry_date >= '2026-04-01'
  AND entry_date < '2026-05-01'
  AND connection_mp_ref = '<a006_connection_mp.id>';
```

# Pitfalls

- Не угадывай `turnover_code`; сначала вызови `list_gl_turnovers`.
- Для месяца в SQL лучше использовать полуоткрытый период: `entry_date >= 'YYYY-MM-01' AND entry_date < 'YYYY-MM+1-01'`.
- `fact` — основной слой для финансового анализа; `oper` подходит для оперативных заказов до финального подтверждения.
- Для рекламы по товарам чаще лучше использовать `dv002_wb_advert_by_items` или `p911_wb_advert_by_items`, а `dv004` — для итоговых GL-оборотов.
