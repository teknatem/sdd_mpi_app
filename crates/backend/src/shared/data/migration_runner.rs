use crate::shared::config;
use sha2::{Digest, Sha384};
use sqlx::sqlite::SqlitePool;
use sqlx::{Executor, Row};
use std::path::{Path, PathBuf};

fn build_sqlite_url(path: &Path) -> String {
    let normalized = path.to_string_lossy().replace('\\', "/");
    let needs_leading_slash = !normalized.starts_with('/') && normalized.contains(':');
    let prefix = if needs_leading_slash { "/" } else { "" };
    format!("sqlite://{}{}?mode=rwc", prefix, normalized)
}

fn candidate_migrations_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();

    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            dirs.push(exe_dir.join("migrations"));
        }
    }

    dirs.push(PathBuf::from("migrations"));
    dirs.push(PathBuf::from("../migrations"));
    dirs.push(PathBuf::from("../../migrations"));
    dirs.push(PathBuf::from("../../../migrations"));

    dirs
}

async fn has_table(pool: &SqlitePool, table_name: &str) -> anyhow::Result<bool> {
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(1) FROM sqlite_master WHERE type='table' AND name = ?1")
            .bind(table_name)
            .fetch_one(pool)
            .await?;
    Ok(count > 0)
}

fn migration_checksum(contents: &[u8]) -> Vec<u8> {
    Sha384::digest(contents).to_vec()
}

fn normalize_line_endings(contents: &[u8]) -> (Vec<u8>, Vec<u8>) {
    let text = String::from_utf8_lossy(contents);
    let lf = text.replace("\r\n", "\n").replace('\r', "\n");
    let crlf = lf.replace('\n', "\r\n");
    (lf.into_bytes(), crlf.into_bytes())
}

fn find_migration_path(migrations_dir: &Path, version: i64) -> anyhow::Result<Option<PathBuf>> {
    let prefix = format!("{version:04}_");
    Ok(std::fs::read_dir(migrations_dir)?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with(&prefix) && name.ends_with(".sql"))
        }))
}

async fn repair_line_ending_checksums(
    pool: &SqlitePool,
    migrations_dir: &Path,
) -> anyhow::Result<()> {
    let applied = sqlx::query("SELECT version, checksum FROM _sqlx_migrations WHERE success = 1")
        .fetch_all(pool)
        .await?;

    for row in applied {
        let version: i64 = row.try_get("version")?;
        let stored_checksum: Vec<u8> = row.try_get("checksum")?;
        let migration_path = find_migration_path(migrations_dir, version)?;

        let Some(migration_path) = migration_path else {
            continue;
        };

        let contents = std::fs::read(&migration_path)?;
        let current_checksum = migration_checksum(&contents);
        if stored_checksum == current_checksum {
            continue;
        }

        let (lf, crlf) = normalize_line_endings(&contents);
        let is_line_ending_only = stored_checksum == migration_checksum(&lf)
            || stored_checksum == migration_checksum(&crlf);
        if !is_line_ending_only {
            continue;
        }

        sqlx::query("UPDATE _sqlx_migrations SET checksum = ?1 WHERE version = ?2")
            .bind(current_checksum)
            .bind(version)
            .execute(pool)
            .await?;
        tracing::info!(
            "Repaired line-ending-only checksum mismatch for migration {}",
            version
        );
    }

    Ok(())
}

async fn repair_known_legacy_checksums(
    pool: &SqlitePool,
    migrations_dir: &Path,
) -> anyhow::Result<()> {
    const LEGACY_MIGRATION_73_CHECKSUM: &str =
        "292C5C2F13A02F029BF59AAABB290D69A0376DB48BBDE5AFDA8019C53467BEF6C4C2F1115E16B0578DA0E8E118038B0C";

    let Some(stored_checksum) = sqlx::query_scalar::<_, Vec<u8>>(
        "SELECT checksum FROM _sqlx_migrations WHERE version = 73 AND success = 1",
    )
    .fetch_optional(pool)
    .await?
    else {
        return Ok(());
    };

    let stored_hex = stored_checksum
        .iter()
        .map(|byte| format!("{byte:02X}"))
        .collect::<String>();
    if stored_hex != LEGACY_MIGRATION_73_CHECKSUM {
        return Ok(());
    }

    let migration_74_applied: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM _sqlx_migrations WHERE version = 74 AND success = 1)",
    )
    .fetch_one(pool)
    .await?;
    if !migration_74_applied || !has_table(pool, "a029_wb_supply").await? {
        return Ok(());
    }

    let Some(migration_path) = find_migration_path(migrations_dir, 73)? else {
        return Ok(());
    };
    let current_checksum = migration_checksum(&std::fs::read(migration_path)?);
    sqlx::query("UPDATE _sqlx_migrations SET checksum = ?1 WHERE version = 73")
        .bind(current_checksum)
        .execute(pool)
        .await?;
    tracing::info!("Repaired known legacy checksum for migration 73");

    Ok(())
}

/// Правки схемы, которые боевая база получила **мимо миграций**.
///
/// **Это починка, а не украшение: установка с нуля была сломана.**
/// `0001_baseline_schema.sql` подписан «used for fresh installations», но
/// цепочка местами обращается к таблицам и колонкам, которых ни одна миграция
/// не создаёт — они достались боевой базе из времён до миграций и правились
/// руками. На пустой базе цепочка падала на `0026` (`ALTER TABLE
/// general_ledger_entries RENAME TO sys_general_ledger` — такой таблицы нет).
/// Никто этого не замечал: боевая база существует непрерывно с дореформенных
/// времён, а нового инстанса с нуля никто не поднимал. Нашёл первый
/// интеграционный тест, поднявший базу в памяти.
///
/// **Почему кодом, а не новыми миграциями.** Исправить `0026` нельзя: правка
/// файла меняет контрольную сумму давно применённой миграции, и sqlx откажется
/// работать с боевой базой. Дописать новую миграцию тоже нельзя — она встанет
/// в конец, а дыры находятся в середине цепочки. Поэтому заплатки
/// **вклиниваются между миграциями** и только на полностью пустой базе.
///
/// **Как проверено.** Схема, полученная прогоном цепочки с этими заплатками,
/// сверена с боевой: 108 таблиц против 108, расхождение — 4 колонки,
/// добавленные мимо миграций уже после (`p904_sales_data.cost`,
/// `dealer_price_ut` у a012/a015 — её ставит `ensure_a015_dealer_price_ut_column`
/// ниже) и мёртвая `sys_journal_entries`.
///
/// Формат: `(версия, SQL)` — SQL выполняется **сразу после** миграции с этим
/// номером; `0` означает «до всех миграций».
const PRE_BASELINE_PATCHES: &[(i64, &str)] = &[
    // Главная книга до переименования в sys_general_ledger (0026/0027/0032
    // работают с этой формой, 0033 пересобирает таблицу начисто).
    (
        0,
        "CREATE TABLE IF NOT EXISTS general_ledger_entries (
            id TEXT PRIMARY KEY NOT NULL,
            entry_date TEXT NOT NULL,
            layer TEXT NOT NULL,
            registrator_type TEXT NOT NULL,
            registrator_ref TEXT NOT NULL,
            debit_account TEXT NOT NULL,
            credit_account TEXT NOT NULL,
            amount REAL NOT NULL,
            qty REAL,
            turnover_code TEXT,
            detail_kind TEXT,
            detail_id TEXT,
            created_at TEXT NOT NULL
        );",
    ),
    // 0022 пересобирает p909/p910, но в боевой базе они называются иначе:
    // event_date/turnover_date → entry_date, journal_entry_id → general_ledger_ref.
    // Без этого падают 0027 (p.general_ledger_ref), 0030 (entry_date), 0037.
    (
        22,
        "ALTER TABLE p909_mp_order_line_turnovers RENAME COLUMN event_date TO entry_date;
         ALTER TABLE p909_mp_order_line_turnovers RENAME COLUMN journal_entry_id TO general_ledger_ref;
         ALTER TABLE p910_mp_unlinked_turnovers RENAME COLUMN turnover_date TO entry_date;
         ALTER TABLE p910_mp_unlinked_turnovers ADD COLUMN general_ledger_ref TEXT;",
    ),
    // 0025 заводит p911_wb_advert_nomenclature_turnovers, а 0043 и дальше
    // обращаются к p911_wb_advert_by_items: таблицу переименовали мимо миграций.
    (
        25,
        "ALTER TABLE p911_wb_advert_nomenclature_turnovers RENAME TO p911_wb_advert_by_items;
         ALTER TABLE p911_wb_advert_by_items RENAME COLUMN turnover_date TO entry_date;
         ALTER TABLE p911_wb_advert_by_items RENAME COLUMN journal_entry_id TO general_ledger_ref;",
    ),
    // 0036 переносит p903 в новую таблицу и читает srid, которого в цепочке нет.
    (35, "ALTER TABLE p903_wb_finance_report ADD COLUMN srid TEXT;"),
];

/// Прогон цепочки на пустой базе: миграции идут ступенями, между ступенями
/// встают заплатки из [`PRE_BASELINE_PATCHES`].
///
/// `Migrator.migrations` — публичное поле sqlx, помеченное semver-exempt;
/// собрать из него подмножество дешевле, чем тащить свой раннер миграций.
async fn run_migrations_staged(
    pool: &SqlitePool,
    migrator: &sqlx::migrate::Migrator,
) -> anyhow::Result<()> {
    tracing::info!("Empty database detected: applying the chain with pre-baseline patches");

    for (version, patch) in PRE_BASELINE_PATCHES {
        if *version > 0 {
            let stage = sqlx::migrate::Migrator {
                migrations: std::borrow::Cow::Owned(
                    migrator
                        .iter()
                        .filter(|migration| migration.version <= *version)
                        .cloned()
                        .collect(),
                ),
                ignore_missing: migrator.ignore_missing,
                locking: migrator.locking,
            };
            stage.run(pool).await?;
        }
        pool.execute(*patch).await?;
    }

    migrator.run(pool).await?;
    Ok(())
}

pub async fn run_migrations() -> anyhow::Result<()> {
    let cfg = config::load_config()?;
    let db_path = config::get_database_path(&cfg)?;
    let db_url = build_sqlite_url(&db_path);

    let pool = SqlitePool::connect(&db_url).await?;
    run_migrations_on(&pool).await
}

/// Тот же прогон, но на уже открытом пуле.
///
/// Отделено от [`run_migrations`] ради тестовой базы в памяти: пул для неё
/// нельзя построить из конфига (файла нет), а прогонять миграции надо тем же
/// кодом — иначе тест проверяет схему, которой в бою не существует.
pub async fn run_migrations_on(pool: &SqlitePool) -> anyhow::Result<()> {
    let pool = pool.clone();

    let has_migrations_table = has_table(&pool, "_sqlx_migrations").await?;
    let has_core_table = has_table(&pool, "a001_connection_1c_database").await?;
    if !has_migrations_table && has_core_table {
        tracing::info!(
            "Legacy database detected (business tables exist, _sqlx_migrations absent). Running baseline migration in idempotent mode."
        );
    }
    let is_empty_database = !has_migrations_table && !has_core_table;

    let migrations_dir = candidate_migrations_dirs()
        .into_iter()
        .find(|p| p.exists() && p.is_dir())
        .ok_or_else(|| anyhow::anyhow!("migrations directory not found"))?;

    tracing::info!("Using migrations directory: {}", migrations_dir.display());

    // Обе починки читают `_sqlx_migrations`. На боевой базе она есть всегда, на
    // пустой (тестовой) её ещё не создал мигратор — чинить там нечего.
    if has_migrations_table {
        repair_line_ending_checksums(&pool, &migrations_dir).await?;
        repair_known_legacy_checksums(&pool, &migrations_dir).await?;
    }

    let migrator = sqlx::migrate::Migrator::new(migrations_dir.as_path()).await?;
    if is_empty_database {
        run_migrations_staged(&pool, &migrator).await?;
    } else {
        migrator.run(&pool).await?;
    }

    ensure_a015_dealer_price_ut_column(&pool).await?;
    ensure_llm_agent_fk_dropped(&pool).await?;

    tracing::info!("Database migrations applied successfully");
    Ok(())
}

/// Текущий номер (наибольшая успешно применённая версия) миграции БД этого инстанса —
/// для ручной сверки с `PluginManifest.built_for_migration` на странице разработки плагина.
pub async fn current_migration_version() -> anyhow::Result<i64> {
    use sea_orm::{ConnectionTrait, Statement};

    let db = crate::shared::data::db::get_connection();
    let stmt = Statement::from_string(
        sea_orm::DatabaseBackend::Sqlite,
        "SELECT MAX(version) as version FROM _sqlx_migrations WHERE success = 1".to_string(),
    );
    let row = db.query_one(stmt).await?;
    Ok(row
        .and_then(|row| row.try_get::<i64>("", "version").ok())
        .unwrap_or(0))
}

async fn has_column(pool: &SqlitePool, table: &str, column: &str) -> anyhow::Result<bool> {
    use sqlx::Row;
    // PRAGMA не принимает bind-параметры — имя таблицы подставляем напрямую (оно из кода, не из ввода).
    let rows = sqlx::query(&format!("PRAGMA table_info({table})"))
        .fetch_all(pool)
        .await?;
    Ok(rows.iter().any(|row| {
        row.try_get::<String, _>("name")
            .map(|name| name == column)
            .unwrap_or(false)
    }))
}

/// Идемпотентно гарантирует наличие денормализованного столбца
/// `a015_wb_orders.dealer_price_ut` (зеркало `line_json.$.dealer_price_ut`).
///
/// Через обычную sqlx-миграцию это сделать нельзя: на части баз столбец был заведён
/// вне миграций, и `ALTER TABLE ... ADD COLUMN` там падает с `duplicate column name`,
/// а в SQLite нет `ADD COLUMN IF NOT EXISTS`. Поэтому делаем программно и идемпотентно:
/// добавляем столбец, если его нет; бэкфиллим пустые значения из `line_json`; создаём индекс.
/// Дальше столбец поддерживается при каждом сохранении документа.
async fn ensure_a015_dealer_price_ut_column(pool: &SqlitePool) -> anyhow::Result<()> {
    if !has_table(pool, "a015_wb_orders").await? {
        return Ok(());
    }

    if !has_column(pool, "a015_wb_orders", "dealer_price_ut").await? {
        sqlx::query("ALTER TABLE a015_wb_orders ADD COLUMN dealer_price_ut REAL")
            .execute(pool)
            .await?;
        tracing::info!("Added column a015_wb_orders.dealer_price_ut");
    }

    // Бэкфилл только пустых зеркал — на повторных стартах ничего не находит и стоит дёшево.
    sqlx::query(
        "UPDATE a015_wb_orders \
         SET dealer_price_ut = json_extract(line_json, '$.dealer_price_ut') \
         WHERE dealer_price_ut IS NULL \
           AND json_extract(line_json, '$.dealer_price_ut') IS NOT NULL",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_a015_dealer_price_ut \
         ON a015_wb_orders(dealer_price_ut)",
    )
    .execute(pool)
    .await?;

    Ok(())
}

/// DDL таблицы из `sqlite_master` (None, если таблицы нет).
async fn table_ddl(pool: &SqlitePool, table: &str) -> anyhow::Result<Option<String>> {
    Ok(sqlx::query_scalar::<_, String>(
        "SELECT sql FROM sqlite_master WHERE type='table' AND name = ?1",
    )
    .bind(table)
    .fetch_optional(pool)
    .await?)
}

/// Вырезает из DDL таблицы объявление внешнего ключа по колонке `agent_id`
/// (вместе с предшествующей запятой). None — если такого FK в DDL нет.
fn strip_agent_fk(ddl: &str) -> Option<String> {
    let re = regex::Regex::new(
        r#"(?is),\s*FOREIGN\s+KEY\s*\(\s*"?agent_id"?\s*\)\s*REFERENCES\s*[^,()]*\([^)]*\)(\s+ON\s+(DELETE|UPDATE)\s+(NO\s+ACTION|RESTRICT|SET\s+NULL|SET\s+DEFAULT|CASCADE))*"#,
    )
    .ok()?;
    let stripped = re.replace(ddl, "");
    match stripped {
        std::borrow::Cow::Borrowed(_) => None,
        std::borrow::Cow::Owned(s) => Some(s),
    }
}

/// Идемпотентно снимает внешний ключ по колонке `agent_id` у одной таблицы: пересобирает её
/// по собственному DDL из `sqlite_master` минус FK-объявление, сохраняя колонки, остальные
/// ограничения и индексы. Возвращает `true`, если пересборка выполнялась.
///
/// SQLite не умеет менять ограничения на месте, а `DROP TABLE` с включёнными FK каскадно
/// удалил бы дочерние строки (сообщения/задания чата), поэтому пересборка идёт при
/// выключенных FK и вне транзакции — внутри неё `PRAGMA foreign_keys` игнорируется.
async fn drop_agent_fk(pool: &SqlitePool, table: &str) -> anyhow::Result<bool> {
    let Some(ddl) = table_ddl(pool, table).await? else {
        return Ok(false);
    };
    let Some(stripped) = strip_agent_fk(&ddl) else {
        return Ok(false);
    };

    // Всё до первой открывающей скобки — заголовок `CREATE TABLE <имя>`; определения колонок
    // начинаются после неё, поэтому переименование сводится к замене заголовка.
    let body_start = stripped
        .find('(')
        .ok_or_else(|| anyhow::anyhow!("Unexpected DDL for {table}: no column list"))?;
    let tmp = format!("{table}__fkfix");

    let index_ddls: Vec<String> = sqlx::query_scalar(
        "SELECT sql FROM sqlite_master WHERE type='index' AND tbl_name = ?1 AND sql IS NOT NULL",
    )
    .bind(table)
    .fetch_all(pool)
    .await?;

    let mut script = String::from("PRAGMA foreign_keys=OFF;\nBEGIN;\n");
    script.push_str(&format!(
        "CREATE TABLE \"{tmp}\" {};\n",
        &stripped[body_start..]
    ));
    script.push_str(&format!(
        "INSERT INTO \"{tmp}\" SELECT * FROM \"{table}\";\n"
    ));
    script.push_str(&format!("DROP TABLE \"{table}\";\n"));
    script.push_str(&format!("ALTER TABLE \"{tmp}\" RENAME TO \"{table}\";\n"));
    for index_ddl in index_ddls {
        script.push_str(&index_ddl);
        script.push_str(";\n");
    }
    script.push_str("COMMIT;\nPRAGMA foreign_keys=ON;\n");

    let mut conn = pool.acquire().await?;
    match (&mut *conn).execute(script.as_str()).await {
        Ok(_) => Ok(true),
        Err(e) => {
            // Откатываем возможную открытую транзакцию и восстанавливаем enforcement FK.
            let _ = (&mut *conn).execute("ROLLBACK;").await;
            let _ = (&mut *conn).execute("PRAGMA foreign_keys=ON;").await;
            Err(anyhow::anyhow!(
                "Failed to drop agent_id FK on {table}: {e}"
            ))
        }
    }
}

/// Идемпотентно снимает внешний ключ `agent_id` у `a018_llm_chat` и `a019_llm_artifact`.
///
/// `agent_id` — полиморфная ссылка: сначала AI-сотрудник `a017_llm_agent` (собеседник чата,
/// именно его id шлёт фронт), иначе legacy-подключение `a038_llm_connection`. Разрешает её
/// код (`a018_llm_chat::service::resolve_effective_agent`), а FK на одну конкретную таблицу
/// выразить это не может: сотрудник без a038-близнеца (миграция 0165 создала близнецов только
/// для существовавших тогда агентов) ронял создание чата с `FOREIGN KEY constraint failed`.
async fn ensure_llm_agent_fk_dropped(pool: &SqlitePool) -> anyhow::Result<()> {
    for table in ["a018_llm_chat", "a019_llm_artifact"] {
        if drop_agent_fk(pool, table).await? {
            tracing::info!("Dropped agent_id FK on {table} (polymorphic ref a017/a038)");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::strip_agent_fk;

    #[test]
    fn strips_agent_fk_and_keeps_the_rest() {
        let ddl = "CREATE TABLE \"a019_llm_artifact\" (id TEXT PRIMARY KEY, \
                   code TEXT NOT NULL UNIQUE, chat_id TEXT NOT NULL, agent_id TEXT NOT NULL, \
                   FOREIGN KEY (chat_id) REFERENCES a018_llm_chat(id), \
                   FOREIGN KEY (agent_id) REFERENCES a038_llm_connection(id))";
        let stripped = strip_agent_fk(ddl).expect("agent_id FK must be found");
        assert!(!stripped.contains("a038_llm_connection"));
        assert!(stripped.contains("FOREIGN KEY (chat_id) REFERENCES a018_llm_chat(id)"));
        assert!(stripped.trim_end().ends_with(')'));
        assert!(stripped.contains("agent_id TEXT NOT NULL"));
    }

    #[test]
    fn returns_none_without_agent_fk() {
        let ddl = "CREATE TABLE a018_llm_chat (id TEXT PRIMARY KEY, agent_id TEXT NOT NULL)";
        assert!(strip_agent_fk(ddl).is_none());
    }
}
