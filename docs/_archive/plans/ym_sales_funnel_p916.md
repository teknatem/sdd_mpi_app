# План (на будущее): отражение Yandex Market в воронке продаж p916

> ⚠️ **Архив: реализовано.** YM заведён в `p916_mp_sales_funnel_turnovers/builder.rs`
> (регистраторы `a041_ym_shows_sales_daily`, `a013_ym_order`, `a016_ym_returns`;
> `from_ym_shows_sales_daily` — верх воронки из a041, низ — из a013/a016).
> Документ оставлен как запись замысла; **актуальное поведение смотри в коде и
> `p916_mp_sales_funnel_turnovers/llm.md`**, а не в тексте ниже.

Статус в момент написания: **не реализовано**, дизайн-план. Каркас — работающая проекция
`p916_mp_sales_funnel_turnovers` (сейчас только Wildberries) + дашборд `d406_wb_sales_funnel`.
Продолжение линии P1-усилений (детерминированный `id`+upsert, `N/A≠0`, когорта выкупов).

---

## Context

`p916` изначально спроектирована **cross-MP** (`connection_mp_ref`, `marketplace_product_ref`
= a007, `nm_id` — nullable WB-мост), но регистраторы пока только WB. Цель — подключить YM,
не ломая WB и не создавая ложной сопоставимости (ТЗ §33).

Ключевое отличие YM от WB: **данные богаче**.
- `a013_ym_order.state.creation_date` — дата заказа **прямо на заказе** ⇒ когорта нативная,
  **хак `srid→order_dt` (Задача 3 для WB) не нужен**.
- `a013_ym_order.state.delivery_date` / `status_changed_at` — дата исполнения (event-ось).
- Построчные `YmOrderLine.details[]` (`count`, `status` = REJECTED/RETURNED, `update_date`) —
  частичные отказы/возвраты с датами.
- `header.fulfillment_type` (FBS/FBY/DBS/LAAS) — измерение нижней воронки (Этап 2).

Ограничение MVP: у YM в системе **нет источника верха воронки** (показы/переходы/корзина —
нет YM-аналитики a0XX). Значит стадия `marketing` для YM = **`N/A`** (уже поддержано флагами
`show_*_available`). Подключаем только стадию `fulfillment` (заказ→доставка→отказ→возврат→pending).

---

## Что уже готово (переиспользуем, не пишем заново)

- **Cross-MP ключи p916**: `marketplace_product_ref` как товарный ключ при `nm_id = None`.
  `deterministic_id` уже берёт `nm_id` **или** `marketplace_product_ref` — YM-ветка работает.
- **Чтение**: `aggregate_by_product` группирует по
  `(nm_id, marketplace_product_ref, nomenclature_ref)` ⇒ YM-строки (`nm_id` NULL, `mp_ref` set)
  агрегируются корректно; конверсии считаются на чтении.
- **`N/A ≠ 0`**: `show_*_available` уже отдаётся в `MpFunnelAggRow`/дашборд — YM-показы честно `N/A`.
- **Паттерн хука**: `a013_ym_order::posting::post_document` **уже** пишет `p915_mp_order_events`
  через `builder::from_ym_order` (delete-by-registrator + insert в локальной txn,
  [posting.rs:53-76](../../crates/backend/src/domain/a013_ym_order/posting.rs)). Копируем ровно
  этот блок для p916.
- **Идемпотентность**: `insert_many_with_conn` + `on_conflict(id)` уже защищает от задвоения.

---

## Маппинг стадий YM → p916 (стадия `fulfillment`)

Источник MVP — **`a013_ym_order`** (одна проекция-истина для YM-когорты). Grain: строка
(`marketplace_product_ref`) × дата. Для всех строк заказа `cohort_date = creation_date`.
Единица счёта — **штуки** (units), чтобы `pending` сходился с построчными `details`.

| Движение (`kind`) | Метрика p916 | Условие / источник | `event_date` |
|---|---|---|---|
| `order` | `order_count += qty`, `order_sum += amount_line` | каждая строка заказа | `creation_date` |
| `buyout` (=fulfilled) | `buyout_count += delivered_units`, `buyout_sum` | заказ доставлен (`status_norm` DELIVERED/RECEIVED); `delivered_units = qty − Σrejected − Σreturned` (clamp ≥0) | `delivery_date` ∥ `status_changed_at` |
| `cancel` | `cancel_count += detail.count` | `details[].status == REJECTED` | `detail.update_date` ∥ `creation_date` |
| `return` | `return_count += detail.count` | `details[].status == RETURNED` | `detail.update_date` ∥ `status_changed_at` |

- **`pending`** не хранится колонкой — считается на чтении:
  `ordered − buyout − cancel − return` (как для WB). Guard качества: отрицательное — flag (ТЗ §12).
- Цена за единицу для sum: `amount_line / qty` (∥ `buyer_price`). `buyout_sum/cancel_sum/return_sum
  = unit_price × count`.
- **Раздельные строки на каждый `kind`** (как WB order/cancel) — разные `event_date`, единый
  `cohort_date`; `deterministic_id` их разводит по `kind`. Пустые строки не пишем (разреженность).

Стадия `marketing` для YM **не пишется** (нет источника) ⇒ на чтении `N/A`.

---

## Изменения по слоям

1. **`p916 builder.rs`**
   - `pub const REG_A013: &str = "a013_ym_order";` (и `REG_A016` — в follow-up).
   - `pub fn from_ym_order(order: &YmOrder, registrator_ref: &str) -> Vec<Model>`: по каждой
     `line` → строка `order`; если заказ доставлен → строка `buyout` (delivered_units); по каждой
     `detail` REJECTED/RETURNED → строки `cancel`/`return`. Товарный ключ =
     `line.marketplace_product_ref` (`nm_id = None`), `nomenclature_ref = line.nomenclature_ref`,
     `cohort_date = creation_date`. Переиспользовать `base_row`/`deterministic_id` (kind готов).
   - Тесты (по образцу существующих): заказ с `details` [REJECTED 1, RETURNED 1] и qty 3 →
     order=3, cancel=1, return=1, buyout=1; `cohort_date=creation_date`, `event_date` по каждому
     движению; детерминизм `id`.

2. **`a013_ym_order/posting.rs`**
   - В `post_document`, в тот же txn-блок, где пишется p915 (строки 55-76): добавить
     `p916::repository::delete_by_registrator_with_conn(&txn, REG_A013, &registrator_ref)` +
     `builder::from_ym_order(&document, &registrator_ref)` + `insert_many_with_conn(&txn, rows)`.
   - В `unpost_document`: `p916::repository::delete_by_registrator_ref(&id.to_string())`.

3. **Бэкфилл истории**: прогнать репост проведённых a013 (переиспользует `post_document`) —
   добавить a013 в опции `u508` **или** разовый endpoint `rebuild-ym-funnel`, как у a036
   (`rebuild-funnel-projection`). Проекция производная — безопасно.

4. **Дашборд**: рекомендую **новый `d407_ym_sales_funnel`** (не ломать WB `d406`, у которого
   мост/подписи по `nm_id`). Читает `p916` c фильтром YM-кабинета; товар — по
   `marketplace_product_ref`/`nomenclature_ref` (join a007/a004, `aggregate_by_product` их уже
   возвращает). Колонки: Заказы/Доставки/Отмены/Возвраты/В процессе (pending) + суммы +
   конверсии заказ→доставка, доля отмен/возвратов; показы/переходы/корзина = `N/A`.
   Позже — объединить d406/d407 в единый mp-funnel с фильтром маркетплейса.

---

## Отличия YM, которые надо учесть

- **Нативная когорта** — `creation_date` на заказе, без `srid`-резолва (проще, чем WB).
- **PENDING реально вычислим** для YM (жизненный цикл известен построчно) — но, как и для WB,
  считается на чтении, не колонкой.
- **Возвраты — один источник в MVP**: берём `RETURNED` из `a013.details` (иначе двойной счёт с
  `a016_ym_returns`). `a016` (есть `order_id`, `refund_status`, причины, точные даты) — **follow-up**:
  причины возвратов (ТЗ §31.3) и/или замена `RETURNED` из a016 при расхождении.
- **`fulfillment_type` (FBS/FBY/DBS)** — разрез нижней воронки (ТЗ §31.6), в MVP не разрезаем.
- **Верх воронки отсутствует** — если появится YM-аналитика показов/переходов, добавить
  отдельный регистратор в `marketing` (схема готова).

## Data quality

- `pending < 0` и `delivered_units < 0` (рассинхрон `details`) → clamp + quality flag (ТЗ §12/§21).
- YM `marketing`-метрики → `N/A` (уже реализовано флагами доступности).

## Verification

- `cargo check -p contracts && cargo check -p backend`; `cargo test -p backend p916`
  (новые тесты `from_ym_order`).
- `cargo check -p frontend --target wasm32-unknown-unknown` (после d407).
- Функционально: провести один `a013` с частичными `details` → строки p916 верны по обеим осям;
  `d407` показывает когорту заказа, доставки/отмены/возвраты, pending; показы = `N/A`.

## Порядок работ

1. `from_ym_order` в `p916 builder.rs` + тесты.
2. Хук в `a013 posting/unpost`.
3. Бэкфилл истории a013 (репост / endpoint).
4. `d407_ym_sales_funnel` (или расширение d406).
5. Follow-up: `a016` (причины/даты возвратов), разрез `fulfillment_type`, YM-`marketing` при
   появлении данных, объединённый mp-funnel дашборд.
