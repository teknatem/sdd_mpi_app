-- Seed: ежедневная загрузка отчёта по платежам Yandex Market (task019) по обоим кабинетам.
--
-- ЗАЧЕМ. Миграция 0139 пыталась завести строку task019 плейсхолдером, но взяла id
-- 'a1b2c3d4-…-ef1234567819', уже занятый миграцией 0102 под task016-kb-intake. Вставка шла
-- через INSERT OR IGNORE — конфликт по первичному ключу проглотился молча, и задания
-- task019 в базе не появилось вовсе (та же беда была у соседнего task018 с id …7818, но там
-- id в sys_tasks оказался свободен и строка создалась). В результате
-- p907_ym_payment_report наполнялся только ручными запусками u503, и слой учёта `fina`
-- по YM отстаёт от факта. Здесь задания заводятся заново — с уникальными id, реальными
-- connection_id и включёнными.
--
-- ВРЕМЯ: планировщик считает cron в UTC (worker.rs использует Utc::now()), МСК = UTC+3.
--   '0 0 2 * * *' → 05:00 МСК — YM СТС
--   '0 0 4 * * *' → 07:00 МСК — YM Vannika
-- Разнесены на 2 часа намеренно: оба задания пишут одну таблицу p907_ym_payment_report, а
-- ResourceCoordinator даёт её только одному заданию за раз (второе будет откладываться до
-- следующего тика). Два часа — это max_duration_seconds задания (7200 с): отчёт асинхронный,
-- генерация → опрос статуса → ZIP → CSV. Кабинеты сидят в разных бизнесах YM (918462 и
-- 186490929), так что за rate-limit Маркета они не конкурируют — конкурируют только за таблицу.
--
-- chunk_days = 45 = resync_days. В установившемся режиме задание и так грузит окно в 45 дней
-- (у united-netting нет фильтра по дате изменения: bank_order_id и act_id Маркет дописывает
-- недели спустя, поймать правки можно только перезабором хвоста). Значит и порция догона может
-- быть такой же — больше, чем штатный ежедневный запрос, задание всё равно не сделает.
--
-- ВОДЯНОЙ ЗНАК. data_loaded_up_to берётся из уже загруженных данных: date(MAX(transaction_date))
-- по этому кабинету. На боевой базе это 2026-06-30 → догон стартует с реального разрыва, а не
-- с work_start_date, и укладывается в один прогон. На чистом экземпляре p907 пуст → MAX = NULL →
-- водяного знака нет → полный бэкфилл с work_start_date порциями chunk_days, как задумано.
--
-- ПЕРВЫЙ ЗАПУСК: next_run_at = NULL воркер считает как «запустить немедленно» (worker.rs:88) —
-- для СТС это и нужно, разрыв в полтора месяца надо закрыть. Vannika сдвинута на +90 минут,
-- иначе оба задания стартуют на первом же тике и подерутся за p907.

-- 1. Плейсхолдер из 0139 — там, где 0139 всё-таки отработала (id был свободен): гасим, чтобы
--    не путался в списке рядом с настоящими заданиями.
--    Ищем по task_type + REPLACE_WITH, а НЕ по id: id из 0139 в этой базе принадлежит
--    task016-kb-intake, и адресация по нему испортила бы чужое задание.
--    Guard по REPLACE_WITH: строку, настроенную вручную, не трогаем.
UPDATE sys_tasks
SET is_enabled = 0,
    is_deleted = 1,
    updated_at = datetime('now')
WHERE task_type = 'task019_ym_payment_report'
  AND config_json LIKE '%REPLACE_WITH%';

-- 2. YM СТС — 05:00 МСК, стартует немедленно.
INSERT OR IGNORE INTO sys_tasks (
    id, code, description, task_type, schedule_cron, config_json,
    is_enabled, next_run_at, data_loaded_up_to, created_at, updated_at, is_deleted
) VALUES (
    'c0190019-0000-4019-b019-000000000019',
    'task019-ym-payment-report_STS',
    'YM Отчёт по платежам — united-netting (YM СТС, 05:00 МСК). Хвост 45 дней ради правок задним числом.',
    'task019_ym_payment_report',
    '0 0 2 * * *',
    '{"connection_id":"1ce94c09-e2b1-46c7-aad5-a597e8911cef","work_start_date":"2026-01-01","overlap_days":3,"chunk_days":45,"resync_days":45}',
    1,
    NULL,
    (SELECT date(MAX(transaction_date))
       FROM p907_ym_payment_report
      WHERE connection_mp_ref = '1ce94c09-e2b1-46c7-aad5-a597e8911cef'),
    datetime('now'),
    datetime('now'),
    0
);

-- 3. YM Vannika — 07:00 МСК, первый запуск через 90 минут.
INSERT OR IGNORE INTO sys_tasks (
    id, code, description, task_type, schedule_cron, config_json,
    is_enabled, next_run_at, data_loaded_up_to, created_at, updated_at, is_deleted
) VALUES (
    'c0190019-0000-4019-b019-000000000020',
    'task019-ym-payment-report_Vannika',
    'YM Отчёт по платежам — united-netting (YM Vannika, 07:00 МСК). Хвост 45 дней ради правок задним числом.',
    'task019_ym_payment_report',
    '0 0 4 * * *',
    '{"connection_id":"47e2ce51-e188-449f-978a-a1c012e22b83","work_start_date":"2026-01-01","overlap_days":3,"chunk_days":45,"resync_days":45}',
    1,
    strftime('%Y-%m-%dT%H:%M:%S+00:00', 'now', '+90 minutes'),
    (SELECT date(MAX(transaction_date))
       FROM p907_ym_payment_report
      WHERE connection_mp_ref = '47e2ce51-e188-449f-978a-a1c012e22b83'),
    datetime('now'),
    datetime('now'),
    0
);
