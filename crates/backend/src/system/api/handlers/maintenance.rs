//! Управление режимом технического обслуживания.
//!
//! Чтение статуса — без авторизации: заглушку надо показать и тому, кто ещё не
//! вошёл, а сам факт «идут работы» секретом не является.

use axum::http::StatusCode;
use axum::Json;
use contracts::system::maintenance::{
    MaintenanceStatusDto, MaintenanceTrigger, SetMaintenanceRequest, DEFAULT_MAINTENANCE_REASON,
};

use crate::system::auth::extractor::CurrentUser;
use crate::system::maintenance;

pub async fn get_status() -> Json<MaintenanceStatusDto> {
    Json(maintenance::status())
}

pub async fn enable(
    CurrentUser(claims): CurrentUser,
    Json(request): Json<SetMaintenanceRequest>,
) -> Json<MaintenanceStatusDto> {
    let reason = request
        .reason
        .map(|reason| reason.trim().to_string())
        .filter(|reason| !reason.is_empty())
        .unwrap_or_else(|| DEFAULT_MAINTENANCE_REASON.to_string());

    maintenance::enable(
        reason,
        MaintenanceTrigger::Manual,
        format!("user:{} ({})", claims.sub, claims.username),
    );
    Json(maintenance::status())
}

pub async fn disable() -> Result<Json<MaintenanceStatusDto>, (StatusCode, String)> {
    // Пока подмена базы подготовлена, снимать режим нельзя: пользователи войдут
    // и начнут писать в файл, который будет заменён при следующем запуске.
    if maintenance::status().requires_restart {
        return Err((
            StatusCode::CONFLICT,
            "Подготовлена подмена базы данных, автоматический перезапуск уже запланирован. \
             Снять режим сейчас означало бы впустить пользователей в базу, которая будет заменена."
                .to_string(),
        ));
    }
    maintenance::disable();
    Ok(Json(maintenance::status()))
}
