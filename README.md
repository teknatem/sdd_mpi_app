# Integrator (MPI)

**Marketplace Integrator** — десктопная система учёта и аналитики для торговли на
маркетплейсах (Wildberries, Ozon, Яндекс Маркет) с синхронизацией из 1С:Управление
торговлей 11. Rust: Axum + Leptos/WASM.

Приложение построено по методу **SDD** (Slice-Driven Design) и является его
референсной реализацией: `architecture.toml` в корне — первый живой манифест SDD.

## Запуск для разработки

Требуется Rust (stable, edition 2021) и Trunk (`cargo install trunk`).

Всё живёт на **одном origin — http://localhost:3000**: бэкенд поднимает API и он
же раздаёт собранный фронт из `dist/` (`ServeDir`). Отдельного dev-сервера для
фронта нет, вотчера нет — сборка запускается явно, одна на пачку правок.

```powershell
cargo run -p backend                                  # API + раздача dist/ на :3000
powershell -File tools/build_frontend.ps1             # пересобрать фронт в dist/
powershell -File tools/build_frontend.ps1 -CssOnly    # только static/ → dist/, без cargo
```

Если `cargo run` падает с «Access is denied» — старый `backend.exe` ещё держит файл:
`powershell -File tools/run_backend.ps1`.

Проверка перед коммитом:

```powershell
cargo check -p backend
cargo check -p contracts
cargo check -p frontend --target wasm32-unknown-unknown --profile wasm-dev   # профиль обязателен
cargo test -p backend router_builds                                          # после правок роутов
```

Release:

```powershell
trunk build --cargo-profile release        # → dist/ (фронт). НЕ `--release`: см. ниже
cargo build --release --bin backend        # → target/release/backend.exe
```

Про `--cargo-profile`: `Trunk.toml` задаёт `cargo_profile = "wasm-dev"`, и этот ключ
**сильнее флага `--release`** — `trunk build --release` молча соберёт dev-профилем.
Профиль для релиза передаётся явно; в dev, наоборот, флаг не нужен и забыть его
нельзя. Тот же профиль обязателен в `cargo check`/`cargo build` фронта, иначе cargo
держит второй набор wasm-артефактов и платит полную холодную пересборку при каждом
переключении. Готовый релиз собирается [scripts/build-release.ps1](scripts/README.md).

## Данные

Рабочая БД и база знаний живут **вне репозитория** — `F:/data/sdd_mpi_app/`
(корень задаётся ключом `[data].root` в `config.toml`, шаблон — `config.toml.example`).
Файл БД — `<root>/db/app.db`.

Миграции — SQL-файлы `migrations/NNNN_имя.sql`, применяются **автоматически** при старте
бэкенда с трекингом по checksum. Новая миграция = следующий свободный номер.

## Структура

```
crates/
├── contracts/    # общие DTO, определения агрегатов, metadata.json
├── backend/      # Axum, домен, БД, проекции, главная книга
└── frontend/     # Leptos/WASM SPA
tools/            # исполняется процессом разработки: генераторы, гейты, git-хук, цикл сборки
scripts/          # операции над средой и данными: релиз, деплой, служба, плагины
```

Два каталога скриптов — не разнобой: граница проходит по тому, **кто и когда**
скрипт запускает. Правило записано в `architecture.toml`
(`[conventions.script_homes]`) и проверяется валидатором, состав каждого каталога —
в [tools/README.md](tools/README.md) и [scripts/README.md](scripts/README.md).

Принципы: DDD + SDD (индексированные срезы, зеркалируемые по трём крейтам), именование
объектов кодом (`a0XX` агрегаты, `p9XX` проекции, `u5XX` use-cases, `d4XX` дашборды),
общие контракты между фронтом и бэком. Метод описан в каноне экосистемы —
[SDD.md](../sdd_ecosystem/SDD.md); правила именно этого приложения — в `architecture.toml`.

## Место в экосистеме

**Канон экосистемы — `F:\dev\sdd_ecosystem\`. Адрес канона называется только здесь и
в шапке `architecture.toml`; остальные файлы репозитория ссылаются на эти два
места, а не повторяют путь** — переименование каталога канона однажды уже стоило
28 правок в трёх репозиториях.

MPI — приложение экосистемы **SDD**, живущей в `F:\dev`. Соседи: `sdd_studio`
(анализатор структуры), `sdd_desktop` (тонкий клиент), `sdd_ecosystem` (канон:
имена, метод, схема манифеста, контракты).

С соседями приложение связано двумя контрактами и ничем больше: Studio читает его
`architecture.toml` и пишет каталог артефактов анализа, откуда `tools/gen_code_metrics.ps1`
забирает число findings. Границы — [CONTRACTS.md](../sdd_ecosystem/CONTRACTS.md), карта —
[ECOSYSTEM.md](../sdd_ecosystem/ECOSYSTEM.md). В `memory-bank` и исходники соседних репозиториев
не заглядываем.

## Документация

Четыре точки входа, по убыванию частоты использования:

| Файл | Что там |
|---|---|
| [CLAUDE.md](CLAUDE.md) | **Гид по проекту**: как собирать, схема именования, карты бэка и фронта, конвенции. Начинать здесь — и людям, и AI-ассистентам |
| [ARCHITECTURE.md](ARCHITECTURE.md) | **Каталог объектов**, генерируется из кода: все агрегаты, проекции, use-cases, задачи, план счетов, разделы UI, API-роуты. Ищи объект здесь, не грепая исходники |
| [CONTEXT.md](CONTEXT.md) | **Глоссарий домена**: что значит «слой учёта», «репост», «кабинет», чем `dsXX` отличается от `dvXX` |
| [memory-bank/](memory-bank/) | ADR, UI-стандарты, runbook'и, известные грабли — см. [индекс](memory-bank/README.md) |

Плюс [docs/](docs/README.md) — гайды по отдельным фичам и планы.

ARCHITECTURE.md регенерируется автоматически pre-commit хуком. После свежего клона включи
хуки один раз:

```powershell
git config core.hooksPath tools/hooks
```

## Лицензия

Proprietary. All rights reserved.
