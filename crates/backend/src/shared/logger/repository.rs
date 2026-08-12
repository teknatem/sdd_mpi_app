use chrono::Utc;
use contracts::shared::logger::LogEntry;
use sea_orm::entity::prelude::*;
use sea_orm::{EntityTrait, QueryOrder, Set};

use crate::shared::data::db::get_connection;

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "system_log")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub timestamp: String,
    pub source: String,
    pub category: String,
    pub message: String,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}

impl From<Model> for LogEntry {
    fn from(m: Model) -> Self {
        LogEntry {
            id: m.id,
            timestamp: m.timestamp,
            source: m.source,
            category: m.category,
            message: m.message,
        }
    }
}

fn conn() -> &'static DatabaseConnection {
    get_connection()
}

/// Добавить запись в лог (внутренняя функция)
pub fn log_event_internal(source: &str, category: &str, message: &str) {
    let source = source.to_string();
    let category = category.to_string();
    let message = message.to_string();

    tokio::spawn(async move {
        let Some(_database_activity) = crate::system::maintenance::try_begin_database_activity()
        else {
            return;
        };
        if let Err(e) = log_event(&source, &category, &message).await {
            eprintln!("Failed to log event: {}", e);
        }
    });
}

/// Добавить запись в лог
pub async fn log_event(source: &str, category: &str, message: &str) -> anyhow::Result<()> {
    let now = Utc::now().format("%Y-%m-%d %H:%M:%S%.3f").to_string();

    let active = ActiveModel {
        id: sea_orm::ActiveValue::NotSet,
        timestamp: Set(now),
        source: Set(source.to_string()),
        category: Set(category.to_string()),
        message: Set(message.to_string()),
    };

    active.insert(conn()).await?;
    Ok(())
}

/// Получить все записи лога (сортировка по времени, новые сверху)
pub async fn get_all_logs() -> anyhow::Result<Vec<LogEntry>> {
    let logs: Vec<LogEntry> = Entity::find()
        .order_by_desc(Column::Id)
        .all(conn())
        .await?
        .into_iter()
        .map(Into::into)
        .collect();
    Ok(logs)
}

/// Очистить все записи лога
pub async fn clear_all_logs() -> anyhow::Result<()> {
    Entity::delete_many().exec(conn()).await?;
    Ok(())
}
