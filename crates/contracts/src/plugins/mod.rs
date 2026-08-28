//! Shared contracts for the plugin subsystem.
//!
//! A plugin is a self-contained interpreted artifact (`PluginBundle`) with a
//! manifest, parameters, data binding, client/server JavaScript modules, view
//! metadata, styles, and assets. It is stored and executed without rebuilding
//! the application.

pub mod bundle;
pub mod document;
pub mod publish;
pub mod runs;

pub use bundle::{
    is_read_only_sql, is_valid_plugin_code, is_valid_resource_name, DataBinding, ParamSpec,
    ParamType, PluginBundle, PluginCapability, PluginClientKit, PluginDataMode, PluginDataSource,
    PluginDataViewContext, PluginDefinition, PluginError, PluginInvokeRequest, PluginListItem,
    PluginManifest, PluginRunContext, PluginRuntime, PluginSchemaAggregate, PluginSchemaFilter,
    PluginSchemaFilterOperator, PluginSchemaMetric, PluginSchemaSortDirection,
    PluginSchemaSortRule, PluginSmokeFailure, PluginSmokeMethod, PluginSmokeReport,
    PluginSmokeRequest, PluginSnapshotMeta, PluginStatus, PluginUpsert, PluginValidateReport,
    ViewSpec, Widget, WidgetKind, DEFAULT_CLIENT_KITS,
};
pub use document::{
    PluginDocumentBinding, PluginDocumentResponse, PluginDocumentSaveRequest,
    PluginDocumentSaveResponse, PluginDocumentTarget,
};
pub use publish::{
    PluginApplyUpdateRequest, PluginCatalog, PluginCatalogEntry, PluginPublishResult,
    PluginUpdateStatus,
};
pub use runs::{
    PluginHealth, PluginModeStats, PluginRunBrief, PluginRunRecord, PluginRunSummary, PluginStats,
    StageCount,
};
