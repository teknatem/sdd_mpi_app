-- Навык оценки качества + регламенты судьи и голден-сета.
--
-- Доступ отдан координатору: оценка работы флота сотрудников — надзорная функция,
-- и раздавать её тем, кого оценивают, незачем. Системный администратор получает
-- extended: ему это нужно при разборе инцидентов, но не в основном наборе.
--
-- Явные строки, а не расчёт на дефолт: незаполненная ячейка матрицы означает
-- Denied для всех, кроме координатора, и новый навык молча стал бы невидимым.

INSERT OR IGNORE INTO sys_llm_skill_access (specialization, skill_id, access_level) VALUES
    ('coordinator_admin', 'llm-quality-review', 'immediate'),
    ('system_admin',      'llm-quality-review', 'extended');

-- Судья: раз в сутки разбирает вчерашние диалоги. Время выбрано после ночного
-- task014 (03:00 UTC), чтобы два фоновых LLM-задания не бодались за одни и те же
-- таблицы на блокировке ресурсов.
INSERT OR IGNORE INTO sys_tasks (
    id, code, description, task_type, schedule_cron, config_json,
    is_enabled, created_at, updated_at, is_deleted
) VALUES (
    'a1b2c3d4-e5f6-7890-abcd-ef1234567827',
    'task027-llm-judge',
    'LLM — оценка качества ответов: ставит вердикты solved/partial/failed по диалогам.',
    'task027_llm_judge',
    '0 30 4 * * *',
    '{"lookback_days":2,"max_chats":15}',
    1,
    datetime('now'),
    datetime('now'),
    0
);

-- Голден-сет выключен по умолчанию: он тратит реальные вызовы модели и должен
-- запускаться осознанно — вручную после правки промптов/навыков либо по
-- расписанию, когда набор кейсов заполнен.
INSERT OR IGNORE INTO sys_tasks (
    id, code, description, task_type, schedule_cron, config_json,
    is_enabled, created_at, updated_at, is_deleted
) VALUES (
    'a1b2c3d4-e5f6-7890-abcd-ef1234567828',
    'task028-llm-golden-set',
    'LLM CI — прогон эталонных вопросов и сверка ответов с ожиданиями.',
    'task028_llm_golden_set',
    '0 0 5 * * 1',
    '{"max_cases":10}',
    0,
    datetime('now'),
    datetime('now'),
    0
);
