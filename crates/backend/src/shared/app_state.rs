//! Состояние приложения, прокинутое через `Router::with_state`.
//!
//! **Зачем.** До этого соединение с базой было глобальным синглтоном
//! (`static DB_CONN: OnceCell` в `shared/data/db.rs`), а `get_connection()`
//! звался из 166 файлов. Следствие не стилистическое, а структурное: открыть
//! *вторую* базу в рамках процесса было нельзя, поэтому интеграционных тестов
//! в проекте было ровно ноль — не по недосмотру, а потому что негде взять
//! тестовую БД.
//!
//! **Как мигрировать.** `get_connection()` намеренно оставлен рабочим мостом:
//! перевод 166 файлов одним коммитом — не миграция, а лотерея. Правило для
//! нового и правимого кода:
//!
//! ```ignore
//! pub async fn list_all(State(state): State<AppState>) -> ApiResult<Json<Vec<Foo>>> {
//!     let items = repository::list_all(state.db()).await?;
//!     Ok(Json(items))
//! }
//! ```
//!
//! то есть хендлер берёт `State<AppState>`, а слои ниже (`service` →
//! `repository`) принимают `&DatabaseConnection` параметром, а не достают
//! соединение сами. Срез считается мигрированным, когда `get_connection()` не
//! встречается в нём ни разу.

use sea_orm::DatabaseConnection;

/// Разделяемое состояние HTTP-слоя.
///
/// Клонируется на каждый запрос, поэтому все поля обязаны быть дёшево
/// клонируемыми: `DatabaseConnection` — это `Arc` над пулом sqlx.
#[derive(Clone, Debug)]
pub struct AppState {
    db: DatabaseConnection,
}

impl AppState {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }

    /// Соединение с базой этого экземпляра приложения.
    pub fn db(&self) -> &DatabaseConnection {
        &self.db
    }
}

/// Позволяет хендлеру просить сразу `State<DatabaseConnection>`, не зная про
/// `AppState`. Нужно ровно затем, чтобы добавление поля в состояние не трогало
/// сигнатуры тех хендлеров, которым нужна только база.
impl axum::extract::FromRef<AppState> for DatabaseConnection {
    fn from_ref(state: &AppState) -> Self {
        state.db.clone()
    }
}
