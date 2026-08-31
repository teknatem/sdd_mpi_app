//! Сервисный слой проекции p914_mp_finance_turnovers.

/// Detail-строки для drill-down по ресурсу Главной книги.
pub struct GlDetail;

#[async_trait::async_trait]
impl crate::general_ledger::resource_detail::GlDetailSource for GlDetail {
    fn detail_table(&self) -> &'static str {
        "p914_mp_finance_turnovers"
    }

    async fn fetch(
        &self,
        gl: &crate::general_ledger::repository::Model,
    ) -> anyhow::Result<Vec<serde_json::Value>> {
        use super::repository::{Column, Entity};

        crate::general_ledger::resource_detail::fetch_linked_rows::<Entity>(
            gl,
            Column::GeneralLedgerRef,
            Column::RegistratorType,
            Column::RegistratorRef,
            Column::TurnoverCode,
        )
        .await
    }
}
