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
    scope_id: "a023_purchase_of_goods",
    operations: &[
    ScopeOperation { id: "list", required_mode: AccessMode::Read },
    ScopeOperation { id: "get", required_mode: AccessMode::Read },
    ScopeOperation { id: "upsert", required_mode: AccessMode::All },
    ScopeOperation { id: "delete", required_mode: AccessMode::All }
    ],
};

/// Entity metadata for PurchaseOfGoods aggregate
pub const ENTITY_METADATA: EntityMetadataInfo = EntityMetadataInfo {
    schema_version: "1.0",
    entity_type: EntityType::Aggregate,
    entity_name: "PurchaseOfGoods",
    entity_index: "a023",
    collection_name: "purchase_of_goods",
    table_name: Some("a023_purchase_of_goods"),
    ui: EntityUiMetadata {
        element_name: "Приобретение товаров",
        element_name_en: Some("Purchase of Goods"),
        list_name: "Приобретение товаров",
        list_name_en: Some("Purchases of Goods"),
        icon: Some("shopping-bag"),
    },
    ai: EntityAiMetadata {
        description: "Документ Приобретение товаров и услуг из 1С:Управление торговлей. Содержит номер и дату документа, контрагента-поставщика и строки с товарами (номенклатура, количество, цена, сумма с НДС). Используется для учёта закупочных цен.",
        questions: &["Сколько потратили на закупки?", "Какова закупочная цена товара?", "Какие поставщики поставляли товар?"],
        related: &["a001_connection_1c", "a003_counterparty", "a004_nomenclature"],
        tags: &["purchase"],
        llm_visible: true,
    },
    access: Some(&ACCESS_META),
};

/// Field metadata array
pub const FIELDS: &[FieldMetadata] = &[
    FieldMetadata {
        name: "id",
        rust_type: "PurchaseOfGoodsId",
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
        name: "document_no",
        rust_type: "String",
        field_type: FieldType::Primitive,
        source: FieldSource::Specific,
        ui: FieldUiMetadata {
            label: "Номер документа",
            label_en: Some("Document No"),
            placeholder: None,
            hint: None,
            visible_in_list: true,
            visible_in_form: true,
            widget: None,
            column_width: Some(160),
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
        name: "document_date",
        rust_type: "String",
        field_type: FieldType::Primitive,
        source: FieldSource::Specific,
        ui: FieldUiMetadata {
            label: "Дата документа",
            label_en: Some("Document Date"),
            placeholder: None,
            hint: None,
            visible_in_list: true,
            visible_in_form: true,
            widget: None,
            column_width: Some(140),
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
        name: "counterparty_key",
        rust_type: "String",
        field_type: FieldType::AggregateRef,
        source: FieldSource::Specific,
        ui: FieldUiMetadata {
            label: "Контрагент",
            label_en: Some("Counterparty"),
            placeholder: None,
            hint: None,
            visible_in_list: true,
            visible_in_form: true,
            widget: None,
            column_width: Some(220),
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
        ai_hint: Some("UUID контрагента из 1С — поставщик товаров по данному документу."),
        physical: true,
        nested_fields: None,
        ref_aggregate: Some("a003_counterparty"),
        enum_values: None,
    },
    FieldMetadata {
        name: "lines_json",
        rust_type: "Option<String>",
        field_type: FieldType::Primitive,
        source: FieldSource::Specific,
        ui: FieldUiMetadata {
            label: "Строки (JSON)",
            label_en: Some("Lines (JSON)"),
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
        ai_hint: Some("JSON-массив строк товаров: [{nomenclature_key, quantity, price, amount_with_vat, vat_amount}]."),
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
