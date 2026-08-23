-- Журнал эффектов: единственная запись о том, что система сделала с миром.
--
-- Зачем отдельная таблица, а не строка в логе прогона: у эффекта своя
-- идентичность, переживающая и прогон, и перезапуск. Экземпляр процесса может
-- упасть между «провёл документ» и «записал, что провёл»; без журнала с ключом
-- идемпотентности повтор проведёт документ второй раз, и разницу уже никто не
-- увидит. Поэтому запись делается ДО исполнения, а не после.
--
-- Почему сухой прогон живёт в той же таблице: план эффекта и сам эффект должны
-- сравниваться глазами («что собирались сделать» против «что сделали»), а это
-- один запрос только пока они рядом. Отличает их `mode`.
--
-- Чего здесь нет: отката. Обратная операция — это отдельный эффект со своей
-- строкой и своим ключом, а не правка этой. Журнал только дописывается.

CREATE TABLE IF NOT EXISTS sys_effect_log (
    id                   TEXT PRIMARY KEY,
    -- Ключ идемпотентности: смысловой, а не случайный. Строится вызывающим из
    -- того, что делает эффект уникальным ("pr0001:2026-08-21:san:post"), и
    -- именно он, а не id, отвечает на вопрос «это уже делали?».
    idempotency_key      TEXT NOT NULL,
    -- Имя Действия: 'repost_documents', 'post_document', …
    action_name          TEXT NOT NULL,
    -- 'execute' | 'dry_run'
    mode                 TEXT NOT NULL,
    -- 'planned'     — сухой прогон, эффекта не было;
    -- 'in_progress' — исполнение начато и не завершилось: после перезапуска это
    --                 неизвестность, а не повод повторить (см. ADR-0011 п.10);
    -- 'executed'    — эффект состоялся, результат в result_json;
    -- 'failed'      — исполнение упало, текст в error_text; повтор разрешён.
    status               TEXT NOT NULL,
    input_json           TEXT NOT NULL DEFAULT '{}',
    result_json          TEXT,
    error_text           TEXT,
    -- Кто инициировал: 'process:<instance_id>' | 'user:<id>' | 'manual'.
    -- Строкой, а не FK: экземпляров процессов ещё нет, а журнал уже нужен.
    actor                TEXT NOT NULL,
    -- Экземпляр процесса и Этап, с которого вызвано (NULL для ручных вызовов).
    process_instance_ref TEXT,
    stage_code           TEXT,
    started_at           TEXT NOT NULL,
    finished_at          TEXT,
    duration_ms          INTEGER
);

-- Идемпотентность гарантируется индексом, а не проверкой в коде: два воркера,
-- одновременно взявшие один экземпляр, обязаны разойтись на уровне БД.
-- Частичный — планы сухого прогона ключ не занимают, иначе прогон плана закрыл
-- бы дорогу настоящему исполнению с тем же ключом.
CREATE UNIQUE INDEX IF NOT EXISTS idx_sys_effect_log_idempotency
    ON sys_effect_log(idempotency_key)
    WHERE mode = 'execute';

CREATE INDEX IF NOT EXISTS idx_sys_effect_log_started
    ON sys_effect_log(started_at DESC);

CREATE INDEX IF NOT EXISTS idx_sys_effect_log_instance
    ON sys_effect_log(process_instance_ref, started_at);
