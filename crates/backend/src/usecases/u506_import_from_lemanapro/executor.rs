use super::{lemanapro_api_client::LemanaProApiClient, progress_tracker::ProgressTracker};
use crate::domain::a007_marketplace_product;
use anyhow::Result;
use contracts::domain::common::AggregateId;
use contracts::usecases::u506_import_from_lemanapro::{
    progress::ImportStatus,
    request::ImportRequest,
    response::{ImportResponse, ImportStartStatus},
};
use std::sync::Arc;
use uuid::Uuid;

/// Executor для UseCase импорта из ЛеманаПро
pub struct ImportExecutor {
    api_client: Arc<LemanaProApiClient>,
    pub progress_tracker: Arc<ProgressTracker>,
}

impl ImportExecutor {
    pub fn new(progress_tracker: Arc<ProgressTracker>) -> Self {
        Self {
            api_client: Arc::new(LemanaProApiClient::new()),
            progress_tracker,
        }
    }

    /// Запустить импорт (создает async task и возвращает session_id)
    pub async fn start_import(&self, request: ImportRequest) -> Result<ImportResponse> {
        let database_activity = crate::system::maintenance::try_begin_database_activity()
            .ok_or_else(|| {
                anyhow::anyhow!("Импорт недоступен во время обслуживания базы данных")
            })?;
        // Валидация запроса
        let connection_id = Uuid::parse_str(&request.connection_id)
            .map_err(|_| anyhow::anyhow!("Invalid connection_id"))?;

        // Получить подключение к маркетплейсу
        let connection = crate::domain::a006_connection_mp::service::get_by_id(connection_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Connection not found"))?;

        // Создать сессию импорта
        let session_id = Uuid::new_v4().to_string();
        self.progress_tracker.create_session(session_id.clone());

        // Добавить агрегаты для отслеживания
        for aggregate_index in &request.target_aggregates {
            let aggregate_name = match aggregate_index.as_str() {
                "a007_marketplace_product" => "Товары маркетплейса",
                _ => "Unknown",
            };
            self.progress_tracker.add_aggregate(
                &session_id,
                aggregate_index.clone(),
                aggregate_name.to_string(),
            );
        }

        // Запустить импорт в фоне
        let self_clone = Arc::new(self.clone());
        let session_id_clone = session_id.clone();
        let request_clone = request.clone();
        let connection_clone = connection.clone();

        tokio::spawn(async move {
            let _database_activity = database_activity;
            if let Err(e) = self_clone
                .execute_import(&session_id_clone, &request_clone, &connection_clone)
                .await
            {
                tracing::error!("Import failed: {}", e);
                self_clone.progress_tracker.add_error(
                    &session_id_clone,
                    None,
                    format!("Import failed: {}", e),
                    None,
                );
                self_clone
                    .progress_tracker
                    .complete_session(&session_id_clone, ImportStatus::Failed);
            }
        });

        Ok(ImportResponse {
            session_id,
            status: ImportStartStatus::Started,
            message: "Импорт запущен".to_string(),
        })
    }

    /// Получить текущий прогресс импорта
    pub fn get_progress(
        &self,
        session_id: &str,
    ) -> Option<contracts::usecases::u506_import_from_lemanapro::progress::ImportProgress> {
        self.progress_tracker.get_progress(session_id)
    }

    /// Выполнить импорт
    pub async fn execute_import(
        &self,
        session_id: &str,
        request: &ImportRequest,
        connection: &contracts::domain::a006_connection_mp::aggregate::ConnectionMP,
    ) -> Result<()> {
        tracing::info!("Starting LemanaPro import for session: {}", session_id);

        for aggregate_index in &request.target_aggregates {
            match aggregate_index.as_str() {
                "a007_marketplace_product" => {
                    self.import_marketplace_products(session_id, connection)
                        .await?;
                }
                _ => {
                    let msg = format!("Unknown aggregate: {}", aggregate_index);
                    tracing::warn!("{}", msg);
                    self.progress_tracker.add_error(
                        session_id,
                        Some(aggregate_index.clone()),
                        msg,
                        None,
                    );
                }
            }
        }

        // Завершить импорт
        let final_status = if self
            .progress_tracker
            .get_progress(session_id)
            .map(|p| p.total_errors > 0)
            .unwrap_or(false)
        {
            ImportStatus::CompletedWithErrors
        } else {
            ImportStatus::Completed
        };

        self.progress_tracker
            .complete_session(session_id, final_status);
        tracing::info!("Import completed for session: {}", session_id);

        Ok(())
    }

    /// Импорт товаров из ЛеманаПро
    async fn import_marketplace_products(
        &self,
        session_id: &str,
        connection: &contracts::domain::a006_connection_mp::aggregate::ConnectionMP,
    ) -> Result<()> {
        tracing::info!("Importing marketplace products for session: {}", session_id);

        let aggregate_index = "a007_marketplace_product";
        let per_page = 100; // API supports up to 1500, but 100 is reasonable for batch processing
        let mut total_processed = 0;
        let mut total_inserted = 0;
        let mut total_updated = 0;
        let mut page = 1;

        // Получаем товары страницами через /b2bintegration-products/v1/products
        loop {
            let response = self
                .api_client
                .fetch_products(connection, page, per_page, None)
                .await?;

            // На первой странице получаем общее количество
            if page == 1 {
                let total_count = response.paging.as_ref().map(|p| p.total_count);
                tracing::info!("Total products available: {:?}", total_count);
            }

            let products = response.products;
            if products.is_empty() {
                break;
            }

            let batch_size = products.len();
            tracing::info!(
                "Processing page {}: {} items, total so far: {}",
                page,
                batch_size,
                total_processed
            );

            // Обрабатываем каждый товар
            for product in products {
                let display_name = format!("{} - {}", product.product_item, product.product_name);

                self.progress_tracker.set_current_item(
                    session_id,
                    aggregate_index,
                    Some(display_name),
                );

                match self.process_product(connection, &product).await {
                    Ok(is_new) => {
                        total_processed += 1;
                        if is_new {
                            total_inserted += 1;
                        } else {
                            total_updated += 1;
                        }
                    }
                    Err(e) => {
                        tracing::error!(
                            "Failed to process product {}: {}",
                            product.product_item,
                            e
                        );
                        self.progress_tracker.add_error(
                            session_id,
                            Some(aggregate_index.to_string()),
                            format!("Failed to process product {}", product.product_item),
                            Some(e.to_string()),
                        );
                    }
                }

                // Обновить прогресс
                self.progress_tracker.update_aggregate(
                    session_id,
                    aggregate_index,
                    total_processed,
                    response.paging.as_ref().map(|p| p.total_count),
                    total_inserted,
                    total_updated,
                );
            }

            // Очистить текущий элемент после страницы
            self.progress_tracker
                .set_current_item(session_id, aggregate_index, None);

            // Проверяем, есть ли еще страницы
            if let Some(paging) = &response.paging {
                let total_pages =
                    (paging.total_count as f64 / paging.per_page as f64).ceil() as i32;
                if page >= total_pages {
                    break;
                }
            } else {
                // Если нет информации о пагинации, выходим
                break;
            }

            // Если получили меньше per_page, значит это последняя страница
            if batch_size < per_page as usize {
                break;
            }

            page += 1;
        }

        self.progress_tracker
            .complete_aggregate(session_id, aggregate_index);
        tracing::info!(
            "Marketplace products import completed: processed={}, inserted={}, updated={}",
            total_processed,
            total_inserted,
            total_updated
        );

        Ok(())
    }

    /// Обработать один товар (upsert)
    async fn process_product(
        &self,
        connection: &contracts::domain::a006_connection_mp::aggregate::ConnectionMP,
        product: &super::lemanapro_api_client::LemanaProProduct,
    ) -> Result<bool> {
        use contracts::domain::a007_marketplace_product::aggregate::MarketplaceProduct;

        // Проверяем, существует ли товар по marketplace_sku (productItem)
        let marketplace_sku = product.product_item.to_string();
        let existing = a007_marketplace_product::repository::get_by_connection_and_sku(
            &connection.base.id.as_string(),
            &marketplace_sku,
        )
        .await?;

        // Получаем category_id и category_name из categories
        let (category_id, category_name) = product
            .categories
            .as_ref()
            .map(|cat| (cat.category_id.clone(), cat.category_name.clone()))
            .unwrap_or((None, None));

        // Получаем бренд
        let brand = product.product_brand.clone();

        // Получаем barcode
        let barcode = product.product_barcode.clone();

        if let Some(mut existing_product) = existing {
            // Обновляем существующий товар
            tracing::debug!("Updating existing product: {}", marketplace_sku);

            existing_product.base.code = marketplace_sku.clone();
            existing_product.base.description = product.product_name.clone();
            existing_product.marketplace_sku = marketplace_sku.clone();
            existing_product.barcode = barcode.clone();
            existing_product.article = marketplace_sku.clone();
            existing_product.brand = brand.clone();
            existing_product.category_id = category_id.clone();
            existing_product.category_name = category_name.clone();
            existing_product.last_update = Some(chrono::Utc::now());
            existing_product.before_write();

            a007_marketplace_product::repository::update(&existing_product).await?;
            Ok(false)
        } else {
            // Создаем новый товар
            tracing::debug!("Inserting new product: {}", marketplace_sku);

            let mut new_product = MarketplaceProduct::new_for_insert(
                marketplace_sku.clone(),
                product.product_name.clone(),
                connection.marketplace_id.clone(),
                connection.base.id.as_string(),
                marketplace_sku.clone(),
                barcode,
                marketplace_sku.clone(),
                brand,
                category_id,
                category_name,
                Some(chrono::Utc::now()),
                None, // nomenclature_ref
                Some("Imported from LemanaPro".to_string()),
            );

            // Автоматический поиск номенклатуры по артикулу
            let _ =
                a007_marketplace_product::service::search_and_set_nomenclature(&mut new_product)
                    .await;

            a007_marketplace_product::repository::insert(&new_product).await?;
            Ok(true)
        }
    }
}

impl Clone for ImportExecutor {
    fn clone(&self) -> Self {
        Self {
            api_client: Arc::clone(&self.api_client),
            progress_tracker: Arc::clone(&self.progress_tracker),
        }
    }
}
