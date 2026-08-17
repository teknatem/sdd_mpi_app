//! Static route policy registry.
//!
//! Every endpoint in the application must have exactly one entry here.
//! The registry is the single source of truth for access policy auditing.
//!
//! Convention:
//!   - method "*" = the route group is protected by check_scope (GET→Read, others→All)
//!   - method "GET" + ReadOnly = POST that is read-only (check_scope_read)
//!   - AdminOnly / Public / AuthOnly have scope_id = None
//!
//! When adding a new route to `api/routes.rs` or `system/api/routes.rs`,
//! you MUST add a corresponding entry here. Tests in this module will catch gaps.

use contracts::system::access::{PolicyMode, RoutePolicy};

pub static ROUTE_REGISTRY: &[RoutePolicy] = &[
    // ========================================================================
    // System / Public routes
    // ========================================================================
    RoutePolicy {
        method: "GET",
        path: "/health",
        scope_id: None,
        mode: PolicyMode::Public,
    },
    RoutePolicy {
        method: "POST",
        path: "/api/system/auth/login",
        scope_id: None,
        mode: PolicyMode::Public,
    },
    // Статус обслуживания читается без авторизации намеренно: страницу-заглушку
    // надо показать и тому, кто ещё не вошёл. Управление — только админ.
    RoutePolicy {
        method: "GET",
        path: "/api/system/maintenance",
        scope_id: None,
        mode: PolicyMode::Public,
    },
    RoutePolicy {
        method: "POST",
        path: "/api/system/maintenance/enable",
        scope_id: None,
        mode: PolicyMode::AdminOnly,
    },
    RoutePolicy {
        method: "POST",
        path: "/api/system/maintenance/disable",
        scope_id: None,
        mode: PolicyMode::AdminOnly,
    },
    RoutePolicy {
        method: "POST",
        path: "/api/system/auth/refresh",
        scope_id: None,
        mode: PolicyMode::Public,
    },
    RoutePolicy {
        method: "POST",
        path: "/api/system/auth/logout",
        scope_id: None,
        mode: PolicyMode::Public,
    },
    RoutePolicy {
        method: "GET",
        path: "/api/system/auth/me",
        scope_id: None,
        mode: PolicyMode::AuthOnly,
    },
    // ========================================================================
    // System admin routes
    // ========================================================================
    RoutePolicy {
        method: "*",
        path: "/api/system/users",
        scope_id: None,
        mode: PolicyMode::AdminOnly,
    },
    RoutePolicy {
        method: "*",
        path: "/api/system/users/:id",
        scope_id: None,
        mode: PolicyMode::AdminOnly,
    },
    RoutePolicy {
        method: "*",
        path: "/api/system/users/:id/change-password",
        scope_id: None,
        mode: PolicyMode::AuthOnly,
    },
    RoutePolicy {
        method: "*",
        path: "/api/system/tickets",
        scope_id: None,
        mode: PolicyMode::AuthOnly,
    },
    RoutePolicy {
        method: "*",
        path: "/api/system/tickets/:id",
        scope_id: None,
        mode: PolicyMode::AuthOnly,
    },
    RoutePolicy {
        method: "*",
        path: "/api/system/tickets/:id/comments",
        scope_id: None,
        mode: PolicyMode::AuthOnly,
    },
    RoutePolicy {
        method: "*",
        path: "/api/system/tickets/:id/attachments",
        scope_id: None,
        mode: PolicyMode::AuthOnly,
    },
    RoutePolicy {
        method: "*",
        path: "/api/system/tickets/:id/attachments/:attachment_id",
        scope_id: None,
        mode: PolicyMode::AuthOnly,
    },
    RoutePolicy {
        method: "GET",
        path: "/api/system/tickets/:id/attachments/:attachment_id/download",
        scope_id: None,
        mode: PolicyMode::AuthOnly,
    },
    RoutePolicy {
        method: "*",
        path: "/api/system/roles",
        scope_id: None,
        mode: PolicyMode::AdminOnly,
    },
    RoutePolicy {
        method: "*",
        path: "/api/system/roles/:id",
        scope_id: None,
        mode: PolicyMode::AdminOnly,
    },
    RoutePolicy {
        method: "*",
        path: "/api/system/roles/:id/permissions",
        scope_id: None,
        mode: PolicyMode::AdminOnly,
    },
    RoutePolicy {
        method: "GET",
        path: "/api/system/scopes",
        scope_id: None,
        mode: PolicyMode::AdminOnly,
    },
    RoutePolicy {
        method: "GET",
        path: "/api/system/runtime-info",
        scope_id: None,
        mode: PolicyMode::AuthOnly,
    },
    RoutePolicy {
        method: "*",
        path: "/api/sys/tasks",
        scope_id: None,
        mode: PolicyMode::AdminOnly,
    },
    RoutePolicy {
        method: "*",
        path: "/api/sys/tasks/:id",
        scope_id: None,
        mode: PolicyMode::AdminOnly,
    },
    RoutePolicy {
        method: "*",
        path: "/api/sys/tasks/:id/toggle_enabled",
        scope_id: None,
        mode: PolicyMode::AdminOnly,
    },
    RoutePolicy {
        method: "GET",
        path: "/api/sys/tasks/runs/active/progress",
        scope_id: None,
        mode: PolicyMode::AuthOnly,
    },
    RoutePolicy {
        method: "GET",
        path: "/api/sys/tasks/:id/progress/:session_id",
        scope_id: None,
        mode: PolicyMode::AuthOnly,
    },
    RoutePolicy {
        method: "GET",
        path: "/api/sys/tasks/:id/log/:session_id",
        scope_id: None,
        mode: PolicyMode::AuthOnly,
    },
    RoutePolicy {
        method: "GET",
        path: "/api/sys/ext-api/history",
        scope_id: None,
        mode: PolicyMode::AdminOnly,
    },
    RoutePolicy {
        method: "GET",
        path: "/api/sys/ext-api/summary",
        scope_id: None,
        mode: PolicyMode::AdminOnly,
    },
    RoutePolicy {
        method: "GET",
        path: "/api/sys/ext-api/recent",
        scope_id: None,
        mode: PolicyMode::AdminOnly,
    },
    RoutePolicy {
        method: "*",
        path: "/api/sys/s3/files",
        scope_id: Some("sys_s3_files"),
        mode: PolicyMode::AdminOnly,
    },
    RoutePolicy {
        method: "GET",
        path: "/api/sys/s3/files/:id/download",
        scope_id: Some("sys_s3_files"),
        mode: PolicyMode::AdminOnly,
    },
    RoutePolicy {
        method: "DELETE",
        path: "/api/sys/s3/files/:id",
        scope_id: Some("sys_s3_files"),
        mode: PolicyMode::AdminOnly,
    },
    RoutePolicy {
        method: "GET",
        path: "/api/sys/datasets/status",
        scope_id: Some("sys_datasets"),
        mode: PolicyMode::AdminOnly,
    },
    RoutePolicy {
        method: "GET",
        path: "/api/sys/datasets/catalog",
        scope_id: Some("sys_datasets"),
        mode: PolicyMode::AdminOnly,
    },
    RoutePolicy {
        method: "POST",
        path: "/api/sys/datasets/snapshots",
        scope_id: Some("sys_datasets"),
        mode: PolicyMode::AdminOnly,
    },
    RoutePolicy {
        method: "*",
        path: "/api/sys/datasets/snapshots/:id",
        scope_id: Some("sys_datasets"),
        mode: PolicyMode::AdminOnly,
    },
    RoutePolicy {
        method: "GET",
        path: "/api/sys/datasets/snapshots/:id/download",
        scope_id: Some("sys_datasets"),
        mode: PolicyMode::AdminOnly,
    },
    RoutePolicy {
        method: "POST",
        path: "/api/sys/datasets/restore/preview",
        scope_id: Some("sys_datasets"),
        mode: PolicyMode::AdminOnly,
    },
    RoutePolicy {
        method: "POST",
        path: "/api/sys/datasets/restore",
        scope_id: Some("sys_datasets"),
        mode: PolicyMode::AdminOnly,
    },
    RoutePolicy {
        method: "POST",
        path: "/api/sys/datasets/restore/upload",
        scope_id: Some("sys_datasets"),
        mode: PolicyMode::AdminOnly,
    },
    RoutePolicy {
        method: "GET",
        path: "/api/sys/datasets/history",
        scope_id: Some("sys_datasets"),
        mode: PolicyMode::AdminOnly,
    },
    RoutePolicy {
        method: "GET",
        path: "/api/sys/datasets/jobs/active",
        scope_id: Some("sys_datasets"),
        mode: PolicyMode::AdminOnly,
    },
    RoutePolicy {
        method: "GET",
        path: "/api/sys/datasets/jobs/:job_id",
        scope_id: Some("sys_datasets"),
        mode: PolicyMode::AdminOnly,
    },
    RoutePolicy {
        method: "POST",
        path: "/api/sys/datasets/jobs/:job_id/cancel",
        scope_id: Some("sys_datasets"),
        mode: PolicyMode::AdminOnly,
    },
    RoutePolicy {
        method: "*",
        path: "/api/system/audit/routes",
        scope_id: None,
        mode: PolicyMode::AdminOnly,
    },
    RoutePolicy {
        method: "*",
        path: "/api/system/audit/violations",
        scope_id: None,
        mode: PolicyMode::AdminOnly,
    },
    // Project metrics: снимок состояния экземпляра и кодовой базы.
    // Без scope — это не бизнес-область, а системный паспорт, как и аудит выше.
    RoutePolicy {
        method: "GET",
        path: "/api/system/metrics/latest",
        scope_id: None,
        mode: PolicyMode::AdminOnly,
    },
    RoutePolicy {
        method: "GET",
        path: "/api/system/metrics/series",
        scope_id: None,
        mode: PolicyMode::AdminOnly,
    },
    RoutePolicy {
        method: "GET",
        path: "/api/system/metrics/snapshots",
        scope_id: None,
        mode: PolicyMode::AdminOnly,
    },
    RoutePolicy {
        method: "POST",
        path: "/api/system/metrics/collect",
        scope_id: None,
        mode: PolicyMode::AdminOnly,
    },
    // Utility routes without explicit scope (logs, form-settings — low sensitivity)
    RoutePolicy {
        method: "*",
        path: "/api/logs",
        scope_id: None,
        mode: PolicyMode::AuthOnly,
    },
    RoutePolicy {
        method: "*",
        path: "/api/form-settings/:form_key",
        scope_id: None,
        mode: PolicyMode::AuthOnly,
    },
    RoutePolicy {
        method: "*",
        path: "/api/form-settings",
        scope_id: None,
        mode: PolicyMode::AuthOnly,
    },
    // Debug — open, dev only
    RoutePolicy {
        method: "GET",
        path: "/api/debug/tool-test",
        scope_id: None,
        mode: PolicyMode::Public,
    },
    // ========================================================================
    // Aggregates A001–A029
    // ========================================================================
    RoutePolicy {
        method: "*",
        path: "/api/connection_1c",
        scope_id: Some("a001_connection_1c"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/connection_1c/list",
        scope_id: Some("a001_connection_1c"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/connection_1c/:id",
        scope_id: Some("a001_connection_1c"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/connection_1c/test",
        scope_id: Some("a001_connection_1c"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/connection_1c/testdata",
        scope_id: Some("a001_connection_1c"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/organization",
        scope_id: Some("a002_organization"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/organization/:id",
        scope_id: Some("a002_organization"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/organization/testdata",
        scope_id: Some("a002_organization"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/counterparty",
        scope_id: Some("a003_counterparty"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/counterparty/:id",
        scope_id: Some("a003_counterparty"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/nomenclature",
        scope_id: Some("a004_nomenclature"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/nomenclature/:id",
        scope_id: Some("a004_nomenclature"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/nomenclature/import-excel",
        scope_id: Some("a004_nomenclature"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/nomenclature/dimensions",
        scope_id: Some("a004_nomenclature"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/nomenclature/search",
        scope_id: Some("a004_nomenclature"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/nomenclature/search-by-barcode",
        scope_id: Some("a004_nomenclature"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/a004/nomenclature",
        scope_id: Some("a004_nomenclature"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/marketplace",
        scope_id: Some("a005_marketplace"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/marketplace/:id",
        scope_id: Some("a005_marketplace"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/marketplace/testdata",
        scope_id: Some("a005_marketplace"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/connection_mp",
        scope_id: Some("a006_connection_mp"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/connection_mp/:id",
        scope_id: Some("a006_connection_mp"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/marketplace_product",
        scope_id: Some("a007_marketplace_product"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/marketplace_product/:id",
        scope_id: Some("a007_marketplace_product"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/marketplace_product/testdata",
        scope_id: Some("a007_marketplace_product"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/marketplace_sales",
        scope_id: Some("a008_marketplace_sales"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/marketplace_sales/:id",
        scope_id: Some("a008_marketplace_sales"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/ozon_returns",
        scope_id: Some("a009_ozon_returns"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/ozon_returns/:id",
        scope_id: Some("a009_ozon_returns"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/a010/ozon-fbs-posting",
        scope_id: Some("a010_ozon_fbs_posting"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/a010/ozon-fbs-posting/:id",
        scope_id: Some("a010_ozon_fbs_posting"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/a011/ozon-fbo-posting",
        scope_id: Some("a011_ozon_fbo_posting"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/a011/ozon-fbo-posting/:id",
        scope_id: Some("a011_ozon_fbo_posting"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/a012/wb-sales",
        scope_id: Some("a012_wb_sales"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/a012/wb-sales/:id",
        scope_id: Some("a012_wb_sales"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/a013/ym-order",
        scope_id: Some("a013_ym_order"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/a013/ym-order/:id",
        scope_id: Some("a013_ym_order"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/ozon_transactions",
        scope_id: Some("a014_ozon_transactions"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/ozon_transactions/:id",
        scope_id: Some("a014_ozon_transactions"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/a015/wb-orders",
        scope_id: Some("a015_wb_orders"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/a015/wb-orders/:id",
        scope_id: Some("a015_wb_orders"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/a016/ym-returns",
        scope_id: Some("a016_ym_returns"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/a016/ym-returns/:id",
        scope_id: Some("a016_ym_returns"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/a017-llm-agent",
        scope_id: Some("a017_llm_agent"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/a017-llm-agent/:id",
        scope_id: Some("a017_llm_agent"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/llm-skills",
        scope_id: Some("a017_llm_agent"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/llm-skills/access-matrix",
        scope_id: Some("a017_llm_agent"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "POST",
        path: "/api/llm-skills/reload",
        scope_id: Some("a017_llm_agent"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/a038-llm-connection",
        scope_id: Some("a038_llm_connection"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/a038-llm-connection/:id",
        scope_id: Some("a038_llm_connection"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/a039-mail-message",
        scope_id: Some("a039_mail_message"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/a039-mail-message/:id",
        scope_id: Some("a039_mail_message"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/a018-llm-chat",
        scope_id: Some("a018_llm_chat"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/a018-llm-chat/:id",
        scope_id: Some("a018_llm_chat"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/a018-llm-chat/:id/messages",
        scope_id: Some("a018_llm_chat"),
        mode: PolicyMode::Auto,
    },
    // Рабочий каталог чата: задачи, анкеты, планы, журнал шагов.
    // Соседние записи выше используют исторический префикс /api/llm-chat; здесь —
    // фактически обслуживаемый путь из routes.rs.
    RoutePolicy {
        method: "*",
        path: "/api/a018-llm-chat/:id/workspace",
        scope_id: Some("a018_llm_chat"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/a018-llm-chat/:id/workspace/active",
        scope_id: Some("a018_llm_chat"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/a018-llm-chat/:id/workspace/file/*path",
        scope_id: Some("a018_llm_chat"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/a018-llm-chat/:id/workspace/answer",
        scope_id: Some("a018_llm_chat"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/a019-llm-artifact",
        scope_id: Some("a019_llm_artifact"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/a019-llm-artifact/:id",
        scope_id: Some("a019_llm_artifact"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/a020/wb-promotions",
        scope_id: Some("a020_wb_promotion"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/a020/wb-promotions/:id",
        scope_id: Some("a020_wb_promotion"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/a021/production-output/list",
        scope_id: Some("a021_production_output"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/a021/production-output/:id",
        scope_id: Some("a021_production_output"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/a022/kit-variant/list",
        scope_id: Some("a022_kit_variant"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/a022/kit-variant/:id",
        scope_id: Some("a022_kit_variant"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/a023/purchase-of-goods/list",
        scope_id: Some("a023_purchase_of_goods"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/a023/purchase-of-goods/:id",
        scope_id: Some("a023_purchase_of_goods"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/a024-bi-indicator",
        scope_id: Some("a024_bi_indicator"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/a024-bi-indicator/:id",
        scope_id: Some("a024_bi_indicator"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/a024-bi-indicator/resolve-batch",
        scope_id: Some("a024_bi_indicator"),
        mode: PolicyMode::ReadOnly,
    },
    RoutePolicy {
        method: "*",
        path: "/api/a024-bi-indicator/:id/compute",
        scope_id: Some("a024_bi_indicator"),
        mode: PolicyMode::ReadOnly,
    },
    RoutePolicy {
        method: "*",
        path: "/api/a024-bi-indicator/compute-batch",
        scope_id: Some("a024_bi_indicator"),
        mode: PolicyMode::ReadOnly,
    },
    RoutePolicy {
        method: "*",
        path: "/api/drilldown/execute",
        scope_id: Some("a024_bi_indicator"),
        mode: PolicyMode::ReadOnly,
    },
    RoutePolicy {
        method: "*",
        path: "/api/a025-bi-dashboard",
        scope_id: Some("a025_bi_dashboard"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/a025-bi-dashboard/:id",
        scope_id: Some("a025_bi_dashboard"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/a031-kb-edit",
        scope_id: Some("a031_kb_edit"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/a031-kb-edit/:id",
        scope_id: Some("a031_kb_edit"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/a031-kb-edit/:id/approve",
        scope_id: Some("a031_kb_edit"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/a031-kb-edit/:id/cancel",
        scope_id: Some("a031_kb_edit"),
        mode: PolicyMode::Auto,
    },
    // a042 — очередь поручений между AI-сотрудниками
    RoutePolicy {
        method: "*",
        path: "/api/a042-agent-task",
        scope_id: Some("a042_agent_task"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/a042-agent-task/list",
        scope_id: Some("a042_agent_task"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/a042-agent-task/:id",
        scope_id: Some("a042_agent_task"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/a042-agent-task/:id/cancel",
        scope_id: Some("a042_agent_task"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/a042-agent-task/:id/requeue",
        scope_id: Some("a042_agent_task"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "GET",
        path: "/api/a032/wb-returns-claims",
        scope_id: Some("a032_wb_returns_claims"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "GET",
        path: "/api/a032/wb-returns-claims/:id",
        scope_id: Some("a032_wb_returns_claims"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/a033/wb-day-close",
        scope_id: Some("a033_wb_day_close"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/a033/wb-day-close/compare",
        scope_id: Some("a033_wb_day_close"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/a033/wb-day-close/by-day/:connection_id/:business_date",
        scope_id: Some("a033_wb_day_close"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/a033/wb-day-close/:id",
        scope_id: Some("a033_wb_day_close"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/a033/wb-day-close/:id/recalculate",
        scope_id: Some("a033_wb_day_close"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/a033/wb-day-close/:id/repost-problematic-a012",
        scope_id: Some("a033_wb_day_close"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/a033/wb-day-close/:id/archive-and-recreate",
        scope_id: Some("a033_wb_day_close"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/bi-timeline/indicators",
        scope_id: Some("bi_timeline"),
        mode: PolicyMode::ReadOnly,
    },
    RoutePolicy {
        method: "*",
        path: "/api/bi-timeline/series",
        scope_id: Some("bi_timeline"),
        mode: PolicyMode::ReadOnly,
    },
    RoutePolicy {
        method: "*",
        path: "/api/a026/wb-advert-daily/list",
        scope_id: Some("a026_wb_advert_daily"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/a034/ym-realization/list",
        scope_id: Some("a034_ym_realization"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/a034/ym-realization/:id",
        scope_id: Some("a034_ym_realization"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/a026/wb-advert-daily/report.csv",
        scope_id: Some("a026_wb_advert_daily"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/a026/wb-advert-daily/:id",
        scope_id: Some("a026_wb_advert_daily"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/a036/wb-sales-funnel/list",
        scope_id: Some("a036_wb_sales_funnel_daily"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/a036/wb-sales-funnel/:id",
        scope_id: Some("a036_wb_sales_funnel_daily"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/a037/wb-product-snapshot/list",
        scope_id: Some("a037_wb_product_snapshot"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/a037/wb-product-snapshot/series",
        scope_id: Some("a037_wb_product_snapshot"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/a037/wb-product-snapshot/:id",
        scope_id: Some("a037_wb_product_snapshot"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/a040/wb-search-analytics/list",
        scope_id: Some("a040_wb_search_analytics_daily"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/a040/wb-search-analytics/:id",
        scope_id: Some("a040_wb_search_analytics_daily"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/a041/ym-shows-sales/list",
        scope_id: Some("a041_ym_shows_sales_daily"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/a041/ym-shows-sales/:id",
        scope_id: Some("a041_ym_shows_sales_daily"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/a043/wb-finance-reports/list",
        scope_id: Some("a043_wb_finance_report"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/a043/wb-finance-reports/:id",
        scope_id: Some("a043_wb_finance_report"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/a043/wb-finance-reports/:id/lines",
        scope_id: Some("a043_wb_finance_report"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/a027/wb-documents/list",
        scope_id: Some("a027_wb_documents"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/a027/wb-documents/:id",
        scope_id: Some("a027_wb_documents"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/a028/missing-cost-registry/list",
        scope_id: Some("a028_missing_cost_registry"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/a028/missing-cost-registry/:id",
        scope_id: Some("a028_missing_cost_registry"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/a029/wb-supply",
        scope_id: Some("a029_wb_supply"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/a029/wb-supply/:id",
        scope_id: Some("a029_wb_supply"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/a029/wb-supply/by-wb-id/:wb_id",
        scope_id: Some("a029_wb_supply"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/a029/wb-supply/by-order/:order_id",
        scope_id: Some("a029_wb_supply"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/a029/raw/:ref_id",
        scope_id: Some("a029_wb_supply"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/a030/wb-advert-campaign/list",
        scope_id: Some("a030_wb_advert_campaign"),
        mode: PolicyMode::ReadOnly,
    },
    RoutePolicy {
        method: "*",
        path: "/api/a030/wb-advert-campaign/:id",
        scope_id: Some("a030_wb_advert_campaign"),
        mode: PolicyMode::ReadOnly,
    },
    // ========================================================================
    // Projections P900–P912
    // ========================================================================
    RoutePolicy {
        method: "*",
        path: "/api/p900/sales-register",
        scope_id: Some("p900_mp_sales_register"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/p900/sales-register/:marketplace/:document_no/:line_id",
        scope_id: Some("p900_mp_sales_register"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/p900/stats/by-date",
        scope_id: Some("p900_mp_sales_register"),
        mode: PolicyMode::ReadOnly,
    },
    RoutePolicy {
        method: "*",
        path: "/api/p900/stats/by-marketplace",
        scope_id: Some("p900_mp_sales_register"),
        mode: PolicyMode::ReadOnly,
    },
    RoutePolicy {
        method: "*",
        path: "/api/p900/backfill-product-refs",
        scope_id: Some("p900_mp_sales_register"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/projections/p900/:registrator_ref",
        scope_id: Some("p900_mp_sales_register"),
        mode: PolicyMode::ReadOnly,
    },
    RoutePolicy {
        method: "*",
        path: "/api/p901/barcode/:barcode",
        scope_id: Some("p901_nomenclature_barcodes"),
        mode: PolicyMode::ReadOnly,
    },
    RoutePolicy {
        method: "*",
        path: "/api/p901/nomenclature/:nomenclature_ref/barcodes",
        scope_id: Some("p901_nomenclature_barcodes"),
        mode: PolicyMode::ReadOnly,
    },
    RoutePolicy {
        method: "*",
        path: "/api/p901/barcodes",
        scope_id: Some("p901_nomenclature_barcodes"),
        mode: PolicyMode::ReadOnly,
    },
    RoutePolicy {
        method: "*",
        path: "/api/p902/finance-realization",
        scope_id: Some("p902_ozon_finance_realization"),
        mode: PolicyMode::ReadOnly,
    },
    RoutePolicy {
        method: "*",
        path: "/api/p902/finance-realization/:posting_number/:sku/:operation_type",
        scope_id: Some("p902_ozon_finance_realization"),
        mode: PolicyMode::ReadOnly,
    },
    RoutePolicy {
        method: "*",
        path: "/api/p902/stats",
        scope_id: Some("p902_ozon_finance_realization"),
        mode: PolicyMode::ReadOnly,
    },
    RoutePolicy {
        method: "*",
        path: "/api/p903/finance-report",
        scope_id: Some("p903_wb_finance_report"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/p903/finance-report/export",
        scope_id: Some("p903_wb_finance_report"),
        mode: PolicyMode::ReadOnly,
    },
    RoutePolicy {
        method: "*",
        path: "/api/p903/finance-report/search-by-srid",
        scope_id: Some("p903_wb_finance_report"),
        mode: PolicyMode::ReadOnly,
    },
    RoutePolicy {
        method: "*",
        path: "/api/p903/finance-report/operation-kinds",
        scope_id: Some("p903_wb_finance_report"),
        mode: PolicyMode::ReadOnly,
    },
    RoutePolicy {
        method: "*",
        path: "/api/p903/finance-report/by-id/:id",
        scope_id: Some("p903_wb_finance_report"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/p903/finance-report/by-id/:id/raw",
        scope_id: Some("p903_wb_finance_report"),
        mode: PolicyMode::ReadOnly,
    },
    RoutePolicy {
        method: "*",
        path: "/api/p904/sales-data",
        scope_id: Some("p904_sales_data"),
        mode: PolicyMode::ReadOnly,
    },
    RoutePolicy {
        method: "*",
        path: "/api/p905-commission/list",
        scope_id: Some("p905_wb_commission_history"),
        mode: PolicyMode::ReadOnly,
    },
    RoutePolicy {
        method: "*",
        path: "/api/p905-commission/sync",
        scope_id: Some("p905_wb_commission_history"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/p905-commission/:id",
        scope_id: Some("p905_wb_commission_history"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/p905-commission",
        scope_id: Some("p905_wb_commission_history"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/p906/nomenclature-prices",
        scope_id: Some("p906_nomenclature_prices"),
        mode: PolicyMode::ReadOnly,
    },
    RoutePolicy {
        method: "*",
        path: "/api/p906/periods",
        scope_id: Some("p906_nomenclature_prices"),
        mode: PolicyMode::ReadOnly,
    },
    RoutePolicy {
        method: "*",
        path: "/api/p906/import-excel",
        scope_id: Some("p906_nomenclature_prices"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/p907/payment-report",
        scope_id: Some("p907_ym_payment_report"),
        mode: PolicyMode::ReadOnly,
    },
    RoutePolicy {
        method: "GET",
        path: "/api/p907/payment-report/filter-options",
        scope_id: Some("p907_ym_payment_report"),
        mode: PolicyMode::ReadOnly,
    },
    RoutePolicy {
        method: "POST",
        path: "/api/p907/payment-report/migrate-keys",
        scope_id: Some("p907_ym_payment_report"),
        mode: PolicyMode::ReadOnly,
    },
    RoutePolicy {
        method: "GET",
        path: "/api/p907/payment-report/:id",
        scope_id: Some("p907_ym_payment_report"),
        mode: PolicyMode::ReadOnly,
    },
    RoutePolicy {
        method: "*",
        path: "/api/p908/goods-prices",
        scope_id: Some("p908_wb_goods_prices"),
        mode: PolicyMode::ReadOnly,
    },
    RoutePolicy {
        method: "*",
        path: "/api/p908/goods-prices/:nm_id",
        scope_id: Some("p908_wb_goods_prices"),
        mode: PolicyMode::ReadOnly,
    },
    RoutePolicy {
        method: "*",
        path: "/api/p912/nomenclature-costs",
        scope_id: Some("p912_nomenclature_costs"),
        mode: PolicyMode::ReadOnly,
    },
    // ========================================================================
    // Usecases U501–U508
    // ========================================================================
    RoutePolicy {
        method: "*",
        path: "/api/u501/import/start",
        scope_id: Some("u501_import_from_ut"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/u501/import/:session_id/progress",
        scope_id: Some("u501_import_from_ut"),
        mode: PolicyMode::ReadOnly,
    },
    RoutePolicy {
        method: "*",
        path: "/api/u502/import/start",
        scope_id: Some("u502_import_from_ozon"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/u502/import/:session_id/progress",
        scope_id: Some("u502_import_from_ozon"),
        mode: PolicyMode::ReadOnly,
    },
    RoutePolicy {
        method: "*",
        path: "/api/u503/import/start",
        scope_id: Some("u503_import_from_yandex"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/u503/import/:session_id/progress",
        scope_id: Some("u503_import_from_yandex"),
        mode: PolicyMode::ReadOnly,
    },
    RoutePolicy {
        method: "*",
        path: "/api/u504/import/start",
        scope_id: Some("u504_import_from_wildberries"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/u504/import/:session_id/progress",
        scope_id: Some("u504_import_from_wildberries"),
        mode: PolicyMode::ReadOnly,
    },
    RoutePolicy {
        method: "*",
        path: "/api/u505/match/start",
        scope_id: Some("u505_match_nomenclature"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/u505/match/:session_id/progress",
        scope_id: Some("u505_match_nomenclature"),
        mode: PolicyMode::ReadOnly,
    },
    RoutePolicy {
        method: "*",
        path: "/api/u506/import/start",
        scope_id: Some("u506_import_from_lemanapro"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/u506/import/:session_id/progress",
        scope_id: Some("u506_import_from_lemanapro"),
        mode: PolicyMode::ReadOnly,
    },
    RoutePolicy {
        method: "*",
        path: "/api/u507/import/start",
        scope_id: Some("u507_import_from_erp"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/u507/import/:session_id/progress",
        scope_id: Some("u507_import_from_erp"),
        mode: PolicyMode::ReadOnly,
    },
    RoutePolicy {
        method: "*",
        path: "/api/u508/repost/projections",
        scope_id: Some("u508_repost_documents"),
        mode: PolicyMode::ReadOnly,
    },
    RoutePolicy {
        method: "*",
        path: "/api/u508/repost/aggregates",
        scope_id: Some("u508_repost_documents"),
        mode: PolicyMode::ReadOnly,
    },
    RoutePolicy {
        method: "*",
        path: "/api/u508/repost/start",
        scope_id: Some("u508_repost_documents"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/u508/repost/aggregate/start",
        scope_id: Some("u508_repost_documents"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/u508/repost/:session_id/progress",
        scope_id: Some("u508_repost_documents"),
        mode: PolicyMode::ReadOnly,
    },
    // ========================================================================
    // Dashboards
    // ========================================================================
    RoutePolicy {
        method: "*",
        path: "/api/d400/monthly_summary",
        scope_id: Some("dashboard"),
        mode: PolicyMode::ReadOnly,
    },
    RoutePolicy {
        method: "*",
        path: "/api/d400/periods",
        scope_id: Some("dashboard"),
        mode: PolicyMode::ReadOnly,
    },
    RoutePolicy {
        method: "*",
        path: "/api/universal-dashboard/execute",
        scope_id: Some("dashboard"),
        mode: PolicyMode::ReadOnly,
    },
    RoutePolicy {
        method: "*",
        path: "/api/universal-dashboard/generate-sql",
        scope_id: Some("dashboard"),
        mode: PolicyMode::ReadOnly,
    },
    RoutePolicy {
        method: "*",
        path: "/api/universal-dashboard/schemas",
        scope_id: Some("dashboard"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/universal-dashboard/schemas/:id",
        scope_id: Some("dashboard"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/universal-dashboard/configs",
        scope_id: Some("dashboard"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/universal-dashboard/configs/:id",
        scope_id: Some("dashboard"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/ds01/execute",
        scope_id: Some("dashboard"),
        mode: PolicyMode::ReadOnly,
    },
    RoutePolicy {
        method: "*",
        path: "/api/ds01/schemas",
        scope_id: Some("dashboard"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/ds01/configs",
        scope_id: Some("dashboard"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/ds02/execute",
        scope_id: Some("dashboard"),
        mode: PolicyMode::ReadOnly,
    },
    RoutePolicy {
        method: "*",
        path: "/api/ds02/schemas",
        scope_id: Some("dashboard"),
        mode: PolicyMode::Auto,
    },
    RoutePolicy {
        method: "*",
        path: "/api/ds02/configs",
        scope_id: Some("dashboard"),
        mode: PolicyMode::Auto,
    },
    // ========================================================================
    // Data Views
    // ========================================================================
    RoutePolicy {
        method: "*",
        path: "/api/data-view",
        scope_id: Some("data_view"),
        mode: PolicyMode::ReadOnly,
    },
    RoutePolicy {
        method: "*",
        path: "/api/data-view/filters",
        scope_id: Some("data_view"),
        mode: PolicyMode::ReadOnly,
    },
    RoutePolicy {
        method: "*",
        path: "/api/data-view/:id",
        scope_id: Some("data_view"),
        mode: PolicyMode::ReadOnly,
    },
    RoutePolicy {
        method: "*",
        path: "/api/data-view/:id/filters",
        scope_id: Some("data_view"),
        mode: PolicyMode::ReadOnly,
    },
    RoutePolicy {
        method: "*",
        path: "/api/data-view/:id/compute",
        scope_id: Some("data_view"),
        mode: PolicyMode::ReadOnly,
    },
    RoutePolicy {
        method: "*",
        path: "/api/data-view/:id/drilldown",
        scope_id: Some("data_view"),
        mode: PolicyMode::ReadOnly,
    },
    RoutePolicy {
        method: "*",
        path: "/api/data-view/:id/drilldown-capabilities",
        scope_id: Some("data_view"),
        mode: PolicyMode::ReadOnly,
    },
    // ========================================================================
    // General Ledger
    // ========================================================================
    RoutePolicy {
        method: "*",
        path: "/api/general-ledger",
        scope_id: Some("general_ledger"),
        mode: PolicyMode::ReadOnly,
    },
    RoutePolicy {
        method: "*",
        path: "/api/general-ledger/turnovers",
        scope_id: Some("general_ledger"),
        mode: PolicyMode::ReadOnly,
    },
    RoutePolicy {
        method: "*",
        path: "/api/general-ledger/report",
        scope_id: Some("general_ledger"),
        mode: PolicyMode::ReadOnly,
    },
    RoutePolicy {
        method: "*",
        path: "/api/general-ledger/account-view",
        scope_id: Some("general_ledger"),
        mode: PolicyMode::ReadOnly,
    },
    RoutePolicy {
        method: "*",
        path: "/api/reports/wb-weekly-reconciliation",
        scope_id: Some("general_ledger"),
        mode: PolicyMode::ReadOnly,
    },
    RoutePolicy {
        method: "GET",
        path: "/api/reports/ym-revenue-reconciliation",
        scope_id: Some("a034_ym_realization"),
        mode: PolicyMode::ReadOnly,
    },
    RoutePolicy {
        method: "*",
        path: "/api/general-ledger/report/dimensions",
        scope_id: Some("general_ledger"),
        mode: PolicyMode::ReadOnly,
    },
    RoutePolicy {
        method: "*",
        path: "/api/general-ledger/report/drilldown",
        scope_id: Some("general_ledger"),
        mode: PolicyMode::ReadOnly,
    },
    RoutePolicy {
        method: "*",
        path: "/api/general-ledger/drilldown",
        scope_id: Some("general_ledger"),
        mode: PolicyMode::ReadOnly,
    },
    RoutePolicy {
        method: "*",
        path: "/api/general-ledger/drilldown/:id",
        scope_id: Some("general_ledger"),
        mode: PolicyMode::ReadOnly,
    },
    RoutePolicy {
        method: "*",
        path: "/api/general-ledger/drilldown/:id/data",
        scope_id: Some("general_ledger"),
        mode: PolicyMode::ReadOnly,
    },
    RoutePolicy {
        method: "*",
        path: "/api/general-ledger/:id",
        scope_id: Some("general_ledger"),
        mode: PolicyMode::ReadOnly,
    },
    // LLM Knowledge (read-only reference data — same scope as llm chat for simplicity)
    RoutePolicy {
        method: "*",
        path: "/api/llm-knowledge",
        scope_id: Some("a018_llm_chat"),
        mode: PolicyMode::ReadOnly,
    },
    RoutePolicy {
        method: "*",
        path: "/api/llm-knowledge/:id",
        scope_id: Some("a018_llm_chat"),
        mode: PolicyMode::ReadOnly,
    },
    // Knowledge Base workspace (read-only)
    RoutePolicy {
        method: "*",
        path: "/api/kb/stats",
        scope_id: Some("knowledge_base"),
        mode: PolicyMode::ReadOnly,
    },
    RoutePolicy {
        method: "*",
        path: "/api/kb/tree",
        scope_id: Some("knowledge_base"),
        mode: PolicyMode::ReadOnly,
    },
    RoutePolicy {
        method: "*",
        path: "/api/kb/articles/:id",
        scope_id: Some("knowledge_base"),
        mode: PolicyMode::ReadOnly,
    },
    RoutePolicy {
        method: "*",
        path: "/api/kb/vocabulary",
        scope_id: Some("knowledge_base"),
        mode: PolicyMode::ReadOnly,
    },
    RoutePolicy {
        method: "*",
        path: "/api/kb/issues",
        scope_id: Some("knowledge_base"),
        mode: PolicyMode::ReadOnly,
    },
    // Перечитывание базы с диска — действие, а не чтение.
    RoutePolicy {
        method: "*",
        path: "/api/kb/reload",
        scope_id: Some("knowledge_base"),
        mode: PolicyMode::Auto,
    },
    // Пересборка карт из БД и рантайма — тем более действие.
    RoutePolicy {
        method: "*",
        path: "/api/kb/generate",
        scope_id: Some("knowledge_base"),
        mode: PolicyMode::Auto,
    },
    // Sys-drilldown session store (internal; tied to data_view usage)
    RoutePolicy {
        method: "*",
        path: "/api/sys-drilldown",
        scope_id: Some("data_view"),
        mode: PolicyMode::ReadOnly,
    },
    RoutePolicy {
        method: "*",
        path: "/api/sys-drilldown/:id",
        scope_id: Some("data_view"),
        mode: PolicyMode::ReadOnly,
    },
    RoutePolicy {
        method: "*",
        path: "/api/sys-drilldown/:id/data",
        scope_id: Some("data_view"),
        mode: PolicyMode::ReadOnly,
    },
    // Quality check routes (authenticated, no scope required)
    RoutePolicy {
        method: "GET",
        path: "/api/quality/checks",
        scope_id: None,
        mode: PolicyMode::AuthOnly,
    },
    RoutePolicy {
        method: "GET",
        path: "/api/quality/checks/overview",
        scope_id: None,
        mode: PolicyMode::AuthOnly,
    },
    RoutePolicy {
        method: "POST",
        path: "/api/quality/checks/reload",
        scope_id: None,
        mode: PolicyMode::AdminOnly,
    },
    RoutePolicy {
        method: "POST",
        path: "/api/quality/checks/:id/run",
        scope_id: None,
        mode: PolicyMode::AuthOnly,
    },
    RoutePolicy {
        method: "GET",
        path: "/api/quality/checks/:id/details",
        scope_id: None,
        mode: PolicyMode::AuthOnly,
    },
    RoutePolicy {
        method: "GET",
        path: "/api/quality/checks/:id/runs",
        scope_id: None,
        mode: PolicyMode::AuthOnly,
    },
    RoutePolicy {
        method: "GET",
        path: "/api/quality/checks/:id/sources",
        scope_id: None,
        mode: PolicyMode::AuthOnly,
    },
    RoutePolicy {
        method: "GET",
        path: "/api/quality/checks/:id/groups",
        scope_id: None,
        mode: PolicyMode::AuthOnly,
    },
    RoutePolicy {
        method: "GET",
        path: "/api/quality/checks/:id/rows",
        scope_id: None,
        mode: PolicyMode::AuthOnly,
    },
    RoutePolicy {
        method: "POST",
        path: "/api/quality/checks/:id/repost",
        scope_id: None,
        mode: PolicyMode::AdminOnly,
    },
    RoutePolicy {
        method: "POST",
        path: "/api/quality/checks/:id/cleanup",
        scope_id: None,
        mode: PolicyMode::AdminOnly,
    },
    // ========================================================================
    // Plugins subsystem — надстройка над платформой
    // (использование — auth-only, управление — admin-only)
    // ========================================================================
    RoutePolicy {
        method: "GET",
        path: "/api/plugin",
        scope_id: None,
        mode: PolicyMode::AuthOnly,
    },
    RoutePolicy {
        method: "POST",
        path: "/api/plugin",
        scope_id: None,
        mode: PolicyMode::AdminOnly,
    },
    RoutePolicy {
        method: "GET",
        path: "/api/plugin/all",
        scope_id: None,
        mode: PolicyMode::AdminOnly,
    },
    RoutePolicy {
        method: "POST",
        path: "/api/plugin/validate",
        scope_id: None,
        mode: PolicyMode::AdminOnly,
    },
    RoutePolicy {
        method: "POST",
        path: "/api/plugin/smoke-test",
        scope_id: None,
        mode: PolicyMode::AdminOnly,
    },
    RoutePolicy {
        method: "POST",
        path: "/api/plugin/testdata",
        scope_id: None,
        mode: PolicyMode::AdminOnly,
    },
    RoutePolicy {
        method: "GET",
        path: "/api/plugin/:id",
        scope_id: None,
        mode: PolicyMode::AuthOnly,
    },
    RoutePolicy {
        method: "DELETE",
        path: "/api/plugin/:id",
        scope_id: None,
        mode: PolicyMode::AdminOnly,
    },
    RoutePolicy {
        method: "POST",
        path: "/api/plugin/import",
        scope_id: None,
        mode: PolicyMode::AdminOnly,
    },
    RoutePolicy {
        method: "GET",
        path: "/api/plugin/runs/summary",
        scope_id: None,
        mode: PolicyMode::AdminOnly,
    },
    RoutePolicy {
        method: "GET",
        path: "/api/plugin/:id/export",
        scope_id: None,
        mode: PolicyMode::AdminOnly,
    },
    RoutePolicy {
        method: "GET",
        path: "/api/plugin/:id/stats",
        scope_id: None,
        mode: PolicyMode::AdminOnly,
    },
    RoutePolicy {
        method: "POST",
        path: "/api/plugin/:id/data",
        scope_id: None,
        mode: PolicyMode::AuthOnly,
    },
    RoutePolicy {
        method: "POST",
        path: "/api/plugin/:id/dev-invoke",
        scope_id: None,
        mode: PolicyMode::AdminOnly,
    },
    RoutePolicy {
        method: "POST",
        path: "/api/plugin/:id/invoke",
        scope_id: None,
        mode: PolicyMode::AuthOnly,
    },
];

/// Look up the policy entries for a given scope_id.
pub fn policies_for_scope(scope_id: &str) -> Vec<&'static RoutePolicy> {
    ROUTE_REGISTRY
        .iter()
        .filter(|p| p.scope_id == Some(scope_id))
        .collect()
}

// ============================================================================
// Integrity tests
// ============================================================================
#[cfg(test)]
mod tests {
    use super::*;
    use crate::system::access::scope_catalog::SCOPE_CATALOG;

    /// Сколько объявленных маршрутов сейчас живут без записи в реестре.
    ///
    /// Заголовок этого модуля обещает «every endpoint … must have exactly one
    /// entry here» и «tests in this module will catch gaps». Ни то, ни другое
    /// не выполнялось: три существовавших теста проверяли только обратное
    /// направление — что у записи валидный scope и что каждый scope кем-то
    /// покрыт. Забытый маршрут проходил молча, и так набралось 250 штук из 523.
    ///
    /// Почему число, а не ноль. Запись в реестре — это не формальность, а
    /// решение: какой `scope_id` и какой `PolicyMode` у эндпоинта. Проставить
    /// 250 таких решений «оптом» значит проставить их наугад, а неверная запись
    /// о политике доступа хуже отсутствующей — она выглядит как проверенная.
    /// Поэтому здесь храповик: новый маршрут без записи валит сборку сразу,
    /// а накопленный долг разбирается порциями, с осознанным выбором политики.
    ///
    /// Уменьшили долг — уменьшите и константу: тест этого требует, иначе
    /// планка тихо перестанет держать.
    const ROUTES_WITHOUT_POLICY_BASELINE: usize = 221;

    /// Пути всех `.route("…")` из обоих файлов маршрутов.
    ///
    /// Разбор строкой, а не по AST — тем же приёмом, что и
    /// `openapi_spec_covers_every_ext_route` в `api/routes.rs`: форма объявления
    /// в проекте одна и та же (`.route(` и следом строковый литерал), никаких
    /// `.nest()` нет, и городить ради этого парсер незачем.
    fn declared_route_paths() -> std::collections::BTreeSet<String> {
        const SOURCES: [&str; 2] = [
            include_str!("../../api/routes.rs"),
            include_str!("../api/routes.rs"),
        ];

        let mut paths = std::collections::BTreeSet::new();
        for src in SOURCES {
            for (index, _) in src.match_indices(".route(") {
                let rest = &src[index + ".route(".len()..];
                // Между `.route(` и литералом бывает перенос строки — rustfmt
                // разносит длинные вызовы. Всё до кавычки должно быть пробелами:
                // иначе это не объявление маршрута, а совпадение в комментарии.
                let Some(quote) = rest.find('"') else {
                    continue;
                };
                if !rest[..quote].chars().all(char::is_whitespace) {
                    continue;
                }
                let after = &rest[quote + 1..];
                let Some(end) = after.find('"') else { continue };
                let path = &after[..end];
                if path.starts_with('/') {
                    paths.insert(path.to_string());
                }
            }
        }
        paths
    }

    #[test]
    fn every_declared_route_has_a_policy_entry() {
        let registered: std::collections::HashSet<&str> =
            ROUTE_REGISTRY.iter().map(|p| p.path).collect();

        let missing: Vec<String> = declared_route_paths()
            .into_iter()
            .filter(|path| !registered.contains(path.as_str()))
            .collect();

        if missing.len() > ROUTES_WITHOUT_POLICY_BASELINE {
            let added = missing.len() - ROUTES_WITHOUT_POLICY_BASELINE;
            panic!(
                "Маршрутов без записи в ROUTE_REGISTRY стало больше: {} вместо {ROUTES_WITHOUT_POLICY_BASELINE} (+{added}).\n\
                 Добавьте RoutePolicy для нового эндпоинта — со scope_id и PolicyMode, а не по образцу соседа.\n\
                 Сейчас без политики:\n{}",
                missing.len(),
                missing.join("\n")
            );
        }

        assert!(
            missing.len() >= ROUTES_WITHOUT_POLICY_BASELINE,
            "Маршрутов без политики стало меньше: {} вместо {ROUTES_WITHOUT_POLICY_BASELINE}. \
             Опустите ROUTES_WITHOUT_POLICY_BASELINE до {}, иначе храповик перестанет держать достигнутое.",
            missing.len(),
            missing.len()
        );
    }

    /// Записи о политике для маршрутов, которых больше нет. **Долг закрыт.**
    ///
    /// Было 31 — следы переименования путей на префиксы `aXXX`
    /// (`/api/wb_sales/:id` вместо нынешнего `/api/a012/wb-sales/:id`,
    /// `/api/llm-chat/:id` вместо `/api/a018-llm-chat/:id`). Решение о политике
    /// никуда не девалось — у него разъехался адрес, поэтому 29 записей
    /// **переуказаны** с сохранением `scope_id` и `PolicyMode`, а не заведены
    /// заново. Тем же движением долг в [`ROUTES_WITHOUT_POLICY_BASELINE`]
    /// сократился с 250 до 221.
    ///
    /// Две записи удалены, потому что эндпоинтов действительно больше нет:
    /// `/api/connection_mp/testdata` (у a006 её не осталось, `testdata` есть
    /// только у a001) и `/api/llm-chat/:id/run` (синхронный запуск заменён
    /// очередью `/api/a018-llm-chat/jobs/:job_id`).
    ///
    /// Ноль означает, что фантом теперь запрещён: переименовали путь — перенесите
    /// запись, а не заводите вторую.
    const ORPHAN_POLICY_BASELINE: usize = 0;

    /// Обратная сторона реестра: запись создаёт впечатление, что эндпоинт
    /// существует и проверен, и переживает переименование кода незамеченной.
    /// Страница аудита доступа считает такие записи наравне с настоящими.
    #[test]
    fn every_policy_entry_points_at_a_declared_route() {
        let declared = declared_route_paths();
        let orphans: Vec<&str> = ROUTE_REGISTRY
            .iter()
            .map(|p| p.path)
            .filter(|path| !declared.contains(*path))
            .collect();

        if orphans.len() > ORPHAN_POLICY_BASELINE {
            let added = orphans.len() - ORPHAN_POLICY_BASELINE;
            panic!(
                "В ROUTE_REGISTRY стало больше записей без маршрута: {} вместо {ORPHAN_POLICY_BASELINE} (+{added}).\n\
                 Переименовали путь — перенесите и запись, а не заводите вторую.\n\
                 Сейчас без маршрута:\n{}",
                orphans.len(),
                orphans.join("\n")
            );
        }

        assert!(
            orphans.len() >= ORPHAN_POLICY_BASELINE,
            "Фантомных записей стало меньше: {} вместо {ORPHAN_POLICY_BASELINE}. \
             Опустите ORPHAN_POLICY_BASELINE до {}, иначе храповик перестанет держать достигнутое.",
            orphans.len(),
            orphans.len()
        );
    }

    #[test]
    fn all_scoped_routes_have_known_scope_id() {
        let catalog_ids: std::collections::HashSet<&str> =
            SCOPE_CATALOG.iter().map(|s| s.scope_id).collect();

        let mut failures = Vec::new();
        for policy in ROUTE_REGISTRY {
            if let Some(scope_id) = policy.scope_id {
                if !catalog_ids.contains(scope_id) {
                    failures.push(format!(
                        "ROUTE_REGISTRY entry {} {} references unknown scope_id '{}'",
                        policy.method, policy.path, scope_id
                    ));
                }
            }
        }

        if !failures.is_empty() {
            panic!(
                "Routes reference unknown scope IDs:\n{}",
                failures.join("\n")
            );
        }
    }

    #[test]
    fn all_catalog_scopes_covered_by_at_least_one_route() {
        let registry_scopes: std::collections::HashSet<&str> =
            ROUTE_REGISTRY.iter().filter_map(|p| p.scope_id).collect();

        let mut orphans = Vec::new();
        for scope in SCOPE_CATALOG {
            if !registry_scopes.contains(scope.scope_id) {
                orphans.push(scope.scope_id);
            }
        }

        if !orphans.is_empty() {
            panic!(
                "SCOPE_CATALOG entries not covered by any route:\n{}",
                orphans.join("\n")
            );
        }
    }

    #[test]
    fn no_auth_only_routes() {
        let auth_only: Vec<_> = ROUTE_REGISTRY
            .iter()
            .filter(|p| p.mode == PolicyMode::AuthOnly)
            .map(|p| format!("{} {}", p.method, p.path))
            .collect();

        if !auth_only.is_empty() {
            // This is a warning, not a hard failure — AuthOnly routes are
            // documented violations that should be resolved over time.
            eprintln!(
                "WARNING: {} AuthOnly routes (no scope assigned):\n{}",
                auth_only.len(),
                auth_only.join("\n")
            );
        }
        // Do not panic — these are known and tracked via the audit endpoint.
    }

    #[test]
    fn plugin_routes_are_registered() {
        let expected = [
            ("GET", "/api/plugin"),
            ("POST", "/api/plugin"),
            ("GET", "/api/plugin/all"),
            ("POST", "/api/plugin/validate"),
            ("POST", "/api/plugin/smoke-test"),
            ("POST", "/api/plugin/testdata"),
            ("POST", "/api/plugin/import"),
            ("GET", "/api/plugin/runs/summary"),
            ("GET", "/api/plugin/:id"),
            ("DELETE", "/api/plugin/:id"),
            ("GET", "/api/plugin/:id/export"),
            ("GET", "/api/plugin/:id/stats"),
            ("POST", "/api/plugin/:id/data"),
            ("POST", "/api/plugin/:id/dev-invoke"),
            ("POST", "/api/plugin/:id/invoke"),
        ];

        for (method, path) in expected {
            assert!(
                ROUTE_REGISTRY.iter().any(|policy| policy.path == path
                    && (policy.method == "*" || policy.method == method)),
                "missing route policy for {method} {path}"
            );
        }

        assert!(
            !ROUTE_REGISTRY
                .iter()
                .any(|policy| policy.path == "/api/plugin/:id/run"),
            "stale plugin run route policy should not exist"
        );
    }
}
