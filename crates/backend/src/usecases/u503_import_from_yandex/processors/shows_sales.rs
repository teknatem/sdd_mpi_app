//! Сборка документов a041 из строк отчёта «Аналитика продаж» YM.
//!
//! Отчёт приходит плоским списком `offer_id × день`; группируем по дате в документы
//! кабинета и резолвим товарные ссылки через a007 по `offer_id` (он же `shop_sku`
//! строк заказа a013).

use anyhow::Result;
use std::collections::{BTreeMap, HashMap};

use contracts::domain::a006_connection_mp::aggregate::ConnectionMP;
use contracts::domain::a041_ym_shows_sales_daily::aggregate::{
    YmShowsSalesDaily, YmShowsSalesDailyHeader, YmShowsSalesDailyLine, YmShowsSalesDailyMetrics,
    YmShowsSalesDailySourceMeta,
};
use contracts::domain::common::AggregateId;

use super::super::yandex_api_client::YmShowsSalesRow;

/// Накопление опциональной метрики: `None` источника не превращает итог в 0,
/// но первое же `Some` делает итог определённым (N/A ≠ 0).
fn add_optional(target: &mut Option<i64>, source: Option<i64>) {
    if let Some(value) = source {
        *target = Some(target.unwrap_or(0) + value);
    }
}

fn append_totals(target: &mut YmShowsSalesDailyMetrics, source: &YmShowsSalesDailyMetrics) {
    add_optional(&mut target.shows, source.shows);
    add_optional(&mut target.by_msku_shows, source.by_msku_shows);
    add_optional(&mut target.clicks, source.clicks);
    add_optional(&mut target.to_cart, source.to_cart);
    add_optional(&mut target.order_items, source.order_items);
    add_optional(&mut target.order_sum, source.order_sum);
    add_optional(&mut target.delivered_count, source.delivered_count);
    add_optional(&mut target.delivered_sum, source.delivered_sum);
    add_optional(&mut target.canceled_count, source.canceled_count);
    add_optional(&mut target.returned_count, source.returned_count);
}

/// Строит документы a041 (по одному на дату) из строк отчёта.
/// Строки вне запрошенного окна и без активности отбрасываются.
pub async fn build_documents(
    connection: &ConnectionMP,
    campaign_id: Option<&str>,
    rows: &[YmShowsSalesRow],
    date_from: &str,
    date_to: &str,
) -> Result<Vec<YmShowsSalesDaily>> {
    let connection_id = connection.base.id.as_string();

    // Ссылки на товар резолвим один раз на offer_id: строк на день сотни, а
    // товаров — десятки.
    let mut product_refs: HashMap<String, (Option<String>, Option<String>)> = HashMap::new();

    let mut by_date: BTreeMap<String, Vec<YmShowsSalesDailyLine>> = BTreeMap::new();

    for row in rows {
        let Some(date) = row.date() else {
            continue;
        };
        if date.as_str() < date_from || date.as_str() > date_to {
            continue;
        }
        if !row.has_activity() {
            continue;
        }
        let Some(offer_id) = row.offer_id.as_ref().filter(|s| !s.trim().is_empty()) else {
            continue;
        };

        if !product_refs.contains_key(offer_id) {
            let refs = crate::domain::a007_marketplace_product::service::get_by_connection_and_sku(
                &connection_id,
                offer_id,
            )
            .await?
            .map(|product| {
                (
                    Some(product.to_string_id()),
                    product.nomenclature_ref.clone(),
                )
            })
            .unwrap_or((None, None));
            product_refs.insert(offer_id.clone(), refs);
        }
        let (marketplace_product_ref, nomenclature_ref) =
            product_refs.get(offer_id).cloned().unwrap_or((None, None));

        by_date
            .entry(date)
            .or_default()
            .push(YmShowsSalesDailyLine {
                offer_id: offer_id.clone(),
                offer_name: row.offer_name.clone().unwrap_or_default(),
                marketplace_product_ref,
                nomenclature_ref,
                metrics: YmShowsSalesDailyMetrics {
                    shows: row.shows,
                    by_msku_shows: row.by_msku_shows,
                    clicks: row.clicks,
                    to_cart: row.to_cart,
                    order_items: row.order_items,
                    order_sum: row.order_items_total_amount,
                    delivered_count: row.order_items_delivered_count,
                    delivered_sum: row.order_items_delivered_total_amount,
                    canceled_count: row.order_items_canceled_count,
                    returned_count: row.order_items_returned_count,
                },
            });
    }

    let fetched_at = chrono::Utc::now().to_rfc3339();
    let mut documents = Vec::with_capacity(by_date.len());
    for (document_date, mut lines) in by_date {
        lines.sort_by(|a, b| a.offer_id.cmp(&b.offer_id));

        let mut totals = YmShowsSalesDailyMetrics::default();
        for line in &lines {
            append_totals(&mut totals, &line.metrics);
        }

        let header = YmShowsSalesDailyHeader {
            document_no: format!("YM-SF-{}", document_date),
            document_date: document_date.clone(),
            connection_id: connection_id.clone(),
            organization_id: connection.organization_ref.clone(),
            marketplace_id: connection.marketplace_id.clone(),
            campaign_id: campaign_id.map(|s| s.to_string()),
        };
        let source_meta = YmShowsSalesDailySourceMeta {
            source: "ym_shows_sales_report".to_string(),
            fetched_at: fetched_at.clone(),
        };

        let mut document = YmShowsSalesDaily::new_for_insert(header, totals, lines, source_meta);
        document.before_write();
        document
            .validate()
            .map_err(|error| anyhow::anyhow!(error))?;
        documents.push(document);
    }

    Ok(documents)
}
