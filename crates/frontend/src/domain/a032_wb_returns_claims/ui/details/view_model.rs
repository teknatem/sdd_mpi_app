use super::api::*;
use crate::layout::global_context::AppGlobalContext;
use leptos::prelude::*;
use wasm_bindgen_futures::spawn_local;

#[derive(Clone)]
pub struct WbReturnsClaimsDetailsVm {
    pub id: RwSignal<Option<String>>,
    pub item: RwSignal<Option<WbReturnsClaimsDetailDto>>,
    pub active_tab: RwSignal<&'static str>,
    pub loading: RwSignal<bool>,
    pub error: RwSignal<Option<String>>,

    pub raw_json: RwSignal<Option<String>>,
    pub raw_json_loaded: RwSignal<bool>,
    pub raw_json_loading: RwSignal<bool>,

    pub connection_info: RwSignal<Option<ConnectionInfo>>,
    pub organization_info: RwSignal<Option<OrganizationInfo>>,
    pub marketplace_info: RwSignal<Option<MarketplaceInfo>>,
}

impl WbReturnsClaimsDetailsVm {
    pub fn new() -> Self {
        Self {
            id: RwSignal::new(None),
            item: RwSignal::new(None),
            active_tab: RwSignal::new("general"),
            loading: RwSignal::new(false),
            error: RwSignal::new(None),

            raw_json: RwSignal::new(None),
            raw_json_loaded: RwSignal::new(false),
            raw_json_loading: RwSignal::new(false),

            connection_info: RwSignal::new(None),
            organization_info: RwSignal::new(None),
            marketplace_info: RwSignal::new(None),
        }
    }

    pub fn set_tab(&self, tab: &'static str) {
        self.active_tab.set(tab);
    }

    pub fn claim_id(&self) -> Signal<String> {
        let item = self.item;
        Signal::derive(move || item.get().map(|d| d.claim_id.clone()).unwrap_or_default())
    }

    pub fn load(&self, id: String) {
        let vm = self.clone();
        vm.id.set(Some(id.clone()));
        vm.loading.set(true);
        vm.error.set(None);

        spawn_local(async move {
            match fetch_by_id(&id).await {
                Ok(data) => {
                    vm.load_related_data(&data);
                    vm.item.set(Some(data));
                    vm.loading.set(false);
                }
                Err(e) => {
                    vm.error.set(Some(e));
                    vm.loading.set(false);
                }
            }
        });
    }

    fn load_related_data(&self, d: &WbReturnsClaimsDetailDto) {
        let conn_id = d.connection_id.clone();
        let conn_info = self.connection_info;
        spawn_local(async move {
            if let Ok(info) = fetch_connection(&conn_id).await {
                conn_info.set(Some(info));
            }
        });

        let org_id = d.organization_id.clone();
        let org_info = self.organization_info;
        spawn_local(async move {
            if let Ok(info) = fetch_organization(&org_id).await {
                org_info.set(Some(info));
            }
        });

        let mp_id = d.marketplace_id.clone();
        let mp_info = self.marketplace_info;
        spawn_local(async move {
            if let Ok(info) = fetch_marketplace(&mp_id).await {
                mp_info.set(Some(info));
            }
        });
    }

    pub fn load_raw_json(&self) {
        if self.raw_json_loaded.get_untracked() || self.raw_json_loading.get_untracked() {
            return;
        }
        let Some(item) = self.item.get_untracked() else {
            return;
        };

        self.raw_json_loading.set(true);
        let json_str = serde_json::to_string_pretty(&item).ok();
        self.raw_json.set(json_str);
        self.raw_json_loaded.set(true);
        self.raw_json_loading.set(false);
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
}

impl Default for WbReturnsClaimsDetailsVm {
    fn default() -> Self {
        Self::new()
    }
}
