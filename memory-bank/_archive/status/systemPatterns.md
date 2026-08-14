# System Architecture & Patterns

## Архитектурный обзор

### Принципы

Проект построен на комбинации двух подходов:

1. **DDD (Domain-Driven Design)**: Разделение на bounded contexts и aggregates
2. **VSA (Vertical Slice Architecture)**: Группировка кода по фичам, а не по техническим слоям

### Структура Cargo Workspace

```
leptos_marketplace_1/
├── crates/
│   ├── contracts/    # Shared types (DTOs, aggregates)
│   ├── backend/      # Axum server
│   └── frontend/     # Leptos WASM app
├── marketplace.db    # SQLite database
└── memory-bank/      # Documentation for AI
```

**Contracts (shared)** - общие типы между frontend и backend:

- Гарантирует type safety на всем стеке
- Изменение в DTO → компилятор требует обновить frontend и backend
- Без runtime ошибок десериализации

## General Ledger — независимая система

> ⚠️ **ВАЖНО для AI:** General Ledger — это отдельная независимая подсистема бухгалтерского учёта.
> Она **НЕ является** domain-агрегатом (не входит в серию a001–a499).
> Весь её код живёт в `src/general_ledger/` во **всех трёх крейтах** — никогда в `src/domain/`.

### Расположение файлов

```
crates/contracts/src/general_ledger/
├── mod.rs
├── dto.rs           # GeneralLedgerEntryDto, GeneralLedgerTurnoverDto
├── turnover.rs      # TurnoverClassDef и связанные типы
├── accounting.rs    # AccountDef, план счетов
└── metadata.rs

crates/backend/src/general_ledger/
├── mod.rs
├── repository.rs    # CRUD в БД
├── service.rs       # Бизнес-логика
├── account_registry.rs   # Реестр счетов (план счетов)
└── turnover_registry.rs  # Реестр видов оборотов

crates/frontend/src/general_ledger/
├── mod.rs
├── api.rs           # API-клиент + DTOs запросов/ответов
└── ui/
    ├── mod.rs
    ├── list/mod.rs      # GeneralLedgerPage (список проводок)
    ├── details/mod.rs   # GeneralLedgerDetailsPage (детали записи)
    └── turnovers/mod.rs # GeneralLedgerTurnoversPage (реестр оборотов)
```

### Правило

Любой новый файл, связанный с General Ledger (новый экран, новый API-метод, новый тип данных),
должен создаваться в `crates/*/src/general_ledger/`, а **не** в `crates/*/src/domain/`.

В `crates/frontend/src/lib.rs` модуль подключён напрямую: `pub mod general_ledger;`
В `crates/frontend/src/layout/tabs/registry.rs` импорт: `use crate::general_ledger::ui::{...};`

---

## Индексированная система именования

Это ключевой паттерн проекта. Все фичи имеют индекс:

### Aggregates: a001-a499

Доменные сущности с бизнес-логикой

**Реализовано (a001-a026):**

- `a001_connection_1c` - Подключения к 1С
- `a002_organization` - Организации
- `a003_counterparty` - Контрагенты
- `a004_nomenclature` - Номенклатура
- `a005_marketplace` - Маркетплейсы
- `a006_connection_mp` - Подключения к маркетплейсам
- `a007-a016` - Продукты, продажи, заказы и возвраты маркетплейсов
- `a017_llm_agent` - LLM-агенты
- `a018_llm_chat` - LLM-чаты
- `a019_llm_artifact` - LLM-артефакты
- `a020_wb_promotion` - WB-продвижение (акции)
- `a021_production_output` - Выпуск продукции
- `a022_kit_variant` - Варианты комплектов
- `a023_purchase_of_goods` - Закупки товаров
- `a024_bi_indicator` - BI Индикаторы
- `a025_bi_dashboard` - BI Дашборды
- `a026_wb_advert_daily` - Реклама WB по дням

**Структура:**

```
crates/backend/src/domain/a001_connection_1c/
├── mod.rs            # Re-exports
├── service.rs        # Business logic orchestration
├── repository.rs     # Database CRUD
└── aggregate.rs      # Optional: data structure (may be in contracts)

crates/contracts/src/domain/a001_connection_1c/
└── aggregate.rs      # Shared DTO/struct

crates/frontend/src/domain/a001_connection_1c/
└── ui/
    ├── list/         # List view
    └── details/      # Details/edit view
```

### UseCases: u501-u999

Операции, часто затрагивающие несколько aggregates

**Реализовано (u501-u506):**

- `u501_import_from_ut` - Импорт из 1С:УТ11
- `u502_import_from_ozon` - Импорт из Ozon
- `u503_import_from_yandex` - Импорт из Яндекс.Маркет
- `u504_import_from_wildberries` - Импорт из Wildberries
- `u505_match_nomenclature` - Сопоставление номенклатуры
- `u506_import_from_lemanapro` - Импорт из LemanaPro

**Структура:**

```
crates/backend/src/usecases/u501_import_from_ut/
├── mod.rs
├── executor.rs       # Main logic
└── ut_odata_client.rs # External integration

crates/contracts/src/usecases/u501_import_from_ut/
├── request.rs
├── response.rs
└── progress.rs       # For long-running operations

crates/frontend/src/usecases/u501_import_from_ut/
├── view.rs           # UI widget
└── monitor.rs        # Progress tracking
```

### Projections: p900-p999

Read models, аналитика, отчеты (CQRS-подобный подход)

**Реализовано (p900-p911):**

- `p900_mp_sales_register` - Регистр продаж маркетплейсов
- `p901_nomenclature_barcodes` - Штрих-коды номенклатуры
- `p902_ozon_finance_realization` - Финансовая реализация Ozon
- `p903_wb_finance_report` - Финансовый отчет Wildberries
- `p904_sales_data` - Аналитика продаж
- `p905_wb_commission_history` - История комиссий Wildberries
- `p906_nomenclature_prices` - Цены номенклатуры
- `p907_ym_payment_report` - Финансовый отчёт Яндекс.Маркет
- `p908_wb_goods_prices` - Цены товаров Wildberries
- `p909_mp_order_line_turnovers` - Обороты по строкам заказов
- `p910_mp_unlinked_turnovers` - Несвязанные обороты
- `p911_wb_advert_by_items` - Реклама WB в разрезе товаров

## Field Metadata System

### Обзор

Система декларативного описания метаданных агрегатов для:
- **Single Source of Truth** — JSON как единственный источник структуры
- **AI/LLM Context** — Подготовка данных для встроенного чата
- **UI Generation** — Автогенерация форм и таблиц (планируется)

### Архитектура

```
metadata.json ──► build.rs ──► metadata_gen.rs
                                    │
                                    ▼
                           AggregateRoot trait
                           ├── entity_metadata_info()
                           └── field_metadata()
```

### Ключевые файлы

```
crates/contracts/
├── build.rs                    # Генератор
├── schemas/metadata.schema.json # JSON Schema
└── src/
    ├── shared/metadata/        # Rust types
    │   ├── types.rs            # EntityMetadataInfo, FieldMetadata
    │   ├── field_type.rs       # FieldType enum
    │   └── validation.rs       # ValidationRules
    └── domain/a001_*/
        ├── metadata.json       # ИСТОЧНИК (ручное редактирование)
        └── metadata_gen.rs     # ГЕНЕРИРУЕТСЯ (не редактировать)
```

### Добавление метаданных для aggregate

1. Создать `metadata.json` (скопировать шаблон из a001)
2. Добавить в `mod.rs`: `mod metadata_gen; pub use metadata_gen::{ENTITY_METADATA, FIELDS};`
3. Реализовать методы trait в `aggregate.rs`
4. `cargo build` — генерация автоматическая

**См. также:** `memory-bank/architecture/metadata-system.md` — полная документация

## Domain Layer Patterns

### Ответственность слоев

```
┌─────────────────────────────────────┐
│         HTTP Handlers               │  ← Axum routes
└──────────────┬──────────────────────┘
               │
               ▼
┌─────────────────────────────────────┐
│         Service Layer               │  ← Orchestration
│  - Calls validators                 │  ← Business rules
│  - Calls lifecycle hooks            │  ← Transaction boundaries
│  - Coordinates operations           │
└──────────────┬──────────────────────┘
               │
               ▼
┌─────────────────────────────────────┐
│       Repository Layer              │  ← Pure CRUD
│  - Database operations              │  ← NO business logic
│  - Model ↔ Aggregate mapping        │
└─────────────────────────────────────┘
               │
               ▼
        [SQLite Database]
```

### Lifecycle Events (1C-style)

Паттерн, вдохновленный событиями 1С (ПередЗаписью, ПриЗаписи и т.д.):

| Событие           | Когда вызывается   | Где определено             |
| ----------------- | ------------------ | -------------------------- |
| `validate()`      | Перед сохранением  | aggregate.rs или events.rs |
| `before_write()`  | Перед записью в БД | aggregate.rs или events.rs |
| `before_delete()` | Перед удалением    | aggregate.rs или events.rs |

**Порядок вызова при создании/обновлении:**

```rust
1. Создать/загрузить aggregate
2. aggregate.validate()?        // Может заблокировать
3. aggregate.before_write()     // Может модифицировать
4. Бизнес-правила (в service)
5. repository::save()
6. После сохранения (опционально)
```

### UseCase vs Service

**Service** (domain layer):

- Операции над ОДНИМ aggregate
- Пример: create, update, delete для Connection1C

**UseCase**:

- Операции над НЕСКОЛЬКИМИ aggregates
- Пример: импорт из 1С затрагивает Organization + Product + Nomenclature

## Thaw UI

Детали интеграции: `techContext.md` → раздел "Thaw UI Integration".

Гибридный подход: Thaw `<Table>` для простых справочников, нативный `<table>` + BEM для сложных (resize, полный контроль).

### Signal Reactivity Pattern

**Проблема:** Non-reactive props не обновляют компоненты при изменении родительского state.

**Решение:** Использовать Signal параметры для реактивных данных.

```rust
// ❌ Bad - не реактивно
#[component]
fn MyComponent(id: Option<String>) -> impl IntoView { ... }

// ✅ Good - реактивно
#[component]
fn MyComponent(#[prop(into)] id: Signal<Option<String>>) -> impl IntoView {
    Effect::new(move |_| {
        if let Some(current_id) = id.get() {
            // Этот код выполнится при каждом изменении id
        }
    });
}
```

**См. также:** `memory-bank/lessons/LL-leptos-signal-vs-value-2025-12-21.md`

## Frontend Patterns

### Component Architecture

```
domain/{feature}/ui/
├── list/
│   ├── mod.rs        # Table view with sorting, filtering
│   └── state.rs      # State management (optional)
└── details/
    └── mod.rs        # Form for create/edit
```

### Common UI Utilities

- `shared/list_utils.rs` - Sorting, filtering for tables
- `shared/date_utils.rs` - Date formatting
- `layout/center/tabs/tabs.rs` - Tab management
- CSS entry point: `crates/frontend/static/themes/core/index.css`

## Database Patterns

### Table Naming

```sql
-- Aggregates
CREATE TABLE a001_connection_1c_database (...);

-- UseCases history
CREATE TABLE u501_import_history (...);

-- Projections
CREATE TABLE p904_sales_data (...);
```

### Migration Strategy

Формальная система миграций на базе `sqlx::migrate::Migrator` (с версии 2026-02-18).

**Структура:**

```
leptos_marketplace_1/
├── migrations/
│   ├── 0001_baseline_schema.sql   <- полная исходная схема (40+ таблиц)
│   ├── 0002_...sql                <- будущие изменения
│   └── archive/                   <- старые migrate_*.sql (только история)
└── crates/backend/src/shared/data/
    ├── db.rs                      <- коннект к БД + get_connection()
    └── migration_runner.rs        <- sqlx::migrate::Migrator + поиск директории
```

**Как это работает:**

1. При старте backend `main.rs` вызывает `migration_runner::run_migrations()`
2. Runner ищет директорию `migrations/` (рядом с .exe → CWD → `../../migrations`)
3. `sqlx::migrate::Migrator::new(dir).run(pool)` применяет все pending-миграции
4. Трекинг применённых миграций в таблице `_sqlx_migrations` (автоматически)

**Трекинг состояния БД:**

```sql
SELECT version, description, installed_on, success
FROM _sqlx_migrations ORDER BY version;
-- 1 | baseline schema | 2026-02-18 10:00:00 | true
```

**Добавление новой миграции:**

```powershell
# 1. Создать файл (номер следующий после последнего)
# migrations/0002_a020_new_table.sql

# 2. Миграция применится автоматически при следующем запуске backend
cargo run --bin backend
```

**Ключевые файлы:**

- `crates/backend/src/shared/data/migration_runner.rs` — логика запуска миграций
- `crates/backend/src/shared/data/db.rs` — только коннект (`get_connection()`)
- `crates/backend/src/system/initialization.rs` — только `ensure_admin_user_exists()`

## Ключевые правила

### ✅ DO

1. Группируй код по фичам (вертикальные срезы)
2. Shared contracts между frontend/backend
3. Service для одного aggregate, UseCase для нескольких
4. Repository - только CRUD, без бизнес-логики
5. Используй indexed naming (a001, u501, p904)

### ❌ DON'T

1. Не размещай бизнес-логику в repository
2. Не дублируй типы между frontend/backend - используй contracts
3. Не создавай UseCase для операций над одним aggregate
4. Не вызывай repository напрямую из handlers
5. Не группируй код по техническим слоям (все controllers в одной папке и т.д.)

## Дополнительная информация

- `architecture/domain-layer-architecture.md` - Полное описание domain layer
- `../CLAUDE.md` и `../ARCHITECTURE.md` - структура workspace и карта объектов (генерируется из кода)
