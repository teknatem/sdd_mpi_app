---
title: Runbook - Adding Fields to LLM Chat Feature
version: 1.0
date: 2026-01-17
tags: [runbook, llm, chat, ddd, vertical-slice]
type: procedure
---

# Runbook: Adding Fields to LLM Chat Feature

## Purpose

Step-by-step procedure for extending the LLM chat feature with new fields (e.g., model selection, confidence tracking). Follows DDD + Vertical Slice Architecture patterns used in the project.

## Prerequisites

- Rust toolchain installed
- SQLite database initialized
- Backend and frontend can compile successfully
- Understanding of DDD aggregates and repository patterns

## Steps

### 1. Update Contracts (Domain Layer)

#### 1.1 Update Aggregate Structures

**File:** `crates/contracts/src/domain/a018_llm_chat/aggregate.rs`

- Add new fields to `LlmChat` struct (for chat-level fields)
- Add new fields to `LlmChatMessage` struct (for message-level fields)
- Update constructors (`new_for_insert`, `new_with_id`, `new_with_metadata`)

**Example:**
```rust
pub struct LlmChat {
    pub base: BaseAggregate<LlmChatId>,
    pub agent_id: LlmAgentId,
    pub model_name: String,  // NEW
}

pub struct LlmChatMessage {
    pub id: Uuid,
    pub chat_id: LlmChatId,
    pub role: ChatRole,
    pub content: String,
    pub tokens_used: Option<i32>,
    pub model_name: Option<String>,   // NEW
    pub confidence: Option<f64>,      // NEW
    pub created_at: DateTime<Utc>,
}
```

#### 1.2 Update Metadata JSON

**File:** `crates/contracts/src/domain/a018_llm_chat/metadata.json`

Add field definitions in the `fields` array:
```json
{
  "name": "model_name",
  "rust_type": "String",
  "field_type": "primitive",
  "source": "custom",
  "ui": {
    "label": "Модель по умолчанию",
    "visible_in_list": true,
    "visible_in_form": true
  },
  "validation": {
    "required": true,
    "max_length": 100
  }
}
```

#### 1.3 Verify Compilation

```powershell
cargo check -p contracts
```

### 2. Create SQL Migration

**File:** `migrate_a018_llm_chat_v2.sql` (or similar)

```sql
-- Add new columns
ALTER TABLE a018_llm_chat ADD COLUMN model_name TEXT NOT NULL DEFAULT 'gpt-4o';
ALTER TABLE a018_llm_chat_message ADD COLUMN model_name TEXT;
ALTER TABLE a018_llm_chat_message ADD COLUMN confidence REAL;

-- Add indexes for performance
CREATE INDEX IF NOT EXISTS idx_a018_llm_chat_model_name ON a018_llm_chat(model_name);
CREATE INDEX IF NOT EXISTS idx_a018_llm_chat_message_model_name ON a018_llm_chat_message(model_name);
```

**Apply migration:**
```powershell
Get-Content migrate_a018_llm_chat_v2.sql | sqlite3 marketplace.db
```

**Verify:**
```powershell
sqlite3 marketplace.db "PRAGMA table_info(a018_llm_chat);"
sqlite3 marketplace.db "PRAGMA table_info(a018_llm_chat_message);"
```

### 3. Update Backend Repository

**File:** `crates/backend/src/domain/a018_llm_chat/repository.rs`

#### 3.1 Update SeaORM Models

```rust
mod chat {
    #[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
    #[sea_orm(table_name = "a018_llm_chat")]
    pub struct Model {
        // ... existing fields
        pub model_name: String,  // NEW
    }
}

mod message {
    #[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
    #[sea_orm(table_name = "a018_llm_chat_message")]
    pub struct Model {
        // ... existing fields
        pub model_name: Option<String>,  // NEW
        pub confidence: Option<f64>,     // NEW
    }
}
```

#### 3.2 Update From Implementations

```rust
impl From<chat::Model> for LlmChat {
    fn from(m: chat::Model) -> Self {
        LlmChat {
            // ... existing mappings
            model_name: m.model_name,  // NEW
        }
    }
}

impl From<message::Model> for LlmChatMessage {
    fn from(m: message::Model) -> Self {
        LlmChatMessage {
            // ... existing mappings
            model_name: m.model_name,   // NEW
            confidence: m.confidence,   // NEW
        }
    }
}
```

#### 3.3 Update Insert/Update Functions

```rust
pub async fn insert(db: &DatabaseConnection, chat: &LlmChat) -> Result<(), DbErr> {
    let active_model = chat::ActiveModel {
        // ... existing fields
        model_name: Set(chat.model_name.clone()),  // NEW
    };
    active_model.insert(db).await?;
    Ok(())
}

pub async fn insert_message(db: &DatabaseConnection, message: &LlmChatMessage) -> Result<(), DbErr> {
    let active_model = message::ActiveModel {
        // ... existing fields
        model_name: Set(message.model_name.clone()),  // NEW
        confidence: Set(message.confidence),          // NEW
    };
    active_model.insert(db).await?;
    Ok(())
}
```

### 4. Update Backend Service

**File:** `crates/backend/src/domain/a018_llm_chat/service.rs`

#### 4.1 Update DTOs

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmChatDto {
    pub id: Option<String>,
    pub description: String,
    pub agent_id: String,
    pub model_name: Option<String>,  // NEW
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendMessageRequest {
    pub content: String,
    pub model_name: Option<String>,  // NEW
}
```

#### 4.2 Update Service Logic

```rust
pub async fn create(dto: LlmChatDto) -> anyhow::Result<Uuid> {
    let agent = agent_repository::find_by_id(&agent_id).await?;
    
    // Use model from DTO or default from agent
    let model_name = dto.model_name.unwrap_or_else(|| agent.model_name.clone());
    
    let mut aggregate = LlmChat::new_for_insert(code, dto.description, agent_id, model_name);
    // ... rest of logic
}

pub async fn send_message(chat_id: &str, request: SendMessageRequest) -> anyhow::Result<LlmChatMessage> {
    let chat = repository::find_by_id(&db, &chat_id_obj).await?;
    
    // Model selection priority: request -> chat -> agent
    let model_to_use = request.model_name
        .unwrap_or_else(|| chat.model_name.clone());
    
    // Use model_to_use when calling LLM
    let llm_response = provider.chat_completion(llm_messages).await?;
    
    // Save with metadata
    let assistant_msg = LlmChatMessage::new_with_metadata(
        chat_id_obj,
        ChatRole::Assistant,
        llm_response.content,
        llm_response.tokens_used,
        Some(model_to_use),
        llm_response.confidence,
    );
    // ... rest of logic
}
```

### 5. Update LLM Provider (if needed)

**File:** `crates/backend/src/shared/llm/types.rs`

```rust
pub struct LlmResponse {
    pub content: String,
    pub tokens_used: Option<i32>,
    pub model: String,
    pub finish_reason: Option<String>,
    pub confidence: Option<f64>,  // NEW
}
```

**File:** `crates/backend/src/shared/llm/openai_provider.rs`

```rust
async fn chat_completion(&self, messages: Vec<ChatMessage>) -> Result<LlmResponse, LlmError> {
    let request = CreateChatCompletionRequestArgs::default()
        .model(&self.model)
        .messages(openai_messages)
        .logprobs(true)        // NEW: request logprobs
        .top_logprobs(1)       // NEW
        .build()?;
    
    let response = self.client.chat().create(request).await?;
    
    // Calculate confidence from logprobs
    let confidence = choice.logprobs.as_ref().and_then(|logprobs| {
        if let Some(content_logprobs) = &logprobs.content {
            let sum: f64 = content_logprobs.iter()
                .map(|token| (token.logprob as f64).exp())
                .sum();
            let count = content_logprobs.len();
            Some(sum / count as f64)
        } else {
            None
        }
    });
    
    Ok(LlmResponse {
        content,
        tokens_used,
        model: response.model.clone(),
        finish_reason,
        confidence,  // NEW
    })
}
```

### 6. Update API Handlers

**File:** `crates/backend/src/api/handlers/a018_llm_chat.rs`

Update handler to pass through new DTO fields:

```rust
pub async fn send_message(
    Path(id): Path<String>,
    Json(payload): Json<SendMessageRequest>,
) -> Result<Json<LlmChatMessage>, StatusCode> {
    // Now payload includes model_name
    match service::send_message(&id, payload).await {
        Ok(msg) => Ok(Json(msg)),
        Err(e) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}
```

### 7. Update Frontend UI

**File:** `crates/frontend/src/domain/a018_llm_chat/ui/list/mod.rs`

#### 7.1 Add State for New Fields

```rust
let (new_chat_model, set_new_chat_model) = signal(String::new());
```

#### 7.2 Add UI Controls

```rust
// Model dropdown
<select
    prop:value=move || new_chat_model.get()
    on:change=move |ev| set_new_chat_model.set(event_target_value(&ev))
>
    <option value="gpt-4o">"gpt-4o"</option>
    <option value="gpt-4o-mini">"gpt-4o-mini"</option>
    // ... more options
</select>
```

#### 7.3 Display New Fields in Messages

```rust
<For each=move || messages.get() key=|msg| msg.id let:msg>
    <div>
        <div>{msg.content.clone()}</div>
        {move || {
            let mut meta_parts = Vec::new();
            if let Some(t) = tokens { meta_parts.push(format!("🎫 {} tokens", t)); }
            if let Some(m) = &model { meta_parts.push(format!("🤖 {}", m)); }
            if let Some(c) = conf { meta_parts.push(format!("📊 {:.1}%", c * 100.0)); }
            if !meta_parts.is_empty() {
                Some(view! { <div>{meta_parts.join(" • ")}</div> })
            } else {
                None
            }
        }}
    </div>
</For>
```

#### 7.4 Update API Calls

```rust
async fn create_chat(description: &str, agent_id: &str, model_name: &str) -> Result<(), String> {
    let model_value: Option<&str> = if model_name.trim().is_empty() {
        None
    } else {
        Some(model_name)
    };
    
    let dto = serde_json::json!({
        "description": description,
        "agent_id": agent_id,
        "model_name": model_value  // NEW
    });
    // ... send request
}
```

### 8. Verify & Test

#### 8.1 Backend Compilation

```powershell
cargo check -p backend
```

#### 8.2 Frontend Compilation

```powershell
cargo check -p frontend
```

#### 8.3 API Testing

```powershell
# Create chat with model
$body = @{ description = 'Test'; agent_id = '...'; model_name = 'gpt-4o-mini' } | ConvertTo-Json
Invoke-RestMethod -Method Post -Uri http://localhost:3000/api/a018-llm-chat -Body $body -ContentType 'application/json'

# Send message
$body = @{ content = 'Hello' } | ConvertTo-Json
Invoke-RestMethod -Method Post -Uri http://localhost:3000/api/a018-llm-chat/<id>/messages -Body $body -ContentType 'application/json'

# Verify response includes new fields
Invoke-RestMethod http://localhost:3000/api/a018-llm-chat/<id>/messages
```

#### 8.4 UI Testing

1. Restart backend: `powershell -File tools/run_backend.ps1`
2. Пересобрать фронт: `powershell -File tools/build_frontend.ps1` (всё на http://localhost:3000)
3. Navigate to LLM Chat in UI
4. Create new chat with model dropdown
5. Send message and verify metadata display

## Common Issues

See:
- [[KI_openai-logprobs-type-mismatch_2026-01-17]]
- [[LL_frontend-null-handling_2026-01-17]]

## Rollback

If migration needs to be reverted:

```sql
-- Rollback migration
ALTER TABLE a018_llm_chat DROP COLUMN model_name;
ALTER TABLE a018_llm_chat_message DROP COLUMN model_name;
ALTER TABLE a018_llm_chat_message DROP COLUMN confidence;
```

Then revert code changes via git.
