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
    scope_id: "a005_marketplace",
    operations: &[
    ScopeOperation { id: "list", required_mode: AccessMode::Read },
    ScopeOperation { id: "get", required_mode: AccessMode::Read },
    ScopeOperation { id: "upsert", required_mode: AccessMode::All },
    ScopeOperation { id: "delete", required_mode: AccessMode::All }
    ],
};

/// Entity metadata for Marketplace aggregate
pub const ENTITY_METADATA: EntityMetadataInfo = EntityMetadataInfo {
    schema_version: "1.0",
    entity_type: EntityType::Aggregate,
    entity_name: "Marketplace",
    entity_index: "a005",
    collection_name: "marketplace",
    table_name: Some("a005_marketplace"),
    ui: EntityUiMetadata {
        element_name: "Маркетплейс",
        element_name_en: Some("Marketplace"),
        list_name: "Маркетплейсы",
        list_name_en: Some("Marketplaces"),
        icon: Some("store"),
    },
    ai: EntityAiMetadata {
        description: "Справочник торговых площадок: Wildberries, Ozon, Яндекс.Маркет. Системные записи, создаются при инициализации. Используется как справочник типов маркетплейсов. Конкретные магазины описываются в a006_connection_mp.",
        questions: &["Какие маркетплейсы подключены?", "Сколько магазинов на каждом маркетплейсе?"],
        related: &["a006_connection_mp"],
        tags: &["ref"],
        llm_visible: true,
    },
    access: Some(&ACCESS_META),
};

/// Field metadata array
pub const FIELDS: &[FieldMetadata] = &[
    FieldMetadata {
        name: "id",
        rust_type: "MarketplaceId",
        field_type: FieldType::Primitive,
        source: FieldSource::Base,
        ui: FieldUiMetadata {
            label: "ID",
            label_en: None,
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
        ai_hint: Some("UUID маркетплейса. Используется как FK в a006_connection_mp (поле marketplace_id)."),
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
            label_en: None,
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
            column_width: Some(200),
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
        ai_hint: Some("Название маркетплейса: 'Wildberries', 'Ozon', 'Яндекс.Маркет'."),
        physical: true,
        nested_fields: None,
        ref_aggregate: None,
        enum_values: None,
    },
    FieldMetadata {
        name: "marketplace_type",
        rust_type: "Option<MarketplaceType>",
        field_type: FieldType::Enum,
        source: FieldSource::Specific,
        ui: FieldUiMetadata {
            label: "Тип",
            label_en: Some("Type"),
            placeholder: None,
            hint: None,
            visible_in_list: true,
            visible_in_form: true,
            widget: None,
            column_width: Some(150),
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
        ai_hint: Some("Тип маркетплейса. Значения: 'Wildberries', 'Озон', 'Яндекс.Маркет'."),
        physical: true,
        nested_fields: None,
        ref_aggregate: None,
        enum_values: Some(&["Wildberries", "Озон", "Яндекс.Маркет"]),
    },
    FieldMetadata {
        name: "acquiring_fee_pro",
        rust_type: "f64",
        field_type: FieldType::Primitive,
        source: FieldSource::Specific,
        ui: FieldUiMetadata {
            label: "Процент эквайринга",
            label_en: Some("Acquiring Fee %"),
            placeholder: None,
            hint: None,
            visible_in_list: true,
            visible_in_form: true,
            widget: None,
            column_width: Some(150),
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
        ai_hint: Some("Плановый процент комиссии за эквайринг (платёжную систему). Используется в расчёте плановой прибыли."),
        physical: true,
        nested_fields: None,
        ref_aggregate: None,
        enum_values: None,
    },
];
