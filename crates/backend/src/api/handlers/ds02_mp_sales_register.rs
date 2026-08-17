use crate::shared::error::ApiError;
use axum::{extract::Path, Json};
use contracts::shared::universal_dashboard::{
    DeleteDashboardConfigResponse, DistinctValuesResponse, ExecuteDashboardRequest,
    ExecuteDashboardResponse, GenerateSqlResponse, GetSchemaResponse, ListDashboardConfigsResponse,
    ListSchemasResponse, SaveDashboardConfigRequest, SaveDashboardConfigResponse,
    SavedDashboardConfig, UpdateDashboardConfigRequest,
};

use crate::data_schemes::ds02_mp_sales_register::{schema::DS02_SCHEMA, service};

/// POST /api/ds02/execute
/// Execute a dashboard query
pub async fn execute_dashboard(
    Json(request): Json<ExecuteDashboardRequest>,
) -> Result<Json<ExecuteDashboardResponse>, ApiError> {
    tracing::info!(
        "DS02 Dashboard: Executing query for data source: {}",
        request.config.data_source
    );

    match service::execute_dashboard(request.config).await {
        Ok(response) => {
            tracing::info!(
                "DS02 Dashboard: Returning {} rows, {} columns",
                response.rows.len(),
                response.columns.len()
            );
            Ok(Json(response))
        }
        Err(e) => {
            tracing::error!("DS02 Dashboard: Failed to execute query: {}", e);
            Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into())
        }
    }
}

/// POST /api/ds02/generate-sql
/// Generate SQL query without executing
pub async fn generate_sql(
    Json(request): Json<ExecuteDashboardRequest>,
) -> Result<Json<GenerateSqlResponse>, ApiError> {
    tracing::info!(
        "DS02 Dashboard: Generating SQL for data source: {}",
        request.config.data_source
    );

    match service::generate_sql(request.config).await {
        Ok(response) => {
            tracing::info!("DS02 Dashboard: Generated SQL query successfully");
            Ok(Json(response))
        }
        Err(e) => {
            tracing::error!("DS02 Dashboard: Failed to generate SQL: {}", e);
            Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into())
        }
    }
}

/// GET /api/ds02/schemas
/// List available data source schemas
pub async fn list_schemas() -> Result<Json<ListSchemasResponse>, ApiError> {
    tracing::info!("DS02 Dashboard: Listing available schemas");

    // Use schema registry for listing
    use crate::shared::universal_dashboard::get_registry;
    let schemas = get_registry().list_all();

    Ok(Json(ListSchemasResponse { schemas }))
}

/// GET /api/ds02/schemas/:id
/// Get schema details
pub async fn get_schema(Path(id): Path<String>) -> Result<Json<GetSchemaResponse>, ApiError> {
    tracing::info!("DS02 Dashboard: Getting schema: {}", id);

    use crate::shared::universal_dashboard::get_registry;

    if let Some(schema) = get_registry().get_schema(&id) {
        Ok(Json(GetSchemaResponse { schema }))
    } else {
        tracing::warn!("DS02 Dashboard: Schema not found: {}", id);
        Err(axum::http::StatusCode::NOT_FOUND.into())
    }
}

/// GET /api/ds02/configs
/// List saved dashboard configurations
pub async fn list_configs() -> Result<Json<ListDashboardConfigsResponse>, ApiError> {
    tracing::info!("DS02 Dashboard: Listing saved configurations");

    match service::list_dashboard_configs(Some(DS02_SCHEMA.id)).await {
        Ok(configs) => {
            tracing::info!(
                "DS02 Dashboard: Returning {} saved configurations",
                configs.len()
            );
            Ok(Json(ListDashboardConfigsResponse { configs }))
        }
        Err(e) => {
            tracing::error!("DS02 Dashboard: Failed to list configs: {}", e);
            Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into())
        }
    }
}

/// GET /api/ds02/configs/:id
/// Get a saved dashboard configuration
pub async fn get_config(Path(id): Path<String>) -> Result<Json<SavedDashboardConfig>, ApiError> {
    tracing::info!("DS02 Dashboard: Getting configuration: {}", id);

    match service::get_dashboard_config(&id).await {
        Ok(config) => Ok(Json(config)),
        Err(e) => {
            tracing::error!("DS02 Dashboard: Failed to get config: {}", e);
            Err(axum::http::StatusCode::NOT_FOUND.into())
        }
    }
}

/// POST /api/ds02/configs
/// Save a new dashboard configuration
pub async fn save_config(
    Json(request): Json<SaveDashboardConfigRequest>,
) -> Result<Json<SaveDashboardConfigResponse>, ApiError> {
    tracing::info!("DS02 Dashboard: Saving configuration: {}", request.name);

    match service::save_dashboard_config(request).await {
        Ok(config) => {
            tracing::info!("DS02 Dashboard: Saved configuration with ID: {}", config.id);
            Ok(Json(SaveDashboardConfigResponse {
                id: config.id,
                message: "Configuration saved successfully".to_string(),
            }))
        }
        Err(e) => {
            tracing::error!("DS02 Dashboard: Failed to save config: {}", e);
            Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into())
        }
    }
}

/// PUT /api/ds02/configs/:id
/// Update a dashboard configuration
pub async fn update_config(
    Path(id): Path<String>,
    Json(mut request): Json<UpdateDashboardConfigRequest>,
) -> Result<Json<SaveDashboardConfigResponse>, ApiError> {
    tracing::info!("DS02 Dashboard: Updating configuration: {}", id);

    request.id = id.clone();

    match service::update_dashboard_config(request).await {
        Ok(_) => {
            tracing::info!("DS02 Dashboard: Updated configuration: {}", id);
            Ok(Json(SaveDashboardConfigResponse {
                id,
                message: "Configuration updated successfully".to_string(),
            }))
        }
        Err(e) => {
            tracing::error!("DS02 Dashboard: Failed to update config: {}", e);
            Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into())
        }
    }
}

/// DELETE /api/ds02/configs/:id
/// Delete a dashboard configuration
pub async fn delete_config(
    Path(id): Path<String>,
) -> Result<Json<DeleteDashboardConfigResponse>, ApiError> {
    tracing::info!("DS02 Dashboard: Deleting configuration: {}", id);

    match service::delete_dashboard_config(&id).await {
        Ok(_) => {
            tracing::info!("DS02 Dashboard: Deleted configuration: {}", id);
            Ok(Json(DeleteDashboardConfigResponse {
                success: true,
                message: "Configuration deleted successfully".to_string(),
            }))
        }
        Err(e) => {
            tracing::error!("DS02 Dashboard: Failed to delete config: {}", e);
            Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into())
        }
    }
}

/// GET /api/ds02/schemas/:schema_id/fields/:field_id/values
/// Get distinct values for a field
pub async fn get_distinct_values(
    Path((schema_id, field_id)): Path<(String, String)>,
) -> Result<Json<DistinctValuesResponse>, ApiError> {
    tracing::info!(
        "DS02 Dashboard: Getting distinct values for field {} in schema {}",
        field_id,
        schema_id
    );

    use crate::shared::universal_dashboard::get_registry;

    // Validate schema exists
    if !get_registry().has_schema(&schema_id) {
        tracing::warn!("DS02 Dashboard: Schema not found: {}", schema_id);
        return Err(axum::http::StatusCode::NOT_FOUND.into());
    }

    match service::get_distinct_values(&schema_id, &field_id, Some(100)).await {
        Ok(values) => {
            tracing::info!(
                "DS02 Dashboard: Returning {} distinct values for field {}",
                values.len(),
                field_id
            );
            Ok(Json(DistinctValuesResponse { field_id, values }))
        }
        Err(e) => {
            tracing::error!("DS02 Dashboard: Failed to get distinct values: {}", e);
            Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into())
        }
    }
}
