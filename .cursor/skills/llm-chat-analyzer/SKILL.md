---
name: llm-chat-analyzer
description: >
  Инструмент отладки в dev-контексте (Cursor IDE): читает артефакты работы
  программы из SQLite-таблиц агрегата a018_llm_chat, анализирует паттерны
  inference-пайплайна (confidence, latency, token usage, tool calls) и
  формирует конкретные изменения Rust-кода и system_prompt для улучшения
  поведения программы в следующих запусках (продакшн).
---

# LLM Chat Dialogue Analyzer

Скилл для итеративного улучшения inference-пайплайна `a018_llm_chat`.
Работает в три фазы: **Extract → Analyze → Recommend**.

---

## Два контекста — ключевое разграничение

Перед началом работы зафиксируй в голове эту модель и не путай контексты:

```
┌─────────────────────────────────────────────────────────────────┐
│  КОНТЕКСТ A — РАЗРАБОТКА  (где работает этот скилл)             │
│                                                                 │
│  • Cursor-агент в IDE                                           │
│  • Читает файлы проекта (Read, Grep, Glob)                      │
│  • Запускает shell-команды (sqlite3, PowerShell)                │
│  • Редактирует Rust-код в исходниках                            │
│  • Имеет доступ к dev-копии БД                                  │
└──────────────────────────────┬──────────────────────────────────┘
                               │ анализирует артефакты ↓
                               │ вносит улучшения в код ↑
┌──────────────────────────────▼──────────────────────────────────┐
│  КОНТЕКСТ B — ВЫПОЛНЕНИЕ ПРОГРАММЫ  (что мы улучшаем)           │
│                                                                 │
│  • Запущенный Rust-бэкенд (cargo run / продакшн-бинарник)       │
│  • Обрабатывает HTTP-запросы пользователей                      │
│  • Вызывает OpenAI API в функции send_message() (service.rs)    │
│  • Сохраняет каждый ответ LLM в БД:                             │
│    role, content, tokens_used, confidence, duration_ms          │
│  • Вызывает tools: list_entities, get_entity_schema,            │
│    get_join_hint — это инструменты ПРОДАКШН-LLM, не Cursor      │
└─────────────────────────────────────────────────────────────────┘
```

**Правило чтения этого скилла:**

- `sqlite3` запросы, `Read`, `Shell` — это действия **Cursor-агента** (Контекст A)
- `confidence`, `duration_ms`, `tokens_used`, `tool calls`, `MAX_TOOL_ITERATIONS` — это метрики и параметры **работающей программы** (Контекст B), сохранённые в БД как артефакты её работы
- "tool calls" в тексте скилла **всегда означает** вызовы инструментов продакшн-LLM (`list_entities` и др.), **никогда** — инструменты Cursor-агента

## Когда использовать

- "проанализируй диалоги llm"
- "что улучшить в промптах агентов"
- "analyze llm chat dialogues"
- "почему llm отвечает медленно / дорого / неточно"
- "оптимизировать inference"

---

## Фаза 0: Pre-flight — найти БД `[Контекст A: Cursor-агент]`

### Шаг 0.1: Определить путь к БД

Прочитай файл `config.toml` в корне проекта:

```
Read: config.toml
```

**Логика разрешения пути:**

- Основной режим: секции `[database]` нет, есть `[data].root` → база лежит в
  `<root>/db/app.db` (путь абсолютный, например `F:/data/sdd_mpi_app/db/app.db`).
- Отклонение: задан `[database].path` — использовать его как есть (он обязан быть
  абсолютным; относительный путь приложение отвергает при старте).

Если `config.toml` не найден или ни одна из настроек не задана — БД найти нельзя,
спроси путь у пользователя.

Боевая БД лежит вне репозитория: `F:/data/sdd_mpi_app/app.db`. Проверь реальное
существование файла перед запросами:

```powershell
# Проверить что файл существует
Test-Path "F:/data/sdd_mpi_app/app.db"
# Если нет — искать в корне данных
Get-ChildItem -Recurse -Filter "app.db" -Path "F:\data" -ErrorAction SilentlyContinue | Select-Object FullName
```

Зафиксируй путь как переменную `$DB` для всех дальнейших запросов.

### Шаг 0.2: Проверить наличие данных

```powershell
sqlite3 $DB "SELECT COUNT(*) as chats FROM a018_llm_chat WHERE is_deleted=0;"
sqlite3 $DB "SELECT COUNT(*) as messages FROM a018_llm_chat_message;"
sqlite3 $DB "SELECT COUNT(*) as assistant FROM a018_llm_chat_message WHERE role='assistant';"
```

Если `sqlite3` не найден:

```powershell
# Установить через scoop или winget
winget install SQLite.SQLite
# Или использовать Python
python -c "import sqlite3, sys; conn=sqlite3.connect(sys.argv[1]); print(conn.execute('SELECT COUNT(*) FROM a018_llm_chat_message').fetchone())" "$DB"
```

**Минимальный порог для анализа:** не менее 3 чатов и 10 сообщений. Если данных меньше — сообщи пользователю и предложи провести несколько тестовых диалогов перед анализом.

---

## Фаза 1: Извлечение данных `[Контекст A: Cursor-агент читает артефакты Контекста B]`

Выполни все запросы ниже. Для каждого запроса используй команду:

```powershell
sqlite3 -column -header $DB "<SQL>"
```

### Запрос 1A: Сводная статистика по агентам

```sql
SELECT
    a.description          AS agent,
    a.model_name           AS default_model,
    a.temperature          AS temp,
    a.max_tokens           AS max_tok,
    COUNT(DISTINCT c.id)   AS chats,
    COUNT(m.id)            AS responses,
    ROUND(AVG(m.tokens_used), 0)    AS avg_tokens,
    MAX(m.tokens_used)              AS max_tokens_used,
    ROUND(AVG(m.confidence), 3)     AS avg_confidence,
    MIN(m.confidence)               AS min_confidence,
    ROUND(AVG(m.duration_ms), 0)    AS avg_duration_ms,
    MAX(m.duration_ms)              AS max_duration_ms
FROM a017_llm_agent a
JOIN a018_llm_chat c ON c.agent_id = a.id AND c.is_deleted = 0
JOIN a018_llm_chat_message m ON m.chat_id = c.id AND m.role = 'assistant'
GROUP BY a.id
ORDER BY responses DESC;
```

### Запрос 1B: System prompts всех агентов

```sql
SELECT
    a.description AS agent,
    a.model_name,
    a.temperature,
    a.max_tokens,
    COALESCE(a.system_prompt, '[НЕТ SYSTEM PROMPT]') AS system_prompt
FROM a017_llm_agent a
WHERE a.is_deleted = 0
ORDER BY a.description;
```

### Запрос 1C: Полные диалоги (последние 20 чатов)

```sql
SELECT
    c.description   AS chat,
    a.description   AS agent,
    m.role,
    LENGTH(m.content) AS content_len,
    m.content,
    m.tokens_used,
    m.confidence,
    m.duration_ms,
    m.created_at
FROM a018_llm_chat_message m
JOIN a018_llm_chat c ON m.chat_id = c.id
JOIN a017_llm_agent a ON c.agent_id = a.id
WHERE c.id IN (
    SELECT id FROM a018_llm_chat
    WHERE is_deleted = 0
    ORDER BY created_at DESC
    LIMIT 20
)
ORDER BY c.id, m.created_at;
```

### Запрос 1D: Типы первых пользовательских вопросов

```sql
SELECT
    m.content   AS first_user_message,
    c.description AS chat,
    a.description AS agent,
    m.created_at
FROM a018_llm_chat_message m
JOIN a018_llm_chat c ON m.chat_id = c.id
JOIN a017_llm_agent a ON c.agent_id = a.id
WHERE m.role = 'user'
  AND m.id IN (
      SELECT MIN(id) FROM a018_llm_chat_message
      WHERE role = 'user'
      GROUP BY chat_id
  )
ORDER BY m.created_at DESC
LIMIT 50;
```

### Запрос 1E: Проблемные ответы (низкая уверенность / долгий ответ / много токенов)

```sql
SELECT
    m.content,
    m.confidence,
    m.duration_ms,
    m.tokens_used,
    c.description AS chat,
    a.description AS agent,
    m.created_at
FROM a018_llm_chat_message m
JOIN a018_llm_chat c ON m.chat_id = c.id
JOIN a017_llm_agent a ON c.agent_id = a.id
WHERE m.role = 'assistant'
  AND (
      m.confidence < 0.7
      OR m.duration_ms > 10000
      OR m.tokens_used > 3000
      OR m.confidence IS NULL
  )
ORDER BY m.confidence ASC NULLS FIRST, m.duration_ms DESC
LIMIT 30;
```

### Запрос 1F: Длина диалогов и паттерны использования инструментов

```sql
SELECT
    c.description   AS chat,
    a.description   AS agent,
    COUNT(m.id)     AS total_messages,
    SUM(CASE WHEN m.role = 'user' THEN 1 ELSE 0 END)      AS user_msgs,
    SUM(CASE WHEN m.role = 'assistant' THEN 1 ELSE 0 END) AS assistant_msgs,
    SUM(CASE WHEN m.role = 'system' THEN 1 ELSE 0 END)    AS system_msgs,
    MAX(m.created_at) AS last_activity
FROM a018_llm_chat c
JOIN a017_llm_agent a ON c.agent_id = a.id
JOIN a018_llm_chat_message m ON m.chat_id = c.id
WHERE c.is_deleted = 0
GROUP BY c.id
ORDER BY total_messages DESC
LIMIT 20;
```

### Запрос 1G: Распределение токенов по моделям

```sql
SELECT
    COALESCE(m.model_name, c.model_name, a.model_name, 'unknown') AS model,
    COUNT(m.id)                             AS responses,
    ROUND(AVG(m.tokens_used), 0)            AS avg_tokens,
    ROUND(AVG(m.duration_ms) / 1000.0, 1)  AS avg_duration_sec,
    ROUND(AVG(m.confidence), 3)             AS avg_confidence
FROM a018_llm_chat_message m
JOIN a018_llm_chat c ON m.chat_id = c.id
JOIN a017_llm_agent a ON c.agent_id = a.id
WHERE m.role = 'assistant'
GROUP BY 1
ORDER BY responses DESC;
```

---

## Фаза 2: Анализ — пороговые значения и диагностика `[Контекст A: Cursor-агент интерпретирует метрики Контекста B]`

Все числа ниже (confidence, duration_ms, tokens_used) — это метрики, которые
**работающая программа** сохраняла в БД при каждом обращении к OpenAI API.
Cursor-агент только читает и интерпретирует их.

### 2.1 Диагностика по confidence

| Значение `avg_confidence` | Диагноз                                                        | Приоритет |
| ------------------------- | -------------------------------------------------------------- | --------- |
| `>= 0.85`                 | Норма                                                          | —         |
| `0.75 – 0.84`             | Субоптимально: system_prompt недостаточно направляет модель    | Medium    |
| `0.60 – 0.74`             | Проблема: запросы неоднозначны, модель «угадывает»             | High      |
| `< 0.60` или NULL         | Критично: модель дезориентирована или confidence не логируется | Critical  |

**Причины низкого confidence:**

- Нет system_prompt или он слишком общий
- Пользователь задаёт неструктурированные вопросы без контекста
- Модель применяется не по назначению (слабая модель для сложной задачи)
- `logprobs` не включены в запросе (→ confidence будет NULL)

### 2.2 Диагностика по duration_ms

| Значение `avg_duration_ms` | Диагноз                                                    | Приоритет |
| -------------------------- | ---------------------------------------------------------- | --------- |
| `< 3000`                   | Отлично                                                    | —         |
| `3000 – 8000`              | Приемлемо                                                  | —         |
| `8000 – 15000`             | Медленно: много tool-calling итераций или большой контекст | Medium    |
| `> 15000`                  | Критично: возможно зависание на MAX_TOOL_ITERATIONS        | High      |

**Причины высокого duration:**

- Большое число итераций tool calling (каждая итерация — отдельный LLM-запрос)
- Длинная история сообщений (контекст растёт с каждым ходом)
- Слишком большой `max_tokens` в конфиге агента
- Сеть/latency провайдера (вне нашего контроля)

### 2.3 Диагностика по tokens_used

| Значение `avg_tokens` | Диагноз                                 | Приоритет |
| --------------------- | --------------------------------------- | --------- |
| `< 500`               | Эффективно                              | —         |
| `500 – 1500`          | Норма                                   | —         |
| `1500 – 3000`         | Расточительно: стоит ограничить историю | Medium    |
| `> 3000`              | Критично: контекст не управляется       | High      |

**Расчёт примерной стоимости:**

```
cost = avg_tokens * responses * price_per_1k_tokens / 1000
# GPT-4o: ~$5/1M input tokens, ~$15/1M output tokens
# GPT-4o-mini: ~$0.15/1M input, ~$0.6/1M output
```

### 2.4 Диагностика tool calling _(инструменты продакшн-LLM, не Cursor)_

> Напоминание: здесь "tool calls" — это вызовы `list_entities`, `get_entity_schema`,
> `get_join_hint`, которые **продакшн-LLM** делал внутри `send_message()` в Контексте B.
> Эти данные сохранены в БД косвенно: если `assistant_msgs > user_msgs` в одном чате,
> значит программа делала несколько LLM-запросов на один вопрос пользователя (итерации tool calling).

Посчитай среднее число `assistant_msgs` на диалог из запроса 1F.

- Если `avg(assistant_msgs / user_msgs) > 1.5` → продакшн-LLM делал несколько итераций на один вопрос (много tool calls).
- Это говорит о том, что:
  1. system_prompt не объясняет когда использовать tools
  2. Описания tools в `tool_executor.rs` недостаточно точны (продакшн-LLM вызывает их избыточно)
  3. `MAX_TOOL_ITERATIONS = 5` в `service.rs` можно снизить до 2-3

### 2.5 Проверка system_prompt

Для каждого агента из запроса 1B оцени system_prompt по чеклисту:

- [ ] Есть ли явное описание роли / контекста домена?
- [ ] Есть ли инструкция о том, когда использовать tools (`list_entities`, `get_entity_schema`, `get_join_hint`)?
- [ ] Есть ли ограничение формата ответа (JSON / Markdown / plain text)?
- [ ] Есть ли языковая инструкция (отвечать на русском)?
- [ ] Есть ли примеры правильных ответов?

Отсутствие каждого пункта — повод для конкретной рекомендации.

---

## Фаза 3: Рекомендации `[Контекст A → изменения вступят в силу в Контексте B]`

### Шаблон 3A: Улучшение system_prompt

> **Контекст:** `system_prompt` — это текст, который **работающая программа** (Контекст B)
> передаёт в OpenAI API первым сообщением при каждом вызове `send_message()`.
> Он хранится в БД в таблице `a017_llm_agent`. Изменение вступит в силу немедленно —
> при следующем запросе пользователя к уже запущенной программе (перезапуск не нужен,
> т.к. агент читается из БД при каждом вызове).

**Когда применять:** `avg_confidence < 0.75` ИЛИ system_prompt отсутствует ИЛИ короче 100 символов.

**Целевой объект:** запись в таблице `a017_llm_agent` (поле `system_prompt`), редактировать через UI или из Контекста A через SQL:

```sql
UPDATE a017_llm_agent
SET system_prompt = '<новый промпт>',
    updated_at = datetime('now'),
    version = version + 1
WHERE description = '<имя агента>';
```

**Шаблон улучшенного system_prompt для агентов домена (анализ данных маркетплейсов):**

````
Ты — аналитический ассистент системы управления маркетплейсами.
Ты помогаешь анализировать данные из баз данных Wildberries, OZON, Яндекс.Маркет.

Доступные инструменты:
- list_entities([category]) — получить список таблиц. Используй в начале, если нужно понять структуру данных.
- get_entity_schema(entity_index) — получить схему таблицы (поля, типы, FK). Используй перед написанием SQL.
- get_join_hint(from_entity, to_entity) — получить SQL JOIN между двумя таблицами.

Правила работы:
1. Сначала вызови list_entities чтобы найти нужные таблицы, если они неизвестны.
2. Затем вызови get_entity_schema для получения точных имён полей.
3. Напиши и объясни SQL-запрос.
4. Отвечай на языке пользователя (русский, если не указано иное).
5. Если запрос неоднозначен — уточни перед выполнением.

Формат ответа:
- SQL-запросы оборачивай в блоки ```sql ... ```
- Давай краткое объяснение результата (2-3 предложения)
- Если нужны дополнительные данные — опиши что именно получить
````

### Шаблон 3B: Снижение MAX_TOOL_ITERATIONS

> **Контекст:** `MAX_TOOL_ITERATIONS` — константа в Rust-коде, которая ограничивает
> сколько раз **работающая программа** (Контекст B) может вызвать OpenAI API подряд
> в рамках одного запроса пользователя. Изменение вступает в силу после пересборки
> и перезапуска бэкенда.

**Когда применять:** большинство диалогов завершается за ≤ 2 tool-calling итерации.

**Целевой файл:** [`crates/backend/src/domain/a018_llm_chat/service.rs`](crates/backend/src/domain/a018_llm_chat/service.rs), строка 17.

**Изменение:**

```rust
// Было:
const MAX_TOOL_ITERATIONS: usize = 5;

// Стало (если avg итераций ≤ 2):
const MAX_TOOL_ITERATIONS: usize = 3;
```

**Мотивация:** снижение лимита предотвращает "зависание" диалогов при ошибках tool calling и уменьшает latency.

### Шаблон 3C: Ограничение истории сообщений (context trimming)

> **Контекст:** при каждом вызове `send_message()` **работающая программа** (Контекст B)
> загружает из БД всю историю чата и передаёт её в OpenAI API как контекст разговора.
> Чем длиннее чат — тем больше токенов тратится на каждый новый запрос.
> Изменение вступает в силу после пересборки и перезапуска бэкенда.

**Когда применять:** `avg_tokens > 1500` ИЛИ диалоги длиннее 10 ходов.

**Целевой файл:** [`crates/backend/src/domain/a018_llm_chat/service.rs`](crates/backend/src/domain/a018_llm_chat/service.rs), step 6 в функции `send_message` (~строка 256).

**Текущий код:**

```rust
// 6. Получить историю сообщений для контекста
let mut history = repository::find_messages_by_chat_id(&db, &chat_id_obj).await?;
```

**Предлагаемое изменение — добавить sliding window:**

```rust
// 6. Получить историю сообщений для контекста (последние N ходов)
const MAX_HISTORY_MESSAGES: usize = 20; // ~10 диалоговых ходов
let mut history = repository::find_messages_by_chat_id(&db, &chat_id_obj).await?;

// Trim: оставляем system-сообщения + последние MAX_HISTORY_MESSAGES
if history.len() > MAX_HISTORY_MESSAGES {
    let system_msgs: Vec<_> = history.iter()
        .filter(|m| m.role == ChatRole::System)
        .cloned()
        .collect();
    let recent: Vec<_> = history.iter()
        .filter(|m| m.role != ChatRole::System)
        .rev()
        .take(MAX_HISTORY_MESSAGES)
        .rev()
        .cloned()
        .collect();
    history = system_msgs.into_iter().chain(recent).collect();
}
```

### Шаблон 3D: Выбор модели по сложности запроса

**Когда применять:** `avg_duration_ms > 8000` при коротких (< 100 символов) пользовательских вопросах.

**Целевой файл:** [`crates/backend/src/domain/a018_llm_chat/service.rs`](crates/backend/src/domain/a018_llm_chat/service.rs), строки 199–204 (model selection).

**Текущая логика:**

```rust
// Выбор модели: из запроса -> из чата -> из агента
let model_to_use = request.model_name
    .as_ref()
    .map(|s| s.clone())
    .unwrap_or_else(|| chat.model_name.clone());
```

**Предлагаемое улучшение — эвристика по длине запроса:**

```rust
// Выбор модели: из запроса -> из чата -> fallback по сложности
let model_to_use = request.model_name
    .as_ref()
    .map(|s| s.clone())
    .unwrap_or_else(|| {
        // Простые вопросы (< 200 символов) — лёгкая модель
        if request.content.len() < 200 && chat.model_name.contains("gpt-4o") {
            "gpt-4o-mini".to_string()
        } else {
            chat.model_name.clone()
        }
    });
```

> Адаптируй пороги под реальные данные из запроса 1G.

### Шаблон 3E: Улучшение описаний tool definitions

**Когда применять:** модель часто вызывает `list_entities` без `category` (избыточный вызов) ИЛИ делает 3+ итераций.

**Целевой файл:** [`crates/backend/src/shared/llm/tool_executor.rs`](crates/backend/src/shared/llm/tool_executor.rs), функция `metadata_tool_definitions`.

**Конкретные улучшения:**

1. Добавить примеры в описание `list_entities`:

```rust
description: "Получить список таблиц базы данных с кратким описанием. \
              ВСЕГДА используй category для сужения поиска. \
              Доступные категории: wb (Wildberries), ozon, ym (Яндекс.Маркет), \
              ref (справочники: организации, номенклатура), llm (чаты, агенты). \
              Пример: category='wb' для данных Wildberries.",
```

2. Добавить примеры entity_index в `get_entity_schema`:

```rust
description: "Получить детальную схему таблицы: поля, SQL-типы, описания, \
              внешние ключи (FK). Используй ПЕРЕД написанием SQL-запроса. \
              Примеры entity_index: 'a012' (продажи WB), 'a004' (номенклатура), \
              'a006' (подключения маркетплейсов), 'a005' (маркетплейсы).",
```

### Шаблон 3F: Включение логирования confidence (если NULL)

**Когда применять:** `confidence IS NULL` для большинства ответов.

Значение `confidence` заполняется из `logprobs` OpenAI. Если оно NULL — провайдер не возвращает logprobs.

**Проверь в `openai_provider.rs`:** включён ли параметр `logprobs: true` в теле запроса к API.

Если не включён — добавь в JSON-тело запроса:

```json
{
  "logprobs": true,
  "top_logprobs": 1
}
```

---

## Фаза 4: Формат выходного отчёта

После анализа сформируй отчёт в следующем формате:

````markdown
## Анализ диалогов LLM Chat — <YYYY-MM-DD>

### Исходные данные

- Проанализировано чатов: N
- Проанализировано сообщений: M (из них assistant: K)
- Период: <дата первого> — <дата последнего>
- Агенты: <список>

### Сводная статистика

| Агент | Модель | Avg confidence | Avg duration, ms | Avg tokens |
| ----- | ------ | -------------- | ---------------- | ---------- |
| ...   | ...    | ...            | ...              | ...        |

### Выявленные проблемы

| Приоритет | Проблема | Метрика          | Файл / Объект  |
| --------- | -------- | ---------------- | -------------- |
| Critical  | ...      | confidence=0.45  | a017_llm_agent |
| High      | ...      | duration=18000ms | service.rs:17  |
| Medium    | ...      | avg_tokens=2200  | service.rs:256 |

### Рекомендованные изменения

#### 1. [Приоритет] <Заголовок изменения>

**Что:** ...
**Где:** `<file>:<line>`
**Изменение:**
\```diff

- старый код

* новый код
  \```
  **Ожидаемый эффект:** ...

#### 2. ...

### System Prompt — предложения

Для агента "<имя>":
\```
<текст улучшенного system_prompt>
\```
````

---

## Контрольный список завершения

- [ ] Шаг 0: Путь к БД найден, файл существует
- [ ] Шаг 0.2: Данных достаточно (≥ 3 чатов, ≥ 10 сообщений)
- [ ] Шаг 1: Все 7 запросов выполнены, данные собраны
- [ ] Шаг 2: Каждая метрика сверена с пороговыми значениями
- [ ] Шаг 3: Для каждой выявленной проблемы подготовлена конкретная рекомендация
- [ ] Шаг 4: Итоговый отчёт представлен пользователю
- [ ] Шаг 4: Пользователь подтвердил какие изменения применить
- [ ] Изменения внесены в код (только после подтверждения пользователя)

---

## Справочник: ключевые файлы

| Файл                                                     | Назначение                                                          |
| -------------------------------------------------------- | ------------------------------------------------------------------- |
| `config.toml`                                            | Путь к SQLite БД                                                    |
| `crates/backend/src/domain/a018_llm_chat/service.rs`     | MAX_TOOL_ITERATIONS, send_message, model selection, context history |
| `crates/backend/src/shared/llm/tool_executor.rs`         | metadata_tool_definitions — описания tools                          |
| `crates/backend/src/shared/llm/openai_provider.rs`       | Параметры запроса к OpenAI API (logprobs)                           |
| `crates/contracts/src/domain/a018_llm_chat/aggregate.rs` | Структуры LlmChatMessage, ChatRole                                  |
| БД: `a017_llm_agent`                                     | system_prompt, temperature, max_tokens агентов                      |
| БД: `a018_llm_chat`                                      | Чаты, привязка к агентам                                            |
| БД: `a018_llm_chat_message`                              | Все сообщения с метриками inference                                 |
