-- Стоимость прогона на сообщении ассистента.
--
-- До этого хранился только `tokens_used` (сумма), и вопрос «во сколько обошёлся
-- ответ» ответа не имел: вход и выход тарифицируются по разным ставкам. Вердикты
-- качества (sys_llm_verdict) дают числитель, эти колонки — знаменатель.
--
-- `cost_micro` в ЦЕЛЫХ микроединицах валюты (1e-6), а не в REAL: стоимость
-- суммируется по тысячам сообщений на дашборде, и накопленная ошибка float
-- там видна. Валюта хранится снимком рядом со стоимостью — прайс подключения
-- со временем меняется, а исторические суммы должны остаться сопоставимыми.
--
-- `cached_prompt_tokens` — ПОДМНОЖЕСТВО `prompt_tokens` (семантика OpenAI
-- prompt_tokens_details.cached_tokens), а не добавка к нему.

ALTER TABLE a018_llm_chat_message ADD COLUMN prompt_tokens INTEGER;
ALTER TABLE a018_llm_chat_message ADD COLUMN completion_tokens INTEGER;
ALTER TABLE a018_llm_chat_message ADD COLUMN cached_prompt_tokens INTEGER;
ALTER TABLE a018_llm_chat_message ADD COLUMN cost_micro INTEGER;
ALTER TABLE a018_llm_chat_message ADD COLUMN currency TEXT;

-- Дашборд d407 берёт стоимость окнами по дате: без индекса это скан всей
-- переписки, которая растёт быстрее всех остальных таблиц.
CREATE INDEX IF NOT EXISTS idx_a018_message_created_cost
    ON a018_llm_chat_message(created_at)
    WHERE cost_micro IS NOT NULL;
