# CLAUDE.md

Гид для Claude Code по проекту **leptos_marketplace_1** — десктопная система управления маркетплейсами (1С:УТ 11, Wildberries, Ozon, Yandex Market) на Rust/Leptos.

> **Источник истины — код.** При расхождениях приоритет:
> `код` > `CONTEXT.md` > авто-память > `memory-bank/` > `docs/`.
> Прежде чем опираться на доку, проверь, что файл/функция/флаг ещё существуют в коде.
>
> «Авто-память» — заметки Claude Code, которые **подгружаются в контекст сами**
> (бизнес-факты YM/WB/GL, текущие планы). Файлов в репозитории нет и искать их не надо:
> если памяти не видно в контексте — её просто нет, а не «лежит где-то не там».

> **Язык домена — `CONTEXT.md`** (в корне): глоссарий терминов предметной области — что означают
> «слой учёта», «репост», «кабинет», «видимость», чем `dsXX` отличается от `dvXX`, а `a017` от `a038`.
> Только определения, без деталей реализации. Правится вручную по мере разбора домена
> (в т.ч. скиллом `grill-with-docs`); при расхождении с кодом прав код.

> **Карта объектов — `ARCHITECTURE.md`** (генерируется из кода): полный каталог агрегатов
> a0XX (с описаниями из metadata.json), проекций, use-cases, DataView, quality-checks,
> плана счетов, видов оборотов, разделов UI (scope-каталог + дерево сайдбара + компоненты
> вкладок) и всех API-роутов. Читай его, чтобы найти нужный объект, **не grep'ая** исходники.
> Колонка `Docs` показывает, у каких объектов есть рукописный `llm.md` рядом с кодом:
> карта отвечает «что есть и где», `llm.md` — «почему так и какие подвохи».
> Регенерация после изменений: `powershell -File tools/gen_architecture.ps1`.
> Авто-обновление: git pre-commit hook (`tools/hooks/`) регенерирует карту, когда в коммит
> попадают domain/projections/usecases/data_view/quality/реестры/`routes.rs`/layout фронта.
> После свежего клона включи один раз: `git config core.hooksPath tools/hooks`.

> **Реестр стилей — `UI_REGISTRY.md`** (генерируется из CSS+Rust): каталог всех BEM-блоков
> фронта с числом классов, реальным использованием в `.rs` и статусом по allowlist; плюс
> блоки, определённые в нескольких файлах; токены, на которые ссылаются, но не определяют
> (это баг-лист, а не стилистика); дрейф токенов между темами; хардкод по файлам; мёртвые
> классы и неподключённые стили. Смотри его **перед тем как заводить новый класс** — скорее
> всего нужный блок уже есть под другим именем.
> Нормативная половина — `memory-bank/architecture/ui-standard.md` (что *разрешено*);
> реестр отвечает «что есть», стандарт — «как должно быть».
> Регенерация: `powershell -File tools/gen_ui_registry.ps1` (тот же pre-commit hook).

> **Метрики проекта — `codebase_metrics.json`** (генерируется, коммитится): размер кода по
> крейтам, топ-10 файлов, плотность тестов, `.unwrap()`, счётчики архитектуры и UI (парсятся
> из `ARCHITECTURE.md` и `UI_REGISTRY.md` — не пересчитываются), активность по git.
> Бэкенд вшивает файл через `include_str!` и при каждом старте пишет снимок в
> `sys_metric_snapshot`, добавляя к нему рантайм-метрики (размер БД, строки по таблицам,
> quality-проверки, аудит доступа). Смотреть — страница «Метрики проекта» (`sys_metrics`,
> admin-only, открывается администратору при старте); подписи и пороги — в
> `backend/src/system/metrics/catalog.rs`, новая метрика заводится там.
> Регенерация: `powershell -File tools/gen_code_metrics.ps1` (тот же pre-commit hook, **после**
> двух генераторов выше).

> **База знаний**: помимо курируемых бизнес-статей есть корпус `generated` — профиль таблиц,
> плагины, навыки и quality-checks, собираемые из данных в `<data>/knowledge/generated/`.
> Каталоги `generated/` и `app/` принадлежат приложению — писать в них нельзя.
> Детали (якоря, `corpus="generated"`, `get_entity_schema`) — `crates/backend/skills/kb-curation.md`
> и `domain/a018_llm_chat/prompts/core.md`.

---

## Сборка и запуск

Dev (два терминала, из корня):
```powershell
cargo run -p backend          # Axum API на http://localhost:3000
trunk serve --port 8080 --cargo-profile wasm-dev   # Leptos/WASM фронт на :8080 (проксирует API на :3000)
```

> Уже запущенный backend.exe держит `target\debug\backend.exe` — повторный `cargo run`
> падает с «Access is denied». Перезапуск: `powershell -File tools/restart_backend.ps1`
> (останавливает процесс и запускает свежую сборку).

> **Планировщик выключен**: в `config.toml` стоит `[scheduled_tasks].enabled = false`, воркер не
> спавнится вообще — флаг `is_enabled` у конкретной задачи при этом ничего не значит. Если
> регламентное задание «не отработало», сначала проверь этот флаг, а не логику задачи. Включение
> стартует **все** задачи сразу — не включать без явной просьбы.

Проверка перед коммитом:
```powershell
cargo check -p backend
cargo check -p contracts
cargo check -p frontend --target wasm32-unknown-unknown   # frontend — только wasm-таргет
cargo test -p backend router_builds   # после правок роутов: конфликт путей axum виден только при сборке Router
```

> **Храповик метрик.** `tools/check_health.ps1` сравнивает свежий
> `codebase_metrics.json` с версией на `HEAD` и **блокирует коммит**, если
> отслеживаемая метрика ухудшилась. Это единственный шаг pre-commit хука с
> зубами (генераторы по-прежнему только предупреждают). Гейтятся те метрики, у
> которых в `system/metrics/catalog.rs` есть и направление, и пороги — список
> не дублируется в скрипте, а выводится из каталога. Осознанный размен —
> `SKIP_HEALTH=1 git commit …`; допуск — собственная точность метрики.
>
> **CI** — `.github/workflows/ci.yml`: fmt, `check` по трём крейтам, тесты
> backend, clippy (пока advisory), и храповик против базовой ветки на PR.
> Проверка форматирования фильтрует `*_gen.rs`: их пишет `contracts/build.rs`
> прямо в `src/`, и `write_if_changed` вернёт неформатированный вид на
> следующей сборке.
>
> **Стоимость сборки** измеряется отдельно: `tools/measure_build.ps1` (10–20 мин,
> в хук не входит) пишет `build_timings.json`, оттуда числа попадают в метрики.
> Гонять после изменений, которые должны были повлиять на время сборки.

> **Экономь сборки, но знай реальную цену.** Замерено `tools/measure_build.ps1`
> (числа — в `build_timings.json` и на странице «Метрики проекта»).
>
> **Три команды — три независимых набора артефактов, и они не греют друг друга:**
> `cargo check` пишет `libbackend-*.rmeta`, `cargo test` — `deps/backend-<hash>.exe`
> (104 МБ), `cargo run`/`cargo build --bin` — `debug/backend.exe` (103 МБ).
> Прогон одной команды **ничего не даёт** двум другим: пройденный `cargo check`
> не сокращает последующий `cargo run` ни на секунду. Отсюда практическое
> правило: делай подряд то, чем реально проверяешь, а не все три по очереди.
>
> Инкрементально, после правки одного файла агрегата:
> `check` backend **9 с**, тест-бинарь **14 с**, **бинарь `cargo run` — 14 с**,
> wasm check **10 с**, сборка wasm **20 с**; правка `contracts` задевает оба
> крейта — **26 с**.
>
> **Фронт — самое дорогое место, и профиль решает.** Полный цикл `trunk`
> (компиляция + `wasm-bindgen`) после правки одного файла:
> **`dev` — 36 с, `wasm-dev` — 25 с**; пересборка без правок — 11,8 с против
> 4,7 с. Причина в размере артефакта: `dev` даёт wasm на **197 МБ**,
> `wasm-dev` — на **66 МБ**, а `wasm-bindgen` работает пропорционально размеру.
> Компиляция при этом не медленнее (20,7 с против 24,6 с): на `opt-level = 0`
> кодогенерация выдаёт столько кода, что дальнейшая обработка дороже самой
> оптимизации. Отсюда `--cargo-profile wasm-dev` в команде запуска выше.
>
> Две оговорки. Первый запуск на новом профиле пересобирает **все 598
> зависимостей** — это разово ~6 минут. И **не чередуй профили**: у каждого
> свой набор артефактов, переключение туда-обратно платит за оба.
>
> Дорого становится, когда инкрементальность теряется целиком: после правки
> `Cargo.toml`, смены зависимостей или `cargo clean` — полный `check` **66 с**
> (backend) и **74 с** (frontend), а сборка бинаря/тестов с нуля — **114 с**.
> Правила ниже про то, чтобы не ронять инкрементальность зря.
> - **Не гоняй wasm-тесты** (`cargo test -p frontend --target wasm32-…`): в этом окружении нет
>   wasm-раннера — бинарь соберётся за ~10 мин и упадёт на запуске (`os error 193`). Чистую логику
>   фронта проверяй компиляцией, а сами тесты пиши так, чтобы исполнялись нативно (или тестируй в
>   `contracts`/`backend`).
> - **Один прогон на крейт после всех правок в нём**, а не после каждой под-области.
> - **Не смешивай профили `check` и `test`** — они собираются раздельно, т.е. проект компилируется
>   дважды. `cargo test` уже включает сборку → если запускаешь тесты, отдельный `cargo check` не нужен.
> - **Объединяй тест-фильтры в один запуск** (несколько фильтров сразу), чтобы не платить за
>   инкрементальную пересборку повторно.
> - **Разведку соразмеряй с задачей**: при живых CLAUDE.md + ARCHITECTURE.md предпочитай точечные
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

| Префикс | Что это | Где |
|---|---|---|
| `a0XX` | **Агрегат** (домен-сущность/документ) | `backend/src/domain/a0XX_*` |
| `p9XX` | **Проекция** (производная read-модель) | `backend/src/projections/p9XX_*` |
| `u5XX` | **Use-case** (импорты, репост) | `backend/src/usecases/u5XX_*` |
| `dsXX` | **Базовая схема данных** (роль *base schema*, движок universal_dashboard; UI: «Схемы таблиц») | `backend/src/data_schemes/dsXX_*` |
| `dvXX` | **DataView** (роль *виртуальная таблица*: курируемые метрики, 2 периода, кэш; UI: «DataView») | `backend/src/data_view/dvXXX` |
| `d4XX` | **Dashboard** (готовый дашборд — *потребитель* слоя) | `frontend/src/dashboards/d4XX_*` (бэкенд — только у части) |
| `task0XX` | **Запланированная задача** (поллинг/импорт) | `backend/src/system/tasks/managers/task0XX_*` |

**Актуальный перечень объектов не дублируется здесь — он в `ARCHITECTURE.md`** (генерируется из
кода, всегда точен). Диапазонов в этом файле намеренно нет: они устаревают молча.

Примеры: a013 = YM order, a015 = WB orders, a034 = YM realization; p904 = sales_data,
p907 = YM payment report; u503 = import from Yandex; ds01–ds03 — базовые схемы для
«Конструктора запросов» (ds01→p903, ds02→p900, ds03→p904).

> `d4XX` — единственное семейство, которое живёт **во фронте**: бэкенд-каталог
> `backend/src/dashboards/` содержит лишь d400, остальные дашборды целиком фронтовые.
> Дашборд — *потребитель* слоя данных, а не схема; не путай `d4XX` с `dsXX`.

## Слой данных: три роли источников (см. `memory-bank/decisions/ADR-0010-data-source-roles.md`)

Три независимых движка с **разными ролями** — выбирай осознанно:
- **`dvXX`** (`data_view/`) — курируемые метрики, 2 периода, кэш. Сюда сложные показатели и BI.
- **`dsXX`** (`data_schemes/` + `shared/universal_dashboard/`) — гибкий ad-hoc через `QueryBuilder`.
- **сырой SQL** (`execute_query`) — разовое; укреплённый escape-hatch.

Перекрытие по одной таблице допустимо только при разных ролях (`p904`: ds03 гибкий, dv001 курируемый).

**Термины код ↔ UI**: `dsXX` → «Схемы таблиц» + «Конструктор запросов»; `dvXX` → «DataView»;
группа `semantic_layer` → «Источники данных». Определения — `CONTEXT.md`, обоснование — ADR-0010.

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

`metadata.json` есть у **каждого** агрегата, проекции и у Главной книги; `contracts/build.rs`
генерирует из него `metadata_gen.rs` — **правится только JSON**, регистрировать сущность руками
не нужно и нельзя.

> Прежде чем править metadata.json — `memory-bank/architecture/metadata-system.md`:
> блок `ai` (теги, `related`, `llm_visible`), разрешение имён (`names_match`) и **ловушка
> `"physical": false`** — без неё LLM генерирует SQL с несуществующими колонками.

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

## Карта фронтенда (`crates/frontend/src/`)

Слои зеркалят бэкенд (`domain/`, `projections/`, `usecases/`, `general_ledger/`, `quality/`,
`data_view/`, `dashboards/`, `plugins/`, `system/`), плюс своё:

| Модуль | Назначение |
|---|---|
| `app.rs` → `app_shell.rs` | Каркас приложения (структура — ADR-0005) |
| `layout/tabs/registry.rs` | **Реестр страниц: сюда добавляется новая вкладка** (самый крупный файл каркаса) |
| `layout/tabs/tab_labels.rs` | Подписи вкладок |
| `layout/left/sidebar.rs` | Дерево сайдбара (что видно в UI — см. `ARCHITECTURE.md`, раздел UI sidebar) |
| `layout/{header,top_header,center,right,footer}/` | Зоны каркаса; `right/panel/` — правая панель |
| `shared/components/`, `shared/page_frame.rs` | Общие компоненты и рамка страницы |
| `shared/modal_stack/`, `shared/modal_frame/` | Модалки (стандарт — ADR-0004) |
| `shared/picker_aggregate/`, `shared/pivot/`, `shared/universal_dashboard/` | Пикер агрегатов, сводные, движок ds-схем |
| `static/pages/<page>.css` | Per-page CSS под корневым классом страницы |

Типовая страница агрегата: `domain/a0XX_*/ui/{list,details}/`, детали — со вкладками
в `ui/details/tabs/`. UI-стандарты (списки, detail-страницы, таблицы) — `memory-bank/architecture/`.

---

## Учётные слои (Главная книга)

GL — скелет финансовой модели. Поверх неё концептуальные слои учёта (детали — в авто-памяти):
- **fact / fina / ybuh** — параллельные слои оборотов для сверки выручки (fina заменяет fact для p903/p907; ybuh — официальные отчёты о реализации, напр. a034).
- `p914` — зеркало GL-проводок слоя fina; `p907` — YM payment report.
- Регистры: `account_registry` (план счетов), `turnover_registry` (обороты).
- После правок в маппинге/проводках нужен **репост документов** через `u508`.

Детали бизнес-правил YM/WB/GL — в авто-памяти (подгружается автоматически каждую сессию). Не дублируй их здесь.

---

## Конвенции

- Rust 2021, Leptos 0.8, SQLite (SeaORM), фронт — WASM.
- **Миграции БД** — SQL-файлы `migrations/NNNN_имя.sql`, применяются автоматически при старте бэкенда (`shared/data/migration_runner.rs`, трекинг по checksum). Новая миграция = следующий номер.
- Soft delete (`is_deleted`); сложные поля хранятся JSON-ом в БД.
- Фронт: `spawn_local` для async, `RwSignal` для состояния; per-page CSS в `static/pages/<page>.css` под корневым классом страницы (см. memory `per-page-css-convention`).
- Боевая БД и knowledge — вне репозитория: `F:/data/leptos_marketplace_1/` (пути в `config.toml`).
  Диск F: отдан под проекты: `F:\dev\<проект>` — репозитории, `F:\data\<проект>` — рабочие данные.
  Файл БД — `F:/data/leptos_marketplace_1/db/app.db` (~3 ГБ), knowledge — соседний `knowledge/`.
  **Никогда не запускай `sqlite3 app.db` из рабочего каталога** — sqlite молча создаст пустышку,
  и дальше всё «пустое». Всегда абсолютный путь.
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

**Как править.** API `/api/plugin/*` (admin-only, юзер `claude_dev`), цикл
`get → правки → validate → invoke → save`. **Всегда адресуй плагин по `code`**: при обновлении БД
из боевой копии UUID меняется, а `code` стабилен. Активация (`status: active` + `is_enabled`) —
только по явной просьбе пользователя. SQL в `sql_resources` ограничен гардом
`sql_guard::inspect_read_query` (один SELECT, без комментариев, без `*` рядом с кредами).

> Полная дока движка, формат бандла и разбор ограничений SQL —
> **`crates/backend/src/domain/a018_llm_chat/prompts/plugin_admin_agent.md`**. Раньше здесь
> описывался helper `plugin_cli.py` — на текущей машине его нет.

---

## Где искать глубже

- `CONTEXT.md` — глоссарий домена (что означает термин). Только определения.
- Авто-память — бизнес-факты YM/WB/GL, текущие планы (свежее всего). Подгружается сама.
- `memory-bank/` — см. `memory-bank/README.md` (индекс каталогов). Ключевое:
  - `decisions/` — ADR, формат `ADR-NNNN-slug.md`; конвенция и индекс — `decisions/README.md`.
    Короткий ADR — нормальный ADR; ADR только для трудно-обратимого, неочевидного и реально
    разменянного. Конвенции кодирования — в `code-standards/`, не в ADR.
  - `architecture/` — **UI-стандарты** (detail-страницы, списки, таблицы, модалки, CSS).
    Самый большой раздел; сюда идти за «как должна выглядеть страница», а не грепать код.
  - `runbooks/` — пошаговые сценарии повторяющихся задач: `RB_add-new-aggregate-ddd-vsa_v1`
    (новый агрегат), `RB_db-migration-workflow_v1` (миграция),
    `RB__metadata-add-to-aggregate__v1` (метаданные). Загляни **до** того, как делать вручную.
  - `lessons/`, `known-issues/` — грабли Leptos/Thaw/Rust; много записей конца 2025 — сверяйся с кодом.
- `docs/` — гайды по фичам и планы; индекс со статусами — `docs/README.md`
  (завершённые/устаревшие вынесены в `docs/_archive/`).
- `general_ledger/llm.md` — заметки по GL для LLM.
- `.claude/skills/` — скиллы проекта (список показывается автоматически, дублировать не нужно).
  Взяты из `mattpocock/skills` и **адаптированы под проект** — при обновлении из апстрима
  правки затрутся, сверяйся с diff.
