use crate::shared::error::ApiError;
use axum::{extract::Query, Json};
use serde::{Deserialize, Serialize};

use crate::general_ledger::detail_links::{descriptor_for_resource_table, GlDetailLinkKind};
use crate::general_ledger::drilldown_dimensions::{
    dimension_signature_for_turnover, dimensions_catalog, dimensions_for_turnover,
    dimensions_for_turnover_at_layer, projections_for_cell, source_short_label,
    table_provides_dimension,
};
use crate::general_ledger::drilldown_session_repository;
use crate::general_ledger::report_repository;
use crate::general_ledger::resource_detail;
use crate::general_ledger::turnover_registry::{get_turnover_class, TURNOVER_CLASSES};
use contracts::general_ledger::{
    GeneralLedgerEntryDto, GeneralLedgerTurnoverDto, GlAccountViewQuery, GlAccountViewResponse,
    GlDimensionsCatalogResponse, GlDimensionsResponse, GlDrilldownQuery, GlDrilldownResponse,
    GlDrilldownSessionCreate, GlDrilldownSessionCreateResponse, GlDrilldownSessionRecord,
    GlEntitiesResponse, GlEntityDto, GlLayerDto, GlLayerTurnoverMatrixResponse, GlLayersResponse,
    GlMatrixCell, GlMatrixDimension, GlMatrixLayer, GlMatrixProjection, GlMatrixTurnover,
    GlReportQuery, GlReportResponse, GlResourceDetailResponse, SupplierBalanceQuery,
    SupplierBalanceResponse, WbWeeklyReconciliationQuery, WbWeeklyReconciliationResponse,
    GL_ENTITY_CLASSES, GL_LAYER_CLASSES,
};

#[derive(Debug, Deserialize)]
pub struct GeneralLedgerQuery {
    pub date_from: Option<String>,
    pub date_to: Option<String>,
    pub registrator_ref: Option<String>,
    pub registrator_type: Option<String>,
    pub layer: Option<String>,
    pub entity: Option<String>,
    pub turnover_code: Option<String>,
    pub connection_mp_ref: Option<String>,
    pub debit_account: Option<String>,
    pub credit_account: Option<String>,
    pub sort_by: Option<String>,
    pub sort_desc: Option<bool>,
    pub limit: Option<u64>,
    pub offset: Option<u64>,
}

#[derive(Debug, Serialize)]
pub struct GeneralLedgerListResponse {
    pub entries: Vec<GeneralLedgerEntryDto>,
    pub total: usize,
    pub page: usize,
    pub page_size: usize,
    pub total_pages: usize,
}

#[derive(Debug, Serialize)]
pub struct GeneralLedgerTurnoverListResponse {
    pub items: Vec<GeneralLedgerTurnoverDto>,
    pub total: usize,
}

pub async fn list(
    Query(q): Query<GeneralLedgerQuery>,
) -> Result<Json<GeneralLedgerListResponse>, ApiError> {
    let page_size = q.limit.unwrap_or(100) as usize;
    let offset = q.offset.unwrap_or(0) as usize;
    let page = if page_size > 0 { offset / page_size } else { 0 };
    let sort_desc = q.sort_desc.unwrap_or(true);

    let total = crate::general_ledger::repository::count_with_filters(
        q.date_from.clone(),
        q.date_to.clone(),
        q.registrator_ref.clone(),
        q.registrator_type.clone(),
        q.layer.clone(),
        q.entity.clone(),
        q.debit_account.clone(),
        q.credit_account.clone(),
        q.turnover_code.clone(),
        q.connection_mp_ref.clone(),
    )
    .await
    .map_err(|e| {
        tracing::error!("general_ledger count error: {}", e);
        ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
    })?;

    let rows = crate::general_ledger::repository::list_with_filters(
        q.date_from,
        q.date_to,
        q.registrator_ref,
        q.registrator_type,
        q.layer,
        q.entity,
        q.debit_account,
        q.credit_account,
        q.turnover_code,
        q.connection_mp_ref,
        q.sort_by,
        sort_desc,
        Some(offset as u64),
        Some(page_size as u64),
    )
    .await
    .map_err(|e| {
        tracing::error!("general_ledger list error: {}", e);
        ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
    })?;

    let entries: Vec<GeneralLedgerEntryDto> = rows.into_iter().map(to_dto).collect();

    let total = total as usize;
    let total_pages = if page_size > 0 {
        total.div_ceil(page_size)
    } else {
        0
    };

    Ok(Json(GeneralLedgerListResponse {
        entries,
        total,
        page,
        page_size,
        total_pages,
    }))
}

pub async fn get_by_id(
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<Json<GeneralLedgerEntryDto>, ApiError> {
    let item = crate::general_ledger::repository::get_by_id(&id)
        .await
        .map_err(|e| {
            tracing::error!("general_ledger get_by_id error: {}", e);
            ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        })?
        .ok_or(ApiError::from(axum::http::StatusCode::NOT_FOUND))?;

    Ok(Json(to_dto(item)))
}

pub async fn get_resource_details(
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<Json<GlResourceDetailResponse>, ApiError> {
    resource_detail::get_resource_details(&id)
        .await
        .map(Json)
        .map_err(|e| {
            tracing::error!("general_ledger resource_details error: {e}");
            ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        })
}

pub async fn get_turnover_by_code(
    axum::extract::Path(code): axum::extract::Path<String>,
) -> Result<Json<GeneralLedgerTurnoverDto>, ApiError> {
    let item =
        get_turnover_class(&code).ok_or(ApiError::from(axum::http::StatusCode::NOT_FOUND))?;
    let counts = crate::general_ledger::repository::count_grouped_by_turnover_code()
        .await
        .map_err(|e| {
            tracing::error!("general_ledger turnover counts error: {}", e);
            ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        })?;

    Ok(Json(GeneralLedgerTurnoverDto {
        code: item.code.to_string(),
        name: item.name.to_string(),
        description: item.description.to_string(),
        llm_description: item.llm_description.to_string(),
        scope: item.scope,
        value_kind: item.value_kind,
        agg_kind: item.agg_kind,
        selection_rule: item.selection_rule,
        sign_policy: item.sign_policy,
        report_group: item.report_group,
        aliases: item.aliases.iter().map(|value| value.to_string()).collect(),
        source_examples: item
            .source_examples
            .iter()
            .map(|value| value.to_string())
            .collect(),
        formula_hint: item.formula_hint.to_string(),
        notes: item.notes.to_string(),
        debit_account: item.debit_account.to_string(),
        credit_account: item.credit_account.to_string(),
        generates_journal_entry: item.generates_journal_entry,
        journal_comment: item.journal_comment.to_string(),
        gl_entries_count: counts.get(item.code).copied().unwrap_or(0),
        available_dimensions: dimensions_for_turnover(item.code),
        dimension_signature: dimension_signature_for_turnover(item.code),
    }))
}

pub async fn list_turnovers() -> Result<Json<GeneralLedgerTurnoverListResponse>, ApiError> {
    let counts = crate::general_ledger::repository::count_grouped_by_turnover_code()
        .await
        .map_err(|e| {
            tracing::error!("general_ledger turnover counts error: {}", e);
            ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        })?;

    let mut items = TURNOVER_CLASSES
        .iter()
        .map(|item| GeneralLedgerTurnoverDto {
            code: item.code.to_string(),
            name: item.name.to_string(),
            description: item.description.to_string(),
            llm_description: item.llm_description.to_string(),
            scope: item.scope,
            value_kind: item.value_kind,
            agg_kind: item.agg_kind,
            selection_rule: item.selection_rule,
            sign_policy: item.sign_policy,
            report_group: item.report_group,
            aliases: item.aliases.iter().map(|value| value.to_string()).collect(),
            source_examples: item
                .source_examples
                .iter()
                .map(|value| value.to_string())
                .collect(),
            formula_hint: item.formula_hint.to_string(),
            notes: item.notes.to_string(),
            debit_account: item.debit_account.to_string(),
            credit_account: item.credit_account.to_string(),
            generates_journal_entry: item.generates_journal_entry,
            journal_comment: item.journal_comment.to_string(),
            gl_entries_count: counts.get(item.code).copied().unwrap_or(0),
            available_dimensions: dimensions_for_turnover(item.code),
            dimension_signature: dimension_signature_for_turnover(item.code),
        })
        .collect::<Vec<_>>();

    items.sort_by(|left, right| {
        right
            .gl_entries_count
            .cmp(&left.gl_entries_count)
            .then_with(|| left.code.cmp(&right.code))
    });

    let total = items.len();

    Ok(Json(GeneralLedgerTurnoverListResponse { items, total }))
}

pub async fn dimensions_catalog_index() -> Json<GlDimensionsCatalogResponse> {
    let items = dimensions_catalog();
    let total = items.len();
    Json(GlDimensionsCatalogResponse { items, total })
}

/// Человекочитаемое описание проекции-зеркала по её `resource_table`.
/// «sys_general_ledger» — поле самой проводки; остальные — detail-проекции из
/// реестра `detail_links` (с указанием способа связи).
fn matrix_projection(resource_table: &str) -> GlMatrixProjection {
    if resource_table == "sys_general_ledger" {
        return GlMatrixProjection {
            resource_table: resource_table.to_string(),
            label: "Журнал GL (sys_general_ledger)".to_string(),
            kind: "gl".to_string(),
        };
    }

    let kind = match descriptor_for_resource_table(resource_table).map(|d| d.kind) {
        Some(GlDetailLinkKind::ProjectionLinked) => "projection_linked",
        Some(GlDetailLinkKind::ExternalLinked) => "external_linked",
        None => "gl",
    };
    GlMatrixProjection {
        resource_table: resource_table.to_string(),
        label: resource_table.to_string(),
        kind: kind.to_string(),
    }
}

/// Матрица «Слой / Оборот»: обзор доступности измерений по слоям.
///
/// Состав (какие обороты × слои и какие измерения) — из реестра
/// (`TURNOVER_CLASSES` × `GL_LAYER_CLASSES` + `dimensions_for_turnover_at_layer`);
/// проекции — из реестра слоёв (`projections_for_cell`); счётчик проводок —
/// overlay из данных GL. Естественная деривация без хардкода.
pub async fn layer_turnover_matrix() -> Result<Json<GlLayerTurnoverMatrixResponse>, ApiError> {
    let counts = crate::general_ledger::repository::count_grouped_by_turnover_and_layer()
        .await
        .map_err(|e| {
            tracing::error!("layer/turnover matrix counts error: {e}");
            ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        })?;

    let mut layers = GL_LAYER_CLASSES
        .iter()
        .map(|layer| GlMatrixLayer {
            code: layer.code.to_string(),
            name: layer.name.to_string(),
            color_key: layer.color_key.to_string(),
            sort_order: layer.sort_order,
        })
        .collect::<Vec<_>>();
    layers.sort_by_key(|layer| layer.sort_order);

    let mut turnovers = TURNOVER_CLASSES
        .iter()
        .map(|tc| GlMatrixTurnover {
            code: tc.code.to_string(),
            name: tc.name.to_string(),
            report_group: tc.report_group.as_str().to_string(),
        })
        .collect::<Vec<_>>();
    turnovers.sort_by(|left, right| {
        left.report_group
            .cmp(&right.report_group)
            .then_with(|| left.code.cmp(&right.code))
    });

    let mut cells = Vec::with_capacity(turnovers.len() * layers.len());
    let mut filter_dimensions = Vec::new();
    let mut seen_filter_ids = std::collections::HashSet::new();

    for turnover in &turnovers {
        for layer in &layers {
            // Зеркала ячейки (GL + проекции слоя) — общий пул источников; для
            // каждого измерения оставляем те, что физически его содержат.
            let cell_sources = projections_for_cell(&turnover.code, &layer.code);

            let raw_dimensions = dimensions_for_turnover_at_layer(&turnover.code, &layer.code);
            let mut top_level_count = 0usize;
            let dimensions = raw_dimensions
                .into_iter()
                .map(|def| {
                    let is_top_level = def.parent_id.is_none();
                    if is_top_level {
                        top_level_count += 1;
                    }
                    if seen_filter_ids.insert(def.id.clone()) {
                        filter_dimensions.push(def.clone());
                    }
                    let sources = cell_sources
                        .iter()
                        .copied()
                        .filter(|table| table_provides_dimension(table, &def.id))
                        .map(source_short_label)
                        .collect::<Vec<_>>();
                    GlMatrixDimension {
                        def,
                        is_top_level,
                        sources,
                    }
                })
                .collect::<Vec<_>>();

            let projections = cell_sources
                .iter()
                .copied()
                .map(matrix_projection)
                .collect::<Vec<_>>();

            let entry_count = counts
                .get(&(turnover.code.clone(), layer.code.clone()))
                .copied()
                .unwrap_or(0);

            cells.push(GlMatrixCell {
                turnover_code: turnover.code.clone(),
                layer: layer.code.clone(),
                top_level_count,
                entry_count,
                dimensions,
                projections,
            });
        }
    }

    filter_dimensions.sort_by(|left, right| left.code.cmp(&right.code));

    Ok(Json(GlLayerTurnoverMatrixResponse {
        layers,
        turnovers,
        cells,
        filter_dimensions,
    }))
}

pub async fn list_layers() -> Result<Json<GlLayersResponse>, ApiError> {
    let counts = crate::general_ledger::repository::count_grouped_by_layer()
        .await
        .map_err(|e| {
            tracing::error!("general_ledger layer counts error: {}", e);
            ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        })?;

    let mut items = GL_LAYER_CLASSES
        .iter()
        .map(|item| GlLayerDto {
            code: item.code.to_string(),
            name: item.name.to_string(),
            description: item.description.to_string(),
            color_key: item.color_key.to_string(),
            sort_order: item.sort_order,
            gl_entries_count: counts.get(item.code).copied().unwrap_or(0),
        })
        .collect::<Vec<_>>();

    items.sort_by_key(|item| item.sort_order);

    let total = items.len();

    Ok(Json(GlLayersResponse { items, total }))
}

pub async fn list_entities() -> Result<Json<GlEntitiesResponse>, ApiError> {
    let counts = crate::general_ledger::repository::count_grouped_by_entity()
        .await
        .map_err(|e| {
            tracing::error!("general_ledger entity counts error: {}", e);
            ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        })?;

    let mut items = GL_ENTITY_CLASSES
        .iter()
        .map(|item| GlEntityDto {
            code: item.code.to_string(),
            name: item.name.to_string(),
            description: item.description.to_string(),
            kind: item.kind.to_string(),
            color_key: item.color_key.to_string(),
            sort_order: item.sort_order,
            gl_entries_count: counts.get(item.code).copied().unwrap_or(0),
        })
        .collect::<Vec<_>>();

    items.sort_by_key(|item| item.sort_order);

    let total = items.len();

    Ok(Json(GlEntitiesResponse { items, total }))
}

/// Баланс к перечислению поставщику (контур ym): сальдо 7609 + кошелёк баллов 76YB.
pub async fn supplier_balance(
    Json(query): Json<SupplierBalanceQuery>,
) -> Result<Json<SupplierBalanceResponse>, ApiError> {
    report_repository::get_supplier_balance(&query)
        .await
        .map(Json)
        .map_err(|e| {
            tracing::error!("supplier_balance error: {e}");
            ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        })
}

// ─────────────────────────────────────────────────────────────────────────────
// GL Report endpoints
// ─────────────────────────────────────────────────────────────────────────────

pub async fn report(Json(query): Json<GlReportQuery>) -> Result<Json<GlReportResponse>, ApiError> {
    report_repository::get_report(&query)
        .await
        .map(Json)
        .map_err(|e| {
            tracing::error!("GL report error: {e}");
            ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        })
}

pub async fn ym_revenue_reconciliation(
    Query(query): Query<contracts::general_ledger::YmRevenueReconQuery>,
) -> Result<Json<contracts::general_ledger::YmRevenueReconResponse>, ApiError> {
    report_repository::get_ym_revenue_reconciliation(&query)
        .await
        .map(Json)
        .map_err(|e| {
            tracing::error!("YM revenue reconciliation error: {e}");
            ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        })
}

#[derive(Debug, serde::Deserialize)]
pub struct DimensionsParams {
    pub turnover_code: String,
}

pub async fn report_dimensions(
    Query(params): Query<DimensionsParams>,
) -> Json<GlDimensionsResponse> {
    let dimensions = dimensions_for_turnover(&params.turnover_code);
    Json(GlDimensionsResponse {
        turnover_code: params.turnover_code,
        dimensions,
    })
}

pub async fn report_drilldown(
    Json(query): Json<GlDrilldownQuery>,
) -> Result<Json<GlDrilldownResponse>, ApiError> {
    report_repository::get_drilldown(&query)
        .await
        .map(Json)
        .map_err(|e| {
            tracing::error!("GL drilldown error: {e}");
            ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        })
}

pub async fn create_drilldown_session(
    Json(body): Json<GlDrilldownSessionCreate>,
) -> Result<Json<GlDrilldownSessionCreateResponse>, ApiError> {
    drilldown_session_repository::create_session(&body)
        .await
        .map(Json)
        .map_err(|e| {
            tracing::error!("GL drilldown session create error: {e}");
            ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        })
}

pub async fn get_drilldown_session(
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<Json<GlDrilldownSessionRecord>, ApiError> {
    let session = drilldown_session_repository::get_session(&id)
        .await
        .map_err(|e| {
            tracing::error!("GL drilldown session get error: {e}");
            ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        })?
        .ok_or(ApiError::from(axum::http::StatusCode::NOT_FOUND))?;

    drilldown_session_repository::touch_session(&id)
        .await
        .map_err(|e| {
            tracing::error!("GL drilldown session touch error: {e}");
            ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        })?;

    Ok(Json(session))
}

pub async fn get_drilldown_session_data(
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<Json<GlDrilldownResponse>, ApiError> {
    let session = drilldown_session_repository::get_session(&id)
        .await
        .map_err(|e| {
            tracing::error!("GL drilldown session data get error: {e}");
            ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        })?
        .ok_or(ApiError::from(axum::http::StatusCode::NOT_FOUND))?;

    drilldown_session_repository::touch_session(&id)
        .await
        .map_err(|e| {
            tracing::error!("GL drilldown session data touch error: {e}");
            ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        })?;

    report_repository::get_drilldown(&session.query)
        .await
        .map(Json)
        .map_err(|e| {
            tracing::error!("GL drilldown session data error: {e}");
            ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        })
}

// ─────────────────────────────────────────────────────────────────────────────
// GL Account View endpoint
// ─────────────────────────────────────────────────────────────────────────────

pub async fn account_view(
    Json(query): Json<GlAccountViewQuery>,
) -> Result<Json<GlAccountViewResponse>, ApiError> {
    crate::general_ledger::account_view::repository::get_view(&query)
        .await
        .map(Json)
        .map_err(|e| {
            tracing::error!("GL account view error: {e}");
            ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        })
}

pub async fn wb_weekly_reconciliation(
    Query(query): Query<WbWeeklyReconciliationQuery>,
) -> Result<Json<WbWeeklyReconciliationResponse>, ApiError> {
    crate::general_ledger::weekly_reconciliation::get_report(&query)
        .await
        .map(Json)
        .map_err(|e| {
            tracing::error!("WB weekly reconciliation report error: {e}");
            ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        })
}

fn to_dto(row: crate::general_ledger::repository::Model) -> GeneralLedgerEntryDto {
    crate::general_ledger::dto::entry_to_dto(row)
}
