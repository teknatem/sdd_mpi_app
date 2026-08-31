//! Schema registry for pivot tables
//!
//! Central registry that combines auto-generated schemas from metadata
//! and custom schemas defined in code.

use std::collections::HashMap;

use contracts::shared::metadata::{EntityMetadataInfo, FieldMetadata};
use contracts::shared::universal_dashboard::{
    DataSourceSchema, DataSourceSchemaOwned, SchemaInfo, SchemaSource,
};

use super::metadata_converter::{metadata_to_pivot_schema, RefResolver};

/// Information about a registered entity with metadata
pub struct RegisteredEntity {
    pub entity: &'static EntityMetadataInfo,
    pub fields: &'static [FieldMetadata],
}

/// Schema registry combining auto and custom schemas
pub struct SchemaRegistry {
    /// Auto-generated schemas from entity metadata
    auto_schemas: HashMap<String, RegisteredEntity>,
    /// Custom schemas defined in code
    custom_schemas: HashMap<String, CustomSchemaEntry>,
    /// Исторические имена схем → канонические.
    aliases: HashMap<&'static str, &'static str>,
    /// Индекс агрегата → таблица, для ссылок без метаданных.
    ref_fallback: HashMap<&'static str, &'static str>,
}

/// Entry for a custom schema
struct CustomSchemaEntry {
    schema: &'static DataSourceSchema,
    table_name: &'static str,
}

impl SchemaRegistry {
    fn canonical_schema_id<'a>(&self, id: &'a str) -> &'a str
    where
        'static: 'a,
    {
        self.aliases.get(id).copied().unwrap_or(id)
    }

    /// Собрать реестр из объявленного состава.
    ///
    /// Состав приходит из composition root: перечисляя схемы здесь, движок
    /// сводных знал бы имена конкретных таблиц и агрегатов.
    pub fn build(
        custom: Vec<(&'static DataSourceSchema, &'static str)>,
        auto: Vec<(&'static EntityMetadataInfo, &'static [FieldMetadata])>,
        aliases: Vec<(&'static str, &'static str)>,
        ref_fallback: Vec<(&'static str, &'static str)>,
    ) -> Self {
        let mut registry = Self {
            auto_schemas: HashMap::new(),
            custom_schemas: HashMap::new(),
            aliases: aliases.into_iter().collect(),
            ref_fallback: ref_fallback.into_iter().collect(),
        };

        for (schema, table_name) in custom {
            registry.register_custom_schema(schema, table_name);
        }

        // Только явно одобренные проекции метаданных исполняемы: сущности с
        // кредами не регистрируются оптом никогда.
        for (entity, fields) in auto {
            registry.register_metadata_schema(entity, fields);
        }

        registry
    }

    /// Register custom schema
    fn register_custom_schema(
        &mut self,
        schema: &'static DataSourceSchema,
        table_name: &'static str,
    ) {
        self.custom_schemas.insert(
            schema.id.to_string(),
            CustomSchemaEntry { schema, table_name },
        );
    }

    fn register_metadata_schema(
        &mut self,
        entity: &'static EntityMetadataInfo,
        fields: &'static [FieldMetadata],
    ) {
        if entity.table_name.is_some() {
            self.auto_schemas.insert(
                entity.entity_index.to_string(),
                RegisteredEntity { entity, fields },
            );
        }
    }

    /// List all available schemas
    pub fn list_all(&self) -> Vec<SchemaInfo> {
        let mut result = Vec::new();

        // Add custom schemas
        for (id, entry) in &self.custom_schemas {
            result.push(SchemaInfo {
                id: id.clone(),
                name: entry.schema.name.to_string(),
                source: SchemaSource::Custom,
                table_name: entry.table_name.to_string(),
            });
        }

        // Add auto schemas
        for (id, entry) in &self.auto_schemas {
            if let Some(table_name) = entry.entity.table_name {
                result.push(SchemaInfo {
                    id: id.clone(),
                    name: entry.entity.ui.list_name.to_string(),
                    source: SchemaSource::Auto,
                    table_name: table_name.to_string(),
                });
            }
        }

        // Sort by id
        result.sort_by(|a, b| a.id.cmp(&b.id));
        result
    }

    /// Get schema by ID
    pub fn get_schema(&self, id: &str) -> Option<DataSourceSchemaOwned> {
        let id = self.canonical_schema_id(id);
        // Check custom schemas first
        if let Some(entry) = self.custom_schemas.get(id) {
            return Some(entry.schema.into());
        }

        // Check auto schemas
        if let Some(entry) = self.auto_schemas.get(id) {
            return Some(metadata_to_pivot_schema(entry.entity, entry.fields, self));
        }

        None
    }

    /// Get table name for schema
    pub fn get_table_name(&self, schema_id: &str) -> Option<String> {
        let schema_id = self.canonical_schema_id(schema_id);
        // Check custom schemas
        if let Some(entry) = self.custom_schemas.get(schema_id) {
            return Some(entry.table_name.to_string());
        }

        // Check auto schemas
        if let Some(entry) = self.auto_schemas.get(schema_id) {
            return entry.entity.table_name.map(|s| s.to_string());
        }

        None
    }

    /// Business context used to describe and search auto-generated schemas.
    pub fn get_ai_description(&self, schema_id: &str) -> Option<&'static str> {
        let schema_id = self.canonical_schema_id(schema_id);
        self.auto_schemas
            .get(schema_id)
            .map(|entry| entry.entity.ai.description)
    }

    /// Check if schema exists
    pub fn has_schema(&self, id: &str) -> bool {
        let id = self.canonical_schema_id(id);
        self.custom_schemas.contains_key(id) || self.auto_schemas.contains_key(id)
    }

    /// Get entity metadata for auto schema
    pub fn get_entity_metadata(&self, id: &str) -> Option<&RegisteredEntity> {
        self.auto_schemas.get(id)
    }
}

/// Implement RefResolver for SchemaRegistry
impl RefResolver for SchemaRegistry {
    fn resolve_ref(&self, aggregate_index: &str) -> (Option<String>, Option<String>) {
        // Look up the referenced aggregate in auto schemas
        if let Some(entry) = self.auto_schemas.get(aggregate_index) {
            let table = entry.entity.table_name.map(|s| s.to_string());
            // Standard display column is "description"
            let display = Some("description".to_string());
            return (table, display);
        }

        // Фолбэк для агрегатов без метаданных: таблицу называет состав,
        // а не этот модуль.
        let table_name = self
            .ref_fallback
            .get(aggregate_index)
            .map(|table| (*table).to_string());

        (table_name, Some("description".to_string()))
    }
}

/// Global schema registry instance
static REGISTRY: std::sync::OnceLock<SchemaRegistry> = std::sync::OnceLock::new();

/// Установить реестр схем. Зовётся один раз из `composition::install_all()`.
///
/// # Panics
/// При повторной установке: второй состав означал бы, что «Конструктор
/// запросов» и «Схемы таблиц» показывают разные наборы источников.
pub fn install(registry: SchemaRegistry) {
    if REGISTRY.set(registry).is_err() {
        panic!("реестр схем уже установлен");
    }
}

/// Get global schema registry
pub fn get_registry() -> &'static SchemaRegistry {
    REGISTRY
        .get()
        .expect("реестр схем не установлен: composition::install_all() не был вызван")
}
