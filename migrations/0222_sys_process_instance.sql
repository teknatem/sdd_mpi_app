-- Экземпляры процессов: долговечное состояние одного прогона по графу.
--
-- Ради этой таблицы всё и строилось. Прогон регламентного задания живёт ровно
-- один запуск; экземпляр переживает перезапуск сервера, потому что ожидание
-- человека измеряется сутками (ADR-0011 п.2, п.9).
--
-- Конструкция аренды и попыток взята у поручений `a042`, а не изобретена
-- заново: там она уже держит воркера, переживающего падение (`claim_session_id`,
-- `attempts`, `next_attempt_at`).

CREATE TABLE IF NOT EXISTS sys_process_instance (
    id                   TEXT PRIMARY KEY,
    -- Процесс и его версия. Версия не меняется до конца жизни экземпляра:
    -- живые доживают на своей (ADR-0011 п.7). Версии Этапов приходят вместе с
    -- ней — они запинены в момент активации Процесса.
    process_code         TEXT NOT NULL,
    process_version      INTEGER NOT NULL,
    -- Ключ корреляции: про что этот прогон. JSON — для чтения человеком,
    -- токен — для сведения с событиями.
    correlation_json     TEXT NOT NULL DEFAULT '{}',
    correlation_token    TEXT NOT NULL,
    -- 'running' | 'waiting' | 'done' | 'quarantined'
    status               TEXT NOT NULL DEFAULT 'running',
    -- Курсор по графу: Этап, который исполняется следующим.
    stage_code           TEXT,
    -- Номер захода в текущий Этап. Входит в ключ идемпотентности: в графе есть
    -- циклы, и без него второй заход вернул бы «уже делали» и не сделал бы
    -- ничего.
    visit                INTEGER NOT NULL DEFAULT 0,
    -- Вход текущего Этапа: ключ корреляции плюс данные выхода предыдущего.
    input_json           TEXT NOT NULL DEFAULT '{}',
    -- Счётчик временных сбоев текущего Этапа и время следующей попытки.
    -- Отдельного статуса «ждёт повтора» нет намеренно: он лишний, пока есть
    -- время следующей попытки.
    attempts             INTEGER NOT NULL DEFAULT 0,
    next_attempt_at      TEXT,
    -- Ожидание: какое событие, с каким токеном, начиная с какого номера
    -- публикации, и до какого дедлайна.
    wait_event           TEXT,
    wait_token           TEXT,
    wait_since_seq       INTEGER,
    wait_deadline_at     TEXT,
    wait_on_timeout_json TEXT,
    last_outcome         TEXT,
    last_error           TEXT,
    -- Аренда: пока она стоит, второй воркер экземпляр не возьмёт. Отсюда же
    -- следует, что незавершённая запись в журнале эффектов означает «кто-то
    -- умер», а не «кто-то работает прямо сейчас».
    claim_session_id     TEXT,
    claimed_at           TEXT,
    started_at           TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at           TEXT NOT NULL DEFAULT (datetime('now')),
    finished_at          TEXT
);

-- Живой экземпляр на пару «Процесс + ключ корреляции» ровно один: повторная
-- публикация факта про тот же день не должна заводить второй прогон. Гарантия
-- нужна на уровне БД — событие может прийти дважды одновременно.
CREATE UNIQUE INDEX IF NOT EXISTS idx_sys_process_instance_live
    ON sys_process_instance(process_code, correlation_token)
    WHERE status IN ('running', 'waiting');

-- Выборка воркера: что готово исполняться.
CREATE INDEX IF NOT EXISTS idx_sys_process_instance_runnable
    ON sys_process_instance(status, next_attempt_at);

-- Пробуждение по событию и разбор просроченных ожиданий.
CREATE INDEX IF NOT EXISTS idx_sys_process_instance_wait
    ON sys_process_instance(status, wait_event, wait_token);
