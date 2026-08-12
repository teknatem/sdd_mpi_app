---
kind: app
title: A024 BI Indicator
tags: [wb, indicators, bi, aggregate, a024]
related: [dv001_revenue, p904_sales_data, a025_bi_dashboard]
updated: 2026-03-17
---

# Summary

`a024_bi_indicator` описывает один KPI-виджет. Он не хранит бизнес-данные продаж, а хранит способ вычисления и способ отображения.

# Data Binding

- `data_spec.view_id` — идентификатор DataView.
- `data_spec.metric_id` — идентификатор метрики внутри DataView.
- `params` — параметры фильтрации индикатора.
- `view_spec` — HTML/CSS представление и форматирование.
- `drill_spec` — правила перехода в детализацию.

# WB Indicator Path

Для WB KPI типовой путь такой:

1. `data_spec.view_id = dv001_revenue`
2. `data_spec.metric_id = revenue | profit | profit_d | commission | expenses | order_count`
3. DataView читает `p904_sales_data`
4. Результат форматируется согласно `view_spec`

# Important Distinction

- `a024_bi_indicator` не является таблицей фактов.
- Он ссылается на слой вычисления.
- Один и тот же `dv001_revenue` может использоваться многими индикаторами с разными `metric_id`, параметрами и стилями.

# Pitfalls

- Когда LLM объясняет значение индикатора, нужно идти в `data_spec` и затем в соответствующий DataView, а не пытаться искать поле в таблице `a024_bi_indicator`.
