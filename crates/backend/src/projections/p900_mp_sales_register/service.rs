use super::{builder, repository};
use anyhow::Result;
use contracts::domain::a009_ozon_returns::aggregate::OzonReturns;
use contracts::domain::a010_ozon_fbs_posting::aggregate::OzonFbsPosting;
use contracts::domain::a011_ozon_fbo_posting::aggregate::OzonFboPosting;
use contracts::domain::a013_ym_order::aggregate::YmOrder;
use contracts::projections::p900_mp_sales_register::{DailyStat, MarketplaceStat};
use std::collections::HashMap;
use uuid::Uuid;

/// Проецировать OZON FBS Posting в Sales Register
pub async fn project_ozon_fbs(document: &OzonFbsPosting, document_id: Uuid) -> Result<()> {
    let entries = builder::from_ozon_fbs(document, &document_id.to_string()).await?;

    for entry in entries {
        repository::upsert_entry(&entry).await?;
    }

    tracing::info!(
        "Projected OZON FBS document {} into Sales Register ({} lines)",
        document.header.document_no,
        document.lines.len()
    );

    Ok(())
}

/// Проецировать OZON FBO Posting в Sales Register
pub async fn project_ozon_fbo(document: &OzonFboPosting, document_id: Uuid) -> Result<()> {
    let entries = builder::from_ozon_fbo(document, &document_id.to_string()).await?;

    for entry in entries {
        repository::upsert_entry(&entry).await?;
    }

    tracing::info!(
        "Projected OZON FBO document {} into Sales Register ({} lines)",
        document.header.document_no,
        document.lines.len()
    );

    Ok(())
}

/// Проецировать YM Order в Sales Register
pub async fn project_ym_order(document: &YmOrder, document_id: Uuid) -> Result<()> {
    let entries = builder::from_ym_order(document, &document_id.to_string()).await?;

    for entry in entries {
        repository::upsert_entry(&entry).await?;
    }

    tracing::info!(
        "Projected YM Order {} into Sales Register ({} lines)",
        document.header.document_no,
        document.lines.len()
    );

    Ok(())
}

/// Проецировать OZON Returns (возвраты) в Sales Register
/// ВАЖНО: Создает запись с отрицательными значениями qty и amount
pub async fn project_ozon_returns(document: &OzonReturns, document_id: Uuid) -> Result<()> {
    let entry = builder::from_ozon_returns(document, &document_id.to_string()).await?;
    repository::upsert_entry(&entry).await?;

    tracing::info!(
        "Projected OZON Return {} into Sales Register (negative qty: {})",
        document.return_id,
        document.quantity
    );

    Ok(())
}

/// Получить список продаж
pub async fn list_sales(limit: Option<u64>) -> Result<Vec<repository::Model>> {
    repository::list_sales(limit).await
}

/// Получить записи по маркетплейсу
pub async fn get_by_marketplace(
    marketplace: &str,
    limit: Option<u64>,
) -> Result<Vec<repository::Model>> {
    repository::get_by_marketplace(marketplace, limit).await
}

// =============================================================================
// Pass-through функции (CRUD через repository)
// =============================================================================

/// Получить список продаж с фильтрами
pub async fn list_with_filters(
    date_from: &str,
    date_to: &str,
    marketplace: Option<String>,
    organization_ref: Option<String>,
    connection_mp_ref: Option<String>,
    status_norm: Option<String>,
    seller_sku: Option<String>,
    limit: i32,
    offset: i32,
) -> Result<(Vec<repository::Model>, i32)> {
    repository::list_with_filters(
        date_from,
        date_to,
        marketplace,
        organization_ref,
        connection_mp_ref,
        status_norm,
        seller_sku,
        limit,
        offset,
    )
    .await
}

/// Получить одну запись по Natural Key
pub async fn get_by_id(
    marketplace: &str,
    document_no: &str,
    line_id: &str,
) -> Result<Option<repository::Model>> {
    repository::get_by_id(marketplace, document_no, line_id).await
}

/// Получить записи по registrator_ref
pub async fn get_by_registrator(registrator_ref: &str) -> Result<Vec<repository::Model>> {
    repository::get_by_registrator(registrator_ref).await
}

/// Удалить записи по registrator_ref
pub async fn delete_by_registrator(registrator_ref: &str) -> Result<u64> {
    repository::delete_by_registrator(registrator_ref).await
}

// =============================================================================
// Бизнес-логика: статистика (группировка и агрегация)
// =============================================================================

/// Рассчитать статистику продаж по датам
pub async fn calculate_daily_stats(
    date_from: &str,
    date_to: &str,
    marketplace: Option<String>,
) -> Result<Vec<DailyStat>> {
    // Получаем сырые данные из repository
    let items = repository::list_by_date_range(date_from, date_to, marketplace).await?;

    // Группировка по датам
    let mut stats_map: HashMap<String, DailyStat> = HashMap::new();

    for item in items {
        let stat = stats_map
            .entry(item.sale_date.clone())
            .or_insert(DailyStat {
                date: item.sale_date.clone(),
                sales_count: 0,
                total_qty: 0.0,
                total_revenue: 0.0,
            });
        stat.sales_count += 1;
        stat.total_qty += item.qty;
        stat.total_revenue += item.amount_line.unwrap_or(0.0);
    }

    let mut result: Vec<DailyStat> = stats_map.into_values().collect();
    result.sort_by(|a, b| a.date.cmp(&b.date));

    Ok(result)
}

/// Рассчитать статистику продаж по маркетплейсам
pub async fn calculate_marketplace_stats(
    date_from: &str,
    date_to: &str,
) -> Result<Vec<MarketplaceStat>> {
    // Получаем сырые данные из repository
    let items = repository::list_by_date_range(date_from, date_to, None).await?;

    // Группировка по маркетплейсам
    let mut stats_map: HashMap<String, MarketplaceStat> = HashMap::new();

    for item in items {
        let stat = stats_map
            .entry(item.marketplace.clone())
            .or_insert(MarketplaceStat {
                marketplace: item.marketplace.clone(),
                sales_count: 0,
                total_qty: 0.0,
                total_revenue: 0.0,
            });
        stat.sales_count += 1;
        stat.total_qty += item.qty;
        stat.total_revenue += item.amount_line.unwrap_or(0.0);
    }

    let mut result: Vec<MarketplaceStat> = stats_map.into_values().collect();
    result.sort_by(|a, b| a.marketplace.cmp(&b.marketplace));

    Ok(result)
}
