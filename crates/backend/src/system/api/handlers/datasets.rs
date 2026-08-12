//! HTTP-слой подсистемы наборов данных.
//!
//! Выгрузка и восстановление уезжают в фоновую задачу и опрашиваются через
//! `/jobs/:job_id`: снимок базы — это `VACUUM INTO` нескольких гигабайт, сжатие
//! и многочастная загрузка, то есть минуты, которых не переживёт ни один
//! разумный HTTP-таймаут. Связка «увидел этот дифф — подтвердил этот дифф» при
//! этом сохраняется: её держит `expected_bundle_sha256` внутри запроса, а не
//! синхронность вызова.
//!
//! Остальные операции (предпросмотр, удаление, скачивание архива) остаются
//! синхронными — они укладываются в один запрос.

use axum::body::Body;
use axum::extract::{Multipart, Path, Query};
use axum::http::header::{CONTENT_DISPOSITION, CONTENT_TYPE};
use axum::http::{HeaderValue, Response, StatusCode};
use axum::Json;
use contracts::system::datasets::{
    CreateSnapshotRequest, DatasetCatalogResponse, DatasetJobDto, DatasetJobKind,
    DatasetJobStartedDto, DatasetsStatusDto, RestoreApplyRequest, RestorePreviewDto,
    RestorePreviewRequest, RestoreResultDto, SnapshotManifestResponse, TransferLogResponse,
};
use serde::Deserialize;

use crate::shared::config;
use crate::system::auth::extractor::CurrentUser;
use crate::system::datasets::{jobs, publish, restore, status, ActorInfo};

/// Ошибки конфигурации S3 — это не сбой сервера, а не выполненная настройка:
/// отвечаем 503, чтобы UI показал «хранилище не настроено», а не «всё сломалось».
fn map_error(error: anyhow::Error) -> (StatusCode, String) {
    let text = error.to_string();
    let is_config = text.contains("S3 storage is disabled")
        || text.contains("[s3].bucket")
        || text.contains("[s3].access_key_id")
        || text.contains("[s3].secret_access_key");
    if is_config {
        return (StatusCode::SERVICE_UNAVAILABLE, text);
    }
    tracing::error!("datasets operation failed: {error:?}");
    (StatusCode::INTERNAL_SERVER_ERROR, text)
}

fn actor(claims: &contracts::system::auth::TokenClaims) -> ActorInfo {
    ActorInfo {
        user_id: Some(claims.sub.clone()),
        login: Some(claims.username.clone()),
    }
}

pub async fn get_status() -> Result<Json<DatasetsStatusDto>, (StatusCode, String)> {
    status::local_status().await.map(Json).map_err(map_error)
}

pub async fn get_catalog() -> Result<Json<DatasetCatalogResponse>, (StatusCode, String)> {
    let catalog = publish::get_catalog().await.map_err(map_error)?;
    let local_instance_id = config::load_config()
        .map(|cfg| config::instance_identity(&cfg).id)
        .unwrap_or_default();
    Ok(Json(DatasetCatalogResponse {
        snapshots: catalog.snapshots,
        local_instance_id,
    }))
}

/// Запускает выгрузку в фоне. 409 — уже идёт другая операция переноса.
pub async fn create_snapshot(
    CurrentUser(claims): CurrentUser,
    Json(request): Json<CreateSnapshotRequest>,
) -> Result<(StatusCode, Json<DatasetJobStartedDto>), (StatusCode, String)> {
    let job = jobs::start(DatasetJobKind::Snapshot, jobs::stage::SNAPSHOT_STAGES)
        .map_err(|error| (StatusCode::CONFLICT, error.to_string()))?;
    let job_id = job.job_id().to_string();
    let actor = actor(&claims);

    tokio::spawn(async move {
        let _guard = jobs::ActiveGuard::new(job.clone());
        match publish::create_snapshot(request, actor, &job).await {
            Ok(response) => job.finish_snapshot(response),
            Err(error) => {
                tracing::error!("datasets: выгрузка снапшота не удалась: {error:?}");
                job.finish_error(&error);
            }
        }
    });

    Ok((StatusCode::ACCEPTED, Json(DatasetJobStartedDto { job_id })))
}

pub async fn get_job(
    Path(job_id): Path<String>,
) -> Result<Json<DatasetJobDto>, (StatusCode, String)> {
    jobs::get(&job_id).map(Json).ok_or((
        StatusCode::NOT_FOUND,
        "Операция не найдена: она завершилась давно и уже выгружена из памяти.".to_string(),
    ))
}

/// Текущая операция, если она есть. Позволяет странице подхватить прогресс
/// работы, запущенной до перезагрузки вкладки.
pub async fn get_active_job() -> Json<Option<DatasetJobDto>> {
    Json(jobs::active())
}

pub async fn cancel_job(Path(job_id): Path<String>) -> Result<StatusCode, (StatusCode, String)> {
    if jobs::cancel(&job_id) {
        return Ok(StatusCode::NO_CONTENT);
    }
    Err((
        StatusCode::NOT_FOUND,
        "Операция не найдена или уже завершена.".to_string(),
    ))
}

pub async fn get_manifest(
    Path(snapshot_id): Path<String>,
) -> Result<Json<SnapshotManifestResponse>, (StatusCode, String)> {
    let manifest = publish::get_manifest(&snapshot_id)
        .await
        .map_err(map_error)?;
    Ok(Json(SnapshotManifestResponse { manifest }))
}

pub async fn download_snapshot(
    Path(snapshot_id): Path<String>,
) -> Result<Response<Body>, (StatusCode, String)> {
    let (filename, bytes) = publish::download_bundle(&snapshot_id)
        .await
        .map_err(map_error)?;

    let mut response = Response::new(Body::from(bytes));
    let headers = response.headers_mut();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/zip"));
    headers.insert(
        CONTENT_DISPOSITION,
        HeaderValue::from_str(&format!("attachment; filename=\"{filename}\"")).map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "bad filename".to_string(),
            )
        })?,
    );
    Ok(response)
}

pub async fn delete_snapshot(
    Path(snapshot_id): Path<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    publish::delete_snapshot(&snapshot_id)
        .await
        .map_err(map_error)?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn restore_preview(
    Json(request): Json<RestorePreviewRequest>,
) -> Result<Json<RestorePreviewDto>, (StatusCode, String)> {
    restore::preview(request).await.map(Json).map_err(map_error)
}

pub async fn restore_apply(
    CurrentUser(claims): CurrentUser,
    Json(request): Json<RestoreApplyRequest>,
) -> Result<(StatusCode, Json<DatasetJobStartedDto>), (StatusCode, String)> {
    let job = jobs::start(DatasetJobKind::Restore, jobs::stage::RESTORE_STAGES)
        .map_err(|error| (StatusCode::CONFLICT, error.to_string()))?;
    let job_id = job.job_id().to_string();
    let actor = actor(&claims);

    tokio::spawn(async move {
        let _guard = jobs::ActiveGuard::new(job.clone());
        match restore::apply(request, actor, &job).await {
            Ok(response) => {
                let restart = response.requires_restart;
                job.finish_restore(response);
                // Даём секундному polling UI забрать финальный результат, затем
                // штатно закрываем HTTP-сервер. NSSM поднимет процесс снова и
                // bootstrap применит pending_restore до открытия пула БД.
                if restart {
                    crate::system::restart::schedule(std::time::Duration::from_secs(5));
                }
            }
            Err(error) => {
                tracing::error!("datasets: восстановление не удалось: {error:?}");
                job.finish_error(&error);
            }
        }
    });

    Ok((StatusCode::ACCEPTED, Json(DatasetJobStartedDto { job_id })))
}

/// Восстановление из загруженного zip — запасной путь, когда S3 недоступен,
/// и штатный способ отката из автоснапшота `pre-restore-*.zip`.
pub async fn restore_upload(
    CurrentUser(claims): CurrentUser,
    mut multipart: Multipart,
) -> Result<Json<RestoreResultDto>, (StatusCode, String)> {
    let mut bytes: Option<bytes::Bytes> = None;
    let mut set_ids: Vec<String> = Vec::new();
    let mut mode = contracts::system::datasets::RestoreMode::Merge;

    while let Some(field) = multipart.next_field().await.map_err(|error| {
        (
            StatusCode::BAD_REQUEST,
            format!("не удалось прочитать форму: {error}"),
        )
    })? {
        match field.name().unwrap_or_default() {
            "set_ids" => {
                let value = field
                    .text()
                    .await
                    .map_err(|_| (StatusCode::BAD_REQUEST, "bad set_ids".to_string()))?;
                set_ids = serde_json::from_str(&value).unwrap_or_else(|_| {
                    value
                        .split(',')
                        .map(|part| part.trim().to_string())
                        .filter(|part| !part.is_empty())
                        .collect()
                });
            }
            "mode" => {
                let value = field
                    .text()
                    .await
                    .map_err(|_| (StatusCode::BAD_REQUEST, "bad mode".to_string()))?;
                if value.trim() == "replace" {
                    mode = contracts::system::datasets::RestoreMode::Replace;
                }
            }
            "file" => {
                bytes = Some(
                    field
                        .bytes()
                        .await
                        .map_err(|_| (StatusCode::BAD_REQUEST, "bad file".to_string()))?,
                );
            }
            _ => {}
        }
    }

    let bytes = bytes.ok_or((
        StatusCode::BAD_REQUEST,
        "не передан файл архива".to_string(),
    ))?;

    // Идентификатор снапшота берём из манифеста самого архива: у офлайн-импорта
    // записи в каталоге нет.
    let manifest = crate::system::datasets::bundle::read_manifest(&bytes).map_err(|error| {
        (
            StatusCode::BAD_REQUEST,
            format!("это не архив снапшота: {error}"),
        )
    })?;
    if set_ids.is_empty() {
        set_ids = manifest.sets.iter().map(|set| set.set_id.clone()).collect();
    }

    restore::apply_from_bytes(
        bytes,
        RestoreApplyRequest {
            snapshot_id: manifest.snapshot_id,
            set_ids,
            mode,
            expected_bundle_sha256: None,
        },
        actor(&claims),
    )
    .await
    .map(Json)
    .map_err(map_error)
}

#[derive(Debug, Deserialize)]
pub struct HistoryQuery {
    pub limit: Option<u64>,
}

pub async fn get_history(
    Query(query): Query<HistoryQuery>,
) -> Result<Json<TransferLogResponse>, (StatusCode, String)> {
    let limit = query.limit.unwrap_or(50).clamp(1, 500);
    let items = crate::system::datasets::repository::list(limit)
        .await
        .map_err(map_error)?;
    Ok(Json(TransferLogResponse { items }))
}
