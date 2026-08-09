-- a042_agent_task — очередь поручений между AI-сотрудниками.
--
-- Одна строка = одно поручение: агент A ставит задачу специализации B, регламент
-- task029 захватывает строку, гоняет её через служебный чат a018 и записывает ответ.
--
-- Отличия от образцов a031/a039, продиктованные тем, что это именно ОЧЕРЕДЬ:
--   * attempts/max_attempts/next_attempt_at — учёт попыток и бэкофф. Без них
--     провалившаяся запись перезабирается каждый тик вечно (болезнь task022).
--   * claim_session_id/started_at — захват и его владелец. Захват атомарный
--     (UPDATE ... WHERE status='pending' + rows_affected), started_at — база
--     отсчёта для развёртки брошенных прогонов.
--   * error — диагностика провала. У a031 такой колонки нет, и запись, брошенная
--     упавшим воркером, висит в processing без единого следа причины.
--   * depth/parent_task_ref — ограничитель рекурсии: канал между агентами
--     приглашает петлю A→B→A, а каждый её виток стоит реальных денег.

CREATE TABLE IF NOT EXISTS a042_agent_task (
    id TEXT PRIMARY KEY,
    code TEXT NOT NULL UNIQUE,
    description TEXT NOT NULL,
    comment TEXT,

    status TEXT NOT NULL DEFAULT 'pending',
    target_agent_type TEXT NOT NULL,
    request_text TEXT NOT NULL,
    payload_json TEXT,

    requested_by_agent_ref TEXT,
    requested_by_chat_ref TEXT,
    requested_by_user_ref TEXT,

    parent_task_ref TEXT,
    depth INTEGER NOT NULL DEFAULT 0,

    attempts INTEGER NOT NULL DEFAULT 0,
    max_attempts INTEGER NOT NULL DEFAULT 2,
    next_attempt_at TEXT,

    claim_session_id TEXT,
    started_at TEXT,
    finished_at TEXT,

    executor_agent_ref TEXT,
    result_chat_ref TEXT,
    result_message_ref TEXT,
    result_artifact_ref TEXT,
    result_text TEXT,
    error TEXT,

    is_deleted INTEGER NOT NULL DEFAULT 0,
    is_posted INTEGER NOT NULL DEFAULT 0,
    created_at TEXT,
    updated_at TEXT,
    version INTEGER NOT NULL DEFAULT 1
);

-- Выборка воркера: status + гейт бэкоффа. Частичный индекс — все запросы
-- репозитория всегда фильтруют is_deleted = 0.
CREATE INDEX IF NOT EXISTS idx_a042_queue
    ON a042_agent_task (status, next_attempt_at) WHERE is_deleted = 0;

-- Заказчик забирает свои поручения по чату; тем же индексом считается потолок
-- незакрытых поручений на чат.
CREATE INDEX IF NOT EXISTS idx_a042_requester
    ON a042_agent_task (requested_by_chat_ref, status) WHERE is_deleted = 0;

-- Обратный поиск при расчёте глубины: «этот чат — исполнение чьего поручения?»
CREATE INDEX IF NOT EXISTS idx_a042_result_chat
    ON a042_agent_task (result_chat_ref) WHERE is_deleted = 0;

CREATE INDEX IF NOT EXISTS idx_a042_parent
    ON a042_agent_task (parent_task_ref);
