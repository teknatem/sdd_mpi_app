---
kind: app
tags: [dev-notes, debug]
title: Заметки разработчика — актуальные инструкции и известные проблемы
related: [data-view, bi-indicators]
---

# Заметки разработчика

Этот документ обновляется разработчиком (Cursor IDE).
Читай его перед началом сложных workflows: `search_knowledge(["dev-notes"])`.

## Текущее состояние системы (2026-03-12)

### Доступные инструменты (актуально)
- `list_entities`, `get_entity_schema`, `get_join_hint` — работают
- `search_knowledge`, `get_knowledge` — работают
- `list_data_sources` — единый каталог источников (base-схемы, DataView, raw); DataView, напр.: `dv001_revenue`
- `execute_query` — работает, только SELECT, автолимит 50 строк
- `create_drilldown_report` — работает, создаёт артефакт с кнопкой отчёта

### Правильные имена таблиц (для execute_query)
| Сущность | Таблица |
|----------|---------|
| Кабинеты МП | `a006_connection_mp` |
| Организации | `a002_organization` |
| Номенклатура | `a004_nomenclature` |
| Маркетплейсы | `a005_marketplace` |
| Продажи WB | `a012_wb_sales` |
| Заказы YM | `a013_ym_order` |

### Важные нюансы execute_query
- Таблица `a006_connection_mp`: колонка `is_used` (INTEGER 0/1), `description` = название магазина
- Если `get_entity_schema("a006")` кажется не возвращает данные — **это не баг**, используй напрямую:
  ```sql
  SELECT id, code, description FROM a006_connection_mp WHERE is_used = 1
  ```
- `a006_connection_mp` содержит UUID (поле `id`) — это то что нужно для `connection_mp_refs`
- Чтобы найти WB кабинеты: сначала узнай UUID маркетплейса Wildberries:
  ```sql
  SELECT id, description FROM a005_marketplace WHERE description LIKE '%Wildberries%'
  ```
  Затем используй этот UUID как фильтр:
  ```sql
  SELECT id, description FROM a006_connection_mp WHERE marketplace_id = '<uuid_wb>' AND is_used = 1
  ```
- После execute_query проверяй `_ok: true` перед использованием rows

### Известная проблема: get_entity_schema кажется "не отвечает"
**Статус**: НЕ баг в коде — схема возвращается корректно (7 полей для a006).
Если LLM говорит что инструмент не отвечает — это может быть:
1. Ответ получен, но LLM не смог его распарсить (неверная интерпретация)
2. Ответ содержит `_ok: true` и поле `fields` — нужно обратить на него внимание
**Воркэраунд**: использовать `execute_query` напрямую с известными именами колонок (см. выше).

### DataView dv001_revenue
- Метрики: `revenue`, `cost`, `commission`, `expenses`, `profit`
- Измерения: `date`, `article`, `marketplace`, `connection_mp_ref`, `nomenclature_ref`, `dim1`..`dim6`
- `connection_mp_refs` в create_drilldown_report: массив UUID из a006_connection_mp.id

## Известные ограничения
- `execute_query` — только SELECT, не INSERT/UPDATE
- Максимум 200 строк в результате execute_query
- `create_drilldown_report` требует `metric_ids` (множественное число), не `metric_id`

## Сообщения разработчику
Если что-то не работает — включи в ответ `bug_report` блок (см. раздел "Режим отладки").
Разработчик прочитает чат и исправит проблему.
