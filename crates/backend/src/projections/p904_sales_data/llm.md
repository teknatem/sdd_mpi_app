---
title: P904 Sales Data
tags: [wb, indicators, projection, sales, p904]
related: [a012_wb_sales, dv001_revenue, a024_bi_indicator, a025_bi_dashboard]
updated: 2026-03-17
---

# P904 Sales Data

## Summary

`p904_sales_data` — единая BI-проекция продаж маркетплейсов. Для WB именно она является главным источником сумм, расходов, комиссии, прибыли и других агрегируемых показателей.

## WB Projection Logic

Для документа `a012_wb_sales` проекция рассчитывает:

- `customer_in` = `finished_price`, если это продажа
- `customer_out` = `finished_price`, если это возврат
- `price_return` = `price_list`, если это возврат
- `commission_out` = `amount_line - price_effective`
- `commission_percent` = `commission_out / price_effective * 100`
- `coinvest_persent` = `spp`
- `acquiring_out` = `-finished_price * 0.019`
- `coinvest_in` определяется через разницу `amount_line - finished_price`
- `seller_out` = `-(customer_out + customer_in) - (acquiring_out + coinvest_in + commission_out)`
- `total` = `-seller_out`

## Additional Enrichment

- `cost` подтягивается из `p906_nomenclature_prices` по `nomenclature_ref` и дате продажи.
- Для продаж `cost` хранится со знаком минус.
- Для возвратов `cost` хранится со знаком плюс.

## Role For Indicators

Индикаторы WB не должны читать `a012_wb_sales` напрямую, если нужен KPI уровень.
Они должны использовать `DataView` поверх `p904_sales_data`.

## Typical Metrics

- `revenue` = `customer_in + customer_out`
- `cost` = `cost`
- `commission` = `commission_out`
- `expenses` = `acquiring_out + penalty_out + logistics_out`
- `profit` = `-seller_out`

## Pitfalls

- В этой проекции знаки полей важны. Многие расходы уже записаны отрицательными величинами.
- `registrator_type = WB_Sales` указывает, что строка пришла из `a012_wb_sales`.
