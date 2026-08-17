use axum::{
    middleware,
    routing::{get, post, put},
    Router,
};

use super::handlers;
use crate::shared::app_state::AppState;
use crate::system::auth;

/// Конфигурация системных роутов приложения
pub fn configure_system_routes() -> Router<AppState> {
    Router::new()
        // ========================================
        // HEALTH CHECK
        // ========================================
        .route("/health", get(|| async { "ok" }))
        // ========================================
        // SYSTEM AUTH ROUTES (PUBLIC)
        // ========================================
        // ========================================
        // MAINTENANCE MODE
        // ========================================
        // Статус — публичный: страницу-заглушку надо показать и тому, кто ещё
        // не вошёл. Включение и снятие — только админ.
        .route(
            "/api/system/maintenance",
            get(handlers::maintenance::get_status),
        )
        .route(
            "/api/system/maintenance/enable",
            post(handlers::maintenance::enable)
                .layer(middleware::from_fn(auth::middleware::require_admin)),
        )
        .route(
            "/api/system/maintenance/disable",
            post(handlers::maintenance::disable)
                .layer(middleware::from_fn(auth::middleware::require_admin)),
        )
        .route("/api/system/auth/login", post(handlers::auth::login))
        .route("/api/system/auth/refresh", post(handlers::auth::refresh))
        .route("/api/system/auth/logout", post(handlers::auth::logout))
        // System auth routes (protected)
        .route(
            "/api/system/auth/me",
            get(handlers::auth::current_user)
                .layer(middleware::from_fn(auth::middleware::require_auth)),
        )
        // ========================================
        // SYSTEM USERS MANAGEMENT (admin only)
        // ========================================
        .route(
            "/api/system/users",
            get(handlers::users::list)
                .post(handlers::users::create)
                .layer(middleware::from_fn(auth::middleware::require_admin)),
        )
        .route(
            "/api/system/users/:id",
            get(handlers::users::get_by_id)
                .put(handlers::users::update)
                .delete(handlers::users::delete)
                .layer(middleware::from_fn(auth::middleware::require_admin)),
        )
        .route(
            "/api/system/users/:id/change-password",
            post(handlers::users::change_password)
                .layer(middleware::from_fn(auth::middleware::require_auth)),
        )
        // ========================================
        // SYSTEM ROLES MANAGEMENT (admin only)
        // ========================================
        .route(
            "/api/system/roles",
            get(handlers::roles::list_roles)
                .post(handlers::roles::create_role)
                .layer(middleware::from_fn(auth::middleware::require_admin)),
        )
        .route(
            "/api/system/roles/:id",
            put(handlers::roles::update_role)
                .delete(handlers::roles::delete_role)
                .layer(middleware::from_fn(auth::middleware::require_admin)),
        )
        .route(
            "/api/system/roles/:id/permissions",
            get(handlers::roles::get_role_permissions)
                .put(handlers::roles::update_role_permissions)
                .layer(middleware::from_fn(auth::middleware::require_admin)),
        )
        .route(
            "/api/system/scopes",
            get(handlers::audit::list_scopes)
                .layer(middleware::from_fn(auth::middleware::require_admin)),
        )
        // ========================================
        // AUDIT (admin only)
        // ========================================
        .route(
            "/api/system/audit/routes",
            get(handlers::audit::list_routes)
                .layer(middleware::from_fn(auth::middleware::require_admin)),
        )
        .route(
            "/api/system/audit/violations",
            get(handlers::audit::list_violations)
                .layer(middleware::from_fn(auth::middleware::require_admin)),
        )
        .route(
            "/api/system/runtime-info",
            get(handlers::runtime_info::get_runtime_info)
                .layer(middleware::from_fn(auth::middleware::require_auth)),
        )
        // ========================================
        // PROJECT METRICS (admin only)
        // Снимок состояния экземпляра и кодовой базы; собирается при старте.
        // ========================================
        .route(
            "/api/system/metrics/latest",
            get(handlers::metrics::latest)
                .layer(middleware::from_fn(auth::middleware::require_admin)),
        )
        .route(
            "/api/system/metrics/series",
            get(handlers::metrics::series)
                .layer(middleware::from_fn(auth::middleware::require_admin)),
        )
        .route(
            "/api/system/metrics/snapshots",
            get(handlers::metrics::snapshots)
                .layer(middleware::from_fn(auth::middleware::require_admin)),
        )
        .route(
            "/api/system/metrics/collect",
            post(handlers::metrics::collect)
                .layer(middleware::from_fn(auth::middleware::require_admin)),
        )
        // ========================================
        // SYSTEM FAVORITES
        // ========================================
        .route(
            "/api/system/favorites",
            get(handlers::favorites::list)
                .post(handlers::favorites::upsert)
                .layer(middleware::from_fn(auth::middleware::require_auth)),
        )
        .route(
            "/api/system/favorites/target",
            get(handlers::favorites::get_target)
                .layer(middleware::from_fn(auth::middleware::require_auth)),
        )
        .route(
            "/api/system/favorites/:id",
            put(handlers::favorites::update)
                .delete(handlers::favorites::delete)
                .layer(middleware::from_fn(auth::middleware::require_auth)),
        )
        // ========================================
        // SYSTEM TICKETS (обратная связь пользователей)
        // Пользователь видит только свои тикеты, админ — все (правило в service)
        // ========================================
        .route(
            "/api/system/tickets",
            get(handlers::tickets::list)
                .post(handlers::tickets::create)
                .layer(middleware::from_fn(auth::middleware::require_auth)),
        )
        .route(
            "/api/system/tickets/:id",
            get(handlers::tickets::get_details)
                .put(handlers::tickets::update)
                .delete(handlers::tickets::delete)
                .layer(middleware::from_fn(auth::middleware::require_auth)),
        )
        .route(
            "/api/system/tickets/:id/comments",
            get(handlers::tickets::list_comments)
                .post(handlers::tickets::add_comment)
                .layer(middleware::from_fn(auth::middleware::require_auth)),
        )
        .route(
            "/api/system/tickets/:id/attachments",
            post(handlers::tickets::upload_attachment)
                .layer(middleware::from_fn(auth::middleware::require_auth)),
        )
        .route(
            "/api/system/tickets/:id/attachments/:attachment_id/download",
            get(handlers::tickets::download_attachment)
                .layer(middleware::from_fn(auth::middleware::require_auth)),
        )
        .route(
            "/api/system/tickets/:id/attachments/:attachment_id",
            axum::routing::delete(handlers::tickets::delete_attachment)
                .layer(middleware::from_fn(auth::middleware::require_auth)),
        )
        // ========================================
        // SYSTEM PAGE HISTORY ("История открытых страниц")
        // ========================================
        .route(
            "/api/system/page-history",
            get(handlers::history::list)
                .post(handlers::history::record)
                .delete(handlers::history::clear)
                .layer(middleware::from_fn(auth::middleware::require_auth)),
        )
        // ========================================
        // EXTERNAL API TRAFFIC STATS (вкладка «Внешний API» на sys_tasks)
        // admin-only: строки содержат IP и query-строки потребителей
        // ========================================
        .route(
            "/api/sys/ext-api/history",
            get(handlers::ext_api_log::ext_api_history)
                .layer(middleware::from_fn(auth::middleware::require_admin)),
        )
        .route(
            "/api/sys/ext-api/summary",
            get(handlers::ext_api_log::ext_api_summary)
                .layer(middleware::from_fn(auth::middleware::require_admin)),
        )
        .route(
            "/api/sys/ext-api/recent",
            get(handlers::ext_api_log::ext_api_recent)
                .layer(middleware::from_fn(auth::middleware::require_admin)),
        )
        // ========================================
        // SYSTEM TASKS (sys_tasks) ROUTES
        // ========================================
        .route(
            "/api/sys/tasks",
            get(handlers::tasks::list_scheduled_tasks)
                .post(handlers::tasks::create_scheduled_task)
                .layer(middleware::from_fn(auth::middleware::require_admin)),
        )
        .route(
            "/api/sys/tasks/task_types",
            get(handlers::tasks::list_task_types)
                .layer(middleware::from_fn(auth::middleware::require_auth)),
        )
        .route(
            "/api/sys/tasks/runs/recent",
            get(handlers::tasks::list_recent_runs)
                .layer(middleware::from_fn(auth::middleware::require_auth)),
        )
        .route(
            "/api/sys/tasks/history",
            get(handlers::tasks::task_history)
                .layer(middleware::from_fn(auth::middleware::require_auth)),
        )
        .route(
            "/api/sys/tasks/runs/active/progress",
            get(handlers::tasks::list_active_runs_with_progress)
                .layer(middleware::from_fn(auth::middleware::require_auth)),
        )
        .route(
            "/api/sys/tasks/runs/active",
            get(handlers::tasks::list_active_runs)
                .layer(middleware::from_fn(auth::middleware::require_auth)),
        )
        .route(
            "/api/sys/tasks/:id",
            get(handlers::tasks::get_scheduled_task)
                .put(handlers::tasks::update_scheduled_task)
                .delete(handlers::tasks::delete_scheduled_task)
                .layer(middleware::from_fn(auth::middleware::require_admin)),
        )
        .route(
            "/api/sys/tasks/:id/toggle_enabled",
            post(handlers::tasks::toggle_scheduled_task_enabled)
                .layer(middleware::from_fn(auth::middleware::require_admin)),
        )
        .route(
            "/api/sys/tasks/:id/run",
            post(handlers::tasks::run_task_now)
                .layer(middleware::from_fn(auth::middleware::require_admin)),
        )
        .route(
            "/api/sys/tasks/:id/watermark",
            post(handlers::tasks::set_watermark)
                .layer(middleware::from_fn(auth::middleware::require_admin)),
        )
        .route(
            "/api/sys/tasks/:id/runs",
            get(handlers::tasks::list_task_runs)
                .layer(middleware::from_fn(auth::middleware::require_auth)),
        )
        .route(
            "/api/sys/tasks/runs/:session_id/abort",
            post(handlers::tasks::abort_task_run)
                .layer(middleware::from_fn(auth::middleware::require_admin)),
        )
        .route(
            "/api/sys/tasks/:id/progress/:session_id",
            get(handlers::tasks::get_task_progress)
                .layer(middleware::from_fn(auth::middleware::require_auth)),
        )
        .route(
            "/api/sys/tasks/:id/log/:session_id",
            get(handlers::tasks::get_task_log)
                .layer(middleware::from_fn(auth::middleware::require_auth)),
        )
        .route(
            "/api/sys/change-tokens",
            get(handlers::tasks::get_change_tokens)
                .layer(middleware::from_fn(auth::middleware::require_auth)),
        )
        .route(
            "/api/sys/scheduler/status",
            get(handlers::tasks::get_scheduler_status)
                .post(handlers::tasks::set_scheduler_status)
                .layer(middleware::from_fn(auth::middleware::require_admin)),
        )
        // ========================================
        // RAW JSON DEBUG STORAGE
        // ========================================
        .route(
            "/api/sys/raw-storage/status",
            get(handlers::raw_storage::get_status)
                .layer(middleware::from_fn(auth::middleware::require_admin)),
        )
        .route(
            "/api/sys/raw-storage/settings",
            post(handlers::raw_storage::set_settings)
                .layer(middleware::from_fn(auth::middleware::require_admin)),
        )
        .route(
            "/api/sys/raw-storage/cleanup/preview",
            post(handlers::raw_storage::cleanup_preview)
                .layer(middleware::from_fn(auth::middleware::require_admin)),
        )
        .route(
            "/api/sys/raw-storage/cleanup",
            post(handlers::raw_storage::cleanup)
                .layer(middleware::from_fn(auth::middleware::require_admin)),
        )
        .route(
            "/api/sys/raw-storage/vacuum",
            get(handlers::raw_storage::get_vacuum_status)
                .post(handlers::raw_storage::run_vacuum)
                .layer(middleware::from_fn(auth::middleware::require_admin)),
        )
        .route(
            "/api/sys/raw-storage/wal-checkpoint",
            post(handlers::raw_storage::truncate_wal)
                .layer(middleware::from_fn(auth::middleware::require_admin)),
        )
        // ========================================
        // SYSTEM S3 FILE MANAGER
        // ========================================
        .route(
            "/api/sys/s3/files",
            get(handlers::s3::list_files)
                .post(handlers::s3::upload_file)
                .layer(middleware::from_fn(auth::middleware::require_admin)),
        )
        .route(
            "/api/sys/s3/files/:id/download",
            get(handlers::s3::download_file)
                .layer(middleware::from_fn(auth::middleware::require_admin)),
        )
        .route(
            "/api/sys/s3/files/:id",
            axum::routing::delete(handlers::s3::delete_file)
                .layer(middleware::from_fn(auth::middleware::require_admin)),
        )
        // ========================================
        // DATASETS: TRANSFER BETWEEN INSTANCES
        // ========================================
        .route(
            "/api/sys/datasets/status",
            get(handlers::datasets::get_status)
                .layer(middleware::from_fn(auth::middleware::require_admin)),
        )
        .route(
            "/api/sys/datasets/catalog",
            get(handlers::datasets::get_catalog)
                .layer(middleware::from_fn(auth::middleware::require_admin)),
        )
        .route(
            "/api/sys/datasets/snapshots",
            post(handlers::datasets::create_snapshot)
                .layer(middleware::from_fn(auth::middleware::require_admin)),
        )
        .route(
            "/api/sys/datasets/snapshots/:id",
            get(handlers::datasets::get_manifest)
                .delete(handlers::datasets::delete_snapshot)
                .layer(middleware::from_fn(auth::middleware::require_admin)),
        )
        .route(
            "/api/sys/datasets/snapshots/:id/download",
            get(handlers::datasets::download_snapshot)
                .layer(middleware::from_fn(auth::middleware::require_admin)),
        )
        .route(
            "/api/sys/datasets/restore/preview",
            post(handlers::datasets::restore_preview)
                .layer(middleware::from_fn(auth::middleware::require_admin)),
        )
        .route(
            "/api/sys/datasets/restore",
            post(handlers::datasets::restore_apply)
                .layer(middleware::from_fn(auth::middleware::require_admin)),
        )
        .route(
            "/api/sys/datasets/restore/upload",
            post(handlers::datasets::restore_upload)
                .layer(middleware::from_fn(auth::middleware::require_admin)),
        )
        .route(
            "/api/sys/datasets/history",
            get(handlers::datasets::get_history)
                .layer(middleware::from_fn(auth::middleware::require_admin)),
        )
        // Фоновые задачи переноса. `/jobs/active` объявлен ДО `/jobs/:job_id`
        // ради читаемости; axum разводит статический сегмент и параметр сам.
        .route(
            "/api/sys/datasets/jobs/active",
            get(handlers::datasets::get_active_job)
                .layer(middleware::from_fn(auth::middleware::require_admin)),
        )
        .route(
            "/api/sys/datasets/jobs/:job_id",
            get(handlers::datasets::get_job)
                .layer(middleware::from_fn(auth::middleware::require_admin)),
        )
        .route(
            "/api/sys/datasets/jobs/:job_id/cancel",
            post(handlers::datasets::cancel_job)
                .layer(middleware::from_fn(auth::middleware::require_admin)),
        )
        // ========================================
        // UTILITIES
        // ========================================
        // Logs handlers
        .route(
            "/api/logs",
            get(handlers::logs::list_all)
                .post(handlers::logs::create)
                .delete(handlers::logs::clear_all),
        )
        // Form Settings handlers
        .route(
            "/api/form-settings/:form_key",
            get(handlers::form_settings::get_settings),
        )
        .route(
            "/api/form-settings",
            post(handlers::form_settings::save_settings),
        )
}
