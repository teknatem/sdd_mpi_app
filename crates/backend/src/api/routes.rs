use axum::{
    body::Body,
    extract::Request,
    middleware::{self, Next},
    routing::{delete, get, post, put},
    Router,
};

use super::handlers;
use crate::shared::app_state::AppState;
use crate::system::auth::middleware::{check_scope, check_scope_read};

/// Business routes configuration.
/// Each aggregate group is wrapped with require_scope_auto for its scope.
/// Projections, usecases, and dashboards require authentication but no scope.
pub fn configure_business_routes() -> Router<AppState> {
    Router::new()
        .merge(a001_routes())
        .merge(a002_routes())
        .merge(a003_routes())
        .merge(a004_routes())
        .merge(a005_routes())
        .merge(a006_routes())
        .merge(a007_routes())
        .merge(a008_routes())
        .merge(a009_routes())
        .merge(a010_routes())
        .merge(a011_routes())
        .merge(a012_routes())
        .merge(a013_routes())
        .merge(a014_routes())
        .merge(a015_routes())
        .merge(a016_routes())
        .merge(a017_routes())
        .merge(a018_routes())
        .merge(llm_skills_routes())
        .merge(llm_tools_routes())
        .merge(llm_quality_routes())
        .merge(a019_routes())
        .merge(a020_routes())
        .merge(a021_routes())
        .merge(a022_routes())
        .merge(a023_routes())
        .merge(a024_routes())
        .merge(a025_routes())
        .merge(a026_routes())
        .merge(a036_routes())
        .merge(a037_routes())
        .merge(a040_routes())
        .merge(a041_routes())
        .merge(a038_routes())
        .merge(a039_routes())
        .merge(a034_routes())
        .merge(a035_routes())
        .merge(a027_routes())
        .merge(a028_routes())
        .merge(a029_routes())
        .merge(a030_routes())
        .merge(a031_routes())
        .merge(a032_routes())
        .merge(a033_routes())
        .merge(a042_routes())
        .merge(a043_routes())
        // External integrations (API-key auth, no JWT required)
        .merge(ext_routes())
        // Usecases — each with their own scope
        .merge(u501_routes())
        .merge(u502_routes())
        .merge(u503_routes())
        .merge(u504_routes())
        .merge(u505_routes())
        .merge(u506_routes())
        .merge(u507_routes())
        .merge(u508_routes())
        // Projections — each with their own scope
        .merge(p900_routes())
        .merge(p901_routes())
        .merge(p902_routes())
        .merge(p903_routes())
        .merge(p904_routes())
        .merge(p905_routes())
        .merge(p906_routes())
        .merge(p907_routes())
        .merge(p908_routes())
        .merge(p912_routes())
        .merge(p913_routes())
        .merge(p914_routes())
        .merge(p915_routes())
        // System views with scopes
        .merge(quality_routes())
        .merge(dashboard_routes())
        .merge(data_view_routes())
        .merge(bi_timeline_routes())
        .merge(general_ledger_routes())
        .merge(kb_read_routes())
        .merge(refs_routes())
        .merge(misc_routes())
        // Plugins subsystem (use: auth-only, manage: admin-only)
        .merge(plugin_routes())
        // Механизм Процессов (admin-only целиком)
        .merge(process_routes())
        // YM maintenance (admin-only)
        .merge(ym_maintenance_routes())
}

// ============================================================================
// YM-обслуживание — консолидация подключений к модели «подключение = бизнес» (admin-only)
// ============================================================================

fn ym_maintenance_routes() -> Router<AppState> {
    use crate::system::auth::middleware::require_admin;

    Router::new()
        .route(
            "/api/ym/consolidate-connections",
            post(handlers::ym_consolidation::consolidate_ym_connections),
        )
        .layer(middleware::from_fn(
            |req: Request<Body>, next: Next| async move { require_admin(req, next).await },
        ))
}

// ============================================================================
// Механизм Процессов — определения, экземпляры, журналы (ADR-0011)
// ============================================================================
// Admin-only целиком, и это свойство предмета, а не осторожность: активация
// Процесса означает, что система начнёт менять данные сама, а сухой прогон
// Этапа исполняет чужой mjs.

fn process_routes() -> Router<AppState> {
    use crate::system::auth::middleware::require_admin;

    Router::new()
        .route(
            "/api/processes/actions",
            get(handlers::processes::list_actions),
        )
        .route(
            "/api/processes/event-kinds",
            get(handlers::processes::list_event_kinds),
        )
        .route(
            "/api/processes/events",
            get(handlers::processes::list_events),
        )
        .route(
            "/api/processes/stages",
            get(handlers::processes::list_stages).post(handlers::processes::save_stage),
        )
        .route(
            "/api/processes/stages/full",
            get(handlers::processes::list_stages_full),
        )
        .route(
            "/api/processes/stages/:code/versions",
            get(handlers::processes::list_stage_versions),
        )
        .route(
            "/api/processes/stages/:code/versions/:version",
            get(handlers::processes::get_stage).delete(handlers::processes::delete_stage),
        )
        .route(
            "/api/processes/stages/:code/versions/:version/activate",
            post(handlers::processes::activate_stage),
        )
        .route(
            "/api/processes/stages/:code/versions/:version/dry-run",
            post(handlers::processes::dry_run_stage),
        )
        .route(
            "/api/processes/definitions",
            get(handlers::processes::list_processes).post(handlers::processes::save_process),
        )
        .route(
            "/api/processes/definitions/full",
            get(handlers::processes::list_processes_full),
        )
        .route(
            "/api/processes/definitions/:code/versions",
            get(handlers::processes::list_process_versions),
        )
        .route(
            "/api/processes/definitions/:code/versions/:version",
            get(handlers::processes::get_process).delete(handlers::processes::delete_process),
        )
        .route(
            "/api/processes/definitions/:code/versions/:version/activation-plan",
            get(handlers::processes::activation_plan),
        )
        .route(
            "/api/processes/definitions/:code/versions/:version/activate",
            post(handlers::processes::activate_process),
        )
        .route(
            "/api/processes/definitions/:code/deactivate",
            post(handlers::processes::deactivate_process),
        )
        .route(
            "/api/processes/instances",
            get(handlers::processes::list_instances),
        )
        .route(
            "/api/processes/instances/:id",
            get(handlers::processes::get_instance),
        )
        .route(
            "/api/processes/instances/:id/human-done",
            post(handlers::processes::human_action_done),
        )
        .route(
            "/api/processes/effects",
            get(handlers::processes::list_effects),
        )
        .route("/api/processes/tick", post(handlers::processes::tick))
        .layer(middleware::from_fn(
            |req: Request<Body>, next: Next| async move { require_admin(req, next).await },
        ))
}

// ============================================================================
// Plugins subsystem — надстройка над платформой
// (использование — auth-only, управление — admin-only)
// ============================================================================

fn plugin_routes() -> Router<AppState> {
    plugin_use_routes().merge(plugin_admin_routes())
}

/// Использование плагинов — доступно любому аутентифицированному пользователю
/// (просмотр списка активных плагинов и их запуск).
fn plugin_use_routes() -> Router<AppState> {
    use crate::system::auth::middleware::require_auth;

    Router::new()
        .route("/api/plugin", get(handlers::plugins::list))
        .route("/api/plugin/:id", get(handlers::plugins::get_by_id))
        .route("/api/plugin/:id/invoke", post(handlers::plugins::invoke))
        .route("/api/plugin/:id/data", post(handlers::plugins::run_data))
        .layer(middleware::from_fn(
            |req: Request<Body>, next: Next| async move { require_auth(req, next).await },
        ))
}

/// Управление плагинами — только администратор (создание/редактирование,
/// импорт/экспорт, публикация в S3, обновление с сервера, отладка, статистика).
fn plugin_admin_routes() -> Router<AppState> {
    use crate::system::auth::middleware::require_admin;

    Router::new()
        .route("/api/plugin", post(handlers::plugins::upsert))
        .route("/api/plugin/all", get(handlers::plugins::list_all))
        .route("/api/plugin/validate", post(handlers::plugins::validate))
        .route(
            "/api/plugin/smoke-test",
            post(handlers::plugins::smoke_test),
        )
        .route("/api/plugin/testdata", post(handlers::plugins::testdata))
        .route("/api/plugin/import", post(handlers::plugins::import))
        .route(
            "/api/plugin/runs/summary",
            get(handlers::plugins::runs_summary),
        )
        .route("/api/plugin/updates", get(handlers::plugins::check_updates))
        .route("/api/plugin/catalog", get(handlers::plugins::get_catalog))
        .route(
            "/api/plugin/catalog/:code/install",
            post(handlers::plugins::install_from_catalog),
        )
        .route(
            "/api/plugin/migration-version",
            get(handlers::plugins::migration_version),
        )
        .route("/api/plugin/:id", delete(handlers::plugins::delete))
        .route(
            "/api/plugin/:id/rating",
            post(handlers::plugins::set_rating),
        )
        .route("/api/plugin/:id/export", get(handlers::plugins::export))
        .route("/api/plugin/:id/stats", get(handlers::plugins::stats))
        .route(
            "/api/plugin/:id/dev-invoke",
            post(handlers::plugins::dev_invoke),
        )
        .route(
            "/api/plugin/:id/publish",
            post(handlers::plugins::publish_to_s3),
        )
        .route(
            "/api/plugin/:id/apply-update",
            post(handlers::plugins::apply_update),
        )
        .layer(middleware::from_fn(
            |req: Request<Body>, next: Next| async move { require_admin(req, next).await },
        ))
}

// ============================================================================
// Универсальный резолвер представлений ссылок (*_ref) — только аутентификация
// ============================================================================

fn refs_routes() -> Router<AppState> {
    Router::new()
        .route("/api/refs/resolve", get(handlers::refs::resolve))
        .layer(middleware::from_fn(
            |req: Request<Body>, next: Next| async move {
                crate::system::auth::middleware::require_auth(req, next).await
            },
        ))
}

// ============================================================================
// Aggregates A001–A025 (each wrapped with require_scope_auto)
// ============================================================================

fn a001_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/connection_1c",
            get(handlers::a001_connection_1c::list_all).post(handlers::a001_connection_1c::upsert),
        )
        .route(
            "/api/connection_1c/list",
            get(handlers::a001_connection_1c::list_paginated),
        )
        .route(
            "/api/connection_1c/:id",
            get(handlers::a001_connection_1c::get_by_id)
                .delete(handlers::a001_connection_1c::delete),
        )
        .route(
            "/api/connection_1c/test",
            post(handlers::a001_connection_1c::test_connection),
        )
        .route(
            "/api/connection_1c/testdata",
            post(handlers::a001_connection_1c::insert_test_data),
        )
        .layer(middleware::from_fn(
            |req: Request<Body>, next: Next| async move {
                check_scope("a001_connection_1c", req, next).await
            },
        ))
}

fn a002_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/organization",
            get(handlers::a002_organization::list_all).post(handlers::a002_organization::upsert),
        )
        .route(
            "/api/organization/:id",
            get(handlers::a002_organization::get_by_id).delete(handlers::a002_organization::delete),
        )
        .route(
            "/api/organization/testdata",
            post(handlers::a002_organization::insert_test_data),
        )
        .layer(middleware::from_fn(
            |req: Request<Body>, next: Next| async move {
                check_scope("a002_organization", req, next).await
            },
        ))
}

fn a003_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/counterparty",
            get(handlers::a003_counterparty::list_all).post(handlers::a003_counterparty::upsert),
        )
        .route(
            "/api/counterparty/:id",
            get(handlers::a003_counterparty::get_by_id).delete(handlers::a003_counterparty::delete),
        )
        .layer(middleware::from_fn(
            |req: Request<Body>, next: Next| async move {
                check_scope("a003_counterparty", req, next).await
            },
        ))
}

fn a004_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/nomenclature",
            get(handlers::a004_nomenclature::list_all).post(handlers::a004_nomenclature::upsert),
        )
        .route(
            "/api/nomenclature/:id",
            get(handlers::a004_nomenclature::get_by_id).delete(handlers::a004_nomenclature::delete),
        )
        .route(
            "/api/nomenclature/import-excel",
            post(handlers::a004_nomenclature::import_excel),
        )
        .route(
            "/api/nomenclature/dimensions",
            get(handlers::a004_nomenclature::get_dimensions),
        )
        .route(
            "/api/nomenclature/:id/orders",
            get(handlers::a004_nomenclature::get_orders),
        )
        .route(
            "/api/nomenclature/search",
            get(handlers::a004_nomenclature::search_by_article),
        )
        .route(
            "/api/nomenclature/search-by-barcode",
            get(handlers::a004_nomenclature::search_by_barcode),
        )
        .route(
            "/api/a004/nomenclature",
            get(handlers::a004_nomenclature::list_paginated),
        )
        .layer(middleware::from_fn(
            |req: Request<Body>, next: Next| async move {
                check_scope("a004_nomenclature", req, next).await
            },
        ))
}

fn a005_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/marketplace",
            get(handlers::a005_marketplace::list_all).post(handlers::a005_marketplace::upsert),
        )
        .route(
            "/api/marketplace/:id",
            get(handlers::a005_marketplace::get_by_id).delete(handlers::a005_marketplace::delete),
        )
        .route(
            "/api/marketplace/testdata",
            post(handlers::a005_marketplace::insert_test_data),
        )
        .layer(middleware::from_fn(
            |req: Request<Body>, next: Next| async move {
                check_scope("a005_marketplace", req, next).await
            },
        ))
}

fn a006_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/connection_mp",
            get(handlers::a006_connection_mp::list_all).post(handlers::a006_connection_mp::upsert),
        )
        .route(
            "/api/connection_mp/:id",
            get(handlers::a006_connection_mp::get_by_id)
                .delete(handlers::a006_connection_mp::delete),
        )
        .route(
            "/api/connection_mp/test",
            post(handlers::a006_connection_mp::test_connection),
        )
        .route(
            "/api/connection_mp/seller_info",
            post(handlers::a006_connection_mp::seller_info),
        )
        .layer(middleware::from_fn(
            |req: Request<Body>, next: Next| async move {
                check_scope("a006_connection_mp", req, next).await
            },
        ))
}

fn a007_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/marketplace_product",
            get(handlers::a007_marketplace_product::list_all)
                .post(handlers::a007_marketplace_product::upsert),
        )
        .route(
            "/api/marketplace_product/:id",
            get(handlers::a007_marketplace_product::get_by_id)
                .delete(handlers::a007_marketplace_product::delete),
        )
        .route(
            "/api/marketplace_product/testdata",
            post(handlers::a007_marketplace_product::insert_test_data),
        )
        .route(
            "/api/a007/marketplace-product",
            get(handlers::a007_marketplace_product::list_paginated),
        )
        .layer(middleware::from_fn(
            |req: Request<Body>, next: Next| async move {
                check_scope("a007_marketplace_product", req, next).await
            },
        ))
}

fn a008_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/marketplace_sales",
            get(handlers::a008_marketplace_sales::list_all)
                .post(handlers::a008_marketplace_sales::upsert),
        )
        .route(
            "/api/marketplace_sales/:id",
            get(handlers::a008_marketplace_sales::get_by_id)
                .delete(handlers::a008_marketplace_sales::delete),
        )
        .layer(middleware::from_fn(
            |req: Request<Body>, next: Next| async move {
                check_scope("a008_marketplace_sales", req, next).await
            },
        ))
}

fn a009_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/ozon_returns",
            get(handlers::a009_ozon_returns::list_all).post(handlers::a009_ozon_returns::upsert),
        )
        .route(
            "/api/ozon_returns/:id",
            get(handlers::a009_ozon_returns::get_by_id).delete(handlers::a009_ozon_returns::delete),
        )
        .route(
            "/api/a009/ozon-returns/:id/post",
            post(handlers::a009_ozon_returns::post_ozon_return),
        )
        .route(
            "/api/a009/ozon-returns/:id/unpost",
            post(handlers::a009_ozon_returns::unpost_ozon_return),
        )
        .layer(middleware::from_fn(
            |req: Request<Body>, next: Next| async move {
                check_scope("a009_ozon_returns", req, next).await
            },
        ))
}

fn a010_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/a010/ozon-fbs-posting",
            get(handlers::a010_ozon_fbs_posting::list_postings),
        )
        .route(
            "/api/a010/ozon-fbs-posting/:id",
            get(handlers::a010_ozon_fbs_posting::get_posting_detail),
        )
        .route(
            "/api/a010/raw/:ref_id",
            get(handlers::a010_ozon_fbs_posting::get_raw_json),
        )
        .route(
            "/api/a010/ozon-fbs-posting/:id/post",
            post(handlers::a010_ozon_fbs_posting::post_document),
        )
        .route(
            "/api/a010/ozon-fbs-posting/:id/unpost",
            post(handlers::a010_ozon_fbs_posting::unpost_document),
        )
        .route(
            "/api/a010/ozon-fbs-posting/post-period",
            post(handlers::a010_ozon_fbs_posting::post_period),
        )
        .layer(middleware::from_fn(
            |req: Request<Body>, next: Next| async move {
                check_scope("a010_ozon_fbs_posting", req, next).await
            },
        ))
}

fn a011_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/a011/ozon-fbo-posting",
            get(handlers::a011_ozon_fbo_posting::list_postings),
        )
        .route(
            "/api/a011/ozon-fbo-posting/:id",
            get(handlers::a011_ozon_fbo_posting::get_posting_detail),
        )
        .route(
            "/api/a011/ozon-fbo-posting/:id/post",
            post(handlers::a011_ozon_fbo_posting::post_document),
        )
        .route(
            "/api/a011/ozon-fbo-posting/:id/unpost",
            post(handlers::a011_ozon_fbo_posting::unpost_document),
        )
        .route(
            "/api/a011/ozon-fbo-posting/post-period",
            post(handlers::a011_ozon_fbo_posting::post_period),
        )
        .layer(middleware::from_fn(
            |req: Request<Body>, next: Next| async move {
                check_scope("a011_ozon_fbo_posting", req, next).await
            },
        ))
}

fn a012_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/a012/wb-sales",
            get(handlers::a012_wb_sales::list_sales),
        )
        .route(
            "/api/a012/wb-sales/:id",
            get(handlers::a012_wb_sales::get_sale_detail),
        )
        .route(
            "/api/a012/wb-sales/search-by-srid",
            get(handlers::a012_wb_sales::search_by_srid),
        )
        .route(
            "/api/a012/raw/:ref_id",
            get(handlers::a012_wb_sales::get_raw_json),
        )
        .route(
            "/api/a012/wb-sales/:id/post",
            post(handlers::a012_wb_sales::post_document),
        )
        .route(
            "/api/a012/wb-sales/:id/unpost",
            post(handlers::a012_wb_sales::unpost_document),
        )
        .route(
            "/api/a012/wb-sales/post-period",
            post(handlers::a012_wb_sales::post_period),
        )
        .route(
            "/api/a012/wb-sales/batch-post",
            post(handlers::a012_wb_sales::batch_post_documents),
        )
        .route(
            "/api/a012/wb-sales/batch-unpost",
            post(handlers::a012_wb_sales::batch_unpost_documents),
        )
        .route(
            "/api/a012/wb-sales/:id/projections",
            get(handlers::a012_wb_sales::get_projections),
        )
        .route(
            "/api/a012/wb-sales/:id/journal",
            get(handlers::a012_wb_sales::get_general_ledger_entries),
        )
        .route(
            "/api/a012/wb-sales/:id/advert-attribution",
            get(handlers::a012_wb_sales::get_advert_attribution),
        )
        .route(
            "/api/a012/wb-sales/:id/refresh-dealer-price",
            post(handlers::a012_wb_sales::refresh_dealer_price),
        )
        .route(
            "/api/a012/wb-sales/migrate-sale-id",
            post(handlers::a012_wb_sales::migrate_fill_sale_id),
        )
        .layer(middleware::from_fn(
            |req: Request<Body>, next: Next| async move {
                check_scope("a012_wb_sales", req, next).await
            },
        ))
}

fn a013_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/a013/ym-order",
            get(handlers::a013_ym_order::list_orders_fast),
        )
        .route(
            "/api/a013/ym-order/list",
            get(handlers::a013_ym_order::list_orders_fast),
        )
        .route(
            "/api/a013/ym-order/:id",
            get(handlers::a013_ym_order::get_order_detail),
        )
        .route(
            "/api/a013/raw/:ref_id",
            get(handlers::a013_ym_order::get_raw_json),
        )
        .route(
            "/api/a013/ym-order/:id/post",
            post(handlers::a013_ym_order::post_document),
        )
        .route(
            "/api/a013/ym-order/:id/unpost",
            post(handlers::a013_ym_order::unpost_document),
        )
        .route(
            "/api/a013/ym-order/:id/projections",
            get(handlers::a013_ym_order::get_projections),
        )
        .route(
            "/api/a013/ym-order/post-period",
            post(handlers::a013_ym_order::post_period),
        )
        .route(
            "/api/a013/ym-order/batch-post",
            post(handlers::a013_ym_order::batch_post_documents),
        )
        .route(
            "/api/a013/ym-order/batch-unpost",
            post(handlers::a013_ym_order::batch_unpost_documents),
        )
        .layer(middleware::from_fn(
            |req: Request<Body>, next: Next| async move {
                check_scope("a013_ym_order", req, next).await
            },
        ))
}

fn a014_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/ozon_transactions",
            get(handlers::a014_ozon_transactions::list_all),
        )
        .route(
            "/api/ozon_transactions/:id",
            get(handlers::a014_ozon_transactions::get_by_id)
                .delete(handlers::a014_ozon_transactions::delete),
        )
        .route(
            "/api/ozon_transactions/by-posting/:posting_number",
            get(handlers::a014_ozon_transactions::get_by_posting_number),
        )
        .route(
            "/api/a014/ozon-transactions/:id/post",
            post(handlers::a014_ozon_transactions::post_document),
        )
        .route(
            "/api/a014/ozon-transactions/:id/unpost",
            post(handlers::a014_ozon_transactions::unpost_document),
        )
        .route(
            "/api/a014/ozon-transactions/:id/projections",
            get(handlers::a014_ozon_transactions::get_projections),
        )
        .layer(middleware::from_fn(
            |req: Request<Body>, next: Next| async move {
                check_scope("a014_ozon_transactions", req, next).await
            },
        ))
}

fn a015_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/a015/wb-orders",
            get(handlers::a015_wb_orders::list_orders),
        )
        .route(
            "/api/a015/wb-orders/:id",
            get(handlers::a015_wb_orders::get_order_detail),
        )
        .route(
            "/api/a015/wb-orders/search-by-srid",
            get(handlers::a015_wb_orders::search_by_srid),
        )
        .route(
            "/api/a015/raw/:ref_id",
            get(handlers::a015_wb_orders::get_raw_json),
        )
        .route(
            "/api/a015/wb-orders/:id/delete",
            post(handlers::a015_wb_orders::delete_order),
        )
        .route(
            "/api/a015/wb-orders/:id/post",
            post(handlers::a015_wb_orders::post_order),
        )
        .route(
            "/api/a015/wb-orders/:id/unpost",
            post(handlers::a015_wb_orders::unpost_order),
        )
        .route(
            "/api/a015/wb-orders/:id/projections",
            get(handlers::a015_wb_orders::get_projections),
        )
        .layer(middleware::from_fn(
            |req: Request<Body>, next: Next| async move {
                check_scope("a015_wb_orders", req, next).await
            },
        ))
}

fn a016_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/a016/ym-returns",
            get(handlers::a016_ym_returns::list_returns),
        )
        .route(
            "/api/a016/ym-returns/source-order/:order_no",
            get(handlers::a016_ym_returns::get_source_order),
        )
        .route(
            "/api/a016/ym-returns/:id",
            get(handlers::a016_ym_returns::get_return_detail),
        )
        .route(
            "/api/a016/raw/:ref_id",
            get(handlers::a016_ym_returns::get_raw_json),
        )
        .route(
            "/api/a016/ym-returns/:id/post",
            post(handlers::a016_ym_returns::post_document),
        )
        .route(
            "/api/a016/ym-returns/:id/unpost",
            post(handlers::a016_ym_returns::unpost_document),
        )
        .route(
            "/api/a016/ym-returns/:id/projections",
            get(handlers::a016_ym_returns::get_projections),
        )
        .route(
            "/api/a016/ym-returns/post-period",
            post(handlers::a016_ym_returns::post_period),
        )
        .route(
            "/api/a016/ym-returns/batch-post",
            post(handlers::a016_ym_returns::batch_post_documents),
        )
        .route(
            "/api/a016/ym-returns/batch-unpost",
            post(handlers::a016_ym_returns::batch_unpost_documents),
        )
        .layer(middleware::from_fn(
            |req: Request<Body>, next: Next| async move {
                check_scope("a016_ym_returns", req, next).await
            },
        ))
}

fn a017_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/a017-llm-agent",
            get(handlers::a017_llm_agent::list_all).post(handlers::a017_llm_agent::upsert),
        )
        .route(
            "/api/a017-llm-agent/list",
            get(handlers::a017_llm_agent::list_paginated),
        )
        .route(
            "/api/a017-llm-agent/primary",
            get(handlers::a017_llm_agent::get_primary),
        )
        .route(
            "/api/a017-llm-agent/skills",
            get(handlers::a017_llm_agent::skills),
        )
        .route(
            "/api/a017-llm-agent/:id",
            get(handlers::a017_llm_agent::get_by_id).delete(handlers::a017_llm_agent::delete),
        )
        .route(
            "/api/a017-llm-agent/:id/test",
            post(handlers::a017_llm_agent::test_connection),
        )
        .route(
            "/api/a017-llm-agent/:id/fetch-models",
            post(handlers::a017_llm_agent::fetch_models),
        )
        .layer(middleware::from_fn(
            |req: Request<Body>, next: Next| async move {
                check_scope("a017_llm_agent", req, next).await
            },
        ))
}

fn a038_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/a038-llm-connection",
            get(handlers::a038_llm_connection::list_all)
                .post(handlers::a038_llm_connection::upsert),
        )
        .route(
            "/api/a038-llm-connection/list",
            get(handlers::a038_llm_connection::list_paginated),
        )
        .route(
            "/api/a038-llm-connection/primary",
            get(handlers::a038_llm_connection::get_primary),
        )
        .route(
            "/api/a038-llm-connection/:id",
            get(handlers::a038_llm_connection::get_by_id)
                .delete(handlers::a038_llm_connection::delete),
        )
        .route(
            "/api/a038-llm-connection/:id/test",
            post(handlers::a038_llm_connection::test_connection),
        )
        .route(
            "/api/a038-llm-connection/:id/fetch-models",
            post(handlers::a038_llm_connection::fetch_models),
        )
        .layer(middleware::from_fn(
            |req: Request<Body>, next: Next| async move {
                check_scope("a038_llm_connection", req, next).await
            },
        ))
}

fn a039_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/a039-mail-message",
            get(handlers::a039_mail_message::list_all),
        )
        .route(
            "/api/a039-mail-message/list",
            get(handlers::a039_mail_message::list_paginated),
        )
        .route(
            "/api/a039-mail-message/:id",
            get(handlers::a039_mail_message::get_by_id).delete(handlers::a039_mail_message::delete),
        )
        .layer(middleware::from_fn(
            |req: Request<Body>, next: Next| async move {
                check_scope("a039_mail_message", req, next).await
            },
        ))
}

/// Каталог и матрица доступа LLM-навыков.
fn llm_skills_routes() -> Router<AppState> {
    Router::new()
        .route("/api/llm-skills", get(handlers::llm_skills::list))
        .route("/api/llm-skills/reload", post(handlers::llm_skills::reload))
        .route(
            "/api/llm-skills/access-matrix",
            get(handlers::llm_skills::access_matrix).put(handlers::llm_skills::save_access_matrix),
        )
        .layer(middleware::from_fn(
            |req: Request<Body>, next: Next| async move {
                check_scope("a017_llm_agent", req, next).await
            },
        ))
}

/// Каталог LLM-инструментов (read-only обзор реестра для UI).
fn llm_tools_routes() -> Router<AppState> {
    Router::new().route("/api/llm-tools", get(handlers::llm_tools::list))
}

/// Сводка качества работы агентов (дашборд d407). Скоуп тот же, что у каталога
/// навыков: это надзорная страница по флоту сотрудников.
fn llm_quality_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/llm-quality/overview",
            get(handlers::llm_quality::overview),
        )
        .layer(middleware::from_fn(
            |req: Request<Body>, next: Next| async move {
                check_scope("a017_llm_agent", req, next).await
            },
        ))
}

fn a018_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/a018-llm-chat",
            get(handlers::a018_llm_chat::list_all).post(handlers::a018_llm_chat::upsert),
        )
        .route(
            "/api/a018-llm-chat/with-stats",
            get(handlers::a018_llm_chat::list_with_stats),
        )
        .route(
            "/api/a018-llm-chat/list",
            get(handlers::a018_llm_chat::list_paginated),
        )
        .route(
            "/api/a018-llm-chat/jobs/:job_id",
            get(handlers::a018_llm_chat::poll_job),
        )
        .route(
            "/api/a018-llm-chat/jobs/:job_id/cancel",
            post(handlers::a018_llm_chat::cancel_job),
        )
        .route(
            "/api/a018-llm-chat/jobs/:job_id/stream",
            get(handlers::a018_llm_chat::stream_job),
        )
        .route(
            "/api/a018-llm-chat/:id",
            get(handlers::a018_llm_chat::get_by_id).delete(handlers::a018_llm_chat::delete),
        )
        .route(
            "/api/a018-llm-chat/:id/model",
            post(handlers::a018_llm_chat::set_model),
        )
        .route(
            "/api/a018-llm-chat/:id/messages",
            get(handlers::a018_llm_chat::get_messages).post(handlers::a018_llm_chat::send_message),
        )
        .route(
            "/api/a018-llm-chat/message/:message_id/tool-trace",
            get(handlers::a018_llm_chat::get_tool_trace),
        )
        .route(
            "/api/a018-llm-chat/:id/rating",
            post(handlers::a018_llm_chat::set_rating),
        )
        .route(
            "/api/a018-llm-chat/:id/shared",
            post(handlers::a018_llm_chat::set_shared),
        )
        .route(
            "/api/a018-llm-chat/:id/upload",
            post(handlers::a018_llm_chat::upload_attachment)
                // Multipart framing adds a small overhead beyond the 10 MiB file limit
                // enforced by the service.
                .layer(axum::extract::DefaultBodyLimit::max(11 * 1024 * 1024)),
        )
        .route(
            "/api/a018-llm-chat/:chat_id/attachments/:attachment_id",
            get(handlers::a018_llm_chat::get_attachment)
                .delete(handlers::a018_llm_chat::delete_pending_attachment),
        )
        .route(
            "/api/a018-llm-chat/:id/context",
            get(handlers::a018_llm_chat::get_chat_context)
                .post(handlers::a018_llm_chat::add_chat_context),
        )
        .route(
            "/api/a018-llm-chat/:id/workspace",
            get(handlers::a018_llm_chat::get_workspace),
        )
        .route(
            "/api/a018-llm-chat/:id/workspace/active",
            post(handlers::a018_llm_chat::set_active_activity),
        )
        .route(
            "/api/a018-llm-chat/:id/workspace/answer",
            post(handlers::a018_llm_chat::answer_intake_question),
        )
        .route(
            "/api/a018-llm-chat/:id/workspace/file/*path",
            get(handlers::a018_llm_chat::get_workspace_file)
                .put(handlers::a018_llm_chat::save_workspace_file),
        )
        .route(
            "/api/a018-llm-chat-context/:id",
            get(handlers::a018_llm_chat::get_context_package),
        )
        .layer(middleware::from_fn(
            |req: Request<Body>, next: Next| async move {
                check_scope("a018_llm_chat", req, next).await
            },
        ))
}

fn a019_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/a019-llm-artifact",
            get(handlers::a019_llm_artifact::list_all).post(handlers::a019_llm_artifact::upsert),
        )
        .route(
            "/api/a019-llm-artifact/list",
            get(handlers::a019_llm_artifact::list_paginated),
        )
        .route(
            "/api/a019-llm-artifact/chat/:chat_id",
            get(handlers::a019_llm_artifact::list_by_chat),
        )
        .route(
            "/api/a019-llm-artifact/:id",
            get(handlers::a019_llm_artifact::get_by_id).delete(handlers::a019_llm_artifact::delete),
        )
        .layer(middleware::from_fn(
            |req: Request<Body>, next: Next| async move {
                check_scope("a019_llm_artifact", req, next).await
            },
        ))
}

fn a020_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/a020/wb-promotions",
            get(handlers::a020_wb_promotion::list_promotions),
        )
        .route(
            "/api/a020/wb-promotions/:id",
            get(handlers::a020_wb_promotion::get_promotion_detail),
        )
        .route(
            "/api/a020/wb-promotions/:id/post",
            post(handlers::a020_wb_promotion::post_promotion),
        )
        .route(
            "/api/a020/wb-promotions/:id/unpost",
            post(handlers::a020_wb_promotion::unpost_promotion),
        )
        .route(
            "/api/a020/raw/:ref_id",
            get(handlers::a020_wb_promotion::get_raw_json),
        )
        .layer(middleware::from_fn(
            |req: Request<Body>, next: Next| async move {
                check_scope("a020_wb_promotion", req, next).await
            },
        ))
}

fn a021_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/a021/production-output/list",
            get(handlers::a021_production_output::list_paginated),
        )
        .route(
            "/api/a021/production-output/:id",
            get(handlers::a021_production_output::get_by_id),
        )
        .route(
            "/api/a021/production-output/:id/post",
            post(handlers::a021_production_output::post_production_output),
        )
        .route(
            "/api/a021/production-output/:id/unpost",
            post(handlers::a021_production_output::unpost_production_output),
        )
        .layer(middleware::from_fn(
            |req: Request<Body>, next: Next| async move {
                check_scope("a021_production_output", req, next).await
            },
        ))
}

fn a022_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/a022/kit-variant/list",
            get(handlers::a022_kit_variant::list_paginated),
        )
        .route(
            "/api/a022/kit-variant/:id",
            get(handlers::a022_kit_variant::get_by_id),
        )
        .layer(middleware::from_fn(
            |req: Request<Body>, next: Next| async move {
                check_scope("a022_kit_variant", req, next).await
            },
        ))
}

fn a026_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/a026/wb-advert-daily/list",
            get(handlers::a026_wb_advert_daily::list_paginated),
        )
        .route(
            "/api/a026/wb-advert-daily/report.csv",
            get(handlers::a026_wb_advert_daily::report_csv),
        )
        .route(
            "/api/a026/wb-advert-daily/:id",
            get(handlers::a026_wb_advert_daily::get_by_id),
        )
        .route(
            "/api/a026/wb-advert-daily/:id/post",
            post(handlers::a026_wb_advert_daily::post_document),
        )
        .route(
            "/api/a026/wb-advert-daily/:id/unpost",
            post(handlers::a026_wb_advert_daily::unpost_document),
        )
        .route(
            "/api/a026/wb-advert-daily/:id/journal",
            get(handlers::a026_wb_advert_daily::get_general_ledger_entries),
        )
        .route(
            "/api/a026/wb-advert-daily/:id/projections",
            get(handlers::a026_wb_advert_daily::get_projections),
        )
        .layer(middleware::from_fn(
            |req: Request<Body>, next: Next| async move {
                check_scope("a026_wb_advert_daily", req, next).await
            },
        ))
}

fn a036_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/a036/wb-sales-funnel/list",
            get(handlers::a036_wb_sales_funnel_daily::list_paginated),
        )
        .route(
            "/api/a036/wb-sales-funnel/export-lines",
            get(handlers::a036_wb_sales_funnel_daily::export_lines),
        )
        .route(
            "/api/a036/wb-sales-funnel/product-metrics",
            get(handlers::a036_wb_sales_funnel_daily::get_product_metrics),
        )
        .route(
            "/api/a036/wb-sales-funnel/rebuild-funnel-projection",
            post(handlers::a036_wb_sales_funnel_daily::rebuild_funnel_projection),
        )
        .route(
            "/api/a036/wb-sales-funnel/:id",
            get(handlers::a036_wb_sales_funnel_daily::get_by_id),
        )
        .route(
            "/api/a036/wb-sales-funnel/:id/post",
            post(handlers::a036_wb_sales_funnel_daily::post),
        )
        .route(
            "/api/a036/wb-sales-funnel/:id/projections",
            get(handlers::a036_wb_sales_funnel_daily::get_projections),
        )
        .layer(middleware::from_fn(
            |req: Request<Body>, next: Next| async move {
                check_scope("a036_wb_sales_funnel_daily", req, next).await
            },
        ))
}

fn a037_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/a037/wb-product-snapshot/list",
            get(handlers::a037_wb_product_snapshot::list_paginated),
        )
        .route(
            "/api/a037/wb-product-snapshot/series",
            get(handlers::a037_wb_product_snapshot::get_series),
        )
        .route(
            "/api/a037/wb-product-snapshot/rating-changes",
            get(handlers::a037_wb_product_snapshot::get_rating_changes),
        )
        .route(
            "/api/a037/wb-product-snapshot/:id",
            get(handlers::a037_wb_product_snapshot::get_by_id),
        )
        .layer(middleware::from_fn(
            |req: Request<Body>, next: Next| async move {
                check_scope("a037_wb_product_snapshot", req, next).await
            },
        ))
}

fn a040_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/a040/wb-search-analytics/list",
            get(handlers::a040_wb_search_analytics_daily::list_paginated),
        )
        .route(
            "/api/a040/wb-search-analytics/:id",
            get(handlers::a040_wb_search_analytics_daily::get_by_id),
        )
        .layer(middleware::from_fn(
            |req: Request<Body>, next: Next| async move {
                check_scope("a040_wb_search_analytics_daily", req, next).await
            },
        ))
}

fn a041_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/a041/ym-shows-sales/list",
            get(handlers::a041_ym_shows_sales_daily::list_paginated),
        )
        .route(
            "/api/a041/ym-shows-sales/:id",
            get(handlers::a041_ym_shows_sales_daily::get_by_id),
        )
        .layer(middleware::from_fn(|req, next| async move {
            check_scope("a041_ym_shows_sales_daily", req, next).await
        }))
}

fn a043_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/a043/wb-finance-reports/list",
            get(handlers::a043_wb_finance_report::list),
        )
        .route(
            "/api/a043/wb-finance-reports/:id",
            get(handlers::a043_wb_finance_report::get),
        )
        .route(
            "/api/a043/wb-finance-reports/:id/lines",
            get(handlers::a043_wb_finance_report::lines),
        )
        .layer(middleware::from_fn(
            |req: Request<Body>, next: Next| async move {
                check_scope("a043_wb_finance_report", req, next).await
            },
        ))
}

fn a034_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/a034/ym-realization/list",
            get(handlers::a034_ym_realization::list_paginated),
        )
        .route(
            "/api/a034/ym-realization/:id",
            get(handlers::a034_ym_realization::get_by_id),
        )
        .route(
            "/api/a034/ym-realization/:id/post",
            post(handlers::a034_ym_realization::post_document),
        )
        .route(
            "/api/a034/ym-realization/:id/unpost",
            post(handlers::a034_ym_realization::unpost_document),
        )
        .route(
            "/api/a034/ym-realization/:id/journal",
            get(handlers::a034_ym_realization::get_general_ledger_entries),
        )
        .route(
            "/api/a034/ym-realization/:id/payment-detail",
            get(handlers::a034_ym_realization::get_payment_detail),
        )
        .route(
            "/api/a034/ym-realization/:id/reconciliation-sales",
            get(handlers::a034_ym_realization::get_reconciliation_sales),
        )
        .route(
            "/api/a034/ym-realization/:id/reconciliation-returns",
            get(handlers::a034_ym_realization::get_reconciliation_returns),
        )
        .route(
            "/api/a034/ym-realization/:id/delivery-orders",
            get(handlers::a034_ym_realization::get_delivery_orders),
        )
        .route(
            "/api/a034/ym-realization/:id/fetch-missing-orders",
            post(handlers::a034_ym_realization::fetch_missing_orders),
        )
        .route(
            "/api/a034/ym-realization/:id/reconciliation-summary",
            get(handlers::a034_ym_realization::get_reconciliation_summary),
        )
        // Revenue reconciliation report (fina/p907 vs ybuh/a034) — read-only,
        // scoped to a034 so operators with a034 access can reach it without
        // requiring the broader `general_ledger` system-view scope.
        .route(
            "/api/reports/ym-revenue-reconciliation",
            get(handlers::general_ledger::ym_revenue_reconciliation),
        )
        .layer(middleware::from_fn(
            |req: Request<Body>, next: Next| async move {
                check_scope("a034_ym_realization", req, next).await
            },
        ))
}

fn a035_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/a035/ym-settlement-recon/list",
            get(handlers::a035_ym_settlement_recon::list_paginated),
        )
        .route(
            "/api/a035/ym-settlement-recon/generate",
            post(handlers::a035_ym_settlement_recon::generate),
        )
        .route(
            "/api/a035/ym-settlement-recon/:id",
            get(handlers::a035_ym_settlement_recon::get_by_id),
        )
        .route(
            "/api/a035/ym-settlement-recon/:id/recompute",
            post(handlers::a035_ym_settlement_recon::recompute),
        )
        .route(
            "/api/a035/ym-settlement-recon/:id/post",
            post(handlers::a035_ym_settlement_recon::post_document),
        )
        .route(
            "/api/a035/ym-settlement-recon/:id/unpost",
            post(handlers::a035_ym_settlement_recon::unpost_document),
        )
        .layer(middleware::from_fn(
            |req: Request<Body>, next: Next| async move {
                check_scope("a035_ym_settlement_recon", req, next).await
            },
        ))
}

fn a023_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/a023/purchase-of-goods/list",
            get(handlers::a023_purchase_of_goods::list_paginated),
        )
        .route(
            "/api/a023/purchase-of-goods/:id",
            get(handlers::a023_purchase_of_goods::get_by_id),
        )
        .route(
            "/api/a023/purchase-of-goods/:id/post",
            post(handlers::a023_purchase_of_goods::post_purchase_of_goods),
        )
        .route(
            "/api/a023/purchase-of-goods/:id/unpost",
            post(handlers::a023_purchase_of_goods::unpost_purchase_of_goods),
        )
        .layer(middleware::from_fn(
            |req: Request<Body>, next: Next| async move {
                check_scope("a023_purchase_of_goods", req, next).await
            },
        ))
}

fn a024_routes() -> Router<AppState> {
    // Write routes: upsert, delete, testdata, generate-view — require "all" access.
    let write_routes = Router::new()
        .route(
            "/api/a024-bi-indicator",
            get(handlers::a024_bi_indicator::list_all).post(handlers::a024_bi_indicator::upsert),
        )
        .route(
            "/api/a024-bi-indicator/upsert",
            post(handlers::a024_bi_indicator::upsert),
        )
        .route(
            "/api/a024-bi-indicator/list",
            get(handlers::a024_bi_indicator::list_paginated),
        )
        .route(
            "/api/a024-bi-indicator/public",
            get(handlers::a024_bi_indicator::list_public),
        )
        .route(
            "/api/a024-bi-indicator/owner/:user_id",
            get(handlers::a024_bi_indicator::list_by_owner),
        )
        .route(
            "/api/a024-bi-indicator/testdata",
            post(handlers::a024_bi_indicator::insert_test_data),
        )
        .route(
            "/api/a024-bi-indicator/generate-view",
            post(handlers::a024_bi_indicator::generate_view),
        )
        .route(
            "/api/a024-bi-indicator/:id",
            get(handlers::a024_bi_indicator::get_by_id).delete(handlers::a024_bi_indicator::delete),
        )
        .layer(middleware::from_fn(
            |req: Request<Body>, next: Next| async move {
                check_scope("a024_bi_indicator", req, next).await
            },
        ));

    // Read-only compute routes: these POST endpoints only compute/query data,
    // they never mutate state — "read" access is sufficient.
    let compute_routes = Router::new()
        .route(
            "/api/a024-bi-indicator/resolve-batch",
            post(handlers::a024_bi_indicator::resolve_batch),
        )
        .route(
            "/api/a024-bi-indicator/:id/compute",
            post(handlers::a024_bi_indicator::compute),
        )
        .route(
            "/api/a024-bi-indicator/compute-batch",
            post(handlers::a024_bi_indicator::compute_batch),
        )
        .route(
            "/api/a024-bi-indicator/:id/drilldown",
            get(handlers::a024_bi_indicator::drilldown),
        )
        .route(
            "/api/drilldown/execute",
            post(handlers::a024_bi_indicator::execute_drilldown),
        )
        .layer(middleware::from_fn(
            |req: Request<Body>, next: Next| async move {
                check_scope_read("a024_bi_indicator", req, next).await
            },
        ));

    write_routes.merge(compute_routes)
}

fn a025_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/a025-bi-dashboard",
            get(handlers::a025_bi_dashboard::list_all).post(handlers::a025_bi_dashboard::upsert),
        )
        .route(
            "/api/a025-bi-dashboard/upsert",
            post(handlers::a025_bi_dashboard::upsert),
        )
        .route(
            "/api/a025-bi-dashboard/list",
            get(handlers::a025_bi_dashboard::list_paginated),
        )
        .route(
            "/api/a025-bi-dashboard/public",
            get(handlers::a025_bi_dashboard::list_public),
        )
        .route(
            "/api/a025-bi-dashboard/testdata",
            post(handlers::a025_bi_dashboard::insert_test_data),
        )
        .route(
            "/api/a025-bi-dashboard/owner/:user_id",
            get(handlers::a025_bi_dashboard::list_by_owner),
        )
        .route(
            "/api/a025-bi-dashboard/:id",
            get(handlers::a025_bi_dashboard::get_by_id).delete(handlers::a025_bi_dashboard::delete),
        )
        .layer(middleware::from_fn(
            |req: Request<Body>, next: Next| async move {
                check_scope("a025_bi_dashboard", req, next).await
            },
        ))
}

fn a031_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/a031-kb-edit",
            get(handlers::a031_kb_edit::list_paginated).post(handlers::a031_kb_edit::upsert),
        )
        .route(
            "/api/a031-kb-edit/list",
            get(handlers::a031_kb_edit::list_paginated),
        )
        .route(
            "/api/a031-kb-edit/:id",
            get(handlers::a031_kb_edit::get_by_id)
                .put(handlers::a031_kb_edit::upsert)
                .delete(handlers::a031_kb_edit::delete),
        )
        .route(
            "/api/a031-kb-edit/:id/approve",
            post(handlers::a031_kb_edit::approve),
        )
        .route(
            "/api/a031-kb-edit/:id/cancel",
            post(handlers::a031_kb_edit::cancel),
        )
        .layer(middleware::from_fn(
            |req: Request<Body>, next: Next| async move {
                check_scope("a031_kb_edit", req, next).await
            },
        ))
}

/// Очередь поручений между AI-сотрудниками.
///
/// Создания через HTTP нет намеренно: поручение ставит агент через LLM-инструмент
/// `create_agent_task`, и все гарды (глубина цепочки, потолки очереди, дубли)
/// живут там. Ручной путь оставлен только на управление уже созданным.
fn a042_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/a042-agent-task",
            get(handlers::a042_agent_task::list_paginated),
        )
        .route(
            "/api/a042-agent-task/list",
            get(handlers::a042_agent_task::list_paginated),
        )
        .route(
            "/api/a042-agent-task/:id",
            get(handlers::a042_agent_task::get_by_id).delete(handlers::a042_agent_task::delete),
        )
        .route(
            "/api/a042-agent-task/:id/cancel",
            post(handlers::a042_agent_task::cancel),
        )
        .route(
            "/api/a042-agent-task/:id/requeue",
            post(handlers::a042_agent_task::requeue),
        )
        .layer(middleware::from_fn(
            |req: Request<Body>, next: Next| async move {
                check_scope("a042_agent_task", req, next).await
            },
        ))
}

fn a032_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/a032/wb-returns-claims",
            get(handlers::a032_wb_returns_claims::list_returns_claims),
        )
        .route(
            "/api/a032/wb-returns-claims/:id",
            get(handlers::a032_wb_returns_claims::get_returns_claim_detail),
        )
        .layer(middleware::from_fn(
            |req: Request<Body>, next: Next| async move {
                check_scope("a032_wb_returns_claims", req, next).await
            },
        ))
}

fn a033_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/a033/wb-day-close",
            get(handlers::a033_wb_day_close::list_paginated)
                .post(handlers::a033_wb_day_close::create_active),
        )
        .route(
            "/api/a033/wb-day-close/compare",
            post(handlers::a033_wb_day_close::compare),
        )
        .route(
            "/api/a033/wb-day-close/by-day/:connection_id/:business_date",
            get(handlers::a033_wb_day_close::list_by_day),
        )
        .route(
            "/api/a033/wb-day-close/:id",
            get(handlers::a033_wb_day_close::get_by_id),
        )
        .route(
            "/api/a033/wb-day-close/:id/advert-live",
            get(handlers::a033_wb_day_close::advert_live),
        )
        .route(
            "/api/a033/wb-day-close/:id/recalculate",
            post(handlers::a033_wb_day_close::recalculate),
        )
        .route(
            "/api/a033/wb-day-close/:id/repost-problematic-a012",
            post(handlers::a033_wb_day_close::repost_problematic_a012),
        )
        .route(
            "/api/a033/wb-day-close/:id/archive-and-recreate",
            post(handlers::a033_wb_day_close::archive_and_recreate),
        )
        .layer(middleware::from_fn(
            |req: Request<Body>, next: Next| async move {
                check_scope("a033_wb_day_close", req, next).await
            },
        ))
}

fn a027_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/a027/wb-documents/list",
            get(handlers::a027_wb_documents::list_paginated),
        )
        .route(
            "/api/a027/wb-documents/:id",
            get(handlers::a027_wb_documents::get_by_id),
        )
        .route(
            "/api/a027/wb-documents/:id/manual",
            put(handlers::a027_wb_documents::update_manual_fields),
        )
        .route(
            "/api/a027/wb-documents/:id/extract-weekly-report",
            post(handlers::a027_wb_documents::extract_weekly_report),
        )
        .route(
            "/api/a027/wb-documents/:id/post",
            post(handlers::a027_wb_documents::post_document),
        )
        .route(
            "/api/a027/wb-documents/:id/download/:extension",
            get(handlers::a027_wb_documents::download_document),
        )
        .layer(middleware::from_fn(
            |req: Request<Body>, next: Next| async move {
                check_scope("a027_wb_documents", req, next).await
            },
        ))
}

fn a028_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/a028/missing-cost-registry/list",
            get(handlers::a028_missing_cost_registry::list_paginated),
        )
        .route(
            "/api/a028/missing-cost-registry/:id",
            get(handlers::a028_missing_cost_registry::get_by_id)
                .put(handlers::a028_missing_cost_registry::update_document),
        )
        .route(
            "/api/a028/missing-cost-registry/:id/post",
            post(handlers::a028_missing_cost_registry::post_document),
        )
        .route(
            "/api/a028/missing-cost-registry/:id/unpost",
            post(handlers::a028_missing_cost_registry::unpost_document),
        )
        .layer(middleware::from_fn(
            |req: Request<Body>, next: Next| async move {
                check_scope("a028_missing_cost_registry", req, next).await
            },
        ))
}

fn a029_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/a029/wb-supply",
            get(handlers::a029_wb_supply::list_supplies),
        )
        // Lookup by WB supply ID string (e.g. "WB-GI-32319994") — must be before :id
        .route(
            "/api/a029/wb-supply/by-wb-id/:wb_id",
            get(handlers::a029_wb_supply::get_supply_by_wb_id),
        )
        .route(
            "/api/a029/wb-supply/by-order/:order_id",
            get(handlers::a029_wb_supply::get_supply_for_order),
        )
        .route(
            "/api/a029/wb-supply/:id",
            get(handlers::a029_wb_supply::get_supply_detail),
        )
        .route(
            "/api/a029/wb-supply/:id/orders",
            get(handlers::a029_wb_supply::get_supply_orders),
        )
        .route(
            "/api/a029/wb-supply/:id/stickers",
            get(handlers::a029_wb_supply::get_supply_stickers),
        )
        .route(
            "/api/a029/raw/:ref_id",
            get(handlers::a029_wb_supply::get_raw_json),
        )
        .route(
            "/api/a029/wb-supply/:id/delete",
            post(handlers::a029_wb_supply::delete_supply),
        )
        .layer(middleware::from_fn(
            |req: Request<Body>, next: Next| async move {
                check_scope("a029_wb_supply", req, next).await
            },
        ))
}

fn a030_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/a030/wb-advert-campaign/list",
            get(handlers::a030_wb_advert_campaign::list),
        )
        .route(
            "/api/a030/wb-advert-campaign/:id",
            get(handlers::a030_wb_advert_campaign::get_by_id),
        )
        .route(
            "/api/a030/wb-advert-campaign/:id/nm-positions",
            get(handlers::a030_wb_advert_campaign::nm_positions),
        )
        .route(
            "/api/a030/wb-advert-campaign/:id/advert-stats",
            get(handlers::a030_wb_advert_campaign::advert_stats),
        )
        .layer(middleware::from_fn(
            |req: Request<Body>, next: Next| async move {
                check_scope_read("a030_wb_advert_campaign", req, next).await
            },
        ))
}

// ============================================================================
// Use Cases U501–U508 — each with its own scope
// ============================================================================

fn u501_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/u501/import/start",
            post(handlers::usecases::u501_start_import),
        )
        .route(
            "/api/u501/import/:session_id/progress",
            get(handlers::usecases::u501_get_progress),
        )
        .layer(middleware::from_fn(
            |req: Request<Body>, next: Next| async move {
                check_scope("u501_import_from_ut", req, next).await
            },
        ))
}

fn u502_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/u502/import/start",
            post(handlers::usecases::u502_start_import),
        )
        .route(
            "/api/u502/import/:session_id/progress",
            get(handlers::usecases::u502_get_progress),
        )
        .layer(middleware::from_fn(
            |req: Request<Body>, next: Next| async move {
                check_scope("u502_import_from_ozon", req, next).await
            },
        ))
}

fn u503_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/u503/import/start",
            post(handlers::usecases::u503_start_import),
        )
        .route(
            "/api/u503/import/:session_id/progress",
            get(handlers::usecases::u503_get_progress),
        )
        .layer(middleware::from_fn(
            |req: Request<Body>, next: Next| async move {
                check_scope("u503_import_from_yandex", req, next).await
            },
        ))
}

fn u504_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/u504/import/start",
            post(handlers::usecases::u504_start_import),
        )
        .route(
            "/api/u504/import/:session_id/progress",
            get(handlers::usecases::u504_get_progress),
        )
        .layer(middleware::from_fn(
            |req: Request<Body>, next: Next| async move {
                check_scope("u504_import_from_wildberries", req, next).await
            },
        ))
}

fn u505_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/u505/match/start",
            post(handlers::usecases::u505_start_matching),
        )
        .route(
            "/api/u505/match/:session_id/progress",
            get(handlers::usecases::u505_get_progress),
        )
        .layer(middleware::from_fn(
            |req: Request<Body>, next: Next| async move {
                check_scope("u505_match_nomenclature", req, next).await
            },
        ))
}

fn u506_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/u506/import/start",
            post(handlers::usecases::u506_start_import),
        )
        .route(
            "/api/u506/import/:session_id/progress",
            get(handlers::usecases::u506_get_progress),
        )
        .layer(middleware::from_fn(
            |req: Request<Body>, next: Next| async move {
                check_scope("u506_import_from_lemanapro", req, next).await
            },
        ))
}

fn u507_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/u507/import/start",
            post(handlers::usecases::u507_start_import),
        )
        .route(
            "/api/u507/import/:session_id/progress",
            get(handlers::usecases::u507_get_progress),
        )
        .layer(middleware::from_fn(
            |req: Request<Body>, next: Next| async move {
                check_scope("u507_import_from_erp", req, next).await
            },
        ))
}

fn u508_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/u508/repost/projections",
            get(handlers::usecases::u508_get_projections),
        )
        .route(
            "/api/u508/repost/aggregates",
            get(handlers::usecases::u508_get_aggregates),
        )
        .route(
            "/api/u508/repost/start",
            post(handlers::usecases::u508_start_repost),
        )
        .route(
            "/api/u508/repost/aggregate/start",
            post(handlers::usecases::u508_start_aggregate_repost),
        )
        .route(
            "/api/u508/repost/funnel/start",
            post(handlers::usecases::u508_start_funnel_rebuild),
        )
        .route(
            "/api/u508/repost/funnel/diagnostics",
            get(handlers::usecases::u508_funnel_diagnostics),
        )
        .route(
            "/api/u508/repost/:session_id/progress",
            get(handlers::usecases::u508_get_progress),
        )
        .layer(middleware::from_fn(
            |req: Request<Body>, next: Next| async move {
                check_scope("u508_repost_documents", req, next).await
            },
        ))
}

// ============================================================================
// Projections P900–P912 — each with its own scope
// ============================================================================

fn p900_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/p900/sales-register",
            get(handlers::p900_mp_sales_register::list_sales),
        )
        .route(
            "/api/p900/sales-register/:marketplace/:document_no/:line_id",
            get(handlers::p900_mp_sales_register::get_sale_detail),
        )
        .route(
            "/api/p900/stats/by-date",
            get(handlers::p900_mp_sales_register::get_stats_by_date),
        )
        .route(
            "/api/p900/stats/by-marketplace",
            get(handlers::p900_mp_sales_register::get_stats_by_marketplace),
        )
        .route(
            "/api/p900/backfill-product-refs",
            post(handlers::p900_mp_sales_register::backfill_product_refs),
        )
        .route(
            "/api/projections/p900/:registrator_ref",
            get(handlers::p900_mp_sales_register::get_by_registrator),
        )
        .layer(middleware::from_fn(
            |req: Request<Body>, next: Next| async move {
                check_scope("p900_mp_sales_register", req, next).await
            },
        ))
}

fn p901_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/p901/barcode/:barcode",
            get(handlers::p901_nomenclature_barcodes::get_by_barcode),
        )
        .route(
            "/api/p901/nomenclature/:nomenclature_ref/barcodes",
            get(handlers::p901_nomenclature_barcodes::get_barcodes_by_nomenclature),
        )
        .route(
            "/api/p901/barcodes",
            get(handlers::p901_nomenclature_barcodes::list_barcodes),
        )
        .layer(middleware::from_fn(
            |req: Request<Body>, next: Next| async move {
                check_scope_read("p901_nomenclature_barcodes", req, next).await
            },
        ))
}

fn p902_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/p902/finance-realization",
            get(handlers::p902_ozon_finance_realization::list_finance_realization),
        )
        .route(
            "/api/p902/finance-realization/:posting_number/:sku/:operation_type",
            get(handlers::p902_ozon_finance_realization::get_finance_realization_detail),
        )
        .route(
            "/api/p902/stats",
            get(handlers::p902_ozon_finance_realization::get_stats),
        )
        .layer(middleware::from_fn(
            |req: Request<Body>, next: Next| async move {
                check_scope_read("p902_ozon_finance_realization", req, next).await
            },
        ))
}

fn p903_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/p903/finance-report",
            get(handlers::p903_wb_finance_report::list_reports),
        )
        .route(
            "/api/p903/finance-report/export",
            get(handlers::p903_wb_finance_report::export_reports),
        )
        .route(
            "/api/p903/finance-report/search-by-srid",
            get(handlers::p903_wb_finance_report::search_by_srid),
        )
        .route(
            "/api/p903/finance-report/operation-kinds",
            get(handlers::p903_wb_finance_report::list_operation_kinds),
        )
        .route(
            "/api/p903/finance-report/by-id/:id",
            get(handlers::p903_wb_finance_report::get_report_detail_by_id),
        )
        .route(
            "/api/p903/finance-report/by-id/:id/post",
            post(handlers::p903_wb_finance_report::post_report_by_id),
        )
        .route(
            "/api/p903/finance-report/by-id/:id/raw",
            get(handlers::p903_wb_finance_report::get_raw_json_by_id),
        )
        .layer(middleware::from_fn(
            |req: Request<Body>, next: Next| async move {
                check_scope("p903_wb_finance_report", req, next).await
            },
        ))
}

fn p904_routes() -> Router<AppState> {
    Router::new()
        .route("/api/p904/sales-data", get(handlers::p904_sales_data::list))
        .layer(middleware::from_fn(
            |req: Request<Body>, next: Next| async move {
                check_scope_read("p904_sales_data", req, next).await
            },
        ))
}

fn p905_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/p905-commission/list",
            get(handlers::p905_wb_commission_history::list_commissions),
        )
        .route(
            "/api/p905-commission/sync",
            post(handlers::p905_wb_commission_history::sync_commissions),
        )
        .route(
            "/api/p905-commission/:id",
            get(handlers::p905_wb_commission_history::get_commission)
                .put(handlers::p905_wb_commission_history::save_commission)
                .delete(handlers::p905_wb_commission_history::delete_commission),
        )
        .route(
            "/api/p905-commission",
            post(handlers::p905_wb_commission_history::save_commission),
        )
        .layer(middleware::from_fn(
            |req: Request<Body>, next: Next| async move {
                check_scope("p905_wb_commission_history", req, next).await
            },
        ))
}

fn p906_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/p906/nomenclature-prices",
            get(handlers::p906_nomenclature_prices::list),
        )
        .route(
            "/api/p906/periods",
            get(handlers::p906_nomenclature_prices::get_periods),
        )
        .route(
            "/api/p906/import-excel",
            post(handlers::p906_nomenclature_prices::import_excel),
        )
        .layer(middleware::from_fn(
            |req: Request<Body>, next: Next| async move {
                check_scope("p906_nomenclature_prices", req, next).await
            },
        ))
}

fn p907_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/p907/payment-report",
            get(handlers::p907_ym_payment_report::list_reports),
        )
        .route(
            "/api/p907/payment-report/filter-options",
            get(handlers::p907_ym_payment_report::filter_options),
        )
        // Migrate SYNTH_... record_keys to ymid_... format (idempotent).
        .route(
            "/api/p907/payment-report/migrate-keys",
            post(handlers::p907_ym_payment_report::migrate_keys),
        )
        // Перепровести все записи p907 (перестроить GL/p914).
        .route(
            "/api/p907/payment-report/repost-all",
            post(handlers::p907_ym_payment_report::repost_all),
        )
        // Get single record by internal UUID (id column).
        .route(
            "/api/p907/payment-report/:id",
            get(handlers::p907_ym_payment_report::get_report),
        )
        .route(
            "/api/p907/payment-report/:id/post",
            post(handlers::p907_ym_payment_report::post_report),
        )
        // p914 finance turnovers (слой fina) для записи p907.
        .route(
            "/api/p907/payment-report/:id/finance-turnovers",
            get(handlers::p907_ym_payment_report::get_finance_turnovers),
        )
        .layer(middleware::from_fn(
            |req: Request<Body>, next: Next| async move {
                check_scope("p907_ym_payment_report", req, next).await
            },
        ))
}

fn p908_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/p908/goods-prices",
            get(handlers::p908_wb_goods_prices::list_goods_prices),
        )
        .route(
            "/api/p908/goods-prices/:nm_id",
            get(handlers::p908_wb_goods_prices::get_goods_price),
        )
        .layer(middleware::from_fn(
            |req: Request<Body>, next: Next| async move {
                check_scope_read("p908_wb_goods_prices", req, next).await
            },
        ))
}

fn p912_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/p912/nomenclature-costs",
            get(handlers::p912_nomenclature_costs::list),
        )
        .layer(middleware::from_fn(
            |req: Request<Body>, next: Next| async move {
                check_scope_read("p912_nomenclature_costs", req, next).await
            },
        ))
}

fn p913_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/p913/wb-advert-order-attr",
            get(handlers::p913_wb_advert_order_attr::list),
        )
        .layer(middleware::from_fn(
            |req: Request<Body>, next: Next| async move {
                check_scope_read("p913_wb_advert_order_attr", req, next).await
            },
        ))
}

fn p914_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/p914/mp-finance-turnovers",
            get(handlers::p914_mp_finance_turnovers::list),
        )
        .layer(middleware::from_fn(
            |req: Request<Body>, next: Next| async move {
                check_scope_read("p914_mp_finance_turnovers", req, next).await
            },
        ))
}

fn p915_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/p915/order-events",
            get(handlers::p915_mp_order_events::list),
        )
        .route(
            "/api/p915/order-events/by-order/:order_id",
            get(handlers::p915_mp_order_events::by_order),
        )
        .layer(middleware::from_fn(
            |req: Request<Body>, next: Next| async move {
                check_scope_read("p915_mp_order_events", req, next).await
            },
        ))
}

// ============================================================================
// Indicators
// ============================================================================

// ============================================================================
// Dashboards (D400, DS01, DS02)
// ============================================================================

fn dashboard_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/d400/monthly_summary",
            get(handlers::d400_monthly_summary::get_monthly_summary),
        )
        .route(
            "/api/dashboards/wb-order-flow",
            get(handlers::dashboards::wb_order_flow),
        )
        .route(
            "/api/dashboards/ym-order-flow",
            get(handlers::dashboards::ym_order_flow),
        )
        .route(
            "/api/dashboards/wb-advert-report",
            get(handlers::dashboards::wb_advert_report),
        )
        .route(
            "/api/dashboards/wb-sales-funnel",
            get(handlers::dashboards::wb_sales_funnel),
        )
        .route(
            "/api/dashboards/wb-sales-funnel/orders",
            get(handlers::dashboards::wb_sales_funnel_orders),
        )
        .route(
            "/api/d400/periods",
            get(handlers::d400_monthly_summary::get_available_periods),
        )
        // Universal Dashboard API
        .route(
            "/api/universal-dashboard/execute",
            post(handlers::ds01_wb_finance_report::execute_dashboard),
        )
        .route(
            "/api/universal-dashboard/generate-sql",
            post(handlers::ds01_wb_finance_report::generate_sql),
        )
        .route(
            "/api/universal-dashboard/schemas",
            get(handlers::ds01_wb_finance_report::list_schemas),
        )
        .route(
            "/api/universal-dashboard/schemas/validate-all",
            post(handlers::ds01_wb_finance_report::validate_all_schemas),
        )
        .route(
            "/api/universal-dashboard/schemas/:id",
            get(handlers::ds01_wb_finance_report::get_schema),
        )
        .route(
            "/api/universal-dashboard/schemas/:id/validate",
            post(handlers::ds01_wb_finance_report::validate_schema),
        )
        .route(
            "/api/universal-dashboard/schemas/:schema_id/fields/:field_id/values",
            get(handlers::ds01_wb_finance_report::get_distinct_values),
        )
        .route(
            "/api/universal-dashboard/configs",
            get(handlers::ds01_wb_finance_report::list_configs)
                .post(handlers::ds01_wb_finance_report::save_config),
        )
        .route(
            "/api/universal-dashboard/configs/:id",
            get(handlers::ds01_wb_finance_report::get_config)
                .put(handlers::ds01_wb_finance_report::update_config)
                .delete(handlers::ds01_wb_finance_report::delete_config),
        )
        // DS01 WB Finance Report
        .route(
            "/api/ds01/execute",
            post(handlers::ds01_wb_finance_report::execute_dashboard),
        )
        .route(
            "/api/ds01/generate-sql",
            post(handlers::ds01_wb_finance_report::generate_sql),
        )
        .route(
            "/api/ds01/schemas",
            get(handlers::ds01_wb_finance_report::list_schemas),
        )
        .route(
            "/api/ds01/schemas/:id",
            get(handlers::ds01_wb_finance_report::get_schema),
        )
        .route(
            "/api/ds01/schemas/:schema_id/fields/:field_id/values",
            get(handlers::ds01_wb_finance_report::get_distinct_values),
        )
        .route(
            "/api/ds01/configs",
            get(handlers::ds01_wb_finance_report::list_configs)
                .post(handlers::ds01_wb_finance_report::save_config),
        )
        .route(
            "/api/ds01/configs/:id",
            get(handlers::ds01_wb_finance_report::get_config)
                .put(handlers::ds01_wb_finance_report::update_config)
                .delete(handlers::ds01_wb_finance_report::delete_config),
        )
        // Legacy D401 routes
        .route(
            "/api/d401/execute",
            post(handlers::ds01_wb_finance_report::execute_dashboard),
        )
        .route(
            "/api/d401/generate-sql",
            post(handlers::ds01_wb_finance_report::generate_sql),
        )
        .route(
            "/api/d401/schemas",
            get(handlers::ds01_wb_finance_report::list_schemas),
        )
        .route(
            "/api/d401/schemas/:id",
            get(handlers::ds01_wb_finance_report::get_schema),
        )
        .route(
            "/api/d401/schemas/:schema_id/fields/:field_id/values",
            get(handlers::ds01_wb_finance_report::get_distinct_values),
        )
        .route(
            "/api/d401/configs",
            get(handlers::ds01_wb_finance_report::list_configs)
                .post(handlers::ds01_wb_finance_report::save_config),
        )
        .route(
            "/api/d401/configs/:id",
            get(handlers::ds01_wb_finance_report::get_config)
                .put(handlers::ds01_wb_finance_report::update_config)
                .delete(handlers::ds01_wb_finance_report::delete_config),
        )
        // DS02 Sales Register routes
        .route(
            "/api/ds02/execute",
            post(handlers::ds02_mp_sales_register::execute_dashboard),
        )
        .route(
            "/api/ds02/generate-sql",
            post(handlers::ds02_mp_sales_register::generate_sql),
        )
        .route(
            "/api/ds02/schemas",
            get(handlers::ds02_mp_sales_register::list_schemas),
        )
        .route(
            "/api/ds02/schemas/:id",
            get(handlers::ds02_mp_sales_register::get_schema),
        )
        .route(
            "/api/ds02/schemas/:schema_id/fields/:field_id/values",
            get(handlers::ds02_mp_sales_register::get_distinct_values),
        )
        .route(
            "/api/ds02/configs",
            get(handlers::ds02_mp_sales_register::list_configs)
                .post(handlers::ds02_mp_sales_register::save_config),
        )
        .route(
            "/api/ds02/configs/:id",
            get(handlers::ds02_mp_sales_register::get_config)
                .put(handlers::ds02_mp_sales_register::update_config)
                .delete(handlers::ds02_mp_sales_register::delete_config),
        )
        // Legacy D402 routes
        .route(
            "/api/dashboards/d402/execute",
            post(handlers::ds02_mp_sales_register::execute_dashboard),
        )
        .route(
            "/api/dashboards/d402/generate-sql",
            post(handlers::ds02_mp_sales_register::generate_sql),
        )
        .route(
            "/api/dashboards/d402/schemas",
            get(handlers::ds02_mp_sales_register::list_schemas),
        )
        .route(
            "/api/dashboards/d402/schemas/:id",
            get(handlers::ds02_mp_sales_register::get_schema),
        )
        .route(
            "/api/dashboards/d402/schemas/:schema_id/fields/:field_id/values",
            get(handlers::ds02_mp_sales_register::get_distinct_values),
        )
        .route(
            "/api/dashboards/d402/configs",
            get(handlers::ds02_mp_sales_register::list_configs)
                .post(handlers::ds02_mp_sales_register::save_config),
        )
        .route(
            "/api/dashboards/d402/configs/:id",
            get(handlers::ds02_mp_sales_register::get_config)
                .put(handlers::ds02_mp_sales_register::update_config)
                .delete(handlers::ds02_mp_sales_register::delete_config),
        )
        .layer(middleware::from_fn(|req: Request<Body>, next: Next| async move {
            check_scope("dashboard", req, next).await
        }))
}

// ============================================================================
// DataView semantic layer — scope: data_view
// ============================================================================

fn data_view_routes() -> Router<AppState> {
    Router::new()
        .route("/api/data-view", get(handlers::data_view::list))
        .route(
            "/api/data-view/filters",
            get(handlers::data_view::list_filters),
        )
        .route("/api/data-view/:id", get(handlers::data_view::get_by_id))
        .route(
            "/api/data-view/:id/filters",
            get(handlers::data_view::get_view_filters),
        )
        .route(
            "/api/data-view/:id/compute",
            axum::routing::post(handlers::data_view::compute),
        )
        .route(
            "/api/data-view/:id/drilldown",
            axum::routing::post(handlers::data_view::drilldown),
        )
        .route(
            "/api/data-view/:id/drilldown-capabilities",
            axum::routing::post(handlers::data_view::drilldown_capabilities),
        )
        // Drilldown session store is tied to data_view usage
        .route(
            "/api/sys-drilldown",
            axum::routing::post(handlers::sys_drilldown::create),
        )
        .route(
            "/api/sys-drilldown/:id",
            axum::routing::get(handlers::sys_drilldown::get_by_id),
        )
        .route(
            "/api/sys-drilldown/:id/data",
            axum::routing::get(handlers::sys_drilldown::get_data),
        )
        .layer(middleware::from_fn(
            |req: Request<Body>, next: Next| async move {
                check_scope_read("data_view", req, next).await
            },
        ))
}

// ============================================================================
// BI Timeline — scope: bi_timeline
// ============================================================================

fn bi_timeline_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/bi-timeline/indicators",
            get(handlers::bi_timeline::indicators),
        )
        .route(
            "/api/bi-timeline/series",
            post(handlers::bi_timeline::series),
        )
        .layer(middleware::from_fn(
            |req: Request<Body>, next: Next| async move {
                check_scope_read("bi_timeline", req, next).await
            },
        ))
}

// ============================================================================
// General Ledger — scope: general_ledger
// ============================================================================

fn general_ledger_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/general-ledger",
            axum::routing::get(handlers::general_ledger::list),
        )
        .route(
            "/api/general-ledger/turnovers",
            axum::routing::get(handlers::general_ledger::list_turnovers),
        )
        .route(
            "/api/general-ledger/turnovers/:code",
            axum::routing::get(handlers::general_ledger::get_turnover_by_code),
        )
        .route(
            "/api/general-ledger/dimensions",
            axum::routing::get(handlers::general_ledger::dimensions_catalog_index),
        )
        .route(
            "/api/general-ledger/layers",
            axum::routing::get(handlers::general_ledger::list_layers),
        )
        .route(
            "/api/general-ledger/entities",
            axum::routing::get(handlers::general_ledger::list_entities),
        )
        .route(
            "/api/general-ledger/supplier-balance",
            axum::routing::post(handlers::general_ledger::supplier_balance),
        )
        .route(
            "/api/general-ledger/layer-turnover-matrix",
            axum::routing::get(handlers::general_ledger::layer_turnover_matrix),
        )
        .route(
            "/api/general-ledger/report",
            axum::routing::post(handlers::general_ledger::report),
        )
        .route(
            "/api/general-ledger/account-view",
            axum::routing::post(handlers::general_ledger::account_view),
        )
        .route(
            "/api/reports/wb-weekly-reconciliation",
            axum::routing::get(handlers::general_ledger::wb_weekly_reconciliation),
        )
        .route(
            "/api/general-ledger/report/dimensions",
            axum::routing::get(handlers::general_ledger::report_dimensions),
        )
        .route(
            "/api/general-ledger/report/drilldown",
            axum::routing::post(handlers::general_ledger::report_drilldown),
        )
        .route(
            "/api/general-ledger/drilldown",
            axum::routing::post(handlers::general_ledger::create_drilldown_session),
        )
        .route(
            "/api/general-ledger/drilldown/:id",
            axum::routing::get(handlers::general_ledger::get_drilldown_session),
        )
        .route(
            "/api/general-ledger/drilldown/:id/data",
            axum::routing::get(handlers::general_ledger::get_drilldown_session_data),
        )
        .route(
            "/api/general-ledger/:id",
            axum::routing::get(handlers::general_ledger::get_by_id),
        )
        .route(
            "/api/general-ledger/:id/resource-details",
            axum::routing::get(handlers::general_ledger::get_resource_details),
        )
        .layer(middleware::from_fn(
            |req: Request<Body>, next: Next| async move {
                check_scope_read("general_ledger", req, next).await
            },
        ))
}

// ============================================================================
// External integration API — authenticated via X-Api-Key header (no JWT)
// ============================================================================

fn ext_routes() -> Router<AppState> {
    let data_routes = Router::new()
        .route(
            "/api/ext/v1/wb-supplies",
            get(handlers::ext_1c_wb_supply::list_supplies),
        )
        .route(
            "/api/ext/v1/wb-supplies/:id",
            get(handlers::ext_1c_wb_supply::get_supply_detail),
        )
        .route(
            "/api/ext/v1/wb-sales-funnel",
            get(handlers::ext_bi_wb_funnel::list_funnel),
        )
        .route(
            "/api/ext/v1/ym-sales-funnel",
            get(handlers::ext_bi_ym_funnel::list_funnel),
        )
        .route(
            "/api/ext/v1/wb-advert-daily",
            get(handlers::ext_bi_wb_advert::list_advert),
        )
        .route(
            "/api/ext/v1/wb-stocks",
            get(handlers::ext_bi_wb_stocks::list_stocks),
        )
        .route(
            "/api/ext/v1/wb-finance-report",
            get(handlers::ext_bi_wb_finance::list_finance_report),
        )
        .route(
            "/api/ext/v1/ym-payment-report",
            get(handlers::ext_bi_ym_payments::list_payment_report),
        )
        .route(
            "/api/ext/v1/nomenclature",
            get(handlers::ext_bi_nomenclature::list_nomenclature),
        )
        .route(
            "/api/ext/v1/nomenclature-skus",
            get(handlers::ext_bi_nomenclature::list_nomenclature_skus),
        )
        .layer(middleware::from_fn(
            crate::system::auth::middleware::check_api_key,
        ));

    Router::new()
        .merge(data_routes)
        // Документация — вне check_api_key: потребитель читает её до того, как
        // получит ключ, и данных она не раскрывает.
        .route(
            "/api/ext/v1/openapi.json",
            get(handlers::ext_docs::openapi_spec),
        )
        .route("/api/ext/v1/docs", get(handlers::ext_docs::docs_page))
        // Слои применяются снизу вверх, поэтому рекордер оборачивает check_api_key
        // снаружи — и в лог попадают 401 (неверный ключ) и 503 (ключ не настроен).
        // Именно эти случаи и есть «контроль корректности» интеграции.
        .layer(middleware::from_fn(
            crate::system::ext_api_log::middleware::record_ext_api_call,
        ))
}

// ============================================================================
// Quality check routes — /api/quality/checks
// ============================================================================

fn quality_routes() -> Router<AppState> {
    Router::new()
        .route("/api/quality/checks", get(handlers::quality::list_checks))
        .route(
            "/api/quality/checks/overview",
            get(handlers::quality::list_check_overviews),
        )
        .route(
            "/api/quality/checks/reload",
            post(handlers::quality::reload_checks),
        )
        .route(
            "/api/quality/checks/:id/run",
            post(handlers::quality::run_check),
        )
        .route(
            "/api/quality/checks/:id/details",
            get(handlers::quality::check_details),
        )
        .route(
            "/api/quality/checks/:id/runs",
            get(handlers::quality::list_runs),
        )
        .route(
            "/api/quality/checks/:id/sources",
            get(handlers::quality::list_sources),
        )
        .route(
            "/api/quality/checks/:id/groups",
            get(handlers::quality::list_groups),
        )
        .route(
            "/api/quality/checks/:id/rows",
            get(handlers::quality::list_rows),
        )
        .route(
            "/api/quality/checks/:id/repost",
            post(handlers::quality::bulk_repost),
        )
        .route(
            "/api/quality/checks/:id/cleanup",
            post(handlers::quality::cleanup_orphans),
        )
        .layer(middleware::from_fn(
            |req: Request<Body>, next: Next| async move {
                crate::system::auth::middleware::require_auth(req, next).await
            },
        ))
}

// ============================================================================
// Misc routes — LLM knowledge (tied to a018_llm_chat scope) + debug
// ============================================================================

fn misc_routes() -> Router<AppState> {
    let llm_knowledge = Router::new()
        .route(
            "/api/llm-knowledge",
            axum::routing::get(handlers::llm_knowledge::list),
        )
        .route(
            "/api/llm-knowledge/:id",
            axum::routing::get(handlers::llm_knowledge::get_by_id),
        )
        .layer(middleware::from_fn(
            |req: Request<Body>, next: Next| async move {
                check_scope_read("a018_llm_chat", req, next).await
            },
        ));

    // Debug endpoints (open, dev only)
    let debug = Router::new().route("/api/debug/tool-test", get(handlers::debug::tool_test));

    llm_knowledge.merge(debug)
}

fn kb_read_routes() -> Router<AppState> {
    let read_only = Router::new()
        .route("/api/kb/stats", get(handlers::kb_read::stats))
        .route("/api/kb/tree", get(handlers::kb_read::tree))
        .route("/api/kb/articles/:id", get(handlers::kb_read::get_article))
        .route("/api/kb/vocabulary", get(handlers::kb_read::vocabulary))
        .route("/api/kb/issues", get(handlers::kb_read::issues))
        // Инвентаризация знаний живёт под тем же scope: она отвечает на тот же
        // вопрос — «что в базе знаний есть», — только шире, чем статьи. Свой
        // scope сломал бы уже розданные роли ради переименования.
        .route(
            "/api/knowledge/inventory",
            get(handlers::knowledge::inventory),
        )
        .route(
            "/api/knowledge/inventory/surfaces",
            get(handlers::knowledge::surfaces),
        )
        .route(
            "/api/knowledge/inventory/unit/:id",
            get(handlers::knowledge::unit),
        )
        .route(
            "/api/knowledge/inventory/history",
            get(handlers::knowledge::history),
        )
        .layer(middleware::from_fn(
            |req: Request<Body>, next: Next| async move {
                check_scope_read("knowledge_base", req, next).await
            },
        ));

    // Перечитывание базы — не чтение: отдельный роутер, т.к. слой выше прибит
    // к check_scope_read и POST через него не пройдёт.
    let mutating = Router::new()
        .route("/api/kb/reload", post(handlers::kb_read::reload))
        .route("/api/kb/generate", post(handlers::kb_read::generate))
        .route(
            "/api/knowledge/inventory/collect",
            post(handlers::knowledge::collect),
        )
        .layer(middleware::from_fn(
            |req: Request<Body>, next: Next| async move {
                check_scope("knowledge_base", req, next).await
            },
        ));

    read_only.merge(mutating)
}

#[cfg(test)]
mod tests {
    /// Конфликт перекрывающихся путей axum паникует при СБОРКЕ Router'а, а не на
    /// cargo check. Тест собирает полный роутер приложения (system + business —
    /// ровно как main.rs), поэтому ловит конфликт за секунды, без запуска сервера:
    /// `cargo test -p backend router_builds`.
    #[test]
    fn router_builds_without_conflicts() {
        let _app: axum::Router<crate::shared::app_state::AppState> = axum::Router::new()
            .merge(crate::system::api::configure_system_routes())
            .merge(super::configure_business_routes());
    }

    /// Спека внешнего API пишется руками — единственное, что удерживает её от
    /// расхождения с кодом, это данный тест: эндпоинт без описания (или описание
    /// без эндпоинта) валит сборку. Сами страницы документации из сверки
    /// исключены — они не часть контракта данных.
    #[test]
    fn openapi_spec_covers_every_ext_route() {
        const DOCS_PATHS: [&str; 2] = ["/api/ext/v1/openapi.json", "/api/ext/v1/docs"];

        let src = include_str!("routes.rs");
        let mut routed: Vec<String> = src
            .match_indices("\"/api/ext/v1")
            .map(|(i, _)| {
                let rest = &src[i + 1..];
                let end = rest.find('"').expect("незакрытый литерал пути");
                // axum `:id` → OpenAPI `{id}`
                rest[..end].replace(":id", "{id}")
            })
            // Отсекаем совпадения с литералами самого теста: голый префикс без
            // сегмента (искомая подстрока) и страницы документации.
            .filter(|p| p.len() > "/api/ext/v1".len() && !DOCS_PATHS.contains(&p.as_str()))
            .collect();
        routed.sort();
        routed.dedup();

        let spec: serde_json::Value =
            serde_json::from_str(include_str!("handlers/ext_openapi.json"))
                .expect("ext_openapi.json — невалидный JSON");
        let mut documented: Vec<String> = spec["paths"]
            .as_object()
            .expect("в спеке нет объекта `paths`")
            .keys()
            .cloned()
            .collect();
        documented.sort();

        assert_eq!(
            routed, documented,
            "список путей /api/ext/v1/* в routes.rs разошёлся с handlers/ext_openapi.json"
        );
    }
}
