---
kind: app
title: Новые универсальные проекции маркетплейсов: p909 и p910
tags: [marketplaces, wildberries, ym, p909, p910, analytics, order-lines, turnovers]
related: [a012_wb_sales, a013_ym_order, a015_wb_orders, p903_wb_finance_report, p904_sales_data]
updated: 2026-03-19
---

# Новые универсальные проекции маркетплейсов: p909 и p910

Документ фиксирует уточненную концепцию новых проекций.

Ключевая идея:

- `p909` и `p910` не проектируются как wide-table с большим набором `oper_*` и `fact_*` колонок по каждому ресурсу
- вместо этого используется узкая модель:
  - код оборота
  - тип значения
  - способ агрегации
  - две ресурсные колонки: `amount_oper`, `amount_fact`
- выбор актуального значения делается запросом или view:
  - брать `fact`, если он есть
  - иначе брать `oper`

Проекции проектируются сразу как универсальные для всех маркетплейсов, но реализуются поэтапно. Первый этап — Wildberries.

## Основные решения

- `p909` полностью заменяет `p904_sales_data` после завершения реализации
- если для WB есть `a012 + p903` по `srid`, но нет `a015`, строка в `p909` все равно создается
- `p910` хранит сырые нормализованные строки отвязанных доходов и расходов
- `connection_mp_ref` считается достаточным идентификатором кабинета
- в терминологии operational используется префикс `oper_`, но в новых проекциях ресурсные колонки только две:
  - `amount_oper`
  - `amount_fact`

## Общая архитектура

Нужны три уровня:

1. Классификатор оборотов
2. Правила соответствия данных маркетплейсов этим оборотам
3. Узкие регистры движений:
   - `p909` — движения, привязанные к строкам заказов
   - `p910` — движения, не привязанные к строкам заказов

Классификатор и правила соответствия хранятся в коде.

## Классификатор оборотов

### Назначение

Классификатор описывает смысл строки проекции:

- что это за оборот
- это деньги, количество или процент
- как его агрегировать
- как выбирать между `oper` и `fact`

### Структура классификатора в коде

Рекомендуемая структура:

```rust
pub struct TurnoverClass {
    pub code: &'static str,
    pub name: &'static str,
    pub scope: TurnoverScope,
    pub value_kind: ValueKind,
    pub agg_kind: AggKind,
    pub selection_rule: SelectionRule,
    pub report_group: ReportGroup,
    pub description: &'static str,
    pub is_active: bool,
}
```

### Enum scope

```rust
pub enum TurnoverScope {
    OrderLine,
    Unlinked,
    Both,
}
```

### Enum value_kind

```rust
pub enum ValueKind {
    Money,
    Quantity,
    Percent,
}
```

На первом этапе рекомендуется ограничиться только числовыми значениями.
Текстовые и статусные данные лучше не включать в регистры оборотов.

### Enum agg_kind

```rust
pub enum AggKind {
    Sum,
    Avg,
    Last,
    None,
}
```

Смысл:

- `Sum` — суммируется
- `Avg` — усредняется
- `Last` — берется последнее значение
- `None` — не агрегируется

### Enum selection_rule

```rust
pub enum SelectionRule {
    PreferFact,
    PreferOper,
    FactOnly,
    OperOnly,
    SumBoth,
}
```

Смысл:

- `PreferFact` — если есть `amount_fact`, брать его, иначе `amount_oper`
- `PreferOper` — если есть `amount_oper`, брать его, иначе `amount_fact`
- `FactOnly` — использовать только `fact`
- `OperOnly` — использовать только `oper`
- `SumBoth` — суммировать оба слоя

### Enum report_group

```rust
pub enum ReportGroup {
    Revenue,
    Commission,
    Logistics,
    Penalty,
    Storage,
    Payout,
    Cost,
    Quantity,
    OtherIncome,
    OtherExpense,
    Info,
}
```

Это не бухгалтерский план счетов, а аналитическая группировка для BI.

### Примеры кодов оборотов

Минимальный стартовый набор:

- `revenue`
- `commission`
- `acquiring`
- `logistics`
- `penalty`
- `storage`
- `payout`
- `cost`
- `qty`
- `buyout_percent`
- `other_income`
- `other_expense`

## Правила соответствия данных маркетплейсов

### Назначение

Одного классификатора недостаточно. Нужен второй слой — правила, которые переводят поля маркетплейсов в строки оборотов.

Например:

- `a012_wb_sales.finished_price` -> `revenue`, слой `oper`
- `p903_wb_finance_report.retail_amount` -> `revenue`, слой `fact`
- `p903_wb_finance_report.storage_fee` -> `storage`, слой `fact`
- `a013_ym_order.qty` -> `qty`, слой `oper`

### Структура правил в коде

Рекомендуемая структура:

```rust
pub struct TurnoverMappingRule {
    pub code: &'static str,
    pub source_entity: SourceEntity,
    pub source_layer: SourceLayer,
    pub turnover_code: &'static str,
    pub link_mode: LinkMode,
    pub date_source: DateSource,
    pub extractor: ExtractorKind,
}
```

### Вспомогательные enum

```rust
pub enum SourceLayer {
    Oper,
    Fact,
}

pub enum LinkMode {
    OrderLine,
    Unlinked,
}
```

`extractor` в реальности будет реализован кодом, а не декларативным SQL.
То есть правила — это registry в коде, а не таблица в БД.

## P909 — регистр оборотов по строкам заказов

### Назначение

`p909` — основная универсальная проекция order-line аналитики.

Одна запись `p909` = один оборот по одной строке заказа.

Это не “одна строка заказа со всеми ресурсами”, а “один ресурс по одной строке заказа”.

### Grain

Гранулярность:

- `connection_mp_ref`
- `order_key`
- `line_key`
- `turnover_code`

При необходимости допускается хранить несколько строк по одной и той же строке заказа, если это разные обороты.

Для WB на первом этапе:

- `order_key = srid`
- `line_key = srid`

Если есть `a012 + p903` по `srid`, но нет `a015`, строка в `p909` все равно создается.

### Минимальный состав полей

Рекомендуемая структура:

```rust
pub struct P909OrderLineTurnover {
    pub id: String,
    pub connection_mp_ref: String,

    pub order_key: String,
    pub line_key: String,

    pub turnover_code: String,
    pub value_kind: String,
    pub agg_kind: String,

    pub amount_oper: Option<f64>,
    pub amount_fact: Option<f64>,

    pub date_oper: Option<String>,
    pub date_fact: Option<String>,

    pub order_doc_ref: Option<String>,
    pub oper_doc_ref: Option<String>,
    pub fact_source_ref: Option<String>,

    pub nomenclature_ref: Option<String>,
    pub marketplace_product_ref: Option<String>,

    pub link_status: String,

    pub loaded_at_utc: String,
    pub updated_at_utc: String,
    pub schema_version: i32,
}
```

### Обязательные поля

- `id`
- `connection_mp_ref`
- `order_key`
- `line_key`
- `turnover_code`
- `value_kind`
- `agg_kind`
- `link_status`

### Комментарии по полям

- `id` — стабильный business key строки оборота
- `turnover_code` — код из классификатора
- `value_kind` и `agg_kind` лучше денормализовать в строку, чтобы не делать обязательный join к классификатору
- `amount_oper` и `amount_fact` — единственные ресурсные колонки
- `date_oper` и `date_fact` — даты происхождения соответствующих значений
- `order_doc_ref`, `oper_doc_ref`, `fact_source_ref` — трассировка источника
- `link_status` — статус полноты связки

### Статусы link_status

Минимальный набор:

- `full`
- `order_only`
- `oper_only`
- `fact_only`
- `order_oper`
- `order_fact`
- `oper_fact`

### Почему отдельные даты нужны

Если хранить только одну дату, теряется различие между:

- оперативной датой
- фактической датой финансового отчета

Для WB это критично, так как `a012` и `p903` приходят в разное время и могут относиться к разным операционным моментам.

## P910 — регистр отвязанных оборотов

### Назначение

`p910` — простая универсальная проекция сырых нормализованных оборотов, которые не удалось надежно привязать к строке заказа.

Одна запись `p910` = один оборот без order-line linkage.

### Минимальный состав полей

Рекомендуемая структура:

```rust
pub struct P910UnlinkedTurnover {
    pub id: String,
    pub connection_mp_ref: String,

    pub turnover_code: String,
    pub value_kind: String,
    pub agg_kind: String,

    pub amount_oper: Option<f64>,
    pub amount_fact: Option<f64>,

    pub date_oper: Option<String>,
    pub date_fact: Option<String>,

    pub source_oper_ref: Option<String>,
    pub source_fact_ref: Option<String>,

    pub nomenclature_ref: Option<String>,
    pub comment: Option<String>,

    pub loaded_at_utc: String,
    pub updated_at_utc: String,
    pub schema_version: i32,
}
```

### Комментарии

- структура максимально похожа на `p909`
- отличие только в отсутствии `order_key`, `line_key` и order-level ссылок
- это упрощает BI и сервисы пересчета

## Выбор актуального значения

Выбор между `amount_oper` и `amount_fact` не фиксируется в проекциях.

Он определяется запросом, view или DataView на основе `selection_rule` классификатора.

Типовые выражения:

- `COALESCE(amount_fact, amount_oper)` для `PreferFact`
- `COALESCE(amount_oper, amount_fact)` для `PreferOper`
- `amount_fact` для `FactOnly`
- `amount_oper` для `OperOnly`
- `COALESCE(amount_oper, 0) + COALESCE(amount_fact, 0)` для `SumBoth`

Это ключевое отличие новой модели от wide-table.

## Роль p903_wb_finance_report

`p903_wb_finance_report` остается upstream-источником WB fact-данных.

Он:

- не переводится в агрегат
- не становится финальной BI-проекцией
- используется как источник строк для `p909` и `p910`

## Этапы внедрения

### Этап 1 — Wildberries

- реализовать классификатор оборотов в коде
- реализовать правила соответствия WB-данных этим оборотам
- построить `p909` и `p910` только для WB
- перевести WB-аналитику с `p904` на новую модель

### Этап 2 — другие маркетплейсы

- подключить YM через тот же классификатор и те же типы правил
- затем остальные маркетплейсы

## Итог

Принята следующая модель:

- `p909` — узкий регистр оборотов по строкам заказов
- `p910` — узкий регистр отвязанных оборотов
- ресурсные колонки только две:
  - `amount_oper`
  - `amount_fact`
- семантика строки определяется классификатором оборотов:
  - `turnover_code`
  - `value_kind`
  - `agg_kind`
  - `selection_rule`
- правила соответствия данных маркетплейсов и оборотов хранятся в коде
- новая модель сознательно не оптимизируется под прямую совместимость с `dv001`
