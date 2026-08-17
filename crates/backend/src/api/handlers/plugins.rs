//! HTTP-обработчики подсистемы Plugins (admin-only).

use crate::shared::error::ApiError;
use axum::body::Bytes;
use axum::http::header;
use axum::response::IntoResponse;
use axum::{
    extract::{Path, Query},
    Json,
};
use serde::Deserialize;
use serde_json::json;

use crate::plugins::{publish, service};
use contracts::plugins::{
    PluginApplyUpdateRequest, PluginBundle, PluginCatalog, PluginDefinition, PluginError,
    PluginInvokeRequest, PluginListItem, PluginPublishResult, PluginRunBrief, PluginRunContext,
    PluginSmokeReport, PluginSmokeRequest, PluginStats, PluginUpdateStatus, PluginUpsert,
    PluginValidateReport,
};

#[derive(Deserialize)]
pub struct DaysQuery {
    #[serde(default)]
    days: Option<i64>,
}
use contracts::shared::drilldown::DrilldownResponse;

/// GET /api/plugin — список включённых плагинов (для меню/навигатора).
pub async fn list() -> Result<Json<Vec<PluginListItem>>, ApiError> {
    match service::list_enabled().await {
        Ok(defs) => Ok(Json(defs.iter().map(PluginListItem::from).collect())),
        Err(_) => Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into()),
    }
}

/// GET /api/plugin/all — все плагины (страница управления).
pub async fn list_all() -> Result<Json<Vec<PluginListItem>>, ApiError> {
    match service::list_all().await {
        Ok(defs) => Ok(Json(defs.iter().map(PluginListItem::from).collect())),
        Err(_) => Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into()),
    }
}

/// GET /api/plugin/:id — полное определение/бандл (включая исходники).
pub async fn get_by_id(Path(id): Path<String>) -> Result<Json<PluginDefinition>, ApiError> {
    match service::get_by_id(&id).await {
        Ok(Some(def)) => Ok(Json(def)),
        Ok(None) => Err(axum::http::StatusCode::NOT_FOUND.into()),
        Err(_) => Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into()),
    }
}

/// POST /api/plugin — создать или обновить плагин.
pub async fn upsert(
    Json(dto): Json<PluginUpsert>,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, Json<serde_json::Value>)> {
    match service::upsert(dto).await {
        Ok(id) => Ok(Json(json!({ "id": id }))),
        Err(e) => Err((
            axum::http::StatusCode::BAD_REQUEST,
            Json(json!({ "error": e.to_string() })),
        )),
    }
}

/// DELETE /api/plugin/:id — мягкое удаление.
pub async fn delete(Path(id): Path<String>) -> Result<(), ApiError> {
    match service::delete(&id).await {
        Ok(()) => Ok(()),
        Err(_) => Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into()),
    }
}

#[derive(Deserialize)]
pub struct SetRatingRequest {
    /// 1..5, либо null чтобы снять оценку.
    pub rating: Option<i32>,
}

/// POST /api/plugin/:id/rating
pub async fn set_rating(
    Path(id): Path<String>,
    Json(payload): Json<SetRatingRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    match service::set_rating(&id, payload.rating).await {
        Ok(()) => Ok(Json(json!({ "success": true }))),
        Err(e) => {
            tracing::warn!("set_rating failed for plugin {}: {}", id, e);
            Err(axum::http::StatusCode::BAD_REQUEST.into())
        }
    }
}

/// POST /api/plugin/validate — проверка бандла без сохранения.
/// Возвращает отчёт с перечнем серверных экспортов и структурированными ошибками.
pub async fn validate(Json(bundle): Json<PluginBundle>) -> Json<PluginValidateReport> {
    Json(service::validate(&bundle).await)
}

/// POST /api/plugin/smoke-test — validate + static client/server checks + dev method invokes.
pub async fn smoke_test(
    Json(request): Json<PluginSmokeRequest>,
) -> Result<Json<PluginSmokeReport>, (axum::http::StatusCode, Json<serde_json::Value>)> {
    match service::smoke_test(request).await {
        Ok(report) => Ok(Json(report)),
        Err(e) => Err((
            axum::http::StatusCode::BAD_REQUEST,
            Json(json!({ "error": e.to_string() })),
        )),
    }
}

/// POST /api/plugin/testdata — вставить демонстрационный плагин.
pub async fn testdata() -> Result<Json<serde_json::Value>, ApiError> {
    match service::insert_test_data().await {
        Ok(()) => Ok(Json(json!({ "ok": true }))),
        Err(_) => Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into()),
    }
}

/// POST /api/plugin/:id/data — выполнить декларативную привязку данных (DataView).
pub async fn run_data(
    Path(id): Path<String>,
    Json(ctx): Json<PluginRunContext>,
) -> Result<Json<DrilldownResponse>, (axum::http::StatusCode, Json<serde_json::Value>)> {
    match service::run_data(&id, &ctx).await {
        Ok(resp) => Ok(Json(resp)),
        Err(e) => Err((
            axum::http::StatusCode::BAD_REQUEST,
            Json(json!({ "error": e.to_string() })),
        )),
    }
}

/// GET /api/plugin/:id/stats?days=N — статистика запусков плагина (сводка + последние).
pub async fn stats(
    Path(id): Path<String>,
    Query(q): Query<DaysQuery>,
) -> Result<Json<PluginStats>, ApiError> {
    let days = q.days.unwrap_or(7).clamp(1, 365);
    match service::stats(&id, days).await {
        Ok(stats) => Ok(Json(stats)),
        Err(_) => Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into()),
    }
}

/// GET /api/plugin/runs/summary?days=N — краткие сводки по всем плагинам (для реестра).
pub async fn runs_summary(
    Query(q): Query<DaysQuery>,
) -> Result<Json<Vec<PluginRunBrief>>, ApiError> {
    let days = q.days.unwrap_or(7).clamp(1, 365);
    match service::runs_summary(days).await {
        Ok(items) => Ok(Json(items)),
        Err(_) => Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into()),
    }
}

/// GET /api/plugin/:id/export — скачать плагин как zip-архив переносимого бандла.
pub async fn export(Path(id): Path<String>) -> Result<impl IntoResponse, ApiError> {
    match service::export(&id).await {
        Ok((filename, bytes)) => Ok((
            [
                (header::CONTENT_TYPE, "application/zip".to_string()),
                (
                    header::CONTENT_DISPOSITION,
                    format!("attachment; filename=\"{filename}\""),
                ),
            ],
            bytes,
        )),
        Err(_) => Err(axum::http::StatusCode::NOT_FOUND.into()),
    }
}

/// POST /api/plugin/import — импортировать плагин из zip-архива (сырые байты в теле).
pub async fn import(
    body: Bytes,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, Json<serde_json::Value>)> {
    match service::import(&body).await {
        Ok(outcome) => Ok(Json(json!({
            "ok": outcome.id.is_some(),
            "id": outcome.id,
            "code": outcome.code,
            "validate": outcome.report,
        }))),
        Err(e) => Err((
            axum::http::StatusCode::BAD_REQUEST,
            Json(json!({ "error": e.to_string() })),
        )),
    }
}

/// POST /api/plugin/:id/invoke: вызвать экспортированную функцию server_script.
pub async fn invoke(
    Path(id): Path<String>,
    Json(request): Json<PluginInvokeRequest>,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, Json<serde_json::Value>)> {
    match service::invoke(&id, request).await {
        Ok((value, logs)) => Ok(Json(json!({ "ok": true, "result": value, "logs": logs }))),
        Err(e) => {
            // `error` остаётся строкой (совместимость с фронтендом), `error_detail`
            // несёт структуру { stage, message, stack } для UI-раннера и LLM-агента.
            let detail = e.downcast_ref::<PluginError>().cloned();
            Err((
                axum::http::StatusCode::BAD_REQUEST,
                Json(json!({ "error": e.to_string(), "error_detail": detail })),
            ))
        }
    }
}

/// POST /api/plugin/:id/publish — опубликовать текущую сохранённую версию в S3.
pub async fn publish_to_s3(
    Path(id): Path<String>,
) -> Result<Json<PluginPublishResult>, (axum::http::StatusCode, Json<serde_json::Value>)> {
    match publish::publish(&id).await {
        Ok(result) => Ok(Json(result)),
        Err(e) => Err((
            axum::http::StatusCode::BAD_REQUEST,
            Json(json!({ "error": e.to_string() })),
        )),
    }
}

/// GET /api/plugin/updates — сравнить локальные версии плагинов с каталогом в S3.
pub async fn check_updates(
) -> Result<Json<Vec<PluginUpdateStatus>>, (axum::http::StatusCode, Json<serde_json::Value>)> {
    match publish::check_updates().await {
        Ok(rows) => Ok(Json(rows)),
        Err(e) => Err((
            axum::http::StatusCode::BAD_REQUEST,
            Json(json!({ "error": e.to_string() })),
        )),
    }
}

/// GET /api/plugin/catalog — весь каталог S3 (code -> последняя опубликованная версия),
/// включая коды, которых ещё нет локально (для вкладки «Доступные плагины»).
pub async fn get_catalog(
) -> Result<Json<PluginCatalog>, (axum::http::StatusCode, Json<serde_json::Value>)> {
    match publish::get_catalog().await {
        Ok(catalog) => Ok(Json(catalog)),
        Err(e) => Err((
            axum::http::StatusCode::BAD_REQUEST,
            Json(json!({ "error": e.to_string() })),
        )),
    }
}

/// POST /api/plugin/catalog/:code/install — скачать и установить локально плагин из каталога S3,
/// которого ещё нет в локальной таблице `plugin`.
///
/// Бандл, валидный на инстансе-источнике публикации, может не пройти валидацию здесь
/// (другая копия приложения — другая версия движка/рантайма). В этом случае
/// `import_bundle_onto` не создаёт запись (`outcome.id` = `None`) — это должно дойти до
/// клиента как ошибка (не 200 "успех"), иначе UI молча покажет «установлено», хотя
/// локальной записи не появилось.
pub async fn install_from_catalog(
    Path(code): Path<String>,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, Json<serde_json::Value>)> {
    match publish::install_from_catalog(&code).await {
        Ok(outcome) if outcome.id.is_some() => Ok(Json(json!({
            "ok": true,
            "id": outcome.id,
            "code": outcome.code,
        }))),
        Ok(outcome) => {
            let message = outcome
                .report
                .errors
                .first()
                .map(|e| e.to_string())
                .unwrap_or_else(|| "unknown plugin validation error".to_string());
            Err((
                axum::http::StatusCode::BAD_REQUEST,
                Json(json!({
                    "error": format!("Plugin {} failed validation on install: {message}", outcome.code),
                    "validate": outcome.report,
                })),
            ))
        }
        Err(e) => Err((
            axum::http::StatusCode::BAD_REQUEST,
            Json(json!({ "error": e.to_string() })),
        )),
    }
}

/// POST /api/plugin/:id/apply-update — скачать и применить опубликованную версию из S3.
pub async fn apply_update(
    Path(id): Path<String>,
    Json(req): Json<PluginApplyUpdateRequest>,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, Json<serde_json::Value>)> {
    match publish::apply_update(&id, req.expected_remote_version).await {
        Ok(outcome) => Ok(Json(json!({
            "ok": outcome.id.is_some(),
            "id": outcome.id,
            "code": outcome.code,
        }))),
        Err(e) => Err((
            axum::http::StatusCode::BAD_REQUEST,
            Json(json!({ "error": e.to_string() })),
        )),
    }
}

/// GET /api/plugin/migration-version — текущий номер применённой миграции БД
/// (для ручной сверки с `PluginManifest.built_for_migration` на странице plugin_dev).
pub async fn migration_version(
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, Json<serde_json::Value>)> {
    match crate::shared::data::migration_runner::current_migration_version().await {
        Ok(version) => Ok(Json(json!({ "version": version }))),
        Err(e) => Err((
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )),
    }
}

/// POST /api/plugin/:id/dev-invoke: developer invoke for draft/disabled plugins.
pub async fn dev_invoke(
    Path(id): Path<String>,
    Json(request): Json<PluginInvokeRequest>,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, Json<serde_json::Value>)> {
    match service::dev_invoke(&id, request).await {
        Ok((value, logs)) => Ok(Json(json!({ "ok": true, "result": value, "logs": logs }))),
        Err(e) => {
            let detail = e.downcast_ref::<PluginError>().cloned();
            Err((
                axum::http::StatusCode::BAD_REQUEST,
                Json(json!({ "error": e.to_string(), "error_detail": detail })),
            ))
        }
    }
}
