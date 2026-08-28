-- Искра для Процесса pr0002 «Проверка номенклатуры».
-- Задание выключено: для разработки хватает ручного «Запустить сейчас»
-- (run_task_now не смотрит ни на is_enabled, ни на [scheduled_tasks].enabled).
-- Cron-заглушка на случай будущего включения; сейчас не стреляет.

INSERT OR IGNORE INTO sys_tasks (
    id, code, description, task_type, schedule_cron, config_json,
    is_enabled, next_run_at, created_at, updated_at, is_deleted
)
SELECT
    'c0320032-0000-4032-b032-000000000032',
    'task032-nomenclature-check',
    'Проверка номенклатуры (pr0002): публикует process.due. Импорт и сопоставление — у Процесса.',
    'task032_nomenclature_check',
    '0 0 4 * * *',
    json_object('process_code', 'pr0002'),
    0,
    NULL,
    datetime('now'),
    datetime('now'),
    0
WHERE NOT EXISTS (
    SELECT 1 FROM sys_tasks
    WHERE task_type = 'task032_nomenclature_check' AND is_deleted = 0
);
