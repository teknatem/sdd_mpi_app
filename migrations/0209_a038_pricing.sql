-- Прайс подключения к модели: ставки за миллион токенов.
--
-- Живёт на подключении, а не в конфиге: у флота несколько провайдеров с разными
-- ставками одновременно, и «цена токена» — свойство конкретного подключения.
--
-- Ставки хранятся как REAL: это справочные величины, по которым считают, а не
-- накопленные суммы (сама стоимость лежит в a018_llm_chat_message.cost_micro
-- целыми микроединицами).
--
-- NULL во всех трёх ставках = прайс не задан: стоимость по этому подключению
-- честно не считается. Явный 0 — это реальная цена (локальная модель).
-- NULL в price_cached_per_mtok = скидки за кэш нет, считаем по входной ставке.

ALTER TABLE a038_llm_connection ADD COLUMN price_in_per_mtok REAL;
ALTER TABLE a038_llm_connection ADD COLUMN price_out_per_mtok REAL;
ALTER TABLE a038_llm_connection ADD COLUMN price_cached_per_mtok REAL;
ALTER TABLE a038_llm_connection ADD COLUMN currency TEXT;
