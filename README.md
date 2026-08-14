# Leptos Marketplace

Десктопная система учёта и аналитики для торговли на маркетплейсах (Wildberries, Ozon,
Яндекс Маркет) с синхронизацией из 1С:Управление торговлей 11. Rust: Axum + Leptos/WASM.

## Запуск для разработки

Требуется Rust (stable, edition 2021) и Trunk (`cargo install trunk`).

Два терминала из корня:

```powershell
cargo run -p backend          # Axum API на http://localhost:3000
trunk serve --port 8080       # фронт на http://localhost:8080 (проксирует API на :3000)
```

Если `cargo run` падает с «Access is denied» — старый `backend.exe` ещё держит файл:
`powershell -File tools/restart_backend.ps1`.

Проверка перед коммитом:

```powershell
cargo check -p backend
cargo check -p contracts
cargo check -p frontend --target wasm32-unknown-unknown   # фронт собирается только под wasm
cargo test -p backend router_builds                        # после правок роутов
```

Release:

```powershell
trunk build --release                      # → dist/
cargo build --release --bin backend        # → target/release/backend.exe
```

## Данные

Рабочая БД и база знаний живут **вне репозитория** — `F:/data/leptos_marketplace_1/`
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
```

Принципы: DDD + VSA (вертикальные срезы), индексированное именование объектов
(`a0XX` агрегаты, `p9XX` проекции, `u5XX` use-cases, `d4XX` дашборды), общие контракты
между фронтом и бэком.

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
