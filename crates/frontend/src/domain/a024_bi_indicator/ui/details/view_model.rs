//! ViewModel for BiIndicator details form (EditDetails MVVM Standard)

use super::model::{self, BiIndicatorSaveDto, ComputedIndicatorValue};
use crate::shared::bi_card::{
    default_design_name, is_known_design, render_srcdoc, IndicatorCardParams,
};
use crate::shared::code_format;
use crate::shared::indicator_format::format_money_with_format_spec;
use contracts::shared::data_view::ViewContext;
use leptos::prelude::*;

pub const GL_TURNOVER_DATA_VIEW_ID: &str = "dv004_general_ledger_turnovers";

/// Read the current app theme from localStorage (key "app_theme")
/// and collapse it to its base ("dark" | "light") via the theme registry.
fn get_app_theme() -> String {
    #[cfg(target_arch = "wasm32")]
    {
        let theme = web_sys::window()
            .and_then(|w| w.local_storage().ok().flatten())
            .and_then(|s| s.get_item("app_theme").ok().flatten())
            .unwrap_or_default();
        crate::shared::theme::registry::theme_by_id(&theme)
            .base
            .as_str()
            .to_string()
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        "dark".to_string()
    }
}

fn data_spec_json_value(view_id: &str, metric_id: &str) -> serde_json::Value {
    serde_json::json!({
        "view_id": if view_id.trim().is_empty() {
            serde_json::Value::Null
        } else {
            serde_json::Value::String(view_id.to_string())
        },
        "metric_id": if metric_id.trim().is_empty() {
            serde_json::Value::Null
        } else {
            serde_json::Value::String(metric_id.to_string())
        },
    })
}

fn default_value_format_value() -> serde_json::Value {
    serde_json::json!({
        "kind": "Number",
        "decimals": 2
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ValueFormatPreset {
    pub key: &'static str,
    pub label: &'static str,
}

const VALUE_FORMAT_PRESETS: [ValueFormatPreset; 9] = [
    ValueFormatPreset {
        key: "money_rub",
        label: "Сумма, RUB",
    },
    ValueFormatPreset {
        key: "money_thousand_rub",
        label: "Сумма, т. руб",
    },
    ValueFormatPreset {
        key: "money_million_rub",
        label: "Сумма, м. руб",
    },
    ValueFormatPreset {
        key: "money_usd",
        label: "Сумма, USD",
    },
    ValueFormatPreset {
        key: "integer",
        label: "Целое число",
    },
    ValueFormatPreset {
        key: "number_2",
        label: "Число, 2 знака",
    },
    ValueFormatPreset {
        key: "number_4",
        label: "Число, 4 знака",
    },
    ValueFormatPreset {
        key: "percent_1",
        label: "Процент, 1 знак",
    },
    ValueFormatPreset {
        key: "custom",
        label: "Свой JSON",
    },
];

pub fn value_format_presets() -> &'static [ValueFormatPreset] {
    &VALUE_FORMAT_PRESETS
}

fn value_format_from_preset_key(key: &str) -> Option<serde_json::Value> {
    match key {
        "money_rub" => Some(serde_json::json!({
            "kind": "Money",
            "currency": "RUB"
        })),
        "money_thousand_rub" => Some(serde_json::json!({
            "kind": "Money",
            "currency": "RUB",
            "scale": "thousand",
            "decimals": 0
        })),
        "money_million_rub" => Some(serde_json::json!({
            "kind": "Money",
            "currency": "RUB",
            "scale": "million",
            "decimals": 2
        })),
        "money_usd" => Some(serde_json::json!({
            "kind": "Money",
            "currency": "USD"
        })),
        "integer" => Some(serde_json::json!({
            "kind": "Integer"
        })),
        "number_2" => Some(serde_json::json!({
            "kind": "Number",
            "decimals": 2
        })),
        "number_4" => Some(serde_json::json!({
            "kind": "Number",
            "decimals": 4
        })),
        "percent_1" => Some(serde_json::json!({
            "kind": "Percent",
            "decimals": 1
        })),
        _ => None,
    }
}

fn preset_key_from_value_format(value: serde_json::Value) -> &'static str {
    let normalized = normalized_value_format(value);
    let kind = normalized["kind"].as_str().unwrap_or("");
    match kind {
        "Money" => match (
            normalized["currency"].as_str().unwrap_or("RUB"),
            normalized["scale"].as_str().unwrap_or("unit"),
        ) {
            ("RUB", "thousand") => "money_thousand_rub",
            ("RUB", "million") => "money_million_rub",
            ("RUB", _) => "money_rub",
            ("USD", _) => "money_usd",
            _ => "custom",
        },
        "Integer" => "integer",
        "Percent" => match normalized["decimals"].as_u64().unwrap_or(1) {
            1 => "percent_1",
            _ => "custom",
        },
        "Number" => match normalized["decimals"].as_u64().unwrap_or(2) {
            2 => "number_2",
            4 => "number_4",
            _ => "custom",
        },
        _ => "custom",
    }
}

fn default_test_ctx() -> ViewContext {
    let date_from = {
        #[cfg(target_arch = "wasm32")]
        {
            use js_sys::Date;
            let d = Date::new_0();
            format!("{}-{:02}-01", d.get_full_year(), d.get_month() + 1)
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            "2026-03-01".to_string()
        }
    };

    let date_to = {
        #[cfg(target_arch = "wasm32")]
        {
            use js_sys::Date;
            let d = Date::new_0();
            format!(
                "{}-{:02}-{:02}",
                d.get_full_year(),
                d.get_month() + 1,
                d.get_date()
            )
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            "2026-03-05".to_string()
        }
    };

    let period2_from = {
        #[cfg(target_arch = "wasm32")]
        {
            use js_sys::Date;
            let d = Date::new_0();
            let y = d.get_full_year();
            let m = d.get_month();
            if m == 0 {
                Some(format!("{}-12-01", y - 1))
            } else {
                Some(format!("{}-{:02}-01", y, m))
            }
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            Some("2026-02-01".to_string())
        }
    };

    let period2_to = {
        #[cfg(target_arch = "wasm32")]
        {
            use js_sys::Date;
            let d = Date::new_0();
            let y = d.get_full_year();
            let m = d.get_month();
            let last_day = if m == 0 {
                Date::new_with_year_month_day(y as u32 - 1, 12, 0)
            } else {
                Date::new_with_year_month_day(y as u32, m as i32, 0)
            };
            Some(format!(
                "{}-{:02}-{:02}",
                last_day.get_full_year(),
                last_day.get_month() + 1,
                last_day.get_date()
            ))
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            Some("2026-02-28".to_string())
        }
    };

    ViewContext {
        date_from,
        date_to,
        period2_from,
        period2_to,
        connection_mp_refs: vec![],
        params: std::collections::HashMap::new(),
    }
}

fn normalized_value_format(value: serde_json::Value) -> serde_json::Value {
    let Some(obj) = value.as_object() else {
        return default_value_format_value();
    };

    let kind = obj.get("kind").and_then(|v| v.as_str()).unwrap_or("");
    match kind {
        "Money" => {
            let currency = obj
                .get("currency")
                .and_then(|v| v.as_str())
                .filter(|s| !s.trim().is_empty())
                .unwrap_or("RUB");
            let scale = obj.get("scale").and_then(|v| v.as_str()).unwrap_or("unit");
            match scale {
                "thousand" => {
                    let decimals = obj.get("decimals").and_then(|v| v.as_u64()).unwrap_or(0) as u8;
                    serde_json::json!({
                        "kind": "Money",
                        "currency": currency,
                        "scale": "thousand",
                        "decimals": decimals
                    })
                }
                "million" => {
                    let decimals = obj.get("decimals").and_then(|v| v.as_u64()).unwrap_or(2) as u8;
                    serde_json::json!({
                        "kind": "Money",
                        "currency": currency,
                        "scale": "million",
                        "decimals": decimals
                    })
                }
                _ => serde_json::json!({
                    "kind": "Money",
                    "currency": currency
                }),
            }
        }
        "Percent" => {
            let decimals = obj.get("decimals").and_then(|v| v.as_u64()).unwrap_or(2) as u8;
            serde_json::json!({
                "kind": "Percent",
                "decimals": decimals
            })
        }
        "Integer" => serde_json::json!({
            "kind": "Integer"
        }),
        "Number" => {
            let decimals = obj.get("decimals").and_then(|v| v.as_u64()).unwrap_or(2) as u8;
            serde_json::json!({
                "kind": "Number",
                "decimals": decimals
            })
        }
        _ => default_value_format_value(),
    }
}

fn normalize_design(style_name: &str, custom_css: &str) -> String {
    let has_custom_css = !custom_css.trim().is_empty();
    if is_known_design(style_name, has_custom_css) {
        style_name.to_string()
    } else {
        default_design_name().to_string()
    }
}

fn normalized_loaded_status(value: &serde_json::Value) -> String {
    let raw = value.as_str().unwrap_or("draft").trim();
    match raw {
        "Draft" | "draft" => "draft".to_string(),
        "Active" | "active" => "active".to_string(),
        "Archived" | "archived" => "archived".to_string(),
        _ => "draft".to_string(),
    }
}

fn read_i64_field(
    root: &serde_json::Value,
    top_level_key: &str,
    metadata_key: &str,
    default: i64,
) -> i64 {
    root[top_level_key]
        .as_i64()
        .or_else(|| root["metadata"][metadata_key].as_i64())
        .unwrap_or(default)
}

fn read_string_field(root: &serde_json::Value, top_level_key: &str, metadata_key: &str) -> String {
    root[top_level_key]
        .as_str()
        .or_else(|| root["metadata"][metadata_key].as_str())
        .unwrap_or("")
        .to_string()
}

#[derive(Clone, Debug)]
pub struct LlmGenerationEntry {
    pub prompt: String,
    pub html: String,
    pub css: String,
    pub explanation: String,
}

#[derive(Clone)]
pub struct BiIndicatorDetailsVm {
    // === Form fields ===
    pub id: RwSignal<Option<String>>,
    pub code: RwSignal<String>,
    pub description: RwSignal<String>,
    pub comment: RwSignal<String>,
    pub explanation: RwSignal<String>,
    pub status: RwSignal<String>,
    pub owner_user_id: RwSignal<String>,
    pub is_public: RwSignal<bool>,
    pub version: RwSignal<i64>,

    // DataView config
    /// DataView ID (e.g. "dv001_revenue") — единственный путь вычисления индикатора.
    pub dsc_view_id: RwSignal<String>,
    /// Resource/metric ID from DataViewMeta.available_resources.
    pub dsc_metric_id: RwSignal<String>,

    // Params (whole Vec<ParamDef> as JSON string)
    pub params_json: RwSignal<String>,

    // ViewSpec fields
    pub view_spec_style_name: RwSignal<String>,
    pub view_spec_custom_html: RwSignal<String>,
    pub view_spec_custom_css: RwSignal<String>,
    pub view_spec_format_json: RwSignal<String>,
    pub view_spec_thresholds_json: RwSignal<String>,

    // DrillSpec (whole Option<DrillSpec> as JSON string, empty = None)
    pub drill_spec_json: RwSignal<String>,

    // Meta (read-only display)
    pub created_at: RwSignal<String>,
    pub updated_at: RwSignal<String>,
    pub created_by: RwSignal<String>,
    pub updated_by: RwSignal<String>,

    // === UI State ===
    pub active_tab: RwSignal<&'static str>,
    pub loading: RwSignal<bool>,
    pub saving: RwSignal<bool>,
    pub error: RwSignal<Option<String>>,
    pub success: RwSignal<Option<String>>,

    // === Preview field values (saved with record via view_spec.preview_values) ===
    pub preview_title: RwSignal<String>,        // {{name}}
    pub preview_value: RwSignal<String>,        // {{value}}
    pub preview_unit: RwSignal<String>,         // {{unit}}
    pub preview_delta: RwSignal<String>,        // {{delta}}
    pub preview_delta_dir: RwSignal<String>,    // {{delta_dir}} → arrow
    pub preview_status: RwSignal<String>,       // {{status}}
    pub preview_chip: RwSignal<String>,         // {{chip}}
    pub preview_meta_1: RwSignal<String>,       // {{meta_1}}
    pub preview_meta_2: RwSignal<String>,       // {{meta_2}}
    pub preview_graph_type: RwSignal<u8>,       // 0-none, 1-progress, 2-spark
    pub preview_progress: RwSignal<u8>,         // {{progress}} (modern)
    pub preview_hint: RwSignal<String>,         // {{hint}} (modern)
    pub preview_footer_1: RwSignal<String>,     // {{footer_1}} (modern)
    pub preview_footer_2: RwSignal<String>,     // {{footer_2}} (modern)
    pub preview_spark_points: RwSignal<String>, // comma-separated f64 for sparkline (classic)
    pub preview_hidden_fields: RwSignal<std::collections::HashSet<String>>, // keys excluded from render
    pub preview_size: RwSignal<String>,                                     // iframe sizing

    // === LLM generation state ===
    pub llm_prompt: RwSignal<String>,
    pub llm_generating: RwSignal<bool>,
    pub llm_error: RwSignal<Option<String>>,
    pub llm_history: RwSignal<Vec<LlmGenerationEntry>>,
    pub llm_panel_open: RwSignal<bool>,

    // === DataSpec live test ===
    pub test_ctx: RwSignal<ViewContext>,
    pub test_loading: RwSignal<bool>,
    pub test_error: RwSignal<Option<String>>,
    pub test_result: RwSignal<Option<ComputedIndicatorValue>>,
}

impl BiIndicatorDetailsVm {
    pub fn new() -> Self {
        Self {
            id: RwSignal::new(None),
            code: RwSignal::new(String::new()),
            description: RwSignal::new(String::new()),
            comment: RwSignal::new(String::new()),
            explanation: RwSignal::new(String::new()),
            status: RwSignal::new("draft".to_string()),
            owner_user_id: RwSignal::new(String::new()),
            is_public: RwSignal::new(false),
            version: RwSignal::new(1),

            dsc_view_id: RwSignal::new(String::new()),
            dsc_metric_id: RwSignal::new(String::new()),

            params_json: RwSignal::new("[]".to_string()),

            view_spec_style_name: RwSignal::new(default_design_name().to_string()),
            view_spec_custom_html: RwSignal::new(String::new()),
            view_spec_custom_css: RwSignal::new(String::new()),
            view_spec_format_json: RwSignal::new(
                serde_json::to_string_pretty(&default_value_format_value())
                    .unwrap_or_else(|_| "{}".to_string()),
            ),
            view_spec_thresholds_json: RwSignal::new("[]".to_string()),

            drill_spec_json: RwSignal::new(String::new()),

            created_at: RwSignal::new(String::new()),
            updated_at: RwSignal::new(String::new()),
            created_by: RwSignal::new(String::new()),
            updated_by: RwSignal::new(String::new()),

            active_tab: RwSignal::new("general"),
            loading: RwSignal::new(false),
            saving: RwSignal::new(false),
            error: RwSignal::new(None),
            success: RwSignal::new(None),

            preview_title: RwSignal::new("Выручка".to_string()),
            preview_value: RwSignal::new("2 400 000 ₽".to_string()),
            preview_unit: RwSignal::new(String::new()),
            preview_delta: RwSignal::new("+12.5%".to_string()),
            preview_delta_dir: RwSignal::new("up".to_string()),
            preview_status: RwSignal::new("ok".to_string()),
            preview_chip: RwSignal::new(String::new()),
            preview_meta_1: RwSignal::new(String::new()),
            preview_meta_2: RwSignal::new(String::new()),
            preview_graph_type: RwSignal::new(2u8),
            preview_progress: RwSignal::new(0u8),
            preview_hint: RwSignal::new(String::new()),
            preview_footer_1: RwSignal::new(String::new()),
            preview_footer_2: RwSignal::new(String::new()),
            preview_spark_points: RwSignal::new(String::new()),
            preview_hidden_fields: RwSignal::new(std::collections::HashSet::new()),
            preview_size: RwSignal::new("1x1".to_string()),

            llm_prompt: RwSignal::new(String::new()),
            llm_generating: RwSignal::new(false),
            llm_error: RwSignal::new(None),
            llm_history: RwSignal::new(Vec::new()),
            llm_panel_open: RwSignal::new(false),

            test_ctx: RwSignal::new(default_test_ctx()),
            test_loading: RwSignal::new(false),
            test_error: RwSignal::new(None),
            test_result: RwSignal::new(None),
        }
    }

    // === Derived signals ===

    pub fn is_edit_mode(&self) -> Signal<bool> {
        let id = self.id;
        Signal::derive(move || id.get().is_some())
    }

    pub fn is_valid(&self) -> Signal<bool> {
        let description = self.description;
        let preview_title = self.preview_title;
        Signal::derive(move || {
            let preview_title = preview_title.get();
            let description = description.get();
            let value = if preview_title.trim().is_empty() {
                description
            } else {
                preview_title
            };
            !value.trim().is_empty()
        })
    }

    pub fn is_save_disabled(&self) -> Signal<bool> {
        let saving = self.saving;
        let is_valid = self.is_valid();
        Signal::derive(move || saving.get() || !is_valid.get())
    }

    pub fn current_format_preset_key(&self) -> String {
        let format_json = self.view_spec_format_json.get();
        let parsed = serde_json::from_str::<serde_json::Value>(&format_json)
            .unwrap_or_else(|_| default_value_format_value());
        preset_key_from_value_format(parsed).to_string()
    }

    pub fn apply_format_preset(&self, preset_key: &str) {
        let Some(format_value) = value_format_from_preset_key(preset_key) else {
            return;
        };
        self.view_spec_format_json
            .set(serde_json::to_string_pretty(&format_value).unwrap_or_else(|_| "{}".to_string()));
        if self.test_result.get().is_some() {
            self.apply_test_to_preview();
        }
    }

    pub fn get_param_default_value(&self, key: &str) -> String {
        let params = serde_json::from_str::<serde_json::Value>(&self.params_json.get())
            .unwrap_or_else(|_| serde_json::json!([]));
        params
            .as_array()
            .and_then(|items| {
                items.iter().find_map(|item| {
                    let item_key = item.get("key")?.as_str()?;
                    if item_key == key {
                        item.get("default_value")
                            .and_then(|value| value.as_str())
                            .map(|value| value.to_string())
                    } else {
                        None
                    }
                })
            })
            .unwrap_or_default()
    }

    pub fn set_param_default_value(
        &self,
        key: &str,
        param_type: &str,
        label: &str,
        default_value: &str,
    ) {
        let mut params = serde_json::from_str::<serde_json::Value>(&self.params_json.get())
            .unwrap_or_else(|_| serde_json::json!([]));
        let Some(items) = params.as_array_mut() else {
            self.params_json.set("[]".to_string());
            return;
        };

        let mut replaced = false;
        for item in items.iter_mut() {
            if item.get("key").and_then(|value| value.as_str()) == Some(key) {
                *item = serde_json::json!({
                    "key": key,
                    "param_type": param_type,
                    "label": label,
                    "default_value": if default_value.trim().is_empty() {
                        serde_json::Value::Null
                    } else {
                        serde_json::Value::String(default_value.to_string())
                    },
                    "required": true,
                    "global_filter_key": serde_json::Value::Null
                });
                replaced = true;
                break;
            }
        }

        if !replaced {
            items.push(serde_json::json!({
                "key": key,
                "param_type": param_type,
                "label": label,
                "default_value": if default_value.trim().is_empty() {
                    serde_json::Value::Null
                } else {
                    serde_json::Value::String(default_value.to_string())
                },
                "required": true,
                "global_filter_key": serde_json::Value::Null
            }));
        }

        self.params_json
            .set(serde_json::to_string_pretty(&params).unwrap_or_else(|_| "[]".to_string()));
    }

    /// Build the iframe srcdoc from current ViewSpec + preview field values
    pub fn build_preview_srcdoc(&self) -> Signal<String> {
        let style_sig = self.view_spec_style_name;
        let html_sig = self.view_spec_custom_html;
        let css_sig = self.view_spec_custom_css;
        let name_sig = self.preview_title;
        let value_sig = self.preview_value;
        let unit_sig = self.preview_unit;
        let delta_sig = self.preview_delta;
        let delta_dir_sig = self.preview_delta_dir;
        let status_sig = self.preview_status;
        let chip_sig = self.preview_chip;
        let meta_1_sig = self.preview_meta_1;
        let meta_2_sig = self.preview_meta_2;
        let graph_type_sig = self.preview_graph_type;
        let progress_sig = self.preview_progress;
        let hint_sig = self.preview_hint;
        let footer_1_sig = self.preview_footer_1;
        let footer_2_sig = self.preview_footer_2;
        let spark_sig = self.preview_spark_points;
        let hidden_sig = self.preview_hidden_fields;

        Signal::derive(move || {
            let hidden = hidden_sig.get();
            let vis = |key: &str, val: String| -> String {
                if hidden.contains(key) {
                    String::new()
                } else {
                    val
                }
            };

            let mut graph_type = graph_type_sig.get().min(2);
            if (graph_type == 1 && hidden.contains("progress"))
                || (graph_type == 2 && hidden.contains("spark"))
            {
                graph_type = 0;
            }

            let spark_points: Vec<f64> = if graph_type == 2 && !hidden.contains("spark") {
                spark_sig
                    .get()
                    .split(',')
                    .filter_map(|p| p.trim().parse::<f64>().ok())
                    .collect()
            } else {
                vec![]
            };
            let params = IndicatorCardParams {
                style_name: style_sig.get(),
                theme: get_app_theme(),
                name: vis("name", name_sig.get()),
                value: vis("value", value_sig.get()),
                unit: vis("unit", unit_sig.get()),
                delta: vis("delta", delta_sig.get()),
                delta_dir: vis("delta_dir", delta_dir_sig.get()),
                status: vis("status", status_sig.get()),
                chip: vis("chip", chip_sig.get()),
                col_class: String::new(),
                graph_type,
                progress: if graph_type == 1 && !hidden.contains("progress") {
                    progress_sig.get()
                } else {
                    0
                },
                spark_points,
                meta_1: vis("meta_1", meta_1_sig.get()),
                meta_2: vis("meta_2", meta_2_sig.get()),
                hint: vis("hint", hint_sig.get()),
                footer_1: vis("footer_1", footer_1_sig.get()),
                footer_2: vis("footer_2", footer_2_sig.get()),
                custom_html: {
                    let html = html_sig.get();
                    if html.trim().is_empty() {
                        None
                    } else {
                        Some(html)
                    }
                },
                custom_css: {
                    let c = css_sig.get();
                    if c.trim().is_empty() {
                        None
                    } else {
                        Some(c)
                    }
                },
            };
            render_srcdoc(&params)
        })
    }

    // === Validation ===

    pub fn validate(&self) -> Result<(), String> {
        if self.resolved_description().trim().is_empty() {
            return Err("Название индикатора обязательно для заполнения".into());
        }
        Ok(())
    }

    // === Data loading ===

    pub fn load(&self, id: String) {
        let this = self.clone();
        this.loading.set(true);
        this.error.set(None);

        leptos::task::spawn_local(async move {
            match model::fetch_by_id(&id).await {
                Ok(item) => {
                    this.from_raw(&item);
                    this.loading.set(false);
                }
                Err(e) => {
                    this.error.set(Some(e));
                    this.loading.set(false);
                }
            }
        });
    }

    // === Commands ===

    pub fn save(&self, on_saved: Callback<()>) {
        if let Err(msg) = self.validate() {
            self.error.set(Some(msg));
            return;
        }

        let this = self.clone();
        this.saving.set(true);
        this.error.set(None);
        let resolved_description = this.resolved_description();
        if this.description.get() != resolved_description {
            this.description.set(resolved_description);
        }

        let dto = this.to_dto();

        leptos::task::spawn_local(async move {
            match model::save_indicator(dto).await {
                Ok(new_id) => {
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

    /// Generate view via LLM
    pub fn generate_view(&self) {
        let prompt = self.llm_prompt.get();
        if prompt.trim().is_empty() {
            self.llm_error.set(Some("Введите описание".into()));
            return;
        }

        let this = self.clone();
        this.llm_generating.set(true);
        this.llm_error.set(None);

        let current_css = {
            let c = this.view_spec_custom_css.get();
            if c.trim().is_empty() {
                None
            } else {
                Some(c)
            }
        };
        let current_html = {
            let html = this.view_spec_custom_html.get();
            if html.trim().is_empty() {
                None
            } else {
                Some(html)
            }
        };
        let indicator_description = this.description.get();

        leptos::task::spawn_local(async move {
            match model::generate_view(
                &prompt,
                current_html.as_deref(),
                current_css.as_deref(),
                &indicator_description,
            )
            .await
            {
                Ok(resp) => {
                    this.llm_history.update(|h| {
                        h.push(LlmGenerationEntry {
                            prompt: prompt.clone(),
                            html: resp.custom_html.clone(),
                            css: resp.custom_css.clone(),
                            explanation: resp.explanation.clone(),
                        });
                        if h.len() > 10 {
                            h.remove(0);
                        }
                    });
                    this.llm_generating.set(false);
                    this.llm_prompt.set(String::new());
                }
                Err(e) => {
                    this.llm_error.set(Some(e));
                    this.llm_generating.set(false);
                }
            }
        });
    }

    /// Apply a specific LLM generation entry to the ViewSpec fields
    pub fn apply_generation(&self, entry: &LlmGenerationEntry) {
        self.view_spec_custom_html.set(entry.html.clone());
        self.view_spec_custom_css
            .set(code_format::format_css(&entry.css));
        if !entry.css.trim().is_empty() {
            self.view_spec_style_name.set("custom".to_string());
        }
    }

    /// Вычислить индикатор через DataView. Требует сохранённого UUID.
    pub fn run_test(&self) {
        let Some(uuid) = self.id.get() else {
            self.test_error
                .set(Some("Сохраните индикатор перед тестированием".into()));
            return;
        };

        let ctx = self.test_ctx.get();

        let this = self.clone();
        this.test_loading.set(true);
        this.test_error.set(None);
        this.test_result.set(None);

        leptos::task::spawn_local(async move {
            match model::compute_indicator_by_id(
                &uuid,
                &ctx.date_from,
                &ctx.date_to,
                ctx.period2_from.as_deref(),
                ctx.period2_to.as_deref(),
                ctx.connection_mp_refs,
                ctx.params,
            )
            .await
            {
                Ok(result) => {
                    this.test_loading.set(false);
                    this.test_result.set(Some(result));
                }
                Err(e) => {
                    this.test_loading.set(false);
                    this.test_error.set(Some(e));
                }
            }
        });
    }

    /// Apply the test result to the preview fields so the preview tab reflects real data
    pub fn apply_test_to_preview(&self) {
        let Some(result) = self.test_result.get() else {
            return;
        };

        // Format the primary value using view_spec format
        let formatted = self.format_value(result.value);
        self.preview_value.set(formatted);

        // Format delta
        if let Some(pct) = result.change_percent {
            let sign = if pct >= 0.0 { "+" } else { "" };
            self.preview_delta.set(format!("{sign}{:.1}%", pct));
            self.preview_delta_dir.set(if pct > 0.0 {
                "up".to_string()
            } else if pct < 0.0 {
                "down".to_string()
            } else {
                "flat".to_string()
            });
        } else {
            self.preview_delta.set(String::new());
            self.preview_delta_dir.set("flat".to_string());
        }

        // Map status
        let status_str = match result.status.as_str() {
            "Good" => "ok",
            "Bad" => "bad",
            "Warning" => "warn",
            _ => "neutral",
        };
        self.preview_status.set(status_str.to_string());
    }

    /// Format a numeric value according to the current view_spec format
    pub fn format_value(&self, value: Option<f64>) -> String {
        let Some(v) = value else {
            return "—".to_string();
        };
        let format_json = self.view_spec_format_json.get();
        let fmt: serde_json::Value =
            serde_json::from_str(&format_json).unwrap_or_else(|_| default_value_format_value());
        let kind = fmt["kind"].as_str().unwrap_or("Number");
        match kind {
            "Money" => format_money_with_format_spec(v, &fmt),
            "Percent" => {
                let decimals = fmt["decimals"].as_u64().unwrap_or(1) as usize;
                format!("{:.prec$}%", v, prec = decimals)
            }
            "Integer" => {
                let abs = v.abs();
                if abs >= 1_000_000_000.0 {
                    format!("{:.2}B", v / 1_000_000_000.0)
                } else if abs >= 1_000_000.0 {
                    format!("{:.2}M", v / 1_000_000.0)
                } else {
                    format!("{}", v as i64)
                }
            }
            _ => {
                let decimals = fmt["decimals"].as_u64().unwrap_or(2) as usize;
                let abs = v.abs();
                if abs >= 1_000_000_000.0 {
                    format!("{:.2}B", v / 1_000_000_000.0)
                } else if abs >= 1_000_000.0 {
                    format!("{:.2}M", v / 1_000_000.0)
                } else {
                    format!("{:.prec$}", v, prec = decimals)
                }
            }
        }
    }

    // === Tab helpers ===

    pub fn set_tab(&self, tab: &'static str) {
        self.active_tab.set(tab);
    }

    // === Private helpers ===

    fn resolved_description(&self) -> String {
        let preview_title = self.preview_title.get();
        if preview_title.trim().is_empty() {
            self.description.get()
        } else {
            preview_title
        }
    }

    fn to_dto(&self) -> BiIndicatorSaveDto {
        let view_id = self.dsc_view_id.get();
        let metric_id = self.dsc_metric_id.get();
        let data_spec = data_spec_json_value(&view_id, &metric_id);

        let params = serde_json::from_str::<serde_json::Value>(&self.params_json.get())
            .unwrap_or(serde_json::json!([]));

        let format_raw =
            serde_json::from_str::<serde_json::Value>(&self.view_spec_format_json.get())
                .unwrap_or_else(|_| default_value_format_value());
        let format = normalized_value_format(format_raw);

        let thresholds =
            serde_json::from_str::<serde_json::Value>(&self.view_spec_thresholds_json.get())
                .unwrap_or(serde_json::json!([]));

        let style_name = normalize_design(
            &self.view_spec_style_name.get(),
            &self.view_spec_custom_css.get(),
        );
        let custom_css = {
            let c = self.view_spec_custom_css.get();
            if c.trim().is_empty() {
                serde_json::Value::Null
            } else {
                serde_json::Value::String(c)
            }
        };
        let custom_html = {
            let html = self.view_spec_custom_html.get();
            if html.trim().is_empty() {
                serde_json::Value::Null
            } else {
                serde_json::Value::String(html)
            }
        };

        let hidden_str = {
            let mut keys: Vec<String> = self.preview_hidden_fields.get().into_iter().collect();
            keys.sort();
            keys.join(",")
        };
        let preview_values = serde_json::json!({
            "name":         self.preview_title.get(),
            "value":        self.preview_value.get(),
            "unit":         self.preview_unit.get(),
            "delta":        self.preview_delta.get(),
            "delta_dir":    self.preview_delta_dir.get(),
            "status":       self.preview_status.get(),
            "chip":         self.preview_chip.get(),
            "meta_1":       self.preview_meta_1.get(),
            "meta_2":       self.preview_meta_2.get(),
            "graph_type":   self.preview_graph_type.get().to_string(),
            "progress":     self.preview_progress.get().to_string(),
            "hint":         self.preview_hint.get(),
            "footer_1":     self.preview_footer_1.get(),
            "footer_2":     self.preview_footer_2.get(),
            "spark_points": self.preview_spark_points.get(),
            "_hidden":      hidden_str,
        });

        let view_spec = serde_json::json!({
            "style_name": style_name,
            "custom_html": custom_html,
            "custom_css": custom_css,
            "format": format,
            "thresholds": thresholds,
            "preview_values": preview_values,
        });

        let drill_spec_raw = self.drill_spec_json.get();
        let drill_spec = if drill_spec_raw.trim().is_empty() || drill_spec_raw.trim() == "null" {
            serde_json::Value::Null
        } else {
            serde_json::from_str::<serde_json::Value>(&drill_spec_raw)
                .unwrap_or(serde_json::Value::Null)
        };

        BiIndicatorSaveDto {
            id: self.id.get(),
            code: self.code.get(),
            description: self.resolved_description(),
            comment: {
                let c = self.comment.get();
                if c.trim().is_empty() {
                    None
                } else {
                    Some(c)
                }
            },
            explanation: {
                let explanation = self.explanation.get();
                if explanation.trim().is_empty() {
                    None
                } else {
                    Some(explanation)
                }
            },
            status: self.status.get(),
            owner_user_id: self.owner_user_id.get(),
            is_public: self.is_public.get(),
            version: self.version.get(),
            data_spec,
            params,
            view_spec,
            drill_spec,
        }
    }

    fn from_raw(&self, v: &serde_json::Value) {
        self.id.set(v["id"].as_str().map(|s| s.to_string()));
        self.code.set(v["code"].as_str().unwrap_or("").to_string());
        self.description
            .set(v["description"].as_str().unwrap_or("").to_string());
        self.comment
            .set(v["comment"].as_str().unwrap_or("").to_string());
        self.explanation
            .set(v["explanation"].as_str().unwrap_or("").to_string());
        self.status.set(normalized_loaded_status(&v["status"]));
        self.owner_user_id
            .set(v["owner_user_id"].as_str().unwrap_or("").to_string());
        self.is_public
            .set(v["is_public"].as_bool().unwrap_or(false));
        self.version.set(read_i64_field(v, "version", "version", 1));

        // DataSpec
        self.dsc_view_id.set(String::new());
        self.dsc_metric_id.set(String::new());
        if let Some(ds) = v.get("data_spec") {
            if let Some(vid) = ds.get("view_id").and_then(|v| v.as_str()) {
                self.dsc_view_id.set(vid.to_string());
            }
            if let Some(metric_id) = ds.get("metric_id").and_then(|v| v.as_str()) {
                self.dsc_metric_id.set(metric_id.to_string());
            }
        }

        // Params
        if let Some(params) = v.get("params") {
            self.params_json
                .set(serde_json::to_string_pretty(params).unwrap_or_else(|_| "[]".to_string()));
        }

        // ViewSpec
        if let Some(vs) = v.get("view_spec") {
            let raw_css = vs["custom_css"].as_str().unwrap_or("").to_string();
            self.view_spec_custom_css
                .set(code_format::format_css(&raw_css));
            self.view_spec_custom_html
                .set(vs["custom_html"].as_str().unwrap_or("").to_string());
            let normalized_style =
                normalize_design(vs["style_name"].as_str().unwrap_or("classic"), &raw_css);
            self.view_spec_style_name.set(normalized_style);
            let normalized_format = normalized_value_format(
                vs.get("format")
                    .cloned()
                    .unwrap_or_else(default_value_format_value),
            );
            self.view_spec_format_json.set(
                serde_json::to_string_pretty(&normalized_format)
                    .unwrap_or_else(|_| "{}".to_string()),
            );
            self.view_spec_thresholds_json.set(
                serde_json::to_string_pretty(
                    vs.get("thresholds").unwrap_or(&serde_json::json!([])),
                )
                .unwrap_or_else(|_| "[]".to_string()),
            );

            // Load saved preview values (if any)
            if let Some(pv) = vs.get("preview_values").and_then(|v| v.as_object()) {
                let s = |key: &str| {
                    pv.get(key)
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string()
                };
                self.preview_title.set(s("name"));
                self.preview_value.set(s("value"));
                self.preview_unit.set(s("unit"));
                self.preview_delta.set(s("delta"));
                self.preview_delta_dir.set({
                    let d = s("delta_dir");
                    if d.is_empty() {
                        "up".to_string()
                    } else {
                        d
                    }
                });
                self.preview_status.set({
                    let st = s("status");
                    if st.is_empty() {
                        "ok".to_string()
                    } else {
                        st
                    }
                });
                self.preview_chip.set(s("chip"));
                self.preview_meta_1.set(s("meta_1"));
                self.preview_meta_2.set(s("meta_2"));
                self.preview_graph_type.set({
                    let explicit = s("graph_type").parse::<u8>().ok().map(|v| v.min(2));
                    if let Some(v) = explicit {
                        v
                    } else {
                        let progress_raw = s("progress").parse::<u8>().unwrap_or(0);
                        let has_spark = !s("spark_points").trim().is_empty();
                        if progress_raw > 0 {
                            1
                        } else if has_spark {
                            2
                        } else {
                            2
                        }
                    }
                });
                self.preview_progress
                    .set(s("progress").parse::<u8>().unwrap_or(0));
                self.preview_hint.set(s("hint"));
                self.preview_footer_1.set(s("footer_1"));
                self.preview_footer_2.set(s("footer_2"));
                self.preview_spark_points.set(s("spark_points"));
                let hidden_str = s("_hidden");
                let hidden: std::collections::HashSet<String> = hidden_str
                    .split(',')
                    .filter(|k| !k.is_empty())
                    .map(|k| k.to_string())
                    .collect();
                self.preview_hidden_fields.set(hidden);
            }
        }

        // DrillSpec
        if let Some(drill) = v.get("drill_spec") {
            if drill.is_null() {
                self.drill_spec_json.set(String::new());
            } else {
                self.drill_spec_json
                    .set(serde_json::to_string_pretty(drill).unwrap_or_default());
            }
        } else {
            self.drill_spec_json.set(String::new());
        }

        // Meta
        self.created_at
            .set(read_string_field(v, "created_at", "created_at"));
        self.updated_at
            .set(read_string_field(v, "updated_at", "updated_at"));
        self.created_by
            .set(v["created_by"].as_str().unwrap_or("").to_string());
        self.updated_by
            .set(v["updated_by"].as_str().unwrap_or("").to_string());
    }
}

impl Default for BiIndicatorDetailsVm {
    fn default() -> Self {
        Self::new()
    }
}
