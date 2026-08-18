//! ViewModel for WB Sales details
//!
//! Contains reactive state, commands, and lazy loading logic.

use super::api::*;
use crate::layout::global_context::AppGlobalContext;
use contracts::general_ledger::GeneralLedgerEntryDto;
use contracts::projections::p903_wb_finance_report::dto::WbFinanceReportDto;
use leptos::prelude::*;
use wasm_bindgen_futures::spawn_local;

/// ViewModel for WB Sales details form
#[derive(Clone)]
pub struct WbSalesDetailsVm {
    // === Entity ID ===
    pub id: RwSignal<Option<String>>,

    // === Main data (loaded from API) ===
    pub sale: RwSignal<Option<WbSalesDetailDto>>,

    // === Related data (lazy loaded) ===
    pub raw_json: RwSignal<Option<String>>,
    pub raw_json_loaded: RwSignal<bool>,
    pub raw_json_loading: RwSignal<bool>,

    pub projections: RwSignal<Option<serde_json::Value>>,
    pub projections_loaded: RwSignal<bool>,
    pub projections_loading: RwSignal<bool>,

    pub finance_reports: RwSignal<Vec<WbFinanceReportDto>>,
    pub finance_reports_loaded: RwSignal<bool>,
    pub finance_reports_loading: RwSignal<bool>,
    pub finance_reports_error: RwSignal<Option<String>>,

    pub general_ledger_entries: RwSignal<Vec<GeneralLedgerEntryDto>>,
    pub general_ledger_entries_loaded: RwSignal<bool>,
    pub general_ledger_entries_loading: RwSignal<bool>,
    pub general_ledger_entries_error: RwSignal<Option<String>>,

    pub advert_attribution: RwSignal<Option<AdvertAttributionResponse>>,
    pub advert_attribution_loaded: RwSignal<bool>,
    pub advert_attribution_loading: RwSignal<bool>,
    pub advert_attribution_error: RwSignal<Option<String>>,

    pub marketplace_product_info: RwSignal<Option<MarketplaceProductInfo>>,
    pub nomenclature_info: RwSignal<Option<NomenclatureInfo>>,
    pub connection_info: RwSignal<Option<ConnectionInfo>>,
    pub organization_info: RwSignal<Option<OrganizationInfo>>,
    pub marketplace_info: RwSignal<Option<MarketplaceInfo>>,

    // === UI State ===
    pub active_tab: RwSignal<&'static str>,
    pub loading: RwSignal<bool>,
    pub posting: RwSignal<bool>,
    pub refreshing_price: RwSignal<bool>,
    pub error: RwSignal<Option<String>>,
}

impl WbSalesDetailsVm {
    /// Create a new ViewModel instance
    pub fn new() -> Self {
        Self {
            id: RwSignal::new(None),
            sale: RwSignal::new(None),

            raw_json: RwSignal::new(None),
            raw_json_loaded: RwSignal::new(false),
            raw_json_loading: RwSignal::new(false),

            projections: RwSignal::new(None),
            projections_loaded: RwSignal::new(false),
            projections_loading: RwSignal::new(false),

            finance_reports: RwSignal::new(Vec::new()),
            finance_reports_loaded: RwSignal::new(false),
            finance_reports_loading: RwSignal::new(false),
            finance_reports_error: RwSignal::new(None),

            general_ledger_entries: RwSignal::new(Vec::new()),
            general_ledger_entries_loaded: RwSignal::new(false),
            general_ledger_entries_loading: RwSignal::new(false),
            general_ledger_entries_error: RwSignal::new(None),

            advert_attribution: RwSignal::new(None),
            advert_attribution_loaded: RwSignal::new(false),
            advert_attribution_loading: RwSignal::new(false),
            advert_attribution_error: RwSignal::new(None),

            marketplace_product_info: RwSignal::new(None),
            nomenclature_info: RwSignal::new(None),
            connection_info: RwSignal::new(None),
            organization_info: RwSignal::new(None),
            marketplace_info: RwSignal::new(None),

            active_tab: RwSignal::new("general"),
            loading: RwSignal::new(false),
            posting: RwSignal::new(false),
            refreshing_price: RwSignal::new(false),
            error: RwSignal::new(None),
        }
    }

    /// Check if document is posted
    pub fn is_posted(&self) -> Signal<bool> {
        let sale = self.sale;
        Signal::derive(move || sale.get().map(|s| s.metadata.is_posted).unwrap_or(false))
    }

    /// Check if document is a customer return
    pub fn is_customer_return(&self) -> Signal<bool> {
        let sale = self.sale;
        Signal::derive(move || sale.get().map(|s| s.is_customer_return).unwrap_or(false))
    }

    /// Get document number for display
    pub fn document_no(&self) -> Signal<String> {
        let sale = self.sale;
        Signal::derive(move || {
            sale.get()
                .map(|s| s.header.document_no.clone())
                .unwrap_or_default()
        })
    }

    /// Get sale ID for display
    pub fn sale_id(&self) -> Signal<String> {
        let sale = self.sale;
        Signal::derive(move || {
            sale.get()
                .and_then(|s| s.header.sale_id.clone())
                .unwrap_or_else(|| "—".to_string())
        })
    }

    /// Get projections count for badge
    pub fn projections_count(&self) -> Signal<usize> {
        let projections = self.projections;
        Signal::derive(move || {
            projections
                .get()
                .as_ref()
                .map(|p| {
                    let p900_len = p["p900_sales_register"]
                        .as_array()
                        .map(|a| a.len())
                        .unwrap_or(0);
                    let p904_len = p["p904_sales_data"]
                        .as_array()
                        .map(|a| a.len())
                        .unwrap_or(0);
                    let p913_len = p["p913_wb_advert_order_attr"]
                        .as_array()
                        .map(|a| a.len())
                        .unwrap_or(0);
                    let p916_len = p["p916_mp_sales_funnel_turnovers"]
                        .as_array()
                        .map(|a| a.len())
                        .unwrap_or(0);
                    p900_len + p904_len + p913_len + p916_len
                })
                .unwrap_or(0)
        })
    }

    /// Get finance reports count for badge
    pub fn finance_reports_count(&self) -> Signal<usize> {
        let reports = self.finance_reports;
        Signal::derive(move || reports.get().len())
    }

    /// Get journal entries count for badge
    pub fn general_ledger_entries_count(&self) -> Signal<usize> {
        let entries = self.general_ledger_entries;
        Signal::derive(move || entries.get().len())
    }

    /// Get advert attribution row count for badge
    pub fn advert_attribution_count(&self) -> Signal<usize> {
        let attribution = self.advert_attribution;
        Signal::derive(move || attribution.get().map(|a| a.totals.rows_count).unwrap_or(0))
    }

    /// Set active tab
    pub fn set_tab(&self, tab: &'static str) {
        self.active_tab.set(tab);
    }

    /// Load main document data
    pub fn load(&self, id: String) {
        let vm = self.clone();
        vm.id.set(Some(id.clone()));
        vm.loading.set(true);
        vm.error.set(None);

        spawn_local(async move {
            match fetch_by_id(&id).await {
                Ok(data) => {
                    // Set sale first (needed for finance_reports loading)
                    vm.sale.set(Some(data.clone()));

                    // Load related data (marketplace product, nomenclature)
                    vm.load_related_data(&data);

                    // Load data for badges immediately (projections, finance reports, journal, attribution)
                    vm.load_projections();
                    vm.load_finance_reports();
                    vm.load_general_ledger_entries();
                    vm.load_advert_attribution();

                    vm.loading.set(false);
                }
                Err(e) => {
                    vm.error.set(Some(e));
                    vm.loading.set(false);
                }
            }
        });
    }

    /// Load related data (marketplace product, nomenclature info)
    fn load_related_data(&self, data: &WbSalesDetailDto) {
        // Load marketplace product info
        if let Some(ref mp_ref) = data.marketplace_product_ref {
            let mp_ref = mp_ref.clone();
            let mp_info = self.marketplace_product_info;
            spawn_local(async move {
                if let Ok(info) = fetch_marketplace_product(&mp_ref).await {
                    mp_info.set(Some(info));
                }
            });
        } else {
            self.marketplace_product_info.set(None);
        }

        // Load nomenclature info
        if let Some(ref nom_ref) = data.nomenclature_ref {
            let nom_ref = nom_ref.clone();
            let nom_info = self.nomenclature_info;
            spawn_local(async move {
                if let Ok(info) = fetch_nomenclature(&nom_ref).await {
                    nom_info.set(Some(info));
                }
            });
        } else {
            self.nomenclature_info.set(None);
        }

        // Load connection info
        let conn_id = data.header.connection_id.clone();
        let conn_info = self.connection_info;
        spawn_local(async move {
            if let Ok(info) = fetch_connection(&conn_id).await {
                conn_info.set(Some(info));
            }
        });

        // Load organization info
        let org_id = data.header.organization_id.clone();
        let org_info = self.organization_info;
        spawn_local(async move {
            if let Ok(info) = fetch_organization(&org_id).await {
                org_info.set(Some(info));
            }
        });

        // Load marketplace info
        let mp_id = data.header.marketplace_id.clone();
        let mp_info = self.marketplace_info;
        spawn_local(async move {
            if let Ok(info) = fetch_marketplace(&mp_id).await {
                mp_info.set(Some(info));
            }
        });
    }

    /// Load raw JSON (lazy, for "json" tab)
    pub fn load_raw_json(&self) {
        if self.raw_json_loaded.get() || self.raw_json_loading.get() {
            return;
        }

        let Some(sale) = self.sale.get() else {
            return;
        };

        let raw_payload_ref = sale.source_meta.raw_payload_ref.clone();
        let vm = self.clone();
        vm.raw_json_loading.set(true);

        spawn_local(async move {
            match fetch_raw_json(&raw_payload_ref).await {
                Ok(json) => {
                    vm.raw_json.set(Some(json));
                    vm.raw_json_loaded.set(true);
                }
                Err(e) => {
                    leptos::logging::log!("Failed to load raw JSON: {}", e);
                }
            }
            vm.raw_json_loading.set(false);
        });
    }

    /// Load projections (lazy, for "projections" tab)
    pub fn load_projections(&self) {
        if self.projections_loaded.get_untracked() || self.projections_loading.get_untracked() {
            return;
        }

        let Some(id) = self.id.get_untracked() else {
            return;
        };

        let vm = self.clone();
        vm.projections_loading.set(true);

        spawn_local(async move {
            match fetch_projections(&id).await {
                Ok(proj) => {
                    vm.projections.set(Some(proj));
                    vm.projections_loaded.set(true);
                }
                Err(e) => {
                    leptos::logging::log!("Failed to load projections: {}", e);
                }
            }
            vm.projections_loading.set(false);
        });
    }

    /// Load journal entries (lazy, for "journal" tab)
    pub fn load_general_ledger_entries(&self) {
        if self.general_ledger_entries_loaded.get_untracked()
            || self.general_ledger_entries_loading.get_untracked()
        {
            return;
        }

        let Some(id) = self.id.get_untracked() else {
            return;
        };

        let vm = self.clone();
        vm.general_ledger_entries_loading.set(true);
        vm.general_ledger_entries_error.set(None);

        spawn_local(async move {
            match fetch_general_ledger_entries(&id).await {
                Ok(entries) => {
                    vm.general_ledger_entries.set(entries);
                    vm.general_ledger_entries_loaded.set(true);
                }
                Err(e) => {
                    vm.general_ledger_entries_error.set(Some(e));
                }
            }
            vm.general_ledger_entries_loading.set(false);
        });
    }

    /// Load finance reports (lazy, for "links" or "line" tabs)
    pub fn load_finance_reports(&self) {
        if self.finance_reports_loaded.get_untracked()
            || self.finance_reports_loading.get_untracked()
        {
            return;
        }

        let Some(sale) = self.sale.get_untracked() else {
            return;
        };

        let srid = sale.header.document_no.clone();
        if srid.is_empty() {
            return;
        }

        let vm = self.clone();
        vm.finance_reports_loading.set(true);
        vm.finance_reports_error.set(None);

        spawn_local(async move {
            match fetch_finance_reports(&srid).await {
                Ok(reports) => {
                    vm.finance_reports.set(reports);
                    vm.finance_reports_loaded.set(true);
                }
                Err(e) => {
                    vm.finance_reports_error.set(Some(e));
                }
            }
            vm.finance_reports_loading.set(false);
        });
    }

    /// Load advert attribution (lazy, for "advert_attribution" tab and badge)
    pub fn load_advert_attribution(&self) {
        if self.advert_attribution_loaded.get_untracked()
            || self.advert_attribution_loading.get_untracked()
        {
            return;
        }

        let Some(id) = self.id.get_untracked() else {
            return;
        };

        let vm = self.clone();
        vm.advert_attribution_loading.set(true);
        vm.advert_attribution_error.set(None);

        spawn_local(async move {
            match fetch_advert_attribution(&id).await {
                Ok(data) => {
                    vm.advert_attribution.set(Some(data));
                    vm.advert_attribution_loaded.set(true);
                }
                Err(e) => {
                    vm.advert_attribution_error.set(Some(e));
                }
            }
            vm.advert_attribution_loading.set(false);
        });
    }

    /// Post document (проведение)
    pub fn post(&self) {
        let Some(id) = self.id.get() else {
            return;
        };

        let vm = self.clone();
        vm.posting.set(true);

        spawn_local(async move {
            match post_document(&id).await {
                Ok(()) => {
                    // Reload document data
                    vm.reload().await;
                }
                Err(e) => {
                    leptos::logging::log!("Error posting: {}", e);
                }
            }
            vm.posting.set(false);
        });
    }

    /// Unpost document (отмена проведения)
    pub fn unpost(&self) {
        let Some(id) = self.id.get() else {
            return;
        };

        let vm = self.clone();
        vm.posting.set(true);

        spawn_local(async move {
            match unpost_document(&id).await {
                Ok(()) => {
                    // Reload document data
                    vm.reload().await;
                }
                Err(e) => {
                    leptos::logging::log!("Error unposting: {}", e);
                }
            }
            vm.posting.set(false);
        });
    }

    /// Reload document and projections after post/unpost
    async fn reload(&self) {
        let Some(id) = self.id.get() else {
            return;
        };

        // Reload main data
        if let Ok(data) = fetch_by_id(&id).await {
            self.load_related_data(&data);
            self.sale.set(Some(data));
        }

        // Reload projections if already loaded
        if self.projections_loaded.get() {
            if let Ok(proj) = fetch_projections(&id).await {
                self.projections.set(Some(proj));
            }
        }

        // Reset journal entries so they reload on next tab visit
        self.general_ledger_entries_loaded.set(false);
        self.general_ledger_entries_loading.set(false);
        self.general_ledger_entries.set(Vec::new());
        if let Ok(entries) = fetch_general_ledger_entries(&id).await {
            self.general_ledger_entries.set(entries);
            self.general_ledger_entries_loaded.set(true);
        }

        // Reset advert attribution: gl_advert_clicks_order_expense changes after posting.
        self.advert_attribution_loaded.set(false);
        self.advert_attribution_loading.set(false);
        self.advert_attribution.set(None);
        self.advert_attribution_error.set(None);
        if let Ok(data) = fetch_advert_attribution(&id).await {
            self.advert_attribution.set(Some(data));
            self.advert_attribution_loaded.set(true);
        }
    }

    pub fn open_order_by_srid(&self, srid: String, tabs_store: AppGlobalContext) {
        if srid.is_empty() {
            return;
        }
        spawn_local(async move {
            if let Some(uuid) = resolve_order_uuid_by_srid(&srid).await {
                let short: String = srid.chars().take(16).collect();
                tabs_store.open_tab(
                    &format!("a015_wb_orders_details_{}", uuid),
                    &format!("WB Order {}", short),
                );
            }
        });
    }

    /// Refresh dealer price
    pub fn refresh_dealer_price(&self) {
        let Some(id) = self.id.get() else {
            return;
        };

        let vm = self.clone();
        vm.refreshing_price.set(true);

        spawn_local(async move {
            match refresh_dealer_price(&id).await {
                Ok(()) => {
                    // Reload document data
                    vm.reload().await;
                    leptos::logging::log!("Dealer price refreshed successfully");
                }
                Err(e) => {
                    leptos::logging::log!("Error refreshing dealer price: {}", e);
                }
            }
            vm.refreshing_price.set(false);
        });
    }
}

impl Default for WbSalesDetailsVm {
    fn default() -> Self {
        Self::new()
    }
}
