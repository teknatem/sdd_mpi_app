-- Журнал доменных событий: факты, по которым Процессы стартуют и просыпаются.
--
-- Зачем таблица, а не рассылка подписчикам в памяти: ожидание переживает
-- перезапуск сервера (ADR-0011 п.2). Экземпляр может ждать сутки, а событие
-- «человек сделал» приходит один раз — если оно жило только в памяти воркера,
-- перезапуск между публикацией и разбором терял бы работу молча.
--
-- Журнал только дописывается. Событие не «доставляется» и не помечается
-- прочитанным: доставку считает потребитель своим курсором по `seq`, а
-- сведение с ожидающим экземпляром — по `correlation_token`. Так один и тот же
-- факт может разбудить и два экземпляра, и ни одного, и это не требует
-- ретроспективной правки строки.
--
-- Каталог событий закрыт и живёт в Rust (`contracts/processes/event.rs`): здесь
-- лежит имя, а не описание. Строка с именем не из каталога — порча данных, а не
-- «новое событие».

CREATE TABLE IF NOT EXISTS sys_domain_event (
    -- Порядковый номер публикации. Курсор потребителя двигается по нему, а не
    -- по времени: время в SQLite не монотонно между процессами, а `seq`
    -- монотонен по определению.
    seq                INTEGER PRIMARY KEY AUTOINCREMENT,
    id                 TEXT NOT NULL UNIQUE,
    -- Имя из каталога: 'import.day.completed', 'human.action.done', …
    kind               TEXT NOT NULL,
    -- Ключ корреляции: {"connection_id": "...", "business_date": "..."}.
    correlation_json   TEXT NOT NULL DEFAULT '{}',
    -- Канонический вид ключа: 'connection_id=…;business_date=…' в порядке,
    -- заданном каталогом. Сведение ожидания с событием — сравнение этой строки,
    -- поэтому она и хранится отдельно от JSON: сравнивать разобранный JSON в
    -- SQL нечем.
    correlation_token  TEXT NOT NULL,
    -- Данные события сверх ключа.
    payload_json       TEXT NOT NULL DEFAULT '{}',
    -- Кто опубликовал: 'u504', 'a033', 'ui', 'worker'.
    source             TEXT NOT NULL DEFAULT '',
    published_at       TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Пробуждение ожидающего экземпляра: «событие такого вида с таким ключом,
-- начиная с такого номера».
CREATE INDEX IF NOT EXISTS idx_sys_domain_event_match
    ON sys_domain_event(kind, correlation_token, seq);

CREATE INDEX IF NOT EXISTS idx_sys_domain_event_published
    ON sys_domain_event(published_at);
