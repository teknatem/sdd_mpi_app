use crate::shared::error::ApiError;
use axum::{
    extract::{Path, Query},
    Json,
};
use contracts::domain::a034_ym_realization::aggregate::{YmRealization, YmRealizationLine};
use contracts::domain::common::AggregateId;
use contracts::general_ledger::GeneralLedgerEntryDto;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::domain::a034_ym_realization;
use crate::domain::a034_ym_realization::repository::{
    YmRealizationListQuery, YmRealizationListRow,
};

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    pub date_from: Option<String>,
    pub date_to: Option<String>,
    pub connection_id: Option<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
    pub search_query: Option<String>,
    pub sort_by: Option<String>,
    pub sort_desc: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct YmRealizationListItemDto {
    pub id: String,
    pub document_no: String,
    pub document_date: String,
    pub lines_count: i32,
    pub total_sales_revenue: f64,
    pub total_return_revenue: f64,
    pub net_revenue: f64,
    pub connection_id: String,
    pub connection_name: Option<String>,
    pub organization_name: Option<String>,
    pub fetched_at: String,
    pub is_posted: bool,
    /// Модель фасилитации (FBS/FBY/…) документа реализации.
    pub placement_type: Option<String>,
    /// campaignId кампании документа реализации.
    pub campaign_id: Option<String>,
    /// Итоговое расхождение по доставкам (отчёт − заказы a013) = «Сумма разница/Итоги»
    /// блока «Доставки» в карточке. ≈0 ⇒ доставки сходятся.
    pub delivery_discrepancy: f64,
    /// Итоговое расхождение по возвратам (отчёт − возвраты a016).
    pub returns_discrepancy: f64,
}

impl From<YmRealizationListRow> for YmRealizationListItemDto {
    fn from(row: YmRealizationListRow) -> Self {
        Self {
            id: row.id,
            document_no: row.document_no,
            document_date: row.document_date,
            lines_count: row.lines_count,
            total_sales_revenue: row.total_sales_revenue,
            total_return_revenue: row.total_return_revenue,
            net_revenue: row.net_revenue,
            connection_id: row.connection_id,
            connection_name: row.connection_name,
            organization_name: row.organization_name,
            fetched_at: row.fetched_at,
            is_posted: row.is_posted,
            placement_type: row.placement_type,
            campaign_id: row.campaign_id,
            delivery_discrepancy: 0.0,
            returns_discrepancy: 0.0,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct PaginatedResponse {
    pub items: Vec<YmRealizationListItemDto>,
    pub total: usize,
    pub page: usize,
    pub page_size: usize,
    pub total_pages: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct YmRealizationDetailsDto {
    pub id: String,
    pub document_no: String,
    pub document_date: String,
    pub connection_id: String,
    pub connection_name: Option<String>,
    pub organization_id: String,
    pub organization_name: Option<String>,
    pub marketplace_id: String,
    /// Модель фасилитации (FBS/FBY/…) и campaignId кампании документа.
    pub placement_type: Option<String>,
    pub campaign_id: Option<String>,
    pub total_sales_revenue: f64,
    pub total_return_revenue: f64,
    pub net_revenue: f64,
    pub source: String,
    pub fetched_at: String,
    pub created_at: String,
    pub updated_at: String,
    pub is_posted: bool,
    /// Строки-продажи и строки-возвраты — раздельно (физически не смешиваются).
    pub sales_lines: Vec<YmRealizationLine>,
    pub return_lines: Vec<YmRealizationLine>,
    /// Имена распознанных позиций a007: marketplace_product_ref → имя (для ссылок).
    pub product_names: std::collections::HashMap<String, String>,
}

/// Подтягивает имена a007 пачкой: ref → имя (`description`, фолбэк `article`).
/// Пустые/нерезолвящиеся ref пропускаются.
async fn resolve_product_names(refs: Vec<String>) -> std::collections::HashMap<String, String> {
    use std::collections::HashMap;
    let mut out: HashMap<String, String> = HashMap::new();
    for r in refs {
        let r = r.trim().to_string();
        if r.is_empty() || out.contains_key(&r) {
            continue;
        }
        let Ok(uuid) = Uuid::parse_str(&r) else {
            continue;
        };
        if let Ok(Some(product)) =
            crate::domain::a007_marketplace_product::service::get_by_id(uuid).await
        {
            let name = if product.base.description.trim().is_empty() {
                product.article.clone()
            } else {
                product.base.description.clone()
            };
            out.insert(r, name);
        }
    }
    out
}

pub async fn list_paginated(
    Query(query): Query<ListQuery>,
) -> Result<Json<PaginatedResponse>, ApiError> {
    let page_size = query.limit.unwrap_or(100);
    let offset = query.offset.unwrap_or(0);
    let page = if page_size > 0 { offset / page_size } else { 0 };

    let list_query = YmRealizationListQuery {
        date_from: query.date_from,
        date_to: query.date_to,
        connection_id: query.connection_id,
        search_query: query.search_query,
        sort_by: query.sort_by.unwrap_or_else(|| "document_date".to_string()),
        sort_desc: query.sort_desc.unwrap_or(true),
        limit: page_size,
        offset,
    };

    match a034_ym_realization::service::list_paginated(list_query).await {
        Ok(result) => {
            let total_pages = if page_size > 0 {
                (result.total + page_size - 1) / page_size
            } else {
                1
            };
            // Расхождения по доставкам/возвратам считаем по строкам текущей страницы
            // (отчёт − заказы), теми же суммами, что и блоки сводки в карточке.
            let mut items: Vec<YmRealizationListItemDto> = Vec::with_capacity(result.items.len());
            for row in result.items {
                let mut dto: YmRealizationListItemDto = row.into();
                if let Ok((deliv, ret)) = compute_row_discrepancies(&dto).await {
                    dto.delivery_discrepancy = deliv;
                    dto.returns_discrepancy = ret;
                }
                items.push(dto);
            }
            Ok(Json(PaginatedResponse {
                items,
                total: result.total,
                page,
                page_size,
                total_pages,
            }))
        }
        Err(e) => {
            tracing::error!("Failed to list YM realization documents: {}", e);
            Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into())
        }
    }
}

/// Итоговые расхождения строки списка (доставки, возвраты) = отчёт − заказы,
/// согласованы с блоками сводки в карточке. Возвраты требуют order_id из строк
/// документа, поэтому подгружаем документ.
async fn compute_row_discrepancies(dto: &YmRealizationListItemDto) -> anyhow::Result<(f64, f64)> {
    let uuid = Uuid::parse_str(&dto.id)?;
    let Some(doc) = a034_ym_realization::service::get_by_id(uuid).await? else {
        return Ok((0.0, 0.0));
    };

    // Доставки: заказы a013 той же модели (кампании), доставленные в дату документа.
    let order_lines = crate::domain::a013_ym_order::repository::list_lines_by_delivery_day(
        &doc.header.connection_id,
        &doc.header.document_date,
        doc.header.placement_type.as_deref(),
    )
    .await
    .unwrap_or_default();
    let orders_deliv: f64 = order_lines.iter().map(|l| l.buyer_price * l.qty).sum();
    let delivery_discrepancy = doc.totals.sales_revenue - orders_deliv;

    // Возвраты: возвраты a016 по order_id из строк-возвратов документа.
    let returns_ybuh = build_ybuh_side_by_sku(&doc.return_lines);
    let order_ids: Vec<i64> = returns_ybuh
        .keys()
        .filter_map(|(o, _)| o.parse::<i64>().ok())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    let orders_ret: f64 = if order_ids.is_empty() {
        0.0
    } else {
        let a016 = crate::domain::a016_ym_returns::repository::list_by_order_ids(&order_ids)
            .await
            .unwrap_or_default();
        build_a016_returns_side(&a016)
            .values()
            .map(|s| s.amount)
            .sum()
    };
    let returns_discrepancy = doc.totals.return_revenue - orders_ret;

    Ok((delivery_discrepancy, returns_discrepancy))
}

pub async fn get_by_id(Path(id): Path<String>) -> Result<Json<YmRealizationDetailsDto>, ApiError> {
    let uuid = Uuid::parse_str(&id).map_err(|_| axum::http::StatusCode::BAD_REQUEST)?;
    let doc = match a034_ym_realization::service::get_by_id(uuid).await {
        Ok(Some(doc)) => doc,
        Ok(None) => return Err(axum::http::StatusCode::NOT_FOUND.into()),
        Err(e) => {
            tracing::error!("Failed to get YM realization document {}: {}", id, e);
            return Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into());
        }
    };

    match build_details_dto(doc).await {
        Ok(dto) => Ok(Json(dto)),
        Err(e) => {
            tracing::error!("Failed to enrich YM realization document {}: {}", id, e);
            Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into())
        }
    }
}

async fn build_details_dto(doc: YmRealization) -> anyhow::Result<YmRealizationDetailsDto> {
    let connection_name = resolve_connection_name(&doc.header.connection_id).await?;
    let organization_name = resolve_organization_name(&doc.header.organization_id).await?;
    let product_names = resolve_product_names(
        doc.sales_lines
            .iter()
            .chain(doc.return_lines.iter())
            .filter_map(|l| l.marketplace_product_ref.clone())
            .collect(),
    )
    .await;

    Ok(YmRealizationDetailsDto {
        id: doc.base.id.as_string(),
        document_no: doc.header.document_no.clone(),
        document_date: doc.header.document_date.clone(),
        connection_id: doc.header.connection_id.clone(),
        connection_name,
        organization_id: doc.header.organization_id.clone(),
        organization_name,
        marketplace_id: doc.header.marketplace_id.clone(),
        placement_type: doc.header.placement_type.clone(),
        campaign_id: doc.header.campaign_id.clone(),
        total_sales_revenue: doc.totals.sales_revenue,
        total_return_revenue: doc.totals.return_revenue,
        net_revenue: doc.totals.net_revenue,
        source: doc.source_meta.source.clone(),
        fetched_at: doc.source_meta.fetched_at.clone(),
        created_at: doc.base.metadata.created_at.to_rfc3339(),
        updated_at: doc.base.metadata.updated_at.to_rfc3339(),
        is_posted: doc.is_posted || doc.base.metadata.is_posted,
        sales_lines: doc.sales_lines,
        return_lines: doc.return_lines,
        product_names,
    })
}

async fn resolve_connection_name(connection_id: &str) -> anyhow::Result<Option<String>> {
    let Some(uuid) = Uuid::parse_str(connection_id).ok() else {
        return Ok(None);
    };
    let connection = crate::domain::a006_connection_mp::service::get_by_id(uuid).await?;
    Ok(connection.map(|item| item.base.description))
}

async fn resolve_organization_name(organization_id: &str) -> anyhow::Result<Option<String>> {
    let Some(uuid) = Uuid::parse_str(organization_id).ok() else {
        return Ok(None);
    };
    let organization = crate::domain::a002_organization::service::get_by_id(
        crate::shared::data::db::get_connection(),
        uuid,
    )
    .await?;
    Ok(organization.map(|item| item.base.description))
}

pub async fn post_document(Path(id): Path<String>) -> Result<Json<serde_json::Value>, ApiError> {
    let uuid = Uuid::parse_str(&id).map_err(|_| axum::http::StatusCode::BAD_REQUEST)?;
    a034_ym_realization::service::post_document(uuid)
        .await
        .map_err(|e| {
            tracing::error!("Failed to post YM realization document {}: {}", id, e);
            ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        })?;
    Ok(Json(serde_json::json!({"success": true})))
}

pub async fn unpost_document(Path(id): Path<String>) -> Result<Json<serde_json::Value>, ApiError> {
    let uuid = Uuid::parse_str(&id).map_err(|_| axum::http::StatusCode::BAD_REQUEST)?;
    a034_ym_realization::service::unpost_document(uuid)
        .await
        .map_err(|e| {
            tracing::error!("Failed to unpost YM realization document {}: {}", id, e);
            ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        })?;
    Ok(Json(serde_json::json!({"success": true})))
}

pub async fn get_general_ledger_entries(
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let rows = crate::general_ledger::repository::list_by_registrator("a034_ym_realization", &id)
        .await
        .map_err(|e| {
            tracing::error!(
                "Failed to get general ledger entries for a034 {}: {}",
                id,
                e
            );
            ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        })?;

    let general_ledger_entries = rows.into_iter().map(to_journal_dto).collect::<Vec<_>>();

    Ok(Json(
        serde_json::json!({ "general_ledger_entries": general_ledger_entries }),
    ))
}

fn to_journal_dto(row: crate::general_ledger::repository::Model) -> GeneralLedgerEntryDto {
    crate::general_ledger::dto::entry_to_dto(row)
}

// ─────────────────────────────────────────────────────────────────────────────
// Мини-отчёт «Платежи YM (p907)»: fina-детализация доставленных заказов и
// возвратов этого дня (кабинет + order_delivery_date = дата документа).
// ─────────────────────────────────────────────────────────────────────────────

const SOURCE_BUYER_PAYMENT: &str = "Платёж покупателя";

#[derive(Debug, Serialize)]
pub struct PaymentDetailRow {
    pub kind: String,
    pub order_id: Option<String>,
    pub marketplace_product_ref: Option<String>,
    pub product_name: Option<String>,
    pub nomenclature: String,
    pub payment_date: Option<String>,
    pub return_amount: f64,
    pub revenue_amount: f64,
    pub return_qty: f64,
    pub revenue_qty: f64,
}

#[derive(Debug, Default, Serialize)]
pub struct PaymentDetailTotals {
    pub return_amount: f64,
    pub revenue_amount: f64,
    pub return_qty: f64,
    pub revenue_qty: f64,
}

#[derive(Debug, Serialize)]
pub struct PaymentDetailResponse {
    pub rows: Vec<PaymentDetailRow>,
    pub totals: PaymentDetailTotals,
}

pub async fn get_payment_detail(
    Path(id): Path<String>,
) -> Result<Json<PaymentDetailResponse>, ApiError> {
    let uuid = Uuid::parse_str(&id).map_err(|_| axum::http::StatusCode::BAD_REQUEST)?;
    let doc = match a034_ym_realization::service::get_by_id(uuid).await {
        Ok(Some(doc)) => doc,
        Ok(None) => return Err(axum::http::StatusCode::NOT_FOUND.into()),
        Err(e) => {
            tracing::error!("Failed to get YM realization document {}: {}", id, e);
            return Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into());
        }
    };

    let day = doc.header.document_date.clone();
    let connection_id = doc.header.connection_id.clone();

    let movements =
        crate::projections::p907_ym_payment_report::repository::list_buyer_movements_by_delivery_day(
            &connection_id,
            &day,
        )
        .await
        .map_err(|e| {
            tracing::error!("Failed to load p907 buyer movements for a034 {}: {}", id, e);
            ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        })?;

    let product_names = resolve_product_names(
        movements
            .iter()
            .filter_map(|r| r.marketplace_product_ref.clone())
            .collect(),
    )
    .await;

    let mut totals = PaymentDetailTotals::default();
    let mut rows = Vec::with_capacity(movements.len());
    for row in movements {
        let is_revenue = row
            .transaction_source
            .as_deref()
            .map(|s| s.trim() == SOURCE_BUYER_PAYMENT)
            .unwrap_or(false);
        let amount = row.transaction_sum.unwrap_or(0.0);
        let qty = row.count.unwrap_or(0) as f64;
        // Возвраты в p907 хранятся отрицательными — в колонки возврата кладём модуль,
        // знак несёт тип строки.
        let (revenue_amount, return_amount, revenue_qty, return_qty) = if is_revenue {
            (amount, 0.0, qty, 0.0)
        } else {
            (0.0, amount.abs(), 0.0, qty.abs())
        };

        let nomenclature = row
            .offer_or_service_name
            .clone()
            .filter(|s| !s.trim().is_empty())
            .or_else(|| row.shop_sku.clone())
            .unwrap_or_default();

        totals.revenue_amount += revenue_amount;
        totals.return_amount += return_amount;
        totals.revenue_qty += revenue_qty;
        totals.return_qty += return_qty;

        let product_name = row
            .marketplace_product_ref
            .as_deref()
            .and_then(|r| product_names.get(r).cloned());

        rows.push(PaymentDetailRow {
            kind: if is_revenue {
                "Выручка"
            } else {
                "Возврат"
            }
            .to_string(),
            order_id: row.order_id.map(|v| v.to_string()),
            marketplace_product_ref: row.marketplace_product_ref.clone(),
            product_name,
            nomenclature,
            payment_date: row.transaction_date.clone(),
            return_amount,
            revenue_amount,
            return_qty,
            revenue_qty,
        });
    }

    Ok(Json(PaymentDetailResponse { rows, totals }))
}

// ─────────────────────────────────────────────────────────────────────────────
// Сверка: строки реализации (ybuh, a034) vs позиции заказов (a013). Разделена на
// две независимые сверки:
//   • Сверка реализации (продажи) — против заказов, доставленных в дату документа
//     (delivery_date == document_date). Блоки: Совпадение / Расходятся / Частичная
//     (неполная) доставка / Нет среди заказов / Нет в реализации / Не распознано.
//   • Сверка возвратов — против исходных заказов по order_id (любая дата доставки).
//     Блоки: Возврат в пределах заказа / Возврат превышает заказ / Товар не в
//     заказе / Заказ не найден / Не распознано.
// Сверяем количество И сумму выручки покупателя (сторона a013: buyer_price * qty).
// ─────────────────────────────────────────────────────────────────────────────

const RECON_EPS: f64 = 0.01;

use std::collections::BTreeMap;

type SideMap = BTreeMap<(String, String), ReconSide>;

#[derive(Debug, Serialize)]
pub struct ReconRow {
    pub order_id: String,
    pub marketplace_product_ref: Option<String>,
    pub product_name: Option<String>,
    pub shop_sku: String,
    pub nomenclature: String,
    /// Статус заказа a013 (для строк со стороны заказов).
    pub order_status: Option<String>,
    /// Дата доставки заказа a013 (может отличаться от даты документа).
    pub order_delivery_date: Option<String>,
    pub ybuh_amount: f64,
    pub order_amount: f64,
    pub amount_delta: f64,
    pub ybuh_qty: f64,
    pub order_qty: f64,
    pub qty_delta: f64,
}

#[derive(Debug, Serialize)]
pub struct ReconGroup {
    pub category: String,
    pub rows: Vec<ReconRow>,
    pub count: i64,
    pub ybuh_total: f64,
    pub order_total: f64,
    pub delta_total: f64,
}

#[derive(Debug, Serialize)]
pub struct ReconResponse {
    pub groups: Vec<ReconGroup>,
}

#[derive(Default, Clone)]
struct ReconSide {
    amount: f64,
    qty: f64,
    shop_sku: String,
    nomenclature: String,
    status: Option<String>,
    /// a007 marketplace_product_ref для гиперссылки «Товар» (когда сторона —
    /// реализация a034). Со стороны a016 не заполняется.
    mp_ref: Option<String>,
}

/// Сторона реализации (ybuh, a034). Распознанные позиции — по (order_id, a007),
/// нераспознанные (без a007) — по (order_id, shop_sku). На вход подаётся уже
/// разделённая коллекция (`sales_lines` либо `return_lines`). Суммы положительные.
fn build_ybuh_side(lines: &[YmRealizationLine]) -> (SideMap, SideMap) {
    let mut ybuh: SideMap = BTreeMap::new();
    let mut ybuh_unres: SideMap = BTreeMap::new();
    for line in lines {
        let order = line.order_id.clone().unwrap_or_default();
        let mp_ref = line
            .marketplace_product_ref
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty());
        let side = match mp_ref {
            Some(r) => ybuh.entry((order, r.to_string())).or_default(),
            None => ybuh_unres
                .entry((order, line.shop_sku.clone()))
                .or_default(),
        };
        side.amount += line.revenue_amount;
        side.qty += line.quantity;
        if side.shop_sku.is_empty() {
            side.shop_sku = line.shop_sku.clone();
        }
        if side.nomenclature.is_empty() && !line.offer_name.trim().is_empty() {
            side.nomenclature = line.offer_name.clone();
        }
    }
    (ybuh, ybuh_unres)
}

/// Сторона заказов (a013). Сумма позиции = `buyer_price * qty`. Ключи аналогично
/// стороне реализации.
fn build_orders_side(
    lines: &[crate::domain::a013_ym_order::repository::DeliveredOrderLine],
) -> (SideMap, SideMap) {
    let mut orders: SideMap = BTreeMap::new();
    let mut orders_unres: SideMap = BTreeMap::new();
    for line in lines {
        let order = line.order_no.clone();
        let mp_ref = line
            .marketplace_product_ref
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty());
        let side = match mp_ref {
            Some(r) => orders.entry((order, r.to_string())).or_default(),
            None => orders_unres
                .entry((order, line.shop_sku.clone()))
                .or_default(),
        };
        side.amount += line.buyer_price * line.qty;
        side.qty += line.qty;
        if side.shop_sku.is_empty() {
            side.shop_sku = line.shop_sku.clone();
        }
        if side.nomenclature.is_empty() && !line.name.trim().is_empty() {
            side.nomenclature = line.name.clone();
        }
        if side.status.is_none() {
            side.status = line.status_norm.clone();
        }
    }
    (orders, orders_unres)
}

async fn load_doc_for_recon(id: &str) -> Result<YmRealization, ApiError> {
    let uuid = Uuid::parse_str(id).map_err(|_| axum::http::StatusCode::BAD_REQUEST)?;
    match a034_ym_realization::service::get_by_id(uuid).await {
        Ok(Some(doc)) => Ok(doc),
        Ok(None) => Err(axum::http::StatusCode::NOT_FOUND.into()),
        Err(e) => {
            tracing::error!("Failed to get YM realization document {}: {}", id, e);
            Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into())
        }
    }
}

// ── Сверка реализации (продажи vs заказы, доставленные в дату документа) ──
pub async fn get_reconciliation_sales(
    Path(id): Path<String>,
) -> Result<Json<ReconResponse>, ApiError> {
    let doc = load_doc_for_recon(&id).await?;
    let groups = compute_recon_sales_groups(&doc).await.map_err(|e| {
        tracing::error!("Failed to compute a034 sales reconciliation {}: {}", id, e);
        ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
    })?;
    Ok(Json(ReconResponse { groups }))
}

/// Ядро «Сверки реализации»: возвращает группы сверки (тот же набор и логика, что
/// видит пользователь на вкладке). Используется и хендлером вкладки, и сводкой
/// «Итоги», чтобы цифры гарантированно совпадали.
async fn compute_recon_sales_groups(doc: &YmRealization) -> anyhow::Result<Vec<ReconGroup>> {
    use std::collections::HashSet;

    let day = doc.header.document_date.clone();
    let connection_id = doc.header.connection_id.clone();

    let (ybuh, ybuh_unres) = build_ybuh_side(&doc.sales_lines);

    let order_lines = crate::domain::a013_ym_order::repository::list_lines_by_delivery_day(
        &connection_id,
        &day,
        doc.header.placement_type.as_deref(),
    )
    .await?;
    let (orders, orders_unres) = build_orders_side(&order_lines);

    let refs: Vec<String> = ybuh
        .keys()
        .chain(orders.keys())
        .map(|(_, r)| r.clone())
        .collect();
    let product_names = resolve_product_names(refs).await;

    // Даты доставки по всем order_id (включая заказы, доставленные не в дату
    // документа — для строк «Нет среди заказов»).
    let order_nos: Vec<String> = collect_order_nos(&[&ybuh, &ybuh_unres, &orders, &orders_unres]);
    let delivery_dates =
        crate::domain::a013_ym_order::repository::delivery_dates_by_order_nos(&order_nos)
            .await
            .unwrap_or_default();

    let ybuh_orders: HashSet<String> = ybuh.keys().map(|(o, _)| o.clone()).collect();
    let order_orders: HashSet<String> = orders.keys().map(|(o, _)| o.clone()).collect();

    let mut keys: Vec<(String, String)> = ybuh.keys().cloned().collect();
    for key in orders.keys() {
        if !ybuh.contains_key(key) {
            keys.push(key.clone());
        }
    }
    keys.sort();

    let mut g_match = new_group("Совпадение");
    let mut g_diff = new_group("Расходятся суммы/кол-во");
    let mut g_partial = new_group("Частичная/неполная доставка");
    let mut g_no_order = new_group("Нет среди заказов");
    let mut g_no_real = new_group("Нет в реализации");

    for key in keys {
        let y = ybuh.get(&key);
        let o = orders.get(&key);
        let row = build_recon_row(
            &key,
            y,
            o,
            product_names.get(&key.1).cloned(),
            delivery_dates.get(&key.0).cloned(),
        );
        let group = match (y.is_some(), o.is_some()) {
            (true, true) => {
                if (row.ybuh_amount - row.order_amount).abs() <= RECON_EPS
                    && (row.ybuh_qty - row.order_qty).abs() <= RECON_EPS
                {
                    &mut g_match
                } else {
                    &mut g_diff
                }
            }
            (true, false) => {
                if order_orders.contains(&key.0) {
                    &mut g_partial
                } else {
                    &mut g_no_order
                }
            }
            (false, true) => {
                if ybuh_orders.contains(&key.0) {
                    &mut g_partial
                } else {
                    &mut g_no_real
                }
            }
            (false, false) => continue,
        };
        push_recon_row(group, row);
    }

    let mut g_unrecognized = new_group("Не распознано (нет a007)");
    push_unrecognized(
        &mut g_unrecognized,
        ybuh_unres,
        orders_unres,
        &delivery_dates,
    );

    Ok(vec![
        g_match,
        g_diff,
        g_partial,
        g_no_order,
        g_no_real,
        g_unrecognized,
    ])
}

// ── Сверка возвратов: возвраты из отчёта о реализации (a034) vs возвраты a016 ──
// Ключ сопоставления — (order_id, shop_sku). Сумма со стороны a016 = header.amount
// возврата, разнесённая по строкам пропорционально количеству. Учитываются возвраты
// a016 любого статуса. Информация по заказу (order_id) сохраняется как ключ-ссылка.
pub async fn get_reconciliation_returns(
    Path(id): Path<String>,
) -> Result<Json<ReconResponse>, ApiError> {
    let doc = load_doc_for_recon(&id).await?;
    let groups = compute_recon_returns_groups(&doc).await.map_err(|e| {
        tracing::error!(
            "Failed to compute a034 returns reconciliation {}: {}",
            id,
            e
        );
        ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
    })?;
    Ok(Json(ReconResponse { groups }))
}

/// Ядро «Сверки возвратов»: те же группы, что на вкладке. Используется и
/// хендлером вкладки, и сводкой «Итоги».
async fn compute_recon_returns_groups(doc: &YmRealization) -> anyhow::Result<Vec<ReconGroup>> {
    // Сторона реализации (a034) — все строки-возвраты по (order_id, shop_sku).
    let ybuh = build_ybuh_side_by_sku(&doc.return_lines);

    // Возвраты a016 по тем же order_id.
    let order_ids: Vec<i64> = ybuh
        .keys()
        .filter_map(|(o, _)| o.parse::<i64>().ok())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    let a016_returns =
        crate::domain::a016_ym_returns::repository::list_by_order_ids(&order_ids).await?;
    let a016 = build_a016_returns_side(&a016_returns);

    // Имена товаров (a007) — только со стороны реализации (a016 не хранит a007-ref).
    let refs: Vec<String> = ybuh.values().filter_map(|s| s.mp_ref.clone()).collect();
    let product_names = resolve_product_names(refs).await;

    let mut g_match = new_group("Совпадение");
    let mut g_diff = new_group("Расходятся суммы/кол-во");
    let mut g_no_a016 = new_group("Нет в возвратах a016");
    let mut g_no_real = new_group("Нет в отчёте о реализации");

    let mut keys: Vec<(String, String)> = ybuh.keys().cloned().collect();
    for key in a016.keys() {
        if !ybuh.contains_key(key) {
            keys.push(key.clone());
        }
    }
    keys.sort();

    for key in keys {
        let y = ybuh.get(&key);
        let o = a016.get(&key);
        let product_name = y
            .and_then(|s| s.mp_ref.as_deref())
            .and_then(|r| product_names.get(r).cloned());
        let row = build_recon_row_sku(&key, y, o, product_name);
        let group = match (y.is_some(), o.is_some()) {
            (true, true) => {
                if (row.ybuh_amount - row.order_amount).abs() <= RECON_EPS
                    && (row.ybuh_qty - row.order_qty).abs() <= RECON_EPS
                {
                    &mut g_match
                } else {
                    &mut g_diff
                }
            }
            (true, false) => &mut g_no_a016,
            (false, true) => &mut g_no_real,
            (false, false) => continue,
        };
        push_recon_row(group, row);
    }

    Ok(vec![g_match, g_diff, g_no_a016, g_no_real])
}

/// Сторона реализации (a034) для сверки возвратов — ключ (order_id, shop_sku).
/// В отличие от `build_ybuh_side`, не разделяет распознанные/нераспознанные: ключ
/// всегда по SKU продавца (так совпадает с a016). a007-ref сохраняется для ссылки.
fn build_ybuh_side_by_sku(lines: &[YmRealizationLine]) -> SideMap {
    let mut map: SideMap = BTreeMap::new();
    for line in lines {
        let order = line.order_id.clone().unwrap_or_default();
        let side = map.entry((order, line.shop_sku.clone())).or_default();
        side.amount += line.revenue_amount;
        side.qty += line.quantity;
        if side.shop_sku.is_empty() {
            side.shop_sku = line.shop_sku.clone();
        }
        if side.nomenclature.is_empty() && !line.offer_name.trim().is_empty() {
            side.nomenclature = line.offer_name.clone();
        }
        if side.mp_ref.is_none() {
            side.mp_ref = line
                .marketplace_product_ref
                .as_deref()
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .map(|v| v.to_string());
        }
    }
    map
}

/// Сторона a016 для сверки возвратов — ключ (order_id, shop_sku). Сумма возврата
/// (`header.amount`) разносится по строкам пропорционально `count` (при нулевом
/// суммарном count — поровну). Статус строки = `refund_status` возврата.
fn build_a016_returns_side(
    returns: &[contracts::domain::a016_ym_returns::aggregate::YmReturn],
) -> SideMap {
    let mut map: SideMap = BTreeMap::new();
    for ret in returns {
        let order = ret.header.order_id.to_string();
        let header_amount = ret.header.amount.unwrap_or(0.0);
        let total_count: i32 = ret.lines.iter().map(|l| l.count).sum();
        let n_lines = ret.lines.len().max(1) as f64;
        for line in &ret.lines {
            let alloc = if total_count > 0 {
                header_amount * (line.count as f64 / total_count as f64)
            } else {
                header_amount / n_lines
            };
            let side = map
                .entry((order.clone(), line.shop_sku.clone()))
                .or_default();
            side.amount += alloc;
            side.qty += line.count as f64;
            if side.shop_sku.is_empty() {
                side.shop_sku = line.shop_sku.clone();
            }
            if side.nomenclature.is_empty() && !line.name.trim().is_empty() {
                side.nomenclature = line.name.clone();
            }
            if side.status.is_none() && !ret.state.refund_status.trim().is_empty() {
                side.status = Some(ret.state.refund_status.clone());
            }
        }
    }
    map
}

/// Строка сверки с ключом (order_id, shop_sku). `order_amount`/`order_qty` несут
/// сторону a016. `order_status` = refund_status возврата a016.
fn build_recon_row_sku(
    key: &(String, String),
    y: Option<&ReconSide>,
    o: Option<&ReconSide>,
    product_name: Option<String>,
) -> ReconRow {
    let nomenclature = y
        .map(|s| s.nomenclature.clone())
        .filter(|s| !s.is_empty())
        .or_else(|| o.map(|s| s.nomenclature.clone()))
        .unwrap_or_default();
    let ybuh_amount = y.map(|s| s.amount).unwrap_or(0.0);
    let order_amount = o.map(|s| s.amount).unwrap_or(0.0);
    let ybuh_qty = y.map(|s| s.qty).unwrap_or(0.0);
    let order_qty = o.map(|s| s.qty).unwrap_or(0.0);
    ReconRow {
        order_id: key.0.clone(),
        marketplace_product_ref: y.and_then(|s| s.mp_ref.clone()),
        product_name,
        shop_sku: key.1.clone(),
        nomenclature,
        order_status: o.and_then(|s| s.status.clone()),
        order_delivery_date: None,
        ybuh_amount,
        order_amount,
        amount_delta: order_amount - ybuh_amount,
        ybuh_qty,
        order_qty,
        qty_delta: order_qty - ybuh_qty,
    }
}

/// Уникальные непустые order_id (`document_no`) по нескольким картам сторон.
fn collect_order_nos(maps: &[&SideMap]) -> Vec<String> {
    use std::collections::BTreeSet;
    let mut set: BTreeSet<String> = BTreeSet::new();
    for m in maps {
        for (o, _) in m.keys() {
            if !o.is_empty() {
                set.insert(o.clone());
            }
        }
    }
    set.into_iter().collect()
}

/// Строка сверки для распознанной позиции (ключ = order_id + a007).
fn build_recon_row(
    key: &(String, String),
    y: Option<&ReconSide>,
    o: Option<&ReconSide>,
    product_name: Option<String>,
    order_delivery_date: Option<String>,
) -> ReconRow {
    let nomenclature = y
        .map(|s| s.nomenclature.clone())
        .filter(|s| !s.is_empty())
        .or_else(|| o.map(|s| s.nomenclature.clone()))
        .unwrap_or_default();
    let shop_sku = y
        .map(|s| s.shop_sku.clone())
        .filter(|s| !s.is_empty())
        .or_else(|| o.map(|s| s.shop_sku.clone()))
        .unwrap_or_default();
    let ybuh_amount = y.map(|s| s.amount).unwrap_or(0.0);
    let order_amount = o.map(|s| s.amount).unwrap_or(0.0);
    let ybuh_qty = y.map(|s| s.qty).unwrap_or(0.0);
    let order_qty = o.map(|s| s.qty).unwrap_or(0.0);
    ReconRow {
        order_id: key.0.clone(),
        marketplace_product_ref: Some(key.1.clone()),
        product_name,
        shop_sku,
        nomenclature,
        order_status: o.and_then(|s| s.status.clone()),
        order_delivery_date,
        ybuh_amount,
        order_amount,
        amount_delta: order_amount - ybuh_amount,
        ybuh_qty,
        order_qty,
        qty_delta: order_qty - ybuh_qty,
    }
}

/// Нераспознанные строки (нет a007) — по каждой стороне отдельной строкой.
fn push_unrecognized(
    group: &mut ReconGroup,
    ybuh_unres: SideMap,
    orders_unres: SideMap,
    delivery_dates: &std::collections::HashMap<String, String>,
) {
    for ((order, sku), side) in ybuh_unres {
        let order_delivery_date = delivery_dates.get(&order).cloned();
        push_recon_row(
            group,
            ReconRow {
                order_id: order,
                marketplace_product_ref: None,
                product_name: None,
                shop_sku: sku,
                nomenclature: side.nomenclature,
                order_status: None,
                order_delivery_date,
                ybuh_amount: side.amount,
                order_amount: 0.0,
                amount_delta: -side.amount,
                ybuh_qty: side.qty,
                order_qty: 0.0,
                qty_delta: -side.qty,
            },
        );
    }
    for ((order, sku), side) in orders_unres {
        let order_delivery_date = delivery_dates.get(&order).cloned();
        push_recon_row(
            group,
            ReconRow {
                order_id: order,
                marketplace_product_ref: None,
                product_name: None,
                shop_sku: sku,
                nomenclature: side.nomenclature,
                order_status: side.status.clone(),
                order_delivery_date,
                ybuh_amount: 0.0,
                order_amount: side.amount,
                amount_delta: side.amount,
                ybuh_qty: 0.0,
                order_qty: side.qty,
                qty_delta: side.qty,
            },
        );
    }
}

fn new_group(category: &str) -> ReconGroup {
    ReconGroup {
        category: category.to_string(),
        rows: Vec::new(),
        count: 0,
        ybuh_total: 0.0,
        order_total: 0.0,
        delta_total: 0.0,
    }
}

fn push_recon_row(group: &mut ReconGroup, row: ReconRow) {
    group.count += 1;
    group.ybuh_total += row.ybuh_amount;
    group.order_total += row.order_amount;
    group.delta_total += row.amount_delta;
    group.rows.push(row);
}

// ─────────────────────────────────────────────────────────────────────────────
// Отчёт «Заказы по дате доставки»: все заказы кабинета с delivery_date = дате
// документа (строится на лету, не хранится). Сумма = buyer_price * qty.
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct DeliveryOrderRow {
    pub order_no: String,
    pub status_norm: Option<String>,
    pub shop_sku: String,
    pub nomenclature: String,
    pub marketplace_product_ref: Option<String>,
    pub product_name: Option<String>,
    pub qty: f64,
    pub buyer_price: f64,
    pub amount: f64,
}

#[derive(Debug, Default, Serialize)]
pub struct DeliveryOrdersTotals {
    pub qty: f64,
    pub amount: f64,
}

#[derive(Debug, Serialize)]
pub struct DeliveryOrdersResponse {
    pub rows: Vec<DeliveryOrderRow>,
    pub totals: DeliveryOrdersTotals,
}

pub async fn get_delivery_orders(
    Path(id): Path<String>,
) -> Result<Json<DeliveryOrdersResponse>, ApiError> {
    let uuid = Uuid::parse_str(&id).map_err(|_| axum::http::StatusCode::BAD_REQUEST)?;
    let doc = match a034_ym_realization::service::get_by_id(uuid).await {
        Ok(Some(doc)) => doc,
        Ok(None) => return Err(axum::http::StatusCode::NOT_FOUND.into()),
        Err(e) => {
            tracing::error!("Failed to get YM realization document {}: {}", id, e);
            return Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into());
        }
    };

    let day = doc.header.document_date.clone();
    let connection_id = doc.header.connection_id.clone();

    let order_lines = crate::domain::a013_ym_order::repository::list_lines_by_delivery_day(
        &connection_id,
        &day,
        doc.header.placement_type.as_deref(),
    )
    .await
    .map_err(|e| {
        tracing::error!("Failed to load a013 delivered lines for a034 {}: {}", id, e);
        ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
    })?;

    let product_names = resolve_product_names(
        order_lines
            .iter()
            .filter_map(|l| l.marketplace_product_ref.clone())
            .collect(),
    )
    .await;

    let mut totals = DeliveryOrdersTotals::default();
    let mut rows = Vec::with_capacity(order_lines.len());
    for line in order_lines {
        let amount = line.buyer_price * line.qty;
        totals.qty += line.qty;
        totals.amount += amount;
        let product_name = line
            .marketplace_product_ref
            .as_deref()
            .and_then(|r| product_names.get(r).cloned());
        rows.push(DeliveryOrderRow {
            order_no: line.order_no,
            status_norm: line.status_norm,
            shop_sku: line.shop_sku,
            nomenclature: line.name,
            marketplace_product_ref: line.marketplace_product_ref,
            product_name,
            qty: line.qty,
            buyer_price: line.buyer_price,
            amount,
        });
    }

    Ok(Json(DeliveryOrdersResponse { rows, totals }))
}

// ─────────────────────────────────────────────────────────────────────────────
// Догрузка отсутствующих заказов. На вкладке «Сверка реализации» блок «Нет среди
// заказов» собирает позиции отчёта о реализации (a034), чьи заказы отсутствуют в
// системе (нет документа a013). Эта команда берёт все order_id, упомянутые в
// документе реализации, отбирает те, которых нет в a013, и догружает их по одному
// через YM API (GET /campaigns/{campaignId}/orders/{orderId}), сохраняя как a013.
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct FetchMissingOrdersResponse {
    /// Сколько уникальных заказов из документа отсутствовало в a013.
    pub total_missing: usize,
    /// Успешно догружено и сохранено.
    pub fetched: usize,
    /// Не удалось догрузить/сохранить.
    pub failed: usize,
    /// Сообщения об ошибках по проблемным заказам.
    pub errors: Vec<String>,
}

pub async fn fetch_missing_orders(
    Path(id): Path<String>,
) -> Result<Json<FetchMissingOrdersResponse>, ApiError> {
    use std::collections::BTreeSet;

    let doc = load_doc_for_recon(&id).await?;

    // Уникальные непустые order_id, упомянутые в строках документа (продажи + возвраты).
    let mut order_nos: BTreeSet<String> = BTreeSet::new();
    for line in doc.sales_lines.iter().chain(doc.return_lines.iter()) {
        if let Some(order) = line.order_id.as_ref() {
            let order = order.trim();
            if !order.is_empty() {
                order_nos.insert(order.to_string());
            }
        }
    }

    // Оставляем только те, которых нет в a013.
    let mut missing: Vec<String> = Vec::new();
    for order_no in order_nos {
        match crate::domain::a013_ym_order::service::get_by_document_no(&order_no).await {
            Ok(Some(_)) => {}
            Ok(None) => missing.push(order_no),
            Err(e) => {
                tracing::error!(
                    "a034 fetch_missing_orders: lookup {} failed: {}",
                    order_no,
                    e
                );
                return Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into());
            }
        }
    }

    let total_missing = missing.len();
    if total_missing == 0 {
        return Ok(Json(FetchMissingOrdersResponse {
            total_missing: 0,
            fetched: 0,
            failed: 0,
            errors: Vec::new(),
        }));
    }

    // Подключение МП (для токена/кампании) и организация (как в импорте u503).
    let connection_uuid = Uuid::parse_str(&doc.header.connection_id)
        .map_err(|_| axum::http::StatusCode::BAD_REQUEST)?;
    let connection =
        match crate::domain::a006_connection_mp::service::get_by_id(connection_uuid).await {
            Ok(Some(c)) => c,
            Ok(None) => return Err(axum::http::StatusCode::NOT_FOUND.into()),
            Err(e) => {
                tracing::error!("a034 fetch_missing_orders: connection load failed: {}", e);
                return Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into());
            }
        };
    let organization_id = doc.header.organization_id.clone();

    let client =
        crate::usecases::u503_import_from_yandex::yandex_api_client::YandexApiClient::new();

    let mut fetched = 0usize;
    let mut failed = 0usize;
    let mut errors: Vec<String> = Vec::new();

    for order_no in missing {
        let order_id: i64 = match order_no.parse() {
            Ok(v) => v,
            Err(_) => {
                failed += 1;
                errors.push(format!("Заказ {}: некорректный ID", order_no));
                continue;
            }
        };

        match client.fetch_order_details(&connection, order_id).await {
            Ok(details) => {
                match crate::usecases::u503_import_from_yandex::processors::order::process_order(
                    &connection,
                    &organization_id,
                    &details,
                    // Точечный дозабор по orderId — placementType кампании здесь
                    // неизвестен; fulfillment_type заполнится при штатном импорте.
                    None,
                )
                .await
                {
                    Ok(_) => fetched += 1,
                    Err(e) => {
                        failed += 1;
                        errors.push(format!("Заказ {}: ошибка сохранения — {}", order_no, e));
                    }
                }
            }
            Err(e) => {
                failed += 1;
                errors.push(format!("Заказ {}: ошибка запроса — {}", order_no, e));
            }
        }
    }

    tracing::info!(
        "a034 fetch_missing_orders ({}): missing={}, fetched={}, failed={}",
        id,
        total_missing,
        fetched,
        failed
    );

    Ok(Json(FetchMissingOrdersResponse {
        total_missing,
        fetched,
        failed,
        errors,
    }))
}

// ─────────────────────────────────────────────────────────────────────────────
// Итоговая сводка сверки (блоки на вкладке «Результат»). Две таблицы за день:
//   • Доставки  — отчёт о реализации (a034, sales_lines) vs заказы a013,
//                 доставленные в дату документа.
//   • Возвраты  — возвраты из отчёта (a034, return_lines) vs возвраты a016.
// Сводка строится из ТЕХ ЖЕ групп, что показывает детальная вкладка сверки
// (compute_recon_sales_groups / compute_recon_returns_groups) — по строке на
// группу. Поэтому цифры сводки в точности равны итогам блоков детализации.
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct ReconSummaryRow {
    /// Категория = название группы детальной сверки (та же, что на вкладке).
    pub category: String,
    pub report_qty: f64,
    pub report_sum: f64,
    pub orders_qty: f64,
    pub orders_sum: f64,
}

#[derive(Debug, Serialize)]
pub struct ReconSummaryResponse {
    pub deliveries: Vec<ReconSummaryRow>,
    pub returns: Vec<ReconSummaryRow>,
}

/// Свернуть группу детальной сверки в строку сводки: суммы берём из итогов группы,
/// количества — суммированием по строкам. Так сводка «Итоги» строится из тех же
/// групп, что показывает вкладка сверки, и цифры гарантированно совпадают.
fn group_to_summary_row(g: &ReconGroup) -> ReconSummaryRow {
    ReconSummaryRow {
        category: g.category.clone(),
        report_qty: g.rows.iter().map(|r| r.ybuh_qty).sum(),
        report_sum: g.ybuh_total,
        orders_qty: g.rows.iter().map(|r| r.order_qty).sum(),
        orders_sum: g.order_total,
    }
}

pub async fn get_reconciliation_summary(
    Path(id): Path<String>,
) -> Result<Json<ReconSummaryResponse>, ApiError> {
    let doc = load_doc_for_recon(&id).await?;

    let deliveries = compute_recon_sales_groups(&doc)
        .await
        .map_err(|e| {
            tracing::error!("a034 recon summary: sales groups failed for {}: {}", id, e);
            ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        })?
        .iter()
        .map(group_to_summary_row)
        .collect();

    let returns = compute_recon_returns_groups(&doc)
        .await
        .map_err(|e| {
            tracing::error!(
                "a034 recon summary: returns groups failed for {}: {}",
                id,
                e
            );
            ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        })?
        .iter()
        .map(group_to_summary_row)
        .collect();

    Ok(Json(ReconSummaryResponse {
        deliveries,
        returns,
    }))
}
