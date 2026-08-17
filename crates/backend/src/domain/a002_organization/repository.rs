use chrono::Utc;
use contracts::domain::a002_organization::aggregate::{Organization, OrganizationId};
use contracts::domain::common::{BaseAggregate, EntityMetadata};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use sea_orm::entity::prelude::*;

use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, Set};

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "a002_organization")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub code: String,
    pub description: String,
    pub comment: Option<String>,
    pub full_name: String,
    pub inn: String,
    pub kpp: String,
    /// Субъект учёта GL (san/sts/upr). Локальное поле — не из 1С.
    #[sea_orm(nullable)]
    pub entity_ref: Option<String>,
    pub is_deleted: bool,
    pub is_posted: bool,
    pub created_at: Option<chrono::DateTime<chrono::Utc>>,
    pub updated_at: Option<chrono::DateTime<chrono::Utc>>,
    pub version: i32,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}

impl From<Model> for Organization {
    fn from(m: Model) -> Self {
        let metadata = EntityMetadata {
            created_at: m.created_at.unwrap_or_else(Utc::now),
            updated_at: m.updated_at.unwrap_or_else(Utc::now),
            is_deleted: m.is_deleted,
            is_posted: m.is_posted,
            version: m.version,
        };
        let uuid = Uuid::parse_str(&m.id).unwrap_or_else(|_| Uuid::new_v4());

        Organization {
            base: BaseAggregate::with_metadata(
                OrganizationId(uuid),
                m.code,
                m.description,
                m.comment.clone(),
                metadata,
            ),
            full_name: m.full_name,
            inn: m.inn,
            kpp: m.kpp,
            entity_ref: m.entity_ref.filter(|value| !value.trim().is_empty()),
        }
    }
}

// Соединение приходит параметром: слой не должен знать, откуда берётся база.
// Пока часть вызывающих передаёт сюда глобальный мост `db::get_connection()` —
// это и есть граница, по которой миграция идёт дальше.
pub async fn list_all(db: &DatabaseConnection) -> anyhow::Result<Vec<Organization>> {
    let mut items: Vec<Organization> = Entity::find()
        .filter(Column::IsDeleted.eq(false))
        .all(db)
        .await?
        .into_iter()
        .map(Into::into)
        .collect();
    items.sort_by(|a, b| {
        a.base
            .description
            .to_lowercase()
            .cmp(&b.base.description.to_lowercase())
    });
    Ok(items)
}

pub async fn get_by_id(db: &DatabaseConnection, id: Uuid) -> anyhow::Result<Option<Organization>> {
    let result = Entity::find_by_id(id.to_string()).one(db).await?;
    Ok(result.map(Into::into))
}

pub async fn insert(db: &DatabaseConnection, aggregate: &Organization) -> anyhow::Result<Uuid> {
    let uuid = aggregate.base.id.value();
    let active = ActiveModel {
        id: Set(uuid.to_string()),
        code: Set(aggregate.base.code.clone()),
        description: Set(aggregate.base.description.clone()),
        comment: Set(aggregate.base.comment.clone()),
        full_name: Set(aggregate.full_name.clone()),
        inn: Set(aggregate.inn.clone()),
        kpp: Set(aggregate.kpp.clone()),
        entity_ref: Set(aggregate.entity_ref.clone()),
        is_deleted: Set(aggregate.base.metadata.is_deleted),
        is_posted: Set(aggregate.base.metadata.is_posted),
        created_at: Set(Some(aggregate.base.metadata.created_at)),
        updated_at: Set(Some(aggregate.base.metadata.updated_at)),
        version: Set(aggregate.base.metadata.version),
    };
    active.insert(db).await?;
    Ok(uuid)
}

pub async fn update(db: &DatabaseConnection, aggregate: &Organization) -> anyhow::Result<()> {
    let id = aggregate.base.id.value().to_string();
    let active = ActiveModel {
        id: Set(id),
        code: Set(aggregate.base.code.clone()),
        description: Set(aggregate.base.description.clone()),
        comment: Set(aggregate.base.comment.clone()),
        full_name: Set(aggregate.full_name.clone()),
        inn: Set(aggregate.inn.clone()),
        kpp: Set(aggregate.kpp.clone()),
        entity_ref: Set(aggregate.entity_ref.clone()),
        is_deleted: Set(aggregate.base.metadata.is_deleted),
        is_posted: Set(aggregate.base.metadata.is_posted),
        updated_at: Set(Some(aggregate.base.metadata.updated_at)),
        version: Set(aggregate.base.metadata.version),
        created_at: sea_orm::ActiveValue::NotSet,
    };
    active.update(db).await?;
    Ok(())
}

pub async fn soft_delete(db: &DatabaseConnection, id: Uuid) -> anyhow::Result<bool> {
    use sea_orm::sea_query::Expr;
    let result = Entity::update_many()
        .col_expr(Column::IsDeleted, Expr::value(true))
        .col_expr(Column::UpdatedAt, Expr::value(Utc::now()))
        .filter(Column::Id.eq(id.to_string()))
        .exec(db)
        .await?;
    Ok(result.rows_affected > 0)
}

pub async fn get_by_code(
    db: &DatabaseConnection,
    code: &str,
) -> anyhow::Result<Option<Organization>> {
    let result = Entity::find()
        .filter(Column::Code.eq(code))
        .filter(Column::IsDeleted.eq(false))
        .one(db)
        .await?;
    Ok(result.map(Into::into))
}

pub async fn get_by_description(
    db: &DatabaseConnection,
    description: &str,
) -> anyhow::Result<Option<Organization>> {
    let result = Entity::find()
        .filter(Column::Description.eq(description))
        .filter(Column::IsDeleted.eq(false))
        .one(db)
        .await?;
    Ok(result.map(Into::into))
}
