//! ViewModel for WB Supply details

use super::api::*;
use leptos::prelude::*;
use std::collections::{HashMap, HashSet};
use wasm_bindgen_futures::spawn_local;

#[derive(Clone)]
pub struct WbSupplyDetailsVm {
    pub id: RwSignal<Option<String>>,
    pub supply: RwSignal<Option<WbSupplyDetailDto>>,

    pub orders: RwSignal<Vec<SupplyOrderDto>>,
    pub orders_loaded: RwSignal<bool>,
    pub orders_loading: RwSignal<bool>,
    pub orders_error: RwSignal<Option<String>>,

    /// Currently selected sticker format: "png" | "svg" | "zplv" | "zplh"
    pub sticker_type: RwSignal<String>,

    pub raw_json: RwSignal<Option<String>>,
    pub raw_json_loaded: RwSignal<bool>,
    pub raw_json_loading: RwSignal<bool>,

    pub connection_info: RwSignal<Option<ConnectionInfo>>,
    pub organization_info: RwSignal<Option<OrganizationInfo>>,
    pub nomenclatures_info: RwSignal<HashMap<String, NomenclatureInfo>>,

    pub active_tab: RwSignal<&'static str>,
    pub loading: RwSignal<bool>,
    pub error: RwSignal<Option<String>>,
}

impl WbSupplyDetailsVm {
    pub fn new() -> Self {
        Self {
            id: RwSignal::new(None),
            supply: RwSignal::new(None),

            orders: RwSignal::new(Vec::new()),
            orders_loaded: RwSignal::new(false),
            orders_loading: RwSignal::new(false),
            orders_error: RwSignal::new(None),

            sticker_type: RwSignal::new("zplv".to_string()),

            raw_json: RwSignal::new(None),
            raw_json_loaded: RwSignal::new(false),
            raw_json_loading: RwSignal::new(false),

            connection_info: RwSignal::new(None),
            organization_info: RwSignal::new(None),
            nomenclatures_info: RwSignal::new(HashMap::new()),

            active_tab: RwSignal::new("general"),
            loading: RwSignal::new(false),
            error: RwSignal::new(None),
        }
    }

    pub fn supply_id(&self) -> Signal<String> {
        let supply = self.supply;
        Signal::derive(move || {
            supply
                .get()
                .map(|s| s.header.supply_id.clone())
                .unwrap_or_default()
        })
    }

    pub fn orders_count(&self) -> Signal<usize> {
        let orders = self.orders;
        Signal::derive(move || orders.get().len())
    }

    pub fn set_tab(&self, tab: &'static str) {
        self.active_tab.set(tab);
    }

    pub fn load(&self, id: String) {
        let vm = self.clone();
        vm.id.set(Some(id.clone()));
        vm.loading.set(true);
        vm.error.set(None);

        spawn_local(async move {
            // The id can be either a UUID (when opened from supply list) or
            // a WB supply ID like "WB-GI-32319994" (when navigated from orders list).
            // WB supply IDs always start with "WB-"; UUIDs use hex+hyphens only.
            let fetch_result = if id.starts_with("WB-") {
                fetch_by_wb_id(&id).await
            } else {
                fetch_by_id(&id).await
            };
            match fetch_result {
                Ok(data) => {
                    // Orders are embedded in the supply aggregate
                    vm.orders.set(data.supply_orders.clone());
                    vm.orders_loaded.set(true);
                    vm.nomenclatures_info.set(HashMap::new());
                    vm.load_nomenclatures(&data.supply_orders);

                    let conn_id = data.header.connection_id.clone();
                    let org_id = data.header.organization_id.clone();

                    vm.supply.set(Some(data));
                    vm.loading.set(false);

                    let conn_info = vm.connection_info;
                    spawn_local(async move {
                        if let Ok(info) = fetch_connection(&conn_id).await {
                            conn_info.set(Some(info));
                        }
                    });

                    let org_info = vm.organization_info;
                    spawn_local(async move {
                        if let Ok(info) = fetch_organization(&org_id).await {
                            org_info.set(Some(info));
                        }
                    });
                }
                Err(e) => {
                    vm.error.set(Some(e));
                    vm.loading.set(false);
                }
            }
        });
    }

    pub fn load_raw_json(&self) {
        if self.raw_json_loaded.get() || self.raw_json_loading.get() {
            return;
        }
        let Some(supply) = self.supply.get() else {
            return;
        };

        let raw_ref = supply.source_meta.raw_payload_ref.clone();
        let vm = self.clone();
        vm.raw_json_loading.set(true);

        spawn_local(async move {
            match fetch_raw_json(&raw_ref).await {
                Ok(json) => {
                    vm.raw_json.set(Some(json));
                    vm.raw_json_loaded.set(true);
                }
                Err(e) => leptos::logging::log!("Failed to load raw JSON: {}", e),
            }
            vm.raw_json_loading.set(false);
        });
    }

    fn load_nomenclatures(&self, orders: &[SupplyOrderDto]) {
        const ZERO_UUID: &str = "00000000-0000-0000-0000-000000000000";

        let refs: HashSet<String> = orders
            .iter()
            .flat_map(|order| {
                let base_ref = order
                    .base_nomenclature_ref
                    .as_deref()
                    .filter(|base_ref| {
                        let base_ref = base_ref.trim();
                        !base_ref.is_empty()
                            && base_ref != ZERO_UUID
                            && Some(base_ref) != order.nomenclature_ref.as_deref()
                    })
                    .map(ToOwned::to_owned);

                [
                    order
                        .nomenclature_ref
                        .as_deref()
                        .map(str::trim)
                        .filter(|nom_ref| !nom_ref.is_empty() && *nom_ref != ZERO_UUID)
                        .map(ToOwned::to_owned),
                    base_ref,
                ]
            })
            .flatten()
            .collect();

        for nom_ref in refs {
            let info_map = self.nomenclatures_info;
            spawn_local(async move {
                if let Ok(info) = fetch_nomenclature(&nom_ref).await {
                    info_map.update(|map| {
                        map.insert(nom_ref.clone(), info);
                    });
                }
            });
        }
    }
}

impl Default for WbSupplyDetailsVm {
    fn default() -> Self {
        Self::new()
    }
}
