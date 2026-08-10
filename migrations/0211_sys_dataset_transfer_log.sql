-- Журнал переносов наборов данных между экземплярами приложения.
--
-- Почему отдельная таблица, а не `sys_files_s3`: снапшоты наборов принципиально
-- НЕ регистрируются как обычные S3-файлы. `sys_files_s3` — локальный реестр
-- одного инстанса, а весь смысл подсистемы в том, что принимающий экземпляр
-- этих строк не имеет и узнаёт о снапшотах из `datasets/catalog.json` в бакете.
-- Регистрация создала бы второй источник истины, который немедленно разъедется:
-- снапшот, удалённый на доноре, остался бы «существующим» на приёмнике.
--
-- Эта таблица решает другую задачу — местную историю: кто и когда что выгрузил
-- или восстановил на ЭТОМ инстансе, чем всё кончилось и куда положен архив для
-- отката. Строка = одна операция.

CREATE TABLE IF NOT EXISTS sys_dataset_transfer_log (
    id                  TEXT PRIMARY KEY,
    -- snapshot | restore
    operation           TEXT NOT NULL,
    snapshot_id         TEXT NOT NULL,
    -- Инстанс, на котором выполнена операция.
    instance_id         TEXT NOT NULL,
    -- Для restore: чей снапшот применяли. Для snapshot совпадает с instance_id.
    source_instance_id  TEXT,
    -- JSON-массив идентификаторов наборов.
    set_ids             TEXT NOT NULL,
    -- merge | replace; для операции snapshot — NULL.
    mode                TEXT,
    -- ok | failed | rolled_back
    status              TEXT NOT NULL,
    bytes               INTEGER NOT NULL DEFAULT 0,
    files_written       INTEGER NOT NULL DEFAULT 0,
    files_deleted       INTEGER NOT NULL DEFAULT 0,
    -- Путь к автоснапшоту, снятому перед восстановлением: единственный способ
    -- вернуться, если replace удалил нужное.
    pre_restore_archive TEXT,
    -- Шапка манифеста (без деревьев файлов) — чтобы история оставалась читаемой
    -- даже после удаления снапшота из бакета.
    manifest_json       TEXT,
    error               TEXT,
    actor_user_id       TEXT,
    created_at          TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS ix_sys_dataset_transfer_log_created
    ON sys_dataset_transfer_log (created_at DESC);
