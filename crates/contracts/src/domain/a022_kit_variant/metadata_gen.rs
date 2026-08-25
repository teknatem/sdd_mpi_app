// ============================================================================
// AUTO-GENERATED FROM metadata.json - DO NOT EDIT MANUALLY
// ============================================================================

#![cfg_attr(rustfmt, rustfmt::skip)]

#![allow(dead_code)]

use crate::shared::metadata::{
    EntityMetadataInfo, EntityType, EntityUiMetadata, EntityAiMetadata,
    FieldMetadata, FieldType, FieldSource, FieldUiMetadata, ValidationRules
};
use crate::shared::access::{EntityAccessMeta, ScopeOperation, AccessMode};

/// Access scope metadata for this entity
pub const ACCESS_META: EntityAccessMeta = EntityAccessMeta {
    scope_id: "a022_kit_variant",
    operations: &[
    ScopeOperation { id: "list", required_mode: AccessMode::Read },
    ScopeOperation { id: "get", required_mode: AccessMode::Read },
    ScopeOperation { id: "upsert", required_mode: AccessMode::All },
    ScopeOperation { id: "delete", required_mode: AccessMode::All }
    ],
};

/// Entity metadata for KitVariant aggregate
pub const ENTITY_METADATA: EntityMetadataInfo = EntityMetadataInfo {
    schema_version: "1.0",
    entity_type: EntityType::Aggregate,
    entity_name: "KitVariant",
    entity_index: "a022",
    collection_name: "kit_variant",
    table_name: Some("a022_kit_variant"),
    ui: EntityUiMetadata {
        element_name: "Вариант комплектации",
        element_name_en: Some("Kit Variant"),
        list_name: "Варианты комплектации",
        list_name_en: Some("Kit Variants"),
        icon: Some("layers"),
    },
    ai: EntityAiMetadata {
        description: "Вариант комплектации номенклатуры из 1С:Управление торговлей. Описывает состав набора (kit) — какая номенклатура и в каком количестве входит в производимый товар. Используется для расчёта себестоимости комплектов.",
        questions: &["Из каких компонентов состоит комплект?", "Какие товары входят в набор?", "Как рассчитать себестоимость набора?"],
        related: &["a004_nomenclature", "a001_connection_1c", "a021_production_output"],
        tags: &["ref", "production"],
        llm_visible: true,
    },
    access: Some(&ACCESS_META),
};

/// Field metadata array
pub const FIELDS: &[FieldMetadata] = &[
    FieldMetadata {
        name: "id",
        rust_type: "KitVariantId",
        field_type: FieldType::Primitive,
        source: FieldSource::Base,
        ui: FieldUiMetadata {
            label: "ID",
            label_en: Some("ID"),
            placeholder: None,
            hint: None,
            visible_in_list: false,
            visible_in_form: false,
            widget: None,
            column_width: None,
        },
        validation: ValidationRules {
            required: true,
            min: None,
            max: None,
            min_length: None,
            max_length: None,
            pattern: None,
            custom_error: None,
        },
        ai_hint: None,
        physical: true,
        nested_fields: None,
        ref_aggregate: None,
        enum_values: None,
    },
    FieldMetadata {
        name: "code",
        rust_type: "String",
        field_type: FieldType::Primitive,
        source: FieldSource::Base,
        ui: FieldUiMetadata {
            label: "Код",
            label_en: Some("Code"),
            placeholder: None,
            hint: None,
            visible_in_list: true,
            visible_in_form: true,
            widget: None,
            column_width: Some(120),
        },
        validation: ValidationRules {
            required: true,
            min: None,
            max: None,
            min_length: None,
            max_length: None,
            pattern: None,
            custom_error: None,
        },
        ai_hint: None,
        physical: true,
        nested_fields: None,
        ref_aggregate: None,
        enum_values: None,
    },
    FieldMetadata {
        name: "description",
        rust_type: "String",
        field_type: FieldType::Primitive,
        source: FieldSource::Base,
        ui: FieldUiMetadata {
            label: "Наименование",
            label_en: Some("Name"),
            placeholder: None,
            hint: None,
            visible_in_list: true,
            visible_in_form: true,
            widget: None,
            column_width: Some(280),
        },
        validation: ValidationRules {
            required: true,
            min: None,
            max: None,
            min_length: None,
            max_length: None,
            pattern: None,
            custom_error: None,
        },
        ai_hint: None,
        physical: true,
        nested_fields: None,
        ref_aggregate: None,
        enum_values: None,
    },
    FieldMetadata {
        name: "owner_ref",
        rust_type: "Option<String>",
        field_type: FieldType::AggregateRef,
        source: FieldSource::Specific,
        ui: FieldUiMetadata {
            label: "Номенклатура-владелец",
            label_en: Some("Owner Nomenclature"),
            placeholder: None,
            hint: None,
            visible_in_list: true,
            visible_in_form: true,
            widget: None,
            column_width: Some(200),
        },
        validation: ValidationRules {
            required: false,
            min: None,
            max: None,
            min_length: None,
            max_length: None,
            pattern: None,
            custom_error: None,
        },
        ai_hint: Some("UUID производимой номенклатуры, для которой описан этот вариант комплектации."),
        physical: true,
        nested_fields: None,
        ref_aggregate: Some("a004_nomenclature"),
        enum_values: None,
    },
    FieldMetadata {
        name: "goods_json",
        rust_type: "Option<String>",
        field_type: FieldType::Primitive,
        source: FieldSource::Specific,
        ui: FieldUiMetadata {
            label: "Состав набора (JSON)",
            label_en: Some("Goods (JSON)"),
            placeholder: None,
            hint: None,
            visible_in_list: false,
            visible_in_form: false,
            widget: None,
            column_width: None,
        },
        validation: ValidationRules {
            required: false,
            min: None,
            max: None,
            min_length: None,
            max_length: None,
            pattern: None,
            custom_error: None,
        },
        ai_hint: Some("JSON-массив [{nomenclature_ref, quantity}] — компоненты набора."),
        physical: true,
        nested_fields: None,
        ref_aggregate: None,
        enum_values: None,
    },
    FieldMetadata {
        name: "connection_id",
        rust_type: "String",
        field_type: FieldType::AggregateRef,
        source: FieldSource::Specific,
        ui: FieldUiMetadata {
            label: "Подключение 1С",
            label_en: Some("1C Connection"),
            placeholder: None,
            hint: None,
            visible_in_list: false,
            visible_in_form: true,
            widget: None,
            column_width: None,
        },
        validation: ValidationRules {
            required: true,
            min: None,
            max: None,
            min_length: None,
            max_length: None,
            pattern: None,
            custom_error: None,
        },
        ai_hint: None,
        physical: true,
        nested_fields: None,
        ref_aggregate: Some("a001_connection_1c"),
        enum_values: None,
    },
];
