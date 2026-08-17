use crate::shared::error::ApiError;
use axum::{
    body::Body,
    extract::{Path, Query},
    http::{
        header::{CONTENT_DISPOSITION, CONTENT_TYPE},
        HeaderValue, StatusCode,
    },
    response::Response,
    Json,
};
use base64::{engine::general_purpose, Engine as _};
use contracts::domain::a027_wb_documents::aggregate::{WbDocument, WbWeeklyReportManualData};
use contracts::domain::common::AggregateId;
use sea_orm::{ConnectionTrait, Statement, Value};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::domain::a027_wb_documents;
use crate::domain::a027_wb_documents::pdf_report_extractor;
use crate::general_ledger::account_view::registry::ACCOUNT_7609_VIEW;
use crate::shared::data::db::get_connection;
use crate::usecases::u504_import_from_wildberries::wildberries_api_client::WildberriesApiClient;

const REALIZED_GOODS_TURNOVERS: &[&str] = &[
    "customer_revenue_pl",
    "spp_discount",
    "wb_extra_discount",
    // Сторно возвратов: показатель 1.1 в отчёте WB — нетто с учётом возвратов,
    // поэтому возвраты (storno-обороты) вычитаются наравне с продажами.
    "customer_revenue_pl_storno",
    "spp_discount_storno",
    "wb_extra_discount_storno",
];
// Вознаграждение WB (2.1+2.2) на oper-слое = маржа WB сверх цены продавца
// (mp_commission, когда покупатель заплатил больше цены продавца) плюс
// соинвестирование WB в скидку (wb_coinvestment, когда меньше). Эти обороты
// взаимоисключающие построчно, вместе дают полную разницу цен = вознаграждение
// WB с учётом софинансирования. Сторно-варианты вычитают возвраты (отчёт — нетто).
const WB_REWARD_TURNOVERS: &[&str] = &[
    "mp_commission",
    "mp_commission_storno",
    "wb_coinvestment",
    "wb_coinvestment_storno",
];
const ADVERT_RECONCILIATION_TURNOVERS: &[&str] =
    &["advert_clicks_no_order", "advert_clicks_order_accrual"];
const LOGISTICS_TURNOVERS: &[&str] = &[
    "mp_ppvz_reward",
    "mp_ppvz_reward_nm",
    "mp_rebill_logistic_cost",
    "mp_rebill_logistic_cost_nm",
];
const ACQUIRING_TURNOVERS: &[&str] = &["mp_acquiring", "mp_acquiring_storno"];

const OPER_LAYER: &str = "oper";
// Слой p903/p907 переведён на `fina` (бывший `fact`). Константа сохраняет имя
// FACT_LAYER для совместимости, но указывает на новый слой.
const FACT_LAYER: &str = "fina";

const REALIZED_GOODS_FORMULA: &str = "SUM(amount) WHERE turnover_code IN ('customer_revenue_pl', 'spp_discount', 'wb_extra_discount', 'customer_revenue_pl_storno', 'spp_discount_storno', 'wb_extra_discount_storno') AND layer = 'oper'";
const WB_REWARD_FORMULA: &str = "SUM(amount) WHERE turnover_code IN ('mp_commission', 'mp_commission_storno', 'wb_coinvestment', 'wb_coinvestment_storno') AND layer = 'oper'";
const SELLER_TRANSFER_FORMULA: &str = "Account 7609 main balance";
const ADVERT_FORMULA: &str = "SUM(amount) WHERE turnover_code IN ('advert_clicks_no_order', 'advert_clicks_order_accrual') AND layer = 'oper'";
const LOGISTICS_FORMULA: &str = "SUM(amount) WHERE turnover_code IN ('mp_ppvz_reward', 'mp_ppvz_reward_nm', 'mp_rebill_logistic_cost', 'mp_rebill_logistic_cost_nm') AND layer = 'fina'";
const ACQUIRING_FORMULA: &str =
    "SUM(amount) WHERE turnover_code IN ('mp_acquiring', 'mp_acquiring_storno') AND layer = 'fina'";

// ─────────────────────────────────────────────────────────────────────────────
// Сверка по слою FACT (отдельная таблица, не влияет на старую).
//
// Слой fact формируется из p903 — это и есть исходные данные недельного отчёта
// WB, поэтому fact-обороты должны совпадать с отчётом. Старая (oper) таблица
// расходится, т.к. oper считается независимо из a012/a015/a026.
//
// Логистика и эквайринг в старой таблице уже считаются по fact, поэтому здесь
// повторяют тот же состав. Реклама (2.10) на fact отсутствует — вместо неё
// берём прочие удержания p903 (приёмка, хранение, штрафы).
// ─────────────────────────────────────────────────────────────────────────────

// 1.1 — выручка от покупателя и возвраты по факту (p903): retail_amount минус
// возвраты. Соответствует «Итого стоимость реализованного товара».
const FACT_REALIZED_GOODS_TURNOVERS: &[&str] = &["customer_revenue", "customer_revenue_storno"];
// 2.1+2.2 — комиссия WB по факту: комиссия по продаже (mp_commission) и её
// сторно по возврату (mp_commission_storno, знак инвертирован в GL-билдере p903).
// Корректировки комиссии (mp_commission_adjustment[_nm]) сюда НЕ входят: они
// возникают на логистических/прочих операциях p903 (строки «Возмещение издержек
// по перевозке…», где ppvz_vw заполнен паразитно), а отчёт WB не включает их в
// строку «Вознаграждение Вайлдберриз (2.1+2.2)».
const FACT_WB_REWARD_TURNOVERS: &[&str] = &["mp_commission", "mp_commission_storno"];
// 2.10 — прочие удержания по факту: платная приёмка, хранение, штрафы и их
// сторно. Рекламы на fact нет, поэтому здесь её не будет.
const FACT_OTHER_DEDUCTIONS_TURNOVERS: &[&str] = &[
    "acceptance",
    "mp_storage",
    "mp_penalty",
    "mp_penalty_storno",
];
// 2.7+2.8 — логистика по факту (тот же состав, что и в старой таблице).
const FACT_LOGISTICS_TURNOVERS: &[&str] = LOGISTICS_TURNOVERS;
// 2.6 — эквайринг по факту (тот же состав, что и в старой таблице).
const FACT_ACQUIRING_TURNOVERS: &[&str] = ACQUIRING_TURNOVERS;

const FACT_REALIZED_GOODS_FORMULA: &str =
    "SUM(amount) WHERE turnover_code IN ('customer_revenue', 'customer_revenue_storno') AND layer = 'fina'";
const FACT_WB_REWARD_FORMULA: &str = "SUM(amount) WHERE turnover_code IN ('mp_commission', 'mp_commission_storno') AND layer = 'fina'";
const FACT_SELLER_TRANSFER_FORMULA: &str = "SUM over account 7609 (Dt − Cr) WHERE layer = 'fina'";
const FACT_OTHER_DEDUCTIONS_FORMULA: &str = "SUM(amount) WHERE turnover_code IN ('acceptance', 'mp_storage', 'mp_penalty', 'mp_penalty_storno') AND layer = 'fina'";
const FACT_LOGISTICS_FORMULA: &str = LOGISTICS_FORMULA;
const FACT_ACQUIRING_FORMULA: &str = ACQUIRING_FORMULA;

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    pub date_from: Option<String>,
    pub date_to: Option<String>,
    pub connection_id: Option<String>,
    pub weekly_only: Option<bool>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
    pub search_query: Option<String>,
    pub sort_by: Option<String>,
    pub sort_desc: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct WbDocumentsListItemDto {
    pub id: String,
    pub service_name: String,
    pub name: String,
    pub category: String,
    pub creation_time: String,
    pub is_weekly_report: bool,
    pub report_period_from: Option<String>,
    pub report_period_to: Option<String>,
    pub viewed: bool,
    pub extensions: Vec<String>,
    pub connection_id: String,
    pub connection_name: Option<String>,
    pub organization_name: Option<String>,
    pub fetched_at: String,
    pub realized_goods_total: Option<f64>,
    pub wb_reward_with_vat: Option<f64>,
    pub seller_transfer_total: Option<f64>,
    pub other_deductions: Option<f64>,
    pub logistics: Option<f64>,
    pub acquiring: Option<f64>,
    pub max_deviation: Option<f64>,
}

#[derive(Debug, Serialize)]
pub struct PaginatedResponse {
    pub items: Vec<WbDocumentsListItemDto>,
    pub total: usize,
    pub page: usize,
    pub page_size: usize,
    pub total_pages: usize,
}

#[derive(Debug, Serialize)]
pub struct WbDocumentDetailsDto {
    pub id: String,
    pub service_name: String,
    pub name: String,
    pub category: String,
    pub creation_time: String,
    pub viewed: bool,
    pub extensions: Vec<String>,
    pub connection_id: String,
    pub connection_name: Option<String>,
    pub organization_id: String,
    pub organization_name: Option<String>,
    pub marketplace_id: String,
    pub marketplace_name: Option<String>,
    pub is_weekly_report: bool,
    pub report_period_from: Option<String>,
    pub report_period_to: Option<String>,
    pub realized_goods_total: Option<f64>,
    pub wb_reward_with_vat: Option<f64>,
    pub seller_transfer_total: Option<f64>,
    pub other_deductions: Option<f64>,
    pub logistics: Option<f64>,
    pub acquiring: Option<f64>,
    pub max_deviation: Option<f64>,
    pub comment: Option<String>,
    pub reconciliation: WbDocumentReconciliationDto,
    pub fact_reconciliation: WbDocumentReconciliationFactDto,
    pub fetched_at: String,
    pub locale: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize)]
pub struct WbDocumentReconciliationDto {
    pub realized_goods_total: ReconciliationLineDto,
    pub wb_reward_with_vat: ReconciliationLineDto,
    pub seller_transfer_total: ReconciliationLineDto,
    pub advert_other_deductions: ReconciliationLineDto,
    pub logistics: ReconciliationLineDto,
    pub acquiring: ReconciliationLineDto,
}

/// Сверка по слою FACT — независимая копия структуры сверки. Те же 6
/// показателей, но «Данные в базе» считаются по слою fact (p903).
#[derive(Debug, Serialize)]
pub struct WbDocumentReconciliationFactDto {
    pub realized_goods_total: ReconciliationLineDto,
    pub wb_reward_with_vat: ReconciliationLineDto,
    pub seller_transfer_total: ReconciliationLineDto,
    pub advert_other_deductions: ReconciliationLineDto,
    pub logistics: ReconciliationLineDto,
    pub acquiring: ReconciliationLineDto,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReconciliationLineDto {
    pub formula: String,
    pub wb_report: Option<f64>,
    pub database_value: Option<f64>,
    pub difference_amount: Option<f64>,
    pub difference_percent: Option<f64>,
    pub is_available: bool,
}

#[derive(Debug, Deserialize)]
pub struct UpdateManualFieldsRequest {
    pub is_weekly_report: bool,
    pub report_period_from: Option<String>,
    pub report_period_to: Option<String>,
    pub realized_goods_total: Option<f64>,
    pub wb_reward_with_vat: Option<f64>,
    pub seller_transfer_total: Option<f64>,
    pub other_deductions: Option<f64>,
    pub logistics: Option<f64>,
    pub acquiring: Option<f64>,
    pub comment: Option<String>,
}

pub async fn list_paginated(
    Query(query): Query<ListQuery>,
) -> Result<Json<PaginatedResponse>, ApiError> {
    let page_size = query.limit.unwrap_or(100);
    let offset = query.offset.unwrap_or(0);
    let page = if page_size > 0 { offset / page_size } else { 0 };

    let list_query = a027_wb_documents::service::WbDocumentsListQuery {
        date_from: query.date_from,
        date_to: query.date_to,
        connection_id: query.connection_id,
        weekly_only: query.weekly_only.unwrap_or(false),
        search_query: query.search_query,
        sort_by: query.sort_by.unwrap_or_else(|| "creation_time".to_string()),
        sort_desc: query.sort_desc.unwrap_or(true),
        limit: page_size,
        offset,
    };

    match a027_wb_documents::service::list_paginated(list_query).await {
        Ok(result) => {
            let total_pages = if page_size > 0 {
                (result.total + page_size - 1) / page_size
            } else {
                1
            };

            Ok(Json(PaginatedResponse {
                items: result
                    .items
                    .into_iter()
                    .map(|row| WbDocumentsListItemDto {
                        id: row.id,
                        service_name: row.service_name,
                        name: row.name,
                        category: row.category,
                        creation_time: row.creation_time,
                        is_weekly_report: row.is_weekly_report,
                        report_period_from: row.report_period_from,
                        report_period_to: row.report_period_to,
                        viewed: row.viewed,
                        extensions: row.extensions,
                        connection_id: row.connection_id,
                        connection_name: row.connection_name,
                        organization_name: row.organization_name,
                        fetched_at: row.fetched_at,
                        realized_goods_total: row.weekly_report_data.realized_goods_total,
                        wb_reward_with_vat: row.weekly_report_data.wb_reward_with_vat,
                        seller_transfer_total: row.weekly_report_data.seller_transfer_total,
                        other_deductions: row.weekly_report_data.other_deductions,
                        logistics: row.weekly_report_data.logistics,
                        acquiring: row.weekly_report_data.acquiring,
                        max_deviation: row.max_deviation,
                    })
                    .collect(),
                total: result.total,
                page,
                page_size,
                total_pages,
            }))
        }
        Err(e) => {
            tracing::error!("Failed to list WB documents: {}", e);
            Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into())
        }
    }
}

pub async fn get_by_id(Path(id): Path<String>) -> Result<Json<WbDocumentDetailsDto>, ApiError> {
    let uuid = Uuid::parse_str(&id).map_err(|_| StatusCode::BAD_REQUEST)?;
    let doc = match a027_wb_documents::service::get_by_id(uuid).await {
        Ok(Some(doc)) => doc,
        Ok(None) => return Err(axum::http::StatusCode::NOT_FOUND.into()),
        Err(e) => {
            tracing::error!("Failed to get WB document {}: {}", id, e);
            return Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into());
        }
    };

    build_details_dto(doc).await.map(Json).map_err(|e| {
        tracing::error!("Failed to build WB document dto {}: {}", id, e);
        ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
    })
}

pub async fn download_document(
    Path((id, extension)): Path<(String, String)>,
) -> Result<Response<Body>, ApiError> {
    let uuid = Uuid::parse_str(&id).map_err(|_| StatusCode::BAD_REQUEST)?;
    let document = a027_wb_documents::service::get_by_id(uuid)
        .await
        .map_err(|e| {
            tracing::error!("Failed to load WB document {} for download: {}", id, e);
            ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        })?
        .ok_or(ApiError::from(axum::http::StatusCode::NOT_FOUND))?;

    if !document
        .header
        .extensions
        .iter()
        .any(|item| item.eq_ignore_ascii_case(&extension))
    {
        return Err(axum::http::StatusCode::BAD_REQUEST.into());
    }

    let connection_id = Uuid::parse_str(&document.header.connection_id)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let connection = crate::domain::a006_connection_mp::service::get_by_id(connection_id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to load connection for WB document {}: {}", id, e);
            ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        })?
        .ok_or(ApiError::from(axum::http::StatusCode::NOT_FOUND))?;

    let api_client = WildberriesApiClient::new();
    let file = api_client
        .download_document(&connection, &document.header.service_name, &extension)
        .await
        .map_err(|e| {
            tracing::error!("Failed to download WB document {}: {}", id, e);
            ApiError::from(axum::http::StatusCode::BAD_GATEWAY)
        })?;

    let bytes = general_purpose::STANDARD
        .decode(&file.document)
        .map_err(|e| {
            tracing::error!("Failed to decode WB document base64 {}: {}", id, e);
            ApiError::from(axum::http::StatusCode::BAD_GATEWAY)
        })?;

    let mut response = Response::new(Body::from(bytes));
    let headers = response.headers_mut();
    headers.insert(
        CONTENT_TYPE,
        HeaderValue::from_static(content_type_for_extension(&file.extension)),
    );
    headers.insert(
        CONTENT_DISPOSITION,
        HeaderValue::from_str(&format!(
            "attachment; filename=\"{}\"",
            sanitize_filename(&file.file_name)
        ))
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?,
    );

    Ok(response)
}

pub async fn extract_weekly_report(
    Path(id): Path<String>,
) -> Result<Json<WbDocumentDetailsDto>, ApiError> {
    let uuid = Uuid::parse_str(&id).map_err(|_| StatusCode::BAD_REQUEST)?;
    let document = a027_wb_documents::service::get_by_id(uuid)
        .await
        .map_err(|e| {
            tracing::error!("Failed to load WB document {} for extraction: {}", id, e);
            ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        })?
        .ok_or(ApiError::from(axum::http::StatusCode::NOT_FOUND))?;

    let extension = preferred_report_extension(&document)
        .ok_or(ApiError::from(axum::http::StatusCode::BAD_REQUEST))?;
    let connection_id = Uuid::parse_str(&document.header.connection_id)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let connection = crate::domain::a006_connection_mp::service::get_by_id(connection_id)
        .await
        .map_err(|e| {
            tracing::error!(
                "Failed to load connection for WB document extraction {}: {}",
                id,
                e
            );
            ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        })?
        .ok_or(ApiError::from(axum::http::StatusCode::NOT_FOUND))?;

    let api_client = WildberriesApiClient::new();
    let file = api_client
        .download_document(&connection, &document.header.service_name, &extension)
        .await
        .map_err(|e| {
            tracing::error!(
                "Failed to download WB document {} for extraction: {}",
                id,
                e
            );
            ApiError::from(axum::http::StatusCode::BAD_GATEWAY)
        })?;

    let bytes = general_purpose::STANDARD
        .decode(&file.document)
        .map_err(|e| {
            tracing::error!("Failed to decode WB document {} for extraction: {}", id, e);
            ApiError::from(axum::http::StatusCode::BAD_GATEWAY)
        })?;

    // Имя документа ("Отчет № … от DD.MM.YYYY") — надёжный источник даты отчёта,
    // поэтому передаём его в экстрактор вместе с именем файла.
    let name_hint = format!("{}\n{}", document.header.name, file.file_name);
    let extracted = pdf_report_extractor::extract_weekly_report_from_document_bytes(
        &bytes,
        &file.extension,
        Some(&name_hint),
    )
    .map_err(|e| {
        tracing::error!("Failed to extract WB weekly report {}: {}", id, e);
        ApiError::from(axum::http::StatusCode::UNPROCESSABLE_ENTITY)
    })?;

    // TODO(debug): временный лог распарсенных строк отчёта для отладки 2.6/2.10/8.
    tracing::info!(
        "WB report {} extracted period {:?}..{:?}, report_date={:?}",
        id,
        extracted.report_period_from,
        extracted.report_period_to,
        extracted.report_date
    );
    for row in &extracted.rows {
        tracing::info!(
            "WB report {} row code={} amount={:?} vat={:?} raw=\"{}\"",
            id,
            row.code,
            row.amount,
            row.vat_amount,
            row.raw_text
        );
    }

    let current = document.weekly_report_data.clone();
    let extracted_data = extracted.manual_data;
    let updated_data = WbWeeklyReportManualData {
        realized_goods_total: extracted_data
            .realized_goods_total
            .or(current.realized_goods_total),
        wb_reward_with_vat: extracted_data
            .wb_reward_with_vat
            .or(current.wb_reward_with_vat),
        seller_transfer_total: extracted_data
            .seller_transfer_total
            .or(current.seller_transfer_total),
        other_deductions: extracted_data.other_deductions.or(current.other_deductions),
        logistics: extracted_data.logistics.or(current.logistics),
        acquiring: extracted_data.acquiring.or(current.acquiring),
    };

    let updated = a027_wb_documents::service::update_manual_fields(
        uuid,
        true,
        extracted
            .report_period_from
            .or(document.report_period_from.clone()),
        extracted
            .report_period_to
            .or(document.report_period_to.clone()),
        updated_data,
        None,
    )
    .await
    .map_err(|e| {
        tracing::error!("Failed to store extracted WB report data {}: {}", id, e);
        ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
    })?
    .ok_or(ApiError::from(axum::http::StatusCode::NOT_FOUND))?;

    build_details_dto(updated).await.map(Json).map_err(|e| {
        tracing::error!("Failed to build extracted WB document dto {}: {}", id, e);
        ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
    })
}

pub async fn update_manual_fields(
    Path(id): Path<String>,
    Json(req): Json<UpdateManualFieldsRequest>,
) -> Result<Json<WbDocumentDetailsDto>, ApiError> {
    let uuid = Uuid::parse_str(&id).map_err(|_| StatusCode::BAD_REQUEST)?;
    let document = a027_wb_documents::service::update_manual_fields(
        uuid,
        req.is_weekly_report,
        normalize_optional_string(req.report_period_from),
        normalize_optional_string(req.report_period_to),
        WbWeeklyReportManualData {
            realized_goods_total: req.realized_goods_total,
            wb_reward_with_vat: req.wb_reward_with_vat,
            seller_transfer_total: req.seller_transfer_total,
            other_deductions: req.other_deductions,
            logistics: req.logistics,
            acquiring: req.acquiring,
        },
        Some(normalize_optional_string(req.comment)),
    )
    .await
    .map_err(|e| {
        tracing::error!("Failed to update WB document manual fields {}: {}", id, e);
        ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
    })?
    .ok_or(ApiError::from(axum::http::StatusCode::NOT_FOUND))?;

    build_details_dto(document).await.map(Json).map_err(|e| {
        tracing::error!("Failed to build updated WB document dto {}: {}", id, e);
        ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
    })
}

pub async fn post_document(Path(id): Path<String>) -> Result<Json<WbDocumentDetailsDto>, ApiError> {
    let uuid = Uuid::parse_str(&id).map_err(|_| StatusCode::BAD_REQUEST)?;
    let doc = match a027_wb_documents::service::get_by_id(uuid).await {
        Ok(Some(doc)) => doc,
        Ok(None) => return Err(axum::http::StatusCode::NOT_FOUND.into()),
        Err(e) => {
            tracing::error!("Failed to load WB document {} for posting: {}", id, e);
            return Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into());
        }
    };

    let reconciliation = build_reconciliation_dto(&doc).await.map_err(|e| {
        tracing::error!(
            "Failed to build reconciliation for WB document {}: {}",
            id,
            e
        );
        ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
    })?;
    let max_deviation = compute_max_deviation(&reconciliation);

    let doc = a027_wb_documents::service::store_max_deviation(uuid, max_deviation)
        .await
        .map_err(|e| {
            tracing::error!(
                "Failed to store max deviation for WB document {}: {}",
                id,
                e
            );
            ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        })?
        .ok_or(ApiError::from(axum::http::StatusCode::NOT_FOUND))?;

    build_details_dto(doc).await.map(Json).map_err(|e| {
        tracing::error!("Failed to post WB document {}: {}", id, e);
        ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
    })
}

/// Largest reconciliation discrepancy (by absolute amount) across the 6 indicators.
/// Returns `None` when no indicator has a comparable difference.
fn compute_max_deviation(reconciliation: &WbDocumentReconciliationDto) -> Option<f64> {
    [
        &reconciliation.realized_goods_total,
        &reconciliation.wb_reward_with_vat,
        &reconciliation.seller_transfer_total,
        &reconciliation.advert_other_deductions,
        &reconciliation.logistics,
        &reconciliation.acquiring,
    ]
    .into_iter()
    .filter_map(|line| line.difference_amount)
    .map(f64::abs)
    .fold(None, |acc: Option<f64>, value| {
        Some(acc.map_or(value, |current| current.max(value)))
    })
}

async fn build_details_dto(doc: WbDocument) -> anyhow::Result<WbDocumentDetailsDto> {
    Ok(WbDocumentDetailsDto {
        id: doc.base.id.as_string(),
        service_name: doc.header.service_name.clone(),
        name: doc.header.name.clone(),
        category: doc.header.category.clone(),
        creation_time: doc.header.creation_time.clone(),
        viewed: doc.header.viewed,
        extensions: doc.header.extensions.clone(),
        connection_id: doc.header.connection_id.clone(),
        connection_name: resolve_connection_name(&doc.header.connection_id).await?,
        organization_id: doc.header.organization_id.clone(),
        organization_name: resolve_organization_name(&doc.header.organization_id).await?,
        marketplace_id: doc.header.marketplace_id.clone(),
        marketplace_name: resolve_marketplace_name(&doc.header.marketplace_id).await?,
        is_weekly_report: doc.is_weekly_report,
        report_period_from: doc.report_period_from.clone(),
        report_period_to: doc.report_period_to.clone(),
        realized_goods_total: doc.weekly_report_data.realized_goods_total,
        wb_reward_with_vat: doc.weekly_report_data.wb_reward_with_vat,
        seller_transfer_total: doc.weekly_report_data.seller_transfer_total,
        other_deductions: doc.weekly_report_data.other_deductions,
        logistics: doc.weekly_report_data.logistics,
        acquiring: doc.weekly_report_data.acquiring,
        max_deviation: doc.max_deviation,
        comment: doc.base.comment.clone(),
        reconciliation: build_reconciliation_dto(&doc).await?,
        fact_reconciliation: build_fact_reconciliation_dto(&doc).await?,
        fetched_at: doc.source_meta.fetched_at.clone(),
        locale: doc.source_meta.locale.clone(),
        created_at: doc.base.metadata.created_at.to_rfc3339(),
        updated_at: doc.base.metadata.updated_at.to_rfc3339(),
    })
}

async fn build_reconciliation_dto(doc: &WbDocument) -> anyhow::Result<WbDocumentReconciliationDto> {
    let values = match complete_period(
        doc.report_period_from.as_deref(),
        doc.report_period_to.as_deref(),
    ) {
        Some((period_from, period_to)) => Some(ReconciliationValues {
            realized_goods_total: fetch_turnover_total(
                &doc.header.connection_id,
                period_from,
                period_to,
                REALIZED_GOODS_TURNOVERS,
                OPER_LAYER,
            )
            .await?,
            wb_reward_with_vat: fetch_turnover_total(
                &doc.header.connection_id,
                period_from,
                period_to,
                WB_REWARD_TURNOVERS,
                OPER_LAYER,
            )
            .await?,
            seller_transfer_total: fetch_7609_main_balance(
                &doc.header.connection_id,
                period_from,
                period_to,
            )
            .await?,
            other_deductions: fetch_turnover_total(
                &doc.header.connection_id,
                period_from,
                period_to,
                ADVERT_RECONCILIATION_TURNOVERS,
                OPER_LAYER,
            )
            .await?,
            logistics: fetch_turnover_total(
                &doc.header.connection_id,
                period_from,
                period_to,
                LOGISTICS_TURNOVERS,
                FACT_LAYER,
            )
            .await?,
            acquiring: fetch_turnover_total(
                &doc.header.connection_id,
                period_from,
                period_to,
                ACQUIRING_TURNOVERS,
                FACT_LAYER,
            )
            .await?,
        }),
        None => None,
    };

    Ok(WbDocumentReconciliationDto {
        realized_goods_total: build_reconciliation_line(
            REALIZED_GOODS_FORMULA,
            doc.weekly_report_data.realized_goods_total,
            values.as_ref().map(|item| item.realized_goods_total),
        ),
        wb_reward_with_vat: build_reconciliation_line(
            WB_REWARD_FORMULA,
            doc.weekly_report_data.wb_reward_with_vat,
            values.as_ref().map(|item| item.wb_reward_with_vat),
        ),
        seller_transfer_total: build_reconciliation_line(
            SELLER_TRANSFER_FORMULA,
            doc.weekly_report_data.seller_transfer_total,
            values.as_ref().map(|item| item.seller_transfer_total),
        ),
        advert_other_deductions: build_reconciliation_line(
            ADVERT_FORMULA,
            doc.weekly_report_data.other_deductions,
            values.as_ref().map(|item| item.other_deductions),
        ),
        logistics: build_reconciliation_line(
            LOGISTICS_FORMULA,
            doc.weekly_report_data.logistics,
            values.as_ref().map(|item| item.logistics),
        ),
        acquiring: build_reconciliation_line(
            ACQUIRING_FORMULA,
            doc.weekly_report_data.acquiring,
            values.as_ref().map(|item| item.acquiring),
        ),
    })
}

struct ReconciliationValues {
    realized_goods_total: f64,
    wb_reward_with_vat: f64,
    seller_transfer_total: f64,
    other_deductions: f64,
    logistics: f64,
    acquiring: f64,
}

/// Сверка по слою FACT. Полностью независимая от старой (oper) сверки: все
/// показатели считаются по слою fact, который формируется из p903 (исходные
/// данные недельного отчёта WB). Логистика и эквайринг повторяют состав старой
/// таблицы (они уже считались по fact).
async fn build_fact_reconciliation_dto(
    doc: &WbDocument,
) -> anyhow::Result<WbDocumentReconciliationFactDto> {
    let values = match complete_period(
        doc.report_period_from.as_deref(),
        doc.report_period_to.as_deref(),
    ) {
        Some((period_from, period_to)) => Some(ReconciliationValues {
            realized_goods_total: fetch_turnover_total(
                &doc.header.connection_id,
                period_from,
                period_to,
                FACT_REALIZED_GOODS_TURNOVERS,
                FACT_LAYER,
            )
            .await?,
            wb_reward_with_vat: fetch_turnover_total(
                &doc.header.connection_id,
                period_from,
                period_to,
                FACT_WB_REWARD_TURNOVERS,
                FACT_LAYER,
            )
            .await?,
            seller_transfer_total: fetch_7609_fact_balance(
                &doc.header.connection_id,
                period_from,
                period_to,
            )
            .await?,
            other_deductions: fetch_turnover_total(
                &doc.header.connection_id,
                period_from,
                period_to,
                FACT_OTHER_DEDUCTIONS_TURNOVERS,
                FACT_LAYER,
            )
            .await?,
            logistics: fetch_turnover_total(
                &doc.header.connection_id,
                period_from,
                period_to,
                FACT_LOGISTICS_TURNOVERS,
                FACT_LAYER,
            )
            .await?,
            acquiring: fetch_turnover_total(
                &doc.header.connection_id,
                period_from,
                period_to,
                FACT_ACQUIRING_TURNOVERS,
                FACT_LAYER,
            )
            .await?,
        }),
        None => None,
    };

    Ok(WbDocumentReconciliationFactDto {
        realized_goods_total: build_reconciliation_line(
            FACT_REALIZED_GOODS_FORMULA,
            doc.weekly_report_data.realized_goods_total,
            values.as_ref().map(|item| item.realized_goods_total),
        ),
        wb_reward_with_vat: build_reconciliation_line(
            FACT_WB_REWARD_FORMULA,
            doc.weekly_report_data.wb_reward_with_vat,
            values.as_ref().map(|item| item.wb_reward_with_vat),
        ),
        seller_transfer_total: build_reconciliation_line(
            FACT_SELLER_TRANSFER_FORMULA,
            doc.weekly_report_data.seller_transfer_total,
            values.as_ref().map(|item| item.seller_transfer_total),
        ),
        advert_other_deductions: build_reconciliation_line(
            FACT_OTHER_DEDUCTIONS_FORMULA,
            doc.weekly_report_data.other_deductions,
            values.as_ref().map(|item| item.other_deductions),
        ),
        logistics: build_reconciliation_line(
            FACT_LOGISTICS_FORMULA,
            doc.weekly_report_data.logistics,
            values.as_ref().map(|item| item.logistics),
        ),
        acquiring: build_reconciliation_line(
            FACT_ACQUIRING_FORMULA,
            doc.weekly_report_data.acquiring,
            values.as_ref().map(|item| item.acquiring),
        ),
    })
}

fn complete_period<'a>(from: Option<&'a str>, to: Option<&'a str>) -> Option<(&'a str, &'a str)> {
    let from = from.map(str::trim).filter(|value| !value.is_empty())?;
    let to = to.map(str::trim).filter(|value| !value.is_empty())?;
    Some((from, to))
}

fn build_reconciliation_line(
    formula: &str,
    wb_report: Option<f64>,
    database_value: Option<f64>,
) -> ReconciliationLineDto {
    let difference_amount = calculate_difference(wb_report, database_value);
    let difference_percent = calculate_difference_percent(difference_amount, wb_report);

    ReconciliationLineDto {
        formula: formula.to_string(),
        wb_report,
        database_value,
        difference_amount,
        difference_percent,
        is_available: database_value.is_some(),
    }
}

fn calculate_difference(wb_report: Option<f64>, database_value: Option<f64>) -> Option<f64> {
    Some(wb_report? - database_value?)
}

fn calculate_difference_percent(difference: Option<f64>, wb_report: Option<f64>) -> Option<f64> {
    let wb_report = wb_report?;
    if wb_report.abs() <= f64::EPSILON {
        return None;
    }
    Some(difference? / wb_report * 100.0)
}

fn sv(value: impl Into<String>) -> Value {
    Value::String(Some(Box::new(value.into())))
}

fn turnover_where_clause(turnovers: &[&str]) -> String {
    let placeholders = turnovers.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
    format!("turnover_code IN ({placeholders}) AND layer = ?")
}

async fn fetch_turnover_total(
    connection_id: &str,
    period_from: &str,
    period_to: &str,
    turnovers: &[&str],
    layer: &str,
) -> anyhow::Result<f64> {
    let mut params = vec![sv(period_from), sv(period_to), sv(connection_id)];
    params.extend(turnovers.iter().map(|code| sv(*code)));
    params.push(sv(layer));

    let sql = format!(
        "SELECT COALESCE(SUM(amount), 0.0) AS total
         FROM sys_general_ledger
         WHERE entry_date >= ?
           AND entry_date <= ?
           AND connection_mp_ref = ?
           AND {}",
        turnover_where_clause(turnovers)
    );

    let db = get_connection();
    let stmt = Statement::from_sql_and_values(db.get_database_backend(), &sql, params);
    let row = db.query_one(stmt).await?;

    Ok(row
        .and_then(|raw| raw.try_get::<f64>("", "total").ok())
        .unwrap_or(0.0))
}

async fn fetch_7609_main_balance(
    connection_id: &str,
    period_from: &str,
    period_to: &str,
) -> anyhow::Result<f64> {
    let mut sql = String::from(
        "SELECT COALESCE(SUM(
             CASE
                 WHEN debit_account = '7609' THEN amount
                 WHEN credit_account = '7609' THEN -amount
                 ELSE 0.0
             END
         ), 0.0) AS total
         FROM sys_general_ledger
         WHERE (debit_account = '7609' OR credit_account = '7609')
           AND entry_date >= ?
           AND entry_date <= ?
           AND connection_mp_ref = ?
           AND (",
    );
    let mut params = vec![sv(period_from), sv(period_to), sv(connection_id)];

    for (idx, entry) in ACCOUNT_7609_VIEW.main_entries.iter().enumerate() {
        if idx > 0 {
            sql.push_str(" OR ");
        }
        if entry.layer.is_empty() {
            sql.push_str("(turnover_code = ?)");
            params.push(sv(entry.turnover_code));
        } else {
            sql.push_str("(turnover_code = ? AND layer = ?)");
            params.push(sv(entry.turnover_code));
            params.push(sv(entry.layer));
        }
    }
    sql.push(')');

    let db = get_connection();
    let stmt = Statement::from_sql_and_values(db.get_database_backend(), &sql, params);
    let row = db.query_one(stmt).await?;

    Ok(row
        .and_then(|raw| raw.try_get::<f64>("", "total").ok())
        .unwrap_or(0.0))
}

/// Сальдо счёта 7609 (Дт − Кт) только по слою fact за период. Используется в
/// fact-таблице сверки для показателя «Итого к перечислению продавцу (8)».
async fn fetch_7609_fact_balance(
    connection_id: &str,
    period_from: &str,
    period_to: &str,
) -> anyhow::Result<f64> {
    let sql = "SELECT COALESCE(SUM(
             CASE
                 WHEN debit_account = '7609' THEN amount
                 WHEN credit_account = '7609' THEN -amount
                 ELSE 0.0
             END
         ), 0.0) AS total
         FROM sys_general_ledger
         WHERE (debit_account = '7609' OR credit_account = '7609')
           AND entry_date >= ?
           AND entry_date <= ?
           AND connection_mp_ref = ?
           AND layer = ?";
    let params = vec![
        sv(period_from),
        sv(period_to),
        sv(connection_id),
        sv(FACT_LAYER),
    ];

    let db = get_connection();
    let stmt = Statement::from_sql_and_values(db.get_database_backend(), sql, params);
    let row = db.query_one(stmt).await?;

    Ok(row
        .and_then(|raw| raw.try_get::<f64>("", "total").ok())
        .unwrap_or(0.0))
}

async fn resolve_connection_name(connection_id: &str) -> anyhow::Result<Option<String>> {
    let Some(uuid) = parse_uuid(connection_id) else {
        return Ok(None);
    };
    let connection = crate::domain::a006_connection_mp::service::get_by_id(uuid).await?;
    Ok(connection.map(|item| item.base.description))
}

async fn resolve_organization_name(organization_id: &str) -> anyhow::Result<Option<String>> {
    let Some(uuid) = parse_uuid(organization_id) else {
        return Ok(None);
    };
    let organization = crate::domain::a002_organization::service::get_by_id(
        crate::shared::data::db::get_connection(),
        uuid,
    )
    .await?;
    Ok(organization.map(|item| item.base.description))
}

async fn resolve_marketplace_name(marketplace_id: &str) -> anyhow::Result<Option<String>> {
    let Some(uuid) = parse_uuid(marketplace_id) else {
        return Ok(None);
    };
    let marketplace = crate::domain::a005_marketplace::service::get_by_id(uuid).await?;
    Ok(marketplace.map(|item| item.base.description))
}

fn parse_uuid(value: &str) -> Option<Uuid> {
    Uuid::parse_str(value).ok()
}

fn normalize_optional_string(value: Option<String>) -> Option<String> {
    value.and_then(|item| {
        let trimmed = item.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn sanitize_filename(value: &str) -> String {
    value.replace(['\r', '\n', '"'], "_")
}

fn content_type_for_extension(extension: &str) -> &'static str {
    match extension.to_ascii_lowercase().as_str() {
        "zip" => "application/zip",
        "pdf" => "application/pdf",
        "xlsx" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        "xls" => "application/vnd.ms-excel",
        "csv" => "text/csv; charset=utf-8",
        "xml" => "application/xml",
        _ => "application/octet-stream",
    }
}

fn preferred_report_extension(document: &WbDocument) -> Option<String> {
    document
        .header
        .extensions
        .iter()
        .find(|ext| ext.eq_ignore_ascii_case("zip"))
        .or_else(|| {
            document
                .header
                .extensions
                .iter()
                .find(|ext| ext.eq_ignore_ascii_case("pdf"))
        })
        .cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weekly_manual_data_deserializes_old_json() {
        let data: WbWeeklyReportManualData = serde_json::from_str(
            r#"{"realized_goods_total":100.0,"wb_reward_with_vat":10.0,"seller_transfer_total":90.0}"#,
        )
        .expect("old weekly JSON must deserialize");

        assert_eq!(data.realized_goods_total, Some(100.0));
        assert_eq!(data.wb_reward_with_vat, Some(10.0));
        assert_eq!(data.seller_transfer_total, Some(90.0));
        assert_eq!(data.other_deductions, None);
        assert_eq!(data.logistics, None);
        assert_eq!(data.acquiring, None);
    }

    #[test]
    fn reconciliation_line_calculates_difference_and_percent() {
        let line = build_reconciliation_line("test", Some(120.0), Some(100.0));

        assert_eq!(line.difference_amount, Some(20.0));
        assert_eq!(line.difference_percent, Some(20.0 / 120.0 * 100.0));
        assert!(line.is_available);
    }

    #[test]
    fn reconciliation_percent_is_empty_for_zero_report() {
        let line = build_reconciliation_line("test", Some(0.0), Some(100.0));

        assert_eq!(line.difference_amount, Some(-100.0));
        assert_eq!(line.difference_percent, None);
    }

    #[test]
    fn reconciliation_requires_both_values_for_difference() {
        let line = build_reconciliation_line("test", Some(120.0), None);

        assert_eq!(line.difference_amount, None);
        assert_eq!(line.difference_percent, None);
        assert!(!line.is_available);
    }

    fn recon_with_differences(differences: [Option<f64>; 6]) -> WbDocumentReconciliationDto {
        let line = |difference_amount: Option<f64>| ReconciliationLineDto {
            formula: "test".to_string(),
            wb_report: None,
            database_value: None,
            difference_amount,
            difference_percent: None,
            is_available: difference_amount.is_some(),
        };
        WbDocumentReconciliationDto {
            realized_goods_total: line(differences[0]),
            wb_reward_with_vat: line(differences[1]),
            seller_transfer_total: line(differences[2]),
            advert_other_deductions: line(differences[3]),
            logistics: line(differences[4]),
            acquiring: line(differences[5]),
        }
    }

    #[test]
    fn max_deviation_picks_largest_absolute_difference() {
        let recon = recon_with_differences([
            Some(10.0),
            Some(-50.0),
            Some(5.0),
            None,
            Some(-3.0),
            Some(40.0),
        ]);

        assert_eq!(compute_max_deviation(&recon), Some(50.0));
    }

    #[test]
    fn max_deviation_is_none_without_any_difference() {
        let recon = recon_with_differences([None, None, None, None, None, None]);

        assert_eq!(compute_max_deviation(&recon), None);
    }

    #[test]
    fn advert_reconciliation_filter_uses_expected_turnovers_and_layer() {
        assert_eq!(
            ADVERT_RECONCILIATION_TURNOVERS,
            &["advert_clicks_no_order", "advert_clicks_order_accrual"]
        );
        assert_eq!(OPER_LAYER, "oper");
        assert_eq!(
            turnover_where_clause(ADVERT_RECONCILIATION_TURNOVERS),
            "turnover_code IN (?, ?) AND layer = ?"
        );
    }
}
