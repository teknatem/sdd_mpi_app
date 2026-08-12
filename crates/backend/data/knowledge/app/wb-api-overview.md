---
title: WB API — обзор, карта эндпоинтов и расхождения
uid: kb-b0000001
status: active
kind: app
tags: [wildberries, api, интеграция, импорт, лимиты, кабинет]
related: [app__wb-api-statistics-sales, app__wb-api-statistics-orders, app__wb-api-finance-report, app__wb-api-advert-fullstats, app__wb-api-funnel-history, app__wb-api-search-analytics, app__wb-api-marketplace-orders, app__wb-api-content-cards, app__wb-api-returns-claims, wildberries]
aliases: [какие эндпоинты wb мы используем, карта api wildberries, лимиты wb, backlog wb api]
summary: Карта всех используемых эндпоинтов Wildberries с хостами, лимитами, расписанием и сводным списком расхождений между документацией, практикой и кодом.
stars: 5
ttl_days: 90
updated: 2026-08-11
verified: 2026-08-11
author: human
---

# WB API — обзор, карта эндпоинтов и расхождения

Wildberries раздаёт данные не одним API, а десятком независимых сервисов на разных хостах, у
каждого своя модель лимитов и свои сроки хранения. Это главное, что нужно понимать про
интеграцию: «лимит WB» — величина не существующая, лимит всегда принадлежит конкретному хосту
и часто конкретному методу.

## Общие правила

**Авторизация.** Один seller-ключ на кабинет во всех сервисах, заголовок `Authorization: <ключ>`
— **без** префикса `Bearer`. Категории токена в личном кабинете WB реальны: ключ без нужной
категории даёт не 403, а другой код (для претензий — 404, см. `app__wb-api-returns-claims`).

**Транспорт.** Общий HTTP-клиент: таймаут 60 с, браузерный `User-Agent`, отключённый системный
прокси, до 10 редиректов. Аналитические методы переопределяют таймаут на 180 с — они считают
дольше, чем живёт дефолт. Все запросы и ответы пишутся в `wildberries_api_requests.log` рядом
с бинарником; это первый файл, куда смотреть при разборе.

**Заголовки лимитов.** WB возвращает `X-Ratelimit-Limit`, `X-Ratelimit-Remaining`,
`X-Ratelimit-Reset` и `X-Ratelimit-Retry`. Код их читает и подставляет в текст ошибки, поэтому
в логе видно не просто «429», а сколько именно ждать.

**Кабинеты.** Каждый кабинет — отдельная строка подключения a006 со своим ключом. Лимиты
считаются на продавца, поэтому задачи по разным кабинетам разведены по времени.

## Карта эндпоинтов

| Хост | Метод | Статья | Куда попадает |
|---|---|---|---|
| `common-api` | `GET /api/v1/seller-info` | `app__wb-api-connection` | — (проверка ключа) |
| `common-api` | `GET /api/v1/tariffs/commission` | `app__wb-api-tariffs-commission` | p905 |
| `seller-analytics-api` | `GET /ping` | `app__wb-api-connection` | — |
| `seller-analytics-api` | `POST /api/analytics/v3/sales-funnel/products` | `app__wb-api-funnel-products` | a036, a037 |
| `seller-analytics-api` | `POST /api/analytics/v3/sales-funnel/products/history` | `app__wb-api-funnel-history` | a036 → p916 |
| `seller-analytics-api` | `POST/GET /api/v2/nm-report/downloads*` | `app__wb-api-funnel-detail-report` | a036 |
| `seller-analytics-api` | `POST /api/v2/search-report/table/details` | `app__wb-api-search-analytics` | a040 |
| `seller-analytics-api` | `POST /api/v2/search-report/product/search-texts` | `app__wb-api-search-analytics` | a040 |
| `content-api` | `POST /content/v2/get/cards/list` | `app__wb-api-content-cards` | a007 |
| `statistics-api` | `GET /api/v1/supplier/sales` | `app__wb-api-statistics-sales` | a012 |
| `statistics-api` | `GET /api/v1/supplier/orders` | `app__wb-api-statistics-orders` | a015 |
| `statistics-api` | `GET /api/v5/supplier/reportDetailByPeriod` | `app__wb-api-finance-report` (legacy active) | p903 |
| `finance-api` | `POST /api/finance/v1/sales-reports/list`, `/detailed/{reportId}` | `app__wb-api-finance-reports-v1` | a043 |
| `marketplace-api` | `GET /api/v3/orders/new`, `GET /api/v3/orders` | `app__wb-api-marketplace-orders` | a015_wb_orders_new |
| `marketplace-api` | `GET /api/v3/supplies`, order-ids, stickers | `app__wb-api-marketplace-supplies` | a029 |
| `advert-api` | `GET /adv/v1/promotion/count`, `GET /api/advert/v2/adverts` | `app__wb-api-advert-campaigns` | a030 |
| `advert-api` | `GET /adv/v3/fullstats` | `app__wb-api-advert-fullstats` | a026 |
| `dp-calendar-api` | `GET /api/v1/calendar/promotions*` (3 метода) | `app__wb-api-promotions` | a020 |
| `discounts-prices-api` | `GET /api/v2/list/goods/filter` | `app__wb-api-prices` | p908 |
| `documents-api` | `GET /api/v1/documents/list`, `/download` | `app__wb-api-documents` | a027 |
| `returns-api` | `GET /api/v1/claims` | `app__wb-api-returns-claims` | a032 |

## Модель лимитов по хостам

От самого жёсткого к самому свободному:

| Лимит | Где | Как обходим |
|---|---|---|
| 5 запросов/с, интервал 200 мс, burst 5 | `promotion/count`, `advert/v2/adverts` | пачки ≤50 ID, последовательный интервал 250 мс, retry по headers |
| 1 запрос/мин | `reportDetailByPeriod`, `supplier/orders` | пауза 65 с после 429, пагинация курсором |
| 3 запроса/мин на метод | все `analytics/v3` и `nm-report` | фиксированная пауза 21 с между вызовами |
| ~1 запрос/10 с (наблюдаемое) | `documents/list` | пауза 11 с, 3 ретрая на страницу |
| 20 запросов/мин | `supplier/sales` | пауза 100 мс между страницами |
| 300 запросов/мин | `content/v2`, `marketplace-api` | без пауз |

Практика, общая для всех: при 429 код читает `X-Ratelimit-Retry`, и если ждать пришлось бы
дольше 300 с, запуск не блокируется — он завершается с маркером отложенности, а задача
переносится. Это сознательный выбор: висящий на час воркер хуже пропущенного запуска.

## Расписание

Cron в задачах записан в **UTC**, МСК = UTC+3.

| Задача | Cron (UTC) | МСК | Эндпоинты |
|---|---|---|---|
| task001 FBS-поллинг | `0 */5 * * * *` | каждые 5 мин | orders/new, orders |
| task002 заказы | `0 0 * * * *` | ежечасно | supplier/orders |
| task005 поставки | `0 0 * * * *` | ежечасно | supplies, order-ids, stickers |
| task006 финансы | `0 0 1 * * *` | 04:00 | reportDetailByPeriod |
| task007 комиссии | `0 0 2 * * 1` | пн 05:00 | tariffs/commission |
| task003 карточки | `0 0 3 * * *` | 06:00 | cards/list |
| task020 снимок товаров | `0 0 3,15 * * *` | 06:00, 18:00 | sales-funnel/products |
| task023 воронка | `0 30 3,15 * * *` | 06:30, 18:30 | sales-funnel/products + history |
| task004 продажи | `0 0 4 * * *` | 07:00 | supplier/sales |
| task024 поисковая аналитика | `0 0 4,16 * * *` | 07:00, 19:00 | search-report |
| task009 акции | `0 0 5 * * *` | 08:00 | calendar/promotions |
| task010 документы | `0 0 5 * * *` | 08:00 | documents/list |
| task012 рекламные кампании | `0 30 5 * * *` | 08:30 | promotion/count, adverts |
| task008 цены | `0 0 6,18 * * *` | 09:00, 21:00 | list/goods/filter |
| task011 реклама | `0 0 6 * * *` | 09:00 | fullstats |
| task017 претензии | не засеяна | — | claims |

Полчаса между task020 и task023 и разведённые по кабинетам экземпляры — не косметика:
у `analytics/v3` лимит 3 запроса/мин общий на кабинет, и одновременный старт двух задач по
одному кабинету гарантированно даёт 429.

> ⚠️ Планировщик выключен глобально (`[scheduled_tasks].enabled = false`): расписание описывает
> намерение, а не то, что происходит на этой машине.

## Сводка расхождений — вход для доработок

Подробности и статус достоверности — в соответствующих статьях.

| Что | Расхождение | Статус |
|---|---|---|
| `supplier/orders` | глубина хранения не документирована; за её пределами WB молча отдаёт самые старые строки вместо ошибки | подтверждено |
| `documents/list` | `beginTime`/`endTime` не фильтруют — отбор делается на нашей стороне | подтверждено |
| `adv/v3/fullstats` | интервал >31 дня отвергается с `max date range 31 days`, в документации лимит не выделен | подтверждено |
| `advert/v2/adverts` | наблюдаемые 429 возможны и при последовательной загрузке; неудачная пачка не стирает `info_json` | подтверждено |
| `content/v2/get/cards/list` | `cursor.total` — размер страницы, а не каталога; без `withPhoto: -1` выдача урезается | подтверждено |
| `search-report/*` | имена полей ответа не верифицированы вживую, парсинг толерантный | требует проверки |
| `search-report/table/details` | отдаёт видимость в %, счётчика показов нет → показы WB остаются N/A | подтверждено |
| `claims` | клиент ходит на `returns-api`, метаданные задачи и контракт a032 говорят `feedbacks-api` | требует проверки |
| `sales-funnel/products/history` | окно ~7 дней, за его пределами дни приходят пустыми | подтверждено |
| `supplier/sales` | `priceWithDisc` и `finishedPrice` временно бывают нулевыми, `forPay` считается упрощённо | подтверждено |

## Официальная документация

- Портал разработчика: `https://dev.wildberries.ru/openapi/`
- Отчёты и статистика: `https://dev.wildberries.ru/openapi/reports`
- Карточки товаров: `https://dev.wildberries.ru/openapi/work-with-products`
- Цены и скидки: `https://dev.wildberries.ru/openapi/prices-and-discounts`
- Тарифы: `https://dev.wildberries.ru/openapi/tariffs`
- Акции: `https://dev.wildberries.ru/openapi/promotion`
