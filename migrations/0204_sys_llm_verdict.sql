-- Вердикты о качестве работы LLM-агентов.
--
-- Зачем отдельная таблица: `sys_tool_trace` знает, ЧТО вызывалось и упало ли это
-- технически, но не знает, ПОМОГ ли ответ. Без структурированной оценки любое
-- изменение промпта/навыка неизмеримо: единственный существовавший потребитель
-- истории чатов (task014_kb_analyze) производит тикеты на правку базы знаний,
-- а не метрику качества ответов.
--
-- Строка = одна оценка одного диалога (или одного кейса голден-сета).
-- Пишется автоматически: вердикт — наблюдение, а не мутация боевых данных,
-- поэтому человеческого подтверждения не требует.
--
-- source различает два происхождения оценки:
--   audit  — судья прошёл по реальным чатам за окно (task027_llm_judge);
--   golden — прогон эталонного набора вопросов (task028_llm_golden_set),
--            где есть ожидаемый ответ, с которым сверяются.
-- Обе живут в одной таблице намеренно: динамика качества смотрится вместе,
-- а разделение — это WHERE по одной колонке.

CREATE TABLE IF NOT EXISTS sys_llm_verdict (
    id            TEXT PRIMARY KEY,
    -- audit | golden
    source        TEXT NOT NULL DEFAULT 'audit',
    chat_id       TEXT NOT NULL,
    -- Оцениваемый ответ ассистента, если судья указал конкретный.
    message_id    TEXT,
    -- Кейс голден-сета (для source='golden'); у аудита пусто.
    case_id       TEXT,
    -- Снимок контекста оценки: специализация сотрудника, активный навык, интент,
    -- модель. Снимок, а не join: навык/модель у чата со временем меняются, а
    -- вердикт должен остаться сопоставимым с тем, что оценивали.
    agent_type    TEXT,
    skill_id      TEXT,
    intent        TEXT,
    model         TEXT,
    -- solved | partial | failed
    verdict       TEXT NOT NULL,
    -- Классификация провала: sql_error | tool_loop | wrong_data | missing_context
    --                       | no_answer | refused | other. Пусто у solved.
    failure_kind  TEXT,
    -- Обоснование вердикта человеческим языком: без него метрика неотличима от шума.
    reason        TEXT NOT NULL DEFAULT '',
    -- Сколько инструментов отработало и сколько из них упало — снимок на момент
    -- оценки, чтобы дашборд не пересчитывал трассу для каждой строки.
    tool_calls    INTEGER NOT NULL DEFAULT 0,
    tool_failures INTEGER NOT NULL DEFAULT 0,
    -- Сессия задачи-судьи: по ней видно, какой прогон породил оценку.
    judge_session_id TEXT,
    judge_model   TEXT,
    created_at    TEXT NOT NULL
);

-- Повторно оценивать уже оценённый диалог в рамках одного источника незачем:
-- судья выбирает чаты «без вердикта», и уникальность защищает от гонки двух прогонов.
CREATE UNIQUE INDEX IF NOT EXISTS idx_sys_llm_verdict_unique
    ON sys_llm_verdict (source, chat_id, COALESCE(case_id, ''));

CREATE INDEX IF NOT EXISTS idx_sys_llm_verdict_created
    ON sys_llm_verdict (created_at);
CREATE INDEX IF NOT EXISTS idx_sys_llm_verdict_verdict
    ON sys_llm_verdict (verdict);
CREATE INDEX IF NOT EXISTS idx_sys_llm_verdict_chat
    ON sys_llm_verdict (chat_id);
