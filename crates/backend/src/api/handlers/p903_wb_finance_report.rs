use crate::shared::error::ApiError;
use axum::{
    body::Body,
    extract::Query,
    http::{header, StatusCode},
    response::Response,
    Json,
};
use chrono::NaiveDate;
use contracts::general_ledger::GeneralLedgerEntryDto;
use contracts::projections::p903_wb_finance_report::dto::{
    WbFinanceReportDetailResponse, WbFinanceReportDto, WbFinanceReportListRequest,
    WbFinanceReportListResponse,
};
use serde::Deserialize;

use crate::projections::p903_wb_finance_report::repository;

/// Handler для получения списка финансовых отчетов с фильтрами
pub async fn list_reports(
    Query(req): Query<WbFinanceReportListRequest>,
) -> Result<Json<WbFinanceReportListResponse>, ApiError> {
    let (items, total) = repository::list_with_filters(
        &req.date_from,
        &req.date_to,
        req.nm_id,
        req.sa_name,
        req.connection_mp_ref,
        req.organization_ref,
        req.supplier_oper_name,
        req.srid,
        &req.sort_by,
        req.sort_desc,
        req.limit,
        req.offset,
    )
    .await
    .map_err(|e| {
        tracing::error!("Failed to list finance report: {}", e);
        ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
    })?;

    let gl_counts = crate::general_ledger::repository::count_by_registrator_refs(
        &items.iter().map(|item| item.id.clone()).collect::<Vec<_>>(),
    )
    .await
    .map_err(|e| {
        tracing::error!("Failed to count p903 general ledger rows: {}", e);
        ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
    })?;

    let dtos: Vec<WbFinanceReportDto> = items
        .into_iter()
        .map(|item| {
            let count = gl_counts.get(&item.id).copied().unwrap_or_default();
            model_to_dto(item, count)
        })
        .collect();

    let has_more = total > (req.offset + dtos.len() as i32);

    Ok(Json(WbFinanceReportListResponse {
        items: dtos,
        total_count: total,
        has_more,
    }))
}

/// Экспорт в CSV: все строки с учётом фильтров (без пагинации), все колонки
/// таблицы `p903_wb_finance_report` с их оригинальными названиями.
pub async fn export_reports(Query(req): Query<WbFinanceReportListRequest>) -> Response {
    let items = match repository::list_all_with_filters(
        &req.date_from,
        &req.date_to,
        req.nm_id,
        req.sa_name,
        req.connection_mp_ref,
        req.organization_ref,
        req.supplier_oper_name,
        req.srid,
        &req.sort_by,
        req.sort_desc,
    )
    .await
    {
        Ok(items) => items,
        Err(e) => {
            tracing::error!("Failed to export finance report: {}", e);
            return csv_plain_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Не удалось загрузить данные для экспорта",
            );
        }
    };

    match build_finance_report_csv(&items) {
        Ok(buffer) => {
            let filename = format!(
                "wb_finance_report_{}.csv",
                chrono::Utc::now().format("%Y%m%d_%H%M%S")
            );
            Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, "text/csv; charset=utf-8")
                .header(
                    header::CONTENT_DISPOSITION,
                    format!(r#"attachment; filename="{filename}""#),
                )
                .body(Body::from(buffer))
                .unwrap_or_else(|_| Response::new(Body::empty()))
        }
        Err(e) => {
            tracing::error!("Failed to build p903 export CSV: {}", e);
            csv_plain_error(StatusCode::INTERNAL_SERVER_ERROR, "Ошибка формирования CSV")
        }
    }
}

fn csv_plain_error(status: StatusCode, message: impl Into<String>) -> Response {
    Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, "text/plain; charset=utf-8")
        .body(Body::from(message.into()))
        .unwrap_or_else(|_| Response::new(Body::empty()))
}

/// Заголовки = оригинальные имена колонок таблицы `p903_wb_finance_report`,
/// в порядке объявления в `repository::Model`.
const EXPORT_HEADERS: &[&str] = &[
    "id",
    "rr_dt",
    "rrd_id",
    "source_row_ref",
    "connection_mp_ref",
    "organization_ref",
    "acquiring_fee",
    "acquiring_percent",
    "additional_payment",
    "bonus_type_name",
    "commission_percent",
    "delivery_amount",
    "delivery_rub",
    "nm_id",
    "a004_nomenclature_ref",
    "penalty",
    "ppvz_vw",
    "ppvz_vw_nds",
    "ppvz_sales_commission",
    "quantity",
    "rebill_logistic_cost",
    "retail_amount",
    "retail_price",
    "retail_price_withdisc_rub",
    "return_amount",
    "sa_name",
    "storage_fee",
    "subject_name",
    "supplier_oper_name",
    "cashback_amount",
    "ppvz_for_pay",
    "ppvz_kvw_prc",
    "ppvz_kvw_prc_base",
    "srv_dbs",
    "srid",
    "loaded_at_utc",
    "payload_version",
];

/// Десятичный разделитель — запятая (Excel/1С, ru-локаль). Полная точность.
fn fmt_opt_f64(v: Option<f64>) -> String {
    v.map(|x| x.to_string().replace('.', ","))
        .unwrap_or_default()
}

fn build_finance_report_csv(items: &[repository::Model]) -> anyhow::Result<Vec<u8>> {
    let mut buffer: Vec<u8> = Vec::new();
    // UTF-8 BOM — корректная кириллица при открытии в Excel.
    buffer.extend_from_slice("\u{FEFF}".as_bytes());

    let mut wtr = csv::WriterBuilder::new()
        .delimiter(b';')
        .from_writer(&mut buffer);

    wtr.write_record(EXPORT_HEADERS)?;

    for m in items {
        let row: [String; 37] = [
            m.id.clone(),
            m.rr_dt.clone(),
            m.rrd_id.to_string(),
            m.source_row_ref.clone(),
            m.connection_mp_ref.clone(),
            m.organization_ref.clone(),
            fmt_opt_f64(m.acquiring_fee),
            fmt_opt_f64(m.acquiring_percent),
            fmt_opt_f64(m.additional_payment),
            m.bonus_type_name.clone().unwrap_or_default(),
            fmt_opt_f64(m.commission_percent),
            fmt_opt_f64(m.delivery_amount),
            fmt_opt_f64(m.delivery_rub),
            m.nm_id.map(|v| v.to_string()).unwrap_or_default(),
            m.a004_nomenclature_ref.clone().unwrap_or_default(),
            fmt_opt_f64(m.penalty),
            fmt_opt_f64(m.ppvz_vw),
            fmt_opt_f64(m.ppvz_vw_nds),
            fmt_opt_f64(m.ppvz_sales_commission),
            m.quantity.map(|v| v.to_string()).unwrap_or_default(),
            fmt_opt_f64(m.rebill_logistic_cost),
            fmt_opt_f64(m.retail_amount),
            fmt_opt_f64(m.retail_price),
            fmt_opt_f64(m.retail_price_withdisc_rub),
            fmt_opt_f64(m.return_amount),
            m.sa_name.clone().unwrap_or_default(),
            fmt_opt_f64(m.storage_fee),
            m.subject_name.clone().unwrap_or_default(),
            m.supplier_oper_name.clone().unwrap_or_default(),
            fmt_opt_f64(m.cashback_amount),
            fmt_opt_f64(m.ppvz_for_pay),
            fmt_opt_f64(m.ppvz_kvw_prc),
            fmt_opt_f64(m.ppvz_kvw_prc_base),
            m.srv_dbs.map(|v| v.to_string()).unwrap_or_default(),
            m.srid.clone().unwrap_or_default(),
            m.loaded_at_utc.clone(),
            m.payload_version.to_string(),
        ];
        wtr.write_record(&row)?;
    }

    wtr.flush()?;
    drop(wtr);
    Ok(buffer)
}

/// Handler для получения детальной информации по композитному ключу
#[derive(Debug, Deserialize)]
pub struct OperationKindsQuery {
    pub date_from: String,
    pub date_to: String,
    pub connection_mp_ref: Option<String>,
    pub organization_ref: Option<String>,
}

pub async fn list_operation_kinds(
    Query(query): Query<OperationKindsQuery>,
) -> Result<Json<Vec<String>>, ApiError> {
    let items = repository::list_distinct_supplier_oper_names(
        &query.date_from,
        &query.date_to,
        query.connection_mp_ref,
        query.organization_ref,
    )
    .await
    .map_err(|e| {
        tracing::error!("Failed to list finance report operation kinds: {}", e);
        ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
    })?;

    Ok(Json(items))
}

pub async fn get_report_detail_by_id(
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<Json<WbFinanceReportDetailResponse>, ApiError> {
    load_report_detail_by_id(&id).await.map(Json)
}

pub async fn post_report_by_id(
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<Json<WbFinanceReportDetailResponse>, ApiError> {
    let item = repository::get_by_id(&id)
        .await
        .map_err(|e| {
            tracing::error!(
                "Failed to get finance report detail before post by id: {}",
                e
            );
            ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        })?
        .ok_or(ApiError::from(axum::http::StatusCode::NOT_FOUND))?;

    let day = NaiveDate::parse_from_str(&item.rr_dt, "%Y-%m-%d").map_err(|e| {
        tracing::error!(
            "Failed to parse p903 rr_dt '{}' for post by id: {}",
            item.rr_dt,
            e
        );
        ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
    })?;

    crate::projections::p903_wb_finance_report::service::rebuild_day_from_existing(
        &item.connection_mp_ref,
        day,
    )
    .await
    .map_err(|e| {
        tracing::error!("Failed to rebuild p903 general ledger for id {}: {}", id, e);
        ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
    })?;

    load_report_detail_by_id(&id).await.map(Json)
}

/// Handler для получения raw JSON по композитному ключу
pub async fn get_raw_json_by_id(
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<String, ApiError> {
    let item = repository::get_by_id(&id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to get finance report raw json by id: {}", e);
            ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        })?
        .ok_or(ApiError::from(axum::http::StatusCode::NOT_FOUND))?;

    Ok(item.extra.unwrap_or_else(|| "{}".to_string()))
}

/// Handler для поиска записей по srid
#[derive(Debug, Deserialize)]
pub struct SearchBySridQuery {
    pub srid: String,
}

pub async fn search_by_srid(
    Query(query): Query<SearchBySridQuery>,
) -> Result<Json<Vec<WbFinanceReportDto>>, ApiError> {
    let items = repository::search_by_srid(&query.srid).await.map_err(|e| {
        tracing::error!("Failed to search finance report by srid: {}", e);
        ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
    })?;

    let dtos: Vec<WbFinanceReportDto> = items
        .into_iter()
        .map(|item| model_to_dto(item, 0))
        .collect();

    Ok(Json(dtos))
}

/// Преобразование Model в DTO для списка (без extra для экономии трафика)
async fn load_report_detail_by_id(id: &str) -> Result<WbFinanceReportDetailResponse, ApiError> {
    let item = repository::get_by_id(id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to get finance report detail by id: {}", e);
            ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        })?
        .ok_or(ApiError::from(axum::http::StatusCode::NOT_FOUND))?;

    let general_ledger_entries =
        crate::general_ledger::repository::list_by_registrator_ref(&item.id)
            .await
            .map_err(|e| {
                tracing::error!("Failed to load p903 general ledger rows by id: {}", e);
                ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
            })?
            .into_iter()
            .map(to_general_ledger_dto)
            .collect::<Vec<_>>();

    let extra = item.extra.clone();
    let mut dto = model_to_dto(item, general_ledger_entries.len());
    dto.extra = extra;

    Ok(WbFinanceReportDetailResponse {
        item: dto,
        general_ledger_entries,
    })
}

fn model_to_dto(
    model: repository::Model,
    general_ledger_entries_count: usize,
) -> WbFinanceReportDto {
    // Убрано форматирование даты для оптимизации - отправляем как есть
    // Поле extra не включаем в список - оно может содержать большой JSON (~3KB на запись)
    WbFinanceReportDto {
        id: model.id,
        rr_dt: model.rr_dt,
        rrd_id: model.rrd_id,
        source_row_ref: model.source_row_ref,
        connection_mp_ref: model.connection_mp_ref,
        organization_ref: model.organization_ref,
        acquiring_fee: model.acquiring_fee,
        acquiring_percent: model.acquiring_percent,
        additional_payment: model.additional_payment,
        bonus_type_name: model.bonus_type_name,
        commission_percent: model.commission_percent,
        delivery_amount: model.delivery_amount,
        delivery_rub: model.delivery_rub,
        nm_id: model.nm_id,
        a004_nomenclature_ref: model.a004_nomenclature_ref,
        marketplace_product_ref: model.marketplace_product_ref,
        marketplace_order_ref: model.marketplace_order_ref,
        penalty: model.penalty,
        ppvz_vw: model.ppvz_vw,
        ppvz_vw_nds: model.ppvz_vw_nds,
        ppvz_sales_commission: model.ppvz_sales_commission,
        quantity: model.quantity,
        rebill_logistic_cost: model.rebill_logistic_cost,
        retail_amount: model.retail_amount,
        retail_price: model.retail_price,
        retail_price_withdisc_rub: model.retail_price_withdisc_rub,
        return_amount: model.return_amount,
        sa_name: model.sa_name,
        storage_fee: model.storage_fee,
        subject_name: model.subject_name,
        supplier_oper_name: model.supplier_oper_name,
        cashback_amount: model.cashback_amount,
        ppvz_for_pay: model.ppvz_for_pay,
        ppvz_kvw_prc: model.ppvz_kvw_prc,
        ppvz_kvw_prc_base: model.ppvz_kvw_prc_base,
        srv_dbs: model.srv_dbs,
        srid: model.srid,
        loaded_at_utc: model.loaded_at_utc,
        payload_version: model.payload_version,
        general_ledger_entries_count,
        extra: None, // Исключаем из списка для экономии трафика (60MB -> ~5MB)
    }
}

fn to_general_ledger_dto(row: crate::general_ledger::repository::Model) -> GeneralLedgerEntryDto {
    crate::general_ledger::dto::entry_to_dto(row)
}
