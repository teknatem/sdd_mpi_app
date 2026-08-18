//! ViewModel for YM Returns details (MVVM Standard, mirrors a015_wb_orders)

use super::api::*;
use leptos::prelude::*;
use wasm_bindgen_futures::spawn_local;

#[derive(Clone)]
pub struct YmReturnDetailsVm {
    pub id: RwSignal<Option<String>>,
    pub return_data: RwSignal<Option<YmReturnDetailDto>>,

    pub raw_json: RwSignal<Option<String>>,
    pub raw_json_loaded: RwSignal<bool>,
    pub raw_json_loading: RwSignal<bool>,

    pub projections: RwSignal<Option<serde_json::Value>>,
    pub projections_loaded: RwSignal<bool>,
    pub projections_loading: RwSignal<bool>,

    pub connection_info: RwSignal<Option<ConnectionInfo>>,
    pub organization_info: RwSignal<Option<OrganizationInfo>>,
    pub marketplace_info: RwSignal<Option<MarketplaceInfo>>,
    /// Внутренний id исходного заказа (a013_ym_order) для гиперссылки; None если не найден
    pub source_order_id: RwSignal<Option<String>>,

    pub active_tab: RwSignal<&'static str>,
    pub loading: RwSignal<bool>,
    pub posting: RwSignal<bool>,
    pub error: RwSignal<Option<String>>,
}

impl YmReturnDetailsVm {
    pub fn new() -> Self {
        Self {
            id: RwSignal::new(None),
            return_data: RwSignal::new(None),

            raw_json: RwSignal::new(None),
            raw_json_loaded: RwSignal::new(false),
            raw_json_loading: RwSignal::new(false),

            projections: RwSignal::new(None),
            projections_loaded: RwSignal::new(false),
            projections_loading: RwSignal::new(false),

            connection_info: RwSignal::new(None),
            organization_info: RwSignal::new(None),
            marketplace_info: RwSignal::new(None),
            source_order_id: RwSignal::new(None),

            active_tab: RwSignal::new("general"),
            loading: RwSignal::new(false),
            posting: RwSignal::new(false),
            error: RwSignal::new(None),
        }
    }

    pub fn is_posted(&self) -> Signal<bool> {
        let data = self.return_data;
        Signal::derive(move || data.get().map(|d| d.is_posted).unwrap_or(false))
    }

    /// Заголовок документа: «YM Возврат {return_id}»
    pub fn title(&self) -> Signal<String> {
        let data = self.return_data;
        Signal::derive(move || {
            data.get()
                .map(|d| format!("YM Возврат {}", d.header.return_id))
                .unwrap_or_else(|| "YM Возврат".to_string())
        })
    }

    pub fn projections_count(&self) -> Signal<usize> {
        let projections = self.projections;
        Signal::derive(move || {
            projections
                .get()
                .as_ref()
                .and_then(|p| p["p904_sales_data"].as_array().map(|a| a.len()))
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

        spawn_local(async move {
            match fetch_by_id(&id).await {
                Ok(data) => {
                    vm.return_data.set(Some(data.clone()));
                    vm.load_related_data(&data);
                    vm.load_projections();
                    vm.loading.set(false);
                }
                Err(e) => {
                    vm.error.set(Some(e));
                    vm.loading.set(false);
                }
            }
        });
    }

    fn load_related_data(&self, data: &YmReturnDetailDto) {
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

        // Резолв исходного заказа по order_id для гиперссылки
        let order_no = data.header.order_id.to_string();
        let src = self.source_order_id;
        src.set(None);
        spawn_local(async move {
            if let Ok(Some(id)) = fetch_source_order_id(&order_no).await {
                src.set(Some(id));
            }
        });
    }

    pub fn load_raw_json(&self) {
        if self.raw_json_loaded.get() || self.raw_json_loading.get() {
            return;
        }
        let Some(data) = self.return_data.get() else {
            return;
        };

        let raw_payload_ref = data.source_meta.raw_payload_ref.clone();
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
                Err(e) => leptos::logging::log!("Failed to load projections: {}", e),
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

    pub fn unpost(&self) {
        let Some(id) = self.id.get() else {
            return;
        };
        let vm = self.clone();
        vm.posting.set(true);

        spawn_local(async move {
            if let Err(e) = unpost_document(&id).await {
                leptos::logging::log!("Error unposting: {}", e);
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
            self.return_data.set(Some(data.clone()));
            self.load_related_data(&data);
        }
        // refresh projections
        self.projections_loaded.set(false);
        self.load_projections();
    }
}

impl Default for YmReturnDetailsVm {
    fn default() -> Self {
        Self::new()
    }
}
