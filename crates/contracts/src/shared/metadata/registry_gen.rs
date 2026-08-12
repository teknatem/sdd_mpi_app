// ============================================================================
// AUTO-GENERATED FROM metadata.json FILES - DO NOT EDIT MANUALLY
// ============================================================================
//
// Every aggregate/projection that has a metadata.json lands here automatically,
// so consumers (LLM metadata registry, dashboards) never miss a new entity.
// Filter with `EntityRegistration::meta.ai.llm_visible` / `.ai.tags`.

#![allow(dead_code)]

use super::{EntityMetadataInfo, FieldMetadata};

/// One entity exposed through [`ALL_ENTITIES`].
pub struct EntityRegistration {
    pub meta: &'static EntityMetadataInfo,
    pub fields: &'static [FieldMetadata],
}

/// All entities generated from metadata.json, ordered by module path.
pub const ALL_ENTITIES: &[EntityRegistration] = &[
    // a001
    EntityRegistration {
        meta: &crate::domain::a001_connection_1c::ENTITY_METADATA,
        fields: crate::domain::a001_connection_1c::FIELDS,
    },
    // a002
    EntityRegistration {
        meta: &crate::domain::a002_organization::ENTITY_METADATA,
        fields: crate::domain::a002_organization::FIELDS,
    },
    // a003
    EntityRegistration {
        meta: &crate::domain::a003_counterparty::ENTITY_METADATA,
        fields: crate::domain::a003_counterparty::FIELDS,
    },
    // a004
    EntityRegistration {
        meta: &crate::domain::a004_nomenclature::ENTITY_METADATA,
        fields: crate::domain::a004_nomenclature::FIELDS,
    },
    // a005
    EntityRegistration {
        meta: &crate::domain::a005_marketplace::ENTITY_METADATA,
        fields: crate::domain::a005_marketplace::FIELDS,
    },
    // a006
    EntityRegistration {
        meta: &crate::domain::a006_connection_mp::ENTITY_METADATA,
        fields: crate::domain::a006_connection_mp::FIELDS,
    },
    // a007
    EntityRegistration {
        meta: &crate::domain::a007_marketplace_product::ENTITY_METADATA,
        fields: crate::domain::a007_marketplace_product::FIELDS,
    },
    // a008
    EntityRegistration {
        meta: &crate::domain::a008_marketplace_sales::ENTITY_METADATA,
        fields: crate::domain::a008_marketplace_sales::FIELDS,
    },
    // a009
    EntityRegistration {
        meta: &crate::domain::a009_ozon_returns::ENTITY_METADATA,
        fields: crate::domain::a009_ozon_returns::FIELDS,
    },
    // a010
    EntityRegistration {
        meta: &crate::domain::a010_ozon_fbs_posting::ENTITY_METADATA,
        fields: crate::domain::a010_ozon_fbs_posting::FIELDS,
    },
    // a011
    EntityRegistration {
        meta: &crate::domain::a011_ozon_fbo_posting::ENTITY_METADATA,
        fields: crate::domain::a011_ozon_fbo_posting::FIELDS,
    },
    // a012
    EntityRegistration {
        meta: &crate::domain::a012_wb_sales::ENTITY_METADATA,
        fields: crate::domain::a012_wb_sales::FIELDS,
    },
    // a013
    EntityRegistration {
        meta: &crate::domain::a013_ym_order::ENTITY_METADATA,
        fields: crate::domain::a013_ym_order::FIELDS,
    },
    // a014
    EntityRegistration {
        meta: &crate::domain::a014_ozon_transactions::ENTITY_METADATA,
        fields: crate::domain::a014_ozon_transactions::FIELDS,
    },
    // a015
    EntityRegistration {
        meta: &crate::domain::a015_wb_orders::ENTITY_METADATA,
        fields: crate::domain::a015_wb_orders::FIELDS,
    },
    // a016
    EntityRegistration {
        meta: &crate::domain::a016_ym_returns::ENTITY_METADATA,
        fields: crate::domain::a016_ym_returns::FIELDS,
    },
    // a017
    EntityRegistration {
        meta: &crate::domain::a017_llm_agent::ENTITY_METADATA,
        fields: crate::domain::a017_llm_agent::FIELDS,
    },
    // a018
    EntityRegistration {
        meta: &crate::domain::a018_llm_chat::ENTITY_METADATA,
        fields: crate::domain::a018_llm_chat::FIELDS,
    },
    // a019
    EntityRegistration {
        meta: &crate::domain::a019_llm_artifact::ENTITY_METADATA,
        fields: crate::domain::a019_llm_artifact::FIELDS,
    },
    // a020
    EntityRegistration {
        meta: &crate::domain::a020_wb_promotion::ENTITY_METADATA,
        fields: crate::domain::a020_wb_promotion::FIELDS,
    },
    // a021
    EntityRegistration {
        meta: &crate::domain::a021_production_output::ENTITY_METADATA,
        fields: crate::domain::a021_production_output::FIELDS,
    },
    // a022
    EntityRegistration {
        meta: &crate::domain::a022_kit_variant::ENTITY_METADATA,
        fields: crate::domain::a022_kit_variant::FIELDS,
    },
    // a023
    EntityRegistration {
        meta: &crate::domain::a023_purchase_of_goods::ENTITY_METADATA,
        fields: crate::domain::a023_purchase_of_goods::FIELDS,
    },
    // a024
    EntityRegistration {
        meta: &crate::domain::a024_bi_indicator::ENTITY_METADATA,
        fields: crate::domain::a024_bi_indicator::FIELDS,
    },
    // a025
    EntityRegistration {
        meta: &crate::domain::a025_bi_dashboard::ENTITY_METADATA,
        fields: crate::domain::a025_bi_dashboard::FIELDS,
    },
    // a026
    EntityRegistration {
        meta: &crate::domain::a026_wb_advert_daily::ENTITY_METADATA,
        fields: crate::domain::a026_wb_advert_daily::FIELDS,
    },
    // a027
    EntityRegistration {
        meta: &crate::domain::a027_wb_documents::ENTITY_METADATA,
        fields: crate::domain::a027_wb_documents::FIELDS,
    },
    // a028
    EntityRegistration {
        meta: &crate::domain::a028_missing_cost_registry::ENTITY_METADATA,
        fields: crate::domain::a028_missing_cost_registry::FIELDS,
    },
    // a029
    EntityRegistration {
        meta: &crate::domain::a029_wb_supply::ENTITY_METADATA,
        fields: crate::domain::a029_wb_supply::FIELDS,
    },
    // a030
    EntityRegistration {
        meta: &crate::domain::a030_wb_advert_campaign::ENTITY_METADATA,
        fields: crate::domain::a030_wb_advert_campaign::FIELDS,
    },
    // a031
    EntityRegistration {
        meta: &crate::domain::a031_kb_edit::ENTITY_METADATA,
        fields: crate::domain::a031_kb_edit::FIELDS,
    },
    // a032
    EntityRegistration {
        meta: &crate::domain::a032_wb_returns_claims::ENTITY_METADATA,
        fields: crate::domain::a032_wb_returns_claims::FIELDS,
    },
    // a033
    EntityRegistration {
        meta: &crate::domain::a033_wb_day_close::ENTITY_METADATA,
        fields: crate::domain::a033_wb_day_close::FIELDS,
    },
    // a034
    EntityRegistration {
        meta: &crate::domain::a034_ym_realization::ENTITY_METADATA,
        fields: crate::domain::a034_ym_realization::FIELDS,
    },
    // a035
    EntityRegistration {
        meta: &crate::domain::a035_ym_settlement_recon::ENTITY_METADATA,
        fields: crate::domain::a035_ym_settlement_recon::FIELDS,
    },
    // a036
    EntityRegistration {
        meta: &crate::domain::a036_wb_sales_funnel_daily::ENTITY_METADATA,
        fields: crate::domain::a036_wb_sales_funnel_daily::FIELDS,
    },
    // a037
    EntityRegistration {
        meta: &crate::domain::a037_wb_product_snapshot::ENTITY_METADATA,
        fields: crate::domain::a037_wb_product_snapshot::FIELDS,
    },
    // a038
    EntityRegistration {
        meta: &crate::domain::a038_llm_connection::ENTITY_METADATA,
        fields: crate::domain::a038_llm_connection::FIELDS,
    },
    // a039
    EntityRegistration {
        meta: &crate::domain::a039_mail_message::ENTITY_METADATA,
        fields: crate::domain::a039_mail_message::FIELDS,
    },
    // a040
    EntityRegistration {
        meta: &crate::domain::a040_wb_search_analytics_daily::ENTITY_METADATA,
        fields: crate::domain::a040_wb_search_analytics_daily::FIELDS,
    },
    // a041
    EntityRegistration {
        meta: &crate::domain::a041_ym_shows_sales_daily::ENTITY_METADATA,
        fields: crate::domain::a041_ym_shows_sales_daily::FIELDS,
    },
    // a042
    EntityRegistration {
        meta: &crate::domain::a042_agent_task::ENTITY_METADATA,
        fields: crate::domain::a042_agent_task::FIELDS,
    },
    // a043
    EntityRegistration {
        meta: &crate::domain::a043_wb_finance_report::ENTITY_METADATA,
        fields: crate::domain::a043_wb_finance_report::FIELDS,
    },
    // p900
    EntityRegistration {
        meta: &crate::projections::p900_mp_sales_register::ENTITY_METADATA,
        fields: crate::projections::p900_mp_sales_register::FIELDS,
    },
    // p901
    EntityRegistration {
        meta: &crate::projections::p901_nomenclature_barcodes::ENTITY_METADATA,
        fields: crate::projections::p901_nomenclature_barcodes::FIELDS,
    },
    // p902
    EntityRegistration {
        meta: &crate::projections::p902_ozon_finance_realization::ENTITY_METADATA,
        fields: crate::projections::p902_ozon_finance_realization::FIELDS,
    },
    // p903
    EntityRegistration {
        meta: &crate::projections::p903_wb_finance_report::ENTITY_METADATA,
        fields: crate::projections::p903_wb_finance_report::FIELDS,
    },
    // p904
    EntityRegistration {
        meta: &crate::projections::p904_sales_data::ENTITY_METADATA,
        fields: crate::projections::p904_sales_data::FIELDS,
    },
    // p905
    EntityRegistration {
        meta: &crate::projections::p905_wb_commission_history::ENTITY_METADATA,
        fields: crate::projections::p905_wb_commission_history::FIELDS,
    },
    // p906
    EntityRegistration {
        meta: &crate::projections::p906_nomenclature_prices::ENTITY_METADATA,
        fields: crate::projections::p906_nomenclature_prices::FIELDS,
    },
    // p907
    EntityRegistration {
        meta: &crate::projections::p907_ym_payment_report::ENTITY_METADATA,
        fields: crate::projections::p907_ym_payment_report::FIELDS,
    },
    // p908
    EntityRegistration {
        meta: &crate::projections::p908_wb_goods_prices::ENTITY_METADATA,
        fields: crate::projections::p908_wb_goods_prices::FIELDS,
    },
    // p909
    EntityRegistration {
        meta: &crate::projections::p909_mp_order_line_turnovers::ENTITY_METADATA,
        fields: crate::projections::p909_mp_order_line_turnovers::FIELDS,
    },
    // p910
    EntityRegistration {
        meta: &crate::projections::p910_mp_unlinked_turnovers::ENTITY_METADATA,
        fields: crate::projections::p910_mp_unlinked_turnovers::FIELDS,
    },
    // p911
    EntityRegistration {
        meta: &crate::projections::p911_wb_advert_by_items::ENTITY_METADATA,
        fields: crate::projections::p911_wb_advert_by_items::FIELDS,
    },
    // p912
    EntityRegistration {
        meta: &crate::projections::p912_nomenclature_costs::ENTITY_METADATA,
        fields: crate::projections::p912_nomenclature_costs::FIELDS,
    },
    // p913
    EntityRegistration {
        meta: &crate::projections::p913_wb_advert_order_attr::ENTITY_METADATA,
        fields: crate::projections::p913_wb_advert_order_attr::FIELDS,
    },
    // p914
    EntityRegistration {
        meta: &crate::projections::p914_mp_finance_turnovers::ENTITY_METADATA,
        fields: crate::projections::p914_mp_finance_turnovers::FIELDS,
    },
    // p915
    EntityRegistration {
        meta: &crate::projections::p915_mp_order_events::ENTITY_METADATA,
        fields: crate::projections::p915_mp_order_events::FIELDS,
    },
    // p916
    EntityRegistration {
        meta: &crate::projections::p916_mp_sales_funnel_turnovers::ENTITY_METADATA,
        fields: crate::projections::p916_mp_sales_funnel_turnovers::FIELDS,
    },
    // gl
    EntityRegistration {
        meta: &crate::general_ledger::ENTITY_METADATA,
        fields: crate::general_ledger::FIELDS,
    },
];
