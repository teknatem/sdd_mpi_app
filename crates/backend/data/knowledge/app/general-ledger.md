---
kind: app
title: "General Ledger — Главная книга"
tags:
  [
    general_ledger,
    gl,
    accounting,
    turnover,
    accounts,
    journal,
    sys_general_ledger,
  ]
related:
  [
    a012_wb_sales,
    a026_wb_advert_daily,
    p903_wb_finance_report,
    p909_mp_order_line_turnovers,
    p910_mp_unlinked_turnovers,
    a024_bi_indicator,
  ]
updated: 2026-04-03
---

# General Ledger (Главная книга)

## Что это такое

General Ledger (GL) — центральный учётный регистр всех хозяйственных операций.
Хранится в таблице `sys_general_ledger`. Каждая строка — одна проводка:
дебет одного счёта / кредит другого на сумму операции.

Это не аналитическая projection-таблица, а факт учёта, порождённый агрегатами при posting.

## Таблица sys_general_ledger

| Колонка             | Тип       | Описание                                                                            |
| ------------------- | --------- | ----------------------------------------------------------------------------------- |
| `id`                | TEXT PK   | UUID записи журнала                                                                 |
| `entry_date`        | TEXT      | Дата проводки (YYYY-MM-DD или ISO datetime)                                         |
| `layer`             | TEXT      | Слой: `oper` / `fact` / `plan`                                                      |
| `connection_mp_ref` | TEXT NULL | Ссылка на подключение МП (`a006_connection_mp.id`)                                  |
| `registrator_type`  | TEXT      | Тип регистратора: `a012_wb_sales`, `p903_wb_finance_report`, `a026_wb_advert_daily` |
| `registrator_ref`   | TEXT      | Чистый id документа-источника; тип хранится отдельно в `registrator_type`           |
| `debit_account`     | TEXT      | Дебетуемый счёт по плану счетов                                                     |
| `credit_account`    | TEXT      | Кредитуемый счёт по плану счетов                                                    |
| `amount`            | REAL      | Сумма проводки                                                                      |
| `qty`               | REAL NULL | Количество, если применимо                                                          |
| `turnover_code`     | TEXT      | Код вида оборота из реестра                                                         |
| `resource_table`    | TEXT      | Detail-таблица, из которой берётся ресурс                                           |
| `resource_field`    | TEXT      | Поле ресурса в detail-таблице                                                       |
| `resource_sign`     | INT       | Знак ресурса относительно оборота: `+1` или `-1`                                    |
| `created_at`        | TEXT      | Время создания записи                                                               |

## API эндпоинты

```text
GET  /api/general-ledger            -> список проводок
GET  /api/general-ledger/:id        -> конкретная проводка
GET  /api/general-ledger/turnovers  -> реестр видов оборотов с количеством записей
```

Параметры `GET /api/general-ledger`:

- `date_from`, `date_to` — диапазон дат `entry_date`
- `registrator_ref`, `registrator_type` — фильтр по источнику
- `layer` — `oper` / `fact` / `plan`
- `turnover_code` — конкретный вид оборота
- `debit_account`, `credit_account` — фильтр по счёту
- `connection_mp_ref` — фильтр по подключению МП
- `limit`, `offset` — пагинация

## Слои данных

| Слой   | Значение                                                        |
| ------ | --------------------------------------------------------------- |
| `oper` | Оперативный слой, обычно до финансового подтверждения           |
| `fact` | Фактический слой из финансовых документов маркетплейса          |
| `plan` | Плановый слой, резерв под будущие сценарии                      |

## План счетов

| Счёт | Название                         | Тип                        |
| ---- | -------------------------------- | -------------------------- |
| 41   | Товары                           | Активный (баланс)          |
| 44   | Расходы на продажу               | Активный (P&L)             |
| 4401 | Расходы на продажу — маркетплейс | Активный (P&L)             |
| 4402 | Комиссия МП                      | Активный (P&L)             |
| 4403 | Эквайринг МП                     | Активный (P&L)             |
| 4404 | Логистика/хранение МП            | Активный (P&L)             |
| 62   | Расчёты с покупателями           | Активно-пассивный (баланс) |
| 76   | Расчёты с прочими                | Активно-пассивный (баланс) |
| 7609 | Расчёты с маркетплейсом          | Активно-пассивный (баланс) |
| 90   | Продажи                          | Активно-пассивный (P&L)    |
| 9001 | Выручка от продаж                | Пассивный (P&L)            |
| 9002 | Себестоимость продаж             | Активный (P&L)             |
| 91   | Прочие доходы и расходы          | Активно-пассивный (P&L)    |
| 9102 | Штрафы от МП                     | —                          |

## Виды оборотов

Каждый оборот привязан к `report_group`. Инструмент `list_gl_turnovers` даёт полный список.

| Код                             | Название                    | Дт   | Кт   | Report group |
| ------------------------------- | --------------------------- | ---- | ---- | ------------ |
| `customer_revenue`              | Выручка от покупателя       | 7609 | 9001 | revenue      |
| `customer_return`               | Возврат покупателя          | 9001 | 62   | returns      |
| `mp_commission`                 | Комиссия МП                 | 4402 | 7609 | commission   |
| `mp_commission_adjustment`      | Корректировка комиссии      | 4402 | 7609 | commission   |
| `mp_acquiring`                  | Эквайринг МП                | 4403 | 7609 | acquiring    |
| `mp_logistics`                  | Логистика МП                | 4404 | 7609 | logistics    |
| `mp_storage`                    | Хранение МП                 | 4404 | 7609 | storage      |
| `mp_penalty`                    | Штраф МП                    | 9102 | 7609 | penalty      |
| `advert_clicks_no_order`         | Реклама (WB Advert)         | 9102 | 7609 | advertising  |
| `voluntary_return_compensation` | Добровольная компенсация    | 7609 | 91   | other        |

## Источники проводок

| Источник             | registrator_type         | Метод                     |
| -------------------- | ------------------------ | ------------------------- |
| WB финансовый отчёт  | `p903_wb_finance_report` | posting при импорте p903  |
| WB продажи           | `a012_wb_sales`          | posting при sync          |
| WB реклама           | `a026_wb_advert_daily`   | posting при sync          |

Проводки создаются функцией `general_ledger::service::save_entries()`.
При обновлении документа старые проводки удаляются через `remove_by_registrator_ref()`.

## Связь sys_general_ledger -> detail projection

`sys_general_ledger` хранит учётный факт. Детализация хранится в отдельных projection-таблицах.
Этот раздел является каноническим описанием маршрутизации drilldown из GL в detail projection.

Каждая запись GL содержит данные, по которым можно восстановить detail-строки:

- `turnover_code` — определяет вид оборота
- `resource_table` — в какую detail-таблицу нужно идти
- `resource_field` — какое поле в detail-таблице является ресурсом
- `resource_sign` — какой знак имеет ресурс относительно оборота

Эти поля определяют источник detail-данных для строки GL.

### Вариант 1. Получение detail по обороту

Используется, когда детализация строится от выбранного `turnover_code`.

Алгоритм:

1. В `sys_general_ledger` отбираются записи по периоду, слою, подключению и `turnover_code`.
2. Для каждой записи GL читаются `resource_table`, `resource_field`, `resource_sign`.
3. По `resource_table` выбирается detail projection, из которой извлекаются строки.
4. `resource_field` и `resource_sign` определяют, какое значение брать из detail-строки.
5. Найденные строки агрегируются в нужный drilldown-отчёт.

### Вариант 2. Получение detail по регистратору

Используется, когда детализация строится в разрезе конкретного регистратора.

Алгоритм:

1. В `sys_general_ledger` отбираются записи по `registrator_type` и `registrator_ref`.
2. Для каждой записи есть два варианта поиска detail-строк:
   - если `registrator_type == resource_table`, строки ищутся по `registrator_ref`
   - иначе строки ищутся по связи `detail.general_ledger_ref = sys_general_ledger.id`
3. Найденные detail-строки используются для построения детального представления.

Иначе говоря:

- drilldown по обороту начинается с `turnover_code`
- drilldown по регистратору начинается с `registrator_type + registrator_ref`
- переход из GL в detail выполняется через `resource_table / resource_field / resource_sign`, а связь со строкой GL при необходимости задаётся как `detail.general_ledger_ref = gl.id`

## Связь с BI и DataView

BI-индикаторы (`a024`) могут агрегировать данные GL через DataView.
Часть DataView работает напрямую по `sys_general_ledger`, часть использует detail-проекции,
которые строятся поверх GL или связаны с ним.

## Примеры SQL

```sql
-- Обороты по счетам за период
SELECT debit_account, credit_account, turnover_code, SUM(amount) AS total
FROM sys_general_ledger
WHERE entry_date BETWEEN '2026-01-01' AND '2026-01-31'
  AND layer = 'fact'
GROUP BY debit_account, credit_account, turnover_code
ORDER BY ABS(SUM(amount)) DESC;

-- Проводки конкретного регистратора
SELECT *
FROM sys_general_ledger
WHERE registrator_type = 'p903_wb_finance_report'
  AND registrator_ref = '{id}'
ORDER BY entry_date;
```

## Ключевые особенности

1. `registrator_ref` — это чистый id документа; тип документа хранится отдельно в `registrator_type`
2. `resource_table / resource_field / resource_sign` — каноническая связь записи GL с detail projection
3. `layer = fact` — основной слой для финансового анализа
4. `turnover_code` — ключ для понимания экономического смысла проводки
5. `generates_journal_entry = false` в реестре оборотов означает, что оборот не порождает запись в GL
