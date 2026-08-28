//! SeaORM-доступ к таблице `plugin`. Бандл хранится JSON-колонками (как у a024).

use chrono::Utc;
use contracts::plugins::{
    DataBinding, ParamSpec, PluginBundle, PluginDefinition, PluginManifest, PluginRuntime,
    PluginStatus, ViewSpec,
};
use sea_orm::entity::prelude::*;
use sea_orm::prelude::Expr;
use sea_orm::{ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, QueryOrder, Set};
use std::collections::HashMap;

const RESOURCE_STORAGE_FORMAT: &str = "plugin_bundle_resources_v1";

#[derive(Default, serde::Serialize, serde::Deserialize)]
struct StoredResources {
    #[serde(default)]
    format: String,
    #[serde(default)]
    assets: HashMap<String, String>,
    #[serde(default)]
    sql_resources: HashMap<String, String>,
}

fn decode_resources(json: &str) -> (HashMap<String, String>, HashMap<String, String>) {
    let value = serde_json::from_str::<serde_json::Value>(json).unwrap_or_default();
    if value.get("format").and_then(serde_json::Value::as_str) == Some(RESOURCE_STORAGE_FORMAT) {
        let stored = serde_json::from_value::<StoredResources>(value).unwrap_or_default();
        return (stored.assets, stored.sql_resources);
    }

    // Before SQL resources were introduced, assets_json contained the asset map directly.
    (
        serde_json::from_str::<HashMap<String, String>>(json).unwrap_or_default(),
        HashMap::new(),
    )
}

fn encode_resources(bundle: &PluginBundle) -> String {
    serde_json::to_string(&StoredResources {
        format: RESOURCE_STORAGE_FORMAT.to_string(),
        assets: bundle.assets.clone(),
        sql_resources: bundle.sql_resources.clone(),
    })
    .unwrap_or_else(|_| "{}".into())
}

mod plugin {
    use sea_orm::entity::prelude::*;
    use serde::{Deserialize, Serialize};

    #[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
    #[sea_orm(table_name = "plugin")]
    pub struct Model {
        #[sea_orm(primary_key, auto_increment = false)]
        pub id: String,
        pub code: String,
        pub title: String,
        pub runtime: String,
        pub status: String,
        pub manifest_json: String,
        pub params_json: String,
        pub data_json: String,
        pub client_script: Option<String>,
        pub server_script: Option<String>,
        pub view_spec_json: String,
        pub styles: Option<String>,
        pub assets_json: String,
        pub owner_user_id: Option<String>,
        pub created_by_agent_id: Option<String>,
        pub is_enabled: bool,
        pub is_deleted: bool,
        pub created_at: Option<chrono::DateTime<chrono::Utc>>,
        pub updated_at: Option<chrono::DateTime<chrono::Utc>>,
        pub version: i32,
        pub rating: Option<i32>,
        pub s3_published_version: Option<i32>,
        pub s3_published_at: Option<chrono::DateTime<chrono::Utc>>,
        pub s3_sha256: Option<String>,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}

    impl ActiveModelBehavior for ActiveModel {}
}

impl From<plugin::Model> for PluginDefinition {
    fn from(m: plugin::Model) -> Self {
        let manifest: PluginManifest =
            serde_json::from_str(&m.manifest_json).unwrap_or_else(|_| PluginManifest {
                code: m.code.clone(),
                title: m.title.clone(),
                runtime: PluginRuntime::from_str(&m.runtime),
                api_version: "1".to_string(),
                description: None,
                capabilities: vec![],
                client_kits: None,
                built_for_migration: None,
            });
        let params: Vec<ParamSpec> = serde_json::from_str(&m.params_json).unwrap_or_default();
        let data: DataBinding = serde_json::from_str(&m.data_json).unwrap_or_default();
        let view_spec: ViewSpec = serde_json::from_str(&m.view_spec_json).unwrap_or_default();
        let (assets, sql_resources) = decode_resources(&m.assets_json);

        let bundle = PluginBundle {
            manifest,
            params,
            data,
            client_script: m.client_script,
            server_script: m.server_script,
            view_spec,
            styles: m.styles,
            sql_resources,
            assets,
        };

        PluginDefinition {
            id: m.id,
            bundle,
            status: PluginStatus::from_str(&m.status),
            is_enabled: m.is_enabled,
            owner_user_id: m.owner_user_id,
            created_by_agent_id: m.created_by_agent_id,
            version: m.version,
            created_at: m.created_at.unwrap_or_else(Utc::now),
            updated_at: m.updated_at.unwrap_or_else(Utc::now),
            rating: m.rating,
            snapshot: None,
            s3_published_version: m.s3_published_version,
            s3_published_at: m.s3_published_at,
        }
    }
}

fn to_active(def: &PluginDefinition, is_insert: bool) -> plugin::ActiveModel {
    let now = Utc::now();
    let b = &def.bundle;
    plugin::ActiveModel {
        id: Set(def.id.clone()),
        code: Set(b.manifest.code.clone()),
        title: Set(b.manifest.title.clone()),
        runtime: Set(b.manifest.runtime.as_str().to_string()),
        status: Set(def.status.as_str().to_string()),
        manifest_json: Set(serde_json::to_string(&b.manifest).unwrap_or_else(|_| "{}".into())),
        params_json: Set(serde_json::to_string(&b.params).unwrap_or_else(|_| "[]".into())),
        data_json: Set(serde_json::to_string(&b.data).unwrap_or_else(|_| "{}".into())),
        client_script: Set(b.client_script.clone()),
        server_script: Set(b.server_script.clone()),
        view_spec_json: Set(serde_json::to_string(&b.view_spec).unwrap_or_else(|_| "{}".into())),
        styles: Set(b.styles.clone()),
        assets_json: Set(encode_resources(b)),
        owner_user_id: Set(def.owner_user_id.clone()),
        created_by_agent_id: Set(def.created_by_agent_id.clone()),
        is_enabled: Set(def.is_enabled),
        is_deleted: Set(false),
        created_at: Set(Some(if is_insert { now } else { def.created_at })),
        updated_at: Set(Some(now)),
        version: Set(def.version),
        rating: Set(def.rating),
        // Публикация в S3 не затрагивается обычным сохранением бандла — обновляется
        // только через mark_published() точечным UPDATE.
        s3_published_version: sea_orm::ActiveValue::NotSet,
        s3_published_at: sea_orm::ActiveValue::NotSet,
        s3_sha256: sea_orm::ActiveValue::NotSet,
    }
}

// ============================================================================
// Repository functions
// ============================================================================

pub async fn list_all(db: &DatabaseConnection) -> Result<Vec<PluginDefinition>, DbErr> {
    let models = plugin::Entity::find()
        .filter(plugin::Column::IsDeleted.eq(false))
        .order_by_desc(plugin::Column::UpdatedAt)
        .all(db)
        .await?;
    Ok(models.into_iter().map(Into::into).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_legacy_asset_map() {
        let (assets, sql_resources) = decode_resources(r#"{"logo.svg":"data:image/svg+xml,..."}"#);
        assert_eq!(
            assets.get("logo.svg").map(String::as_str),
            Some("data:image/svg+xml,...")
        );
        assert!(sql_resources.is_empty());
    }

    #[test]
    fn decodes_versioned_resources() {
        let json = serde_json::json!({
            "format": RESOURCE_STORAGE_FORMAT,
            "assets": { "logo.svg": "logo" },
            "sql_resources": { "report": "SELECT 1" }
        })
        .to_string();
        let (assets, sql_resources) = decode_resources(&json);
        assert_eq!(assets.get("logo.svg").map(String::as_str), Some("logo"));
        assert_eq!(
            sql_resources.get("report").map(String::as_str),
            Some("SELECT 1")
        );
    }
}

/// Плагины, доступные для использования обычным пользователям и в меню:
/// включённые И со статусом `active` (только такие реально запускаются — см.
/// гейт в `service::invoke`).
pub async fn list_enabled(db: &DatabaseConnection) -> Result<Vec<PluginDefinition>, DbErr> {
    let models = plugin::Entity::find()
        .filter(plugin::Column::IsDeleted.eq(false))
        .filter(plugin::Column::IsEnabled.eq(true))
        .filter(plugin::Column::Status.eq(PluginStatus::Active.as_str()))
        .order_by_asc(plugin::Column::Title)
        .all(db)
        .await?;
    Ok(models.into_iter().map(Into::into).collect())
}

pub async fn find_by_id<C: ConnectionTrait>(
    db: &C,
    id: &str,
) -> Result<Option<PluginDefinition>, DbErr> {
    let model = plugin::Entity::find_by_id(id.to_string()).one(db).await?;
    Ok(model.filter(|m| !m.is_deleted).map(Into::into))
}

/// Поиск плагина по бизнес-коду (`manifest.code`) — ключ переносимости между
/// экземплярами и основа upsert-by-code / будущего импорта.
pub async fn find_by_code(
    db: &DatabaseConnection,
    code: &str,
) -> Result<Option<PluginDefinition>, DbErr> {
    let model = plugin::Entity::find()
        .filter(plugin::Column::IsDeleted.eq(false))
        .filter(plugin::Column::Code.eq(code.to_string()))
        .one(db)
        .await?;
    Ok(model.map(Into::into))
}

pub async fn insert<C: ConnectionTrait>(db: &C, def: &PluginDefinition) -> Result<(), DbErr> {
    to_active(def, true).insert(db).await?;
    Ok(())
}

pub async fn update<C: ConnectionTrait>(db: &C, def: &PluginDefinition) -> Result<(), DbErr> {
    plugin::Entity::update(to_active(def, false))
        .exec(db)
        .await?;
    Ok(())
}

/// Точечно фиксирует последнюю успешную публикацию плагина в S3, не трогая
/// остальные колонки (bundle JSON и т.п.) — в отличие от полного `update()`.
pub async fn mark_published<C: ConnectionTrait>(
    db: &C,
    id: &str,
    version: i32,
    published_at: chrono::DateTime<chrono::Utc>,
    sha256: &str,
) -> Result<(), DbErr> {
    plugin::Entity::update_many()
        .col_expr(plugin::Column::S3PublishedVersion, Expr::value(version))
        .col_expr(plugin::Column::S3PublishedAt, Expr::value(published_at))
        .col_expr(plugin::Column::S3Sha256, Expr::value(sha256.to_string()))
        .filter(plugin::Column::Id.eq(id.to_string()))
        .exec(db)
        .await?;
    Ok(())
}

pub async fn soft_delete(db: &DatabaseConnection, id: &str) -> Result<(), DbErr> {
    plugin::Entity::update_many()
        .col_expr(plugin::Column::IsDeleted, Expr::value(true))
        .col_expr(plugin::Column::UpdatedAt, Expr::value(Utc::now()))
        .filter(plugin::Column::Id.eq(id.to_string()))
        .exec(db)
        .await?;
    Ok(())
}

pub async fn insert_revision<C: ConnectionTrait>(
    db: &C,
    plugin_id: &str,
    version: i32,
    bundle: &PluginBundle,
    validate_report: &contracts::plugins::PluginValidateReport,
    smoke_report_json: Option<&str>,
    created_by_agent_id: Option<&str>,
    source_spec_json: Option<&str>,
    source_hash: Option<&str>,
    snapshot_meta_json: Option<&str>,
    origin_json: Option<&str>,
) -> Result<(), DbErr> {
    let id = uuid::Uuid::new_v4().to_string();
    let bundle_json = serde_json::to_string(bundle).unwrap_or_else(|_| "{}".to_string());
    let validate_json = serde_json::to_string(validate_report).unwrap_or_else(|_| "{}".to_string());
    db.execute(sea_orm::Statement::from_sql_and_values(
        sea_orm::DatabaseBackend::Sqlite,
        "INSERT INTO plugin_revision \
            (id, plugin_id, version, bundle_json, validate_report_json, smoke_report_json, created_by_agent_id, \
             source_spec_json, source_hash, snapshot_meta_json, origin_json, created_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, datetime('now'))",
        vec![
            id.into(),
            plugin_id.into(),
            version.into(),
            bundle_json.into(),
            validate_json.into(),
            smoke_report_json.map(str::to_string).into(),
            created_by_agent_id.map(str::to_string).into(),
            source_spec_json.map(str::to_string).into(),
            source_hash.map(str::to_string).into(),
            snapshot_meta_json.map(str::to_string).into(),
            origin_json.map(str::to_string).into(),
        ],
    ))
    .await?;
    Ok(())
}

pub async fn upsert_snapshot<C: ConnectionTrait>(
    db: &C,
    plugin_id: &str,
    plugin_version: i32,
    method: &str,
    payload: &serde_json::Value,
    row_count: usize,
    size_bytes: usize,
    source_hash: &str,
) -> Result<contracts::plugins::PluginSnapshotMeta, DbErr> {
    let id = uuid::Uuid::new_v4().to_string();
    let payload_json = serde_json::to_string(payload).unwrap_or_else(|_| "[]".to_string());
    db.execute(sea_orm::Statement::from_sql_and_values(
        sea_orm::DatabaseBackend::Sqlite,
        "INSERT OR REPLACE INTO plugin_snapshot \
            (id, plugin_id, plugin_version, method, payload_json, row_count, size_bytes, source_hash, created_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, datetime('now'))",
        vec![
            id.into(),
            plugin_id.into(),
            plugin_version.into(),
            method.into(),
            payload_json.into(),
            (row_count as i64).into(),
            (size_bytes as i64).into(),
            source_hash.into(),
        ],
    ))
    .await?;
    Ok(contracts::plugins::PluginSnapshotMeta {
        plugin_version,
        created_at: Utc::now(),
        row_count,
        size_bytes,
        source_hash: source_hash.to_string(),
    })
}

pub async fn find_snapshot<C: ConnectionTrait>(
    db: &C,
    plugin_id: &str,
    plugin_version: i32,
    method: &str,
) -> Result<Option<(contracts::plugins::PluginSnapshotMeta, serde_json::Value)>, DbErr> {
    use sea_orm::FromQueryResult;
    let rows = serde_json::Value::find_by_statement(sea_orm::Statement::from_sql_and_values(
        sea_orm::DatabaseBackend::Sqlite,
        "SELECT payload_json, row_count, size_bytes, source_hash, created_at \
         FROM plugin_snapshot WHERE plugin_id = ? AND plugin_version = ? AND method = ? LIMIT 1",
        vec![plugin_id.into(), plugin_version.into(), method.into()],
    ))
    .all(db)
    .await?;
    let Some(row) = rows.into_iter().next() else {
        return Ok(None);
    };
    let payload_json = row
        .get("payload_json")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("[]");
    let payload = serde_json::from_str(payload_json).unwrap_or_else(|_| serde_json::json!([]));
    let created_raw = row
        .get("created_at")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    let created_at = chrono::NaiveDateTime::parse_from_str(created_raw, "%Y-%m-%d %H:%M:%S")
        .map(|value| chrono::DateTime::<Utc>::from_naive_utc_and_offset(value, Utc))
        .unwrap_or_else(|_| Utc::now());
    let meta = contracts::plugins::PluginSnapshotMeta {
        plugin_version,
        created_at,
        row_count: row
            .get("row_count")
            .and_then(serde_json::Value::as_i64)
            .unwrap_or(0) as usize,
        size_bytes: row
            .get("size_bytes")
            .and_then(serde_json::Value::as_i64)
            .unwrap_or(0) as usize,
        source_hash: row
            .get("source_hash")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_string(),
    };
    Ok(Some((meta, payload)))
}
