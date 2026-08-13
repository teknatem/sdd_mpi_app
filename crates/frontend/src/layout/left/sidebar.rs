//! Sidebar component with collapsible menu items
//! Based on bolt-mpi-ui-redesign/src/components/Sidebar.tsx

use crate::layout::global_context::AppGlobalContext;
use crate::layout::tabs::tab_label_for_key;
use crate::shared::icons::icon;
use crate::system::auth::context::{has_read_access, use_auth};
use leptos::prelude::*;

/// A single sidebar navigation item.
#[derive(Clone, Debug, PartialEq)]
struct SidebarItem {
    id: &'static str,
    label: &'static str,
    icon: &'static str,
    /// Optional access scope. When set, the item is hidden unless the user
    /// has at least `read` access to this scope. `None` = always visible.
    scope_id: Option<&'static str>,
    /// Hide the item from non-admin users even when they have its scope as a
    /// dependency of another feature.
    admin_only: bool,
}

impl SidebarItem {
    fn new(id: &'static str, label: &'static str, icon: &'static str) -> Self {
        Self {
            id,
            label,
            icon,
            scope_id: None,
            admin_only: false,
        }
    }

    fn with_scope(id: &'static str, label: &'static str, icon: &'static str) -> Self {
        // For aggregates the scope_id equals the tab key (folder name).
        Self {
            id,
            label,
            icon,
            scope_id: Some(id),
            admin_only: false,
        }
    }

    fn admin_only(mut self) -> Self {
        self.admin_only = true;
        self
    }
}

#[derive(Clone, Debug, PartialEq)]
struct MenuGroup {
    id: &'static str,
    label: &'static str,
    icon: &'static str,
    items: Vec<SidebarItem>,
    admin_only: bool,
}

fn get_menu_groups() -> Vec<MenuGroup> {
    vec![
        MenuGroup {
            id: "navigator",
            label: "Навигация",
            icon: "compass",
            items: vec![SidebarItem::new(
                "navigator_marketplace",
                tab_label_for_key("navigator_marketplace"),
                "store",
            )],
            admin_only: false,
        },
        MenuGroup {
            id: "dashboards",
            label: "Дашборды",
            icon: "bar-chart",
            items: vec![
                SidebarItem::with_scope(
                    "a024_bi_indicator",
                    tab_label_for_key("a024_bi_indicator"),
                    "activity",
                ),
                SidebarItem::with_scope(
                    "a025_bi_dashboard",
                    tab_label_for_key("a025_bi_dashboard"),
                    "layout-dashboard",
                ),
                SidebarItem::with_scope(
                    "bi_timeline",
                    tab_label_for_key("bi_timeline"),
                    "line-chart",
                ),
                SidebarItem::new(
                    "d402_wb_order_flow",
                    tab_label_for_key("d402_wb_order_flow"),
                    "list-ordered",
                ),
                SidebarItem::new(
                    "d403_ym_order_flow",
                    tab_label_for_key("d403_ym_order_flow"),
                    "list-ordered",
                ),
                SidebarItem::new(
                    "d406_wb_sales_funnel",
                    tab_label_for_key("d406_wb_sales_funnel"),
                    "filter",
                ),
            ],
            admin_only: false,
        },
        MenuGroup {
            id: "knowledge_base",
            label: "База знаний",
            icon: "book-open",
            items: vec![
                SidebarItem::with_scope(
                    "knowledge_base",
                    tab_label_for_key("knowledge_base"),
                    "book-open-text",
                ),
                SidebarItem::with_scope(
                    "a031_kb_edit",
                    tab_label_for_key("a031_kb_edit"),
                    "book-open",
                ),
            ],
            admin_only: false,
        },
        MenuGroup {
            id: "references",
            label: "Справочники",
            icon: "database",
            items: vec![
                SidebarItem::with_scope(
                    "a002_organization",
                    tab_label_for_key("a002_organization"),
                    "building",
                ),
                SidebarItem::with_scope(
                    "a003_counterparty",
                    tab_label_for_key("a003_counterparty"),
                    "contact",
                ),
                SidebarItem::with_scope(
                    "a004_nomenclature",
                    tab_label_for_key("a004_nomenclature"),
                    "list",
                ),
                // a004_nomenclature_list is a view variant of a004, same scope.
                SidebarItem {
                    id: "a004_nomenclature_list",
                    label: tab_label_for_key("a004_nomenclature_list"),
                    icon: "table",
                    scope_id: Some("a004_nomenclature"),
                    admin_only: false,
                },
                SidebarItem::with_scope(
                    "a005_marketplace",
                    tab_label_for_key("a005_marketplace"),
                    "store",
                ),
                SidebarItem::with_scope(
                    "a007_marketplace_product",
                    tab_label_for_key("a007_marketplace_product"),
                    "package",
                ),
                SidebarItem::with_scope(
                    "a030_wb_advert_campaign",
                    tab_label_for_key("a030_wb_advert_campaign"),
                    "megaphone",
                ),
            ],
            admin_only: false,
        },
        MenuGroup {
            id: "documents",
            label: "Документы",
            icon: "file-text",
            items: vec![
                SidebarItem::with_scope(
                    "a015_wb_orders",
                    tab_label_for_key("a015_wb_orders"),
                    "file-text",
                ),
                SidebarItem::with_scope(
                    "a026_wb_advert_daily",
                    tab_label_for_key("a026_wb_advert_daily"),
                    "activity",
                ),
                SidebarItem::with_scope(
                    "a036_wb_sales_funnel_daily",
                    tab_label_for_key("a036_wb_sales_funnel_daily"),
                    "filter",
                ),
                SidebarItem::with_scope(
                    "a037_wb_product_snapshot",
                    tab_label_for_key("a037_wb_product_snapshot"),
                    "package",
                ),
                SidebarItem::with_scope(
                    "a040_wb_search_analytics_daily",
                    tab_label_for_key("a040_wb_search_analytics_daily"),
                    "search",
                ),
                SidebarItem::with_scope(
                    "a041_ym_shows_sales_daily",
                    tab_label_for_key("a041_ym_shows_sales_daily"),
                    "filter",
                ),
                SidebarItem::with_scope(
                    "a033_wb_day_close",
                    tab_label_for_key("a033_wb_day_close"),
                    "calendar-check",
                ),
                SidebarItem::with_scope(
                    "a027_wb_documents",
                    tab_label_for_key("a027_wb_documents"),
                    "file-text",
                ),
                SidebarItem::with_scope(
                    "a043_wb_finance_report",
                    tab_label_for_key("a043_wb_finance_report"),
                    "receipt",
                ),
                SidebarItem::with_scope(
                    "a021_production_output",
                    tab_label_for_key("a021_production_output"),
                    "package",
                ),
                SidebarItem::with_scope(
                    "a022_kit_variant",
                    tab_label_for_key("a022_kit_variant"),
                    "layers",
                ),
                SidebarItem::with_scope(
                    "a023_purchase_of_goods",
                    tab_label_for_key("a023_purchase_of_goods"),
                    "shopping-cart",
                ),
                SidebarItem::with_scope(
                    "a028_missing_cost_registry",
                    tab_label_for_key("a028_missing_cost_registry"),
                    "alert-circle",
                ),
                SidebarItem::with_scope(
                    "a029_wb_supply",
                    tab_label_for_key("a029_wb_supply"),
                    "package",
                ),
                SidebarItem::with_scope(
                    "a032_wb_returns_claims",
                    tab_label_for_key("a032_wb_returns_claims"),
                    "rotate-ccw",
                ),
                SidebarItem::with_scope(
                    "a020_wb_promotion",
                    tab_label_for_key("a020_wb_promotion"),
                    "tag",
                ),
                SidebarItem::with_scope(
                    "a013_ym_order",
                    tab_label_for_key("a013_ym_order"),
                    "file-text",
                ),
                SidebarItem::with_scope(
                    "a010_ozon_fbs_posting",
                    tab_label_for_key("a010_ozon_fbs_posting"),
                    "file-text",
                ),
                SidebarItem::with_scope(
                    "a011_ozon_fbo_posting",
                    tab_label_for_key("a011_ozon_fbo_posting"),
                    "file-text",
                ),
                SidebarItem::with_scope(
                    "a012_wb_sales",
                    tab_label_for_key("a012_wb_sales"),
                    "file-text",
                ),
                SidebarItem::with_scope(
                    "a009_ozon_returns",
                    tab_label_for_key("a009_ozon_returns"),
                    "package-x",
                ),
                SidebarItem::with_scope(
                    "a016_ym_returns",
                    tab_label_for_key("a016_ym_returns"),
                    "package-x",
                ),
                SidebarItem::with_scope(
                    "a008_marketplace_sales",
                    tab_label_for_key("a008_marketplace_sales"),
                    "cash",
                ),
                SidebarItem::with_scope(
                    "a014_ozon_transactions",
                    tab_label_for_key("a014_ozon_transactions"),
                    "credit-card",
                ),
            ],
            admin_only: false,
        },
        MenuGroup {
            id: "integrations",
            label: "Интеграции",
            icon: "plug",
            items: vec![
                SidebarItem::with_scope(
                    "a001_connection_1c",
                    tab_label_for_key("a001_connection_1c"),
                    "database",
                ),
                SidebarItem::with_scope(
                    "a006_connection_mp",
                    tab_label_for_key("a006_connection_mp"),
                    "plug",
                ),
                SidebarItem::new(
                    "u501_import_from_ut",
                    tab_label_for_key("u501_import_from_ut"),
                    "import",
                ),
                SidebarItem::new(
                    "u502_import_from_ozon",
                    tab_label_for_key("u502_import_from_ozon"),
                    "import",
                ),
                SidebarItem::new(
                    "u503_import_from_yandex",
                    tab_label_for_key("u503_import_from_yandex"),
                    "import",
                ),
                SidebarItem::new(
                    "u504_import_from_wildberries",
                    tab_label_for_key("u504_import_from_wildberries"),
                    "import",
                ),
                SidebarItem::new(
                    "u506_import_from_lemanapro",
                    tab_label_for_key("u506_import_from_lemanapro"),
                    "import",
                ),
                SidebarItem::new(
                    "u507_import_from_erp",
                    tab_label_for_key("u507_import_from_erp"),
                    "import",
                ),
                SidebarItem::new(
                    "u508_repost_documents",
                    tab_label_for_key("u508_repost_documents"),
                    "refresh-cw",
                ),
            ],
            admin_only: false,
        },
        MenuGroup {
            id: "operations",
            label: "Финансы",
            icon: "layers",
            items: vec![
                SidebarItem::new(
                    "general_ledger",
                    tab_label_for_key("general_ledger"),
                    "book-open",
                ),
                SidebarItem::new(
                    "general_ledger_turnovers",
                    tab_label_for_key("general_ledger_turnovers"),
                    "table",
                ),
                SidebarItem::new(
                    "general_ledger_dimensions",
                    tab_label_for_key("general_ledger_dimensions"),
                    "layers",
                ),
                SidebarItem::new(
                    "general_ledger_layers",
                    tab_label_for_key("general_ledger_layers"),
                    "database",
                ),
                SidebarItem::new(
                    "general_ledger_entities",
                    tab_label_for_key("general_ledger_entities"),
                    "database",
                ),
                SidebarItem::new(
                    "supplier_balance",
                    tab_label_for_key("supplier_balance"),
                    "dollar-sign",
                ),
                SidebarItem::new(
                    "general_ledger_matrix",
                    tab_label_for_key("general_ledger_matrix"),
                    "table",
                ),
                SidebarItem::new(
                    "u505_match_nomenclature",
                    tab_label_for_key("u505_match_nomenclature"),
                    "layers",
                ),
            ],
            admin_only: true,
        },
        MenuGroup {
            id: "llm",
            label: "Чаты LLM",
            icon: "message-square",
            items: vec![
                SidebarItem::with_scope(
                    "a018_llm_chat",
                    tab_label_for_key("a018_llm_chat"),
                    "message-square",
                ),
                SidebarItem::with_scope(
                    "a019_llm_artifact",
                    tab_label_for_key("a019_llm_artifact"),
                    "file-text",
                )
                .admin_only(),
                SidebarItem::with_scope(
                    "a017_llm_agent",
                    tab_label_for_key("a017_llm_agent"),
                    "robot",
                )
                .admin_only(),
                SidebarItem::with_scope(
                    "a038_llm_connection",
                    tab_label_for_key("a038_llm_connection"),
                    "plug-connected",
                )
                .admin_only(),
                SidebarItem::with_scope(
                    "a039_mail_message",
                    tab_label_for_key("a039_mail_message"),
                    "mail",
                )
                .admin_only(),
                SidebarItem::with_scope(
                    "a042_agent_task",
                    tab_label_for_key("a042_agent_task"),
                    "share-2",
                )
                .admin_only(),
                SidebarItem::new("llm_skills", tab_label_for_key("llm_skills"), "list")
                    .admin_only(),
                SidebarItem::new("llm_tools", tab_label_for_key("llm_tools"), "wrench")
                    .admin_only(),
                SidebarItem::new(
                    "d407_llm_quality",
                    tab_label_for_key("d407_llm_quality"),
                    "gauge",
                )
                .admin_only(),
            ],
            admin_only: false,
        },
        MenuGroup {
            id: "reports",
            label: "Отчеты",
            icon: "table",
            items: vec![
                SidebarItem::new(
                    "general_ledger_report",
                    tab_label_for_key("general_ledger_report"),
                    "file-text",
                ),
                SidebarItem::new(
                    "gl_account_view__7609",
                    tab_label_for_key("gl_account_view__7609"),
                    "trending-up",
                ),
                SidebarItem::new(
                    "wb_weekly_reconciliation",
                    tab_label_for_key("wb_weekly_reconciliation"),
                    "table",
                ),
                SidebarItem::new(
                    "ym_revenue_reconciliation",
                    tab_label_for_key("ym_revenue_reconciliation"),
                    "table",
                ),
                SidebarItem {
                    id: "report_a026_wb_advert_daily",
                    label: tab_label_for_key("report_a026_wb_advert_daily"),
                    icon: "download",
                    scope_id: Some("a026_wb_advert_daily"),
                    admin_only: false,
                },
            ],
            admin_only: true,
        },
        MenuGroup {
            id: "information",
            label: "Информация",
            icon: "database",
            items: vec![
                SidebarItem::new(
                    "p900_sales_register",
                    tab_label_for_key("p900_sales_register"),
                    "database",
                ),
                SidebarItem::new(
                    "p901_barcodes",
                    tab_label_for_key("p901_barcodes"),
                    "barcode",
                ),
                SidebarItem::new(
                    "p902_ozon_finance_realization",
                    tab_label_for_key("p902_ozon_finance_realization"),
                    "dollar-sign",
                ),
                SidebarItem::new(
                    "p903_wb_finance_report",
                    tab_label_for_key("p903_wb_finance_report"),
                    "dollar-sign",
                ),
                SidebarItem::new(
                    "p904_sales_data",
                    tab_label_for_key("p904_sales_data"),
                    "dollar-sign",
                ),
                SidebarItem::new(
                    "p905_commission_history",
                    tab_label_for_key("p905_commission_history"),
                    "percent",
                ),
                SidebarItem::new(
                    "p906_nomenclature_prices",
                    tab_label_for_key("p906_nomenclature_prices"),
                    "dollar-sign",
                ),
                SidebarItem::new(
                    "p907_ym_payment_report",
                    tab_label_for_key("p907_ym_payment_report"),
                    "receipt",
                ),
                SidebarItem::new(
                    "a034_ym_realization",
                    tab_label_for_key("a034_ym_realization"),
                    "receipt",
                ),
                SidebarItem::new(
                    "a035_ym_settlement_recon",
                    tab_label_for_key("a035_ym_settlement_recon"),
                    "receipt",
                ),
                SidebarItem::new(
                    "p908_wb_goods_prices",
                    tab_label_for_key("p908_wb_goods_prices"),
                    "tag",
                ),
                SidebarItem::new(
                    "p913_wb_advert_order_attr",
                    tab_label_for_key("p913_wb_advert_order_attr"),
                    "trending-up",
                ),
                SidebarItem::new(
                    "p914_mp_finance_turnovers",
                    tab_label_for_key("p914_mp_finance_turnovers"),
                    "layers",
                ),
                SidebarItem::new(
                    "a032_wb_returns_claims",
                    tab_label_for_key("a032_wb_returns_claims"),
                    "file-text",
                ),
            ],
            admin_only: false,
        },
        MenuGroup {
            id: "support",
            label: "Техподдержка",
            icon: "message-circle",
            items: vec![SidebarItem::new(
                "sys_tickets",
                tab_label_for_key("sys_tickets"),
                "message-circle",
            )],
            admin_only: false,
        },
        MenuGroup {
            id: "settings",
            label: "Настройки",
            icon: "settings",
            items: vec![
                SidebarItem::new("data_view", tab_label_for_key("data_view"), "layers"),
                SidebarItem::new(
                    "universal_dashboard",
                    tab_label_for_key("universal_dashboard"),
                    "table-pivot",
                ),
                SidebarItem::new(
                    "schema_browser",
                    tab_label_for_key("schema_browser"),
                    "database-cog",
                ),
                SidebarItem::new(
                    "drilldown__new",
                    tab_label_for_key("drilldown__new"),
                    "zoom-in",
                ),
                SidebarItem::new("all_reports", tab_label_for_key("all_reports"), "table"),
                SidebarItem::new(
                    "filter_registry",
                    tab_label_for_key("filter_registry"),
                    "filter",
                ),
                SidebarItem::new(
                    "d400_monthly_summary",
                    tab_label_for_key("d400_monthly_summary"),
                    "bar-chart",
                ),
                SidebarItem::new(
                    "d405_metadata_dashboard",
                    tab_label_for_key("d405_metadata_dashboard"),
                    "layout-dashboard",
                ),
            ],
            admin_only: true,
        },
        MenuGroup {
            id: "administration",
            label: "Система",
            icon: "shield",
            items: vec![
                SidebarItem::new("sys_users", tab_label_for_key("sys_users"), "users"),
                SidebarItem::new("sys_roles", tab_label_for_key("sys_roles"), "shield"),
                SidebarItem::new(
                    "sys_roles_matrix",
                    tab_label_for_key("sys_roles_matrix"),
                    "table",
                ),
                SidebarItem::new("sys_audit", tab_label_for_key("sys_audit"), "shield-check"),
                SidebarItem::new(
                    "sys_s3_files",
                    tab_label_for_key("sys_s3_files"),
                    "download-cloud",
                ),
                SidebarItem::new("sys_datasets", tab_label_for_key("sys_datasets"), "package"),
                SidebarItem::new(
                    "sys_raw_storage",
                    tab_label_for_key("sys_raw_storage"),
                    "database",
                ),
                SidebarItem::new(
                    "quality_checks",
                    tab_label_for_key("quality_checks"),
                    "check-circle",
                ),
                SidebarItem::new("sys_tasks", tab_label_for_key("sys_tasks"), "calendar"),
                SidebarItem::new(
                    "sys_task_type_registry",
                    tab_label_for_key("sys_task_type_registry"),
                    "layers",
                ),
                SidebarItem::new(
                    "sys_thaw_test",
                    tab_label_for_key("sys_thaw_test"),
                    "test-tube",
                ),
                SidebarItem::new(
                    "sys_style_guide",
                    tab_label_for_key("sys_style_guide"),
                    "palette",
                ),
            ],
            admin_only: true,
        },
    ]
}

#[component]
pub fn Sidebar() -> impl IntoView {
    let ctx = use_context::<AppGlobalContext>().expect("AppGlobalContext not found");
    let (auth_state, _) = use_auth();

    // Check admin status once, untracked, for filtering menu groups.
    // Scopes are also read untracked, they are stable for the session lifetime.
    let is_admin_untracked = auth_state.with_untracked(|state| {
        state
            .user_info
            .as_ref()
            .map(|u| u.is_admin)
            .unwrap_or(false)
    });

    // Аккордеон: одновременно раскрыта максимум одна группа (включая динамическую
    // группу плагинов, которая работает с этим же сигналом).
    let expanded_group = RwSignal::new(None::<String>);
    let groups = get_menu_groups();

    view! {
        <div class="app-sidebar__content">
            {groups
                .into_iter()
                .filter_map(|group| {
                    let is_admin_only = group.admin_only;

                    if is_admin_only && !is_admin_untracked {
                        return None;
                    }

                    let visible_items: Vec<SidebarItem> = group
                        .items
                        .into_iter()
                        .filter(|item| {
                            if item.admin_only && !is_admin_untracked {
                                return false;
                            }
                            match item.scope_id {
                                None => true,
                                Some(scope) => has_read_access(auth_state, scope),
                            }
                        })
                        .collect();

                    // Keep the user-facing settings group visible even if it becomes empty.
                    if visible_items.is_empty() && group.id != "settings" {
                        return None;
                    }

                    let group_id = group.id.to_string();
                    let has_children = !visible_items.is_empty();

                    let group_id_stored = StoredValue::new(group_id.clone());
                    let group_id_for_exp = group_id.clone();
                    let group_id_for_click = group_id.clone();

                    Some(view! {
                        <div>
                            <div
                                class="app-sidebar__item"
                                class:app-sidebar__item--active=move || {
                                    let gid = group_id_stored.get_value();
                                    !has_children
                                        && ctx.active.get().as_ref().map(|a| a == &gid).unwrap_or(false)
                                }
                                style:padding-left="12px"
                                on:click=move |_| {
                                    if has_children {
                                        let gid = group_id_for_click.clone();
                                        expanded_group.update(move |current| {
                                            if current.as_deref() == Some(gid.as_str()) {
                                                *current = None;
                                            } else {
                                                *current = Some(gid);
                                            }
                                        });
                                    } else {
                                        ctx.open_tab(group.id, group.label);
                                    }
                                }
                            >
                                <div class="app-sidebar__item-content">
                                    {icon(group.icon)}
                                    <span>{group.label}</span>
                                </div>
                                {has_children.then(|| {
                                    let gid_exp = group_id_for_exp.clone();
                                    view! {
                                        <div
                                            class="app-sidebar__chevron"
                                            class:app-sidebar__chevron--expanded=move || {
                                                expanded_group.with(|g| g.as_deref() == Some(gid_exp.as_str()))
                                            }
                                        >
                                            {icon("chevron-right")}
                                        </div>
                                    }
                                })}
                            </div>

                            {has_children.then(|| {
                                let gid_show = group_id.clone();
                                // Подменю всегда в DOM: раскрытие/схлопывание анимируется
                                // через grid-template-rows, иначе соседние группы дёргаются.
                                view! {
                                    <div
                                        class="app-sidebar__collapse"
                                        class:app-sidebar__collapse--open=move || {
                                            expanded_group.with(|g| g.as_deref() == Some(gid_show.as_str()))
                                        }
                                    >
                                        <div class="app-sidebar__collapse-inner">
                                        <div class="app-sidebar__children">
                                            {visible_items
                                                .into_iter()
                                                .map(|item| {
                                                    let item_id = StoredValue::new(item.id.to_string());
                                                    view! {
                                                        <div
                                                            class="app-sidebar__item"
                                                            class:app-sidebar__item--active=move || {
                                                                let iid = item_id.get_value();
                                                                ctx.active.get().as_ref().map(|a| a == &iid).unwrap_or(false)
                                                            }
                                                            style:padding-left="10px"
                                                            on:click=move |_| {
                                                                ctx.open_tab(item.id, item.label);
                                                            }
                                                        >
                                                            <div class="app-sidebar__item-content">
                                                                {icon(item.icon)}
                                                                <span>{item.label}</span>
                                                            </div>
                                                        </div>
                                                    }
                                                })
                                                .collect_view()}
                                        </div>
                                        </div>
                                    </div>
                                }
                            })}
                        </div>
                    })
                })
                .collect_view()}

            // Плагины — динамическая группа (использование доступно всем; управление — админам)
            <crate::plugins::PluginsSidebarGroup expanded_group=expanded_group />
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wb_finance_report_is_available_in_documents_group() {
        let documents = get_menu_groups()
            .into_iter()
            .find(|group| group.id == "documents")
            .expect("documents sidebar group must exist");

        let item = documents
            .items
            .iter()
            .find(|item| item.id == "a043_wb_finance_report")
            .expect("a043 sidebar item must exist");

        assert_eq!(item.scope_id, Some("a043_wb_finance_report"));
    }
}
