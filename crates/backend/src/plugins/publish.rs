//! Публикация плагинов в S3 и проверка/применение обновлений.
//!
//! Простая схема (без multi-tenant entitlements/подписей): один shared bucket,
//! одни креды из `[s3]`, единый объект `plugins/catalog.json` со списком последних
//! опубликованных версий по коду плагина. Бандл лежит по ключу
//! `plugins/{code}/{version}/bundle.plugin`.
//!
//! Известное ограничение: `upsert_catalog_entry` — это read-modify-write без
//! ETag-блокировки; гонка при одновременной публикации двумя админами теоретически
//! возможна, но не обрабатывается — не нужно для single-admin сценария использования.

use bytes::Bytes;
use chrono::Utc;
use contracts::plugins::{
    PluginCatalog, PluginCatalogEntry, PluginPublishResult, PluginUpdateStatus,
};

use super::{package, repository, service};
use crate::shared::config::S3Config;
use crate::system::s3::client;
use crate::system::s3::service as s3_service;

const CATALOG_KEY: &str = "plugins/catalog.json";

fn db() -> &'static sea_orm::DatabaseConnection {
    crate::shared::data::db::get_connection()
}

fn bundle_key(code: &str, version: i32) -> String {
    format!("plugins/{code}/{version}/bundle.plugin")
}

async fn read_catalog(cfg: &S3Config) -> anyhow::Result<PluginCatalog> {
    match client::get_object_opt(cfg, CATALOG_KEY).await? {
        None => Ok(PluginCatalog::new()),
        Some(object) if object.bytes.is_empty() => Ok(PluginCatalog::new()),
        Some(object) => Ok(serde_json::from_slice(&object.bytes).unwrap_or_default()),
    }
}

async fn upsert_catalog_entry(
    cfg: &S3Config,
    code: &str,
    entry: PluginCatalogEntry,
) -> anyhow::Result<()> {
    let mut catalog = read_catalog(cfg).await?;
    catalog.insert(code.to_string(), entry);
    let bytes = serde_json::to_vec(&catalog)?;
    client::put_object(
        cfg,
        CATALOG_KEY,
        Some("application/json"),
        Bytes::from(bytes),
    )
    .await?;
    Ok(())
}

/// Публикует текущую сохранённую (в БД) версию плагина в S3.
pub async fn publish(id: &str) -> anyhow::Result<PluginPublishResult> {
    let cfg = s3_service::s3_config()?;
    let def = repository::find_by_id(db(), id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Plugin not found: {id}"))?;

    let code = def.bundle.manifest.code.clone();
    let title = def.bundle.manifest.title.clone();
    let version = def.version;

    // Публикуем код/манифест, без snapshot данных — снепшоты не переносимы между инстансами.
    let bytes = package::export_bundle(&def.bundle)?;
    let sha256 = client::sha256_hex(&bytes);
    let size_bytes = bytes.len() as u64;
    let key = bundle_key(&code, version);

    client::put_object(
        &cfg,
        &key,
        Some("application/octet-stream"),
        Bytes::from(bytes),
    )
    .await?;

    let uploaded_at = Utc::now();
    upsert_catalog_entry(
        &cfg,
        &code,
        PluginCatalogEntry {
            version,
            uploaded_at,
            sha256: sha256.clone(),
            key: key.clone(),
            size_bytes,
            title,
        },
    )
    .await?;

    repository::mark_published(db(), id, version, uploaded_at, &sha256).await?;
    super::change_token::TOKEN.bump();

    Ok(PluginPublishResult {
        code,
        version,
        uploaded_at,
        sha256,
        key,
        size_bytes,
    })
}

/// Возвращает каталог целиком (code -> последняя опубликованная версия) — для UI-вкладки
/// «Доступные плагины» (плагины, опубликованные в S3, но отсутствующие локально).
pub async fn get_catalog() -> anyhow::Result<PluginCatalog> {
    let cfg = s3_service::s3_config()?;
    read_catalog(&cfg).await
}

/// Скачивает и создаёт новую локальную запись (`Draft`/выключен) из версии, опубликованной
/// в S3 под данным `code`, если такого плагина ещё нет локально.
pub async fn install_from_catalog(code: &str) -> anyhow::Result<service::ImportOutcome> {
    if service::get_by_code(code).await?.is_some() {
        anyhow::bail!("Plugin {code} is already installed locally — use apply-update instead");
    }

    let cfg = s3_service::s3_config()?;
    let catalog = read_catalog(&cfg).await?;
    let entry = catalog
        .get(code)
        .ok_or_else(|| anyhow::anyhow!("No published version found in S3 for code {code}"))?;

    let object = client::get_object(&cfg, &entry.key).await?;
    let actual_sha256 = client::sha256_hex(&object.bytes);
    if actual_sha256 != entry.sha256 {
        anyhow::bail!("Downloaded bundle hash mismatch for plugin {code}");
    }

    let bundle = package::import_archive(&object.bytes)?;
    // Свежая установка сразу получает номер версии из каталога, чтобы локальный
    // счётчик совпадал с сервером.
    let outcome = service::import_bundle_onto(None, bundle, None, Some(entry.version)).await?;
    super::change_token::TOKEN.bump();
    Ok(outcome)
}

/// Сравнивает локальные версии плагинов с каталогом в S3 (один запрос catalog.json).
pub async fn check_updates() -> anyhow::Result<Vec<PluginUpdateStatus>> {
    let cfg = s3_service::s3_config()?;
    let catalog = read_catalog(&cfg).await?;
    let plugins = repository::list_all(db()).await?;

    Ok(plugins
        .into_iter()
        .map(|def| {
            let code = def.bundle.manifest.code.clone();
            let remote = catalog.get(&code);
            let remote_version = remote.map(|entry| entry.version);
            let update_available = remote_version
                .map(|remote_version| remote_version > def.version)
                .unwrap_or(false);
            PluginUpdateStatus {
                plugin_id: def.id,
                code,
                local_version: def.version,
                remote_version,
                remote_uploaded_at: remote.map(|entry| entry.uploaded_at),
                update_available,
            }
        })
        .collect())
}

/// Скачивает и применяет опубликованную версию плагина поверх существующей записи `id`,
/// сохраняя `status`/`is_enabled`/`owner_user_id`/`rating` как есть (меняются только
/// бандл и версия).
pub async fn apply_update(
    id: &str,
    expected_remote_version: Option<i32>,
) -> anyhow::Result<service::ImportOutcome> {
    let cfg = s3_service::s3_config()?;
    let def = repository::find_by_id(db(), id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Plugin not found: {id}"))?;
    let code = def.bundle.manifest.code.clone();

    let catalog = read_catalog(&cfg).await?;
    let entry = catalog
        .get(&code)
        .ok_or_else(|| anyhow::anyhow!("No published version found in S3 for code {code}"))?;

    if let Some(expected) = expected_remote_version {
        if entry.version != expected {
            anyhow::bail!(
                "Catalog version changed: expected {expected}, actual {}",
                entry.version
            );
        }
    }
    if entry.version <= def.version {
        anyhow::bail!(
            "Local version {} is already up to date with catalog version {}",
            def.version,
            entry.version
        );
    }

    let object = client::get_object(&cfg, &entry.key).await?;
    let actual_sha256 = client::sha256_hex(&object.bytes);
    if actual_sha256 != entry.sha256 {
        anyhow::bail!("Downloaded bundle hash mismatch for plugin {code}");
    }

    let bundle = package::import_archive(&object.bytes)?;
    // Локальная версия становится номером установленной версии из каталога, а не +1.
    let outcome = service::import_bundle_onto(Some(id), bundle, None, Some(entry.version)).await?;
    super::change_token::TOKEN.bump();
    Ok(outcome)
}
