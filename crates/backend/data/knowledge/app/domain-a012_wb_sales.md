---
kind: app
title: A012 WB Sales
tags: [wb, sales, indicators, aggregate, a012]
related: [u504_import_from_wildberries, p904_sales_data, dv001_revenue, a024_bi_indicator]
updated: 2026-03-17
---

# Summary

`a012_wb_sales` хранит нормализованные строки продаж и возвратов Wildberries после импорта из WB API.
Одна запись агрегата соответствует одной строке внешнего отчета WB.

# Inputs

- Источник: usecase `u504_import_from_wildberries`.
- DTO импорта: `WbSaleRow`.
- Сырой JSON строки сохраняется вместе с документом через `store_document_with_raw`.

# Key Fields

- `header.sale_id` — главный идентификатор дедупликации.
- `header.document_no` — обычно `srid` из WB.
- `line.supplier_article` — артикул продавца.
- `line.nm_id` — `nmId` WB.
- `line.finished_price` — итоговая цена покупателя после скидок.
- `line.amount_line` — сумма к выплате или расчетная база из WB.
- `state.event_type` — нормализованный тип события: `sale` или `return`.
- `state.sale_dt` — дата продажи/возврата.

# Rules

- Если `sale_id` не пришел из API, он генерируется из `document_no`, `event_type`, `supplier_article` и timestamp.
- `event_type` определяется по знаку `quantity`: отрицательное значение означает возврат.
- `status_norm` нормализуется в `DELIVERED` для продажи и `RETURNED` для возврата.
- Агрегат сам по себе не является BI-слоем. Он подготавливает нормализованные данные для posting и проекций.

# Downstream

- При posting документ проецируется в `p904_sales_data`.
- Для детального анализа WB продаж LLM должна рассматривать `a012_wb_sales` как источник строковых операций, а `p904_sales_data` как источник BI-метрик.

# Pitfalls

- `brand` из API в коде попадает в `line.name`; это не гарантированно полное наименование товара.
- Для BI не следует суммировать поля напрямую из `a012_wb_sales`, если уже есть нужная метрика в `p904_sales_data`.
