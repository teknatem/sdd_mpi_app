---
kind: app
title: A025 BI Dashboard
tags: [wb, indicators, dashboard, aggregate, a025]
related: [a024_bi_indicator]
updated: 2026-03-17
---

# Summary

`a025_bi_dashboard` хранит компоновку набора индикаторов. Это слой отображения и организации экрана, а не слой вычисления продаж.

# Structure

- `layout.groups` — дерево групп.
- `layout.groups[].items[].indicator_id` — ссылка на `a024_bi_indicator`.
- `filters` — глобальные фильтры дашборда.

# WB Indicator Display Path

Для WB dashboard flow типовой путь такой:

1. Dashboard содержит ссылку на `a024_bi_indicator`
2. Индикатор через `data_spec` вызывает `dv001_revenue`
3. `dv001_revenue` читает `p904_sales_data`
4. `p904_sales_data` содержит проекции WB продаж из `a012_wb_sales`

# Important Note

Строка формата `a025_bi_dashboard_view_<uuid>` в frontend — это ключ UI-вкладки, а не отдельная таблица данных.

# Pitfalls

- Если нужно объяснить, откуда взялось значение на дашборде, LLM должна спускаться по ссылке `indicator_id`, а не останавливаться на самом `a025`.
