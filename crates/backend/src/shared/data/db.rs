use crate::shared::config;
use once_cell::sync::OnceCell;
use sea_orm::{
    ConnectionTrait, DatabaseBackend, DatabaseConnection, SqlxSqliteConnector, Statement,
};
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use std::time::Duration;
use tokio::sync::{Mutex, MutexGuard};

static DB_CONN: OnceCell<DatabaseConnection> = OnceCell::new();
// SQLite has exactly one writer. Serializing the application's compound write
// transactions avoids DEFERRED transaction upgrade races (SQLITE_BUSY_SNAPSHOT)
// without reducing concurrency of API calls and read-only work.
static SQLITE_WRITE_LOCK: Mutex<()> = Mutex::const_new(());

/// Pool sizing. SQLite serializes writes regardless of pool size, so a larger pool does NOT add
/// write throughput — its job is to avoid *connection starvation* (the "pool timed out" failure)
/// when many tasks hold connections while blocked on the single write lock.
const MAX_CONNECTIONS: u32 = 16;
/// How long `.await` on a query waits for a free pooled connection before erroring.
/// Kept >= BUSY_TIMEOUT so a connection legitimately waiting on the write lock doesn't trip this.
const ACQUIRE_TIMEOUT_SECS: u64 = 30;
/// How long SQLite waits for the write lock before returning SQLITE_BUSY (per connection).
const BUSY_TIMEOUT_SECS: u64 = 15;

/// Открывает боевую базу и запоминает соединение в глобальном мосте.
///
/// Возвращает соединение, чтобы вызывающий положил его в [`AppState`] —
/// синглтон при этом остаётся заполненным ради 166 файлов, которые пока ходят
/// через [`get_connection`].
///
/// [`AppState`]: crate::shared::app_state::AppState
pub async fn initialize_database() -> anyhow::Result<DatabaseConnection> {
    if let Some(existing) = DB_CONN.get() {
        return Ok(existing.clone());
    }

    let cfg = config::load_config()?;
    let config_path = config::get_config_path()?;
    let absolute_path = config::get_database_path(&cfg)?;

    if let Some(parent) = absolute_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    println!("✓ Config path resolved to: {}", config_path.display());
    println!("✓ Database path resolved to: {}", absolute_path.display());
    tracing::info!("Connecting to database: {}", absolute_path.display());

    // Per-connection options applied to EVERY pooled connection by sqlx.
    // WAL is a persistent file property (readers no longer block the writer and vice versa —
    // the key fix for write-lock contention under concurrent load); synchronous=NORMAL is safe
    // under WAL; busy_timeout lets writers queue for the lock instead of erroring immediately.
    let connect_options = SqliteConnectOptions::new()
        .filename(&absolute_path)
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal)
        .busy_timeout(Duration::from_secs(BUSY_TIMEOUT_SECS))
        .foreign_keys(true);

    let pool = SqlitePoolOptions::new()
        .max_connections(MAX_CONNECTIONS)
        .acquire_timeout(Duration::from_secs(ACQUIRE_TIMEOUT_SECS))
        .connect_with(connect_options)
        .await?;

    let conn = SqlxSqliteConnector::from_sqlx_sqlite_pool(pool);

    DB_CONN
        .set(conn.clone())
        .map_err(|_| anyhow::anyhow!("Failed to set DB_CONN"))?;
    Ok(conn)
}

/// Пустая база с прогнанными миграциями — для интеграционных тестов.
///
/// Не `#[cfg(test)]`: интеграционные тесты компилируются отдельным крейтом и
/// видят библиотеку ровно в том виде, в каком её собирают для боевого бинаря.
///
/// Соединение кладётся в тот же глобальный мост, поэтому код, ещё не
/// переведённый на [`AppState`], в тестах работает против этой базы. Отсюда
/// два следствия, и оба намеренные: **база одна на весь тестовый бинарь**
/// (`OnceCell` заполняется один раз), и тесты, пишущие в одни и те же таблицы,
/// обязаны учитывать соседей.
///
/// **Почему файл во временном каталоге, а не `:memory:`.** У SQLite in-memory
/// база принадлежит *соединению*: пул открывает второе — и оно попадает в
/// пустую базу, где нет ни `_sqlx_migrations`, ни настроек. Проверено дорогой
/// ценой: тесты падали то на «no such table», то на 401, потому что секрет JWT
/// каждый раз читался из другой базы. `cache=shared` чинит владение, но взамен
/// приносит `SQLITE_LOCKED`, который не лечится `busy_timeout`. Файл в TEMP
/// снимает вопрос целиком; имя содержит pid, так что параллельные тестовые
/// бинари не пересекаются, а остатки перетираются при следующем запуске.
///
/// [`AppState`]: crate::shared::app_state::AppState
pub async fn init_test_database() -> anyhow::Result<DatabaseConnection> {
    // Тесты внутри бинаря идут параллельно и все зовут эту функцию. Без замка
    // второй вызов увидел бы пустой `OnceCell`, начал поднимать свою базу и
    // проиграл гонку на `set` — а первый к тому моменту уже отдал бы роутер,
    // работающий против ещё не мигрированной базы.
    static INIT: Mutex<()> = Mutex::const_new(());
    let _guard = INIT.lock().await;

    if let Some(existing) = DB_CONN.get() {
        return Ok(existing.clone());
    }

    let path = std::env::temp_dir().join(format!("backend-test-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&path);

    let pool = SqlitePoolOptions::new()
        .max_connections(4)
        .acquire_timeout(Duration::from_secs(10))
        .connect_with(
            SqliteConnectOptions::new()
                .filename(&path)
                .create_if_missing(true)
                .foreign_keys(true),
        )
        .await?;

    crate::shared::data::migration_runner::run_migrations_on(&pool).await?;

    // Секрет JWT фиксируем здесь, а не оставляем на `get_jwt_secret`: тот при
    // пустой таблице генерирует новый и сохраняет, и два параллельных теста
    // успевают сгенерировать по своему — токен, подписанный первым секретом,
    // проверяется вторым и даёт 401. Ставим один раз, до выдачи роутера.
    sqlx::query(
        "INSERT OR REPLACE INTO sys_settings (key, value, description, created_at, updated_at)
         VALUES ('jwt_secret', ?, 'Fixed secret for the test database', ?, ?)",
    )
    .bind("test-jwt-secret-not-for-production")
    .bind(chrono::Utc::now().to_rfc3339())
    .bind(chrono::Utc::now().to_rfc3339())
    .execute(&pool)
    .await?;

    let conn = SqlxSqliteConnector::from_sqlx_sqlite_pool(pool);
    DB_CONN
        .set(conn.clone())
        .map_err(|_| anyhow::anyhow!("Failed to set DB_CONN"))?;
    Ok(conn)
}

/// **Мост, а не точка расширения.** Глобальный синглтон оставлен рабочим,
/// потому что перевод 166 файлов одним коммитом непроверяем. Новый код берёт
/// базу из [`AppState`] (`state.db()`), правимый — переводится вместе с правкой.
///
/// [`AppState`]: crate::shared::app_state::AppState
pub fn get_connection() -> &'static DatabaseConnection {
    DB_CONN
        .get()
        .expect("Database connection has not been initialized")
}

/// Serialize compound SQLite write transactions inside this application process.
/// Keep the guard only for the actual database transaction; prepare data before acquiring it.
pub async fn acquire_sqlite_write_lock() -> MutexGuard<'static, ()> {
    SQLITE_WRITE_LOCK.lock().await
}

/// Migrate existing WB Sales documents to fill denormalized columns from JSON
/// This function should be called once to backfill all denormalized fields
pub async fn migrate_wb_sales_denormalize() -> anyhow::Result<u64> {
    let conn = get_connection();

    // Get all WB Sales documents that need migration (sale_date is null means not migrated)
    let sql = r#"
        SELECT id, header_json, line_json, state_json, source_meta_json 
        FROM a012_wb_sales 
        WHERE sale_date IS NULL
    "#;

    let rows = conn
        .query_all(Statement::from_string(
            DatabaseBackend::Sqlite,
            sql.to_string(),
        ))
        .await?;

    let total = rows.len();
    if total == 0 {
        tracing::info!("No WB Sales documents need denormalization migration");
        return Ok(0);
    }

    tracing::info!(
        "Found {} WB Sales documents to denormalize, starting migration...",
        total
    );

    let mut updated = 0u64;
    for row in rows {
        let id: String = row.try_get("", "id")?;
        let header_json: String = row.try_get("", "header_json")?;
        let line_json: String = row.try_get("", "line_json")?;
        let state_json: String = row.try_get("", "state_json")?;
        let source_meta_json: String = row.try_get("", "source_meta_json")?;

        // Parse JSON fields
        let header: serde_json::Value = serde_json::from_str(&header_json).unwrap_or_default();
        let line: serde_json::Value = serde_json::from_str(&line_json).unwrap_or_default();
        let state: serde_json::Value = serde_json::from_str(&state_json).unwrap_or_default();
        let source_meta: serde_json::Value =
            serde_json::from_str(&source_meta_json).unwrap_or_default();

        // Extract values
        let sale_id = header
            .get("sale_id")
            .and_then(|v| v.as_str())
            .map(|s| s.replace("'", "''"));
        let organization_id = header
            .get("organization_id")
            .and_then(|v| v.as_str())
            .map(|s| s.replace("'", "''"));
        let connection_id = header
            .get("connection_id")
            .and_then(|v| v.as_str())
            .map(|s| s.replace("'", "''"));

        let supplier_article = line
            .get("supplier_article")
            .and_then(|v| v.as_str())
            .map(|s| s.replace("'", "''"));
        let nm_id = line.get("nm_id").and_then(|v| v.as_i64());
        let barcode = line
            .get("barcode")
            .and_then(|v| v.as_str())
            .map(|s| s.replace("'", "''"));
        let product_name = line
            .get("name")
            .and_then(|v| v.as_str())
            .map(|s| s.replace("'", "''"));
        let qty = line.get("qty").and_then(|v| v.as_f64());
        let amount_line = line.get("amount_line").and_then(|v| v.as_f64());
        let total_price = line.get("total_price").and_then(|v| v.as_f64());
        let finished_price = line.get("finished_price").and_then(|v| v.as_f64());

        let event_type = state
            .get("event_type")
            .and_then(|v| v.as_str())
            .map(|s| s.replace("'", "''"));
        let sale_dt = state
            .get("sale_dt")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        // Try to get sale_id from raw JSON if not in header
        let sale_id = if sale_id.is_none() {
            let raw_payload_ref = source_meta.get("raw_payload_ref").and_then(|v| v.as_str());
            if let Some(ref_id) = raw_payload_ref {
                let raw_sql = format!(
                    "SELECT raw_json FROM document_raw_storage WHERE id = '{}'",
                    ref_id
                );
                let raw_result = conn
                    .query_one(Statement::from_string(DatabaseBackend::Sqlite, raw_sql))
                    .await
                    .ok()
                    .flatten();

                if let Some(raw_row) = raw_result {
                    if let Ok(raw_json_str) = raw_row.try_get::<String>("", "raw_json") {
                        let raw: serde_json::Value =
                            serde_json::from_str(&raw_json_str).unwrap_or_default();
                        raw.get("saleID")
                            .and_then(|v| v.as_str())
                            .map(|s| s.replace("'", "''"))
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            sale_id
        };

        // Build UPDATE statement
        let mut sets = Vec::new();

        if let Some(v) = sale_id {
            sets.push(format!("sale_id = '{}'", v));
        }
        if let Some(v) = sale_dt {
            sets.push(format!("sale_date = '{}'", v));
        }
        if let Some(v) = organization_id {
            sets.push(format!("organization_id = '{}'", v));
        }
        if let Some(v) = connection_id {
            sets.push(format!("connection_id = '{}'", v));
        }
        if let Some(v) = supplier_article {
            sets.push(format!("supplier_article = '{}'", v));
        }
        if let Some(v) = nm_id {
            sets.push(format!("nm_id = {}", v));
        }
        if let Some(v) = barcode {
            sets.push(format!("barcode = '{}'", v));
        }
        if let Some(v) = product_name {
            sets.push(format!("product_name = '{}'", v));
        }
        if let Some(v) = qty {
            sets.push(format!("qty = {}", v));
        }
        if let Some(v) = amount_line {
            sets.push(format!("amount_line = {}", v));
        }
        if let Some(v) = total_price {
            sets.push(format!("total_price = {}", v));
        }
        if let Some(v) = finished_price {
            sets.push(format!("finished_price = {}", v));
        }
        if let Some(v) = event_type {
            sets.push(format!("event_type = '{}'", v));
        }

        if sets.is_empty() {
            continue;
        }

        let update_sql = format!(
            "UPDATE a012_wb_sales SET {} WHERE id = '{}'",
            sets.join(", "),
            id
        );

        conn.execute(Statement::from_string(DatabaseBackend::Sqlite, update_sql))
            .await?;

        updated += 1;

        if updated % 100 == 0 {
            tracing::info!(
                "Denormalization progress: {}/{} documents updated",
                updated,
                total
            );
        }
    }

    tracing::info!(
        "WB Sales denormalization completed: {} documents updated",
        updated
    );
    Ok(updated)
}
