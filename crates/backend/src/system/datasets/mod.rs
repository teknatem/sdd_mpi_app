//! Наборы данных: систематизированная раскладка файловых данных приложения и
//! перенос выбранных наборов между экземплярами через S3.
//!
//! Задача подсистемы — сделать копирование данных между рабочим и тестовым
//! экземпляром обозримой операцией: видно, что именно переносится, откуда оно
//! приехало и чем отличается от того, что уже лежит на приёмнике.
//!
//! Раскладка в бакете:
//!
//! ```text
//! datasets/catalog.json
//! datasets/<instance_id>/<snapshot_id>/manifest.json
//! datasets/<instance_id>/<snapshot_id>/bundle.zip
//! ```
//!
//! Единственный способ узнать о снапшотах на принимающей стороне — `catalog.json`:
//! таблица `sys_files_s3` локальна для инстанса-донора, а листинга бакета в
//! самописном S3-клиенте нет. Каталог обновляется read-modify-write, ровно как
//! `plugins/catalog.json`, и наследует то же ограничение: одновременная
//! публикация с двух инстансов может потерять одну из записей. Для админской
//! операции, выполняемой руками, это приемлемо; при появлении автоматической
//! выгрузки понадобится условная запись по ETag.

pub mod bootstrap;
pub mod bundle;
pub mod publish;
pub mod registry;
pub mod repository;
pub mod restore;
pub mod scan;
pub mod status;

use chrono::Utc;
use contracts::system::datasets::SourceInfo;

use crate::shared::config::{self, Config};

/// Кто выполняет операцию — попадает в манифест и журнал переносов.
#[derive(Debug, Clone)]
pub struct ActorInfo {
    pub user_id: Option<String>,
    pub login: Option<String>,
}

impl ActorInfo {
    pub fn system(reason: &str) -> Self {
        Self {
            user_id: None,
            login: Some(format!("auto:{reason}")),
        }
    }

    /// Строка для поля `created_by` манифеста.
    pub fn created_by(&self) -> String {
        match (&self.user_id, &self.login) {
            (Some(id), Some(login)) => format!("user:{id} ({login})"),
            (Some(id), None) => format!("user:{id}"),
            (None, Some(login)) => login.clone(),
            (None, None) => "unknown".to_string(),
        }
    }
}

/// `<YYYYMMDDTHHMMSSZ>-<8hex>`: сортируется лексикографически по времени и при
/// этом уникален, даже если два снапшота попали в одну секунду.
pub fn new_snapshot_id() -> String {
    let stamp = Utc::now().format("%Y%m%dT%H%M%SZ");
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    format!("{stamp}-{}", &suffix[..8])
}

/// Диагностика текущего инстанса для манифеста снапшота.
pub async fn source_info(config: &Config) -> SourceInfo {
    let identity = config::instance_identity(config);
    let schema_version = crate::shared::data::migration_runner::current_migration_version()
        .await
        .unwrap_or(-1);

    SourceInfo {
        instance_id: identity.id,
        instance_label: identity.label,
        instance_env: identity.env,
        hostname: identity.hostname,
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        git_commit: option_env!("GIT_COMMIT").unwrap_or("unknown").to_string(),
        build_profile: option_env!("BUILD_PROFILE").unwrap_or("unknown").to_string(),
        schema_version,
        os: format!("{}/{}", std::env::consts::OS, std::env::consts::ARCH),
        data_root: config::get_data_root(config)
            .map(|root| root.display().to_string())
            .unwrap_or_default(),
        database_path: config::get_database_path(config)
            .map(|path| path.display().to_string())
            .unwrap_or_default(),
        config_path: config::get_config_path()
            .map(|path| path.display().to_string())
            .unwrap_or_default(),
        captured_at: Utc::now().to_rfc3339(),
    }
}
