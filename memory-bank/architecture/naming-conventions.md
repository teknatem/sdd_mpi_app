# Соглашения об именовании

> **Что где лежит.** Этот файл объясняет *почему* конвенции такие и как ими
> пользоваться. Машиночитаемая версия тех же правил — `architecture.toml`
> в корне (её читают генераторы и внешний анализатор). Актуальный перечень
> самих объектов — `ARCHITECTURE.md`, он генерируется из кода.
> Правило: **факт — в `architecture.toml`, список — в `ARCHITECTURE.md`,
> объяснение — здесь.** Дублировать список объектов сюда нельзя: он разъедется.

> **Сверено с кодом 2026-08-17.** До этого документ описывал структуру
> frontend-части usecase как `mod.rs` + `widget.rs` + `monitor.rs` — таких
> файлов не существовало ни в одном usecase, и расхождение прожило долго
> именно потому, что проверить его было нечем. Теперь состав ролей записан
> в `architecture.toml` числами вида «43/43», и разойтись молча ему сложнее.

## Индексы сущностей

Код объекта = префикс + номер. Зная код, можно прыгнуть сразу в файл.
Диапазоны и regex — в `architecture.toml`, раздел `[indices]`.

| Префикс | Что это | Диапазон |
|---|---|---|
| `aNNN` | Агрегат (домен-сущность/документ) | a001–a499 |
| `uNNN` | Use-case (импорты, репост) | u501–u999 |
| `p9NN` | Проекция (производная read-модель) | p900–p999 |
| `d4NN` | Dashboard | d400–d499 |
| `dsNN` | Базовая схема данных | ds01–ds99 |
| `dvNNN` | DataView | dv001–dv999 |
| `taskNNN` | Регламентное задание | task001–task999 |

## Агрегаты (`aNNN_snake_case_name`)

Примеры: `a001_connection_1c`, `a002_organization`, `a003_counterparty`.

**Состав ролей** (доли — замер по 43 агрегатам, см. `[roles.aggregate.*]`):

```
crates/
├── contracts/src/domain/a001_connection_1c/     ← все 4 роли обязательны
│   ├── mod.rs
│   ├── aggregate.rs          структура агрегата (DTO)
│   ├── metadata.json         описание полей — ЕДИНСТВЕННОЕ, что правится руками
│   └── metadata_gen.rs       генерируется build.rs из metadata.json
├── backend/src/domain/a001_connection_1c/
│   ├── mod.rs                ─┐
│   ├── repository.rs          ├ обязательны (43/43)
│   ├── service.rs            ─┘
│   ├── representation.rs      опционально (14/43) — участие в drilldown
│   ├── posting.rs             опционально (11/43) — проведение в Главную книгу
│   └── change_token.rs        опционально (4/43)  — инвалидация кэша фронта
└── frontend/src/domain/a001_connection_1c/
    ├── mod.rs                 обязателен (43/43)
    └── ui/                    41/43 — у a034 и a035 UI пока целиком в mod.rs
        ├── list/
        └── details/
```

Файл вне этого списка (например `a004_nomenclature/excel_import.rs`) —
**не нарушение**. Это законный доменный сервис; `architecture.toml` прямо
просит инструменты не помечать такое как `misplaced_path`.

**Таблица БД** именуется по агрегату:

```sql
CREATE TABLE a001_connection_1c_database (
    id TEXT PRIMARY KEY NOT NULL,
    code TEXT NOT NULL DEFAULT '',
    description TEXT NOT NULL,
    ...
);
```

## Use-cases (`uNNN_snake_case_name`)

Фактические: `u501_import_from_ut`, `u502_import_from_ozon`,
`u503_import_from_yandex`, `u504_import_from_wildberries`,
`u505_match_nomenclature`, `u506_import_from_lemanapro`,
`u507_import_from_erp`, `u508_repost_documents`.

```
crates/
├── contracts/src/usecases/u501_import_from_ut/
│   ├── mod.rs
│   ├── request.rs
│   ├── response.rs
│   ├── events.rs
│   └── progress.rs
├── backend/src/usecases/u501_import_from_ut/
│   ├── mod.rs                ─┐
│   ├── executor.rs            ├ обязательны (8/8)
│   ├── progress_tracker.rs   ─┘
│   ├── <источник>_client.rs   напр. ut_odata_client.rs, wildberries_api_client.rs
│   └── processors/            опционально (3/8)
└── frontend/src/usecases/u501_import_from_ut/
    ├── mod.rs                ─┐
    ├── view.rs                ├ обязательны (8/8)
    └── api.rs                ─┘
```

> `api.rs` во фронте назывался двумя именами — `api.rs` (5 срезов) и `ops.rs`
> (3 срезa) для одной и той же роли. Приведено к `api.rs` 2026-08-17: два имени
> одной роли заставляют ветвиться и человека, и любой ролевой словарь.

**Своих таблиц у use-case нет.** Прогресс и события живут в памяти процесса
(`progress_tracker.rs`), история импортов — в `sys_task_*` через планировщик.
Общей таблицы `usecase_events` в проекте не существует.

## Проекции (`p9NN_snake_case_name`)

```
backend/src/projections/p904_sales_data/
├── mod.rs                     ─┐ обязательны (17/17)
├── repository.rs              ─┘
├── service.rs                  опционально (12/17)
├── builder.rs                  опционально (7/17) — построение самой проекции
└── general_ledger_builder.rs   опционально (2/17) — проводки в Главную книгу
```

> `builder.rs` тоже назывался двумя именами (`builder.rs` / `projection_builder.rs`).
> Приведено к `builder.rs` 2026-08-17. Префикс `projection_` был избыточен —
> каталог `projections/p9NN_*` и так говорит, что это проекция, — а рядом стоит
> `general_ledger_builder.rs`, и пара «builder / general_ledger_builder»
> читается как «строитель проекции / строитель проводок», что и есть правда.

## Раскладка модулей: `foo.rs` или `foo/mod.rs`

**Обе формы нормальны, это не разнобой.** Замер: 105 каталогов содержат только
`mod.rs`, 1166 модулей — простые файлы.

Правило: модуль остаётся файлом, пока у него нет подмодулей; каталог с `mod.rs`
появляется, когда они заводятся — или когда каталог держат ради позиционной
роли имени (`ui/list/`, `ui/details/`: роль здесь несёт путь, а не имя файла).

Приводить одну форму к другой смысла нет — это выбор раскладки, а не имя роли.

## API endpoints

**Единой формы URL в проекте нет**, и это оставлено осознанно
(`architecture.toml`, `[conventions.api_paths]`). Замер:

| Форма | Путей | Пример |
|---|---|---|
| `/api/<имя>` | 150 | `/api/connection_1c/:id` |
| `/api/aNNN/<имя>` | 149 | `/api/a012/wb-sales/:id` |
| `/api/<код>/…` | 80 | `/api/d401/configs`, `/api/p915/order-events` |
| `/api/aNNN-<имя>` | 69 | `/api/a017-llm-agent/:id` |

Почему не приводим к одной: URL — публичный контракт. Его читают фронт,
внешний API (`/api/ext/v1/*`), 1С и Power BI. Переименование ради красоты
ломает работающих потребителей.

**Новый эндпоинт заводится в форме `/api/aNNN/<имя>`** — самой частой среди
недавних и однозначно связывающей путь со срезом.

Use-cases единообразны:
```
POST /api/u501/import/start
GET  /api/u501/import/:session_id/progress
```

## Зачем индексы

1. **Явное разделение** — сразу видно агрегат (`a*`), проекцию (`p*`), операцию (`u*`).
2. **Навигация** — код объекта ведёт прямо в каталог, без grep'а.
3. **Изоляция в БД** — таблицы `a001_*` не пересекаются с `p9*`.
4. **Генерация** — `ARCHITECTURE.md` и метаданные строятся по индексам автоматически.

## Контрольный список при создании нового use-case

- [ ] `contracts/usecases/uNNN_name/` — Request/Response DTO, события, progress
- [ ] `backend/usecases/uNNN_name/` — `executor.rs` + `progress_tracker.rs`
- [ ] Реализовать `UseCaseMetadata` (`contracts/src/usecases/common/usecase_metadata.rs`)
- [ ] Роуты в **`backend/src/api/routes.rs`** (не в `main.rs`)
- [ ] **Запись в `ROUTE_REGISTRY`** (`system/access/route_registry.rs`) — иначе
      тест `every_declared_route_has_a_policy_entry` завалит сборку
- [ ] `frontend/usecases/uNNN_name/` — `view.rs` + `api.rs`
- [ ] Миграция БД, если нужны таблицы (`migrations/NNNN_имя.sql`)
- [ ] Регенерировать `ARCHITECTURE.md` (pre-commit hook делает это сам)
