//! Материализация результата SELECT в `serde_json::Value` по **рантайм-типу** значения
//! (как это делает `sqlite3` CLI), а НЕ по объявленному типу колонки.
//!
//! Зачем: генерический путь SeaORM `serde_json::Value::find_by_statement` на SQLite молча
//! теряет вычисляемые колонки без объявленного типа (`SUM()`, `COUNT()`, `CAST(...)`,
//! числовые литералы) — из-за чего графики/таблицы конструктора «фактически всегда» падали
//! с «Measure absent». Здесь колонки берём из `row.columns()` (порядок и имена сохраняются,
//! в т.ч. NULL-ячейки), а значение декодируем по фактическому классу хранения SQLite.

use crate::shared::data::db::get_connection;
use serde_json::{Number, Value};
use sqlx::{Column, Row, TypeInfo, ValueRef};

/// Скалярный bind-параметр для параметризованного SELECT (`?`-плейсхолдеры).
pub(crate) enum JsonBind {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    Text(String),
}

/// Выполнить SELECT через sqlx-пул и вернуть `(rows, column_names)`.
/// Каждая колонка материализуется по рантайм-типу значения — вычисляемые/агрегатные
/// колонки не теряются. Имена/порядок колонок берутся из `row.columns()`.
pub(crate) async fn fetch_json_rows(
    sql: &str,
    binds: Vec<JsonBind>,
) -> Result<(Vec<Value>, Vec<String>), String> {
    let pool = get_connection().get_sqlite_connection_pool();

    let mut query = sqlx::query(sql);
    for bind in binds {
        query = match bind {
            JsonBind::Null => query.bind(Option::<String>::None),
            JsonBind::Bool(value) => query.bind(value),
            JsonBind::Int(value) => query.bind(value),
            JsonBind::Float(value) => query.bind(value),
            JsonBind::Text(value) => query.bind(value),
        };
    }

    let sqlite_rows = query
        .fetch_all(pool)
        .await
        .map_err(|error| format!("SQL execution error: {error}"))?;

    let columns: Vec<String> = sqlite_rows
        .first()
        .map(|row| {
            row.columns()
                .iter()
                .map(|column| column.name().to_string())
                .collect()
        })
        .unwrap_or_default();

    let mut rows = Vec::with_capacity(sqlite_rows.len());
    for row in &sqlite_rows {
        let mut object = serde_json::Map::with_capacity(row.columns().len());
        for column in row.columns() {
            let index = column.ordinal();
            object.insert(column.name().to_string(), decode_cell(row, index)?);
        }
        rows.push(Value::Object(object));
    }

    Ok((rows, columns))
}

/// Декодировать одну ячейку по её рантайм-классу хранения SQLite.
fn decode_cell(row: &sqlx::sqlite::SqliteRow, index: usize) -> Result<Value, String> {
    // Имя типа берём из value-ref (owned), чтобы не держать borrow при последующем try_get.
    let type_name = {
        let value_ref = row
            .try_get_raw(index)
            .map_err(|error| format!("column read error: {error}"))?;
        if value_ref.is_null() {
            return Ok(Value::Null);
        }
        value_ref.type_info().name().to_string()
    };

    let cell = match type_name.as_str() {
        "INTEGER" => Value::from(get::<i64>(row, index)?),
        "REAL" => Number::from_f64(get::<f64>(row, index)?)
            .map(Value::Number)
            .unwrap_or(Value::Null),
        "TEXT" => Value::from(get::<String>(row, index)?),
        "BLOB" => {
            let bytes = get::<Vec<u8>>(row, index)?;
            Value::from(hex_encode(&bytes))
        }
        // Неизвестный класс — пытаемся как текст, иначе как число.
        _ => match row.try_get::<String, usize>(index) {
            Ok(text) => Value::from(text),
            Err(_) => Number::from_f64(get::<f64>(row, index)?)
                .map(Value::Number)
                .unwrap_or(Value::Null),
        },
    };
    Ok(cell)
}

fn get<'r, T>(row: &'r sqlx::sqlite::SqliteRow, index: usize) -> Result<T, String>
where
    T: sqlx::Decode<'r, sqlx::Sqlite> + sqlx::Type<sqlx::Sqlite>,
{
    row.try_get::<T, usize>(index)
        .map_err(|error| format!("column decode error at {index}: {error}"))
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    /// Ядро регрессии: агрегатная колонка `SUM(v) AS s` ДОЛЖНА присутствовать в строке
    /// и быть числом. До фикса SeaORM JsonValue её молча выбрасывал.
    #[tokio::test]
    async fn aggregate_and_computed_columns_survive() {
        let pool = SqlitePoolOptions::new()
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::query("CREATE TABLE t (d TEXT, v REAL)")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO t (d, v) VALUES ('a', 2.0), ('a', 3.0), ('b', 5.0)")
            .execute(&pool)
            .await
            .unwrap();

        // Прямой прогон декодера на этом пуле (мимо глобального соединения).
        let rows = sqlx::query(
            "SELECT d, SUM(v) AS s, COUNT(*) AS n, 1 AS lit FROM t GROUP BY d ORDER BY s DESC",
        )
        .fetch_all(&pool)
        .await
        .unwrap();

        let first = &rows[0];
        let cols: Vec<String> = first
            .columns()
            .iter()
            .map(|c| c.name().to_string())
            .collect();
        assert_eq!(cols, vec!["d", "s", "n", "lit"]);

        // Все вычисляемые колонки материализуются числами.
        assert_eq!(super::decode_cell(first, 0).unwrap(), Value::from("b"));
        assert_eq!(super::decode_cell(first, 1).unwrap(), Value::from(5.0));
        assert_eq!(super::decode_cell(first, 2).unwrap(), Value::from(1_i64));
        assert_eq!(super::decode_cell(first, 3).unwrap(), Value::from(1_i64));
    }

    /// Прямое воспроизведение против ЖИВОЙ БД: до фикса SUM(qty) пропадал (только product_name).
    /// #[ignore] — требует локальный app.db; путь берётся из APP_DB_PATH, чтобы тест не был
    /// привязан к конкретной машине. Запуск:
    /// `$env:APP_DB_PATH="F:/data/sdd_mpi_app/app.db"; cargo test ... -- --ignored --nocapture`
    #[tokio::test]
    #[ignore]
    async fn live_db_aggregate_column_present() {
        use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode};
        use std::time::Duration;
        let db_path = std::env::var("APP_DB_PATH")
            .expect("APP_DB_PATH не задана — укажите путь к живой app.db");
        let opts = SqliteConnectOptions::new()
            .filename(&db_path)
            .journal_mode(SqliteJournalMode::Wal)
            .busy_timeout(Duration::from_secs(15));
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(opts)
            .await
            .unwrap();
        let rows = sqlx::query(
            "SELECT product_name, SUM(qty) AS qty FROM a012_wb_sales \
             WHERE substr(sale_date,1,10) BETWEEN '2026-05-01' AND '2026-05-31' \
             GROUP BY product_name ORDER BY qty DESC",
        )
        .fetch_all(&pool)
        .await
        .unwrap();
        let first = &rows[0];
        let cols: Vec<String> = first
            .columns()
            .iter()
            .map(|c| c.name().to_string())
            .collect();
        eprintln!("LIVE first_keys={cols:?}");
        assert!(cols.contains(&"qty".to_string()), "qty must be present");
        assert!(
            super::decode_cell(first, 1).unwrap().is_number(),
            "qty must decode as number"
        );
    }

    #[tokio::test]
    async fn null_cells_are_preserved_as_null() {
        let pool = SqlitePoolOptions::new()
            .connect("sqlite::memory:")
            .await
            .unwrap();
        let rows = sqlx::query("SELECT NULL AS a, 'x' AS b")
            .fetch_all(&pool)
            .await
            .unwrap();
        assert_eq!(super::decode_cell(&rows[0], 0).unwrap(), Value::Null);
        assert_eq!(super::decode_cell(&rows[0], 1).unwrap(), Value::from("x"));
    }
}
