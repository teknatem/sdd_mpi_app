//! ViewModel for WB Advert Daily details
//!
//! Contains reactive state, commands, and lazy loading logic.

use super::api::*;
use crate::layout::global_context::AppGlobalContext;
use crate::shared::list_utils::sort_list;
use contracts::general_ledger::GeneralLedgerEntryDto;
use leptos::prelude::*;
use leptos::task::spawn_local;

/// ViewModel for WB Advert Daily details form
#[derive(Clone)]
pub struct WbAdvertDailyDetailsVm {
    /// Tabs / navigation context (open related documents, update tab title).
    pub tabs: AppGlobalContext,

    // === Entity ID ===
    pub id: RwSignal<Option<String>>,

    // === Main data (loaded from API) ===
    pub doc: RwSignal<Option<DetailsDto>>,

    // === Related data (lazy loaded) ===
    pub projections: RwSignal<Option<serde_json::Value>>,
    pub projections_loaded: RwSignal<bool>,
    pub projections_loading: RwSignal<bool>,

    pub general_ledger_entries: RwSignal<Vec<GeneralLedgerEntryDto>>,
    pub general_ledger_entries_loaded: RwSignal<bool>,
    pub general_ledger_entries_loading: RwSignal<bool>,
    pub general_ledger_entries_error: RwSignal<Option<String>>,

    // === UI State ===
    pub active_tab: RwSignal<&'static str>,
    pub loading: RwSignal<bool>,
    pub posting: RwSignal<bool>,
    pub error: RwSignal<Option<String>>,

    // === Tab-local UI state ===
    pub lines_sort_field: RwSignal<String>,
    pub lines_sort_ascending: RwSignal<bool>,
    pub linked_orders_tree_expanded: RwSignal<bool>,
}

impl WbAdvertDailyDetailsVm {
    /// Create a new ViewModel instance
    pub fn new(tabs: AppGlobalContext) -> Self {
        Self {
            tabs,
            id: RwSignal::new(None),
            doc: RwSignal::new(None),

            projections: RwSignal::new(None),
            projections_loaded: RwSignal::new(false),
            projections_loading: RwSignal::new(false),

            general_ledger_entries: RwSignal::new(Vec::new()),
            general_ledger_entries_loaded: RwSignal::new(false),
            general_ledger_entries_loading: RwSignal::new(false),
            general_ledger_entries_error: RwSignal::new(None),

            active_tab: RwSignal::new("general"),
            loading: RwSignal::new(true),
            posting: RwSignal::new(false),
            error: RwSignal::new(None),

            lines_sort_field: RwSignal::new("wb_name".to_string()),
            lines_sort_ascending: RwSignal::new(true),
            linked_orders_tree_expanded: RwSignal::new(true),
        }
    }

    // ── Derived signals ─────────────────────────────────────────────────────

    /// Check if document is posted
    pub fn is_posted(&self) -> Signal<bool> {
        let doc = self.doc;
        Signal::derive(move || doc.get().map(|d| d.is_posted).unwrap_or(false))
    }

    /// Header title (`h1`): "WB Ads {document_no} · {advert_id} от {date}".
    pub fn header_title(&self) -> Signal<String> {
        let doc = self.doc;
        Signal::derive(move || {
            doc.get()
                .map(|d| {
                    if d.advert_id > 0 {
                        format!(
                            "WB Ads {} · {} от {}",
                            d.document_no,
                            d.advert_id,
                            fmt_date(&d.document_date)
                        )
                    } else {
                        format!("WB Ads {} от {}", d.document_no, fmt_date(&d.document_date))
                    }
                })
                .unwrap_or_else(|| "WB Ads".to_string())
        })
    }

    /// Tab / favorite label: "WB Ads {document_date} · {advert_id}".
    pub fn tab_label(&self) -> Signal<String> {
        let doc = self.doc;
        Signal::derive(move || {
            doc.get()
                .map(|d| {
                    if d.advert_id > 0 {
                        format!("WB Ads {} · {}", d.document_date, d.advert_id)
                    } else {
                        format!("WB Ads {}", d.document_date)
                    }
                })
                .unwrap_or_else(|| "WB Ads".to_string())
        })
    }

    /// Total projection rows (p913 + p911 + p916) for the tab badge.
    pub fn projections_count(&self) -> Signal<usize> {
        let projections = self.projections;
        Signal::derive(move || {
            projections
                .get()
                .as_ref()
                .map(|p| {
                    let p913 = p["p913_wb_advert_order_attr"]
                        .as_array()
                        .map(|a| a.len())
                        .unwrap_or(0);
                    let p911 = p["p911_wb_advert_by_items"]
                        .as_array()
                        .map(|a| a.len())
                        .unwrap_or(0);
                    let p916 = p["p916_mp_sales_funnel_turnovers"]
                        .as_array()
                        .map(|a| a.len())
                        .unwrap_or(0);
                    p913 + p911 + p916
                })
                .unwrap_or(0)
        })
    }

    /// Journal entries count for the tab badge.
    pub fn general_ledger_entries_count(&self) -> Signal<usize> {
        let entries = self.general_ledger_entries;
        Signal::derive(move || entries.get().len())
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
                    let title = if data.advert_id > 0 {
                        format!("WB Ads {} · {}", data.document_date, data.advert_id)
                    } else {
                        format!("WB Ads {}", data.document_date)
                    };
                    vm.tabs.update_tab_title(
                        &format!("a026_wb_advert_daily_details_{tab_id}"),
                        &title,
                    );
                    vm.doc.set(Some(data));

                    // Eager-load badge data (projections + journal). Guards prevent
                    // duplicate loads when the tab is later opened.
                    vm.load_projections();
                    vm.load_general_ledger_entries();

                    vm.loading.set(false);
                }
                Err(e) => {
                    vm.error.set(Some(e));
                    vm.loading.set(false);
                }
            }
        });
    }

    /// Load projections (lazy, for "projections" tab and badge)
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

    /// Load journal entries (lazy, for "journal" tab and badge)
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

    // ── Commands ──────────────────────────────────────────────────────────────

    /// Post document (проведение). Idempotent: re-posting a posted document
    /// rebuilds projections ("Перепровести").
    pub fn post(&self) {
        let Some(id) = self.id.get() else {
            return;
        };
        let vm = self.clone();
        vm.posting.set(true);

        spawn_local(async move {
            match post_document(&id).await {
                Ok(()) => vm.reload().await,
                Err(e) => leptos::logging::log!("Error posting: {}", e),
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
                Ok(()) => vm.reload().await,
                Err(e) => leptos::logging::log!("Error unposting: {}", e),
            }
            vm.posting.set(false);
        });
    }

    /// Reload document, journal and projections after post/unpost.
    async fn reload(&self) {
        let Some(id) = self.id.get_untracked() else {
            return;
        };

        if let Ok(data) = fetch_by_id(&id).await {
            self.doc.set(Some(data));
        }

        // Reset journal so it reloads with fresh state.
        self.general_ledger_entries_loaded.set(false);
        self.general_ledger_entries_loading.set(false);
        self.general_ledger_entries.set(Vec::new());
        self.general_ledger_entries_error.set(None);
        self.load_general_ledger_entries();

        // Reset projections so they reload with fresh state.
        self.projections_loaded.set(false);
        self.projections_loading.set(false);
        self.projections.set(None);
        self.load_projections();
    }

    // ── Navigation ──────────────────────────────────────────────────────────────

    pub fn open_order(&self, order_id: String) {
        if order_id.is_empty() {
            return;
        }
        self.tabs
            .open_tab(&format!("a015_wb_orders_details_{}", order_id), "WB Order");
    }

    pub fn open_nomenclature(&self, nom_ref: String) {
        if nom_ref.is_empty() {
            return;
        }
        self.tabs.open_tab(
            &format!("a004_nomenclature_details_{}", nom_ref),
            "Номенклатура",
        );
    }

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
