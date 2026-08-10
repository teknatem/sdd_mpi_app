# CLAUDE.md

Гид для Claude Code по проекту **leptos_marketplace_1** — десктопная система управления маркетплейсами (1С:УТ 11, Wildberries, Ozon, Yandex Market) на Rust/Leptos.

> **Источник истины — код.** При расхождениях приоритет:
> `код` > `CONTEXT.md` > `.claude/memory/` > `memory-bank/` > `docs/`.
> Прежде чем опираться на доку, проверь, что файл/функция/флаг ещё существуют в коде.

> **Язык домена — `CONTEXT.md`** (в корне): глоссарий терминов предметной области — что означают
> «слой учёта», «репост», «кабинет», «видимость», чем `dsXX` отличается от `dvXX`, а `a017` от `a038`.
> Только определения, без деталей реализации. Правится вручную по мере разбора домена
> (в т.ч. скиллом `grill-with-docs`); при расхождении с кодом прав код.

> **Карта объектов — `ARCHITECTURE.md`** (генерируется из кода): полный каталог агрегатов
> a0XX (с описаниями из metadata.json), проекций, use-cases, плана счетов, видов оборотов
> и всех API-роутов. Читай его, чтобы найти нужный объект, **не grep'ая** исходники.
> Регенерация после изменений: `powershell -File tools/gen_architecture.ps1`.
> Авто-обновление: git pre-commit hook (`tools/hooks/`) регенерирует карту, когда в коммит
> попадают domain/projections/usecases/реестры/`routes.rs`. После свежего клона включи один раз:
> `git config core.hooksPath tools/hooks`.

---

## Сборка и запуск

Dev (два терминала, из корня):
```powershell
cargo run -p backend          # Axum API на http://localhost:3000
trunk serve --port 8080       # Leptos/WASM фронт на http://localhost:8080 (проксирует API на :3000)
```

> Уже запущенный backend.exe держит `target\debug\backend.exe` — повторный `cargo run`
> падает с «Access is denied». Перезапуск: `powershell -File tools/restart_backend.ps1`
> (останавливает процесс и запускает свежую сборку).

Проверка перед коммитом:
```powershell
cargo check -p backend
cargo check -p contracts
cargo check -p frontend --target wasm32-unknown-unknown   # frontend — только wasm-таргет
cargo test -p backend router_builds   # после правок роутов: конфликт путей axum виден только при сборке Router
```

> **Экономь сборки — компиляция здесь дороже всего** (backend test ≈ 3 мин, wasm-check ≈ 1 мин):
> - **Не гоняй wasm-тесты** (`cargo test -p frontend --target wasm32-…`): в этом окружении нет
>   wasm-раннера — бинарь соберётся за ~10 мин и упадёт на запуске (`os error 193`). Чистую логику
>   фронта проверяй компиляцией, а сами тесты пиши так, чтобы исполнялись нативно (или тестируй в
>   `contracts`/`backend`).
> - **Один прогон на крейт после всех правок в нём**, а не после каждой под-области.
> - **Не смешивай профили `check` и `test`** — они собираются раздельно, т.е. проект компилируется
>   дважды. `cargo test` уже включает сборку → если запускаешь тесты, отдельный `cargo check` не нужен.
> - **Объединяй тест-фильтры в один запуск** (несколько фильтров сразу), чтобы не платить за
>   инкрементальную пересборку повторно.
> - **Разведку соразмеряй с задачей**: при живых CLAUDE.md + `.claude/memory/` предпочитай точечные
>   Grep/Read; параллельных Explore-агентов береги для действительно широкого неизвестного скоупа.

Release:
```powershell
trunk build --release                      # → dist/ (фронт)
cargo build --release --bin backend        # → target/release/backend.exe
```

Заметки по профилям (`Cargo.toml`): dev-сборка без оптимизаций ради скорости; `adobe-cmap-parser` собирается без overflow-checks (иначе паника при извлечении PDF в dev).

---

## Крейты (workspace)

| Крейт | Роль |
|---|---|
| `crates/backend` | Axum-сервер, бизнес-логика, БД (SQLite + SeaORM), проекции, главная книга |
| `crates/frontend` | Leptos/WASM SPA (Trunk), thaw UI + кастомный BEM/CSS |
| `crates/contracts` | Общие DTO, определения агрегатов, metadata — разделяемы между фронтом и бэком |

Все три крейта зеркалят одну структуру слоёв: `domain/`, `projections/`, `general_ledger/`, `dashboards/`, `quality/`, `system/`, `shared/`, `usecases/`.

---

## Схема именования (ключ к навигации)

Код объекта = префикс + номер. Зная код, прыгай сразу в файл — не ищи.

| Префикс | Что это | Где (backend) | Диапазон |
|---|---|---|---|
| `a0XX` | **Агрегат** (домен-сущность/документ) | `domain/a0XX_*` | a001–a035 |
| `p9XX` | **Проекция** (производная read-модель) | `projections/p9XX_*` | p900–p915 |
| `u5XX` | **Use-case** (импорты, репост) | `usecases/u5XX_*` | u501–u508 |
| `dsXX` | **Базовая схема данных** (роль *base schema*, движок universal_dashboard; UI: «Схемы таблиц») | `data_schemes/dsXX_*` | ds01–ds03 |
| `dvXX` | **DataView** (роль *виртуальная таблица*: курируемые метрики, 2 периода, кэш; UI: «DataView») | `data_view/dvXXX_*` | dv001–dv007 |
| `d4XX` | **Dashboard** (готовый дашборд — *потребитель* слоя) | `dashboards/d4XX_*` | d400–d405 |
| `task0XX` | **Запланированная задача** (поллинг/импорт) | `system/tasks/managers/task0XX_*` | task001+ |

Примеры: a013 = YM order, a015 = WB orders, a034 = YM realization; p904 = sales_data, p907 = YM payment report; u503 = import from Yandex; ds01–ds03 = базовые схемы для «Конструктора запросов» (ds01→p903, ds02→p900, ds03→p904). d400–d405 = готовые дашборды (d400 сводка за месяц, d401 WB Finance, d402/d403 история заказов WB/YM, d404 отчёт по рекламе WB, d405 метаданные) — это **потребители** слоя, НЕ схемы (прежняя пометка «ds01/ds02 доступны как d401/d402» была неверной; коллизия двух d401 устранена — метаданные перенесены на d405).

## Слой данных: три роли источников (см. `memory-bank/decisions/ADR-0010-data-source-roles.md`)

Доступ к аналитическим данным — три независимых движка с разными ролями (выбирай по дереву):
- **DataView `dvXX`** (`data_view/`) — курируемые «виртуальные таблицы»: составные метрики, **2 периода**, кэш. Для благословлённых показателей и BI (a024/a025). Сложные метрики (revenue = customer_in+customer_out, GL turnover CASE) живут здесь.
- **Базовая схема `dsXX`** (`data_schemes/` + движок `shared/universal_dashboard/`; UI: «Схемы таблиц») — декларативное описание таблицы; гибкий ad-hoc (группировки/фильтры/агрегаты) через `QueryBuilder`. Governance по построению (поля — allowlist).
- **Сырой SQL** (`execute_query`) — нестандартные/разовые случаи; укреплённый escape-hatch.

Перекрытие источников по одной таблице допустимо только при разных ролях (напр. `p904`: ds03 — гибкий, dv001 — курируемый 2-периодный). UI-инструменты слоя собраны в sidebar-группе «Источники данных».

**Термины код ↔ UI** (код-идентификаторы не меняются, в интерфейсе свои подписи): `dsXX` → «Схемы таблиц» (каталог) + «Конструктор запросов» (построитель); `dvXX` → «DataView»; сайдбар-группа `semantic_layer` → «Источники данных».

---

## Внутренняя структура агрегата `a0XX`

**Backend** (`crates/backend/src/domain/a0XX_*/`):
- `mod.rs` — сборка модуля
- `repository.rs` — доступ к БД (SeaORM)
- `service.rs` — бизнес-логика, в т.ч. `insert_test_data`
- `posting.rs` — проведение в Главную книгу / проекции (есть не у всех)
- `representation.rs` — представление агрегата (title+date+doc_id) для drilldown
- `change_token.rs` — инвалидация кэша/реактивность

**Contracts** (`crates/contracts/src/domain/a0XX_*/`):
- `aggregate.rs` — структура агрегата (DTO)
- `metadata.json` + `metadata_gen.rs` — система метаданных полей (генерируемая)

**Frontend** (`crates/frontend/src/domain/a0XX_*/`): UI по MVVM, страницы details со вкладками — напр. `ui/details/tabs/general.rs`.

### Метаданные: `metadata.json` → генерация

**Единый стандарт без исключений: у каждого агрегата, каждой проекции и Главной книги есть
`metadata.json`.** `crates/contracts/build.rs` сканирует `src/domain`, `src/projections` и
`src/general_ledger` одинаково: рядом с DTO кладётся `metadata.json`, из него генерируется
`metadata_gen.rs` (правится только JSON). Модуль подключается конвенционально:
`mod metadata_gen; pub use metadata_gen::{ENTITY_METADATA, FIELDS};`.

Все найденные сущности build-скрипт собирает в `src/shared/metadata/registry_gen.rs`
(`ALL_ENTITIES`) — **регистрировать сущность где-либо руками не нужно и нельзя**.
Блок `ai` в metadata.json задаёт поведение LLM-каталога:
- `"tags": ["wb", "sales"]` — тематические теги для фильтра;
- `"related": [...]` — только зарегистрированные сущности (тест `related_targets_resolve_…`);
- `"llm_visible": false` — скрыть сущность от LLM. Механизм есть, действующих исключений нет.

Поля описывают **реальные колонки таблицы**. У документов часть структуры лежит внутри
JSON-колонок — такие логические поля помечаются `"physical": false`, чтобы не попадать в
`columns_for_sql` (иначе LLM генерирует SQL с несуществующими колонками). Сами JSON-колонки
описаны как обычные поля — по ним работает `json_extract`.

Потребитель — `backend/src/shared/llm/metadata_registry.rs`. Сущность разрешается по любому из
своих имён (индекс `a001`, каталог `a001_connection_1c`, таблица `a001_connection_1c_database`,
коллекция) — одно правило `names_match` на поиск и на JOIN-подсказки. Невалидные `field_type`/
`source`/`entity_type` в JSON падают при сборке с внятным сообщением, а не компиляционной ошибкой
внутри сгенерированного файла.

---

## Карта бэкенда (`crates/backend/src/`)

| Модуль | Назначение |
|---|---|
| `api/routes.rs` | Все HTTP-маршруты (единый файл) |
| `api/handlers/` | Обработчики по объектам |
| `domain/` | Агрегаты a0XX |
| `projections/` | Проекции p9XX + `projections/general_ledger/` |
| `general_ledger/` | Главная книга: `account_registry`, `turnover_registry`, `report_repository`, `drilldown_*`, `account_view/`, `service.rs` |
| `data_schemes/` | dsXX — схемы для universal dashboard |
| `dashboards/` | d4XX |
| `quality/checks/` | Quality-checks (популяция/нарушения/доля) |
| `system/` | `auth`, `access`, `roles`, `users`, `tasks` (планировщик), `audit`, `history`, `favorites`, `settings`, `middleware`, `initialization` |
| `shared/` | `analytics` (account/turnover registry, нормализация, wb_mapping), `indicators` (BI compute), `llm`, `marketplaces`, `representation`, `universal_dashboard`, `drilldown`, `format`, `config` |
| `bi_timeline/` | BI timeline |

---

## Учётные слои (Главная книга)

GL — скелет финансовой модели. Поверх неё концептуальные слои учёта (см. `.claude/memory/`):
- **fact / fina / ybuh** — параллельные слои оборотов для сверки выручки (fina заменяет fact для p903/p907; ybuh — официальные отчёты о реализации, напр. a034).
- `p914` — зеркало GL-проводок слоя fina; `p907` — YM payment report.
- Регистры: `account_registry` (план счетов), `turnover_registry` (обороты).
- После правок в маппинге/проводках нужен **репост документов** через `u508`.

Детали бизнес-правил YM/WB/GL — в `.claude/memory/MEMORY.md` (подгружается автоматически каждую сессию). Не дублируй их здесь.

---

## Конвенции

- Rust 2021, Leptos 0.8, SQLite (SeaORM), фронт — WASM.
- **Миграции БД** — SQL-файлы `migrations/NNNN_имя.sql`, применяются автоматически при старте бэкенда (`shared/data/migration_runner.rs`, трекинг по checksum). Новая миграция = следующий номер.
- Soft delete (`is_deleted`); сложные поля хранятся JSON-ом в БД.
- Фронт: `spawn_local` для async, `RwSignal` для состояния; per-page CSS в `static/pages/<page>.css` под корневым классом страницы (см. memory `per-page-css-convention`).
- Боевая БД и knowledge — вне репозитория: `F:/data/leptos_marketplace_1/` (пути в `config.toml`).
  Диск F: отдан под проекты: `F:\dev\<проект>` — репозитории, `F:\data\<проект>` — рабочие данные.
- **Все данные — под одним корнем `[data].root`**: это единственный путь в конфиге в
  основном режиме. Из него выводятся `db/app.db` (файл БД) и каталоги knowledge, skills,
  chats, golden_set, quality_checks, attachments, backups, tmp — с фиксированными именами.
  Абсолютный путь в своей секции (`[database].path`, `[llm].*_path`, `[quality].checks_path`)
  по-прежнему побеждает, но это **отклонение**: каталог уходит из-под корня, помечается в UI
  «Наборы данных» как «Индивидуальный путь» и попадает в аномалии. Логи — исключение: лежат
  рядом с бинарником (`system/tracing.rs`), в корень данных не входят.
  Никогда не строй путь к данным вручную — бери `config::resolve_*`/`get_*`.
  Перенос наборов между экземплярами через S3 — `system/datasets/`, дока
  `docs/DATASETS_TRANSFER.md`. Перенос БД — фаза 2 той же подсистемы.

---

## Плагины (runtime JS)

Плагины — самодостаточные JS-артефакты (`bundle`: `client_script` + `server_script` в QuickJS +
`styles` + `sql_resources`), которые **живут строками в таблице `plugin`** (боевая БД, см. `[database].path`),
а НЕ файлами в репо. Их не найти grep'ом по коду. Идентичность — `manifest.code` (UUID `id` локальный).
Движок — `plugins/engine.rs`, доступ — `plugins/repository.rs`.

**Как править (самый простой путь):** helper `plugin_cli.py` в каталоге состояния Claude Code
для этого проекта (`~/.claude/projects/<derived-от-пути-проекта>/secrets/plugin_cli.py`; admin-креды
и Bearer-логин он берёт из соседнего `plugin-admin.json`; backend должен работать на :3000 — иначе
`powershell -File tools/restart_backend.ps1`).
> ⚠️ На текущей машине этого файла нет — при необходимости восстановить перед использованием.

```bash
python plugin_cli.py get <id|code> <dir>       # explode бандла в файлы: client.js, server.js, styles.css, manifest.json, sql/<name>.sql
python plugin_cli.py validate <dir>            # реальный QuickJS-компайл server+client → {ok, *_exports, errors}
python plugin_cli.py invoke <id|code> <method> '{json-args}'   # dev-invoke на живой БД → result + logs
python plugin_cli.py save <dir> [--id ID]      # validate + upsert: пишет revision, бампит ChangeToken → фронт рефрешится сам
```

Цикл: `get` → правь файлы (SQL — в `sql/*.sql`) → `validate` → `invoke` (сверь форму данных) → `save`.
API `/api/plugin/*` — admin-only; **всегда адресуй плагин по `code`** (helper сам резолвит `code`→UUID):
при обновлении БД из боевой копии UUID меняется, а `code` стабилен. Admin-юзер для API — `claude_dev`
(в `plugin-admin.json`); он должен существовать в боевой БД, иначе после рефреша логин отвалится (401).
После смены БД у фронта устаревают вкладки на старый UUID («Plugin has no client_script») — жёсткий reload.
Активация (`status: active` + `is_enabled`) — только по явной просьбе пользователя. Fallback без API —
прямой `UPDATE` в БД (WAL, live), но без валидации/ревизий и с ручным рефрешем фронта.

**Ограничения SQL в `sql_resources`** (гард `sql_guard::inspect_read_query`, проверяется и в `plugin_validate`):
только один `SELECT`/`WITH`; без комментариев `--`/`/* */`; без `SELECT *` и `alias.*` (в т.ч. `f.*`)
при обращении к `a006_connection_mp`; без защищённых полей (`api_key`, `*_token`, `secret`). Параметры —
позиционные `?` (порядок в SQL = порядок в массиве серверного вызова; при добавлении пересчитывай оба).

Детали — memory `plugin-editing-workflow`; авторская дока движка — `domain/a018_llm_chat/prompts/plugin_admin_agent.md`.

---

## Скиллы проекта (`.claude/skills/`)

Установлены из `mattpocock/skills` и **адаптированы под этот проект** (пути, русский язык,
запрет на сборки во время интервью) — при обновлении из апстрима правки затрутся, сверяйся с diff.

| Скилл | Что делает |
|---|---|
| `grill-with-docs` | Точка входа: допрос по плану + ведение доки. Вызывать перед реализацией нетривиальной задачи |
| `grilling` | Механика допроса: по одному вопросу за раз, с рекомендованным ответом; факты ищет сам, решения — за тобой |
| `domain-modeling` | Ведение `CONTEXT.md` и ADR по ходу разговора; форматы — в `domain-modeling/ADR-FORMAT.md` и `CONTEXT-FORMAT.md` |

`grill-with-docs` — это тонкая обёртка, которая требует оба остальных скилла; по отдельности
`grilling` даёт стресс-тест без записи в файлы.

---

## Где искать глубже

- `CONTEXT.md` — глоссарий домена (что означает термин). Только определения.
- `.claude/memory/MEMORY.md` — бизнес-факты YM/WB/GL, текущие планы (свежее всего).
- `memory-bank/` — ADR (`decisions/`), уроки (`lessons/`), runbook'и (`runbooks/`), known-issues, debrief'ы. Контент перекошен во фронтенд-миграции конца 2025 — сверяйся с кодом.
  - ADR: единый формат `ADR-NNNN-slug.md`, конвенция и индекс — `memory-bank/decisions/README.md`.
    Короткий ADR — нормальный ADR (абзаца достаточно); ADR только для трудно-обратимого,
    неочевидного и реально разменянного. Конвенции кодирования — в `code-standards/`, не в ADR.
- `docs/` — актуальные гайды по фичам (завершённые/устаревшие вынесены в `docs/_archive/`).
- `general_ledger/llm.md` — заметки по GL для LLM.
