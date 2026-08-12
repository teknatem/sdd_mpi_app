use crate::layout::global_context::AppGlobalContext;
use crate::layout::tabs::tab_label_for_key;
use crate::shared::icons;
use crate::system::auth::context::is_admin;
use leptos::prelude::*;

#[derive(Debug, Clone)]
pub struct MegaMenuItem {
    pub key: &'static str,
    pub title: &'static str,
    pub icon_name: &'static str,
}

#[component]
pub fn MegaMenuCategory(
    label: &'static str,
    items: Vec<MegaMenuItem>,
    #[prop(default = 2)] columns: usize,
) -> impl IntoView {
    let (is_open, set_is_open) = signal(false);
    let tabs_store = leptos::context::use_context::<AppGlobalContext>()
        .expect("AppGlobalContext context not found");

    let grid_cols_class = match columns {
        1 => "mega-menu-grid-1",
        2 => "mega-menu-grid-2",
        3 => "mega-menu-grid-3",
        _ => "mega-menu-grid-2",
    };

    view! {
        <div
            class="mega-menu-category"
            on:mouseenter=move |_| set_is_open.set(true)
            on:mouseleave=move |_| set_is_open.set(false)
        >
            <button
                class="mega-menu-btn"
                class:mega-menu-btn-active=move || is_open.get()
            >
                <span>{label}</span>
                <span
                    class="mega-menu-chevron"
                    class:mega-menu-chevron-open=move || is_open.get()
                >
                    {icons::icon("chevron-down")}
                </span>
            </button>

            <div
                class="mega-menu-panel"
                class:mega-menu-panel-open=move || is_open.get()
            >
                <div class=format!("mega-menu-content {}", grid_cols_class)>
                    {items.into_iter().map(|item| {
                        let key = item.key;
                        let title = item.title;
                        let icon_name = item.icon_name;

                        view! {
                            <button
                                class="mega-menu-card"
                                on:click=move |_| {
                                    tabs_store.open_tab(key, title);
                                    set_is_open.set(false);
                                }
                            >
                                <div class="mega-menu-card-icon">
                                    {icons::icon(icon_name)}
                                </div>
                                <div class="mega-menu-card-title">
                                    {title}
                                </div>
                            </button>
                        }
                    }).collect_view()}
                </div>
            </div>
        </div>
    }
}

#[component]
pub fn MegaMenuBar() -> impl IntoView {
    // Справочники
    let directories = vec![
        MegaMenuItem {
            key: "a002_organization",
            title: tab_label_for_key("a002_organization"),
            icon_name: "building",
        },
        MegaMenuItem {
            key: "a003_counterparty",
            title: tab_label_for_key("a003_counterparty"),
            icon_name: "contact",
        },
        MegaMenuItem {
            key: "a004_nomenclature",
            title: tab_label_for_key("a004_nomenclature"),
            icon_name: "list",
        },
        MegaMenuItem {
            key: "a004_nomenclature_list",
            title: tab_label_for_key("a004_nomenclature_list"),
            icon_name: "table",
        },
        MegaMenuItem {
            key: "a005_marketplace",
            title: tab_label_for_key("a005_marketplace"),
            icon_name: "store",
        },
        MegaMenuItem {
            key: "a007_marketplace_product",
            title: tab_label_for_key("a007_marketplace_product"),
            icon_name: "package",
        },
    ];

    // Документы
    let documents = vec![
        MegaMenuItem {
            key: "a008_marketplace_sales",
            title: tab_label_for_key("a008_marketplace_sales"),
            icon_name: "dollar-sign",
        },
        MegaMenuItem {
            key: "a010_ozon_fbs_posting",
            title: tab_label_for_key("a010_ozon_fbs_posting"),
            icon_name: "file-text",
        },
        MegaMenuItem {
            key: "a011_ozon_fbo_posting",
            title: tab_label_for_key("a011_ozon_fbo_posting"),
            icon_name: "file-text",
        },
        MegaMenuItem {
            key: "a015_wb_orders",
            title: tab_label_for_key("a015_wb_orders"),
            icon_name: "file-text",
        },
        MegaMenuItem {
            key: "a026_wb_advert_daily",
            title: tab_label_for_key("a026_wb_advert_daily"),
            icon_name: "activity",
        },
        MegaMenuItem {
            key: "a036_wb_sales_funnel_daily",
            title: tab_label_for_key("a036_wb_sales_funnel_daily"),
            icon_name: "filter",
        },
        MegaMenuItem {
            key: "a037_wb_product_snapshot",
            title: tab_label_for_key("a037_wb_product_snapshot"),
            icon_name: "package",
        },
        MegaMenuItem {
            key: "a040_wb_search_analytics_daily",
            title: tab_label_for_key("a040_wb_search_analytics_daily"),
            icon_name: "search",
        },
        MegaMenuItem {
            key: "a043_wb_finance_report",
            title: tab_label_for_key("a043_wb_finance_report"),
            icon_name: "receipt",
        },
        MegaMenuItem {
            key: "a027_wb_documents",
            title: tab_label_for_key("a027_wb_documents"),
            icon_name: "file-text",
        },
        MegaMenuItem {
            key: "a021_production_output",
            title: tab_label_for_key("a021_production_output"),
            icon_name: "package",
        },
        MegaMenuItem {
            key: "a022_kit_variant",
            title: tab_label_for_key("a022_kit_variant"),
            icon_name: "layers",
        },
        MegaMenuItem {
            key: "a023_purchase_of_goods",
            title: tab_label_for_key("a023_purchase_of_goods"),
            icon_name: "shopping-cart",
        },
        MegaMenuItem {
            key: "a028_missing_cost_registry",
            title: tab_label_for_key("a028_missing_cost_registry"),
            icon_name: "alert-circle",
        },
        MegaMenuItem {
            key: "a029_wb_supply",
            title: tab_label_for_key("a029_wb_supply"),
            icon_name: "package",
        },
        MegaMenuItem {
            key: "a012_wb_sales",
            title: tab_label_for_key("a012_wb_sales"),
            icon_name: "file-text",
        },
        MegaMenuItem {
            key: "a013_ym_order",
            title: tab_label_for_key("a013_ym_order"),
            icon_name: "file-text",
        },
        MegaMenuItem {
            key: "a009_ozon_returns",
            title: tab_label_for_key("a009_ozon_returns"),
            icon_name: "package-x",
        },
        MegaMenuItem {
            key: "a016_ym_returns",
            title: tab_label_for_key("a016_ym_returns"),
            icon_name: "package-x",
        },
        MegaMenuItem {
            key: "a014_ozon_transactions",
            title: tab_label_for_key("a014_ozon_transactions"),
            icon_name: "credit-card",
        },
    ];

    // Интеграции
    let integrations = vec![
        MegaMenuItem {
            key: "a001_connection_1c",
            title: tab_label_for_key("a001_connection_1c"),
            icon_name: "database",
        },
        MegaMenuItem {
            key: "a006_connection_mp",
            title: tab_label_for_key("a006_connection_mp"),
            icon_name: "plug",
        },
        MegaMenuItem {
            key: "u501_import_from_ut",
            title: tab_label_for_key("u501_import_from_ut"),
            icon_name: "import",
        },
        MegaMenuItem {
            key: "u502_import_from_ozon",
            title: tab_label_for_key("u502_import_from_ozon"),
            icon_name: "import",
        },
        MegaMenuItem {
            key: "u503_import_from_yandex",
            title: tab_label_for_key("u503_import_from_yandex"),
            icon_name: "import",
        },
        MegaMenuItem {
            key: "u504_import_from_wildberries",
            title: tab_label_for_key("u504_import_from_wildberries"),
            icon_name: "import",
        },
        MegaMenuItem {
            key: "u506_import_from_lemanapro",
            title: tab_label_for_key("u506_import_from_lemanapro"),
            icon_name: "import",
        },
        MegaMenuItem {
            key: "u507_import_from_erp",
            title: tab_label_for_key("u507_import_from_erp"),
            icon_name: "import",
        },
        MegaMenuItem {
            key: "u508_repost_documents",
            title: tab_label_for_key("u508_repost_documents"),
            icon_name: "refresh-cw",
        },
    ];

    // Операции
    let operations = vec![MegaMenuItem {
        key: "u505_match_nomenclature",
        title: tab_label_for_key("u505_match_nomenclature"),
        icon_name: "layers",
    }];

    // Регистры
    let registers = vec![
        MegaMenuItem {
            key: "p900_sales_register",
            title: tab_label_for_key("p900_sales_register"),
            icon_name: "database",
        },
        MegaMenuItem {
            key: "p901_barcodes",
            title: tab_label_for_key("p901_barcodes"),
            icon_name: "barcode",
        },
        MegaMenuItem {
            key: "p902_ozon_finance_realization",
            title: tab_label_for_key("p902_ozon_finance_realization"),
            icon_name: "dollar-sign",
        },
        MegaMenuItem {
            key: "p903_wb_finance_report",
            title: tab_label_for_key("p903_wb_finance_report"),
            icon_name: "dollar-sign",
        },
        MegaMenuItem {
            key: "p904_sales_data",
            title: tab_label_for_key("p904_sales_data"),
            icon_name: "dollar-sign",
        },
        MegaMenuItem {
            key: "p905_commission_history",
            title: tab_label_for_key("p905_commission_history"),
            icon_name: "percent",
        },
    ];

    let user_is_admin = is_admin();

    view! {
        <nav class="mega-menu-bar">
            <MegaMenuCategory label="Справочники" items=directories columns=2 />
            <MegaMenuCategory label="Документы" items=documents columns=2 />
            <MegaMenuCategory label="Интеграции" items=integrations columns=2 />
            {user_is_admin.then(|| view! {
                <MegaMenuCategory label="Операции" items=operations columns=1 />
            })}
            <MegaMenuCategory label="Регистры" items=registers columns=2 />
            // Плагины доступны всем аутентифицированным пользователям (использование);
            // управление внутри — только админам.
            <crate::plugins::PluginsMenuCategory />
        </nav>
    }
}
