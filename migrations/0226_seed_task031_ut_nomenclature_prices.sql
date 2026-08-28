-- Ежедневная загрузка номенклатуры 1С и дилерских цен УТ для BI-выгрузки.
-- Cron в UTC: 03:00 = 06:00 МСК. Задание включено в sys_tasks, но воркер
-- не стартует, пока в config.toml [scheduled_tasks].enabled = false.
-- connection_id берётся из основного подключения 1С; если его нет —
-- заглушка REPLACE_WITH_1C_CONNECTION_ID (карточка задачи, иначе запуск падает).

INSERT OR IGNORE INTO sys_tasks (
    id, code, description, task_type, schedule_cron, config_json,
    is_enabled, next_run_at, created_at, updated_at, is_deleted
)
SELECT
    'c0310031-0000-4031-b031-000000000031',
    'task031-ut-nomenclature-prices',
    '1С: номенклатура и дилерские цены (ежедневно, 06:00 МСК).',
    'task031_ut_nomenclature_prices',
    '0 0 3 * * *',
    json_object(
        'connection_id',
        COALESCE(
            (SELECT id FROM a001_connection_1c_database
             WHERE is_primary = 1 AND is_deleted = 0
             LIMIT 1),
            'REPLACE_WITH_1C_CONNECTION_ID'
        )
    ),
    1,
    NULL,
    datetime('now'),
    datetime('now'),
    0
WHERE NOT EXISTS (
    SELECT 1 FROM sys_tasks
    WHERE task_type = 'task031_ut_nomenclature_prices' AND is_deleted = 0
);
