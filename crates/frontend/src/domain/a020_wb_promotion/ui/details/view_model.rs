use super::api::*;
use leptos::prelude::*;
use wasm_bindgen_futures::spawn_local;

#[derive(Clone)]
pub struct WbPromotionDetailsVm {
    pub id: RwSignal<Option<String>>,
    pub promotion: RwSignal<Option<WbPromotionDetailDto>>,

    pub raw_json: RwSignal<Option<String>>,
    pub raw_json_loaded: RwSignal<bool>,
    pub raw_json_loading: RwSignal<bool>,

    pub active_tab: RwSignal<&'static str>,
    pub loading: RwSignal<bool>,
    pub error: RwSignal<Option<String>>,
}

impl WbPromotionDetailsVm {
    pub fn new() -> Self {
        Self {
            id: RwSignal::new(None),
            promotion: RwSignal::new(None),
            raw_json: RwSignal::new(None),
            raw_json_loaded: RwSignal::new(false),
            raw_json_loading: RwSignal::new(false),
            active_tab: RwSignal::new("general"),
            loading: RwSignal::new(false),
            error: RwSignal::new(None),
        }
    }

    pub fn promotion_name(&self) -> Signal<String> {
        let promotion = self.promotion;
        Signal::derive(move || {
            promotion
                .get()
                .map(|p| p.data.name.clone())
                .unwrap_or_default()
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
                    vm.promotion.set(Some(data));
                    vm.loading.set(false);
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
        let Some(promotion) = self.promotion.get() else {
            return;
        };

        let raw_payload_ref = promotion.source_meta.raw_payload_ref.clone();
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
}

impl Default for WbPromotionDetailsVm {
    fn default() -> Self {
        Self::new()
    }
}
