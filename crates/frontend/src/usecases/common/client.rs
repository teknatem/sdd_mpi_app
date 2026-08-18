//! HTTP-клиент страниц импорта: один на три use-case.
//!
//! Различия u502/u503/u504 живут только в контрактах (`dateFrom` против
//! `date_from`, лишний флаг у Yandex), поэтому здесь один `match` на три ветки
//! вместо трёх одинаковых файлов `api.rs`.

use chrono::NaiveDate;
use contracts::domain::a005_marketplace::aggregate::Marketplace;
use contracts::domain::a006_connection_mp::aggregate::ConnectionMP;
use contracts::domain::common::AggregateId;
use contracts::enums::marketplace_type::MarketplaceType;
use std::collections::HashMap;

use super::progress::{IntoRunProgress, RunProgress, RunStatus};
use crate::shared::api_utils::{get_json, post_json};

use contracts::usecases::u502_import_from_ozon as ozon;
use contracts::usecases::u503_import_from_yandex as yandex;
use contracts::usecases::u504_import_from_wildberries as wb;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImportUseCase {
    Ozon,
    Yandex,
    Wildberries,
}

impl ImportUseCase {
    fn code(self) -> &'static str {
        match self {
            Self::Ozon => "u502",
            Self::Yandex => "u503",
            Self::Wildberries => "u504",
        }
    }

    /// Тип маркетплейса, подключения которого показывает страница.
    pub fn marketplace(self) -> MarketplaceType {
        match self {
            Self::Ozon => MarketplaceType::Ozon,
            Self::Yandex => MarketplaceType::YandexMarket,
            Self::Wildberries => MarketplaceType::Wildberries,
        }
    }
}

/// Запустить загрузку одного агрегата. Возвращает `session_id`.
pub async fn start(
    use_case: ImportUseCase,
    connection_id: String,
    aggregate: &str,
    date_from: NaiveDate,
    date_to: NaiveDate,
) -> Result<String, String> {
    let path = format!("/api/{}/import/start", use_case.code());
    let target_aggregates = vec![aggregate.to_string()];

    let session_id = match use_case {
        ImportUseCase::Ozon => {
            let request = ozon::ImportRequest {
                connection_id,
                target_aggregates,
                mode: ozon::request::ImportMode::Interactive,
                date_from,
                date_to,
            };
            post_json::<_, ozon::ImportResponse>(&path, &request)
                .await?
                .session_id
        }
        ImportUseCase::Yandex => {
            let request = yandex::ImportRequest {
                connection_id,
                target_aggregates,
                mode: yandex::request::ImportMode::Interactive,
                date_from,
                date_to,
                // Ручной импорт = полный бэкфилл за период. Отбор по дате
                // изменения оставлен планировщику (task013).
                incremental_by_update: false,
            };
            post_json::<_, yandex::ImportResponse>(&path, &request)
                .await?
                .session_id
        }
        ImportUseCase::Wildberries => {
            let request = wb::ImportRequest {
                connection_id,
                target_aggregates,
                mode: wb::request::ImportMode::Interactive,
                date_from,
                date_to,
            };
            post_json::<_, wb::ImportResponse>(&path, &request)
                .await?
                .session_id
        }
    };

    Ok(session_id)
}

/// Прогресс сессии, сведённый к строке нужного агрегата.
pub async fn progress(
    use_case: ImportUseCase,
    session_id: &str,
    aggregate: &str,
) -> Result<RunProgress, String> {
    let path = format!("/api/{}/import/{}/progress", use_case.code(), session_id);

    let progress = match use_case {
        ImportUseCase::Ozon => get_json::<ozon::ImportProgress>(&path)
            .await?
            .into_run_progress(aggregate),
        ImportUseCase::Yandex => get_json::<yandex::ImportProgress>(&path)
            .await?
            .into_run_progress(aggregate),
        ImportUseCase::Wildberries => get_json::<wb::ImportProgress>(&path)
            .await?
            .into_run_progress(aggregate),
    };

    Ok(progress)
}

/// Запустить загрузку и дождаться конца, отдавая прогресс наружу.
///
/// Нужна страницам агрегатов (a036/a037/a040), где импорт — одна кнопка без
/// собственной панели прогресса: страница загрузок работает не так, ей нужен
/// живой опрос без ожидания.
pub async fn run_to_completion(
    use_case: ImportUseCase,
    connection_id: String,
    aggregate: &str,
    date_from: NaiveDate,
    date_to: NaiveDate,
    on_progress: impl Fn(&RunProgress),
) -> Result<(), String> {
    let session_id = start(use_case, connection_id, aggregate, date_from, date_to).await?;

    loop {
        gloo_timers::future::TimeoutFuture::new(2000).await;
        let progress = progress(use_case, &session_id, aggregate).await?;
        on_progress(&progress);

        if progress.status.is_finished() {
            return match progress.status {
                RunStatus::Completed => Ok(()),
                _ => Err(progress
                    .messages
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "см. журнал импорта".to_string())),
            };
        }
    }
}

/// Подключения нужного маркетплейса: тип хранится у a005, а не у самого подключения.
pub async fn load_connections(marketplace: MarketplaceType) -> Result<Vec<ConnectionMP>, String> {
    let marketplaces: Vec<Marketplace> = get_json("/api/marketplace").await?;
    let types: HashMap<String, Option<MarketplaceType>> = marketplaces
        .into_iter()
        .map(|mp| (mp.base.id.as_string(), mp.marketplace_type))
        .collect();

    let connections: Vec<ConnectionMP> = get_json("/api/connection_mp").await?;

    Ok(connections
        .into_iter()
        .filter(|conn| {
            types
                .get(&conn.marketplace_id)
                .and_then(|t| t.as_ref())
                .is_some_and(|t| *t == marketplace)
        })
        .collect())
}
