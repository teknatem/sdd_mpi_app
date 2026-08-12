---
title: WB Finance API v1 — ежедневные финансовые отчёты a043
uid: kb-b0000018
status: active
kind: app
tags: [wildberries, api, финансы, отчёт, a043, лимиты]
related: [app__wb-api-overview, app__wb-api-finance-report, a043, wildberries]
aliases: [sales-reports/list, sales-reports/detailed, finance api wb, task030]
summary: Новый независимый контур WB Finance API: список ежедневных отчётов → полная детализация → a043 → read-only UI и task030; проекций и проводок нет.
stars: 5
ttl_days: 90
updated: 2026-08-11
verified: 2026-08-11
author: human
---

# WB Finance API v1 — ежедневные финансовые отчёты a043

Независимый от legacy `p903/task006` контур. Одна запись `a043` соответствует одному
`reportId`; полного переноса, сравнения или удаления старых документов нет.

## Эндпоинты и пагинация

1. `POST https://finance-api.wildberries.ru/api/finance/v1/sales-reports/list` —
   `period=daily`, `limit=1000`, страницы через `offset`.
2. `POST https://finance-api.wildberries.ru/api/finance/v1/sales-reports/detailed/{reportId}` —
   `limit=100000`, первый `rrdId=0`, затем ID последней строки; нормальное завершение только
   по HTTP 204. Пустой первый ответ 204 означает отчёт без строк.

Числовой либо строковый `reportId` сохраняется строкой. Денежные значения заголовка также
сохраняются десятичными строками. Полные исходные объекты лежат в `header_json` и
`lines_json`, поэтому новые неизвестные поля WB не теряются.

## Надёжность и лимиты

Официальный лимит — один вызов в минуту. Все list/detail-вызовы одного кабинета проходят
через общий последовательный gate с интервалом 61 с. При 429 учитываются `Retry-After` и
`X-Ratelimit-*`, без заголовков — 65 с, максимум три повторные попытки.

Нерастущий или повторяющийся `rrdId` завершает импорт ошибкой. Документ заменяется только
после полной загрузки детализации, поэтому ошибка промежуточной страницы не повреждает
предыдущую версию. Доступны периоды начиная с 2025-01-01.

## Цепочка потребителей

`sales-reports/list` → `sales-reports/detailed/{reportId}` → **a043** → read-only API/UI и
ручной импорт u504 / отключённая по умолчанию `task030_wb_finance_reports` (окно 35 дней).

> **Проекций, проводок GL, сверок и интеграции с u508 нет.** Это отдельный будущий этап.

## Официальная документация

- `https://dev.wildberries.ru/docs/openapi/financial-reports-and-accounting`
