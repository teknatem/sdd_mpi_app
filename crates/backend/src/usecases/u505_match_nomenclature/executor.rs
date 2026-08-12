use super::progress_tracker::ProgressTracker;
use crate::domain::{a004_nomenclature, a007_marketplace_product};
use anyhow::Result;
use contracts::domain::common::AggregateId;
use contracts::usecases::u505_match_nomenclature::{
    progress::MatchStatus,
    request::MatchRequest,
    response::{MatchResponse, MatchStartStatus},
};
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

/// Executor для UseCase сопоставления номенклатуры
pub struct MatchExecutor {
    pub progress_tracker: Arc<ProgressTracker>,
}

impl MatchExecutor {
    pub fn new(progress_tracker: Arc<ProgressTracker>) -> Self {
        Self { progress_tracker }
    }

    /// Запустить сопоставление (создает async task и возвращает session_id)
    pub async fn start_matching(&self, request: MatchRequest) -> Result<MatchResponse> {
        let database_activity = crate::system::maintenance::try_begin_database_activity()
            .ok_or_else(|| {
                anyhow::anyhow!("Сопоставление недоступно во время обслуживания базы данных")
            })?;
        tracing::info!("Starting nomenclature matching with request: {:?}", request);

        // Создать сессию сопоставления
        let session_id = Uuid::new_v4().to_string();

        // Получить количество товаров для обработки
        let products = a007_marketplace_product::repository::list_for_matching(
            request.marketplace_id.as_deref(),
            !request.overwrite_existing,
        )
        .await?;

        let total = products.len() as i32;
        self.progress_tracker
            .create_session(session_id.clone(), Some(total));

        // Запустить сопоставление в фоне
        let self_clone = Arc::new(self.clone());
        let session_id_clone = session_id.clone();
        let request_clone = request.clone();

        tokio::spawn(async move {
            let _database_activity = database_activity;
            if let Err(e) = self_clone
                .execute_matching(&session_id_clone, &request_clone)
                .await
            {
                tracing::error!("Matching failed: {}", e);
                self_clone.progress_tracker.add_error(
                    &session_id_clone,
                    format!("Matching failed: {}", e),
                    None,
                    None,
                );
                self_clone
                    .progress_tracker
                    .complete_session(&session_id_clone, MatchStatus::Failed);
            }
        });

        Ok(MatchResponse {
            session_id,
            status: MatchStartStatus::Started,
            message: format!("Сопоставление запущено для {} товаров", total),
        })
    }

    /// Получить текущий прогресс сопоставления
    pub fn get_progress(
        &self,
        session_id: &str,
    ) -> Option<contracts::usecases::u505_match_nomenclature::progress::MatchProgress> {
        self.progress_tracker.get_progress(session_id)
    }

    /// Построить индекс артикул -> номенклатура для быстрого поиска
    /// Возвращает HashMap где ключ - нормализованный артикул, значение - список номенклатур
    /// Если ignore_case=true, ключи будут в нижнем регистре
    async fn build_article_index(
        ignore_case: bool,
    ) -> Result<HashMap<String, Vec<contracts::domain::a004_nomenclature::aggregate::Nomenclature>>>
    {
        let start_time = std::time::Instant::now();

        // Загрузить все элементы номенклатуры (не папки, не удаленные)
        let all_items = a004_nomenclature::repository::list_all().await?;

        let items: Vec<_> = all_items
            .into_iter()
            .filter(|n| !n.is_folder && !n.base.metadata.is_deleted)
            .collect();

        tracing::info!("Loaded {} nomenclature items for indexing", items.len());

        let mut index: HashMap<String, Vec<_>> = HashMap::new();

        for item in items {
            let article = item.article.trim();
            if article.is_empty() {
                continue;
            }

            let key = if ignore_case {
                article.to_lowercase()
            } else {
                article.to_string()
            };

            index.entry(key).or_insert_with(Vec::new).push(item);
        }

        let duration = start_time.elapsed();
        tracing::info!(
            "Built article index in {:?}ms: {} unique articles (ignore_case: {})",
            duration.as_millis(),
            index.len(),
            ignore_case
        );

        Ok(index)
    }

    /// Выполнить сопоставление
    pub async fn execute_matching(&self, session_id: &str, request: &MatchRequest) -> Result<()> {
        let overall_start = std::time::Instant::now();
        tracing::info!("Running matching for session: {}", session_id);

        // Загрузить товары маркетплейса
        let load_products_start = std::time::Instant::now();
        let products = a007_marketplace_product::repository::list_for_matching(
            request.marketplace_id.as_deref(),
            !request.overwrite_existing,
        )
        .await?;
        let load_products_duration = load_products_start.elapsed();
        tracing::info!(
            "Loaded {} products in {:?}ms",
            products.len(),
            load_products_duration.as_millis()
        );

        // Построить индекс артикул -> номенклатура
        let build_index_start = std::time::Instant::now();
        let article_index = Self::build_article_index(request.ignore_case).await?;
        let build_index_duration = build_index_start.elapsed();

        let mut processed = 0;
        let mut matched = 0;
        let mut cleared = 0;
        let mut skipped = 0;
        let mut ambiguous = 0;

        // Обработать каждый товар
        let process_start = std::time::Instant::now();
        for product in products {
            // Установить текущий товар
            self.progress_tracker.set_current_item(
                session_id,
                Some(format!(
                    "{} - {}",
                    product.article, product.base.description
                )),
            );

            match self
                .process_product(&product, request, &article_index)
                .await
            {
                Ok(result) => {
                    processed += 1;
                    match result {
                        MatchResult::Matched => matched += 1,
                        MatchResult::Cleared => cleared += 1,
                        MatchResult::ClearedAmbiguous => {
                            cleared += 1;
                            ambiguous += 1;
                        }
                        MatchResult::AmbiguousUnchanged => ambiguous += 1,
                        MatchResult::Skipped => skipped += 1,
                    }
                }
                Err(e) => {
                    tracing::error!("Failed to process product {}: {}", product.article, e);
                    self.progress_tracker.add_error(
                        session_id,
                        format!("Failed to process product {}", product.article),
                        Some(e.to_string()),
                        Some(product.article.clone()),
                    );
                    processed += 1;
                }
            }

            // Обновить прогресс
            self.progress_tracker
                .update_progress(session_id, processed, matched, cleared, skipped, ambiguous);
        }
        let process_duration = process_start.elapsed();

        // Очистить текущий элемент
        self.progress_tracker.set_current_item(session_id, None);

        // Обновить счетчики mp_ref_count для всей номенклатуры
        let update_counts_start = std::time::Instant::now();
        tracing::info!("Updating mp_ref_count for all nomenclature...");
        if let Err(e) = self.update_mp_ref_counts().await {
            tracing::error!("Failed to update mp_ref_count: {}", e);
            self.progress_tracker.add_error(
                session_id,
                "Failed to update mp_ref_count".to_string(),
                Some(e.to_string()),
                None,
            );
        }
        let update_counts_duration = update_counts_start.elapsed();

        // Завершить сессию
        let final_status = if self
            .progress_tracker
            .get_progress(session_id)
            .map(|p| p.errors > 0)
            .unwrap_or(false)
        {
            MatchStatus::CompletedWithErrors
        } else {
            MatchStatus::Completed
        };

        self.progress_tracker
            .complete_session(session_id, final_status);

        let overall_duration = overall_start.elapsed();
        let avg_time_per_product = if processed > 0 {
            process_duration.as_millis() as f64 / processed as f64
        } else {
            0.0
        };

        // Аудит производительности
        tracing::info!("=== Performance Audit for session {} ===", session_id);
        tracing::info!(
            "Total time: {:?}ms ({:.2}s)",
            overall_duration.as_millis(),
            overall_duration.as_secs_f64()
        );
        tracing::info!("Load products: {:?}ms", load_products_duration.as_millis());
        tracing::info!("Build index: {:?}ms", build_index_duration.as_millis());
        tracing::info!(
            "Process products: {:?}ms ({:.2}s)",
            process_duration.as_millis(),
            process_duration.as_secs_f64()
        );
        tracing::info!("Average time per product: {:.2}ms", avg_time_per_product);
        tracing::info!(
            "Update mp_ref_count: {:?}ms",
            update_counts_duration.as_millis()
        );
        tracing::info!(
            "Results: Processed: {}, Matched: {}, Cleared: {}, Skipped: {}, Ambiguous: {}",
            processed,
            matched,
            cleared,
            skipped,
            ambiguous
        );
        tracing::info!("=== End Performance Audit ===");

        tracing::info!(
            "Matching completed for session: {}. Processed: {}, Matched: {}, Cleared: {}, Skipped: {}, Ambiguous: {}",
            session_id,
            processed,
            matched,
            cleared,
            skipped,
            ambiguous
        );

        Ok(())
    }

    /// Обновить счетчики mp_ref_count для всей номенклатуры
    async fn update_mp_ref_counts(&self) -> Result<()> {
        use std::collections::HashMap;

        // Получить все товары маркетплейса с nomenclature_id
        let all_products = a007_marketplace_product::service::list_all().await?;

        // Подсчитать количество ссылок для каждой номенклатуры
        let mut ref_counts: HashMap<String, i32> = HashMap::new();
        for product in all_products {
            if let Some(nomenclature_id) = &product.nomenclature_ref {
                *ref_counts.entry(nomenclature_id.clone()).or_insert(0) += 1;
            }
        }

        tracing::info!(
            "Found {} nomenclature items with marketplace references",
            ref_counts.len()
        );

        // Получить всю номенклатуру
        let all_nomenclature = a004_nomenclature::service::list_all().await?;

        // Обновить счетчики для всей номенклатуры
        for nomenclature in all_nomenclature {
            let nomenclature_id_str = nomenclature.base.id.as_string();
            let count = ref_counts.get(&nomenclature_id_str).copied().unwrap_or(0);

            // Обновить только если значение изменилось
            if nomenclature.mp_ref_count != count {
                a004_nomenclature::repository::update_mp_ref_count(
                    nomenclature.base.id.value(),
                    count,
                )
                .await?;
            }
        }

        tracing::info!("Successfully updated mp_ref_count for all nomenclature");
        Ok(())
    }

    /// Обработать один товар
    async fn process_product(
        &self,
        product: &contracts::domain::a007_marketplace_product::aggregate::MarketplaceProduct,
        request: &MatchRequest,
        article_index: &HashMap<
            String,
            Vec<contracts::domain::a004_nomenclature::aggregate::Nomenclature>,
        >,
    ) -> Result<MatchResult> {
        // Проверить, нужно ли обрабатывать товар
        if !request.overwrite_existing && product.nomenclature_ref.is_some() {
            tracing::debug!(
                "Skipping product {} - already has nomenclature_id and overwrite_existing=false",
                product.article
            );
            return Ok(MatchResult::Skipped);
        }

        // Найти номенклатуру по артикулу
        let article = product.article.trim();
        if article.is_empty() {
            tracing::warn!("Product {} has empty article", product.base.id.as_string());
            return self.clear_nomenclature_link(product).await;
        }

        tracing::debug!(
            "Searching for article: '{}' (ignore_case: {})",
            article,
            request.ignore_case
        );

        // Нормализовать артикул для поиска в индексе
        let search_key = if request.ignore_case {
            article.to_lowercase()
        } else {
            article.to_string()
        };

        // Получить список номенклатур из индекса
        let found_items = article_index.get(&search_key);
        let found_count = found_items.map(|items| items.len()).unwrap_or(0);

        tracing::debug!(
            "Found {} nomenclature items for article '{}'",
            found_count,
            article
        );

        match found_count {
            0 => {
                // Не найдено совпадений - очистить связь
                tracing::debug!("No nomenclature found for article: {}", article);
                self.clear_nomenclature_link(product).await
            }
            1 => {
                // Найдено ровно 1 совпадение - установить связь
                let nomenclature = &found_items.expect("count checked")[0];
                tracing::info!(
                    "Matched product {} with nomenclature {} ({})",
                    article,
                    nomenclature.base.code,
                    nomenclature.base.description
                );
                self.set_nomenclature_link(product, nomenclature.base.id.as_string())
                    .await
            }
            _ => {
                // Найдено >1 совпадений - очистить связь
                tracing::warn!(
                    "Ambiguous match for article {}: found {} nomenclature items",
                    article,
                    found_count
                );
                self.clear_nomenclature_link_ambiguous(product).await
            }
        }
    }

    /// Установить связь с номенклатурой
    async fn set_nomenclature_link(
        &self,
        product: &contracts::domain::a007_marketplace_product::aggregate::MarketplaceProduct,
        nomenclature_id: String,
    ) -> Result<MatchResult> {
        if product.nomenclature_ref.as_deref() == Some(nomenclature_id.as_str()) {
            return Ok(MatchResult::Skipped);
        }

        let mut product_clone = product.clone();
        product_clone.nomenclature_ref = Some(nomenclature_id);
        product_clone.before_write();

        a007_marketplace_product::repository::update(&product_clone).await?;
        Ok(MatchResult::Matched)
    }

    /// Очистить связь с номенклатурой (не найдено совпадений)
    async fn clear_nomenclature_link(
        &self,
        product: &contracts::domain::a007_marketplace_product::aggregate::MarketplaceProduct,
    ) -> Result<MatchResult> {
        if product.nomenclature_ref.is_none() {
            // Уже пусто - это no-op, не считаем очисткой
            return Ok(MatchResult::Skipped);
        }

        let mut product_clone = product.clone();
        product_clone.nomenclature_ref = None;
        product_clone.before_write();

        a007_marketplace_product::repository::update(&product_clone).await?;
        Ok(MatchResult::Cleared)
    }

    /// Очистить связь с номенклатурой (неоднозначное сопоставление)
    async fn clear_nomenclature_link_ambiguous(
        &self,
        product: &contracts::domain::a007_marketplace_product::aggregate::MarketplaceProduct,
    ) -> Result<MatchResult> {
        if product.nomenclature_ref.is_none() {
            // Уже пусто - связь не меняем, но ambiguity нужно показать в статистике
            return Ok(MatchResult::AmbiguousUnchanged);
        }

        let mut product_clone = product.clone();
        product_clone.nomenclature_ref = None;
        product_clone.before_write();

        a007_marketplace_product::repository::update(&product_clone).await?;
        Ok(MatchResult::ClearedAmbiguous)
    }
}

impl Clone for MatchExecutor {
    fn clone(&self) -> Self {
        Self {
            progress_tracker: Arc::clone(&self.progress_tracker),
        }
    }
}

/// Результат обработки одного товара
enum MatchResult {
    /// Успешно сопоставлен
    Matched,
    /// Связь очищена (не найдено совпадений)
    Cleared,
    /// Связь очищена (неоднозначное сопоставление)
    ClearedAmbiguous,
    /// Найдено неоднозначное сопоставление, но связь и так была пустой
    AmbiguousUnchanged,
    /// Пропущен
    Skipped,
}
