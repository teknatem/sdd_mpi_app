---
id: chart-builder
title: Графики и диаграммы
description: Построение графиков из данных: пользователь описывает, что показать — агент собирает SELECT, выбирает тип (линия/столбцы/доли) и публикует график-плагин (Chart.js).
intents: [chart_build]
tools: [find_data_sources, preview_data, build_chart]
default_for: []
---

## Навык: графики

Цель — создать один активный график-плагин с режимами LIVE и «Сохраненные данные».

Канонический путь — не более трех вызовов:

1. `find_data_sources` — найди DataView или base-схему. Raw SQL является штатным fallback, если готового источника нет.
2. `preview_data` — передай `source` вида `schema`, `dataview` или `sql` **и сразу намеченный `chart`** (тот же presentation-спек, что пойдёт в build_chart, плюс `title`). Сверь реальные имена/типы колонок и проверь `build_ready`. Preview не создает SQL-артефакт.
3. `build_chart` — вызывай ТОЛЬКО когда preview вернул `build_ready: true`; передай тот же `source`, тот же `chart` и заголовок.

Гейт презентации у `preview_data` и `build_chart` один и тот же: если `build_ready: true`, build не упадёт на презентации. Если `build_ready: false` — в ответе есть `presentation` с `error_code` и фактическим списком `columns`: выбери `chart.category` / `chart.series[].field` строго из этих `columns` и **повтори `preview_data`**. Не вызывай `build_chart` на неподтверждённом `chart` и не сдавайся с отчётом об ошибке, не исчерпав хотя бы одну такую правку.

`source` всегда передавай JSON-объектом, не строкой и не массивом. Копируй `source_template` из `find_data_sources`. Не вызывай `get_entity_schema`: в этом навыке его нет, все разрешенные IDs уже перечислены в `capabilities`.

Не вызывай `chart_template`, `plugin_validate`, `plugin_smoke_test` или `plugin_upsert`: `build_chart` выполняет проверки, сохраняет live binding и снимок, публикует плагин и возвращает карточку.

Правила:

- `chart.category` — существующая колонка из preview; числовая категория допустима, например номер дня недели 1–7.
- `chart.series[].field` — существующая числовая колонка.
- line/area — динамика по времени; bar — сравнение категорий; pie/doughnut — доли небольшого числа категорий.
- Для top-N bar ставь `horizontal:true`.
- Raw SQL: один SELECT/WITH, значения передавай через `params`; закрытые credential-поля недоступны.
- Диалект raw SQL — SQLite: `strftime`, `date`, `datetime`; не используй `date_trunc`, `EXTRACT`, `:named` параметры. Только `?` placeholders.
- Номер дня недели 1=понедельник … 7=воскресенье в SQLite: `((CAST(strftime('%w', date_column) AS INTEGER) + 6) % 7) + 1`. Для `a012_wb_sales` используй реальные колонки `sale_date`, `supplier_article`, `qty`, `amount_line`, `total_price`.
- Для выбираемого периода используй `?` и `params:["$context.date_from","$context.date_to"]`; в `preview_data.context` передай начальные даты. Builder автоматически добавит date inputs и live-перезапрос.
- В schema filters допустимы только операторы `eq|not_eq|lt|lte|gt|gte|between|in|not_in|contains|is_null|is_not_null`; для периода: `between` + `from:"$context.date_from"` + `to:"$context.date_to"`.
- Если результат превышает 200 строк, агрегируй или ограничь запрос. Не проси систему молча обрезать снимок.
- При структурированной ошибке исправь только указанный `stage` и повтори соответствующий вызов.
- После успешного `preview_data` сразу вызывай `build_chart`; не ищи источники повторно.

После успеха кратко сообщи название, период/метрику и наличие LIVE/снимка.
