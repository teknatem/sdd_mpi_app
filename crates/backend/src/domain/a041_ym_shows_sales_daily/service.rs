use anyhow::Result;
use contracts::domain::a041_ym_shows_sales_daily::aggregate::YmShowsSalesDaily;
use uuid::Uuid;

use super::repository;

/// Замена периода целиком (импорт отчёта «Аналитика продаж»).
pub async fn replace_for_period(
    connection_id: &str,
    date_from: &str,
    date_to: &str,
    documents: &[YmShowsSalesDaily],
) -> Result<usize> {
    repository::replace_for_period(connection_id, date_from, date_to, documents).await
}

pub async fn get_by_id(id: Uuid) -> Result<Option<YmShowsSalesDaily>> {
    repository::get_by_id(id).await
}

pub async fn list_paginated(
    query: repository::YmShowsSalesListQuery,
) -> Result<repository::YmShowsSalesListResult> {
    repository::list_paginated(query).await
}

/// Проведение документа: пересобрать его движения воронки p916 (стадия marketing).
pub async fn post_document(id: Uuid) -> Result<()> {
    repository::post_document(id).await
}
