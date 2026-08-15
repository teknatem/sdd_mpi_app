-- Метрики проекта: снимок состояния экземпляра и кодовой базы на каждый старт.
--
-- Зачем история, а не расчёт на лету: вопрос страницы — не «сколько сейчас строк»,
-- а «куда это движется». Одно значение без предыдущего бесполезно, поэтому снимок
-- фиксируется при каждом запуске и живёт дольше процесса.
--
-- Почему две таблицы, а не одна с JSON: ряд по метрике («покажи размер БД за
-- последние 60 стартов») должен быть одним индексным запросом, иначе график
-- заставит разобрать все снимки. Но топ-10 файлов и построчный список проверок
-- в такой ряд не ложатся — они лежат в `details_json` того же снимка.
--
-- Чего здесь нет: описаний метрик. Подпись, единица, направление «больше — лучше»
-- и пороги живут в коде (`system/metrics/catalog.rs`), как SCOPE_CATALOG и план
-- счетов: это часть версии приложения, а не данные экземпляра.

CREATE TABLE IF NOT EXISTS sys_metric_snapshot (
    id                TEXT PRIMARY KEY,
    captured_at       TEXT NOT NULL,
    -- 'startup' | 'manual'
    trigger           TEXT NOT NULL,
    -- Паспорт сборки: снимок описывает конкретный бинарь, и без этих полей
    -- скачок метрики нельзя отличить от «просто выкатили другую версию».
    app_version       TEXT NOT NULL,
    git_commit        TEXT,
    build_profile     TEXT NOT NULL,
    schema_version    INTEGER NOT NULL DEFAULT 0,
    -- Когда собран codebase_metrics.json, вшитый в этот бинарь.
    code_generated_at TEXT,
    -- Сколько занял сбор: сигнал, что какой-то источник перестал быть дешёвым.
    collect_ms        INTEGER NOT NULL DEFAULT 0,
    details_json      TEXT NOT NULL DEFAULT '{}'
);

CREATE INDEX IF NOT EXISTS idx_sys_metric_snapshot_at
    ON sys_metric_snapshot(captured_at DESC);

CREATE TABLE IF NOT EXISTS sys_metric_value (
    snapshot_id TEXT NOT NULL,
    -- 'code.lines.total', 'db.file_mb', 'ui.dead_classes' — новый ключ не требует
    -- миграции, но требует записи в METRIC_CATALOG, иначе не будет показан.
    metric_key  TEXT NOT NULL,
    value       REAL NOT NULL,
    PRIMARY KEY (snapshot_id, metric_key)
);

-- Порядок колонок именно такой: ряд читается по ключу, снимки — по времени.
CREATE INDEX IF NOT EXISTS idx_sys_metric_value_key
    ON sys_metric_value(metric_key, snapshot_id);
