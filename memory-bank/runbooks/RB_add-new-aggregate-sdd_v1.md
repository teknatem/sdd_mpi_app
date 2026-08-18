---
type: runbook
version: 1
topic: Adding New Aggregate in DDD/VSA Architecture
date: 2026-01-20
tags: [ddd, vsa, aggregate, architecture, leptos, rust]
---

# Runbook: Adding New Aggregate (DDD/VSA)

## Purpose
Step-by-step procedure for adding a new aggregate to the Leptos Marketplace project following Domain-Driven Design and Vertical Slice Architecture patterns.

## Prerequisites
- Understanding of DDD concepts (aggregates, entities, value objects)
- Familiarity with Rust, Leptos, Sea-ORM
- Database access for migrations
- Project follows aXXX numbering scheme (e.g., a019_llm_artifact)

## Naming Convention
- Aggregate index: `aXXX` where XXX is next available number (e.g., a019)
- Directory names: `aXXX_<domain_name>` (e.g., a019_llm_artifact)
- Table names: `aXXX_<domain_name>` (e.g., a019_llm_artifact)

## Step-by-Step Process

### Phase 1: Contracts Layer

#### 1.1 Create Aggregate Structure
**Location**: `crates/contracts/src/domain/aXXX_<name>/`

**Files to create**:
1. `aggregate.rs`:
   - Define aggregate ID type implementing `AggregateId` trait
   - Define main aggregate struct with `BaseAggregate`
   - Define enums for types/statuses
   - Implement `AggregateRoot` trait
   - Create DTO types (e.g., `ListItem`)

2. `metadata.json`:
   - Schema version, entity info
   - UI labels (Russian + English)
   - Field definitions with types, validation, UI hints
   - AI descriptions and related entities

3. `mod.rs`:
   - Export all public types
   - Add `#[cfg(feature = "metadata_gen")]` for metadata_gen module

**Example structure**:
```rust
// aggregate.rs
pub struct MyAggregateId(pub Uuid);
impl AggregateId for MyAggregateId { /* ... */ }

pub struct MyAggregate {
    pub base: BaseAggregate<MyAggregateId>,
    // ... custom fields
}

impl AggregateRoot for MyAggregate { /* ... */ }
```

#### 1.2 Register in Contracts
**File**: `crates/contracts/src/domain/mod.rs`
```rust
pub mod aXXX_<name>;
```

### Phase 2: Database Schema

#### 2.1 Create Migration File
**Location**: Root directory `migrate_aXXX_<name>.sql`

**Template**:
```sql
-- Migration: Create aXXX_<name> table
-- Description: <Purpose>
-- Date: YYYY-MM-DD

CREATE TABLE IF NOT EXISTS aXXX_<name> (
    id TEXT PRIMARY KEY,
    code TEXT NOT NULL UNIQUE,
    description TEXT NOT NULL,
    comment TEXT,
    
    -- Custom fields
    -- ...
    
    -- Standard BaseAggregate fields
    is_deleted INTEGER NOT NULL DEFAULT 0,
    is_posted INTEGER NOT NULL DEFAULT 0,
    created_at TEXT,
    updated_at TEXT,
    version INTEGER NOT NULL DEFAULT 1,
    
    -- Foreign keys if needed
    FOREIGN KEY (other_id) REFERENCES other_table(id)
);

-- Indexes
CREATE INDEX IF NOT EXISTS idx_aXXX_<name>_code ON aXXX_<name>(code);
CREATE INDEX IF NOT EXISTS idx_aXXX_<name>_is_deleted ON aXXX_<name>(is_deleted);
-- Add indexes for foreign keys and frequently queried fields
```

#### 2.2 Related Table Migrations
If integrating with existing tables (e.g., adding references), create separate migration files like `migrate_aYYY_<name>_vN.sql`.

### Phase 3: Backend Repository

#### 3.1 Create Repository
**Location**: `crates/backend/src/domain/aXXX_<name>/repository.rs`

**Required components**:
1. **Sea-ORM Entity Model**:
```rust
mod entity_name {
    use sea_orm::entity::prelude::*;
    
    #[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
    #[sea_orm(table_name = "aXXX_<name>")]
    pub struct Model {
        #[sea_orm(primary_key, auto_increment = false)]
        pub id: String,
        // ... all fields matching SQL schema
    }
    
    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}
    
    impl ActiveModelBehavior for ActiveModel {}
}
```

2. **From Implementation**:
```rust
impl From<entity::Model> for MyAggregate {
    fn from(m: entity::Model) -> Self {
        // Convert Sea-ORM model to domain aggregate
    }
}
```

3. **CRUD Functions**:
- `list_all(db) -> Result<Vec<MyAggregate>>`
- `find_by_id(db, id) -> Result<Option<MyAggregate>>`
- `insert(db, aggregate) -> Result<()>`
- `update(db, aggregate) -> Result<()>`
- `soft_delete(db, id) -> Result<()>`
- Add custom queries as needed (e.g., `list_by_parent_id`)

#### 3.2 Create Module Files
**Location**: `crates/backend/src/domain/aXXX_<name>/`

1. `mod.rs`:
```rust
pub mod repository;
pub mod service;
```

2. Register in `crates/backend/src/domain/mod.rs`:
```rust
pub mod aXXX_<name>;
```

### Phase 4: Backend Service

#### 4.1 Create Service Layer
**Location**: `crates/backend/src/domain/aXXX_<name>/service.rs`

**Components**:
1. **DTO for API**:
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MyAggregateDto {
    pub id: Option<String>,
    pub code: Option<String>,
    // ... fields needed for create/update
}
```

2. **Service Functions**:
```rust
pub async fn create(dto: MyAggregateDto) -> anyhow::Result<Uuid> {
    // 1. Validate input
    // 2. Create aggregate
    // 3. Call repository::insert
    // 4. Return ID
}

pub async fn update(dto: MyAggregateDto) -> anyhow::Result<()> {
    // 1. Load existing
    // 2. Update fields
    // 3. Validate
    // 4. Call repository::update
}

pub async fn delete(id: &str) -> anyhow::Result<()>
pub async fn get_by_id(id: &str) -> anyhow::Result<Option<MyAggregate>>
pub async fn list_all() -> anyhow::Result<Vec<MyAggregate>>
```

### Phase 5: Backend API

#### 5.1 Create API Handlers
**Location**: `crates/backend/src/api/handlers/aXXX_<name>.rs`

**Standard endpoints**:
```rust
// GET /api/aXXX-<name>
pub async fn list_all() -> Result<Json<Vec<MyAggregate>>, StatusCode>

// GET /api/aXXX-<name>/:id
pub async fn get_by_id(Path(id): Path<String>) -> Result<Json<MyAggregate>, StatusCode>

// POST /api/aXXX-<name>
pub async fn upsert(Json(dto): Json<MyAggregateDto>) -> Result<Json<Value>, StatusCode>

// DELETE /api/aXXX-<name>/:id
pub async fn delete(Path(id): Path<String>) -> Result<(), StatusCode>
```

#### 5.2 Register Handlers
1. **Add to handlers/mod.rs**:
```rust
pub mod aXXX_<name>;
```

2. **Register routes in api/routes.rs**:
```rust
// AXXX <Name> handlers
.route(
    "/api/aXXX-<name>",
    get(handlers::aXXX_<name>::list_all)
        .post(handlers::aXXX_<name>::upsert),
)
.route(
    "/api/aXXX-<name>/:id",
    get(handlers::aXXX_<name>::get_by_id)
        .delete(handlers::aXXX_<name>::delete),
)
```

### Phase 6: Frontend List UI

#### 6.1 Create List Component
**Location**: `crates/frontend/src/domain/aXXX_<name>/ui/list/mod.rs`

**Components**:
1. **DTO/Types** (if different from contracts):
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MyAggregateListItem {
    // Fields for display in list
}
```

2. **API Functions**:
```rust
async fn fetch_items() -> Result<Vec<MyAggregateListItem>, String>
async fn delete_item(id: &str) -> Result<(), String>
```

3. **Leptos Component**:
```rust
#[component]
#[allow(non_snake_case)]
pub fn MyAggregateList() -> impl IntoView {
    // State with RwSignal
    // Effect for loading
    // Table with columns
    // Buttons for actions
}
```

### Phase 7: Frontend Details UI

#### 7.1 Create Details Structure (MVVM)
**Location**: `crates/frontend/src/domain/aXXX_<name>/ui/details/`

**Files required**:

1. **model.rs** - API functions:
```rust
pub async fn fetch_item(id: &str) -> Result<MyAggregate, String>
pub async fn update_item(dto: MyAggregateDto) -> Result<(), String>
```

2. **view_model.rs** - Reactive state:
```rust
#[derive(Clone, Copy)]
pub struct MyAggregateDetailsVm {
    pub item: RwSignal<Option<MyAggregate>>,
    pub error: RwSignal<Option<String>>,
    pub is_editing: RwSignal<bool>,
    pub is_saving: RwSignal<bool>,
}

impl MyAggregateDetailsVm {
    pub fn new() -> Self { /* ... */ }
}
```

3. **view.rs** - Main component:
```rust
#[component]
#[allow(non_snake_case)]
pub fn MyAggregateDetails(id: String, on_close: Callback<()>) -> impl IntoView {
    // VM instance
    // Load effect
    // Save handler
    // UI with tabs/forms
}
```

4. **mod.rs**:
```rust
mod model;
mod view;
mod view_model;

pub use view::MyAggregateDetails;
pub use view_model::MyAggregateDetailsVm;
```

#### 7.2 Create UI Module Structure
**Files**:
1. `crates/frontend/src/domain/aXXX_<name>/ui/mod.rs`:
```rust
pub mod details;
pub mod list;

pub use list::MyAggregateList;
pub use details::MyAggregateDetails;
```

2. `crates/frontend/src/domain/aXXX_<name>/mod.rs`:
```rust
pub mod ui;
pub use ui::{MyAggregateDetails, MyAggregateList};
```

3. Register in `crates/frontend/src/domain/mod.rs`:
```rust
pub mod aXXX_<name>;
```

### Phase 8: Register Frontend Routes

#### 8.1 Update Tab Registry
**Location**: `crates/frontend/src/layout/tabs/registry.rs`

1. **Add imports**:
```rust
use crate::domain::aXXX_<name>::ui::details::MyAggregateDetails;
use crate::domain::aXXX_<name>::ui::list::MyAggregateList;
```

2. **Register routes in `render_tab_content`**:
```rust
// AXXX: <Name>
"aXXX_<name>" => view! { <MyAggregateList /> }.into_any(),
k if k.starts_with("aXXX_<name>_detail_") => {
    let id = k.strip_prefix("aXXX_<name>_detail_").unwrap().to_string();
    log!("✅ Creating MyAggregateDetails with id: {}", id);
    view! {
        <MyAggregateDetails
            id=id
            on_close=Callback::new({
                let key_for_close = key_for_close.clone();
                move |_| {
                    tabs_store.close_tab(&key_for_close);
                }
            })
        />
    }
    .into_any()
}
```

## Deployment Checklist

- [ ] All contracts files created
- [ ] Migration SQL created
- [ ] Backend repository with Sea-ORM models
- [ ] Backend service with business logic
- [ ] Backend API handlers
- [ ] Backend routes registered
- [ ] Frontend list UI component
- [ ] Frontend details UI (MVVM: model, view_model, view)
- [ ] Frontend routes registered in tab registry
- [ ] Module exports in all mod.rs files
- [ ] Run database migrations
- [ ] Recompile backend (cargo build)
- [ ] Recompile frontend (trunk build)
- [ ] Test create/read/update/delete operations
- [ ] Check linter errors

## Common Pitfalls

1. **Forgetting module registrations**: Each layer needs `pub mod aXXX_<name>;` added
2. **Mismatched field names**: SQL schema vs Sea-ORM model vs Aggregate struct
3. **Missing indexes**: Foreign keys and frequently queried fields need indexes
4. **DateTime handling**: Use RFC3339 strings in SQLite, parse to DateTime<Utc>
5. **UUID parsing errors**: Always handle parse failures gracefully
6. **Tab key naming**: Must match pattern in registry.rs (e.g., `aXXX_<name>_detail_{id}`)

## Example Implementation
See: `a019_llm_artifact` for complete reference implementation.

## Related Documentation
- [[2026-01-20_session-debrief_llm-artifact-implementation|Session Debrief: LLM Artifact]]
- Project .cursorrules file for architecture patterns
