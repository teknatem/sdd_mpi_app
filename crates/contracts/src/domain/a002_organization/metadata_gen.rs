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
    scope_id: "a002_organization",
    operations: &[
    ScopeOperation { id: "list", required_mode: AccessMode::Read },
    ScopeOperation { id: "get", required_mode: AccessMode::Read },
    ScopeOperation { id: "upsert", required_mode: AccessMode::All },
    ScopeOperation { id: "delete", required_mode: AccessMode::All }
    ],
};

/// Entity metadata for Organization aggregate
pub const ENTITY_METADATA: EntityMetadataInfo = EntityMetadataInfo {
    schema_version: "1.0",
    entity_type: EntityType::Aggregate,
    entity_name: "Organization",
    entity_index: "a002",
    collection_name: "organization",
    table_name: Some("a002_organization"),
    ui: EntityUiMetadata {
        element_name: "Организация",
        element_name_en: Some("Organization"),
        list_name: "Организации",
        list_name_en: Some("Organizations"),
        icon: Some("building"),
    },
    ai: EntityAiMetadata {
        description: "Юридические лица и ИП, от имени которых ведётся торговля на маркетплейсах. Импортируются из 1С:УТ. Используются для группировки продаж и финансовой аналитики по юрлицам.",
        questions: &["Какие организации есть в системе?", "Какая выручка по каждой организации?", "Какие подключения к маркетплейсам у организации?"],
        related: &["a001_connection_1c", "a006_connection_mp", "a012_wb_sales"],
        tags: &["ref"],
        llm_visible: true,
    },
    access: Some(&ACCESS_META),
};

/// Field metadata array
pub const FIELDS: &[FieldMetadata] = &[
    FieldMetadata {
        name: "id",
        rust_type: "OrganizationId",
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
            max_length: Some(50),
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
            column_width: Some(250),
        },
        validation: ValidationRules {
            required: true,
            min: None,
            max: None,
            min_length: None,
            max_length: Some(255),
            pattern: None,
            custom_error: None,
        },
        ai_hint: Some("Краткое название организации (используется в отчётах). JOIN с a012_wb_sales через поле organization_id."),
        physical: true,
        nested_fields: None,
        ref_aggregate: None,
        enum_values: None,
    },
    FieldMetadata {
        name: "full_name",
        rust_type: "String",
        field_type: FieldType::Primitive,
        source: FieldSource::Specific,
        ui: FieldUiMetadata {
            label: "Полное наименование",
            label_en: Some("Full Name"),
            placeholder: None,
            hint: None,
            visible_in_list: true,
            visible_in_form: true,
            widget: None,
            column_width: Some(350),
        },
        validation: ValidationRules {
            required: true,
            min: None,
            max: None,
            min_length: None,
            max_length: Some(500),
            pattern: None,
            custom_error: None,
        },
        ai_hint: Some("Полное юридическое наименование организации."),
        physical: true,
        nested_fields: None,
        ref_aggregate: None,
        enum_values: None,
    },
    FieldMetadata {
        name: "inn",
        rust_type: "String",
        field_type: FieldType::Primitive,
        source: FieldSource::Specific,
        ui: FieldUiMetadata {
            label: "ИНН",
            label_en: Some("INN"),
            placeholder: None,
            hint: None,
            visible_in_list: true,
            visible_in_form: true,
            widget: None,
            column_width: Some(130),
        },
        validation: ValidationRules {
            required: false,
            min: None,
            max: None,
            min_length: None,
            max_length: Some(12),
            pattern: None,
            custom_error: None,
        },
        ai_hint: Some("ИНН: 10 цифр для юридических лиц, 12 цифр для ИП."),
        physical: true,
        nested_fields: None,
        ref_aggregate: None,
        enum_values: None,
    },
    FieldMetadata {
        name: "kpp",
        rust_type: "String",
        field_type: FieldType::Primitive,
        source: FieldSource::Specific,
        ui: FieldUiMetadata {
            label: "КПП",
            label_en: Some("KPP"),
            placeholder: None,
            hint: None,
            visible_in_list: false,
            visible_in_form: true,
            widget: None,
            column_width: Some(120),
        },
        validation: ValidationRules {
            required: false,
            min: None,
            max: None,
            min_length: None,
            max_length: Some(9),
            pattern: None,
            custom_error: None,
        },
        ai_hint: Some("КПП: 9 цифр, только для юридических лиц. Пустой для ИП."),
        physical: true,
        nested_fields: None,
        ref_aggregate: None,
        enum_values: None,
    },
];
