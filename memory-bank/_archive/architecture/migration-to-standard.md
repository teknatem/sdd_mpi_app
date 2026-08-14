# Migration Plan: Current Project → Aggregate Standard

## 🎯 Цель миграции

Привести текущий проект `leptos_marketplace_1` в соответствие с [aggregate-standard.md](./aggregate-standard.md) для последующего использования VSA Project Explorer.

---

## 📊 Текущее состояние проекта

### Существующие агрегаты:

```
api/src/domain/
  └── connection_1c/          ❌ Нет префикса

server/src/domain/
  └── connection_1c/          ❌ Нет префикса

app/src/domain/
  └── connection_1c/          ❌ Нет префикса

База данных:
  └── connection_1c_database  ❌ Нет префикса
```

### Проблемы:

1. ❌ Нет префиксов `a001_` в именах
2. ❌ Нет файла `_aggregate.toml` с метаданными
3. ❌ Нет модуля `meta` с константами
4. ❌ Нет общего модуля `_common` для базовых типов
5. ❌ Таблица БД без префикса
6. ❌ Нет инструментов валидации

---

## 🚀 План миграции (пошагово)

### ШАГ 1: Создать общий модуль `_common`

**1.1 Создать структуру:**

```
api/src/domain/
  └── _common/
      ├── mod.rs
      ├── aggregate_root.rs
      ├── base_types.rs
      ├── events.rs
      └── errors.rs
```

**1.2 Переместить общие типы:**

- `BaseAggregate` → `_common/base_types.rs`
- `EntityMetadata` → `_common/base_types.rs`
- `EventStore` → `_common/events.rs`
- `AggregateRoot` trait → `_common/aggregate_root.rs`

**Файлы для создания:**

```rust
// api/src/domain/_common/mod.rs
pub mod aggregate_root;
pub mod base_types;
pub mod events;
pub mod errors;

pub use aggregate_root::AggregateRoot;
pub use base_types::{BaseAggregate, EntityMetadata};
pub use events::EventStore;
```

```rust
// api/src/domain/_common/base_types.rs
use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityMetadata {
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub is_deleted: bool,
    pub version: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaseAggregate<Id> {
    pub id: Id,
    pub metadata: EntityMetadata,
    pub events: super::EventStore,
}

impl<Id> BaseAggregate<Id> {
    pub fn new(id: Id) -> Self {
        Self {
            id,
            metadata: EntityMetadata {
                created_at: Utc::now(),
                updated_at: Utc::now(),
                is_deleted: false,
                version: 1,
            },
            events: super::EventStore::default(),
        }
    }

    pub fn with_metadata(id: Id, metadata: EntityMetadata) -> Self {
        Self {
            id,
            metadata,
            events: super::EventStore::default(),
        }
    }
}
```

```rust
// api/src/domain/_common/aggregate_root.rs
use super::{EntityMetadata, EventStore};

pub trait AggregateRoot {
    type Id;

    fn id(&self) -> Self::Id;
    fn metadata(&self) -> &EntityMetadata;
    fn metadata_mut(&mut self) -> &mut EntityMetadata;
    fn aggregate_type() -> &'static str;
    fn aggregate_id() -> &'static str;
    fn events(&self) -> &EventStore;
    fn events_mut(&mut self) -> &mut EventStore;
}
```

```rust
// api/src/domain/_common/events.rs
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EventStore {
    // Future: Vec<DomainEvent>
}
```

```rust
// api/src/domain/_common/errors.rs
use thiserror::Error;

#[derive(Error, Debug)]
pub enum DomainError {
    #[error("Entity not found: {0}")]
    NotFound(String),

    #[error("Validation error: {0}")]
    ValidationError(String),

    #[error("Concurrent modification detected")]
    ConcurrentModification,
}
```

**1.3 Обновить `api/src/domain/mod.rs`:**

```rust
pub mod _common;
pub mod connection_1c; // Пока оставляем старое имя
```

---

### ШАГ 2: Переименовать агрегат `connection_1c` → `a001_connection_1c`

**2.1 Переименовать папки:**

```bash
# API layer
mv api/src/domain/connection_1c api/src/domain/a001_connection_1c

# Server layer
mv server/src/domain/connection_1c server/src/domain/a001_connection_1c

# App layer
mv app/src/domain/connection_1c app/src/domain/a001_connection_1c
```

**2.2 Обновить `mod.rs` файлы:**

```rust
// api/src/domain/mod.rs
pub mod _common;
pub mod a001_connection_1c;

// server/src/domain/mod.rs
pub mod a001_connection_1c;

// app/src/domain/mod.rs
pub mod a001_connection_1c;
```

**2.3 Обновить все импорты:**

Find & Replace во всех файлах:

```
domain::connection_1c → domain::a001_connection_1c
```

Файлы, которые точно нужно обновить:

- `server/src/main.rs`
- `app/src/app.rs`
- `app/src/routes/routes.rs`
- Все файлы внутри самого агрегата

---

### ШАГ 3: Добавить модуль `meta` и обновить структуру

**3.1 Обновить `api/src/domain/a001_connection_1c/mod.rs`:**

```rust
//! # a001_connection_1c - 1C Database Connection
//!
//! **Category:** Integration
//! **Status:** Production
//! **Version:** 1.0.0
//!
//! Manages connections to 1C:Enterprise databases via OData protocol.
//! Supports multiple database configurations with primary/secondary selection.

pub mod aggregate;

/// Aggregate metadata constants
pub mod meta {
    /// Aggregate unique identifier
    pub const ID: &str = "a001";

    /// Aggregate name (snake_case)
    pub const NAME: &str = "connection_1c";

    /// Full aggregate name (with prefix)
    pub const FULL_NAME: &str = "a001_connection_1c";

    /// Aggregate category
    pub const CATEGORY: &str = "integration";

    /// Current version
    pub const VERSION: &str = "1.0.0";

    /// Database tables managed by this aggregate
    pub const DB_TABLES: &[&str] = &[
        "a001_connection_1c_database",
    ];
}

// Re-export main types
pub use aggregate::{
    Connection1CDatabase,
    Connection1CDatabaseId,
    Connection1CDatabaseDto,
};
```

**3.2 Обновить `aggregate.rs` - заменить импорты:**

```rust
// Старый импорт (удалить определения из этого файла)
// pub struct BaseAggregate<Id> { ... }
// pub struct EntityMetadata { ... }
// pub trait AggregateRoot { ... }

// Новый импорт
use crate::domain::_common::{
    AggregateRoot,
    BaseAggregate,
    EntityMetadata,
    EventStore,
};

// ... остальной код остаётся без изменений

impl AggregateRoot for Connection1CDatabase {
    type Id = Connection1CDatabaseId;

    fn id(&self) -> Self::Id {
        self.base.id
    }

    fn metadata(&self) -> &EntityMetadata {
        &self.base.metadata
    }

    fn metadata_mut(&mut self) -> &mut EntityMetadata {
        &mut self.base.metadata
    }

    fn aggregate_type() -> &'static str {
        "Connection1CDatabase"
    }

    fn aggregate_id() -> &'static str {
        super::meta::ID  // Используем константу из meta
    }

    fn events(&self) -> &EventStore {
        &self.base.events
    }

    fn events_mut(&mut self) -> &mut EventStore {
        &mut self.base.events
    }
}
```

---

### ШАГ 4: Создать `_aggregate.toml`

**4.1 Создать файл:**

```
api/src/domain/a001_connection_1c/_aggregate.toml
```

**4.2 Содержимое:**

```toml
# Aggregate Metadata File
# This file is used by VSA Project Explorer to scan and validate the aggregate

[aggregate]
id = "a001"
name = "connection_1c"
display_name = "1C Database Connection"
version = "1.0.0"
category = "integration"
status = "production"

[metadata]
description = """
Manages connections to 1C:Enterprise databases via OData protocol.
Supports multiple database configurations with primary/secondary selection.
Handles authentication, connection validation, and primary database tracking.
"""
author = "Development Team"
created_at = "2025-01-15"
updated_at = "2025-02-02"

[layers]
api = true
server = true
app = true

[database]
tables = [
    "a001_connection_1c_database"
]
prefix = "a001_connection_1c_"

[database.schema.a001_connection_1c_database]
description = "Main table storing 1C database connection configurations"
columns = [
    { name = "id", type = "INTEGER", primary_key = true },
    { name = "description", type = "TEXT", nullable = false },
    { name = "url", type = "TEXT", nullable = false },
    { name = "comment", type = "TEXT", nullable = true },
    { name = "login", type = "TEXT", nullable = false },
    { name = "password", type = "TEXT", nullable = false },
    { name = "is_primary", type = "INTEGER", nullable = false },
    { name = "is_deleted", type = "INTEGER", nullable = false },
    { name = "created_at", type = "TEXT", nullable = true },
    { name = "updated_at", type = "TEXT", nullable = true },
]

[domain]
aggregates = ["Connection1CDatabase"]
value_objects = ["Connection1CDatabaseId"]
dtos = ["Connection1CDatabaseDto"]

[domain.types.Connection1CDatabase]
description = "Aggregate root representing 1C database connection"
fields = [
    "description",
    "url",
    "comment",
    "login",
    "password",
    "is_primary"
]

[dependencies]
# This aggregate is isolated - no dependencies on other aggregates
aggregates = []

[validation]
enforce_isolation = true
require_all_layers = true
check_table_prefix = true
check_naming_convention = true

[ui]
has_list_view = true
has_details_view = true
has_form = true

[testing]
has_unit_tests = false
has_integration_tests = false
test_coverage_target = 80

[documentation]
readme = false
architecture_notes = """
This aggregate follows DDD principles:
- Repository pattern for data access
- Soft delete for data retention
- Primary database constraint (only one can be primary)
- Optimistic locking ready (version field)
"""
```

---

### ШАГ 5: Переименовать таблицы БД

**5.1 Создать миграцию:**

```sql
-- server/migrations/001_rename_to_a001_prefix.sql

-- Rename existing table
ALTER TABLE connection_1c_database
RENAME TO a001_connection_1c_database;

-- Recreate index with new name
DROP INDEX IF EXISTS idx_connection_1c_database_deleted;

CREATE INDEX IF NOT EXISTS idx_a001_connection_1c_database_deleted
ON a001_connection_1c_database(is_deleted);

-- Add version column if not exists (for optimistic locking)
-- SQLite doesn't support ALTER TABLE ADD COLUMN IF NOT EXISTS directly
-- Check manually and add if needed
```

**5.2 Обновить `server/src/shared/data/db.rs`:**

```rust
// Обновить SQL для создания таблицы
let create_connection_1c_table_sql = r#"
    CREATE TABLE IF NOT EXISTS a001_connection_1c_database (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        description TEXT NOT NULL,
        url TEXT NOT NULL,
        comment TEXT,
        login TEXT NOT NULL,
        password TEXT NOT NULL,
        is_primary INTEGER NOT NULL DEFAULT 0,
        is_deleted INTEGER NOT NULL DEFAULT 0,
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL,
        version INTEGER NOT NULL DEFAULT 1
    );
"#;

// Обновить индекс
conn.execute(Statement::from_string(
    DatabaseBackend::Sqlite,
    r#"
    CREATE INDEX IF NOT EXISTS idx_a001_connection_1c_database_deleted
    ON a001_connection_1c_database(is_deleted)
    "#.to_string(),
))
.await?;
```

**5.3 Обновить `server/src/domain/a001_connection_1c/repository.rs`:**

```rust
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "a001_connection_1c_database")]  // ← Обновить имя таблицы
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub description: String,
    pub url: String,
    pub comment: Option<String>,
    pub login: String,
    pub password: String,
    pub is_primary: bool,
    pub is_deleted: bool,
    pub created_at: Option<chrono::DateTime<chrono::Utc>>,
    pub updated_at: Option<chrono::DateTime<chrono::Utc>>,
    pub version: i32,  // ← Добавить поле version
}
```

---

### ШАГ 6: Обновить Cargo.toml (добавить зависимости)

**6.1 `api/Cargo.toml`:**

```toml
[dependencies]
serde = { workspace = true }
chrono = { workspace = true }
thiserror = "1"  # ← Добавить для errors.rs
```

**6.2 `server/Cargo.toml`:**

```toml
[dependencies]
api = { path = "../api" }
sea-orm = { version = "0.12", features = ["sqlx-sqlite", "runtime-tokio-native-tls"] }
anyhow = "1"
chrono = { workspace = true }
serde = { workspace = true }
```

---

### ШАГ 7: Создать структуру для следующих агрегатов

**7.1 Зарезервировать ID для будущих агрегатов:**

Создать файл `docs/aggregate-registry.md`:

```markdown
# Aggregate Registry

| ID   | Name            | Category    | Status     | Owner        |
| ---- | --------------- | ----------- | ---------- | ------------ |
| a001 | connection_1c   | integration | production | Team Backend |
| a002 | user_profile    | core        | planned    | Team Auth    |
| a003 | invoice         | payment     | planned    | Team Finance |
| a004 | product_catalog | catalog     | planned    | Team Product |
| ...  | ...             | ...         | ...        | ...          |
| a099 | (reserved)      | -           | -          | -            |

## ID Allocation Rules

- a001-a020: Integration & External Systems
- a021-a040: Core Domain (Users, Auth, Settings)
- a041-a060: Payment & Financial
- a061-a080: Catalog & Inventory
- a081-a100: Orders & Sales
- a101-a120: Reports & Analytics
- a121-a140: Notifications & Communication
- a141-a160: Security & Permissions
- a161-a999: Future expansion
```

---

### ШАГ 8: Создать инструменты валидации

**8.1 Создать проект валидатора:**

```
tools/
  └── aggregate-validator/
      ├── Cargo.toml
      └── src/
          ├── main.rs
          ├── scanner.rs
          ├── validator.rs
          └── report.rs
```

**8.2 `tools/aggregate-validator/Cargo.toml`:**

```toml
[package]
name = "aggregate-validator"
version = "0.1.0"
edition = "2021"

[dependencies]
walkdir = "2"
regex = "1"
toml = "0.8"
serde = { version = "1", features = ["derive"] }
colored = "2"
anyhow = "1"
syn = { version = "2", features = ["full", "parsing"] }
```

**8.3 Минимальный `main.rs`:**

```rust
use std::path::PathBuf;
use colored::*;

fn main() -> anyhow::Result<()> {
    let project_root = std::env::current_dir()?;

    println!("{}", "🔍 VSA Aggregate Validator v0.1.0".bold().cyan());
    println!("📁 Project: {}\n", project_root.display());

    let aggregates = scan_aggregates(&project_root)?;

    println!("📊 Found {} aggregates\n", aggregates.len());

    let mut errors = 0;
    let mut warnings = 0;

    for agg in &aggregates {
        let validation = validate_aggregate(agg)?;

        if !validation.errors.is_empty() {
            errors += validation.errors.len();
            println!("{} {}", "❌".red(), agg.id.bold());
            for err in validation.errors {
                println!("   └─ {}", err);
            }
        } else if !validation.warnings.is_empty() {
            warnings += validation.warnings.len();
            println!("{} {}", "⚠️ ".yellow(), agg.id.bold());
        } else {
            println!("{} {}", "✅".green(), agg.id);
        }
    }

    println!("\n{}", "━".repeat(60).dimmed());
    println!("Errors: {}  Warnings: {}", errors, warnings);

    if errors > 0 {
        std::process::exit(1);
    }

    Ok(())
}

fn scan_aggregates(root: &PathBuf) -> anyhow::Result<Vec<Aggregate>> {
    // TODO: Implement scanning
    Ok(vec![])
}

fn validate_aggregate(agg: &Aggregate) -> anyhow::Result<ValidationResult> {
    // TODO: Implement validation
    Ok(ValidationResult::default())
}

struct Aggregate {
    id: String,
    name: String,
}

#[derive(Default)]
struct ValidationResult {
    errors: Vec<String>,
    warnings: Vec<String>,
}
```

---

## 📋 ЧЕКЛИСТ МИГРАЦИИ

### Подготовка

- [ ] Создать ветку `feature/aggregate-standard`
- [ ] Сделать backup БД
- [ ] Закоммитить текущее состояние

### Реализация

- [ ] ✅ ШАГ 1: Создать `_common` модуль
- [ ] ✅ ШАГ 2: Переименовать `connection_1c` → `a001_connection_1c`
- [ ] ✅ ШАГ 3: Добавить модуль `meta`
- [ ] ✅ ШАГ 4: Создать `_aggregate.toml`
- [ ] ✅ ШАГ 5: Переименовать таблицы БД
- [ ] ✅ ШАГ 6: Обновить `Cargo.toml`
- [ ] ✅ ШАГ 7: Создать `aggregate-registry.md`
- [ ] ✅ ШАГ 8: Создать базовый валидатор

### Тестирование

- [ ] Проверить компиляцию: `cargo check --workspace`
- [ ] Запустить тесты: `cargo test --workspace`
- [ ] Запустить сервер: `cargo run --bin server`
- [ ] Проверить UI: открыть в браузере
- [ ] Проверить БД: подключиться и проверить данные
- [ ] Запустить валидатор: `cargo run --bin aggregate-validator`

### Документация

- [ ] Обновить `README.md` с новой структурой
- [ ] Добавить `aggregate-standard.md` в корень проекта
- [ ] Создать `CONTRIBUTING.md` с правилами добавления агрегатов

### Финализация

- [ ] Code review
- [ ] Merge в main
- [ ] Тегнуть версию: `v1.0.0-aggregate-standard`

---

## 🎯 Результат миграции

После миграции проект будет:

✅ **Стандартизирован:**

- Все агрегаты следуют единому паттерну
- Префиксы во всех слоях
- Метаданные в `_aggregate.toml`

✅ **Валидируем:**

- Автоматическая проверка структуры
- CI/CD интеграция
- Раннее выявление ошибок

✅ **Готов к сканированию:**

- Project Explorer сможет парсить за O(n)
- Все метаданные доступны
- Граф зависимостей построим

✅ **Масштабируем:**

- Легко добавлять новые агрегаты
- От 1 до 999 агрегатов без проблем
- Чёткая структура ID allocation

---

## 📞 Вопросы?

Если что-то непонятно в процессе миграции:

1. Проверьте `aggregate-standard.md`
2. Посмотрите на `a001_connection_1c` как reference implementation
3. Запустите валидатор для диагностики
