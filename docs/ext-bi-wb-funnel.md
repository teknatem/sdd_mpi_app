# Внешний BI-API маркетплейсов для Power BI

Внешние эндпоинты для BI-потребителей (Power BI и пр.). Все — под одной простой
авторизацией по заголовку `X-Api-Key` (см. раздел «Авторизация»). Доступные выгрузки:

| Эндпоинт | Данные |
|---|---|
| `GET /api/ext/v1/wb-sales-funnel` | Воронка продаж WB (a036), строка на `nm_id × дата` |
| `GET /api/ext/v1/wb-stocks` | Остатки WB (a037) на уровне товара, последние или на дату |
| `GET /api/ext/v1/wb-finance-report` | Финансовый отчёт WB (p903) за период, native-строки WB |
| `GET /api/ext/v1/wb-advert-daily` | Дневная реклама WB (a026) за период, строка на кампанию × товар × дата |
| `GET /api/ext/v1/ym-payment-report` | Отчёт по платежам YM (p907) за период, native-строки YM |

---

## Воронка продаж: `GET /api/ext/v1/wb-sales-funnel`

Плоские строки воронки продаж Wildberries (`a036_wb_sales_funnel_daily`), одна строка на `nm_id × дата`.

## Эндпоинт

```
GET http://<SERVER>:3000/api/ext/v1/wb-sales-funnel
```

| Параметр        | Обяз. | Описание                                       |
|-----------------|-------|------------------------------------------------|
| `date_from`     | да    | Начало периода, `YYYY-MM-DD`                    |
| `date_to`       | да    | Конец периода, `YYYY-MM-DD` (включительно)      |
| `connection_id` | нет   | Фильтр по кабинету WB (UUID)                    |
| `limit`         | нет   | Макс. строк в ответе (по умолчанию 5000, потолок 50000) |
| `offset`        | нет   | Смещение для постраничной выгрузки              |

Без `date_from`/`date_to` → `400`. WB отдаёт данные воронки примерно за последнюю неделю.

### Кабинеты WB (`connection_id`)

| Кабинет        | `connection_id`                        |
|----------------|----------------------------------------|
| WB - SANSTAR   | `1386a311-1e26-4676-b696-8d577a119eec` |
| WB2 - CTC      | `42e29532-72b1-4f38-be6e-38c331c61fe6` |

Без параметра `connection_id` в ответ попадают все кабинеты; каждая строка уже содержит
`connection_id` и `connection_name`, так что фильтровать можно и на стороне Power BI.

## Авторизация

Статический ключ в заголовке `X-Api-Key`. Значение — `[external_api].api_key` из `config.toml`
на сервере (тот же ключ, что у 1С-интеграции). Без ключа или с неверным → `401`;
если ключ на сервере не задан → `503`.

## Формат ответа

```json
{
  "items": [
    {
      "date": "2026-07-14",
      "connection_id": "1386a311-…",
      "connection_name": "WB - SANSTAR",
      "organization_name": "ООО …",
      "currency": "RUB",
      "nm_id": 316700126,
      "vendor_code": "…",
      "brand_name": "NESSTOR",
      "subject_id": 7717,
      "subject_name": "…",
      "title": "…",
      "open_count": 7,
      "add_to_wishlist_count": 0,
      "cart_count": 3,
      "order_count": 0,
      "order_sum": 0.0,
      "buyout_count": 0,
      "buyout_sum": 0.0,
      "buyout_percent": 0.0,
      "add_to_cart_conversion": 43.0,
      "cart_to_order_conversion": 0.0
    }
  ],
  "total": 8408
}
```

Поля метрик идут в порядке стадий воронки: переходы → отложенные → корзина → заказ → выкуп + конверсии.
`total` — общее число строк за период (до пагинации).

## Быстрая проверка (curl)

```powershell
curl.exe -H "X-Api-Key: <КЛЮЧ>" `
  "http://localhost:3000/api/ext/v1/wb-sales-funnel?date_from=2026-07-08&date_to=2026-07-14"
```

## Подключение в Power BI Desktop

1. **Получить данные** → **Из Интернета** → режим **Расширенный**.
2. **URL по частям** — адрес эндпоинта с параметрами периода, например:
   `http://<SERVER>:3000/api/ext/v1/wb-sales-funnel?date_from=2026-07-08&date_to=2026-07-14`
3. **Заголовки HTTP-запроса** → добавить `X-Api-Key` со значением ключа.
4. **ОК** → Power BI распознаёт JSON. Развернуть список `items` в таблицу
   (**Преобразовать в таблицу** → развернуть столбцы записей).
5. При необходимости задать типы столбцов (даты, числа) и нажать **Закрыть и применить**.

Пример запроса на языке M (вкладка **Расширенный редактор**):

```m
let
    Source = Json.Document(
        Web.Contents(
            "http://<SERVER>:3000/api/ext/v1/wb-sales-funnel?date_from=2026-07-08&date_to=2026-07-14",
            [ Headers = [ #"X-Api-Key" = "<КЛЮЧ>" ] ]
        )
    ),
    Items = Source[items],
    Table = Table.FromRecords(Items)
in
    Table
```

> Для запланированного обновления в Power BI Service ключ удобно вынести в
> **Параметр** (Manage Parameters), а не хранить в тексте запроса.

---

## Остатки: `GET /api/ext/v1/wb-stocks`

Остатки WB на уровне товара (`nm_id`): на складах WB и на складах продавца.
Источник — ежедневные снимки `a037_wb_product_snapshot`. Разбивки по складам/штрихкодам
и «в пути» нет (эти данные не хранятся).

| Параметр        | Обяз. | Описание                                                        |
|-----------------|-------|-----------------------------------------------------------------|
| `date`          | нет   | Остаток на дату `YYYY-MM-DD`. Пусто → последний снимок           |
| `connection_id` | нет   | Фильтр по кабинету WB (UUID). Пусто → все кабинеты               |
| `limit`         | нет   | Макс. строк (по умолчанию/потолок 50000)                        |
| `offset`        | нет   | Смещение для постраничной выгрузки                              |

Снимки есть не за каждый день (WB не отдаёт историю задним числом), поэтому при заданной
`date` берётся снимок с ближайшей датой **≤ `date`**. Фактическая дата — в поле `snapshot_date`
каждой строки.

Пример ответа:

```json
{
  "items": [
    {
      "snapshot_date": "2026-07-15",
      "connection_id": "1386a311-…",
      "connection_name": "WB - SANSTAR",
      "organization_name": "ООО …",
      "nm_id": 316700126,
      "vendor_code": "…",
      "brand_name": "NESSTOR",
      "subject_id": 7717,
      "subject_name": "…",
      "title": "…",
      "stock_wb": 42,
      "stock_mp": 0,
      "stock_balance_sum": 12990.0
    }
  ],
  "total": 3134
}
```

```powershell
curl.exe -H "X-Api-Key: <КЛЮЧ>" "http://localhost:3000/api/ext/v1/wb-stocks"                 # последние
curl.exe -H "X-Api-Key: <КЛЮЧ>" "http://localhost:3000/api/ext/v1/wb-stocks?date=2026-07-10" # на дату
```

---

## Финансовый отчёт: `GET /api/ext/v1/wb-finance-report`

Строки финансового отчёта WB (`p903_wb_finance_report`) за период — в «сыром» native-виде WB
(тот же набор полей, что отдаёт `reportDetailByPeriod`: `rrd_id`, `rr_dt`, `realizationreport_id`,
`doc_type_name`, `supplier_oper_name`, `ppvz_for_pay`, `retail_amount`, `delivery_rub`, `penalty`,
`sale_dt`, `srid`, … — всё как у WB). Дополнительно в каждую строку добавлены `connection_mp_ref`
и `organization_ref` для различения кабинетов.

| Параметр        | Обяз. | Описание                                             |
|-----------------|-------|------------------------------------------------------|
| `date_from`     | да    | Начало периода по `rr_dt`, `YYYY-MM-DD`              |
| `date_to`       | да    | Конец периода по `rr_dt`, `YYYY-MM-DD` (включительно)|
| `connection_id` | нет   | Фильтр по кабинету WB (= `connection_mp_ref`)        |
| `limit`         | нет   | Макс. строк (по умолчанию 5000, потолок 20000)       |
| `offset`        | нет   | Смещение для постраничной выгрузки                   |

Без `date_from`/`date_to` → `400`. Отчёт объёмный (~3 КБ на строку) — выгружайте постранично
через `limit`/`offset`; порядок стабилен (`rr_dt`, затем `rrd_id`), поле `total` — общее число строк.

```powershell
curl.exe -H "X-Api-Key: <КЛЮЧ>" `
  "http://localhost:3000/api/ext/v1/wb-finance-report?date_from=2026-06-01&date_to=2026-06-30&limit=5000&offset=0"
```

Ответ: `{ "items": [ <native-строки WB> ], "total": …, "limit": …, "offset": … }`.

---

## Реклама (дневная): `GET /api/ext/v1/wb-advert-daily`

Дневные показатели рекламы Wildberries (`a026_wb_advert_daily`) — одна строка на
кампанию × товар × дата. Источник — `repository::product_rows_for_period`.

| Параметр        | Обяз. | Описание                                                |
|-----------------|-------|---------------------------------------------------------|
| `date_from`     | да    | Начало периода, `YYYY-MM-DD`                            |
| `date_to`       | да    | Конец периода, `YYYY-MM-DD` (включительно)              |
| `connection_id` | нет   | Фильтр по кабинету WB (UUID). Пусто → все кабинеты      |
| `limit`         | нет   | Макс. строк (по умолчанию 5000, потолок 50000)          |
| `offset`        | нет   | Смещение для постраничной выгрузки                      |

Без `date_from`/`date_to` → `400`.

Пример ответа:

```json
{
  "items": [
    {
      "date": "2026-07-14",
      "connection_id": "1386a311-…",
      "connection_name": "WB - SANSTAR",
      "organization_name": "ООО …",
      "advert_id": 12345678,
      "nm_id": 316700126,
      "nm_name": "…",
      "nomenclature_ref": "…",
      "app_types": [1, 32],
      "placements": ["recom", "search"],
      "views": 1420,
      "clicks": 37,
      "ctr": 2.6,
      "cpc": 8.1,
      "atbs": 12,
      "orders": 4,
      "shks": 5,
      "sum": 300.0,
      "sum_price": 5990.0,
      "cr": 10.8,
      "canceled": 1
    }
  ],
  "total": 2740
}
```

Метрики: `views`, `clicks`, `ctr`, `cpc` (воронка показов) → `atbs`, `orders`, `shks`,
`cr`, `canceled` (корзина/заказы) → `sum` (расход на рекламу), `sum_price` (сумма заказов).
`total` — общее число строк за период (до пагинации).

```powershell
curl.exe -H "X-Api-Key: <КЛЮЧ>" `
  "http://localhost:3000/api/ext/v1/wb-advert-daily?date_from=2026-07-08&date_to=2026-07-14"
```

---

## Отчёт по платежам YM: `GET /api/ext/v1/ym-payment-report`

Строки отчёта по платежам Яндекс Маркета (`p907_ym_payment_report`, источник —
`POST /v2/reports/united-netting`) за период — в «сыром» native-виде YM: те же колонки, что
в CSV Маркета (`transaction_date`, `transaction_type`, `transaction_source`, `transaction_sum`,
`payment_status`, `order_id`, `shop_order_id`, `order_creation_date`, `order_delivery_date`,
`shop_sku`, `offer_or_service_name`, `count`, `act_id`, `act_date`, `bank_order_id`,
`bank_order_date`, `bank_sum`, `claim_number`, `comments`, …). Полный аналог
`wb-finance-report` — та же авторизация, те же параметры, та же форма ответа.

| Параметр        | Обяз. | Описание                                                        |
|-----------------|-------|-----------------------------------------------------------------|
| `date_from`     | да    | Начало периода по `transaction_date`, `YYYY-MM-DD`              |
| `date_to`       | да    | Конец периода по `transaction_date`, `YYYY-MM-DD` (включительно)|
| `connection_id` | нет   | Фильтр по кабинету YM (= `connection_mp_ref`)                   |
| `limit`         | нет   | Макс. строк (по умолчанию 5000, потолок 20000)                  |
| `offset`        | нет   | Смещение для постраничной выгрузки                              |

Без `date_from`/`date_to` → `400`. Порядок стабилен (`transaction_date`, затем `record_key`),
поле `total` — общее число строк за период.

### Кабинеты YM (`connection_id`)

| Кабинет      | `connection_id`                        |
|--------------|----------------------------------------|
| YM СТС       | `1ce94c09-e2b1-46c7-aad5-a597e8911cef` |
| YM Vannika   | `47e2ce51-e188-449f-978a-a1c012e22b83` |

Без параметра в ответ попадают все кабинеты; поля `connection_mp_ref` и `organization_ref`
есть в каждой строке, так что фильтровать можно и на стороне Power BI.

### Дополнительные поля

К native-колонкам YM добавлены:

- `connection_mp_ref` / `organization_ref` — кабинет и организация (отчёт YM строится на
  уровне бизнеса и сам магазин не различает);
- `record_key` — устойчивый ключ строки, собранный при импорте из номера заказа, даты, типа
  операции, товара и суммы (собственного идентификатора строки Маркет не даёт). Годится как
  ключ таблицы в BI;
- `loaded_at_utc` — момент последней загрузки строки. Полезен: Маркет дозаполняет
  `bank_order_id` и `act_id` спустя недели, а фильтра «изменённые с» у отчёта нет, поэтому
  «свежесть» строки видна только по этому полю.

Внутренние поля связи (`id`, `marketplace_product_ref`, `marketplace_order_ref`,
`nomenclature_ref`, `payload_version`) наружу не отдаются.

```powershell
curl.exe -H "X-Api-Key: <КЛЮЧ>" `
  "http://localhost:3000/api/ext/v1/ym-payment-report?date_from=2026-06-01&date_to=2026-06-30&limit=5000&offset=0"
```

Ответ: `{ "items": [ <native-строки YM> ], "total": …, "limit": …, "offset": … }`.

> **Периодичность строк неоднородна**: выплаты недельные, акт услуг — месячный. Сопоставлять
> строки отчёта с продажами день в день нельзя. Подробности — статья базы знаний
> `app__ym-api-united-netting`.

