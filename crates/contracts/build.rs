//! Build script for generating metadata.rs files from metadata.json
//!
//! This script scans the entity directories (`src/domain`, `src/projections`) for
//! metadata.json files and generates corresponding metadata_gen.rs files with
//! static Rust constants. It also emits `src/shared/metadata/registry_gen.rs`,
//! which lists every generated entity, so that consumers never have to register
//! a new aggregate or projection by hand.

use serde::Deserialize;
use std::fs;
use std::path::Path;

/// Crate-relative directories scanned for `<entity>/metadata.json`.
/// The second element is the Rust module path the generated registry uses.
const ENTITY_ROOTS: &[(&str, &str)] = &[
    ("src/domain", "crate::domain"),
    ("src/projections", "crate::projections"),
];

/// Single-entity directories: the metadata.json sits directly inside, without a
/// per-entity subdirectory. Kept separate so the General Ledger is described by the
/// same JSON standard as everything else instead of a hand-written module.
const ENTITY_DIRS: &[(&str, &str)] = &[("src/general_ledger", "crate::general_ledger")];

/// Subdirectories that never carry entity metadata.
const SKIP_DIRS: &[&str] = &["common", "general_ledger"];

/// One entity picked up by the scan, in the order it will appear in the registry.
struct RegistryItem {
    module_path: String,
    entity_index: String,
}

fn main() {
    let mut registry: Vec<RegistryItem> = Vec::new();

    for (root, module_root) in ENTITY_ROOTS {
        println!("cargo:rerun-if-changed={}", root);

        let root_dir = Path::new(root);
        if !root_dir.exists() {
            println!(
                "cargo:warning={} not found, skipping metadata generation",
                root
            );
            continue;
        }

        // Sort by directory name so the generated registry has a stable order.
        let mut dirs: Vec<_> = fs::read_dir(root_dir)
            .unwrap_or_else(|e| panic!("Failed to read {}: {}", root, e))
            .map(|entry| entry.expect("Failed to read entry").path())
            .filter(|path| path.is_dir())
            .collect();
        dirs.sort();

        for path in dirs {
            let dir_name = path.file_name().unwrap().to_str().unwrap().to_string();
            if SKIP_DIRS.contains(&dir_name.as_str()) {
                continue;
            }

            let metadata_json = path.join("metadata.json");
            if !metadata_json.exists() {
                continue;
            }
            println!("cargo:rerun-if-changed={}", metadata_json.display());

            let output_rs = path.join("metadata_gen.rs");
            let entity_index = match generate_metadata(&metadata_json, &output_rs) {
                Ok(index) => index,
                Err(e) => panic!("Failed to generate metadata for {}: {}", dir_name, e),
            };

            registry.push(RegistryItem {
                module_path: format!("{}::{}", module_root, dir_name),
                entity_index,
            });
        }
    }

    for (dir, module_path) in ENTITY_DIRS {
        let path = Path::new(dir);
        let metadata_json = path.join("metadata.json");
        if !metadata_json.exists() {
            panic!(
                "{} is declared as an entity directory but has no metadata.json",
                dir
            );
        }
        println!("cargo:rerun-if-changed={}", metadata_json.display());

        let entity_index = generate_metadata(&metadata_json, &path.join("metadata_gen.rs"))
            .unwrap_or_else(|e| panic!("Failed to generate metadata for {}: {}", dir, e));

        registry.push(RegistryItem {
            module_path: module_path.to_string(),
            entity_index,
        });
    }

    let registry_path = Path::new("src/shared/metadata/registry_gen.rs");
    write_if_changed(registry_path, &generate_registry_code(&registry))
        .unwrap_or_else(|e| panic!("Failed to generate entity registry: {}", e));
}

// ============================================================================
// JSON Schema Types (owned Strings for serde deserialization)
// ============================================================================

#[derive(Debug, Deserialize)]
struct MetadataJson {
    schema_version: String,
    entity_type: String,
    entity_name: String,
    entity_index: String,
    collection_name: String,
    table_name: Option<String>,
    ui: UiMetadataJson,
    ai: AiMetadataJson,
    fields: Vec<FieldJson>,
    #[serde(default)]
    access: Option<AccessJson>,
}

#[derive(Debug, Deserialize)]
struct UiMetadataJson {
    element_name: String,
    element_name_en: Option<String>,
    list_name: String,
    list_name_en: Option<String>,
    icon: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AiMetadataJson {
    description: String,
    #[serde(default)]
    questions: Vec<String>,
    #[serde(default)]
    related: Vec<String>,
    /// Thematic tags for the LLM registry filter ("wb", "ym", "ref", ...).
    #[serde(default)]
    tags: Vec<String>,
    /// Whether the entity is advertised to the LLM. Visible unless opted out.
    #[serde(default = "default_true")]
    llm_visible: bool,
}

#[derive(Debug, Deserialize)]
struct FieldJson {
    name: String,
    rust_type: String,
    field_type: String,
    #[serde(default)]
    source: String,
    ui: FieldUiJson,
    #[serde(default)]
    validation: ValidationJson,
    ai_hint: Option<String>,
    /// False for logical fields that live inside a JSON column, not in the table.
    #[serde(default = "default_true")]
    physical: bool,
    #[allow(dead_code)]
    nested_fields: Option<Vec<FieldJson>>,
    ref_aggregate: Option<String>,
    enum_values: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, Default)]
struct FieldUiJson {
    #[serde(default)]
    label: String,
    label_en: Option<String>,
    placeholder: Option<String>,
    hint: Option<String>,
    #[serde(default = "default_true")]
    visible_in_list: bool,
    #[serde(default = "default_true")]
    visible_in_form: bool,
    widget: Option<String>,
    column_width: Option<u32>,
}

#[derive(Debug, Deserialize, Default)]
struct ValidationJson {
    #[serde(default)]
    required: bool,
    min: Option<f64>,
    max: Option<f64>,
    min_length: Option<usize>,
    max_length: Option<usize>,
    pattern: Option<String>,
    custom_error: Option<String>,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Deserialize)]
struct AccessJson {
    scope_id: String,
    #[serde(default)]
    operations: Vec<AccessOperationJson>,
}

#[derive(Debug, Deserialize)]
struct AccessOperationJson {
    id: String,
    required_mode: String,
}

// ============================================================================
// Code Generation
// ============================================================================

/// Generates `metadata_gen.rs` next to the source JSON and returns the entity index.
fn generate_metadata(
    json_path: &Path,
    output_path: &Path,
) -> Result<String, Box<dyn std::error::Error>> {
    let json_content = fs::read_to_string(json_path)?;
    let metadata: MetadataJson = serde_json::from_str(&json_content)?;
    validate_metadata(&metadata)?;

    let code = generate_rust_code(&metadata);
    write_if_changed(output_path, &code)?;

    Ok(metadata.entity_index.clone())
}

/// Rejects enum values the generator would happily turn into non-existent Rust variants.
/// Without this an unknown `field_type` only fails much later, as a compile error inside
/// a generated file — and not at all while the module stays unreferenced.
fn validate_metadata(meta: &MetadataJson) -> Result<(), Box<dyn std::error::Error>> {
    const ENTITY_TYPES: &[&str] = &["aggregate", "usecase", "projection"];
    const FIELD_TYPES: &[&str] = &[
        "primitive",
        "enum",
        "aggregate_ref",
        "nested_struct",
        "nested_table",
    ];
    const FIELD_SOURCES: &[&str] = &["specific", "base", "metadata"];

    if !ENTITY_TYPES.contains(&meta.entity_type.as_str()) {
        return Err(format!(
            "unknown entity_type '{}' (expected one of {:?})",
            meta.entity_type, ENTITY_TYPES
        )
        .into());
    }

    for field in &meta.fields {
        if !FIELD_TYPES.contains(&field.field_type.as_str()) {
            return Err(format!(
                "field '{}': unknown field_type '{}' (expected one of {:?})",
                field.name, field.field_type, FIELD_TYPES
            )
            .into());
        }
        if !field.source.is_empty() && !FIELD_SOURCES.contains(&field.source.as_str()) {
            return Err(format!(
                "field '{}': unknown source '{}' (expected one of {:?})",
                field.name, field.source, FIELD_SOURCES
            )
            .into());
        }
    }

    Ok(())
}

/// Writes only when the content differs, so no-op builds don't touch timestamps.
fn write_if_changed(path: &Path, content: &str) -> Result<bool, Box<dyn std::error::Error>> {
    if path.exists() && fs::read_to_string(path)? == content {
        return Ok(false);
    }

    fs::write(path, content)?;

    Ok(true)
}

/// Emits the crate-wide registry of every entity backed by a metadata.json.
fn generate_registry_code(items: &[RegistryItem]) -> String {
    let mut code = String::from(
        "// ============================================================================\n\
         // AUTO-GENERATED FROM metadata.json FILES - DO NOT EDIT MANUALLY\n\
         // ============================================================================\n\
         //\n\
         // Every aggregate/projection that has a metadata.json lands here automatically,\n\
         // so consumers (LLM metadata registry, dashboards) never miss a new entity.\n\
         // Filter with `EntityRegistration::meta.ai.llm_visible` / `.ai.tags`.\n\n\
         #![allow(dead_code)]\n\n\
         use super::{EntityMetadataInfo, FieldMetadata};\n\n\
         /// One entity exposed through [`ALL_ENTITIES`].\n\
         pub struct EntityRegistration {\n\
         \x20   pub meta: &'static EntityMetadataInfo,\n\
         \x20   pub fields: &'static [FieldMetadata],\n\
         }\n\n",
    );

    code.push_str(
        "/// All entities generated from metadata.json, ordered by module path.\n\
         pub const ALL_ENTITIES: &[EntityRegistration] = &[\n",
    );

    for item in items {
        code.push_str(&format!(
            "    // {}\n    EntityRegistration {{\n\
             \x20       meta: &{path}::ENTITY_METADATA,\n\
             \x20       fields: {path}::FIELDS,\n\
             \x20   }},\n",
            item.entity_index,
            path = item.module_path,
        ));
    }

    code.push_str("];\n");
    code
}

fn generate_rust_code(meta: &MetadataJson) -> String {
    let mut code = String::new();

    // Header
    code.push_str(
        "// ============================================================================\n\
         // AUTO-GENERATED FROM metadata.json - DO NOT EDIT MANUALLY\n\
         // ============================================================================\n\n",
    );

    // Imports
    let has_access = meta.access.is_some();
    if has_access {
        code.push_str(
            "#![allow(dead_code)]\n\n\
             use crate::shared::metadata::{\n\
             \x20   EntityMetadataInfo, EntityType, EntityUiMetadata, EntityAiMetadata,\n\
             \x20   FieldMetadata, FieldType, FieldSource, FieldUiMetadata, ValidationRules\n\
             };\n\
             use crate::shared::access::{EntityAccessMeta, ScopeOperation, AccessMode};\n\n",
        );
    } else {
        code.push_str(
            "#![allow(dead_code)]\n\n\
             use crate::shared::metadata::{\n\
             \x20   EntityMetadataInfo, EntityType, EntityUiMetadata, EntityAiMetadata,\n\
             \x20   FieldMetadata, FieldType, FieldSource, FieldUiMetadata, ValidationRules\n\
             };\n\n",
        );
    }

    // Access metadata constant (if present)
    if let Some(access) = &meta.access {
        code.push_str(&generate_access_metadata(access));
        code.push_str("\n\n");
    }

    // Entity metadata constant
    code.push_str(&generate_entity_metadata(meta));
    code.push_str("\n\n");

    // Fields array constant
    code.push_str(&generate_fields_array(&meta.fields));

    code
}

fn generate_access_metadata(access: &AccessJson) -> String {
    let ops: Vec<String> = access
        .operations
        .iter()
        .map(|op| {
            let mode = match op.required_mode.as_str() {
                "all" => "AccessMode::All",
                _ => "AccessMode::Read",
            };
            format!(
                "    ScopeOperation {{ id: \"{}\", required_mode: {} }}",
                op.id, mode
            )
        })
        .collect();

    format!(
        "/// Access scope metadata for this entity\n\
         pub const ACCESS_META: EntityAccessMeta = EntityAccessMeta {{\n\
         \x20   scope_id: \"{}\",\n\
         \x20   operations: &[\n{}\n\x20   ],\n\
         }};",
        access.scope_id,
        ops.join(",\n")
    )
}

fn generate_entity_metadata(meta: &MetadataJson) -> String {
    let access_field = if meta.access.is_some() {
        "    access: Some(&ACCESS_META),\n".to_string()
    } else {
        "    access: None,\n".to_string()
    };

    format!(
        "/// Entity metadata for {} {}\n\
         pub const ENTITY_METADATA: EntityMetadataInfo = EntityMetadataInfo {{\n\
         \x20   schema_version: \"{}\",\n\
         \x20   entity_type: EntityType::{},\n\
         \x20   entity_name: \"{}\",\n\
         \x20   entity_index: \"{}\",\n\
         \x20   collection_name: \"{}\",\n\
         \x20   table_name: {},\n\
         \x20   ui: EntityUiMetadata {{\n\
         \x20       element_name: \"{}\",\n\
         \x20       element_name_en: {},\n\
         \x20       list_name: \"{}\",\n\
         \x20       list_name_en: {},\n\
         \x20       icon: {},\n\
         \x20   }},\n\
         \x20   ai: EntityAiMetadata {{\n\
         \x20       description: \"{}\",\n\
         \x20       questions: &[{}],\n\
         \x20       related: &[{}],\n\
         \x20       tags: &[{}],\n\
         \x20       llm_visible: {},\n\
         \x20   }},\n\
         {}}};",
        meta.entity_name,
        meta.entity_type,
        meta.schema_version,
        to_pascal_case(&meta.entity_type),
        meta.entity_name,
        meta.entity_index,
        meta.collection_name,
        option_str(&meta.table_name),
        escape_string(&meta.ui.element_name),
        option_str(&meta.ui.element_name_en),
        escape_string(&meta.ui.list_name),
        option_str(&meta.ui.list_name_en),
        option_str(&meta.ui.icon),
        escape_string(&meta.ai.description),
        string_array(&meta.ai.questions),
        string_array(&meta.ai.related),
        string_array(&meta.ai.tags),
        meta.ai.llm_visible,
        access_field,
    )
}

fn generate_fields_array(fields: &[FieldJson]) -> String {
    let mut code =
        String::from("/// Field metadata array\npub const FIELDS: &[FieldMetadata] = &[\n");

    for field in fields {
        code.push_str(&generate_field_metadata(field, 1));
        code.push_str(",\n");
    }

    code.push_str("];\n");
    code
}

fn generate_field_metadata(field: &FieldJson, indent: usize) -> String {
    let i = "    ".repeat(indent);
    format!(
        "{i}FieldMetadata {{\n\
         {i}    name: \"{}\",\n\
         {i}    rust_type: \"{}\",\n\
         {i}    field_type: FieldType::{},\n\
         {i}    source: FieldSource::{},\n\
         {i}    ui: FieldUiMetadata {{\n\
         {i}        label: \"{}\",\n\
         {i}        label_en: {},\n\
         {i}        placeholder: {},\n\
         {i}        hint: {},\n\
         {i}        visible_in_list: {},\n\
         {i}        visible_in_form: {},\n\
         {i}        widget: {},\n\
         {i}        column_width: {},\n\
         {i}    }},\n\
         {i}    validation: ValidationRules {{\n\
         {i}        required: {},\n\
         {i}        min: {},\n\
         {i}        max: {},\n\
         {i}        min_length: {},\n\
         {i}        max_length: {},\n\
         {i}        pattern: {},\n\
         {i}        custom_error: {},\n\
         {i}    }},\n\
         {i}    ai_hint: {},\n\
         {i}    physical: {},\n\
         {i}    nested_fields: None,\n\
         {i}    ref_aggregate: {},\n\
         {i}    enum_values: {},\n\
         {i}}}",
        field.name,
        field.rust_type,
        to_pascal_case(&field.field_type),
        to_pascal_case(if field.source.is_empty() {
            "specific"
        } else {
            &field.source
        }),
        escape_string(&field.ui.label),
        option_str(&field.ui.label_en),
        option_str(&field.ui.placeholder),
        option_str(&field.ui.hint),
        field.ui.visible_in_list,
        field.ui.visible_in_form,
        option_str(&field.ui.widget),
        option_u32(field.ui.column_width),
        field.validation.required,
        option_f64(field.validation.min),
        option_f64(field.validation.max),
        option_usize(field.validation.min_length),
        option_usize(field.validation.max_length),
        option_str(&field.validation.pattern),
        option_str(&field.validation.custom_error),
        option_str(&field.ai_hint),
        field.physical,
        option_str(&field.ref_aggregate),
        option_str_array(&field.enum_values),
        i = i
    )
}

// ============================================================================
// Helper functions
// ============================================================================

fn to_pascal_case(s: &str) -> String {
    s.split('_')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(c) => c.to_uppercase().chain(chars).collect(),
            }
        })
        .collect()
}

fn option_str(opt: &Option<String>) -> String {
    match opt {
        Some(s) => format!("Some(\"{}\")", escape_string(s)),
        None => "None".to_string(),
    }
}

fn option_u32(opt: Option<u32>) -> String {
    match opt {
        Some(v) => format!("Some({})", v),
        None => "None".to_string(),
    }
}

fn option_f64(opt: Option<f64>) -> String {
    match opt {
        Some(v) => format!("Some({:.1})", v),
        None => "None".to_string(),
    }
}

fn option_usize(opt: Option<usize>) -> String {
    match opt {
        Some(v) => format!("Some({})", v),
        None => "None".to_string(),
    }
}

fn option_str_array(opt: &Option<Vec<String>>) -> String {
    match opt {
        Some(arr) if !arr.is_empty() => format!("Some(&[{}])", string_array(arr)),
        _ => "None".to_string(),
    }
}

fn string_array(arr: &[String]) -> String {
    if arr.is_empty() {
        return String::new();
    }
    arr.iter()
        .map(|s| format!("\"{}\"", escape_string(s)))
        .collect::<Vec<_>>()
        .join(", ")
}

fn escape_string(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}
