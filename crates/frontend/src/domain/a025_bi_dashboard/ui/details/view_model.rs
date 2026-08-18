//! ViewModel for BiDashboard details form (EditDetails MVVM Standard)

use super::api::{self, BiDashboardSaveDto};
use leptos::prelude::*;

#[derive(Clone)]
pub struct BiDashboardDetailsVm {
    // === Base fields ===
    pub id: RwSignal<Option<String>>,
    pub code: RwSignal<String>,
    pub description: RwSignal<String>,
    pub comment: RwSignal<String>,
    pub status: RwSignal<String>,
    pub owner_user_id: RwSignal<String>,
    pub is_public: RwSignal<bool>,
    pub rating: RwSignal<Option<u8>>,
    pub version: RwSignal<i64>,

    // Layout (whole DashboardLayout as JSON string for the tree editor)
    pub layout_json: RwSignal<String>,

    // Filters (whole Vec<FilterRef> as JSON string)
    pub filters_json: RwSignal<String>,

    // Meta (read-only)
    pub created_at: RwSignal<String>,
    pub updated_at: RwSignal<String>,
    pub created_by: RwSignal<String>,
    pub updated_by: RwSignal<String>,

    // === UI state ===
    pub active_tab: RwSignal<&'static str>,
    pub loading: RwSignal<bool>,
    pub saving: RwSignal<bool>,
    pub error: RwSignal<Option<String>>,
    pub success: RwSignal<Option<String>>,
}

impl BiDashboardDetailsVm {
    pub fn new() -> Self {
        Self {
            id: RwSignal::new(None),
            code: RwSignal::new(String::new()),
            description: RwSignal::new(String::new()),
            comment: RwSignal::new(String::new()),
            status: RwSignal::new("draft".to_string()),
            owner_user_id: RwSignal::new(String::new()),
            is_public: RwSignal::new(false),
            rating: RwSignal::new(None),
            version: RwSignal::new(1),
            layout_json: RwSignal::new(r#"{"groups":[]}"#.to_string()),
            filters_json: RwSignal::new("[]".to_string()),
            created_at: RwSignal::new(String::new()),
            updated_at: RwSignal::new(String::new()),
            created_by: RwSignal::new(String::new()),
            updated_by: RwSignal::new(String::new()),
            active_tab: RwSignal::new("general"),
            loading: RwSignal::new(false),
            saving: RwSignal::new(false),
            error: RwSignal::new(None),
            success: RwSignal::new(None),
        }
    }

    pub fn is_edit_mode(&self) -> Signal<bool> {
        let id = self.id;
        Signal::derive(move || id.get().is_some())
    }

    pub fn is_save_disabled(&self) -> Signal<bool> {
        let saving = self.saving;
        let loading = self.loading;
        Signal::derive(move || saving.get() || loading.get())
    }

    pub fn set_tab(&self, tab: &'static str) {
        self.active_tab.set(tab);
        self.error.set(None);
        self.success.set(None);
    }

    pub fn load(&self, id: String) {
        let vm = self.clone();
        leptos::task::spawn_local(async move {
            vm.loading.set(true);
            vm.error.set(None);
            match api::fetch_by_id(&id).await {
                Ok(raw) => {
                    vm.from_raw(raw);
                }
                Err(e) => vm.error.set(Some(e)),
            }
            vm.loading.set(false);
        });
    }

    pub fn from_raw(&self, raw: serde_json::Value) {
        if let Some(id) = raw["id"].as_str() {
            self.id.set(Some(id.to_string()));
        }
        if let Some(v) = raw["code"].as_str() {
            self.code.set(v.to_string());
        }
        if let Some(v) = raw["description"].as_str() {
            self.description.set(v.to_string());
        }
        if let Some(v) = raw["comment"].as_str() {
            self.comment.set(v.to_string());
        }
        if let Some(v) = raw["status"].as_str() {
            self.status.set(v.to_string());
        }
        if let Some(v) = raw["owner_user_id"].as_str() {
            self.owner_user_id.set(v.to_string());
        }
        if let Some(v) = raw["is_public"].as_bool() {
            self.is_public.set(v);
        }
        if let Some(v) = raw["rating"].as_u64() {
            let r = v as u8;
            self.rating
                .set(if r >= 1 && r <= 5 { Some(r) } else { None });
        } else {
            self.rating.set(None);
        }
        if let Some(v) = raw["version"].as_i64() {
            self.version.set(v);
        }

        // Layout
        if let Some(layout) = raw.get("layout") {
            self.layout_json.set(
                serde_json::to_string_pretty(layout)
                    .unwrap_or_else(|_| r#"{"groups":[]}"#.to_string()),
            );
        }

        // Filters
        if let Some(filters) = raw.get("filters") {
            self.filters_json
                .set(serde_json::to_string_pretty(filters).unwrap_or_else(|_| "[]".to_string()));
        }

        // Meta
        if let Some(meta) = raw.get("metadata") {
            if let Some(v) = meta["created_at"].as_str() {
                self.created_at.set(v.to_string());
            }
            if let Some(v) = meta["updated_at"].as_str() {
                self.updated_at.set(v.to_string());
            }
        }
        if let Some(v) = raw["created_by"].as_str() {
            self.created_by.set(v.to_string());
        }
        if let Some(v) = raw["updated_by"].as_str() {
            self.updated_by.set(v.to_string());
        }
    }

    pub fn to_dto(&self) -> BiDashboardSaveDto {
        let layout: serde_json::Value = serde_json::from_str(&self.layout_json.get_untracked())
            .unwrap_or_else(|_| serde_json::json!({"groups": []}));
        let filters: serde_json::Value = serde_json::from_str(&self.filters_json.get_untracked())
            .unwrap_or_else(|_| serde_json::json!([]));

        let comment = {
            let c = self.comment.get_untracked();
            if c.is_empty() {
                None
            } else {
                Some(c)
            }
        };

        BiDashboardSaveDto {
            id: self.id.get_untracked(),
            code: self.code.get_untracked(),
            description: self.description.get_untracked(),
            comment,
            status: self.status.get_untracked(),
            owner_user_id: self.owner_user_id.get_untracked(),
            is_public: self.is_public.get_untracked(),
            rating: self.rating.get_untracked(),
            version: self.version.get_untracked(),
            layout,
            filters,
        }
    }

    pub fn save(&self, on_saved: Callback<()>) {
        let vm = self.clone();
        leptos::task::spawn_local(async move {
            vm.saving.set(true);
            vm.error.set(None);
            vm.success.set(None);

            let dto = vm.to_dto();
            match api::save_dashboard(dto).await {
                Ok(new_id) => {
                    if vm.id.get_untracked().is_none() {
                        vm.id.set(Some(new_id));
                    }
                    vm.success.set(Some("Сохранено".to_string()));
                    on_saved.run(());
                }
                Err(e) => vm.error.set(Some(e)),
            }
            vm.saving.set(false);
        });
    }
}
