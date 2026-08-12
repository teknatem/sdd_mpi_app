---
id: table-builder
title: Таблицы данных
description: Построение таблиц из данных: агент собирает SELECT, задаёт колонки/форматы/условное форматирование и публикует таблицу-плагин (фильтры, сортировка, итоги, экспорт).
intents: [table_build]
tools: [find_data_sources, preview_data, build_table]
default_for: []
---

## Навык: таблицы данных

Цель — создать один активный таблица-плагин с режимами LIVE и «Сохраненные данные».

Канонический путь — не более трех вызовов:

1. `find_data_sources` — выбери DataView/base-схему или raw SQL fallback.
2. `preview_data` — проверь реальные колонки и их типы, не создавая SQL-артефакт.
3. `build_table` — передай тот же `source`, заголовок и presentation `table`.

`source` всегда является JSON-объектом. Копируй `source_template` из `find_data_sources`; не передавай JSON строкой и не вызывай недоступный `get_entity_schema`.

Не вызывай template/plugin_* инструменты: `build_table` сам проверяет данные, table spec, live binding и snapshot и публикует результат.

`table.columns` необязателен: без него builder покажет все колонки и определит типы. Если задаешь явно:

- `field` обязан совпадать с preview column;
- money/int/number/percent допустимы только для числового поля;
- сортировку по основной мере делай desc, по дате — asc;
- conditional formatting и totals ссылаются только на показанные колонки;
- проценты передаются долями: `0.34` = 34%.

Raw SQL остается штатным путем для отсутствующей схемы/DataView: один SELECT/WITH с bind-параметрами. Если результат превышает 2 000 строк или 1 MiB, добавь фильтр, GROUP BY или LIMIT. Снимок не обрезается молча.

Диалект raw SQL — SQLite, placeholders только `?`. Для выбираемого периода используй `params:["$context.date_from","$context.date_to"]` и начальные даты в `preview_data.context`; builder добавит date inputs. После успешного preview сразу вызывай `build_table`.

После успеха сообщи название, основные колонки и наличие LIVE/снимка.
