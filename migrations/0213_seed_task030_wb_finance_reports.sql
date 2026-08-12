-- Новый WB Finance API v1. Задания намеренно выключены до проверки токенов категории Финансы.
INSERT OR IGNORE INTO sys_tasks (
    id, code, description, task_type, schedule_cron, config_json,
    is_enabled, created_at, updated_at, is_deleted
) VALUES (
    'c0300030-0000-4030-b030-000000000030',
    'task030-wb-finance-reports_SANSTAR',
    'WB Finance API v1 — ежедневные отчёты a043, окно 35 дней (SANSTAR)',
    'task030_wb_finance_reports',
    '0 0 2 * * *',
    '{"connection_id":"1386a311-1e26-4676-b696-8d577a119eec","lookback_days":35}',
    0, datetime('now'), datetime('now'), 0
);

INSERT OR IGNORE INTO sys_tasks (
    id, code, description, task_type, schedule_cron, config_json,
    is_enabled, created_at, updated_at, is_deleted
) VALUES (
    'c0300030-0000-4030-b030-000000000031',
    'task030-wb-finance-reports_CTC',
    'WB Finance API v1 — ежедневные отчёты a043, окно 35 дней (CTC)',
    'task030_wb_finance_reports',
    '0 0 3 * * *',
    '{"connection_id":"42e29532-72b1-4f38-be6e-38c331c61fe6","lookback_days":35}',
    0, datetime('now'), datetime('now'), 0
);
