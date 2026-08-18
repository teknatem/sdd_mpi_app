//! ViewModel for WB Sales Funnel Daily details
//!
//! Contains reactive state and commands.

use super::api::*;
use crate::layout::global_context::AppGlobalContext;
use crate::shared::list_utils::sort_list;
use leptos::prelude::*;
use leptos::task::spawn_local;

/// ViewModel for WB Sales Funnel Daily details form
#[derive(Clone)]
pub struct WbSalesFunnelDailyDetailsVm {
    /// Tabs / navigation context (open related documents, update tab title).
    pub tabs: AppGlobalContext,

    // === Entity ID ===
    pub id: RwSignal<Option<String>>,

    // === Main data (loaded from API) ===
    pub doc: RwSignal<Option<DetailsDto>>,

    // === Projections (p916 funnel movements) ===
    pub projections: RwSignal<Option<serde_json::Value>>,
    pub projections_loaded: RwSignal<bool>,
    pub projections_loading: RwSignal<bool>,

    // === UI State ===
    pub active_tab: RwSignal<&'static str>,
    pub loading: RwSignal<bool>,
    pub posting: RwSignal<bool>,
    pub error: RwSignal<Option<String>>,

    // === Tab-local UI state ===
    pub lines_sort_field: RwSignal<String>,
    pub lines_sort_ascending: RwSignal<bool>,
}

impl WbSalesFunnelDailyDetailsVm {
    /// Create a new ViewModel instance
    pub fn new(tabs: AppGlobalContext) -> Self {
        Self {
            tabs,
            id: RwSignal::new(None),
            doc: RwSignal::new(None),

            projections: RwSignal::new(None),
            projections_loaded: RwSignal::new(false),
            projections_loading: RwSignal::new(false),

            active_tab: RwSignal::new("general"),
            loading: RwSignal::new(true),
            posting: RwSignal::new(false),
            error: RwSignal::new(None),

            lines_sort_field: RwSignal::new("title".to_string()),
            lines_sort_ascending: RwSignal::new(true),
        }
    }

    // ── Derived signals ─────────────────────────────────────────────────────

    /// Header title (`h1`): "Воронка WB {document_no} от {date}".
    pub fn header_title(&self) -> Signal<String> {
        let doc = self.doc;
        Signal::derive(move || {
            doc.get()
                .map(|d| {
                    format!(
                        "Воронка WB {} от {}",
                        d.document_no,
                        fmt_date(&d.document_date)
                    )
                })
                .unwrap_or_else(|| "Воронка WB".to_string())
        })
    }

    /// Tab / favorite label: "Воронка WB {document_date}".
    pub fn tab_label(&self) -> Signal<String> {
        let doc = self.doc;
        Signal::derive(move || {
            doc.get()
                .map(|d| format!("Воронка WB {}", d.document_date))
                .unwrap_or_else(|| "Воронка WB".to_string())
        })
    }

    /// Lines sorted by the current sort field/direction.
    pub fn sorted_lines(&self) -> Signal<Vec<LineDto>> {
        let doc = self.doc;
        let field = self.lines_sort_field;
        let ascending = self.lines_sort_ascending;
        Signal::derive(move || {
            let mut lines = doc.get().map(|d| d.lines).unwrap_or_default();
            sort_list(&mut lines, &field.get(), ascending.get());
            lines
        })
    }

    pub fn set_tab(&self, tab: &'static str) {
        self.active_tab.set(tab);
    }

    pub fn toggle_lines_sort(&self, field: &'static str) {
        if self.lines_sort_field.get_untracked() == field {
            self.lines_sort_ascending.update(|value| *value = !*value);
        } else {
            self.lines_sort_field.set(field.to_string());
            self.lines_sort_ascending.set(true);
        }
    }

    // ── Loaders ──────────────────────────────────────────────────────────────

    /// Load main document data
    pub fn load(&self, id: String) {
        let vm = self.clone();
        vm.id.set(Some(id.clone()));
        vm.loading.set(true);
        vm.error.set(None);

        spawn_local(async move {
            match fetch_by_id(&id).await {
                Ok(data) => {
                    let tab_id = id.clone();
                    let title = format!("Воронка WB {}", data.document_date);
                    vm.tabs.update_tab_title(
                        &format!("a036_wb_sales_funnel_daily_details_{tab_id}"),
                        &title,
                    );
                    vm.doc.set(Some(data));
                    vm.loading.set(false);
                }
                Err(e) => {
                    vm.error.set(Some(e));
                    vm.loading.set(false);
                }
            }
        });
    }

    /// Кол-во строк движений p916 для бейджа вкладки «Проекции».
    pub fn projections_count(&self) -> Signal<usize> {
        let projections = self.projections;
        Signal::derive(move || {
            projections
                .get()
                .as_ref()
                .map(|p| {
                    p["p916_mp_sales_funnel_turnovers"]
                        .as_array()
                        .map(|a| a.len())
                        .unwrap_or(0)
                })
                .unwrap_or(0)
        })
    }

    /// Ленивая загрузка движений проекции p916 (при активации вкладки «Проекции»).
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
                Err(e) => leptos::logging::log!("Failed to load a036 projections: {}", e),
            }
            vm.projections_loading.set(false);
        });
    }

    /// Провести документ: пересобрать движения p916, затем обновить закладку «Проекции».
    pub fn post(&self) {
        let Some(id) = self.id.get_untracked() else {
            return;
        };
        let vm = self.clone();
        vm.posting.set(true);

        spawn_local(async move {
            match post_document(&id).await {
                Ok(()) => {
                    // Проекции пересобраны — перезагружаем, если уже показывались.
                    if vm.projections_loaded.get_untracked() {
                        vm.projections_loaded.set(false);
                        vm.projections_loading.set(false);
                        vm.load_projections();
                    }
                }
                Err(e) => leptos::logging::log!("Failed to post a036 document: {}", e),
            }
            vm.posting.set(false);
        });
    }

    // ── Navigation ──────────────────────────────────────────────────────────────

    pub fn open_product(&self, product_ref: String) {
        if product_ref.is_empty() {
            return;
        }
        self.tabs.open_tab(
            &format!("a007_marketplace_product_details_{}", product_ref),
            "Товар маркетплейса",
        );
    }
}
