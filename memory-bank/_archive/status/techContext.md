# Technical Context

## Технологический стек

### Core Technologies

**Frontend:**

- **Leptos 0.8** - Реактивный Rust фреймворк для веба
- **Thaw UI 0.5.0-beta** - Компонентная библиотека для Leptos
- **WASM** - Компиляция Rust в WebAssembly
- **Trunk** - Build tool и dev server для Leptos

**Backend:**

- **Axum 0.7** - Async web framework на Tokio
- **SQLite** - Встроенная база данных
- **Sea-ORM 0.12** - ORM для работы с БД
- **sqlx 0.7** - Используется для миграций (`sqlx::migrate::Migrator`)
- **Tokio 1.x** - Async runtime

**Shared:**

- **Serde 1.x** - Сериализация/десериализация (JSON)
- **Chrono 0.4** - Работа с датами и временем
- **UUID 1.11** - Генерация уникальных идентификаторов

### Thaw UI Integration

**Основные компоненты:** ConfigProvider, Table, Button, Input, Select, DatePicker

**Темизация:** light / dark / forest через CSS переменные (`--colorNeutralBackground1`, `--colorBrandBackground`)

**Гибридный подход к таблицам:**
- Thaw `<Table>` — для простых справочников
- Нативный HTML `<table>` + BEM классы — для полного контроля (сортировка, resize)

**Документация:**
- `memory-bank/runbooks/RB-thaw-table-sorting-v1.md` - добавление сортировки
- `memory-bank/known-issues/KI-thaw-table-style-limitations-2025-12-21.md` - ограничения

### Workspace Structure

```toml
[workspace]
members = ["crates/frontend", "crates/backend", "crates/contracts"]
```

- **contracts** — Shared DTOs и типы (dependency для frontend и backend)
- **backend** — Axum server, HTTP API, database, business logic, external integrations
- **frontend** — Leptos WASM app: UI components, API client, state management, routing

## Development Environment

### OS & Shell

- **OS**: Windows 11 / **Shell**: PowerShell
- НЕ использовать `&&` для цепочки команд — использовать `;` или отдельные команды
- **Node.js**: `pnpm` (НЕ npm или yarn)

### Rust Toolchain

- **Edition**: 2021 / **Toolchain**: Stable
- **Target**: `wasm32-unknown-unknown` (для frontend)

## Build & Development

```powershell
# Backend (миграции применяются автоматически при старте)
cargo run --bin backend     # http://localhost:3000

# Frontend (hot reload)
trunk serve --port 8080     # http://localhost:8080 (проксирует API на :3000)

# Production build
cargo build --release --bin backend
trunk build --release       # результат в dist/
```

## Database

### SQLite Configuration

- **File**: `marketplace.db` — путь из `config.toml` рядом с .exe (дефолт: `target/db/app.db`)
- **Journal mode**: WAL рекомендуется

### Migrations

Формальная система через `sqlx::migrate::Migrator` (с 2026-02-18).

**Автоматическое применение при старте:**

```powershell
# Создать файл с номером после последнего
# migrations/0002_description.sql
# При следующем запуске backend применится автоматически
cargo run --bin backend
```

**Проверить состояние:**

```powershell
sqlite3 marketplace.db "SELECT version, description, installed_on FROM _sqlx_migrations"
```

**Структура:**

```
migrations/
├── 0001_baseline_schema.sql   <- полная схема (40+ таблиц)
├── 0002_...sql                <- новые изменения
└── archive/                   <- старые migrate_*.sql (история)
```

**Ключевые файлы:**
- `crates/backend/src/shared/data/migration_runner.rs` — запускает `sqlx::migrate::Migrator`
- `crates/backend/src/shared/data/db.rs` — только коннект (`get_connection()`)

**ВАЖНО:** Никогда не редактировать уже применённые файлы в `migrations/` — sqlx проверяет checksum.

### Database Tools

- **CLI**: `sqlite3` / **GUI**: DB Browser for SQLite, DBeaver

## External Integrations

### 1C:Управление торговлей 11

**Protocol**: OData v4 / **Auth**: Basic authentication

- Client: `backend/src/usecases/u501_import_from_ut/ut_odata_client.rs`

### Wildberries API

**Auth**: Token-based (x-api-key header) / **Base URL**: `https://statistics-api.wildberries.ru`

Rate limits: обработка 429 ответов обязательна.

### Ozon API

**Auth**: Client-Id + Api-Key headers / **Base URL**: `https://api-seller.ozon.ru`

### LemanaPro

**Auth**: API Key / Analytics and planning data

## Configuration

### config.toml (рядом с backend.exe)

```toml
[database]
path = "C:/path/to/data/app.db"   # абсолютный путь рекомендуется

[scheduled_tasks]
enabled = true                     # false для dev без фонового воркера
```

При отсутствии файла: дефолт `target/db/app.db`. `build.rs` backend автоматически копирует `config.toml` из корня в `target/debug/` при сборке.

Env var для логирования: `RUST_LOG=info` (или debug, warn, error)

## Technical Constraints

### Database

- **SQLite only** — не PostgreSQL, MySQL
- **Migration checksum** — не редактировать применённые файлы

### Frontend

- **WASM bundle size** — следить за размером артефактов (debug build очень большой, использовать --release для тестирования)
- **No multithreading** — WASM без threads
- **Async**: требует wasm-bindgen-futures

### Backend

- **Async runtime** — Tokio, все I/O operations асинхронные
- **Multi-user** — JWT-авторизация, несколько пользователей

## Development Workflow

1. **Start backend** (миграции применятся автоматически):
   ```powershell
   cargo run --bin backend   # В логах: "✓ Database migrations processed"
   ```
2. **Start frontend**: `trunk serve --port 8080`
3. **Open browser**: http://localhost:8080
4. **Добавить изменение схемы БД**: Создать `migrations/NNNN_description.sql` → restart backend

**Debugging:**
```rust
// Backend
log::info!("Information");
// Frontend
leptos::logging::log!("Debug message");
```

## Key Dependencies (versions to watch)

- **Leptos 0.8** — активно развивается, следить за минорными обновлениями
- **Thaw UI 0.5.0-beta** — Beta, возможны изменения API
- **Sea-ORM 0.12** — ORM для domain repositories
- **sqlx 0.7** — для миграций

## CI/CD

Manual deployment — build locally, test manually, deploy self-hosted.
