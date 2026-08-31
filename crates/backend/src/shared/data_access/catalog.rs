use crate::data_view::DataViewRegistry;
use crate::shared::universal_dashboard::get_registry;
use contracts::shared::data_view::FilterKind;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DataSourceKind {
    Base,
    Dataview,
    Raw,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DataSourceRef {
    pub kind: DataSourceKind,
    pub id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityField {
    pub id: String,
    pub name: String,
    pub value_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub db_column: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SourceCapabilities {
    pub dimensions: Vec<CapabilityField>,
    pub metrics: Vec<CapabilityField>,
    pub filters: Vec<CapabilityField>,
    pub supports_two_periods: bool,
    pub ad_hoc: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataSourceCatalogItem {
    pub kind: DataSourceKind,
    pub id: String,
    pub name: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub table: Option<String>,
    pub capabilities: SourceCapabilities,
    pub related_sources: Vec<DataSourceRef>,
}

fn filter_kind_name(kind: &FilterKind) -> &'static str {
    match kind {
        FilterKind::DateRange { .. } => "date_range",
        FilterKind::MultiSelect { .. } => "multi_select",
        FilterKind::Select { .. } => "select",
        FilterKind::Text => "text",
    }
}

pub fn list_sources(kind: Option<DataSourceKind>) -> Vec<DataSourceCatalogItem> {
    let schema_registry = get_registry();
    let view_registry = DataViewRegistry::new();
    let mut items = Vec::new();
    let mut source_tables: HashMap<(DataSourceKind, String), HashSet<String>> = HashMap::new();

    for info in schema_registry.list_all() {
        let Some(schema) = schema_registry.get_schema(&info.id) else {
            continue;
        };
        let fields =
            |predicate: fn(&contracts::shared::universal_dashboard::FieldDefOwned) -> bool| {
                schema
                    .fields
                    .iter()
                    .filter(|field| predicate(field))
                    .map(|field| CapabilityField {
                        id: field.id.clone(),
                        name: field.name.clone(),
                        value_type: field.value_type.canonical_name(),
                        db_column: Some(field.db_column.clone()),
                    })
                    .collect::<Vec<_>>()
            };
        let key = (DataSourceKind::Base, info.id.clone());
        source_tables.insert(key, HashSet::from([info.table_name.clone()]));
        let description = schema_registry
            .get_ai_description(&info.id)
            .map(str::to_string)
            .unwrap_or_else(|| {
                format!(
                    "Безопасная декларативная схема для ad-hoc срезов таблицы {}",
                    info.table_name
                )
            });
        items.push(DataSourceCatalogItem {
            kind: DataSourceKind::Base,
            id: info.id,
            name: info.name,
            description,
            table: Some(info.table_name),
            capabilities: SourceCapabilities {
                dimensions: fields(|field| field.can_group),
                metrics: fields(|field| field.can_aggregate),
                filters: fields(|field| field.can_filter),
                supports_two_periods: false,
                ad_hoc: true,
            },
            related_sources: Vec::new(),
        });
    }

    for meta in view_registry.list_meta() {
        let filters = view_registry
            .resolve_filters(&meta.id)
            .into_iter()
            .map(|filter| CapabilityField {
                id: filter.id,
                name: filter.label,
                value_type: filter_kind_name(&filter.kind).to_string(),
                db_column: None,
            })
            .collect();
        let key = (DataSourceKind::Dataview, meta.id.clone());
        source_tables.insert(key, meta.data_sources.iter().cloned().collect());
        items.push(DataSourceCatalogItem {
            kind: DataSourceKind::Dataview,
            id: meta.id.clone(),
            name: meta.name.clone(),
            description: meta.ai_description.clone(),
            table: None,
            capabilities: SourceCapabilities {
                dimensions: meta
                    .available_dimensions
                    .iter()
                    .map(|dimension| CapabilityField {
                        id: dimension.id.clone(),
                        name: dimension.label.clone(),
                        value_type: "dimension".to_string(),
                        db_column: None,
                    })
                    .collect(),
                metrics: meta
                    .available_resources
                    .iter()
                    .map(|resource| CapabilityField {
                        id: resource.id.clone(),
                        name: resource.label.clone(),
                        value_type: resource.unit.clone(),
                        db_column: None,
                    })
                    .collect(),
                filters,
                supports_two_periods: true,
                ad_hoc: false,
            },
            related_sources: Vec::new(),
        });
    }

    items.push(DataSourceCatalogItem {
        kind: DataSourceKind::Raw,
        id: "raw_sql".to_string(),
        name: "Raw SQL fallback".to_string(),
        description:
            "Ограниченный SELECT для нестандартных запросов, не покрытых схемами и DataView"
                .to_string(),
        table: None,
        capabilities: SourceCapabilities {
            ad_hoc: true,
            ..SourceCapabilities::default()
        },
        related_sources: Vec::new(),
    });

    for item in &mut items {
        let Some(tables) = source_tables.get(&(item.kind, item.id.clone())) else {
            continue;
        };
        let mut related: Vec<DataSourceRef> = source_tables
            .iter()
            .filter(|((other_kind, other_id), other_tables)| {
                (*other_kind != item.kind || *other_id != item.id)
                    && !tables.is_disjoint(other_tables)
            })
            .map(|((other_kind, other_id), _)| DataSourceRef {
                kind: *other_kind,
                id: other_id.clone(),
            })
            .collect();
        related.sort_by(|a, b| a.id.cmp(&b.id));
        item.related_sources = related;
    }

    items.retain(|item| kind.map_or(true, |expected| item.kind == expected));
    items.sort_by(|a, b| a.id.cmp(&b.id));
    items
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_links_overlapping_schema_and_view() {
        crate::composition::install_all();
        let items = list_sources(None);
        let ds03 = items
            .iter()
            .find(|item| item.id == "ds03_p904_sales")
            .unwrap();
        assert!(ds03
            .related_sources
            .iter()
            .any(|source| source.id == "dv001_revenue"));
    }

    #[test]
    fn safe_connection_schema_exposes_id_but_not_credentials() {
        crate::composition::install_all();
        let items = list_sources(Some(DataSourceKind::Base));
        let a006 = items.iter().find(|item| item.id == "a006").unwrap();
        let ids: HashSet<_> = a006
            .capabilities
            .dimensions
            .iter()
            .map(|field| field.id.as_str())
            .collect();
        assert!(ids.contains("id"));
        assert!(!ids.contains("api_key"));
        assert!(!ids.contains("api_key_stats"));
    }
}
