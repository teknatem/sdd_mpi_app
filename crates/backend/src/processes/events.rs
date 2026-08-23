//! Публикация и чтение доменных событий.
//!
//! Каталог событий — в `contracts::processes::event`; здесь только журнал
//! (`sys_domain_event`) и две операции над ним: опубликовать факт и найти
//! факты, до которых потребитель ещё не дошёл.
//!
//! Два решения, которые важно не потерять:
//!
//! - **Публикация валидирует ключ корреляции по каталогу.** Событие с неполным
//!   или «расширенным» ключом не пишется вовсе: токен, собранный не по правилу,
//!   не сведётся с ожиданием, и потеря будет молчаливой. Лучше отказ в момент
//!   публикации, где виден издатель.
//! - **Доставки нет.** Строка не помечается прочитанной, потому что читателей у
//!   факта может быть сколько угодно — от нуля до всех ожидающих экземпляров.
//!   Позицию держит потребитель (`seq`), а не журнал.

use anyhow::{anyhow, Result};
use chrono::Utc;
use contracts::processes::{CorrelationKey, DomainEvent, DomainEventKind};
use sea_orm::entity::prelude::*;
use sea_orm::{
    ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder, QuerySelect, Set,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "sys_domain_event")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub seq: i64,
    pub id: String,
    pub kind: String,
    pub correlation_json: String,
    pub correlation_token: String,
    pub payload_json: String,
    pub source: String,
    pub published_at: String,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}

impl Model {
    /// Разобрать строку журнала.
    ///
    /// Имя не из каталога — порча данных, а не «новое событие»: каталог закрыт
    /// (ADR-0011 п.5), и молча пропустить такую строку значило бы отдать
    /// решение о том, что является фактом, тому, кто испортил таблицу.
    fn into_event(self) -> Result<DomainEvent> {
        let kind = DomainEventKind::parse(&self.kind)
            .ok_or_else(|| anyhow!("в журнале событие не из каталога: '{}'", self.kind))?;
        Ok(DomainEvent {
            id: self.id,
            seq: self.seq,
            kind,
            correlation: serde_json::from_str(&self.correlation_json).unwrap_or_default(),
            correlation_token: self.correlation_token,
            payload: serde_json::from_str(&self.payload_json).unwrap_or(Value::Null),
            source: self.source,
            published_at: self.published_at,
        })
    }
}

/// Опубликовать факт.
///
/// Точки публикации расставляются вручную в use-case'ах и сервисах (ADR-0011
/// п.5): универсальный поток «агрегат изменён» отклонён, потому что смысл
/// пришлось бы вычислять подписчику.
pub async fn publish(
    db: &DatabaseConnection,
    kind: DomainEventKind,
    correlation: CorrelationKey,
    payload: Value,
    source: &str,
) -> Result<DomainEvent> {
    let token = correlation
        .token(kind)
        .map_err(|problem| anyhow!(problem))?;
    let row = ActiveModel {
        seq: sea_orm::ActiveValue::NotSet,
        id: Set(Uuid::new_v4().to_string()),
        kind: Set(kind.as_str().to_string()),
        correlation_json: Set(serde_json::to_string(&correlation)?),
        correlation_token: Set(token),
        payload_json: Set(payload.to_string()),
        source: Set(source.to_string()),
        published_at: Set(Utc::now().to_rfc3339()),
    };
    row.insert(db).await?.into_event()
}

/// События после указанного номера — курсор потребителя.
///
/// `after` — номер последнего разобранного события, а не «сколько пропустить»:
/// потребитель хранит позицию, переживающую перезапуск.
pub async fn list_since(
    db: &DatabaseConnection,
    after: i64,
    limit: u64,
) -> Result<Vec<DomainEvent>> {
    Entity::find()
        .filter(Column::Seq.gt(after))
        .order_by_asc(Column::Seq)
        .limit(limit)
        .all(db)
        .await?
        .into_iter()
        .map(Model::into_event)
        .collect()
}

/// События нужного вида с нужным ключом — то, чем просыпается ожидающий
/// экземпляр (ADR-0011 п.9).
pub async fn find_matching(
    db: &DatabaseConnection,
    kind: DomainEventKind,
    correlation_token: &str,
    after: i64,
    limit: u64,
) -> Result<Vec<DomainEvent>> {
    Entity::find()
        .filter(Column::Kind.eq(kind.as_str()))
        .filter(Column::CorrelationToken.eq(correlation_token))
        .filter(Column::Seq.gt(after))
        .order_by_asc(Column::Seq)
        .limit(limit)
        .all(db)
        .await?
        .into_iter()
        .map(Model::into_event)
        .collect()
}

/// Номер последнего опубликованного события: с него начинает потребитель,
/// которому прошлое не нужно.
pub async fn last_seq(db: &DatabaseConnection) -> Result<i64> {
    Ok(Entity::find()
        .order_by_desc(Column::Seq)
        .one(db)
        .await?
        .map(|row| row.seq)
        .unwrap_or(0))
}

/// Последние события — для экрана разбора.
pub async fn list_recent(db: &DatabaseConnection, limit: u64) -> Result<Vec<DomainEvent>> {
    Entity::find()
        .order_by_desc(Column::Seq)
        .limit(limit)
        .all(db)
        .await?
        .into_iter()
        .map(Model::into_event)
        .collect()
}
