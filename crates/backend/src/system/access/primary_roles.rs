//! Built-in primary role catalog with default scope grants.
//!
//! Primary roles are defined in code, not in the DB (they are seeded with is_system=1
//! but their grants are the authoritative source here, not sys_role_scope_access).

/// Default scope grants for the `manager` role.
/// Managers get full access to all aggregates, projections, usecases, and system views.
pub const MANAGER_GRANTS: &[(&str, &str)] = &[
    // Aggregates
    ("a001_connection_1c", "all"),
    ("a002_organization", "all"),
    ("a003_counterparty", "all"),
    ("a004_nomenclature", "all"),
    ("a005_marketplace", "all"),
    ("a006_connection_mp", "all"),
    ("a007_marketplace_product", "all"),
    ("a008_marketplace_sales", "all"),
    ("a009_ozon_returns", "all"),
    ("a010_ozon_fbs_posting", "all"),
    ("a011_ozon_fbo_posting", "all"),
    ("a012_wb_sales", "all"),
    ("a013_ym_order", "all"),
    ("a014_ozon_transactions", "all"),
    ("a015_wb_orders", "all"),
    ("a016_ym_returns", "all"),
    ("a017_llm_agent", "all"),
    ("a038_llm_connection", "all"),
    ("a039_mail_message", "all"),
    ("a042_agent_task", "all"),
    ("a018_llm_chat", "all"),
    ("a019_llm_artifact", "all"),
    ("a020_wb_promotion", "all"),
    ("a021_production_output", "all"),
    ("a022_kit_variant", "all"),
    ("a023_purchase_of_goods", "all"),
    ("a024_bi_indicator", "all"),
    ("a025_bi_dashboard", "all"),
    ("a026_wb_advert_daily", "all"),
    ("a034_ym_realization", "all"),
    ("a036_wb_sales_funnel_daily", "all"),
    ("a037_wb_product_snapshot", "all"),
    ("a040_wb_search_analytics_daily", "all"),
    ("a027_wb_documents", "all"),
    ("a028_missing_cost_registry", "all"),
    ("a029_wb_supply", "all"),
    ("a030_wb_advert_campaign", "all"),
    ("a031_kb_edit", "all"),
    ("a032_wb_returns_claims", "all"),
    // Projections
    ("p900_mp_sales_register", "all"),
    ("p901_nomenclature_barcodes", "all"),
    ("p902_ozon_finance_realization", "all"),
    ("p903_wb_finance_report", "all"),
    ("p904_sales_data", "all"),
    ("p905_wb_commission_history", "all"),
    ("p906_nomenclature_prices", "all"),
    ("p907_ym_payment_report", "all"),
    ("p908_wb_goods_prices", "all"),
    ("p912_nomenclature_costs", "all"),
    // Usecases
    ("u501_import_from_ut", "all"),
    ("u502_import_from_ozon", "all"),
    ("u503_import_from_yandex", "all"),
    ("u504_import_from_wildberries", "all"),
    ("u505_match_nomenclature", "all"),
    ("u506_import_from_lemanapro", "all"),
    ("u507_import_from_erp", "all"),
    ("u508_repost_documents", "all"),
    // System views
    ("general_ledger", "all"),
    ("data_view", "all"),
    ("bi_timeline", "all"),
    ("dashboard", "all"),
    ("knowledge_base", "all"),
];

/// Default scope grants for the `operator` role.
/// Operators get read access to references, full access to marketplace operational data.
/// LLM chat is available; agent definitions are read-only because chat creation needs them.
/// No access to: production, other AI/LLM administration, imports, system views.
pub const OPERATOR_GRANTS: &[(&str, &str)] = &[
    // Aggregates — references: read only
    ("a001_connection_1c", "read"),
    ("a002_organization", "read"),
    ("a003_counterparty", "read"),
    ("a004_nomenclature", "read"),
    ("a005_marketplace", "read"),
    ("a006_connection_mp", "read"),
    ("a007_marketplace_product", "read"),
    // Aggregates — marketplace operational data: full access
    ("a008_marketplace_sales", "all"),
    ("a009_ozon_returns", "all"),
    ("a010_ozon_fbs_posting", "all"),
    ("a011_ozon_fbo_posting", "all"),
    ("a012_wb_sales", "all"),
    ("a013_ym_order", "all"),
    ("a014_ozon_transactions", "all"),
    ("a015_wb_orders", "all"),
    ("a016_ym_returns", "all"),
    ("a017_llm_agent", "read"),
    ("a038_llm_connection", "read"),
    ("a039_mail_message", "read"),
    ("a042_agent_task", "read"),
    ("a018_llm_chat", "all"),
    // Артефакты — прямой результат чата (SQL/отчёты, встроенные в сообщения); нужен
    // для загрузки карточек артефактов и работы со страницей артефакта.
    ("a019_llm_artifact", "all"),
    ("a020_wb_promotion", "all"),
    ("a024_bi_indicator", "read"),
    ("a025_bi_dashboard", "all"),
    ("a026_wb_advert_daily", "all"),
    ("a034_ym_realization", "all"),
    ("a036_wb_sales_funnel_daily", "all"),
    ("a037_wb_product_snapshot", "all"),
    ("a040_wb_search_analytics_daily", "all"),
    ("a027_wb_documents", "all"),
    ("a029_wb_supply", "all"),
    ("a030_wb_advert_campaign", "all"),
    ("a032_wb_returns_claims", "all"),
    ("a033_wb_day_close", "all"),
    // Projections — read access to analytics
    ("p900_mp_sales_register", "read"),
    ("p901_nomenclature_barcodes", "read"),
    ("p902_ozon_finance_realization", "read"),
    ("p903_wb_finance_report", "read"),
    ("p904_sales_data", "read"),
    ("p905_wb_commission_history", "all"),
    ("p906_nomenclature_prices", "all"),
    ("p907_ym_payment_report", "read"),
    ("p908_wb_goods_prices", "read"),
    ("p912_nomenclature_costs", "read"),
    ("bi_timeline", "read"),
];

/// Default scope grants for the `viewer` role.
/// Viewers get read-only access to marketplace analytics and core references.
/// No access to: production, AI/LLM, imports, system views.
pub const VIEWER_GRANTS: &[(&str, &str)] = &[
    // Aggregates — references and marketplace data: read only
    ("a002_organization", "read"),
    ("a004_nomenclature", "read"),
    ("a005_marketplace", "read"),
    ("a007_marketplace_product", "read"),
    ("a008_marketplace_sales", "read"),
    ("a012_wb_sales", "read"),
    ("a013_ym_order", "read"),
    ("a024_bi_indicator", "read"),
    ("a025_bi_dashboard", "read"),
    ("a026_wb_advert_daily", "read"),
    ("a034_ym_realization", "read"),
    ("a036_wb_sales_funnel_daily", "read"),
    ("a037_wb_product_snapshot", "read"),
    ("a040_wb_search_analytics_daily", "read"),
    ("a027_wb_documents", "read"),
    ("a030_wb_advert_campaign", "read"),
    ("bi_timeline", "read"),
    // Projections — key analytics views
    ("p900_mp_sales_register", "read"),
    ("p904_sales_data", "read"),
    ("p912_nomenclature_costs", "read"),
];

/// `admin` primary role: all access is granted via `is_admin=true` bypass.
/// No explicit grants needed.
pub const ADMIN_GRANTS: &[(&str, &str)] = &[];

/// Get built-in grants for a primary role code.
/// Returns empty slice for unknown roles (default deny).
pub fn grants_for_role(role_code: &str) -> &'static [(&'static str, &'static str)] {
    match role_code {
        "admin" => ADMIN_GRANTS,
        "manager" => MANAGER_GRANTS,
        "operator" => OPERATOR_GRANTS,
        "viewer" => VIEWER_GRANTS,
        _ => &[],
    }
}

#[cfg(test)]
mod tests {
    use super::OPERATOR_GRANTS;

    #[test]
    fn operator_can_use_llm_chat_without_administering_agents() {
        assert!(OPERATOR_GRANTS.contains(&("a018_llm_chat", "all")));
        assert!(OPERATOR_GRANTS.contains(&("a017_llm_agent", "read")));
        assert!(!OPERATOR_GRANTS.contains(&("a017_llm_agent", "all")));
        // Артефакты чата должны быть доступны, иначе карточки артефактов отдают 403.
        assert!(OPERATOR_GRANTS.contains(&("a019_llm_artifact", "all")));
    }
}
