use crate::shared::error::ApiError;
use axum::{extract::Query, Json};
use contracts::dashboards::d402_wb_order_flow::{
    AdvertFlowItem, ClaimFlowItem, OrderFlowItem, P903FlowItem, SaleFlowItem, SupplyFlowItem,
    WbOrderFlowResponse,
};
use contracts::dashboards::d403_ym_order_flow::{
    YmOrderFlowItem, YmOrderFlowResponse, YmPaymentFlowItem, YmRealizationFlowItem,
    YmReturnFlowItem,
};
use contracts::dashboards::d404_wb_advert_report::{
    WbAdvertReportLink, WbAdvertReportNode, WbAdvertReportRequest, WbAdvertReportResponse,
    WbAdvertReportTotals,
};
use contracts::dashboards::d406_wb_sales_funnel::{
    FunnelOrderChannel, WbSalesFunnelConversions, WbSalesFunnelMetrics, WbSalesFunnelOrderItem,
    WbSalesFunnelOrdersResponse, WbSalesFunnelRequest, WbSalesFunnelResponse, WbSalesFunnelRow,
};
use contracts::projections::p916_mp_sales_funnel_turnovers::dto::MpFunnelListRequest;
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet, HashMap};

#[derive(Deserialize)]
pub struct WbOrderFlowQuery {
    pub srid: String,
}

#[derive(Deserialize)]
pub struct YmOrderFlowQuery {
    pub order_id: String,
}

#[derive(Default)]
struct AdvertReportAccum {
    accrued: f64,
    expensed: f64,
    expense_no_order: f64,
    links: BTreeMap<String, WbAdvertReportLink>,
    children: BTreeMap<String, AdvertReportAccum>,
}

impl AdvertReportAccum {
    fn add_order(&mut self, accrued: f64, expensed: f64, links: &[WbAdvertReportLink]) {
        self.accrued += accrued;
        self.expensed += expensed;
        self.add_links(links);
    }

    fn add_no_order(&mut self, amount: f64, links: &[WbAdvertReportLink]) {
        self.expense_no_order += amount;
        self.add_links(links);
    }

    fn add_links(&mut self, links: &[WbAdvertReportLink]) {
        for link in links {
            self.links
                .entry(link.tab_key.clone())
                .or_insert_with(|| link.clone());
        }
    }
}

struct P913AdvertReportRow {
    campaign_code: String,
    nomenclature_ref: Option<String>,
    order_key: String,
    accrued: f64,
    expensed: f64,
    registrators: String,
}

struct P911AdvertReportRow {
    campaign_code: String,
    nomenclature_ref: Option<String>,
    expense_no_order: f64,
}

struct OrderInfo {
    id: String,
    date: String,
}

fn sql_lit(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

fn push_common_filters(
    conditions: &mut Vec<String>,
    filters: &WbAdvertReportRequest,
    campaign_column: &str,
) {
    if let Some(value) = filters
        .date_from
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        conditions.push(format!("entry_date >= {}", sql_lit(value)));
    }
    if let Some(value) = filters
        .date_to
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        conditions.push(format!("entry_date <= {}", sql_lit(value)));
    }
    if let Some(value) = filters
        .connection_mp_ref
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        conditions.push(format!("connection_mp_ref = {}", sql_lit(value)));
    }
    if let Some(value) = filters
        .wb_advert_campaign_code
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        conditions.push(format!("{campaign_column} = {}", sql_lit(value)));
    }
}

async fn fetch_p913_advert_report_rows(
    filters: &WbAdvertReportRequest,
) -> anyhow::Result<Vec<P913AdvertReportRow>> {
    use sea_orm::{ConnectionTrait, Statement};

    let mut conditions = Vec::new();
    push_common_filters(&mut conditions, filters, "wb_advert_campaign_code");
    let where_clause = if conditions.is_empty() {
        String::new()
    } else {
        format!(" WHERE {}", conditions.join(" AND "))
    };
    let sql = format!(
        "SELECT \
            COALESCE(wb_advert_campaign_code, '') AS campaign_code, \
            nomenclature_ref, \
            COALESCE(order_key, '') AS order_key, \
            COALESCE(SUM(CASE WHEN turnover_code = 'advert_clicks_order_accrual' THEN amount ELSE 0 END), 0) AS accrued, \
            COALESCE(SUM(CASE WHEN turnover_code = 'advert_clicks_order_expense' THEN amount ELSE 0 END), 0) AS expensed, \
            COALESCE(GROUP_CONCAT(DISTINCT registrator_type || '|' || registrator_ref), '') AS registrators \
         FROM p913_wb_advert_order_attr{where_clause} \
         GROUP BY COALESCE(wb_advert_campaign_code, ''), nomenclature_ref, COALESCE(order_key, '')"
    );

    let db = crate::shared::data::db::get_connection();
    let rows = db
        .query_all(Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            sql,
        ))
        .await?;
    Ok(rows
        .into_iter()
        .map(|row| P913AdvertReportRow {
            campaign_code: row.try_get("", "campaign_code").unwrap_or_default(),
            nomenclature_ref: row.try_get("", "nomenclature_ref").ok(),
            order_key: row.try_get("", "order_key").unwrap_or_default(),
            accrued: row.try_get("", "accrued").unwrap_or(0.0),
            expensed: row.try_get("", "expensed").unwrap_or(0.0),
            registrators: row.try_get("", "registrators").unwrap_or_default(),
        })
        .collect())
}

async fn fetch_p911_advert_report_rows(
    filters: &WbAdvertReportRequest,
) -> anyhow::Result<Vec<P911AdvertReportRow>> {
    use sea_orm::{ConnectionTrait, Statement};

    let mut conditions = vec!["turnover_code = 'advert_clicks_no_order'".to_string()];
    push_common_filters(&mut conditions, filters, "wb_advert_campaign_code");
    let sql = format!(
        "SELECT \
            COALESCE(wb_advert_campaign_code, '') AS campaign_code, \
            nomenclature_ref, \
            COALESCE(SUM(amount), 0) AS expense_no_order \
         FROM p911_wb_advert_by_items \
         WHERE {} \
         GROUP BY COALESCE(wb_advert_campaign_code, ''), nomenclature_ref",
        conditions.join(" AND ")
    );

    let db = crate::shared::data::db::get_connection();
    let rows = db
        .query_all(Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            sql,
        ))
        .await?;
    Ok(rows
        .into_iter()
        .map(|row| P911AdvertReportRow {
            campaign_code: row.try_get("", "campaign_code").unwrap_or_default(),
            nomenclature_ref: row.try_get("", "nomenclature_ref").ok(),
            expense_no_order: row.try_get("", "expense_no_order").unwrap_or(0.0),
        })
        .collect())
}

fn link_for_registrator(
    registrator_type: &str,
    registrator_ref: &str,
) -> Option<WbAdvertReportLink> {
    let clean_ref = registrator_ref
        .strip_prefix("a026:")
        .unwrap_or(registrator_ref);
    match registrator_type {
        "a026_wb_advert_daily" => Some(WbAdvertReportLink {
            label: "Реклама".to_string(),
            tab_key: format!("a026_wb_advert_daily_details_{clean_ref}"),
        }),
        "a012_wb_sales" => Some(WbAdvertReportLink {
            label: "Продажа".to_string(),
            tab_key: format!("a012_wb_sales_details_{clean_ref}"),
        }),
        "a015_wb_orders" => Some(WbAdvertReportLink {
            label: "Заказ".to_string(),
            tab_key: format!("a015_wb_orders_details_{clean_ref}"),
        }),
        _ => None,
    }
}

fn parse_registrator_links(value: &str) -> Vec<WbAdvertReportLink> {
    let mut links = BTreeMap::new();
    for item in value
        .split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
    {
        let Some((registrator_type, registrator_ref)) = item.split_once('|') else {
            continue;
        };
        if registrator_ref.trim().is_empty() {
            continue;
        }
        if let Some(link) = link_for_registrator(registrator_type, registrator_ref) {
            links.entry(link.tab_key.clone()).or_insert(link);
        }
    }
    links.into_values().collect()
}

async fn fetch_order_info(order_keys: &[String]) -> anyhow::Result<HashMap<String, OrderInfo>> {
    use sea_orm::{ConnectionTrait, Statement};

    if order_keys.is_empty() {
        return Ok(HashMap::new());
    }
    let keys = order_keys
        .iter()
        .map(|key| sql_lit(key))
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!(
        "SELECT id, document_no, document_date, line_json \
         FROM a015_wb_orders \
         WHERE is_deleted = 0 AND document_no IN ({keys})"
    );
    let db = crate::shared::data::db::get_connection();
    let rows = db
        .query_all(Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            sql,
        ))
        .await?;
    let mut result = HashMap::with_capacity(rows.len());
    for row in rows {
        let document_no: String = row.try_get("", "document_no").unwrap_or_default();
        result.insert(
            document_no,
            OrderInfo {
                id: row.try_get("", "id").unwrap_or_default(),
                date: row
                    .try_get::<Option<String>>("", "document_date")
                    .ok()
                    .flatten()
                    .unwrap_or_default(),
            },
        );
    }
    Ok(result)
}

fn empty_key(value: &str) -> String {
    if value.trim().is_empty() {
        "__empty__".to_string()
    } else {
        value.to_string()
    }
}

fn opt_key(value: &Option<String>) -> String {
    value
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .unwrap_or_else(|| "__none__".to_string())
}

fn campaign_label(code: &str, names: &HashMap<String, String>) -> String {
    if code.is_empty() || code == "__empty__" {
        return "Кампания не определена".to_string();
    }
    match names.get(code) {
        Some(name) if !name.trim().is_empty() => format!("{name} ({code})"),
        _ => format!("Кампания {code}"),
    }
}

fn nomenclature_label(key: &str, names: &HashMap<String, String>) -> String {
    if key == "__none__" {
        return "Номенклатура не определена".to_string();
    }
    names
        .get(key)
        .filter(|name| !name.trim().is_empty())
        .cloned()
        .unwrap_or_else(|| key.to_string())
}

fn order_label(key: &str) -> String {
    if key == "__no_order__" {
        "Нет аналитики по заказу".to_string()
    } else if key.is_empty() || key == "__empty__" {
        "Заказ не определен".to_string()
    } else {
        key.to_string()
    }
}

fn format_date_dmy(value: &str) -> String {
    let trimmed = value.trim();
    let date_part = trimmed.get(..10).unwrap_or(trimmed);
    chrono::NaiveDate::parse_from_str(date_part, "%Y-%m-%d")
        .map(|date| date.format("%d.%m.%Y").to_string())
        .unwrap_or_else(|_| trimmed.to_string())
}

fn to_report_node(
    level: &str,
    key: &str,
    accum: &AdvertReportAccum,
    campaign_code: Option<&str>,
    nomenclature_ref: Option<&str>,
    campaign_names: &HashMap<String, String>,
    nomenclature_names: &HashMap<String, String>,
    order_infos: &HashMap<String, OrderInfo>,
) -> WbAdvertReportNode {
    let mut label = match level {
        "campaign" => campaign_label(key, campaign_names),
        "nomenclature" => nomenclature_label(key, nomenclature_names),
        "order" => order_label(key),
        _ => key.to_string(),
    };
    let is_real_order = level == "order" && key != "__no_order__" && key != "__empty__";
    let mut links = if is_real_order {
        accum.links.values().cloned().collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    if is_real_order {
        if let Some(info) = order_infos.get(key) {
            label = if info.date.trim().is_empty() {
                format!("Заказ № {key}")
            } else {
                format!("Заказ от {} № {key}", format_date_dmy(&info.date))
            };
            if !info.id.trim().is_empty() {
                let link = WbAdvertReportLink {
                    label: "Заказ".to_string(),
                    tab_key: format!("a015_wb_orders_details_{}", info.id),
                };
                if !links
                    .iter()
                    .any(|existing| existing.tab_key == link.tab_key)
                {
                    links.insert(0, link);
                }
            }
        }
    }
    let children = accum
        .children
        .iter()
        .map(|(child_key, child)| match level {
            "campaign" => to_report_node(
                "nomenclature",
                child_key,
                child,
                Some(key),
                if child_key == "__none__" {
                    None
                } else {
                    Some(child_key)
                },
                campaign_names,
                nomenclature_names,
                order_infos,
            ),
            "nomenclature" => to_report_node(
                "order",
                child_key,
                child,
                campaign_code,
                nomenclature_ref,
                campaign_names,
                nomenclature_names,
                order_infos,
            ),
            _ => unreachable!("advert report tree has only three levels"),
        })
        .collect::<Vec<_>>();
    let wb_advert_campaign_code = campaign_code
        .or_else(|| (level == "campaign").then_some(key))
        .filter(|value| !value.is_empty() && *value != "__empty__")
        .map(ToString::to_string);
    let nomenclature_ref = nomenclature_ref
        .or_else(|| (level == "nomenclature" && key != "__none__").then_some(key))
        .map(ToString::to_string);
    let order_key = (level == "order" && key != "__no_order__")
        .then(|| key.to_string())
        .filter(|value| !value.is_empty() && value != "__empty__");

    WbAdvertReportNode {
        level: level.to_string(),
        id: match level {
            "campaign" => format!("campaign:{key}"),
            "nomenclature" => format!(
                "campaign:{}:nomenclature:{key}",
                campaign_code.unwrap_or("")
            ),
            "order" => format!(
                "campaign:{}:nomenclature:{}:order:{key}",
                campaign_code.unwrap_or(""),
                nomenclature_ref.as_deref().unwrap_or("")
            ),
            _ => key.to_string(),
        },
        label,
        wb_advert_campaign_code,
        nomenclature_ref,
        order_key,
        accrued: accum.accrued,
        expensed: accum.expensed,
        balance: accum.accrued - accum.expensed,
        expense_no_order: accum.expense_no_order,
        links,
        children,
    }
}

/// GET /api/dashboards/wb-advert-report
pub async fn wb_advert_report(
    Query(filters): Query<WbAdvertReportRequest>,
) -> Result<Json<WbAdvertReportResponse>, ApiError> {
    let p913_rows = fetch_p913_advert_report_rows(&filters)
        .await
        .map_err(|error| {
            tracing::error!("wb_advert_report p913 aggregation failed: {}", error);
            ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        })?;
    let p911_rows = fetch_p911_advert_report_rows(&filters)
        .await
        .map_err(|error| {
            tracing::error!("wb_advert_report p911 aggregation failed: {}", error);
            ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        })?;

    let mut root = BTreeMap::<String, AdvertReportAccum>::new();
    let mut nomenclature_refs = BTreeSet::<String>::new();
    let mut order_keys = BTreeSet::<String>::new();

    for row in p913_rows {
        let links = parse_registrator_links(&row.registrators);
        let campaign_key = empty_key(&row.campaign_code);
        let nomenclature_key = opt_key(&row.nomenclature_ref);
        if nomenclature_key != "__none__" {
            nomenclature_refs.insert(nomenclature_key.clone());
        }
        let order_key = empty_key(&row.order_key);
        if order_key != "__empty__" {
            order_keys.insert(order_key.clone());
        }
        let campaign = root.entry(campaign_key).or_default();
        campaign.add_order(row.accrued, row.expensed, &[]);
        let nomenclature = campaign.children.entry(nomenclature_key).or_default();
        nomenclature.add_order(row.accrued, row.expensed, &[]);
        let order = nomenclature.children.entry(order_key).or_default();
        order.add_order(row.accrued, row.expensed, &links);
    }

    for row in p911_rows {
        let campaign_key = empty_key(&row.campaign_code);
        let nomenclature_key = opt_key(&row.nomenclature_ref);
        if nomenclature_key != "__none__" {
            nomenclature_refs.insert(nomenclature_key.clone());
        }
        let campaign = root.entry(campaign_key).or_default();
        campaign.add_no_order(row.expense_no_order, &[]);
        let nomenclature = campaign.children.entry(nomenclature_key).or_default();
        nomenclature.add_no_order(row.expense_no_order, &[]);
        let no_order = nomenclature
            .children
            .entry("__no_order__".to_string())
            .or_default();
        no_order.add_no_order(row.expense_no_order, &[]);
    }

    let campaign_names = crate::domain::a030_wb_advert_campaign::service::list_all()
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|campaign| {
            (
                campaign.header.advert_id.to_string(),
                campaign.base.description.clone(),
            )
        })
        .collect::<HashMap<_, _>>();

    let nomenclature_names = crate::domain::a004_nomenclature::repository::list_by_ids(
        &nomenclature_refs.into_iter().collect::<Vec<_>>(),
    )
    .await
    .unwrap_or_default()
    .into_iter()
    .map(|item| {
        let id = item.base.id.0.to_string();
        let label = if item.article.trim().is_empty() {
            item.base.description
        } else {
            format!("{} — {}", item.article, item.base.description)
        };
        (id, label)
    })
    .collect::<HashMap<_, _>>();
    let order_infos = fetch_order_info(&order_keys.into_iter().collect::<Vec<_>>())
        .await
        .unwrap_or_default();

    let mut totals = WbAdvertReportTotals::default();
    let campaigns = root
        .iter()
        .map(|(key, accum)| {
            totals.accrued += accum.accrued;
            totals.expensed += accum.expensed;
            totals.expense_no_order += accum.expense_no_order;
            to_report_node(
                "campaign",
                key,
                accum,
                None,
                None,
                &campaign_names,
                &nomenclature_names,
                &order_infos,
            )
        })
        .collect::<Vec<_>>();
    totals.balance = totals.accrued - totals.expensed;

    Ok(Json(WbAdvertReportResponse {
        filters,
        totals,
        campaigns,
    }))
}

/// GET /api/dashboards/wb-order-flow?srid={srid}
pub async fn wb_order_flow(
    Query(query): Query<WbOrderFlowQuery>,
) -> Result<Json<WbOrderFlowResponse>, ApiError> {
    let srid = query.srid.trim().to_string();

    // 1. a015: заказ
    let order_opt = crate::domain::a015_wb_orders::service::get_by_document_no(&srid)
        .await
        .map_err(|e| {
            tracing::error!("wb_order_flow a015 {}: {}", srid, e);
            ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        })?;

    let order_item = order_opt.as_ref().map(|o| OrderFlowItem {
        id: o.base.id.value().to_string(),
        document_no: o.header.document_no.clone(),
        document_date: Some(o.state.order_dt.format("%d.%m.%Y").to_string()),
        supplier_article: Some(o.line.supplier_article.clone()),
        brand: o.line.brand.clone(),
        subject: o.line.subject.clone(),
        nm_id: Some(o.line.nm_id),
        qty: Some(o.line.qty),
        finished_price: o.line.finished_price,
        total_price: o.line.total_price,
        price_with_disc: o.line.price_with_disc,
        spp: o.line.spp,
        dealer_price_ut: o.line.dealer_price_ut,
        income_id: o.source_meta.income_id,
        is_cancel: o.state.is_cancel,
        is_supply: o.state.is_supply.unwrap_or(false),
        is_realization: o.state.is_realization.unwrap_or(false),
        is_posted: o.is_posted,
        warehouse_name: o.warehouse.warehouse_name.clone(),
        g_number: o.source_meta.g_number.clone(),
    });

    // Описания номенклатуры и базовой номенклатуры (для заголовка дашборда)
    let (nomenclature_description, base_nomenclature_description) =
        resolve_nomenclature_descriptions(order_opt.as_ref()).await;

    // 2. a029: поставка
    let supply_item = if let Some(ref order) = order_opt {
        crate::domain::a029_wb_supply::service::get_for_order(order)
            .await
            .ok()
            .flatten()
            .map(|s| SupplyFlowItem {
                id: s.base.id.value().to_string(),
                supply_id: s.header.supply_id.clone(),
                supply_name: s.info.name.clone(),
                created_at_wb: s
                    .info
                    .created_at_wb
                    .map(|dt| dt.format("%d.%m.%Y").to_string()),
                closed_at_wb: s
                    .info
                    .closed_at_wb
                    .map(|dt| dt.format("%d.%m.%Y").to_string()),
                is_done: s.info.is_done,
            })
    } else {
        None
    };

    // 3. a012: продажи
    let sales_raw = crate::domain::a012_wb_sales::repository::search_by_document_no(&srid)
        .await
        .unwrap_or_default();

    let sales: Vec<SaleFlowItem> = sales_raw
        .into_iter()
        .map(|s| SaleFlowItem {
            id: s.base.id.value().to_string(),
            document_no: s.header.document_no.clone(),
            event_type: s.state.event_type.clone(),
            status_norm: s.state.status_norm.clone(),
            sale_dt: s.state.sale_dt.format("%d.%m.%Y").to_string(),
            is_posted: s.is_posted,
            is_customer_return: s.is_customer_return,
            warehouse_name: s.warehouse.warehouse_name.clone(),
            name: s.line.name.clone(),
            supplier_article: s.line.supplier_article.clone(),
            finished_price: s.line.finished_price,
            amount_line: s.line.amount_line,
            sell_out_plan: s.line.sell_out_plan,
            commission_plan: s.line.commission_plan,
            acquiring_fee_plan: s.line.acquiring_fee_plan,
            other_fee_plan: s.line.other_fee_plan,
            supplier_payout_plan: s.line.supplier_payout_plan,
            cost_of_production: s.line.cost_of_production,
            dealer_price_ut: s.line.dealer_price_ut,
            profit_plan: s.line.profit_plan,
            sell_out_fact: s.line.sell_out_fact,
            supplier_payout_fact: s.line.supplier_payout_fact,
            profit_fact: s.line.profit_fact,
            is_fact: s.line.is_fact.unwrap_or(false),
        })
        .collect();

    // 4. p913 → a026: рекламная атрибуция
    let p913_rows =
        crate::projections::p913_wb_advert_order_attr::repository::list_by_order_key_and_turnover(
            &srid,
            "advert_clicks_order_accrual",
        )
        .await
        .unwrap_or_default();

    let mut advert_map: HashMap<String, (f64, String)> = HashMap::new();
    for row in &p913_rows {
        if row.registrator_type == "a026_wb_advert_daily" {
            let e = advert_map
                .entry(row.registrator_ref.clone())
                .or_insert((0.0, row.entry_date.clone()));
            e.0 += row.amount;
        }
    }

    let mut advert_campaigns: Vec<AdvertFlowItem> = Vec::new();
    for (ref_id, (allocated_cost, entry_date)) in &advert_map {
        let item = if let Ok(uuid) = uuid::Uuid::parse_str(ref_id) {
            match crate::domain::a026_wb_advert_daily::service::get_by_id(uuid).await {
                Ok(Some(doc)) => {
                    let advert_id = doc.header.advert_id;
                    let campaign =
                        crate::domain::a030_wb_advert_campaign::repository::get_by_advert_id(
                            advert_id,
                        )
                        .await
                        .ok()
                        .flatten();
                    AdvertFlowItem {
                        advert_id,
                        registrator_ref: ref_id.clone(),
                        document_date: doc.header.document_date.clone(),
                        allocated_cost: *allocated_cost,
                        campaign_name: campaign.as_ref().map(|c| c.base.description.clone()),
                        campaign_status: campaign.and_then(|c| c.header.status),
                        views: doc.totals.views,
                        clicks: doc.totals.clicks,
                        orders_reported: doc.totals.orders as i64,
                        total_spend: doc.totals.sum,
                        ctr: doc.totals.ctr,
                        cpc: doc.totals.cpc,
                    }
                }
                _ => AdvertFlowItem {
                    advert_id: 0,
                    registrator_ref: ref_id.clone(),
                    document_date: entry_date.clone(),
                    allocated_cost: *allocated_cost,
                    campaign_name: None,
                    campaign_status: None,
                    views: 0,
                    clicks: 0,
                    orders_reported: 0,
                    total_spend: 0.0,
                    ctr: 0.0,
                    cpc: 0.0,
                },
            }
        } else {
            AdvertFlowItem {
                advert_id: 0,
                registrator_ref: ref_id.clone(),
                document_date: entry_date.clone(),
                allocated_cost: *allocated_cost,
                campaign_name: None,
                campaign_status: None,
                views: 0,
                clicks: 0,
                orders_reported: 0,
                total_spend: 0.0,
                ctr: 0.0,
                cpc: 0.0,
            }
        };
        advert_campaigns.push(item);
    }
    advert_campaigns.sort_by(|a, b| a.document_date.cmp(&b.document_date));

    let total_advert_cost = advert_campaigns.iter().map(|a| a.allocated_cost).sum();

    // 5. p903: строки финансового отчёта WB
    let p903_raw = crate::projections::p903_wb_finance_report::repository::search_by_srid(&srid)
        .await
        .unwrap_or_default();

    let mut p903_rows: Vec<P903FlowItem> = p903_raw
        .into_iter()
        .map(|r| P903FlowItem {
            id: r.id,
            rr_dt: r.rr_dt,
            supplier_oper_name: r.supplier_oper_name,
            retail_price_withdisc_rub: r.retail_price_withdisc_rub,
            ppvz_for_pay: r.ppvz_for_pay,
            ppvz_sales_commission: r.ppvz_sales_commission,
            acquiring_fee: r.acquiring_fee,
            commission_percent: r.commission_percent,
            delivery_rub: r.delivery_rub,
            penalty: r.penalty,
            storage_fee: r.storage_fee,
            rebill_logistic_cost: r.rebill_logistic_cost,
            additional_payment: r.additional_payment,
            return_amount: r.return_amount,
            quantity: r.quantity,
        })
        .collect();
    p903_rows.sort_by(|a, b| a.rr_dt.cmp(&b.rr_dt));

    // 6. a032: заявки покупателя на возврат
    let claims_raw = crate::domain::a032_wb_returns_claims::repository::search_by_srid(&srid)
        .await
        .unwrap_or_default();

    let mut claims: Vec<ClaimFlowItem> = claims_raw
        .into_iter()
        .map(|c| ClaimFlowItem {
            id: c.base.id.value().to_string(),
            claim_id: c.claim_id.clone(),
            status: c.status,
            dt: c.dt.format("%d.%m.%Y").to_string(),
            dt_update: c.dt_update.map(|d| d.format("%d.%m.%Y").to_string()),
            price: c.price,
            user_comment: c.user_comment.clone(),
            is_archive: c.is_archive,
        })
        .collect();
    claims.sort_by(|a, b| a.dt.cmp(&b.dt));

    Ok(Json(WbOrderFlowResponse {
        srid,
        order: order_item,
        supply: supply_item,
        sales,
        advert_campaigns,
        total_advert_cost,
        p903_rows,
        claims,
        base_nomenclature_description,
        nomenclature_description,
    }))
}

/// GET /api/dashboards/ym-order-flow?order_id={order_no}
///
/// «Вся история» YM-заказа: строки реализации a034 + платёжные транзакции p907,
/// собранные по номеру заказа.
pub async fn ym_order_flow(
    Query(query): Query<YmOrderFlowQuery>,
) -> Result<Json<YmOrderFlowResponse>, ApiError> {
    let order_no = query.order_id.trim().to_string();

    // 0. a013: сам заказ (первое событие ленты, как в d402).
    let order = crate::domain::a013_ym_order::repository::get_by_document_no(&order_no)
        .await
        .unwrap_or(None)
        .map(|o| {
            let qty: f64 = o.lines.iter().map(|l| l.qty).sum();
            YmOrderFlowItem {
                id: o.base.id.0.to_string(),
                document_no: o.header.document_no.clone(),
                order_date: o
                    .state
                    .creation_date
                    .map(|d| d.format("%d.%m.%Y").to_string()),
                status: Some(o.state.status_norm.clone()).filter(|s| !s.trim().is_empty()),
                delivery_date: o
                    .state
                    .delivery_date
                    .map(|d| d.format("%d.%m.%Y").to_string()),
                qty,
                items_total: o.header.items_total,
                total_amount: o.header.total_amount,
                is_posted: o.is_posted,
            }
        });

    // 1. a034: строки официальной реализации по заказу.
    let realizations: Vec<YmRealizationFlowItem> =
        crate::domain::a034_ym_realization::repository::lines_by_order_id(&order_no)
            .await
            .unwrap_or_default()
            .into_iter()
            .map(|l| YmRealizationFlowItem {
                doc_id: l.doc_id,
                document_no: l.document_no,
                document_date: l.document_date,
                shop_sku: l.shop_sku,
                offer_name: l.offer_name,
                quantity: l.quantity,
                revenue_amount: l.revenue_amount,
                is_return: l.is_return,
            })
            .collect();

    // 2. p907: платёжные транзакции по заказу (order_id хранится как i64).
    let payments: Vec<YmPaymentFlowItem> = match order_no.parse::<i64>() {
        Ok(oid) => {
            let (rows, _) =
                crate::projections::p907_ym_payment_report::repository::list_with_filters(
                    "",
                    "",
                    None,
                    None,
                    None,
                    None,
                    Some(oid),
                    None,
                    None,
                    None,
                    "transaction_date",
                    false,
                    1000,
                    0,
                )
                .await
                .unwrap_or_default();
            rows.into_iter()
                .map(|r| YmPaymentFlowItem {
                    id: r.id,
                    transaction_date: r.transaction_date,
                    transaction_type: r.transaction_type,
                    transaction_id: r.transaction_id,
                    transaction_sum: r.transaction_sum,
                    bank_sum: r.bank_sum,
                    payment_status: r.payment_status,
                    transaction_source: r.transaction_source,
                    shop_sku: r.shop_sku,
                    offer_or_service_name: r.offer_or_service_name,
                    count: r.count,
                    comments: r.comments,
                })
                .collect()
        }
        Err(_) => Vec::new(),
    };

    // 3. a016: возвраты по этому заказу (если есть).
    let returns: Vec<YmReturnFlowItem> = match order_no.parse::<i64>() {
        Ok(oid) => crate::domain::a016_ym_returns::repository::list_by_order_ids(&[oid])
            .await
            .unwrap_or_default()
            .into_iter()
            .map(|r| {
                let qty: i32 = r.lines.iter().map(|l| l.count).sum();
                YmReturnFlowItem {
                    id: r.base.id.0.to_string(),
                    return_id: r.header.return_id,
                    return_type: r.header.return_type.clone(),
                    refund_status: r.state.refund_status.clone(),
                    created_at_source: r
                        .state
                        .created_at_source
                        .map(|d| d.format("%d.%m.%Y").to_string()),
                    amount: r.header.amount.unwrap_or(0.0),
                    qty,
                }
            })
            .collect(),
        Err(_) => Vec::new(),
    };

    Ok(Json(YmOrderFlowResponse {
        order_no,
        order,
        realizations,
        payments,
        returns,
    }))
}

async fn resolve_nomenclature_descriptions(
    order: Option<&contracts::domain::a015_wb_orders::aggregate::WbOrders>,
) -> (Option<String>, Option<String>) {
    const ZERO_UUID: &str = "00000000-0000-0000-0000-000000000000";
    let Some(order) = order else {
        return (None, None);
    };

    let nom_desc = if let Some(nom_ref) = order.nomenclature_ref.as_deref() {
        if !nom_ref.is_empty() && nom_ref != ZERO_UUID {
            if let Ok(uuid) = uuid::Uuid::parse_str(nom_ref) {
                crate::domain::a004_nomenclature::service::get_by_id(uuid)
                    .await
                    .ok()
                    .flatten()
                    .map(|n| n.base.description.clone())
            } else {
                None
            }
        } else {
            None
        }
    } else {
        None
    };

    let base_desc = if let Some(base_ref) = order.base_nomenclature_ref.as_deref() {
        if !base_ref.is_empty() && base_ref != ZERO_UUID {
            if let Ok(uuid) = uuid::Uuid::parse_str(base_ref) {
                crate::domain::a004_nomenclature::service::get_by_id(uuid)
                    .await
                    .ok()
                    .flatten()
                    .map(|n| n.base.description.clone())
            } else {
                None
            }
        } else {
            None
        }
    } else {
        None
    };

    (nom_desc, base_desc)
}

/// GET /api/dashboards/wb-sales-funnel
/// Воронка продаж WB (d406) — агрегат p916 `товар × дата` по выбранной оси
/// (когорта/событие) с именами товаров (джойн a004) и производными конверсиями.
pub async fn wb_sales_funnel(
    Query(filters): Query<WbSalesFunnelRequest>,
) -> Result<Json<WbSalesFunnelResponse>, ApiError> {
    let request = MpFunnelListRequest {
        date_from: filters.date_from.clone(),
        date_to: filters.date_to.clone(),
        connection_mp_ref: filters.connection_mp_ref.clone(),
        nm_id: filters.nm_id,
        axis: filters.axis,
        offset: None,
        limit: Some(50_000),
    };

    let agg_rows =
        crate::projections::p916_mp_sales_funnel_turnovers::repository::aggregate_by_product(
            &request,
        )
        .await
        .map_err(|error| {
            tracing::error!("wb_sales_funnel p916 aggregation failed: {}", error);
            ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        })?;

    // Имена товаров: джойн a004 по nomenclature_ref (одним запросом на весь набор).
    // Артикул и наименование храним раздельно — d406 показывает только артикул (наим. в тултипе).
    let nomenclature_refs: Vec<String> = agg_rows
        .iter()
        .filter_map(|row| row.nomenclature_ref.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let nomenclature_names: HashMap<String, (Option<String>, Option<String>)> =
        crate::domain::a004_nomenclature::repository::list_by_ids(&nomenclature_refs)
            .await
            .unwrap_or_default()
            .into_iter()
            .map(|item| {
                let id = item.base.id.0.to_string();
                let article = Some(item.article.trim().to_string()).filter(|s| !s.is_empty());
                let name = Some(item.base.description.trim().to_string()).filter(|s| !s.is_empty());
                (id, (article, name))
            })
            .collect();

    // Маркетплейс по connection_mp_ref: a006 (подключение) → a005 (справочник) → тип МП.
    let marketplace_by_connection = funnel_marketplace_labels().await;

    let mut totals = WbSalesFunnelMetrics::default();
    let rows: Vec<WbSalesFunnelRow> = agg_rows
        .into_iter()
        .map(|row| {
            let show_total = row.show_total_count();
            let metrics = WbSalesFunnelMetrics {
                show_free_count: row.show_free_count,
                show_paid_count: row.show_paid_count,
                show_total_count: show_total,
                show_free_available: row.show_free_available,
                show_paid_available: row.show_paid_available,
                show_total_available: row.show_free_available || row.show_paid_available,
                total_impressions: row.total_impressions,
                total_impressions_available: row.total_impressions_available,
                advert_available: row.advert_available,
                open_count: row.open_count,
                cart_count: row.cart_count,
                paid_open_count: row.paid_open_count,
                paid_cart_count: row.paid_cart_count,
                wishlist_count: row.wishlist_count,
                funnel_order_count: row.funnel_order_count,
                funnel_order_sum: row.funnel_order_sum,
                funnel_cancel_count: row.funnel_cancel_count,
                funnel_cancel_sum: row.funnel_cancel_sum,
                funnel_cancel_available: row.funnel_cancel_available,
                order_count: row.order_count,
                order_sum: row.order_sum,
                paid_order_count: row.paid_order_count,
                paid_order_sum: row.paid_order_sum,
                cancel_count: row.cancel_count,
                cancel_sum: row.cancel_sum,
                paid_cancel_count: row.paid_cancel_count,
                paid_cancel_sum: row.paid_cancel_sum,
                buyout_count: row.buyout_count,
                buyout_sum: row.buyout_sum,
                paid_buyout_count: row.paid_buyout_count,
                paid_buyout_sum: row.paid_buyout_sum,
                return_count: row.return_count,
                return_sum: row.return_sum,
                paid_return_count: row.paid_return_count,
                paid_return_sum: row.paid_return_sum,
            };
            accumulate_funnel_totals(&mut totals, &metrics);

            let (article, product_name) = row
                .nomenclature_ref
                .as_ref()
                .and_then(|r| nomenclature_names.get(r).cloned())
                .unwrap_or((None, None));
            let (marketplace, marketplace_code) = marketplace_by_connection
                .get(&row.connection_mp_ref)
                .cloned()
                .unwrap_or((None, None));
            // Конверсии «всего» (для строки total-канала); канальные конверсии считает клиент.
            let conversions = WbSalesFunnelConversions::from_metrics_with_funnel_cancel(
                metrics.open_count,
                metrics.cart_count,
                metrics.order_count,
                metrics.buyout_count,
                metrics.cancel_count,
                metrics.funnel_cancel_count,
                metrics.funnel_order_count,
            );

            WbSalesFunnelRow {
                date: row.date,
                connection_mp_ref: row.connection_mp_ref,
                nm_id: row.nm_id,
                marketplace_product_ref: row.marketplace_product_ref,
                nomenclature_ref: row.nomenclature_ref,
                marketplace,
                marketplace_code,
                article,
                product_name,
                brand: None,
                metrics,
                conversions,
            }
        })
        .collect();

    let totals_conversions = WbSalesFunnelConversions::from_metrics_with_funnel_cancel(
        totals.open_count,
        totals.cart_count,
        totals.order_count,
        totals.buyout_count,
        totals.cancel_count,
        totals.funnel_cancel_count,
        totals.funnel_order_count,
    );

    Ok(Json(WbSalesFunnelResponse {
        filters,
        rows,
        totals,
        totals_conversions,
    }))
}

/// Прибавить метрики строки к аккумулятору итогов периода (аддитивные поля).
fn accumulate_funnel_totals(totals: &mut WbSalesFunnelMetrics, row: &WbSalesFunnelMetrics) {
    totals.show_free_count += row.show_free_count;
    totals.show_paid_count += row.show_paid_count;
    totals.show_total_count += row.show_total_count;
    totals.show_free_available |= row.show_free_available;
    totals.show_paid_available |= row.show_paid_available;
    totals.show_total_available |= row.show_total_available;
    totals.total_impressions += row.total_impressions;
    totals.total_impressions_available |= row.total_impressions_available;
    totals.advert_available |= row.advert_available;
    totals.open_count += row.open_count;
    totals.cart_count += row.cart_count;
    totals.paid_open_count += row.paid_open_count;
    totals.paid_cart_count += row.paid_cart_count;
    totals.wishlist_count += row.wishlist_count;
    totals.funnel_order_count += row.funnel_order_count;
    totals.funnel_order_sum += row.funnel_order_sum;
    totals.funnel_cancel_count += row.funnel_cancel_count;
    totals.funnel_cancel_sum += row.funnel_cancel_sum;
    totals.funnel_cancel_available |= row.funnel_cancel_available;
    totals.order_count += row.order_count;
    totals.order_sum += row.order_sum;
    totals.paid_order_count += row.paid_order_count;
    totals.paid_order_sum += row.paid_order_sum;
    totals.cancel_count += row.cancel_count;
    totals.cancel_sum += row.cancel_sum;
    totals.paid_cancel_count += row.paid_cancel_count;
    totals.paid_cancel_sum += row.paid_cancel_sum;
    totals.buyout_count += row.buyout_count;
    totals.buyout_sum += row.buyout_sum;
    totals.paid_buyout_count += row.paid_buyout_count;
    totals.paid_buyout_sum += row.paid_buyout_sum;
    totals.return_count += row.return_count;
    totals.return_sum += row.return_sum;
    totals.paid_return_count += row.paid_return_count;
    totals.paid_return_sum += row.paid_return_sum;
}

#[derive(Deserialize)]
pub struct WbSalesFunnelOrdersQuery {
    pub connection_mp_ref: String,
    pub nm_id: i64,
    /// Дата заказа (когорта), YYYY-MM-DD.
    pub date: String,
    /// Фильтр канала: `paid` | `free` | (пусто → все).
    #[serde(default)]
    pub channel: Option<String>,
}

/// Drilldown воронки: конкретные заказы одной ячейки `nm_id × дата` с меткой канала.
/// «Платный» ⇔ srid заказа входит в атрибуцию рекламы p913 (`advert_clicks_order_accrual`).
/// Счётчики `paid_count`/`free_count` считаются по всей ячейке (до фильтра `channel`).
pub async fn wb_sales_funnel_orders(
    Query(query): Query<WbSalesFunnelOrdersQuery>,
) -> Result<Json<WbSalesFunnelOrdersResponse>, ApiError> {
    let channel = match query.channel.as_deref() {
        Some("paid") => FunnelOrderChannel::Paid,
        Some("free") => FunnelOrderChannel::Free,
        _ => FunnelOrderChannel::All,
    };

    let orders = crate::domain::a015_wb_orders::repository::list_for_advert_attribution(
        query.nm_id,
        &query.connection_mp_ref,
        &query.date,
    )
    .await
    .map_err(|error| {
        tracing::error!("wb_sales_funnel_orders a015 lookup failed: {}", error);
        ApiError::from(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
    })?;

    let srids: Vec<String> = orders
        .iter()
        .map(|o| o.header.document_no.clone())
        .collect();
    let paid_map =
        crate::projections::p913_wb_advert_order_attr::repository::sum_reserve_by_order_keys(
            &srids, None,
        )
        .await
        .unwrap_or_default();

    let mut items = Vec::new();
    let mut paid_count = 0i64;
    let mut free_count = 0i64;
    for order in &orders {
        let srid = order.header.document_no.clone();
        let is_paid = paid_map.contains_key(&srid);
        if is_paid {
            paid_count += 1;
        } else {
            free_count += 1;
        }

        let keep = match channel {
            FunnelOrderChannel::Paid => is_paid,
            FunnelOrderChannel::Free => !is_paid,
            FunnelOrderChannel::All => true,
        };
        if !keep {
            continue;
        }

        let advert_campaign = if is_paid {
            crate::projections::p913_wb_advert_order_attr::repository::list_by_order_key_and_turnover(
                &srid,
                "advert_clicks_order_accrual",
            )
            .await
            .ok()
            .and_then(|rows| {
                rows.into_iter()
                    .find_map(|r| Some(r.wb_advert_campaign_code).filter(|c| !c.is_empty()))
            })
        } else {
            None
        };

        items.push(WbSalesFunnelOrderItem {
            srid,
            order_date:
                crate::projections::p916_mp_sales_funnel_turnovers::builder::msk_date_from_utc(
                    &order.state.order_dt,
                ),
            amount: order.line.allocation_basis(),
            is_cancel: order.state.is_cancel,
            is_paid,
            advert_campaign,
        });
    }

    Ok(Json(WbSalesFunnelOrdersResponse {
        items,
        paid_count,
        free_count,
    }))
}

/// Карта `connection_mp_ref → (человекочитаемое название МП, код типа МП)`.
/// Резолв через a006 (подключение → marketplace_id) и a005 (справочник → тип МП).
/// Ошибки чтения не критичны — при их наличии колонка «Маркетплейс» будет пустой.
async fn funnel_marketplace_labels() -> HashMap<String, (Option<String>, Option<String>)> {
    use contracts::enums::marketplace_type::MarketplaceType;

    let marketplaces = crate::domain::a005_marketplace::repository::list_all()
        .await
        .unwrap_or_default();
    // marketplace_id → тип МП.
    let type_by_marketplace: HashMap<String, MarketplaceType> = marketplaces
        .into_iter()
        .filter_map(|mp| mp.marketplace_type.map(|t| (mp.base.id.0.to_string(), t)))
        .collect();

    let connections = crate::domain::a006_connection_mp::repository::list_all()
        .await
        .unwrap_or_default();
    connections
        .into_iter()
        .map(|conn| {
            let label = type_by_marketplace
                .get(&conn.marketplace_id)
                .map(|t| {
                    (
                        Some(t.display_name().to_string()),
                        Some(t.code().to_string()),
                    )
                })
                .unwrap_or((None, None));
            (conn.to_string_id(), label)
        })
        .collect()
}
