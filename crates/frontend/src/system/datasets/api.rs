use contracts::system::datasets::{
    CreateSnapshotRequest, DatasetCatalogResponse, DatasetJobDto, DatasetJobStartedDto,
    DatasetsStatusDto, RestoreApplyRequest, RestorePreviewDto, RestorePreviewRequest,
    SnapshotManifestResponse, TransferLogResponse,
};
use gloo_net::http::Request;

use crate::shared::api_utils::api_base;
use crate::system::auth::storage;

fn auth_header() -> Result<String, String> {
    storage::get_access_token()
        .map(|token| format!("Bearer {}", token))
        .ok_or_else(|| "Не выполнен вход".to_string())
}

/// Тело ответа несёт текст ошибки от бэкенда (в том числе 503 «S3 не настроен»),
/// и показать его пользователю полезнее, чем голый код статуса.
async fn read_error(response: gloo_net::http::Response, context: &str) -> String {
    let status = response.status();
    match response.text().await {
        Ok(body) if !body.trim().is_empty() => format!("{context}: {body}"),
        _ => format!("{context}: HTTP {status}"),
    }
}

pub async fn fetch_status() -> Result<DatasetsStatusDto, String> {
    let response = Request::get(&format!("{}/api/sys/datasets/status", api_base()))
        .header("Authorization", &auth_header()?)
        .send()
        .await
        .map_err(|e| format!("Не удалось получить состояние наборов: {e}"))?;

    if !response.ok() {
        return Err(read_error(response, "Не удалось получить состояние наборов").await);
    }
    response
        .json()
        .await
        .map_err(|e| format!("Не удалось разобрать состояние наборов: {e}"))
}

pub async fn fetch_catalog() -> Result<DatasetCatalogResponse, String> {
    let response = Request::get(&format!("{}/api/sys/datasets/catalog", api_base()))
        .header("Authorization", &auth_header()?)
        .send()
        .await
        .map_err(|e| format!("Не удалось получить каталог снапшотов: {e}"))?;

    if !response.ok() {
        return Err(read_error(response, "Не удалось получить каталог снапшотов").await);
    }
    response
        .json()
        .await
        .map_err(|e| format!("Не удалось разобрать каталог снапшотов: {e}"))
}

/// Запускает выгрузку. Ответ — только идентификатор фоновой задачи: сама
/// операция может идти минутами, и её результат забирается опросом `fetch_job`.
pub async fn create_snapshot(
    request: &CreateSnapshotRequest,
) -> Result<DatasetJobStartedDto, String> {
    let response = Request::post(&format!("{}/api/sys/datasets/snapshots", api_base()))
        .header("Authorization", &auth_header()?)
        .json(request)
        .map_err(|e| format!("Не удалось сформировать запрос: {e}"))?
        .send()
        .await
        .map_err(|e| format!("Не удалось запустить выгрузку: {e}"))?;

    if !response.ok() {
        return Err(read_error(response, "Не удалось запустить выгрузку").await);
    }
    response
        .json()
        .await
        .map_err(|e| format!("Не удалось разобрать ответ на запуск выгрузки: {e}"))
}

pub async fn fetch_manifest(snapshot_id: &str) -> Result<SnapshotManifestResponse, String> {
    let response = Request::get(&format!(
        "{}/api/sys/datasets/snapshots/{snapshot_id}",
        api_base()
    ))
    .header("Authorization", &auth_header()?)
    .send()
    .await
    .map_err(|e| format!("Не удалось получить манифест: {e}"))?;

    if !response.ok() {
        return Err(read_error(response, "Не удалось получить манифест").await);
    }
    response
        .json()
        .await
        .map_err(|e| format!("Не удалось разобрать манифест: {e}"))
}

pub async fn delete_snapshot(snapshot_id: &str) -> Result<(), String> {
    let response = Request::delete(&format!(
        "{}/api/sys/datasets/snapshots/{snapshot_id}",
        api_base()
    ))
    .header("Authorization", &auth_header()?)
    .send()
    .await
    .map_err(|e| format!("Не удалось удалить снапшот: {e}"))?;

    if !response.ok() {
        return Err(read_error(response, "Не удалось удалить снапшот").await);
    }
    Ok(())
}

pub async fn restore_preview(request: &RestorePreviewRequest) -> Result<RestorePreviewDto, String> {
    let response = Request::post(&format!("{}/api/sys/datasets/restore/preview", api_base()))
        .header("Authorization", &auth_header()?)
        .json(request)
        .map_err(|e| format!("Не удалось сформировать запрос: {e}"))?
        .send()
        .await
        .map_err(|e| format!("Не удалось посчитать различия: {e}"))?;

    if !response.ok() {
        return Err(read_error(response, "Не удалось посчитать различия").await);
    }
    response
        .json()
        .await
        .map_err(|e| format!("Не удалось разобрать различия: {e}"))
}

pub async fn restore_apply(request: &RestoreApplyRequest) -> Result<DatasetJobStartedDto, String> {
    let response = Request::post(&format!("{}/api/sys/datasets/restore", api_base()))
        .header("Authorization", &auth_header()?)
        .json(request)
        .map_err(|e| format!("Не удалось сформировать запрос: {e}"))?
        .send()
        .await
        .map_err(|e| format!("Не удалось запустить восстановление: {e}"))?;

    if !response.ok() {
        return Err(read_error(response, "Не удалось запустить восстановление").await);
    }
    response
        .json()
        .await
        .map_err(|e| format!("Не удалось разобрать ответ на запуск восстановления: {e}"))
}

/// Состояние фоновой задачи. Опрашивается раз в секунду, пока задача идёт.
pub async fn fetch_job(job_id: &str) -> Result<DatasetJobDto, String> {
    let response = Request::get(&format!("{}/api/sys/datasets/jobs/{job_id}", api_base()))
        .header("Authorization", &auth_header()?)
        .send()
        .await
        .map_err(|e| format!("Не удалось получить состояние операции: {e}"))?;

    if !response.ok() {
        return Err(read_error(response, "Не удалось получить состояние операции").await);
    }
    response
        .json()
        .await
        .map_err(|e| format!("Не удалось разобрать состояние операции: {e}"))
}

/// Операция, идущая прямо сейчас, — чтобы страница после перезагрузки вкладки
/// снова показала прогресс, а не пустой экран поверх работающей выгрузки.
pub async fn fetch_active_job() -> Result<Option<DatasetJobDto>, String> {
    let response = Request::get(&format!("{}/api/sys/datasets/jobs/active", api_base()))
        .header("Authorization", &auth_header()?)
        .send()
        .await
        .map_err(|e| format!("Не удалось получить текущую операцию: {e}"))?;

    if !response.ok() {
        return Err(read_error(response, "Не удалось получить текущую операцию").await);
    }
    response
        .json()
        .await
        .map_err(|e| format!("Не удалось разобрать текущую операцию: {e}"))
}

pub async fn cancel_job(job_id: &str) -> Result<(), String> {
    let response = Request::post(&format!(
        "{}/api/sys/datasets/jobs/{job_id}/cancel",
        api_base()
    ))
    .header("Authorization", &auth_header()?)
    .send()
    .await
    .map_err(|e| format!("Не удалось прервать операцию: {e}"))?;

    if !response.ok() {
        return Err(read_error(response, "Не удалось прервать операцию").await);
    }
    Ok(())
}

pub async fn fetch_history() -> Result<TransferLogResponse, String> {
    let response = Request::get(&format!("{}/api/sys/datasets/history", api_base()))
        .header("Authorization", &auth_header()?)
        .send()
        .await
        .map_err(|e| format!("Не удалось получить журнал операций: {e}"))?;

    if !response.ok() {
        return Err(read_error(response, "Не удалось получить журнал операций").await);
    }
    response
        .json()
        .await
        .map_err(|e| format!("Не удалось разобрать журнал операций: {e}"))
}

/// Ссылка на скачивание бандла. Токен в заголовок обычной ссылки не подставить,
/// поэтому качаем через fetch и отдаём blob (см. `download_snapshot_file`).
pub fn snapshot_download_url(snapshot_id: &str) -> String {
    format!(
        "{}/api/sys/datasets/snapshots/{snapshot_id}/download",
        api_base()
    )
}

pub async fn download_snapshot_bytes(snapshot_id: &str) -> Result<Vec<u8>, String> {
    let response = Request::get(&snapshot_download_url(snapshot_id))
        .header("Authorization", &auth_header()?)
        .send()
        .await
        .map_err(|e| format!("Не удалось скачать архив: {e}"))?;

    if !response.ok() {
        return Err(read_error(response, "Не удалось скачать архив").await);
    }
    response
        .binary()
        .await
        .map_err(|e| format!("Не удалось прочитать архив: {e}"))
}
