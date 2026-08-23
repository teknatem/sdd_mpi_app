-- Определения механизма Процессов: Этапы и Процессы, версия — строкой.
--
-- Почему в БД, а не файлами рядом с quality-проверками: экземпляр процесса
-- пинит версию определения, и этот пин обязан быть транзакционным с самим
-- экземпляром, который тоже в БД (ADR-0011 п.6). Ссылка на файл такой гарантии
-- не даёт: файл переписывается мимо транзакции и мимо истории.
--
-- Почему версия — отдельная строка, а не колонка `version` поверх одной записи:
-- живые экземпляры доживают на своей версии (п.7). Пока экземпляр идёт по
-- графу, его Этапы обязаны существовать ровно в том виде, в каком он их взял,
-- поэтому опубликованная версия не редактируется и не удаляется — новая
-- публикация заводит новую строку.
--
-- Идентичность — `code` (st0001 / pr0001), а не `id`: UUID меняется при
-- обновлении БД из боевой копии, код нет. Всё, что ссылается на определение
-- снаружи, ссылается парой (code, version).

CREATE TABLE IF NOT EXISTS sys_stage_definition (
    id            TEXT PRIMARY KEY,
    -- st0001. Идентичность Этапа, общая для всех его версий.
    code          TEXT NOT NULL,
    version       INTEGER NOT NULL,
    -- 'draft'    — правится и удаляется свободно, экземпляры не стартуют;
    -- 'active'   — ровно одна на код: на ней стартуют новые экземпляры;
    -- 'archived' — была активной; хранится вечно, на ней доживают экземпляры.
    status        TEXT NOT NULL DEFAULT 'draft',
    -- Дублируется из манифеста ради списков: история версий показывается без
    -- разбора JSON каждой строки.
    title         TEXT NOT NULL DEFAULT '',
    manifest_json TEXT NOT NULL DEFAULT '{}',
    script        TEXT NOT NULL DEFAULT '',
    -- SHA-256 манифеста и кода: «тот же Этап» опознаётся сравнением строки, а
    -- не разбором определения.
    digest        TEXT NOT NULL DEFAULT '',
    created_at    TEXT NOT NULL DEFAULT (datetime('now')),
    created_by    TEXT
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_sys_stage_definition_version
    ON sys_stage_definition(code, version);

-- Активная версия ровно одна на код — это гарантия БД, а не соглашение кода:
-- активацию может нажать второй администратор, пока идёт первая.
CREATE UNIQUE INDEX IF NOT EXISTS idx_sys_stage_definition_active
    ON sys_stage_definition(code)
    WHERE status = 'active';

-- Черновик тоже один на код: правка идёт по месту, а не плодит версии. Иначе
-- история версий заросла бы промежуточными сохранениями автора, и «что
-- изменилось с прошлого раза» пришлось бы искать глазами.
CREATE UNIQUE INDEX IF NOT EXISTS idx_sys_stage_definition_draft
    ON sys_stage_definition(code)
    WHERE status = 'draft';

CREATE TABLE IF NOT EXISTS sys_process_definition (
    id            TEXT PRIMARY KEY,
    -- pr0001.
    code          TEXT NOT NULL,
    version       INTEGER NOT NULL,
    status        TEXT NOT NULL DEFAULT 'draft',
    title         TEXT NOT NULL DEFAULT '',
    -- Манифест: триггер, входной Этап, рёбра графа, парная quality-проверка.
    -- Этапы адресуются кодом, без версии: версии пинит экземпляр на старте.
    manifest_json TEXT NOT NULL DEFAULT '{}',
    digest        TEXT NOT NULL DEFAULT '',
    -- Версии Этапов, зафиксированные в момент активации этой версии Процесса:
    -- [{"code","version","digest"}]. Пин живёт здесь, а не разрешается заново
    -- на старте каждого экземпляра, по двум причинам. Во-первых, иначе
    -- публикация Этапа молча меняла бы поведение всех Процессов, которые его
    -- переиспользуют, — а Этапы лежат в глобальном каталоге именно ради
    -- переиспользования. Во-вторых, без записи «что работало вчера» нечем
    -- показать двухуровневый diff перед активацией (ADR-0011 п.7): «Процесс не
    -- менялся» ничего не значит, если под ним поменялся Этап.
    pins_json     TEXT NOT NULL DEFAULT '[]',
    created_at    TEXT NOT NULL DEFAULT (datetime('now')),
    created_by    TEXT
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_sys_process_definition_version
    ON sys_process_definition(code, version);

CREATE UNIQUE INDEX IF NOT EXISTS idx_sys_process_definition_active
    ON sys_process_definition(code)
    WHERE status = 'active';

CREATE UNIQUE INDEX IF NOT EXISTS idx_sys_process_definition_draft
    ON sys_process_definition(code)
    WHERE status = 'draft';
