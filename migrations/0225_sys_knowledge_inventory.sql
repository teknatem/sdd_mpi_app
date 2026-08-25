-- Инвентаризация знаний: снимок того, из чего система и её LLM-чат извлекают
-- знание, и сколько из этого достижимо.
--
-- Зачем история, а не расчёт на лету: вопрос не «сколько единиц сейчас», а
-- «что появилось, что исчезло и что перестало быть достижимым». Одно значение
-- без предыдущего на этот вопрос не отвечает.
--
-- Почему строка на единицу, а не JSON-документ: главный сценарий страницы —
-- таблица с фильтрами по девяти осям. В JSON такой фильтр превращается в разбор
-- всего снимка на каждый чих; в колонках это обычный WHERE.
--
-- Чего здесь принципиально нет: описаний классификаторов. Коды осей, их
-- подписи и порядок живут в contracts::knowledge::classifiers, как METRIC_CATALOG
-- и SCOPE_CATALOG живут в коде: состав классификаторов — часть версии
-- приложения, а не данные экземпляра. Экземпляр, умеющий завести свою ось,
-- сделал бы две базы несравнимыми.

CREATE TABLE IF NOT EXISTS sys_knowledge_snapshot (
    id                 TEXT PRIMARY KEY,
    captured_at        TEXT NOT NULL,
    -- 'startup' | 'manual'
    trigger            TEXT NOT NULL,
    -- Версия состава классификаторов (contracts::knowledge::CLASSIFIER_VERSION).
    -- Снимки разных версий не сравниваются поразрезно: разреза, которого тогда
    -- не было, задним числом не существует, и дельта по нему была бы выдумкой.
    classifier_version INTEGER NOT NULL,
    app_version        TEXT NOT NULL,
    unit_count         INTEGER NOT NULL DEFAULT 0,
    surface_count      INTEGER NOT NULL DEFAULT 0,
    -- Только хранимые единицы. Вычисляемые сюда не входят по определению:
    -- у них нет полного размера, только цена конкретного ответа.
    stored_tokens      INTEGER NOT NULL DEFAULT 0,
    collect_ms         INTEGER NOT NULL DEFAULT 0,
    -- Сводка §6 целиком: списки недостижимых поверхностей, мёртвых инструментов,
    -- расхождений. Ряд по ним не строится, разбирать по колонкам нечего.
    summary_json       TEXT NOT NULL DEFAULT '{}',
    diagnostics_json   TEXT NOT NULL DEFAULT '[]'
);

CREATE INDEX IF NOT EXISTS idx_sys_knowledge_snapshot_at
    ON sys_knowledge_snapshot(captured_at DESC);

CREATE TABLE IF NOT EXISTS sys_knowledge_unit (
    snapshot_id    TEXT NOT NULL,
    -- Всегда с префиксом типа: article:, entity:, action:, skill:, plugin:,
    -- process:, stage:, check:, source:, account:, turnover:, ui_scope:, tool:,
    -- task:, ext_route:, table:, help:, prompt:, vocabulary:.
    -- Без префикса статья `plugins` и карта `plugins.md` схлопнулись бы в один
    -- идентификатор, и одна из них молча исчезла бы из счёта.
    unit_id        TEXT NOT NULL,
    surface_id     TEXT NOT NULL,

    -- Девять осей классификации. Коды стабильны и разбираются
    -- classifiers::from_code; неизвестный код означает снимок другой версии.
    family         TEXT NOT NULL,
    origin         TEXT NOT NULL,
    storage_form   TEXT NOT NULL,
    editor         TEXT NOT NULL,
    reachability   TEXT NOT NULL,
    lifecycle      TEXT NOT NULL,
    scope          TEXT NOT NULL,
    channel        TEXT NOT NULL,
    -- NULL — исходный код к единице отношения не имеет (курируемая статья).
    code_role      TEXT,

    title          TEXT NOT NULL DEFAULT '',
    subtitle       TEXT NOT NULL DEFAULT '',
    source_ref     TEXT,
    -- NULL, а не 0, у вычисляемых: ноль сложился бы в сумму и соврал бы про
    -- бюджет контекста.
    bytes          INTEGER,
    tokens         INTEGER,

    search_hits    INTEGER NOT NULL DEFAULT 0,
    read_hits      INTEGER NOT NULL DEFAULT 0,
    cited_hits     INTEGER NOT NULL DEFAULT 0,

    updated        TEXT,
    staleness_pct  INTEGER,
    tags_json      TEXT NOT NULL DEFAULT '[]',
    issues_json    TEXT NOT NULL DEFAULT '[]',

    PRIMARY KEY (snapshot_id, unit_id)
);

-- Фасеты страницы считаются по поверхности внутри снимка.
CREATE INDEX IF NOT EXISTS idx_sys_knowledge_unit_surface
    ON sys_knowledge_unit(snapshot_id, surface_id);

-- История одной единицы: «когда эта статья перестала читаться».
CREATE INDEX IF NOT EXISTS idx_sys_knowledge_unit_history
    ON sys_knowledge_unit(unit_id, snapshot_id);
