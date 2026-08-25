---
kind: app
title: Raw JSON storage — отладочное хранилище API payload
tags: [raw_json, raw_storage, sys_raw_storage, debug, document_raw_storage, vacuum, sqlite, cleanup, gc, marketplace-api]
related: [a010_ozon_fbs_posting, a011_ozon_fbo_posting, a012_wb_sales, a013_ym_order, a015_wb_orders, a016_ym_returns, a020_wb_promotion, a029_wb_supply]
updated: 2026-07-09
---

# Raw JSON storage — отладочное хранилище API payload

Отдельная таблица `document_raw_storage`, в которую опционально сохраняется исходный (сырой)
JSON-ответ API маркетплейса при импорте документа — до нормализации в доменный агрегат.
Нужна для отладки маппинга/парсинга, когда непонятно, откуда взялось значение поля.
Не является частью бизнес-модели: агрегаты (a010, a012, a013, a015 и т.д.) от неё не зависят,
они лишь опционально ссылаются на строку через `raw_payload_ref` в `source_meta_json`.

Управляется через страницу **Система → Raw JSON** (код страницы `sys_raw_storage`,
только для админов, `RequireAdmin`). Backend-код: `crates/backend/src/shared/data/raw_storage.rs`
(модель + бизнес-логика) и `crates/backend/src/system/api/handlers/raw_storage.rs` (HTTP-обработчики).
Frontend: `crates/frontend/src/system/raw_storage/ui/mod.rs`.

## Захват (capture) {#capture}

- Управляется настройкой `sys_settings.raw_json_capture_enabled` (по умолчанию `false` —
  в штатном режиме raw JSON не пишется).
- Значение кэшируется в процессе backend'а через `AtomicBool` (`RAW_CAPTURE_ENABLED` /
  `RAW_CAPTURE_LOADED`), чтобы не ходить в БД на каждый импорт. Переключение чекбокса
  на странице (`POST /api/sys/raw-storage/settings`) обновляет и настройку в БД, и кэш
  в памяти — действует сразу, без рестарта backend'а.
- Включать стоит только на время расследования конкретной проблемы с маппингом —
  каждый API-ответ маркетплейса может быть большим, и таблица быстро растёт.

## Кто пишет raw JSON {#writers}

Вызывают `save_raw_json(marketplace, document_type, document_no, raw_json, fetched_at)` при импорте:

- `a010_ozon_fbs_posting`, `a011_ozon_fbo_posting` — Ozon FBS/FBO
- `a012_wb_sales` — WB продажи/возвраты
- `a013_ym_order` — заказы Яндекс.Маркет
- `a015_wb_orders` — заказы WB (два источника: обычный `raw_payload_ref` и
  `marketplace_raw_payload_ref` для карточки Marketplace API; карточка Marketplace API
  есть только у FBS-заказов, у FBW и отменённых FBS её нет по дизайну)
- `a016_ym_returns` — возвраты Яндекс.Маркет
- `a020_wb_promotion` — акции WB
- `a029_wb_supply` — поставки WB

Функция дедуплицирует по SHA-256 хэшу тела (`raw_hash`): если для той же тройки
(`marketplace`, `document_type`, `document_no`) уже есть строка с идентичным хэшем,
новая запись не создаётся — переиспользуется существующий `id` (это и есть `raw_payload_ref`,
который агрегат сохраняет у себя в `source_meta_json`). Если capture выключен,
`save_raw_json` сразу возвращает `None`, ничего не пишет и не трогает БД.

## Просмотр raw JSON конкретного документа {#viewing}

Каждый из перечисленных выше обработчиков документов (a010/a012/a013/a015/a016/a020/a029)
имеет свой HTTP-эндпоинт, который вызывает `get_json_value_by_ref(ref_id)` и отдаёт
сырой JSON во frontend (обычно вкладка «Raw»/«Отладка» в деталях документа). Если строка
была удалена очисткой на этой странице (или capture никогда не был включён), вместо
ошибки возвращается плейсхолдер:
```json
{ "raw_not_available": true, "message": "Raw JSON не сохранен. Включите debug capture..." }
```
Это ожидаемое поведение, а не баг — raw JSON всегда опционален.

## Referenced / Unreferenced {#referenced}

«Referenced» — строки `document_raw_storage`, на `id` которых ссылается хотя бы один
документ через `raw_payload_ref`/`marketplace_raw_payload_ref` в `source_meta_json`
(или `raw_payload_ref` напрямую в колонке — для `a020_wb_promotion`). Список источников
собирается SQL CTE (`REFERENCED_REFS_CTE` в `raw_storage.rs`) через `UNION ALL` по всем
семи таблицам агрегатов. «Unreferenced» — всё остальное: обычно старые снапшоты после
повторного импорта того же документа (raw_hash изменился → новая строка, старый ref уже
никто не держит) либо документ, который был удалён.

Раздел «Состояние» на странице показывает: `Строк` (total_rows), `Raw JSON` (total_mb —
суммарный объём в МБ), `Referenced`, `Unreferenced`. Плюс разбивка `by_type`
(marketplace × document_type: строк и объём) в таблице «По типам» внизу страницы.

## Очистка (cleanup) {#cleanup}

Четыре режима (`RawStorageCleanupMode` в `contracts::system::raw_storage`), UI всегда
сначала показывает precision-preview (`POST /cleanup/preview`, кол-во строк + оценка МБ)
с `window.confirm`, и только после подтверждения реально удаляет (`POST /cleanup`):

- **Unreferenced** — `id NOT IN (SELECT ref FROM clean_refs)`. Безопасно: не трогает
  строки, на которые кто-то ссылается.
- **Duplicates** — точные дубли одного документа (совпадают marketplace+document_type+
  document_no+raw_hash), оставляет одну копию (referenced приоритетнее, иначе
  самую свежую по `created_at`), и тоже исключает referenced-строки из удаления.
- **OlderThanDays** — `created_at < now() - N дней`, тоже с исключением referenced-строк.
- **All** — **без всяких исключений**, буквально `DELETE FROM document_raw_storage`.
  Это единственный режим, который может удалить referenced-строки — после него
  просмотр raw JSON у любого документа начнёт отдавать `raw_not_available` плейсхолдер,
  пока документ не будет переимпортирован. На странице это подсвечено как опасное
  действие (`button--danger`, красная строка списка).

## VACUUM {#vacuum}

Отдельная секция — обслуживание всего файла SQLite, не только этой таблицы.
`DELETE`/`UPDATE` в SQLite не уменьшают файл на диске — освобождённые страницы
помечаются в freelist и переиспользуются под новые записи. `VACUUM` пересобирает
файл целиком и физически возвращает место ОС.

- `vacuum_status()` читает `PRAGMA page_count/freelist_count/page_size` — даёт
  `file_mb` (текущий размер файла) и `reclaimable_mb` (сколько освободит VACUUM,
  копится от изменений во **всех** таблицах БД, не только raw storage).
- `vacuum()` выполняет `VACUUM`, меряет длительность и файл до/после.
- Держит запись занятой на всё время выполнения — другие писатели (в т.ч. фоновые
  задания-импортёры) встанут в очередь на `busy_timeout`. UI явно предупреждает
  через `window.confirm` — выполнять вне рабочего времени/пиковой нагрузки.

## API routes {#api}

| Метод | Путь | Назначение |
|---|---|---|
| GET | `/api/sys/raw-storage/status` | Состояние + разбивка по типам |
| POST | `/api/sys/raw-storage/settings` | Включить/выключить capture |
| POST | `/api/sys/raw-storage/cleanup/preview` | Превью очистки (без удаления) |
| POST | `/api/sys/raw-storage/cleanup` | Выполнить очистку |
| GET | `/api/sys/raw-storage/vacuum` | Статус файла БД / реклеймable |
| POST | `/api/sys/raw-storage/vacuum` | Выполнить VACUUM |

## Pitfalls {#pitfalls}

- Capture по умолчанию выключен и должен оставаться выключенным вне сессий отладки —
  это не механизм аудита и не замена событийного лога.
- «Все raw» (`All`) — единственная операция, которая ломает просмотр raw JSON
  у ещё живых документов; остальные режимы безопасны для referenced-строк.
- `reclaimable_mb` не привязан к Raw JSON — это общий показатель по всей БД,
  большое значение может означать чистку совсем других таблиц, не только
  `document_raw_storage`.
- Изменение capture-настройки не ретроактивно: включение не досоздаёт raw JSON для
  документов, импортированных пока capture был выключен.
