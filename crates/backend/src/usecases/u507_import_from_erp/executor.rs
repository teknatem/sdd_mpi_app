use super::{erp_api_client::ErpApiClient, progress_tracker::ProgressTracker};
use anyhow::Result;
use contracts::domain::a021_production_output::aggregate::ProductionOutput;
use contracts::usecases::u507_import_from_erp::{
    progress::ImportStatus,
    request::ImportRequest,
    response::{ImportResponse, ImportStartStatus},
};
use std::sync::Arc;
use uuid::Uuid;

/// Executor для UseCase импорта из ERP (Выпуск продукции)
pub struct ImportExecutor {
    api_client: Arc<ErpApiClient>,
    pub progress_tracker: Arc<ProgressTracker>,
}

impl ImportExecutor {
    pub fn new(progress_tracker: Arc<ProgressTracker>) -> Self {
        Self {
            api_client: Arc::new(ErpApiClient::new()),
            progress_tracker,
        }
    }

    /// Запустить импорт (создаёт async task и возвращает session_id)
    pub async fn start_import(&self, request: ImportRequest) -> Result<ImportResponse> {
        let database_activity = crate::system::maintenance::try_begin_database_activity()
            .ok_or_else(|| {
                anyhow::anyhow!("Импорт недоступен во время обслуживания базы данных")
            })?;
        let connection_id = Uuid::parse_str(&request.connection_id)
            .map_err(|_| anyhow::anyhow!("Invalid connection_id"))?;

        // Проверить существование подключения
        let connection = crate::domain::a001_connection_1c::service::get_by_id(connection_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("1C connection not found"))?;

        let session_id = Uuid::new_v4().to_string();
        self.progress_tracker.create_session(session_id.clone());

        let executor = Arc::new(Self {
            api_client: self.api_client.clone(),
            progress_tracker: self.progress_tracker.clone(),
        });
        let sid = session_id.clone();
        let req = request.clone();
        let conn = connection.clone();

        tokio::spawn(async move {
            let _database_activity = database_activity;
            if let Err(e) = executor.execute_import(&sid, &req, &conn).await {
                tracing::error!("ERP import failed: {}", e);
                executor
                    .progress_tracker
                    .add_error(&sid, format!("Import failed: {}", e));
                executor
                    .progress_tracker
                    .complete_session(&sid, ImportStatus::Failed);
            }
        });

        Ok(ImportResponse {
            session_id,
            status: ImportStartStatus::Started,
            message: "Import started".to_string(),
        })
    }

    /// Получить прогресс сессии
    pub fn get_progress(
        &self,
        session_id: &str,
    ) -> Option<contracts::usecases::u507_import_from_erp::progress::ImportProgress> {
        self.progress_tracker.get_progress(session_id)
    }

    /// Выполнить импорт (фоновая задача)
    async fn execute_import(
        &self,
        session_id: &str,
        request: &ImportRequest,
        connection: &contracts::domain::a001_connection_1c::aggregate::Connection1CDatabase,
    ) -> Result<()> {
        tracing::info!(
            "Starting ERP production output import for period {} - {}",
            request.date_from,
            request.date_to
        );

        // Запрос к API 1С
        let response = self
            .api_client
            .fetch_production_output(connection, &request.date_from, &request.date_to)
            .await?;

        let total = response.data.len() as i32;
        self.progress_tracker.set_total(session_id, total);
        tracing::info!("Got {} documents from ERP API", total);

        let mut inserted = 0i32;
        let mut updated = 0i32;
        let mut errors = 0i32;

        for (idx, item) in response.data.iter().enumerate() {
            self.progress_tracker.update_progress(
                session_id,
                idx as i32,
                inserted,
                updated,
                Some(format!("{} {}", item.document_no, item.article)),
            );

            // Парсинг UUID из 1С
            let id = match Uuid::parse_str(&item.id) {
                Ok(v) => v,
                Err(e) => {
                    let msg = format!("Invalid UUID '{}': {}", item.id, e);
                    tracing::warn!("{}", msg);
                    self.progress_tracker.add_error(session_id, msg);
                    errors += 1;
                    continue;
                }
            };

            let mut doc = ProductionOutput::new_from_api(
                id,
                item.document_no.clone(),
                item.document_date.clone(),
                item.description.clone(),
                item.article.clone(),
                item.count,
                item.amount,
                request.connection_id.clone(),
            );

            // Заполнить nomenclature_ref если пустой
            if let Err(e) =
                crate::domain::a021_production_output::service::fill_nomenclature_ref_if_empty(
                    &mut doc,
                )
                .await
            {
                tracing::warn!(
                    "Failed to fill nomenclature_ref for {}: {}",
                    item.article,
                    e
                );
            }

            match crate::domain::a021_production_output::service::upsert_from_api(&doc).await {
                Ok((_, is_new)) => {
                    if is_new {
                        inserted += 1;
                    } else {
                        updated += 1;
                    }
                }
                Err(e) => {
                    let msg = format!("Failed to upsert document {}: {}", item.document_no, e);
                    tracing::error!("{}", msg);
                    self.progress_tracker.add_error(session_id, msg);
                    errors += 1;
                }
            }
        }

        self.progress_tracker
            .update_progress(session_id, total, inserted, updated, None);

        let final_status = if errors > 0 {
            ImportStatus::CompletedWithErrors
        } else {
            ImportStatus::Completed
        };

        self.progress_tracker
            .complete_session(session_id, final_status);

        tracing::info!(
            "ERP import finished: inserted={}, updated={}, errors={}",
            inserted,
            updated,
            errors
        );

        Ok(())
    }
}
