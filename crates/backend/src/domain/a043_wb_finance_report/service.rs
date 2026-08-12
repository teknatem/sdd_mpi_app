use anyhow::Result;
use contracts::domain::a043_wb_finance_report::WbFinanceReport;
use uuid::Uuid;

use super::repository::{self, FinanceReportListQuery, FinanceReportListResult, LinesPage};

pub async fn upsert_complete(document: &WbFinanceReport) -> Result<()> {
    document.validate().map_err(anyhow::Error::msg)?;
    repository::upsert_complete(document).await
}

pub async fn get_by_id(id: Uuid) -> Result<Option<WbFinanceReport>> {
    repository::get_by_id(id).await
}

pub async fn list(query: FinanceReportListQuery) -> Result<FinanceReportListResult> {
    repository::list(query).await
}

pub async fn lines(id: Uuid, offset: usize, limit: usize) -> Result<Option<LinesPage>> {
    repository::lines(id, offset, limit.min(500)).await
}
