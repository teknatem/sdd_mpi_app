---
title: DV001 Revenue
tags: [wb, indicators, dataview, dv001, revenue]
related: [p904_sales_data, a024_bi_indicator]
updated: 2026-03-17
---

# Summary

`dv001_revenue` — семантический слой BI над `p904_sales_data`. Он превращает строки проекции в метрики и drilldown, пригодные для индикаторов и дашбордов.

# Data Source

- Источник данных: `p904_sales_data`.
- Контекст фильтрации: `date_from`, `date_to`, опционально `period2_from`, `period2_to`, `connection_mp_refs`.

# Supported Metrics

- `revenue` = `customer_in + customer_out`
- `order_count` = `COUNT(DISTINCT registrator_ref)`
- `avg_check` = `(customer_in + customer_out) / COUNT(DISTINCT registrator_ref)`
- `cost` = `cost`
- `commission` = `commission_out`
- `expenses` = `acquiring_out + penalty_out + logistics_out`
- `profit` = `-seller_out`
- `profit_d` = `(customer_in + customer_out) + cost`

# Supported Drilldown Dimensions

- `date`
- `article`
- `registrator_type`
- `connection_mp_ref`
- `marketplace`
- `nomenclature_ref`
- `dim1` .. `dim6`

# Role For A024

Если в `a024_bi_indicator.data_spec.view_id = dv001_revenue`, то индикатор вычисляется именно через этот DataView.
`metric_id` из индикатора пробрасывается в `ViewContext.params["metric"]`.

# Pitfalls

- Для WB KPI правильнее описывать метрику через `metric_id`, а не писать свободный SQL.
- `order_count` считает distinct `registrator_ref`, а не количество строк в `p904_sales_data`.
- `avg_check` должен быть согласован с показателями `revenue` и `order_count`, то есть считаться как `revenue / order_count`.
- `p904_sales_data.date` может храниться как ISO datetime (`YYYY-MM-DDTHH:MM:SS...`). Если период приходит как `YYYY-MM-DD`, фильтровать нужно по дате без времени, иначе выпадет последний день периода. См. `docs/date-period-filtering.md`.
