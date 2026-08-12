use contracts::system::maintenance::{MaintenanceStatusDto, SetMaintenanceRequest};
use gloo_net::http::Request;

use crate::shared::api_utils::api_base;
use crate::system::auth::storage;

fn auth_header() -> Result<String, String> {
    storage::get_access_token()
        .map(|token| format!("Bearer {}", token))
        .ok_or_else(|| "Не выполнен вход".to_string())
}

/// Статус читается без авторизации: заглушку надо показать и тому, кто ещё не
/// вошёл, и тому, у кого истёк токен.
pub async fn fetch_status() -> Result<MaintenanceStatusDto, String> {
    let response = Request::get(&format!("{}/api/system/maintenance", api_base()))
        .send()
        .await
        .map_err(|e| format!("Не удалось получить статус обслуживания: {e}"))?;

    if !response.ok() {
        return Err(format!(
            "Не удалось получить статус обслуживания: HTTP {}",
            response.status()
        ));
    }
    response
        .json()
        .await
        .map_err(|e| format!("Не удалось разобрать статус обслуживания: {e}"))
}

pub async fn enable(reason: Option<String>) -> Result<MaintenanceStatusDto, String> {
    let response = Request::post(&format!("{}/api/system/maintenance/enable", api_base()))
        .header("Authorization", &auth_header()?)
        .json(&SetMaintenanceRequest { reason })
        .map_err(|e| format!("Не удалось сформировать запрос: {e}"))?
        .send()
        .await
        .map_err(|e| format!("Не удалось включить режим обслуживания: {e}"))?;

    if !response.ok() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(if body.trim().is_empty() {
            format!("Не удалось включить режим обслуживания: HTTP {status}")
        } else {
            body
        });
    }
    response
        .json()
        .await
        .map_err(|e| format!("Не удалось разобрать ответ: {e}"))
}

pub async fn disable() -> Result<MaintenanceStatusDto, String> {
    let response = Request::post(&format!("{}/api/system/maintenance/disable", api_base()))
        .header("Authorization", &auth_header()?)
        .send()
        .await
        .map_err(|e| format!("Не удалось снять режим обслуживания: {e}"))?;

    if !response.ok() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(if body.trim().is_empty() {
            format!("Не удалось снять режим обслуживания: HTTP {status}")
        } else {
            body
        });
    }
    response
        .json()
        .await
        .map_err(|e| format!("Не удалось разобрать ответ: {e}"))
}
