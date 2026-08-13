-- Два ежедневных задания загрузки отчёта «Аналитика продаж» Yandex Market (a041).
-- Cron хранится в UTC: 03:30 и 05:00 МСК. Запуски разнесены, поскольку оба задания
-- блокируют общие таблицы a041 и p916 на время атомарной замены периода.

INSERT OR IGNORE INTO sys_tasks (
    id, code, description, task_type, schedule_cron, config_json,
    is_enabled, next_run_at, data_loaded_up_to, created_at, updated_at, is_deleted
) VALUES (
    'c0260026-0000-4026-b026-000000000026',
    'task026-ym-shows-sales_STS',
    'YM Воронка продаж — отчёт «Аналитика продаж» (YM СТС, 03:30 МСК).',
    'task026_ym_shows_sales_daily',
    '0 30 0 * * *',
    '{"connection_id":"1ce94c09-e2b1-46c7-aad5-a597e8911cef","work_start_date":"2026-01-01","overlap_days":3,"chunk_days":30}',
    1,
    NULL,
    (SELECT MAX(document_date) FROM a041_ym_shows_sales_daily
      WHERE connection_id = '1ce94c09-e2b1-46c7-aad5-a597e8911cef' AND is_deleted = 0),
    datetime('now'), datetime('now'), 0
);

INSERT OR IGNORE INTO sys_tasks (
    id, code, description, task_type, schedule_cron, config_json,
    is_enabled, next_run_at, data_loaded_up_to, created_at, updated_at, is_deleted
) VALUES (
    'c0260026-0000-4026-b026-000000000027',
    'task026-ym-shows-sales_Vannika',
    'YM Воронка продаж — отчёт «Аналитика продаж» (YM Vannika, 05:00 МСК).',
    'task026_ym_shows_sales_daily',
    '0 0 2 * * *',
    '{"connection_id":"47e2ce51-e188-449f-978a-a1c012e22b83","work_start_date":"2026-01-01","overlap_days":3,"chunk_days":30}',
    1,
    strftime('%Y-%m-%dT%H:%M:%S+00:00', 'now', '+75 minutes'),
    (SELECT MAX(document_date) FROM a041_ym_shows_sales_daily
      WHERE connection_id = '47e2ce51-e188-449f-978a-a1c012e22b83' AND is_deleted = 0),
    datetime('now'), datetime('now'), 0
);
