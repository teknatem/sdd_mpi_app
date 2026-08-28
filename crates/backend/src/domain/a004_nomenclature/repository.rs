use chrono::Utc;
use contracts::domain::a004_nomenclature::aggregate::{Nomenclature, NomenclatureId};
use contracts::domain::common::{BaseAggregate, EntityMetadata};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use sea_orm::entity::prelude::*;
use sea_orm::Condition;
use sea_orm::{
    ColumnTrait, ConnectionTrait, EntityTrait, FromQueryResult, QueryFilter, QueryOrder,
    QuerySelect, Set, Statement, Value,
};

use crate::shared::data::db::get_connection;

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "a004_nomenclature")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub code: String,
    pub description: String,
    pub full_description: String,
    pub comment: Option<String>,
    pub is_folder: bool,
    pub parent_id: Option<String>,
    pub article: String,
    pub mp_ref_count: i32,
    // Измерения (классификация)
    pub dim1_category: String,
    pub dim2_line: String,
    pub dim3_model: String,
    pub dim4_format: String,
    pub dim5_sink: String,
    pub dim6_size: String,
    pub is_assembly: bool,
    pub base_nomenclature_ref: Option<String>,
    pub alternative_cost_source_ref: Option<String>,
    pub kit_variant_ref: Option<String>,
    pub is_derivative: bool,
    pub is_deleted: bool,
    pub is_posted: bool,
    pub created_at: Option<chrono::DateTime<chrono::Utc>>,
    pub updated_at: Option<chrono::DateTime<chrono::Utc>>,
    pub version: i32,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}

impl From<Model> for Nomenclature {
    fn from(m: Model) -> Self {
        let metadata = EntityMetadata {
            created_at: m.created_at.unwrap_or_else(Utc::now),
            updated_at: m.updated_at.unwrap_or_else(Utc::now),
            is_deleted: m.is_deleted,
            is_posted: m.is_posted,
            version: m.version,
        };
        let uuid = Uuid::parse_str(&m.id).unwrap_or_else(|_| Uuid::new_v4());

        Nomenclature {
            base: BaseAggregate::with_metadata(
                NomenclatureId(uuid),
                m.code,
                m.description,
                m.comment.clone(),
                metadata,
            ),
            full_description: m.full_description,
            is_folder: m.is_folder,
            parent_id: m.parent_id,
            article: m.article,
            mp_ref_count: m.mp_ref_count,
            dim1_category: m.dim1_category,
            dim2_line: m.dim2_line,
            dim3_model: m.dim3_model,
            dim4_format: m.dim4_format,
            dim5_sink: m.dim5_sink,
            dim6_size: m.dim6_size,
            is_assembly: m.is_assembly,
            base_nomenclature_ref: m.base_nomenclature_ref,
            alternative_cost_source_ref: m.alternative_cost_source_ref,
            kit_variant_ref: m.kit_variant_ref,
            is_derivative: m.is_derivative,
        }
    }
}

fn conn() -> &'static DatabaseConnection {
    get_connection()
}

pub async fn list_all() -> anyhow::Result<Vec<Nomenclature>> {
    let mut items: Vec<Nomenclature> = Entity::find()
        .all(conn())
        .await?
        .into_iter()
        .map(Into::into)
        .collect();
    // Sort: folders first, then by description (case-insensitive)
    items.sort_by(|a, b| match (a.is_folder, b.is_folder) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => a
            .base
            .description
            .to_lowercase()
            .cmp(&b.base.description.to_lowercase()),
    });
    Ok(items)
}

pub async fn list_paginated(
    limit: u64,
    offset: u64,
    sort_by: &str,
    sort_desc: bool,
    q: &str,
    only_mp: bool,
    no_analytics: bool,
) -> anyhow::Result<(Vec<Nomenclature>, u64)> {
    let mut condition = Condition::all()
        .add(Column::IsDeleted.eq(false))
        .add(Column::IsFolder.eq(false));

    if only_mp {
        condition = condition.add(Column::MpRefCount.gt(0));
    }

    if no_analytics {
        let missing_dimension = Condition::any()
            .add(Column::Dim1Category.eq(""))
            .add(Column::Dim2Line.eq(""))
            .add(Column::Dim3Model.eq(""))
            .add(Column::Dim4Format.eq(""))
            .add(Column::Dim5Sink.eq(""))
            .add(Column::Dim6Size.eq(""));
        condition = condition.add(missing_dimension);
    }

    let q_trimmed = q.trim();
    if q_trimmed.len() >= 3 {
        let or = Condition::any()
            .add(Column::Article.contains(q_trimmed))
            .add(Column::Description.contains(q_trimmed))
            .add(Column::Dim1Category.contains(q_trimmed))
            .add(Column::Dim2Line.contains(q_trimmed))
            .add(Column::Dim3Model.contains(q_trimmed))
            .add(Column::Dim4Format.contains(q_trimmed))
            .add(Column::Dim5Sink.contains(q_trimmed))
            .add(Column::Dim6Size.contains(q_trimmed));
        condition = condition.add(or);
    }

    let total: u64 = Entity::find()
        .filter(condition.clone())
        .count(conn())
        .await?;

    let sort_column = match sort_by {
        "article" => Column::Article,
        "description" => Column::Description,
        "dim1_category" => Column::Dim1Category,
        "dim2_line" => Column::Dim2Line,
        "dim3_model" => Column::Dim3Model,
        "dim4_format" => Column::Dim4Format,
        "dim5_sink" => Column::Dim5Sink,
        "dim6_size" => Column::Dim6Size,
        "mp_ref_count" => Column::MpRefCount,
        _ => Column::Article,
    };

    let mut q_items = Entity::find().filter(condition);
    q_items = if sort_desc {
        q_items.order_by_desc(sort_column)
    } else {
        q_items.order_by_asc(sort_column)
    };

    // Secondary sort for stability
    q_items = q_items.order_by_asc(Column::Article);

    let items: Vec<Nomenclature> = q_items
        .limit(limit)
        .offset(offset)
        .all(conn())
        .await?
        .into_iter()
        .map(Into::into)
        .collect();

    Ok((items, total))
}

pub async fn get_by_id(id: Uuid) -> anyhow::Result<Option<Nomenclature>> {
    let result = Entity::find_by_id(id.to_string()).one(conn()).await?;
    Ok(result.map(Into::into))
}

pub async fn list_by_ids(ids: &[String]) -> anyhow::Result<Vec<Nomenclature>> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }

    let items = Entity::find()
        .filter(Column::Id.is_in(ids.iter().cloned()))
        .all(conn())
        .await?
        .into_iter()
        .map(Into::into)
        .collect();
    Ok(items)
}

pub async fn insert(aggregate: &Nomenclature) -> anyhow::Result<Uuid> {
    let uuid = aggregate.base.id.value();
    let active = ActiveModel {
        id: Set(uuid.to_string()),
        code: Set(aggregate.base.code.clone()),
        description: Set(aggregate.base.description.clone()),
        full_description: Set(aggregate.full_description.clone()),
        comment: Set(aggregate.base.comment.clone()),
        is_folder: Set(aggregate.is_folder),
        parent_id: Set(aggregate.parent_id.clone()),
        article: Set(aggregate.article.clone()),
        mp_ref_count: Set(aggregate.mp_ref_count),
        dim1_category: Set(aggregate.dim1_category.clone()),
        dim2_line: Set(aggregate.dim2_line.clone()),
        dim3_model: Set(aggregate.dim3_model.clone()),
        dim4_format: Set(aggregate.dim4_format.clone()),
        dim5_sink: Set(aggregate.dim5_sink.clone()),
        dim6_size: Set(aggregate.dim6_size.clone()),
        is_assembly: Set(aggregate.is_assembly),
        base_nomenclature_ref: Set(aggregate.base_nomenclature_ref.clone()),
        alternative_cost_source_ref: Set(aggregate.alternative_cost_source_ref.clone()),
        kit_variant_ref: Set(aggregate.kit_variant_ref.clone()),
        is_derivative: Set(aggregate.is_derivative),
        is_deleted: Set(aggregate.base.metadata.is_deleted),
        is_posted: Set(aggregate.base.metadata.is_posted),
        created_at: Set(Some(aggregate.base.metadata.created_at)),
        updated_at: Set(Some(aggregate.base.metadata.updated_at)),
        version: Set(aggregate.base.metadata.version),
    };
    active.insert(conn()).await?;
    Ok(uuid)
}

pub async fn update(aggregate: &Nomenclature) -> anyhow::Result<()> {
    let id = aggregate.base.id.value().to_string();
    let active = ActiveModel {
        id: Set(id),
        code: Set(aggregate.base.code.clone()),
        description: Set(aggregate.base.description.clone()),
        full_description: Set(aggregate.full_description.clone()),
        comment: Set(aggregate.base.comment.clone()),
        is_folder: Set(aggregate.is_folder),
        parent_id: Set(aggregate.parent_id.clone()),
        article: Set(aggregate.article.clone()),
        mp_ref_count: Set(aggregate.mp_ref_count),
        dim1_category: Set(aggregate.dim1_category.clone()),
        dim2_line: Set(aggregate.dim2_line.clone()),
        dim3_model: Set(aggregate.dim3_model.clone()),
        dim4_format: Set(aggregate.dim4_format.clone()),
        dim5_sink: Set(aggregate.dim5_sink.clone()),
        dim6_size: Set(aggregate.dim6_size.clone()),
        is_assembly: Set(aggregate.is_assembly),
        base_nomenclature_ref: Set(aggregate.base_nomenclature_ref.clone()),
        alternative_cost_source_ref: Set(aggregate.alternative_cost_source_ref.clone()),
        kit_variant_ref: Set(aggregate.kit_variant_ref.clone()),
        is_derivative: Set(aggregate.is_derivative),
        is_deleted: Set(aggregate.base.metadata.is_deleted),
        is_posted: Set(aggregate.base.metadata.is_posted),
        updated_at: Set(Some(aggregate.base.metadata.updated_at)),
        version: Set(aggregate.base.metadata.version),
        created_at: sea_orm::ActiveValue::NotSet,
    };
    active.update(conn()).await?;
    Ok(())
}

pub async fn soft_delete(id: Uuid) -> anyhow::Result<bool> {
    use sea_orm::sea_query::Expr;
    let result = Entity::update_many()
        .col_expr(Column::IsDeleted, Expr::value(true))
        .col_expr(Column::UpdatedAt, Expr::value(Utc::now()))
        .filter(Column::Id.eq(id.to_string()))
        .exec(conn())
        .await?;
    Ok(result.rows_affected > 0)
}

/// Найти номенклатуру по артикулу
/// Возвращает только элементы (не папки) и не удаленные
/// ВАЖНО: article должен быть уже trimmed
pub async fn find_by_article(article: &str) -> anyhow::Result<Vec<Nomenclature>> {
    // IMPORTANT: don't load the whole table. Use SQL TRIM() so trailing spaces from 1C won't break matching.
    use sea_orm::sea_query::Expr;

    find_by_article_with_conn(
        conn(),
        article,
        Expr::cust_with_values("trim(article) = ?", [article]),
    )
    .await
}

async fn find_by_article_with_conn<C: ConnectionTrait>(
    db: &C,
    _article: &str,
    expr: sea_orm::sea_query::SimpleExpr,
) -> anyhow::Result<Vec<Nomenclature>> {
    let items: Vec<Nomenclature> = Entity::find()
        .filter(Column::IsFolder.eq(false))
        .filter(Column::IsDeleted.eq(false))
        .filter(expr)
        .all(db)
        .await?
        .into_iter()
        .map(Into::into)
        .collect();

    Ok(items)
}

/// Найти номенклатуру по артикулу (без учета регистра)
/// Возвращает только элементы (не папки) и не удаленные
/// ВАЖНО: article должен быть уже trimmed
///
/// Примечание: SQLite lower() работает только с ASCII.
/// Для кириллических артикулов (например "515.1-1.4.1.Р") используем
/// комбинированный OR: точное совпадение (trim) + ASCII case-insensitive (lower).
pub async fn find_by_article_ignore_case(article: &str) -> anyhow::Result<Vec<Nomenclature>> {
    let article_trimmed = article.trim();
    let article_lower = article_trimmed.to_lowercase();

    use sea_orm::sea_query::Expr;

    find_by_article_with_conn(
        conn(),
        article_trimmed,
        // trim(article) = ? — точное совпадение (покрывает Unicode/кириллицу)
        // lower(trim(article)) = ? — ASCII case-insensitive (покрывает латинские артикулы)
        Expr::cust_with_values(
            "trim(article) = ? OR lower(trim(article)) = ?",
            [article_trimmed.to_string(), article_lower],
        ),
    )
    .await
}

pub async fn find_by_article_txn<C: ConnectionTrait>(
    db: &C,
    article: &str,
) -> anyhow::Result<Vec<Nomenclature>> {
    use sea_orm::sea_query::Expr;
    find_by_article_with_conn(
        db,
        article,
        Expr::cust_with_values("trim(article) = ?", [article]),
    )
    .await
}

pub async fn update_txn<C: ConnectionTrait>(
    db: &C,
    aggregate: &Nomenclature,
) -> anyhow::Result<()> {
    let id = aggregate.base.id.value().to_string();
    let active = ActiveModel {
        id: Set(id),
        code: Set(aggregate.base.code.clone()),
        description: Set(aggregate.base.description.clone()),
        full_description: Set(aggregate.full_description.clone()),
        comment: Set(aggregate.base.comment.clone()),
        is_folder: Set(aggregate.is_folder),
        parent_id: Set(aggregate.parent_id.clone()),
        article: Set(aggregate.article.clone()),
        mp_ref_count: Set(aggregate.mp_ref_count),
        dim1_category: Set(aggregate.dim1_category.clone()),
        dim2_line: Set(aggregate.dim2_line.clone()),
        dim3_model: Set(aggregate.dim3_model.clone()),
        dim4_format: Set(aggregate.dim4_format.clone()),
        dim5_sink: Set(aggregate.dim5_sink.clone()),
        dim6_size: Set(aggregate.dim6_size.clone()),
        is_assembly: Set(aggregate.is_assembly),
        base_nomenclature_ref: Set(aggregate.base_nomenclature_ref.clone()),
        alternative_cost_source_ref: Set(aggregate.alternative_cost_source_ref.clone()),
        kit_variant_ref: Set(aggregate.kit_variant_ref.clone()),
        is_derivative: Set(aggregate.is_derivative),
        is_deleted: Set(aggregate.base.metadata.is_deleted),
        is_posted: Set(aggregate.base.metadata.is_posted),
        updated_at: Set(Some(aggregate.base.metadata.updated_at)),
        version: Set(aggregate.base.metadata.version),
        created_at: sea_orm::ActiveValue::NotSet,
    };
    active.update(db).await?;
    Ok(())
}

/// Обновить счетчик ссылок на маркетплейс для номенклатуры
pub async fn update_mp_ref_count(nomenclature_id: Uuid, count: i32) -> anyhow::Result<()> {
    use sea_orm::sea_query::Expr;
    Entity::update_many()
        .col_expr(Column::MpRefCount, Expr::value(count))
        .col_expr(Column::UpdatedAt, Expr::value(Utc::now()))
        .filter(Column::Id.eq(nomenclature_id.to_string()))
        .exec(conn())
        .await?;
    Ok(())
}

pub async fn update_kit_variant_ref(
    nomenclature_id: Uuid,
    kit_variant_ref: Option<String>,
) -> anyhow::Result<()> {
    use sea_orm::sea_query::Expr;
    Entity::update_many()
        .col_expr(Column::KitVariantRef, Expr::value(kit_variant_ref))
        .col_expr(Column::UpdatedAt, Expr::value(Utc::now()))
        .filter(Column::Id.eq(nomenclature_id.to_string()))
        .exec(conn())
        .await?;
    Ok(())
}

/// Структура для возврата списка уникальных значений измерений
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DimensionValues {
    pub dim1_category: Vec<String>,
    pub dim2_line: Vec<String>,
    pub dim3_model: Vec<String>,
    pub dim4_format: Vec<String>,
    pub dim5_sink: Vec<String>,
    pub dim6_size: Vec<String>,
}

/// Получить все уникальные значения измерений
pub async fn get_distinct_dimension_values() -> anyhow::Result<DimensionValues> {
    use std::collections::BTreeSet;

    // Получаем все записи
    let all_items: Vec<Model> = Entity::find()
        .filter(Column::IsDeleted.eq(false))
        .all(conn())
        .await?;

    // Используем BTreeSet для автоматической сортировки и уникальности
    let mut dim1_set = BTreeSet::new();
    let mut dim2_set = BTreeSet::new();
    let mut dim3_set = BTreeSet::new();
    let mut dim4_set = BTreeSet::new();
    let mut dim5_set = BTreeSet::new();
    let mut dim6_set = BTreeSet::new();

    for item in all_items {
        // Добавляем только непустые значения (после trim)
        let dim1 = item.dim1_category.trim();
        if !dim1.is_empty() {
            dim1_set.insert(dim1.to_string());
        }

        let dim2 = item.dim2_line.trim();
        if !dim2.is_empty() {
            dim2_set.insert(dim2.to_string());
        }

        let dim3 = item.dim3_model.trim();
        if !dim3.is_empty() {
            dim3_set.insert(dim3.to_string());
        }

        let dim4 = item.dim4_format.trim();
        if !dim4.is_empty() {
            dim4_set.insert(dim4.to_string());
        }

        let dim5 = item.dim5_sink.trim();
        if !dim5.is_empty() {
            dim5_set.insert(dim5.to_string());
        }

        let dim6 = item.dim6_size.trim();
        if !dim6.is_empty() {
            dim6_set.insert(dim6.to_string());
        }
    }

    Ok(DimensionValues {
        dim1_category: dim1_set.into_iter().collect(),
        dim2_line: dim2_set.into_iter().collect(),
        dim3_model: dim3_set.into_iter().collect(),
        dim4_format: dim4_set.into_iter().collect(),
        dim5_sink: dim5_set.into_iter().collect(),
        dim6_size: dim6_set.into_iter().collect(),
    })
}

/// Удалить записи по списку ID (жесткое удаление)
pub async fn delete_by_ids(ids: Vec<Uuid>) -> anyhow::Result<u64> {
    if ids.is_empty() {
        return Ok(0);
    }

    let id_strings: Vec<String> = ids.iter().map(|id| id.to_string()).collect();

    let result = Entity::delete_many()
        .filter(Column::Id.is_in(id_strings))
        .exec(conn())
        .await?;

    Ok(result.rows_affected)
}

// ============================================================================
// External BI: nomenclature catalog + marketplace SKU bridge
// ============================================================================

const ZERO_UUID: &str = "00000000-0000-0000-0000-000000000000";

/// Строка справочника 1С для Power BI: одна номенклатура, без разворота по МП.
#[derive(Debug, Clone, FromQueryResult)]
pub struct BiNomenclatureRow {
    pub id: String,
    pub name: String,
    pub article: String,
    pub category: String,
    pub line: String,
    /// dim3: в системе поле «Модель»; в BI отдаём как цвет/исполнение.
    pub color: String,
    pub format: String,
    pub sink: String,
    pub size: String,
    pub is_assembly: bool,
    pub dealer_price: Option<f64>,
}

pub struct BiNomenclatureResult {
    pub rows: Vec<BiNomenclatureRow>,
    pub total: usize,
}

/// Связка номенклатуры 1С с кодом товара на маркетплейсе.
/// Одна строка — один SKU; дубли кабинетов схлопнуты.
#[derive(Debug, Clone)]
pub struct BiNomenclatureSkuRow {
    pub nomenclature_id: String,
    /// Короткий код площадки: `WB`, `YM`, `OZON`, …
    pub marketplace: String,
    /// `marketplace_sku` из a007: для WB это `nm_id`, для YM — `shop_sku`.
    pub sku: String,
    /// `nm_id` как число, если `marketplace = WB` и `sku` парсится. Для связи
    /// с воронкой/остатками, где ключ — integer, а не текст.
    pub wb_nm_id: Option<i64>,
}

pub struct BiNomenclatureSkuResult {
    pub rows: Vec<BiNomenclatureSkuRow>,
    pub total: usize,
}

#[derive(Debug, FromQueryResult)]
struct BiSkuSqlRow {
    nomenclature_id: String,
    marketplace_type: Option<String>,
    marketplace_code: Option<String>,
    sku: String,
}

#[derive(Debug, FromQueryResult)]
struct CountRow {
    count: i64,
}

fn marketplace_label(marketplace_type: Option<&str>, marketplace_code: Option<&str>) -> String {
    match marketplace_type.unwrap_or("") {
        "mp-wb" => "WB".to_string(),
        "mp-ym" => "YM".to_string(),
        "mp-ozon" => "OZON".to_string(),
        "mp-kuper" => "KUPER".to_string(),
        "mp-lemana" => "LEMANA".to_string(),
        other if !other.is_empty() => other.to_string(),
        _ => marketplace_code.unwrap_or("").trim().to_uppercase(),
    }
}

fn parse_wb_nm_id(marketplace: &str, sku: &str) -> Option<i64> {
    if marketplace != "WB" {
        return None;
    }
    sku.parse::<i64>().ok().filter(|id| *id > 0)
}

const MATCHED_NOM_SQL: &str = "
    n.is_deleted = 0
    AND n.is_folder = 0
    AND EXISTS (
        SELECT 1
        FROM a007_marketplace_product p
        WHERE p.nomenclature_ref = n.id
          AND p.is_deleted = 0
          AND p.marketplace_sku IS NOT NULL
          AND TRIM(p.marketplace_sku) != ''
    )";

/// Каталог номенклатуры 1С, у которой есть хотя бы одно сопоставление a007.
/// Дилерская цена — последняя ненулевая `p906` на дату `as_of` (включительно),
/// с запасным путём через `base_nomenclature_ref`.
pub async fn bi_nomenclature_rows(
    as_of: &str,
    limit: usize,
    offset: usize,
) -> anyhow::Result<BiNomenclatureResult> {
    let db = conn();

    let count_sql = format!(
        "SELECT COUNT(*) AS count FROM a004_nomenclature n WHERE {}",
        MATCHED_NOM_SQL
    );
    let total = CountRow::find_by_statement(Statement::from_string(
        db.get_database_backend(),
        count_sql,
    ))
    .one(db)
    .await?
    .map(|r| r.count as usize)
    .unwrap_or(0);

    let list_sql = format!(
        "WITH latest_price AS (
            SELECT p.nomenclature_ref, p.price
            FROM p906_nomenclature_prices p
            INNER JOIN (
                SELECT nomenclature_ref, MAX(period) AS max_period
                FROM p906_nomenclature_prices
                WHERE price > 0 AND period <= ?
                GROUP BY nomenclature_ref
            ) t
              ON t.nomenclature_ref = p.nomenclature_ref
             AND t.max_period = p.period
            WHERE p.price > 0
        )
        SELECT
            n.id,
            n.description AS name,
            n.article,
            n.dim1_category AS category,
            n.dim2_line AS line,
            n.dim3_model AS color,
            n.dim4_format AS format,
            n.dim5_sink AS sink,
            n.dim6_size AS size,
            n.is_assembly,
            COALESCE(own.price, base.price) AS dealer_price
        FROM a004_nomenclature n
        LEFT JOIN latest_price own ON own.nomenclature_ref = n.id
        LEFT JOIN latest_price base
          ON base.nomenclature_ref = n.base_nomenclature_ref
         AND n.base_nomenclature_ref IS NOT NULL
         AND n.base_nomenclature_ref != ''
         AND n.base_nomenclature_ref != ?
        WHERE {}
        ORDER BY n.article COLLATE NOCASE, n.description COLLATE NOCASE
        LIMIT ? OFFSET ?",
        MATCHED_NOM_SQL
    );

    let rows = BiNomenclatureRow::find_by_statement(Statement::from_sql_and_values(
        db.get_database_backend(),
        list_sql,
        [
            Value::from(as_of.to_string()),
            Value::from(ZERO_UUID.to_string()),
            Value::from(limit as i64),
            Value::from(offset as i64),
        ],
    ))
    .all(db)
    .await?;

    Ok(BiNomenclatureResult { rows, total })
}

/// Мост «номенклатура 1С ↔ SKU маркетплейса» для связей в Power BI.
pub async fn bi_nomenclature_sku_rows(
    limit: usize,
    offset: usize,
) -> anyhow::Result<BiNomenclatureSkuResult> {
    let db = conn();

    let from_sql = format!(
        "FROM (
            SELECT DISTINCT
                p.nomenclature_ref AS nomenclature_id,
                m.marketplace_type AS marketplace_type,
                m.code AS marketplace_code,
                TRIM(p.marketplace_sku) AS sku
            FROM a007_marketplace_product p
            INNER JOIN a004_nomenclature n ON n.id = p.nomenclature_ref
            LEFT JOIN a005_marketplace m ON m.id = p.marketplace_ref
            WHERE p.is_deleted = 0
              AND {}
              AND p.marketplace_sku IS NOT NULL
              AND TRIM(p.marketplace_sku) != ''
        ) x",
        MATCHED_NOM_SQL
    );

    let count_sql = format!("SELECT COUNT(*) AS count {}", from_sql);
    let total = CountRow::find_by_statement(Statement::from_string(
        db.get_database_backend(),
        count_sql,
    ))
    .one(db)
    .await?
    .map(|r| r.count as usize)
    .unwrap_or(0);

    let list_sql = format!(
        "SELECT nomenclature_id, marketplace_type, marketplace_code, sku
         {}
         ORDER BY nomenclature_id, marketplace_type, sku
         LIMIT ? OFFSET ?",
        from_sql
    );

    let sql_rows = BiSkuSqlRow::find_by_statement(Statement::from_sql_and_values(
        db.get_database_backend(),
        list_sql,
        [Value::from(limit as i64), Value::from(offset as i64)],
    ))
    .all(db)
    .await?;

    let rows = sql_rows
        .into_iter()
        .map(|r| {
            let marketplace = marketplace_label(
                r.marketplace_type.as_deref(),
                r.marketplace_code.as_deref(),
            );
            let wb_nm_id = parse_wb_nm_id(&marketplace, &r.sku);
            BiNomenclatureSkuRow {
                nomenclature_id: r.nomenclature_id,
                marketplace,
                sku: r.sku,
                wb_nm_id,
            }
        })
        .collect();

    Ok(BiNomenclatureSkuResult { rows, total })
}

#[cfg(test)]
mod tests {
    use super::{marketplace_label, parse_wb_nm_id};

    #[test]
    fn marketplace_label_maps_known_types() {
        assert_eq!(marketplace_label(Some("mp-wb"), Some("wb")), "WB");
        assert_eq!(marketplace_label(Some("mp-ym"), None), "YM");
        assert_eq!(marketplace_label(None, Some("Ozon")), "OZON");
    }

    #[test]
    fn wb_nm_id_only_for_numeric_wb_sku() {
        assert_eq!(parse_wb_nm_id("WB", "12345678"), Some(12345678));
        assert_eq!(parse_wb_nm_id("WB", "abc"), None);
        assert_eq!(parse_wb_nm_id("YM", "12345678"), None);
    }
}
