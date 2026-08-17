use crate::shared::error::ApiError;
use axum::{
    extract::{Path, Query},
    Json,
};
use contracts::general_ledger::GeneralLedgerEntryDto;
use contracts::projections::p907_ym_payment_report::dto::{
    YmPaymentReportDetailResponse, YmPaymentReportDto, YmPaymentReportFilterOptionsResponse,
    YmPaymentReportListRequest, YmPaymentReportListResponse,
};
use serde::Deserialize;

use crate::projections::p907_ym_payment_report::repository;
use crate::usecases::u503_import_from_yandex::processors::payment_report as payment_report_processor;

/// Handler для получения списка записей отчёта по платежам YM
pub async fn list_reports(
    Query(req): Query<YmPaymentReportListRequest>,
) -> Result<Json<YmPaymentReportListResponse>, ApiError> {
    let (items, total) = repository::list_with_filters(
        &req.date_from,
        &req.date_to,
        req.transaction_type,
        req.payment_status,
        req.transaction_source,
        req.shop_sku,
        req.order_id,
        req.bank_order_id,
        req.connection_mp_ref,
        req.organization_ref,
        &req.sort_by,
        req.sort_desc,
        req.limit,
        req.offset,
    )
    .await
    .map_err(|e| {
        tracing::error!("Failed to list YM payment report: {}", e);
        ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
    })?;

    let gl_counts = crate::general_ledger::repository::count_by_registrator_refs(
        &items.iter().map(|item| item.id.clone()).collect::<Vec<_>>(),
    )
    .await
    .map_err(|e| {
        tracing::error!("Failed to count p907 general ledger rows: {}", e);
        ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
    })?;

    let dtos: Vec<YmPaymentReportDto> = items
        .into_iter()
        .map(|item| {
            let count = gl_counts.get(&item.id).copied().unwrap_or_default();
            model_to_dto(item, count)
        })
        .collect();
    let has_more = total > (req.offset + dtos.len() as i32);

    Ok(Json(YmPaymentReportListResponse {
        items: dtos,
        total_count: total,
        has_more,
    }))
}

#[derive(Debug, Deserialize)]
pub struct FilterOptionsQuery {
    #[serde(default)]
    pub date_from: String,
    #[serde(default)]
    pub date_to: String,
    pub connection_mp_ref: Option<String>,
    pub organization_ref: Option<String>,
}

pub async fn filter_options(
    Query(req): Query<FilterOptionsQuery>,
) -> Result<Json<YmPaymentReportFilterOptionsResponse>, ApiError> {
    let (transaction_types, payment_statuses, transaction_sources) =
        repository::list_filter_options(
            &req.date_from,
            &req.date_to,
            req.connection_mp_ref,
            req.organization_ref,
        )
        .await
        .map_err(|e| {
            tracing::error!("Failed to list YM payment report filter options: {}", e);
            ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        })?;

    Ok(Json(YmPaymentReportFilterOptionsResponse {
        transaction_types,
        payment_statuses,
        transaction_sources,
    }))
}

/// Handler для получения одной записи отчёта по платежам YM по UUID (`id` поле).
pub async fn get_report(
    Path(id): Path<String>,
) -> Result<Json<YmPaymentReportDetailResponse>, ApiError> {
    load_report_detail_by_id(&id).await.map(Json)
}

pub async fn post_report(
    Path(id): Path<String>,
) -> Result<Json<YmPaymentReportDetailResponse>, ApiError> {
    crate::projections::p907_ym_payment_report::service::rebuild_entry_from_existing(&id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to rebuild p907 general ledger for id {}: {}", id, e);
            ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        })?;

    load_report_detail_by_id(&id).await.map(Json)
}

/// Массовое перепроведение всех записей p907: перестраивает GL/p914 для каждой
/// строки. Используется после изменения маппинга оборотов, чтобы провести ранее
/// не отражавшиеся операции YM.
pub async fn repost_all() -> Result<Json<serde_json::Value>, ApiError> {
    let (rows, gl_entries) = crate::projections::p907_ym_payment_report::service::repost_all()
        .await
        .map_err(|e| {
            tracing::error!("Failed to repost all p907 rows: {}", e);
            ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        })?;

    Ok(Json(serde_json::json!({
        "success": true,
        "rows": rows,
        "general_ledger_entries": gl_entries,
        "message": format!("Reposted {} rows, {} GL entries", rows, gl_entries),
    })))
}

/// Строки проекции p914 (слой `fina`), относящиеся к данной записи p907.
/// Используется на детальной странице для показа соответствующего JSON.
pub async fn get_finance_turnovers(
    Path(id): Path<String>,
) -> Result<
    Json<Vec<contracts::projections::p914_mp_finance_turnovers::dto::MpFinanceTurnoverDto>>,
    ApiError,
> {
    let rows = crate::projections::p914_mp_finance_turnovers::repository::list_by_registrator(
        "p907_ym_payment_report",
        &id,
    )
    .await
    .map_err(|e| {
        tracing::error!(
            "Failed to load p914 finance turnovers for p907 id {}: {}",
            id,
            e
        );
        ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
    })?;

    let dtos = rows
        .into_iter()
        .map(super::p914_mp_finance_turnovers::model_to_dto)
        .collect::<Vec<_>>();

    Ok(Json(dtos))
}

/// Migrate all SYNTH_... record keys to ymid_... format.
/// Safe to call multiple times — already-migrated rows are skipped.
pub async fn migrate_keys() -> Result<Json<serde_json::Value>, ApiError> {
    let (migrated, _already_ymid, errors) = repository::migrate_synth_keys(|record| {
        payment_report_processor::build_ymid_key(
            record.order_id,
            record.transaction_date.as_deref().unwrap_or(""),
            record.transaction_type.as_deref().unwrap_or(""),
            record.shop_sku.as_deref().unwrap_or(""),
            record.transaction_sum,
        )
    })
    .await
    .map_err(|e| {
        tracing::error!("migrate_keys: {}", e);
        ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
    })?;

    Ok(Json(serde_json::json!({
        "success": true,
        "migrated": migrated,
        "errors": errors,
        "message": format!("Migration complete: {} rows migrated, {} errors", migrated, errors)
    })))
}

async fn load_report_detail_by_id(id: &str) -> Result<YmPaymentReportDetailResponse, ApiError> {
    let item = repository::get_by_uuid(id).await.map_err(|e| {
        tracing::error!("Failed to get YM payment report detail: {}", e);
        ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
    })?;

    let Some(item) = item else {
        return Err(axum::http::StatusCode::NOT_FOUND.into());
    };

    let general_ledger_entries =
        crate::general_ledger::repository::list_by_registrator("p907_ym_payment_report", &item.id)
            .await
            .map_err(|e| {
                tracing::error!("Failed to load p907 general ledger rows by id: {}", e);
                ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
            })?
            .into_iter()
            .map(to_general_ledger_dto)
            .collect::<Vec<_>>();

    let count = general_ledger_entries.len();
    Ok(YmPaymentReportDetailResponse {
        item: model_to_dto(item, count),
        general_ledger_entries,
    })
}

fn model_to_dto(
    model: repository::Model,
    general_ledger_entries_count: usize,
) -> YmPaymentReportDto {
    YmPaymentReportDto {
        id: model.id,
        record_key: model.record_key,
        transaction_id: model.transaction_id,
        connection_mp_ref: model.connection_mp_ref,
        organization_ref: model.organization_ref,
        business_id: model.business_id,
        partner_id: model.partner_id,
        shop_name: model.shop_name,
        inn: model.inn,
        model: model.model,
        transaction_date: model.transaction_date,
        transaction_type: model.transaction_type,
        transaction_source: model.transaction_source,
        transaction_sum: model.transaction_sum,
        payment_status: model.payment_status,
        order_id: model.order_id,
        shop_order_id: model.shop_order_id,
        order_creation_date: model.order_creation_date,
        order_delivery_date: model.order_delivery_date,
        order_type: model.order_type,
        shop_sku: model.shop_sku,
        offer_or_service_name: model.offer_or_service_name,
        count: model.count,
        act_id: model.act_id,
        act_date: model.act_date,
        bank_order_id: model.bank_order_id,
        bank_order_date: model.bank_order_date,
        bank_sum: model.bank_sum,
        claim_number: model.claim_number,
        bonus_account_year_month: model.bonus_account_year_month,
        comments: model.comments,
        marketplace_product_ref: model.marketplace_product_ref,
        marketplace_order_ref: model.marketplace_order_ref,
        nomenclature_ref: model.nomenclature_ref,
        loaded_at_utc: model.loaded_at_utc,
        payload_version: model.payload_version,
        general_ledger_entries_count,
    }
}

fn to_general_ledger_dto(row: crate::general_ledger::repository::Model) -> GeneralLedgerEntryDto {
    crate::general_ledger::dto::entry_to_dto(row)
}
