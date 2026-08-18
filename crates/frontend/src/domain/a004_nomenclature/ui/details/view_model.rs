//! ViewModel for Nomenclature details form (EditDetails MVVM Standard)
//!
//! Contains all form fields as individual RwSignals for THAW two-way binding,
//! nested data (barcodes), UI state, and commands.

use super::api::{
    self, DealerPriceDto, DimensionValuesResponse, KitComponentInfo, KitVariantInfo,
    NomenclatureBarcodeDto,
};
use contracts::domain::a004_nomenclature::aggregate::NomenclatureDto;
use contracts::domain::a004_nomenclature::orders_dto::NomenclatureOrderRowDto;
use contracts::domain::common::AggregateId;
use contracts::projections::p912_nomenclature_costs::dto::NomenclatureCostDto;
use leptos::prelude::*;

/// Период по умолчанию для вкладки «Заказы» на карточке номенклатуры.
const DEFAULT_ORDERS_DAYS: u32 = 180;

/// Helper to convert empty strings to None
fn opt(v: String) -> Option<String> {
    if v.trim().is_empty() {
        None
    } else {
        Some(v)
    }
}

/// ViewModel for Nomenclature details form
#[derive(Clone)]
pub struct NomenclatureDetailsVm {
    // === Form fields (individual RwSignals for THAW) ===
    pub id: RwSignal<Option<String>>,
    pub code: RwSignal<String>,
    pub description: RwSignal<String>,
    pub full_description: RwSignal<String>,
    pub article: RwSignal<String>,
    pub comment: RwSignal<String>,
    pub is_folder: RwSignal<bool>,
    pub parent_id: RwSignal<String>,

    // Dimension fields
    pub dim1_category: RwSignal<String>,
    pub dim2_line: RwSignal<String>,
    pub dim3_model: RwSignal<String>,
    pub dim4_format: RwSignal<String>,
    pub dim5_sink: RwSignal<String>,
    pub dim6_size: RwSignal<String>,

    // Derivative nomenclature fields
    pub base_nomenclature_ref: RwSignal<String>,
    pub alternative_cost_source_ref: RwSignal<String>,
    pub is_derivative: RwSignal<bool>,
    pub is_assembly: RwSignal<bool>,
    pub base_nomenclature_name: RwSignal<String>,
    pub base_nomenclature_article: RwSignal<String>,
    pub alternative_cost_source_name: RwSignal<String>,
    pub alternative_cost_source_article: RwSignal<String>,
    pub kit_variant_ref: RwSignal<String>,
    pub kit_variant_info: RwSignal<Option<KitVariantInfo>>,
    pub kit_components: RwSignal<Vec<KitComponentInfo>>,
    pub kit_components_loading: RwSignal<bool>,

    // === Nested data (tables) ===
    pub barcodes: RwSignal<Vec<NomenclatureBarcodeDto>>,
    pub barcodes_count: RwSignal<usize>,
    pub barcodes_loaded: RwSignal<bool>,
    pub barcodes_loading: RwSignal<bool>,

    pub dealer_prices: RwSignal<Vec<DealerPriceDto>>,
    pub dealer_prices_count: RwSignal<usize>,
    pub dealer_prices_loaded: RwSignal<bool>,
    pub dealer_prices_loading: RwSignal<bool>,

    pub production_costs: RwSignal<Vec<NomenclatureCostDto>>,
    pub production_costs_count: RwSignal<usize>,
    pub production_costs_loaded: RwSignal<bool>,
    pub production_costs_loading: RwSignal<bool>,

    pub orders: RwSignal<Vec<NomenclatureOrderRowDto>>,
    pub orders_count: RwSignal<usize>,
    pub orders_loaded: RwSignal<bool>,
    pub orders_loading: RwSignal<bool>,

    // === Reference data (dropdown options) ===
    pub dimension_options: RwSignal<Option<DimensionValuesResponse>>,

    // === UI State ===
    pub active_tab: RwSignal<&'static str>,
    pub loading: RwSignal<bool>,
    pub saving: RwSignal<bool>,
    pub error: RwSignal<Option<String>>,
    pub success: RwSignal<Option<String>>,
}

impl NomenclatureDetailsVm {
    /// Create a new ViewModel instance
    pub fn new() -> Self {
        Self {
            // Form fields
            id: RwSignal::new(None),
            code: RwSignal::new(String::new()),
            description: RwSignal::new(String::new()),
            full_description: RwSignal::new(String::new()),
            article: RwSignal::new(String::new()),
            comment: RwSignal::new(String::new()),
            is_folder: RwSignal::new(false),
            parent_id: RwSignal::new(String::new()),

            // Dimensions
            dim1_category: RwSignal::new(String::new()),
            dim2_line: RwSignal::new(String::new()),
            dim3_model: RwSignal::new(String::new()),
            dim4_format: RwSignal::new(String::new()),
            dim5_sink: RwSignal::new(String::new()),
            dim6_size: RwSignal::new(String::new()),

            // Derivative nomenclature
            base_nomenclature_ref: RwSignal::new(String::new()),
            alternative_cost_source_ref: RwSignal::new(String::new()),
            is_derivative: RwSignal::new(false),
            is_assembly: RwSignal::new(false),
            base_nomenclature_name: RwSignal::new(String::new()),
            base_nomenclature_article: RwSignal::new(String::new()),
            alternative_cost_source_name: RwSignal::new(String::new()),
            alternative_cost_source_article: RwSignal::new(String::new()),
            kit_variant_ref: RwSignal::new(String::new()),
            kit_variant_info: RwSignal::new(None),
            kit_components: RwSignal::new(Vec::new()),
            kit_components_loading: RwSignal::new(false),

            // Nested data
            barcodes: RwSignal::new(Vec::new()),
            barcodes_count: RwSignal::new(0),
            barcodes_loaded: RwSignal::new(false),
            barcodes_loading: RwSignal::new(false),

            dealer_prices: RwSignal::new(Vec::new()),
            dealer_prices_count: RwSignal::new(0),
            dealer_prices_loaded: RwSignal::new(false),
            dealer_prices_loading: RwSignal::new(false),

            production_costs: RwSignal::new(Vec::new()),
            production_costs_count: RwSignal::new(0),
            production_costs_loaded: RwSignal::new(false),
            production_costs_loading: RwSignal::new(false),

            orders: RwSignal::new(Vec::new()),
            orders_count: RwSignal::new(0),
            orders_loaded: RwSignal::new(false),
            orders_loading: RwSignal::new(false),

            // Reference data
            dimension_options: RwSignal::new(None),

            // UI state
            active_tab: RwSignal::new("general"),
            loading: RwSignal::new(false),
            saving: RwSignal::new(false),
            error: RwSignal::new(None),
            success: RwSignal::new(None),
        }
    }

    // === Derived signals ===

    /// Check if in edit mode (has ID)
    pub fn is_edit_mode(&self) -> Signal<bool> {
        let id = self.id;
        Signal::derive(move || id.get().is_some())
    }

    /// Check if form is valid
    pub fn is_valid(&self) -> Signal<bool> {
        let description = self.description;
        Signal::derive(move || !description.get().trim().is_empty())
    }

    /// Check if save button should be disabled
    pub fn is_save_disabled(&self) -> Signal<bool> {
        let saving = self.saving;
        let is_valid = self.is_valid();
        Signal::derive(move || saving.get() || !is_valid.get())
    }

    // === Validation ===

    /// Validate form and return error message if invalid
    pub fn validate(&self) -> Result<(), String> {
        if self.description.get().trim().is_empty() {
            return Err("Наименование обязательно для заполнения".into());
        }
        Ok(())
    }

    // === Data loading ===

    /// Load dimension options (should be called on mount)
    pub fn load_dimension_options(&self) {
        let dimension_options = self.dimension_options;
        leptos::task::spawn_local(async move {
            match api::fetch_dimension_values().await {
                Ok(data) => dimension_options.set(Some(data)),
                Err(_) => dimension_options.set(None),
            }
        });
    }

    /// Load entity data by ID
    pub fn load(&self, id: String) {
        let this = self.clone();
        this.loading.set(true);
        this.error.set(None);
        this.id.set(Some(id.clone()));

        leptos::task::spawn_local(async move {
            match api::fetch_by_id(id.clone()).await {
                Ok(item) => {
                    this.from_aggregate(&item);
                    this.loading.set(false);
                    // Load counts for badges immediately
                    this.load_counts();
                }
                Err(e) => {
                    this.error.set(Some(e));
                    this.loading.set(false);
                }
            }
        });
    }

    /// Load counts for badges (barcodes and dealer prices)
    pub fn load_counts(&self) {
        let Some(nom_id) = self.id.get_untracked() else {
            return;
        };

        // Load barcodes count
        let this_barcodes = self.clone();
        let nom_id_barcodes = nom_id.clone();
        leptos::task::spawn_local(async move {
            match api::fetch_barcodes_count(nom_id_barcodes).await {
                Ok(count) => this_barcodes.barcodes_count.set(count),
                Err(_) => this_barcodes.barcodes_count.set(0),
            }
        });

        // Load dealer prices count
        let this_prices = self.clone();
        let nom_id_prices = nom_id.clone();
        let base_ref = this_prices.base_nomenclature_ref.get_untracked();
        let base_ref_option = if base_ref.is_empty() {
            None
        } else {
            Some(base_ref)
        };
        leptos::task::spawn_local(async move {
            match api::fetch_dealer_prices_count(nom_id_prices, base_ref_option).await {
                Ok(count) => this_prices.dealer_prices_count.set(count),
                Err(_) => this_prices.dealer_prices_count.set(0),
            }
        });

        let this_production = self.clone();
        let nom_id_production = nom_id.clone();
        leptos::task::spawn_local(async move {
            match api::fetch_production_costs(&nom_id_production).await {
                Ok(items) => this_production.production_costs_count.set(items.len()),
                Err(_) => this_production.production_costs_count.set(0),
            }
        });

        // Load orders count
        let this_orders = self.clone();
        leptos::task::spawn_local(async move {
            match api::fetch_nomenclature_orders(&nom_id, DEFAULT_ORDERS_DAYS).await {
                Ok(items) => this_orders.orders_count.set(items.len()),
                Err(_) => this_orders.orders_count.set(0),
            }
        });
    }

    /// Load barcodes (lazy, called when barcodes tab is activated)
    pub fn load_barcodes(&self) {
        let Some(nom_id) = self.id.get() else {
            return;
        };

        if self.barcodes_loaded.get() {
            return;
        }

        let this = self.clone();
        this.barcodes_loading.set(true);

        leptos::task::spawn_local(async move {
            match api::fetch_barcodes_by_nomenclature(nom_id).await {
                Ok(data) => {
                    this.barcodes.set(data.barcodes);
                    this.barcodes_count.set(data.total_count);
                    this.barcodes_loaded.set(true);
                    this.barcodes_loading.set(false);
                }
                Err(_) => {
                    this.barcodes.set(Vec::new());
                    this.barcodes_count.set(0);
                    this.barcodes_loaded.set(true);
                    this.barcodes_loading.set(false);
                }
            }
        });
    }

    /// Load dealer prices (lazy, called when dealer prices tab is activated)
    pub fn load_dealer_prices(&self) {
        let Some(nom_id) = self.id.get() else {
            return;
        };

        if self.dealer_prices_loaded.get() {
            return;
        }

        let this = self.clone();
        this.dealer_prices_loading.set(true);

        let base_ref = this.base_nomenclature_ref.get();
        let base_ref_option = if base_ref.is_empty() {
            None
        } else {
            Some(base_ref)
        };

        leptos::task::spawn_local(async move {
            match api::fetch_dealer_prices_by_nomenclature(nom_id, base_ref_option).await {
                Ok(data) => {
                    this.dealer_prices_count.set(data.len());
                    this.dealer_prices.set(data);
                    this.dealer_prices_loaded.set(true);
                    this.dealer_prices_loading.set(false);
                }
                Err(_) => {
                    this.dealer_prices.set(Vec::new());
                    this.dealer_prices_count.set(0);
                    this.dealer_prices_loaded.set(true);
                    this.dealer_prices_loading.set(false);
                }
            }
        });
    }

    /// Load production costs (lazy, called when production tab is activated)
    pub fn load_production_costs(&self) {
        let Some(nom_id) = self.id.get() else {
            return;
        };

        if self.production_costs_loaded.get() {
            return;
        }

        let this = self.clone();
        this.production_costs_loading.set(true);

        leptos::task::spawn_local(async move {
            match api::fetch_production_costs(&nom_id).await {
                Ok(items) => {
                    this.production_costs_count.set(items.len());
                    this.production_costs.set(items);
                    this.production_costs_loaded.set(true);
                    this.production_costs_loading.set(false);
                }
                Err(_) => {
                    this.production_costs.set(Vec::new());
                    this.production_costs_count.set(0);
                    this.production_costs_loaded.set(true);
                    this.production_costs_loading.set(false);
                }
            }
        });
    }

    /// Load related marketplace orders (lazy, called when orders tab is activated)
    pub fn load_orders(&self) {
        let Some(nom_id) = self.id.get() else {
            return;
        };

        if self.orders_loaded.get() {
            return;
        }

        let this = self.clone();
        this.orders_loading.set(true);

        leptos::task::spawn_local(async move {
            match api::fetch_nomenclature_orders(&nom_id, DEFAULT_ORDERS_DAYS).await {
                Ok(items) => {
                    this.orders_count.set(items.len());
                    this.orders.set(items);
                    this.orders_loaded.set(true);
                    this.orders_loading.set(false);
                }
                Err(_) => {
                    this.orders.set(Vec::new());
                    this.orders_count.set(0);
                    this.orders_loaded.set(true);
                    this.orders_loading.set(false);
                }
            }
        });
    }

    // === Commands ===

    /// Save the form
    pub fn save(&self, on_saved: Callback<()>) {
        if let Err(msg) = self.validate() {
            self.error.set(Some(msg));
            return;
        }

        let this = self.clone();
        this.saving.set(true);
        this.error.set(None);

        let dto = this.to_dto();

        leptos::task::spawn_local(async move {
            match api::save_form(dto).await {
                Ok(new_id) => {
                    // Update ID if it was a new record
                    if this.id.get().is_none() {
                        this.id.set(Some(new_id));
                    }
                    this.saving.set(false);
                    this.success.set(Some("Сохранено успешно".into()));
                    on_saved.run(());
                }
                Err(e) => {
                    this.saving.set(false);
                    this.error.set(Some(e));
                }
            }
        });
    }

    /// Reset form to initial state
    pub fn reset(&self) {
        self.id.set(None);
        self.code.set(String::new());
        self.description.set(String::new());
        self.full_description.set(String::new());
        self.article.set(String::new());
        self.comment.set(String::new());
        self.is_folder.set(false);
        self.parent_id.set(String::new());

        self.dim1_category.set(String::new());
        self.dim2_line.set(String::new());
        self.dim3_model.set(String::new());
        self.dim4_format.set(String::new());
        self.dim5_sink.set(String::new());
        self.dim6_size.set(String::new());

        self.base_nomenclature_ref.set(String::new());
        self.alternative_cost_source_ref.set(String::new());
        self.is_derivative.set(false);
        self.is_assembly.set(false);
        self.base_nomenclature_name.set(String::new());
        self.base_nomenclature_article.set(String::new());
        self.alternative_cost_source_name.set(String::new());
        self.alternative_cost_source_article.set(String::new());
        self.kit_variant_ref.set(String::new());
        self.kit_variant_info.set(None);
        self.kit_components.set(Vec::new());
        self.kit_components_loading.set(false);

        self.barcodes.set(Vec::new());
        self.barcodes_count.set(0);
        self.barcodes_loaded.set(false);

        self.dealer_prices.set(Vec::new());
        self.dealer_prices_count.set(0);
        self.dealer_prices_loaded.set(false);
        self.dealer_prices_loading.set(false);

        self.production_costs.set(Vec::new());
        self.production_costs_count.set(0);
        self.production_costs_loaded.set(false);
        self.production_costs_loading.set(false);

        self.orders.set(Vec::new());
        self.orders_count.set(0);
        self.orders_loaded.set(false);
        self.orders_loading.set(false);

        self.active_tab.set("general");
        self.error.set(None);
        self.success.set(None);
    }

    // === Tab helpers ===

    /// Set active tab
    pub fn set_tab(&self, tab: &'static str) {
        self.active_tab.set(tab);
    }

    /// Get dimension options for a specific dimension
    pub fn get_dim_options(&self, dim: &str) -> Signal<Vec<String>> {
        let dimension_options = self.dimension_options;
        let dim = dim.to_string();
        Signal::derive(move || {
            dimension_options
                .get()
                .map(|d| match dim.as_str() {
                    "dim1_category" => d.dim1_category,
                    "dim2_line" => d.dim2_line,
                    "dim3_model" => d.dim3_model,
                    "dim4_format" => d.dim4_format,
                    "dim5_sink" => d.dim5_sink,
                    "dim6_size" => d.dim6_size,
                    _ => Vec::new(),
                })
                .unwrap_or_default()
        })
    }

    // === Private helpers ===

    /// Convert ViewModel to DTO for saving
    fn to_dto(&self) -> NomenclatureDto {
        NomenclatureDto {
            id: self.id.get(),
            code: opt(self.code.get()),
            description: self.description.get(),
            full_description: opt(self.full_description.get()),
            is_folder: self.is_folder.get(),
            parent_id: opt(self.parent_id.get()),
            article: opt(self.article.get()),
            comment: opt(self.comment.get()),
            updated_at: None,
            mp_ref_count: 0,
            dim1_category: opt(self.dim1_category.get()),
            dim2_line: opt(self.dim2_line.get()),
            dim3_model: opt(self.dim3_model.get()),
            dim4_format: opt(self.dim4_format.get()),
            dim5_sink: opt(self.dim5_sink.get()),
            dim6_size: opt(self.dim6_size.get()),
            is_assembly: None,
            base_nomenclature_ref: opt(self.base_nomenclature_ref.get()),
            alternative_cost_source_ref: opt(self.alternative_cost_source_ref.get()),
            kit_variant_ref: None,
            is_derivative: None, // Вычисляется автоматически на backend
        }
    }

    /// Populate ViewModel from loaded aggregate
    fn from_aggregate(&self, item: &contracts::domain::a004_nomenclature::aggregate::Nomenclature) {
        self.id.set(Some(item.base.id.as_string()));
        self.code.set(item.base.code.clone());
        self.description.set(item.base.description.clone());
        self.full_description.set(item.full_description.clone());
        self.article.set(item.article.clone());
        self.comment
            .set(item.base.comment.clone().unwrap_or_default());
        self.is_folder.set(item.is_folder);
        self.parent_id
            .set(item.parent_id.clone().unwrap_or_default());

        self.dim1_category.set(item.dim1_category.clone());
        self.dim2_line.set(item.dim2_line.clone());
        self.dim3_model.set(item.dim3_model.clone());
        self.dim4_format.set(item.dim4_format.clone());
        self.dim5_sink.set(item.dim5_sink.clone());
        self.dim6_size.set(item.dim6_size.clone());

        // Derivative nomenclature fields
        self.base_nomenclature_ref
            .set(item.base_nomenclature_ref.clone().unwrap_or_default());
        self.alternative_cost_source_ref
            .set(item.alternative_cost_source_ref.clone().unwrap_or_default());
        self.is_derivative.set(item.is_derivative);
        self.is_assembly.set(item.is_assembly);
        self.kit_variant_ref
            .set(item.kit_variant_ref.clone().unwrap_or_default());

        // Load base nomenclature info if derivative
        if item.is_derivative {
            if let Some(ref base_ref) = item.base_nomenclature_ref {
                let this = self.clone();
                let base_ref_clone = base_ref.clone();
                leptos::task::spawn_local(async move {
                    match api::fetch_base_nomenclature_info(&base_ref_clone).await {
                        Ok(info) => {
                            this.base_nomenclature_name.set(info.name);
                            this.base_nomenclature_article.set(info.article);
                        }
                        Err(_) => {
                            this.base_nomenclature_name
                                .set(format!("[{}]", base_ref_clone));
                            this.base_nomenclature_article.set(String::new());
                        }
                    }
                });
            }
        } else {
            self.base_nomenclature_name.set(String::new());
            self.base_nomenclature_article.set(String::new());
        }

        if let Some(ref alternative_ref) = item.alternative_cost_source_ref {
            let this = self.clone();
            let alternative_ref_clone = alternative_ref.clone();
            leptos::task::spawn_local(async move {
                match api::fetch_base_nomenclature_info(&alternative_ref_clone).await {
                    Ok(info) => {
                        this.alternative_cost_source_name.set(info.name);
                        this.alternative_cost_source_article.set(info.article);
                    }
                    Err(_) => {
                        this.alternative_cost_source_name
                            .set(format!("[{}]", alternative_ref_clone));
                        this.alternative_cost_source_article.set(String::new());
                    }
                }
            });
        } else {
            self.alternative_cost_source_name.set(String::new());
            self.alternative_cost_source_article.set(String::new());
        }

        if let Some(ref kit_variant_ref) = item.kit_variant_ref {
            let this = self.clone();
            let kit_variant_ref_clone = kit_variant_ref.clone();
            self.kit_components_loading.set(true);
            leptos::task::spawn_local(async move {
                match api::fetch_kit_variant_info(&kit_variant_ref_clone).await {
                    Ok(info) => this.kit_variant_info.set(Some(info)),
                    Err(_) => this.kit_variant_info.set(None),
                }

                match api::fetch_kit_components(&kit_variant_ref_clone).await {
                    Ok(items) => this.kit_components.set(items),
                    Err(_) => this.kit_components.set(Vec::new()),
                }
                this.kit_components_loading.set(false);
            });
        } else {
            self.kit_variant_info.set(None);
            self.kit_components.set(Vec::new());
            self.kit_components_loading.set(false);
        }
    }
}

impl Default for NomenclatureDetailsVm {
    fn default() -> Self {
        Self::new()
    }
}
