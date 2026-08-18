//! ViewModel for WB Orders details

use super::api::*;
use contracts::projections::p903_wb_finance_report::dto::WbFinanceReportDto;
use leptos::prelude::*;
use wasm_bindgen_futures::spawn_local;

#[derive(Clone)]
pub struct WbOrdersDetailsVm {
    pub id: RwSignal<Option<String>>,
    pub order: RwSignal<Option<WbOrderDetailDto>>,

    pub raw_json: RwSignal<Option<String>>,
    pub raw_json_loaded: RwSignal<bool>,
    pub raw_json_loading: RwSignal<bool>,

    pub marketplace_raw_json: RwSignal<Option<String>>,
    pub marketplace_raw_json_loaded: RwSignal<bool>,
    pub marketplace_raw_json_loading: RwSignal<bool>,

    pub finance_reports: RwSignal<Vec<WbFinanceReportDto>>,
    pub finance_reports_loaded: RwSignal<bool>,
    pub finance_reports_loading: RwSignal<bool>,
    pub finance_reports_error: RwSignal<Option<String>>,

    pub wb_sales: RwSignal<Vec<WbSalesListItemDto>>,
    pub wb_sales_loaded: RwSignal<bool>,
    pub wb_sales_loading: RwSignal<bool>,
    pub wb_sales_error: RwSignal<Option<String>>,

    pub marketplace_product_info: RwSignal<Option<MarketplaceProductInfo>>,
    pub nomenclature_info: RwSignal<Option<NomenclatureInfo>>,
    pub base_nomenclature_info: RwSignal<Option<NomenclatureInfo>>,
    pub supply_link: RwSignal<Option<WbSupplyLinkInfo>>,
    pub supply_link_loaded: RwSignal<bool>,
    pub supply_link_loading: RwSignal<bool>,
    pub connection_info: RwSignal<Option<ConnectionInfo>>,
    pub organization_info: RwSignal<Option<OrganizationInfo>>,
    pub marketplace_info: RwSignal<Option<MarketplaceInfo>>,

    pub projections: RwSignal<Option<serde_json::Value>>,
    pub projections_loaded: RwSignal<bool>,
    pub projections_loading: RwSignal<bool>,

    pub active_tab: RwSignal<&'static str>,
    pub loading: RwSignal<bool>,
    pub posting: RwSignal<bool>,
    pub error: RwSignal<Option<String>>,
}

impl WbOrdersDetailsVm {
    pub fn new() -> Self {
        Self {
            id: RwSignal::new(None),
            order: RwSignal::new(None),

            raw_json: RwSignal::new(None),
            raw_json_loaded: RwSignal::new(false),
            raw_json_loading: RwSignal::new(false),

            marketplace_raw_json: RwSignal::new(None),
            marketplace_raw_json_loaded: RwSignal::new(false),
            marketplace_raw_json_loading: RwSignal::new(false),

            finance_reports: RwSignal::new(Vec::new()),
            finance_reports_loaded: RwSignal::new(false),
            finance_reports_loading: RwSignal::new(false),
            finance_reports_error: RwSignal::new(None),

            wb_sales: RwSignal::new(Vec::new()),
            wb_sales_loaded: RwSignal::new(false),
            wb_sales_loading: RwSignal::new(false),
            wb_sales_error: RwSignal::new(None),

            marketplace_product_info: RwSignal::new(None),
            nomenclature_info: RwSignal::new(None),
            base_nomenclature_info: RwSignal::new(None),
            supply_link: RwSignal::new(None),
            supply_link_loaded: RwSignal::new(false),
            supply_link_loading: RwSignal::new(false),
            connection_info: RwSignal::new(None),
            organization_info: RwSignal::new(None),
            marketplace_info: RwSignal::new(None),

            projections: RwSignal::new(None),
            projections_loaded: RwSignal::new(false),
            projections_loading: RwSignal::new(false),

            active_tab: RwSignal::new("general"),
            loading: RwSignal::new(false),
            posting: RwSignal::new(false),
            error: RwSignal::new(None),
        }
    }

    pub fn is_posted(&self) -> Signal<bool> {
        let order = self.order;
        Signal::derive(move || order.get().map(|s| s.metadata.is_posted).unwrap_or(false))
    }

    pub fn document_no(&self) -> Signal<String> {
        let order = self.order;
        Signal::derive(move || {
            order
                .get()
                .map(|s| s.header.document_no.clone())
                .unwrap_or_default()
        })
    }

    pub fn finance_reports_count(&self) -> Signal<usize> {
        let reports = self.finance_reports;
        Signal::derive(move || reports.get().len())
    }

    pub fn wb_sales_count(&self) -> Signal<usize> {
        let wb_sales = self.wb_sales;
        Signal::derive(move || wb_sales.get().len())
    }

    /// Кол-во строк движений проекций (p909 + p916) для бейджа вкладки.
    pub fn projections_count(&self) -> Signal<usize> {
        let projections = self.projections;
        Signal::derive(move || {
            projections
                .get()
                .as_ref()
                .map(|p| {
                    let p909 = p["p909_mp_order_line_turnovers"]
                        .as_array()
                        .map(|a| a.len())
                        .unwrap_or(0);
                    let p916 = p["p916_mp_sales_funnel_turnovers"]
                        .as_array()
                        .map(|a| a.len())
                        .unwrap_or(0);
                    p909 + p916
                })
                .unwrap_or(0)
        })
    }

    pub fn set_tab(&self, tab: &'static str) {
        self.active_tab.set(tab);
    }

    pub fn load(&self, id: String) {
        let vm = self.clone();
        vm.id.set(Some(id.clone()));
        vm.loading.set(true);
        vm.error.set(None);
        vm.supply_link.set(None);
        vm.supply_link_loaded.set(false);
        vm.supply_link_loading.set(false);

        spawn_local(async move {
            match fetch_by_id(&id).await {
                Ok(data) => {
                    vm.order.set(Some(data.clone()));
                    vm.load_related_data(&data);
                    vm.load_supply_link(&data.id);
                    // finance_reports и wb_sales грузятся по-требованию
                    // из Effect в page.rs при переходе на нужную вкладку
                    vm.loading.set(false);
                }
                Err(e) => {
                    vm.error.set(Some(e));
                    vm.loading.set(false);
                }
            }
        });
    }

    fn load_supply_link(&self, order_id: &str) {
        let order_id = order_id.to_string();
        let vm = self.clone();
        vm.supply_link.set(None);
        vm.supply_link_loaded.set(false);
        vm.supply_link_loading.set(true);

        spawn_local(async move {
            match fetch_supply_for_order(&order_id).await {
                Ok(link) => vm.supply_link.set(link),
                Err(e) => leptos::logging::log!("Failed to load WB supply link: {}", e),
            }
            vm.supply_link_loaded.set(true);
            vm.supply_link_loading.set(false);
        });
    }

    fn load_related_data(&self, data: &WbOrderDetailDto) {
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

        if let Some(ref base_nom_ref) = data.base_nomenclature_ref {
            let base_nom_ref = base_nom_ref.clone();
            let base_nom_info = self.base_nomenclature_info;
            spawn_local(async move {
                if let Ok(info) = fetch_nomenclature(&base_nom_ref).await {
                    base_nom_info.set(Some(info));
                }
            });
        } else {
            self.base_nomenclature_info.set(None);
        }

        let conn_id = data.header.connection_id.clone();
        let conn_info = self.connection_info;
        spawn_local(async move {
            if let Ok(info) = fetch_connection(&conn_id).await {
                conn_info.set(Some(info));
            }
        });

        let org_id = data.header.organization_id.clone();
        let org_info = self.organization_info;
        spawn_local(async move {
            if let Ok(info) = fetch_organization(&org_id).await {
                org_info.set(Some(info));
            }
        });

        let mp_id = data.header.marketplace_id.clone();
        let mp_info = self.marketplace_info;
        spawn_local(async move {
            if let Ok(info) = fetch_marketplace(&mp_id).await {
                mp_info.set(Some(info));
            }
        });
    }

    pub fn load_raw_json(&self) {
        if self.raw_json_loaded.get() || self.raw_json_loading.get() {
            return;
        }
        let Some(order) = self.order.get() else {
            return;
        };

        let raw_payload_ref = order.source_meta.raw_payload_ref.clone();
        let vm = self.clone();
        vm.raw_json_loading.set(true);

        spawn_local(async move {
            match fetch_raw_json(&raw_payload_ref).await {
                Ok(json) => {
                    vm.raw_json.set(Some(json));
                    vm.raw_json_loaded.set(true);
                }
                Err(e) => leptos::logging::log!("Failed to load raw JSON: {}", e),
            }
            vm.raw_json_loading.set(false);
        });
    }

    pub fn load_marketplace_raw_json(&self) {
        if self.marketplace_raw_json_loaded.get() || self.marketplace_raw_json_loading.get() {
            return;
        }
        let Some(order) = self.order.get() else {
            return;
        };
        let Some(raw_payload_ref) = order.source_meta.marketplace_raw_payload_ref.clone() else {
            self.marketplace_raw_json_loaded.set(true);
            return;
        };

        let vm = self.clone();
        vm.marketplace_raw_json_loading.set(true);

        spawn_local(async move {
            match fetch_raw_json(&raw_payload_ref).await {
                Ok(json) => {
                    vm.marketplace_raw_json.set(Some(json));
                    vm.marketplace_raw_json_loaded.set(true);
                }
                Err(e) => leptos::logging::log!("Failed to load marketplace raw JSON: {}", e),
            }
            vm.marketplace_raw_json_loading.set(false);
        });
    }

    pub fn load_finance_reports(&self) {
        if self.finance_reports_loaded.get_untracked()
            || self.finance_reports_loading.get_untracked()
        {
            return;
        }
        let Some(order) = self.order.get_untracked() else {
            return;
        };

        let srid = order.header.document_no.clone();
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
                Err(e) => vm.finance_reports_error.set(Some(e)),
            }
            vm.finance_reports_loading.set(false);
        });
    }

    pub fn load_wb_sales(&self) {
        if self.wb_sales_loaded.get_untracked() || self.wb_sales_loading.get_untracked() {
            return;
        }
        let Some(order) = self.order.get_untracked() else {
            return;
        };

        let document_no = order.header.document_no.clone();
        if document_no.is_empty() {
            return;
        }

        let vm = self.clone();
        vm.wb_sales_loading.set(true);
        vm.wb_sales_error.set(None);

        spawn_local(async move {
            match fetch_wb_sales(&document_no).await {
                Ok(items) => {
                    vm.wb_sales.set(items);
                    vm.wb_sales_loaded.set(true);
                }
                Err(e) => vm.wb_sales_error.set(Some(e)),
            }
            vm.wb_sales_loading.set(false);
        });
    }

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
                Ok(value) => {
                    vm.projections.set(Some(value));
                    vm.projections_loaded.set(true);
                }
                Err(e) => leptos::logging::log!("Failed to load a015 projections: {}", e),
            }
            vm.projections_loading.set(false);
        });
    }

    pub fn post(&self) {
        let Some(id) = self.id.get() else {
            return;
        };
        let vm = self.clone();
        vm.posting.set(true);

        spawn_local(async move {
            if let Err(e) = post_document(&id).await {
                leptos::logging::log!("Error posting: {}", e);
            } else {
                vm.reload().await;
            }
            vm.posting.set(false);
        });
    }

    async fn reload(&self) {
        let Some(id) = self.id.get() else {
            return;
        };
        if let Ok(data) = fetch_by_id(&id).await {
            self.order.set(Some(data.clone()));
            self.load_related_data(&data);
        }
        // Проекции могли измениться при проведении — перезагружаем, если уже показывались.
        if self.projections_loaded.get_untracked() {
            self.projections_loaded.set(false);
            self.projections_loading.set(false);
            self.load_projections();
        }
    }
}

impl Default for WbOrdersDetailsVm {
    fn default() -> Self {
        Self::new()
    }
}
