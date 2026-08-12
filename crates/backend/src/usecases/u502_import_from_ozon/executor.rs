use super::{
    ozon_api_client::OzonApiClient,
    processors::{postings, product, realization, returns, sales, transaction},
    progress_tracker::ProgressTracker,
};
use anyhow::Result;
use contracts::domain::common::AggregateId;
use contracts::system::tasks::progress::TaskProgress;
use contracts::usecases::u502_import_from_ozon::{
    progress::ImportStatus,
    request::ImportRequest,
    response::{ImportResponse, ImportStartStatus},
};
use std::sync::Arc;
use uuid::Uuid;

/// Executor для UseCase импорта из OZON
pub struct ImportExecutor {
    api_client: Arc<OzonApiClient>,
    pub progress_tracker: Arc<ProgressTracker>,
}

impl ImportExecutor {
    pub fn new(progress_tracker: Arc<ProgressTracker>) -> Self {
        Self {
            api_client: Arc::new(OzonApiClient::new()),
            progress_tracker,
        }
    }

    /// Только память: активные (`Running`) сессии для лёгкого мониторинга, без БД и без диска.
    pub fn list_live_task_progress(&self) -> Vec<TaskProgress> {
        self.progress_tracker
            .snapshot_sessions()
            .into_iter()
            .filter(|p| matches!(p.status, ImportStatus::Running))
            .map(Into::into)
            .collect()
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
                "a008_marketplace_sales" => "Продажи (фин. транзакции)",
                "a009_ozon_returns" => "Возвраты OZON",
                "a010_ozon_fbs_posting" => "OZON FBS Документы продаж",
                "a011_ozon_fbo_posting" => "OZON FBO Документы продаж",
                "a014_ozon_transactions" => "Транзакции OZON",
                "p902_ozon_finance_realization" => "Финансовые данные реализации OZON",
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
    ) -> Option<contracts::usecases::u502_import_from_ozon::progress::ImportProgress> {
        self.progress_tracker.get_progress(session_id)
    }

    async fn resolve_organization_id(
        &self,
        connection: &contracts::domain::a006_connection_mp::aggregate::ConnectionMP,
        session_id: &str,
        aggregate_index: &str,
    ) -> Result<String> {
        let organization_uuid = Uuid::parse_str(&connection.organization_ref).map_err(|_| {
            let error_msg = format!(
                "Некорректный organization_ref UUID в подключении: '{}'",
                connection.organization_ref
            );
            self.progress_tracker.add_error(
                session_id,
                Some(aggregate_index.to_string()),
                error_msg.clone(),
                None,
            );
            anyhow::anyhow!(error_msg)
        })?;

        match crate::domain::a002_organization::service::get_by_id(organization_uuid).await? {
            Some(org) => Ok(org.base.id.as_string()),
            None => {
                let error_msg = format!(
                    "Организация с UUID '{}' не найдена в справочнике",
                    connection.organization_ref
                );
                self.progress_tracker.add_error(
                    session_id,
                    Some(aggregate_index.to_string()),
                    error_msg.clone(),
                    None,
                );
                anyhow::bail!("{}", error_msg);
            }
        }
    }

    /// Выполнить импорт
    pub async fn execute_import(
        &self,
        session_id: &str,
        request: &ImportRequest,
        connection: &contracts::domain::a006_connection_mp::aggregate::ConnectionMP,
    ) -> Result<()> {
        tracing::info!("Starting OZON import for session: {}", session_id);

        if self.progress_tracker.get_progress(session_id).is_none() {
            self.progress_tracker.create_session(session_id.to_string());
            for aggregate_index in &request.target_aggregates {
                let aggregate_name = match aggregate_index.as_str() {
                    "a007_marketplace_product" => "Товары маркетплейса",
                    "a008_marketplace_sales" => "Продажи (фин. транзакции)",
                    "a009_ozon_returns" => "Возвраты OZON",
                    "a010_ozon_fbs_posting" => "OZON FBS Документы продаж",
                    "a011_ozon_fbo_posting" => "OZON FBO Документы продаж",
                    "a014_ozon_transactions" => "Транзакции OZON",
                    "p902_ozon_finance_realization" => "Финансовые данные реализации OZON",
                    _ => "Unknown",
                };
                self.progress_tracker.add_aggregate(
                    session_id,
                    aggregate_index.clone(),
                    aggregate_name.to_string(),
                );
            }
        }

        let work_result = self.run_aggregates(session_id, request, connection).await;

        let final_status = if let Err(ref e) = work_result {
            self.progress_tracker
                .add_error(session_id, None, e.to_string(), None);
            ImportStatus::Failed
        } else if self
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

        work_result
    }

    async fn run_aggregates(
        &self,
        session_id: &str,
        request: &ImportRequest,
        connection: &contracts::domain::a006_connection_mp::aggregate::ConnectionMP,
    ) -> Result<()> {
        for aggregate_index in &request.target_aggregates {
            match aggregate_index.as_str() {
                "a007_marketplace_product" => {
                    self.import_marketplace_products(session_id, connection)
                        .await?;
                }
                "a008_marketplace_sales" => {
                    self.import_marketplace_sales(
                        session_id,
                        connection,
                        request.date_from,
                        request.date_to,
                    )
                    .await?;
                }
                "a009_ozon_returns" => {
                    self.import_ozon_returns(
                        session_id,
                        connection,
                        request.date_from,
                        request.date_to,
                    )
                    .await?;
                }
                "a010_ozon_fbs_posting" => {
                    self.import_ozon_fbs_postings(
                        session_id,
                        connection,
                        request.date_from,
                        request.date_to,
                    )
                    .await?;
                }
                "a011_ozon_fbo_posting" => {
                    self.import_ozon_fbo_postings(
                        session_id,
                        connection,
                        request.date_from,
                        request.date_to,
                    )
                    .await?;
                }
                "a014_ozon_transactions" => {
                    self.import_ozon_transactions(
                        session_id,
                        connection,
                        request.date_from,
                        request.date_to,
                    )
                    .await?;
                }
                "p902_ozon_finance_realization" => {
                    self.import_ozon_finance_realization(
                        session_id,
                        connection,
                        request.date_from,
                        request.date_to,
                    )
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
        Ok(())
    }

    /// Импорт товаров из OZON
    async fn import_marketplace_products(
        &self,
        session_id: &str,
        connection: &contracts::domain::a006_connection_mp::aggregate::ConnectionMP,
    ) -> Result<()> {
        tracing::info!("Importing marketplace products for session: {}", session_id);

        let aggregate_index = "a007_marketplace_product";
        let page_size = 100;
        let mut total_processed = 0;
        let mut total_inserted = 0;
        let mut total_updated = 0;
        let mut last_id: Option<String> = None;

        // Получаем товары страницами через /v3/product/list
        loop {
            let list_response = self
                .api_client
                .fetch_product_list(connection, page_size, last_id.clone())
                .await?;

            let items = list_response.result.items;
            if items.is_empty() {
                break;
            }

            let batch_size = items.len();
            tracing::info!(
                "Processing batch: {} items, total so far: {}",
                batch_size,
                total_processed
            );

            // Группируем product_id для batch запроса к /v3/product/info
            let product_ids: Vec<i64> = items.iter().map(|item| item.product_id).collect();

            // Получаем детальную информацию
            let info_response = self
                .api_client
                .fetch_product_info(connection, product_ids)
                .await?;

            // Обрабатываем каждый товар
            for product_info in info_response.items {
                let display_name = format!("{} - {}", product_info.offer_id, product_info.name);

                self.progress_tracker.set_current_item(
                    session_id,
                    aggregate_index,
                    Some(display_name),
                );

                match product::process_product(connection, &product_info).await {
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
                            product_info.offer_id,
                            e
                        );
                        self.progress_tracker.add_error(
                            session_id,
                            Some(aggregate_index.to_string()),
                            format!("Failed to process product {}", product_info.offer_id),
                            Some(e.to_string()),
                        );
                    }
                }

                // Обновить прогресс
                self.progress_tracker.update_aggregate(
                    session_id,
                    aggregate_index,
                    total_processed,
                    Some(list_response.result.total),
                    total_inserted,
                    total_updated,
                );
            }

            // Очистить текущий элемент после страницы
            self.progress_tracker
                .set_current_item(session_id, aggregate_index, None);

            // Обновляем last_id для следующей страницы
            let old_last_id = last_id.clone();
            last_id = Some(list_response.result.last_id.clone());

            // Защита от зацикливания: если last_id не изменился, прекращаем
            if old_last_id.is_some() && old_last_id == last_id {
                tracing::warn!(
                    "last_id did not change, stopping to prevent infinite loop. last_id: {:?}",
                    last_id
                );
                break;
            }

            // Если получили меньше page_size, значит это последняя страница
            if batch_size < page_size as usize {
                break;
            }
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

    /// Импорт финансовых транзакций (продажи/возвраты) в a008
    async fn import_marketplace_sales(
        &self,
        session_id: &str,
        connection: &contracts::domain::a006_connection_mp::aggregate::ConnectionMP,
        date_from: chrono::NaiveDate,
        date_to: chrono::NaiveDate,
    ) -> Result<()> {
        let aggregate_index = "a008_marketplace_sales";
        let mut total_processed = 0;
        let mut total_inserted = 0;
        let mut total_updated = 0;

        let organization_id = self
            .resolve_organization_id(connection, session_id, aggregate_index)
            .await?;

        // Кеш сопоставлений SKU/offer_id -> product_id
        use std::collections::HashMap;
        let mut sku_to_product_id: HashMap<String, String> = HashMap::new();

        // Пагинация страницами
        let page_size = 1000;
        let mut page = 1;

        loop {
            let resp = self
                .api_client
                .fetch_finance_transactions(connection, date_from, date_to, page, page_size)
                .await?;

            if resp.result.operations.is_empty() {
                break;
            }

            for op in resp.result.operations {
                self.progress_tracker.set_current_item(
                    session_id,
                    aggregate_index,
                    Some(op.operation_type.clone()),
                );

                // Берем позиции из items (могут быть несколько на одну операцию)
                let items = if op.items.is_empty() {
                    vec![super::ozon_api_client::OzonFinanceItem {
                        sku: None,
                        offer_id: None,
                        quantity: None,
                    }]
                } else {
                    op.items.clone()
                };

                // Вычисляем общее количество для распределения суммы
                let total_qty: i32 = items.iter().map(|item| item.quantity.unwrap_or(1)).sum();

                // Если все quantity отсутствуют, используем количество позиций
                let items_count = items.len() as i32;
                let divisor = if total_qty > 0 {
                    total_qty
                } else {
                    items_count
                };

                for item in items {
                    let key = item
                        .sku
                        .map(|v| v.to_string())
                        .or(item.offer_id.clone())
                        .unwrap_or_default();
                    if key.is_empty() {
                        self.progress_tracker.add_error(
                            session_id,
                            Some(aggregate_index.to_string()),
                            "Операция без sku/offer_id".to_string(),
                            None,
                        );
                        continue;
                    }

                    // Дата начисления = дата операции (дата часть); API вернул "YYYY-MM-DD HH:MM:SS"
                    let accrual_date = chrono::NaiveDateTime::parse_from_str(
                        &op.operation_date,
                        "%Y-%m-%d %H:%M:%S",
                    )
                    .map(|dt| dt.date())
                    .unwrap_or_else(|_| {
                        chrono::DateTime::parse_from_rfc3339(&op.operation_date)
                            .map(|dt| dt.naive_utc().date())
                            .unwrap_or(date_from)
                    });

                    // Количество: используем из API или 1 по умолчанию
                    let qty = item.quantity.unwrap_or(1);

                    // Распределяем сумму операции пропорционально количеству
                    let revenue = if divisor > 0 {
                        (op.amount * qty as f64) / divisor as f64
                    } else {
                        op.amount
                    };

                    match sales::process_sale_item(
                        connection,
                        &organization_id,
                        &mut sku_to_product_id,
                        accrual_date,
                        &op.operation_type,
                        &key,
                        qty,
                        revenue,
                    )
                    .await
                    {
                        Ok(true) => total_inserted += 1,
                        Ok(false) => total_updated += 1,
                        Err(e) => {
                            tracing::error!("Failed to process sale item {}: {}", key, e);
                            self.progress_tracker.add_error(
                                session_id,
                                Some(aggregate_index.to_string()),
                                format!("Failed to process sale item {}", key),
                                Some(e.to_string()),
                            );
                        }
                    }
                }

                total_processed += 1;
                self.progress_tracker.update_aggregate(
                    session_id,
                    aggregate_index,
                    total_processed,
                    None,
                    total_inserted,
                    total_updated,
                );
            }

            // Пагинация: если API вернул has_next, используем его, иначе вычисляем по page/total при наличии
            if let Some(has_next) = resp.result.has_next {
                if !has_next {
                    break;
                }
                page += 1;
            } else if let (Some(p), Some(ps), Some(t)) =
                (resp.result.page, resp.result.page_size, resp.result.total)
            {
                if (p as usize * ps as usize) >= t as usize {
                    break;
                }
                page += 1;
            } else {
                // если нет явных полей пагинации, выходим когда получили пустой список (выше) или продолжаем одну страницу
                break;
            }
            self.progress_tracker
                .set_current_item(session_id, aggregate_index, None);
        }

        self.progress_tracker
            .complete_aggregate(session_id, aggregate_index);
        Ok(())
    }

    /// Импорт возвратов из OZON в a009
    async fn import_ozon_returns(
        &self,
        session_id: &str,
        connection: &contracts::domain::a006_connection_mp::aggregate::ConnectionMP,
        date_from: chrono::NaiveDate,
        date_to: chrono::NaiveDate,
    ) -> Result<()> {
        let aggregate_index = "a009_ozon_returns";
        let mut total_processed = 0;
        let mut total_inserted = 0;
        let mut total_updated = 0;

        let organization_id = self
            .resolve_organization_id(connection, session_id, aggregate_index)
            .await?;

        // Курсорная пагинация через last_id
        let mut last_id: i64 = 0;
        let limit = 500; // Максимум для OZON API

        loop {
            let resp = self
                .api_client
                .fetch_returns_list(connection, date_from, date_to, last_id, limit)
                .await?;

            if resp.returns.is_empty() {
                break;
            }

            let returns_count = resp.returns.len();
            tracing::info!(
                "Received {} returns from API (last_id={})",
                returns_count,
                last_id
            );

            // Сохраняем last_id для следующего запроса перед перемещением вектора
            let new_last_id = resp.returns.last().map(|r| r.id).unwrap_or(last_id);

            for return_item in resp.returns {
                let return_id_str = return_item.id.to_string();
                let return_reason = return_item
                    .return_reason_name
                    .as_deref()
                    .unwrap_or("Unknown");
                let return_type = return_item.return_type.as_deref().unwrap_or("Unknown");
                let order_id_str = return_item
                    .order_id
                    .map(|id| id.to_string())
                    .unwrap_or_default();
                let order_number = return_item.order_number.as_deref().unwrap_or("");
                let posting_number = return_item.posting_number.as_deref().unwrap_or("");
                let clearing_id_str = return_item.clearing_id.map(|id| id.to_string());
                let return_clearing_id_str =
                    return_item.return_clearing_id.map(|id| id.to_string());

                // Парсим дату возврата из logistic.return_date (ISO 8601 формат: "2022-01-19T19:55:35.433Z")
                let return_date = return_item
                    .logistic
                    .as_ref()
                    .and_then(|l| l.return_date.as_ref())
                    .and_then(|moment_str| {
                        // Пытаемся распарсить как ISO datetime
                        chrono::DateTime::parse_from_rfc3339(moment_str)
                            .map(|dt| dt.naive_utc().date())
                            .or_else(|_| {
                                // Fallback: пытаемся распарсить как дату YYYY-MM-DD
                                chrono::NaiveDate::parse_from_str(moment_str, "%Y-%m-%d")
                            })
                            .ok()
                    })
                    .unwrap_or(date_from); // Если не удалось распарсить, используем дату из периода запроса

                // Проверяем наличие товара: если product == None или все поля внутри пустые
                let has_product = return_item
                    .product
                    .as_ref()
                    .and_then(|p| {
                        // Если хотя бы одно поле заполнено, считаем что товар есть
                        if p.sku.is_some() || p.name.is_some() || p.offer_id.is_some() {
                            Some(())
                        } else {
                            None
                        }
                    })
                    .is_some();

                tracing::debug!(
                    "Return {} has_product={}, product: {:?}",
                    return_id_str,
                    has_product,
                    return_item.product
                );

                // Если нет товара в возврате, создаем одну запись без товара
                if !has_product {
                    let display_name = format!("Возврат {} без товаров", return_id_str);
                    self.progress_tracker.set_current_item(
                        session_id,
                        aggregate_index,
                        Some(display_name.clone()),
                    );

                    match returns::process_return_item(
                        connection,
                        &organization_id,
                        &return_id_str,
                        return_date,
                        return_reason,
                        return_type,
                        &order_id_str,
                        order_number,
                        posting_number,
                        &clearing_id_str,
                        &return_clearing_id_str,
                        "",
                        "",
                        0.0,
                        0,
                        &display_name,
                    )
                    .await
                    {
                        Ok(true) => total_inserted += 1,
                        Ok(false) => total_updated += 1,
                        Err(e) => {
                            tracing::error!(
                                "Failed to process return item {}: {}",
                                return_id_str,
                                e
                            );
                            self.progress_tracker.add_error(
                                session_id,
                                Some(aggregate_index.to_string()),
                                format!("Failed to process return item {}", return_id_str),
                                Some(e.to_string()),
                            );
                        }
                    }

                    total_processed += 1;
                    self.progress_tracker.update_aggregate(
                        session_id,
                        aggregate_index,
                        total_processed,
                        None,
                        total_inserted,
                        total_updated,
                    );
                } else {
                    // Обрабатываем товар в возврате
                    let product = return_item.product.as_ref().unwrap(); // уже проверили что has_product
                    let sku_str = product.sku.map(|s| s.to_string()).unwrap_or_default();
                    let product_name = product.name.as_deref().unwrap_or("Unknown");
                    let price = product.price.as_ref().and_then(|p| p.price).unwrap_or(0.0);
                    let quantity = product.quantity.unwrap_or(0);

                    let display_name =
                        format!("{} - {} - {}", return_id_str, sku_str, product_name);
                    self.progress_tracker.set_current_item(
                        session_id,
                        aggregate_index,
                        Some(display_name.clone()),
                    );

                    match returns::process_return_item(
                        connection,
                        &organization_id,
                        &return_id_str,
                        return_date,
                        return_reason,
                        return_type,
                        &order_id_str,
                        order_number,
                        posting_number,
                        &clearing_id_str,
                        &return_clearing_id_str,
                        &sku_str,
                        product_name,
                        price,
                        quantity,
                        &display_name,
                    )
                    .await
                    {
                        Ok(true) => total_inserted += 1,
                        Ok(false) => total_updated += 1,
                        Err(e) => {
                            tracing::error!("Failed to process return product {}: {}", sku_str, e);
                            self.progress_tracker.add_error(
                                session_id,
                                Some(aggregate_index.to_string()),
                                format!("Failed to process return product {}", sku_str),
                                Some(e.to_string()),
                            );
                        }
                    }

                    total_processed += 1;
                    self.progress_tracker.update_aggregate(
                        session_id,
                        aggregate_index,
                        total_processed,
                        None,
                        total_inserted,
                        total_updated,
                    );
                }
            }

            // Если получили меньше limit, значит это последняя страница
            if returns_count < limit as usize {
                break;
            }

            // Защита от зацикливания: если last_id не изменился
            if returns_count > 0 && last_id == new_last_id {
                tracing::warn!("last_id did not change, stopping pagination");
                break;
            }

            last_id = new_last_id;
        }

        self.progress_tracker
            .complete_aggregate(session_id, aggregate_index);
        tracing::info!(
            "OZON returns import completed: processed={}, inserted={}, updated={}",
            total_processed,
            total_inserted,
            total_updated
        );
        Ok(())
    }

    /// Импорт OZON FBS Posting в a010
    async fn import_ozon_fbs_postings(
        &self,
        session_id: &str,
        connection: &contracts::domain::a006_connection_mp::aggregate::ConnectionMP,
        date_from: chrono::NaiveDate,
        date_to: chrono::NaiveDate,
    ) -> Result<()> {
        let aggregate_index = "a010_ozon_fbs_posting";
        let mut total_processed = 0;
        let mut total_inserted = 0;
        let mut total_updated = 0;

        let organization_id = self
            .resolve_organization_id(connection, session_id, aggregate_index)
            .await?;

        // Пагинация
        let limit = 100;
        let mut offset = 0;

        loop {
            let resp = self
                .api_client
                .fetch_fbs_postings(connection, date_from, date_to, limit, offset)
                .await?;

            if resp.result.postings.is_empty() {
                break;
            }

            let postings_count = resp.result.postings.len();
            tracing::info!(
                "Received {} FBS postings from API (offset={})",
                postings_count,
                offset
            );

            for posting in resp.result.postings {
                let posting_number = posting.posting_number.clone();
                self.progress_tracker.set_current_item(
                    session_id,
                    aggregate_index,
                    Some(format!("FBS Posting {}", posting_number)),
                );

                match postings::process_fbs_posting(connection, &organization_id, &posting).await {
                    Ok(is_new) => {
                        total_processed += 1;
                        if is_new {
                            total_inserted += 1;
                        } else {
                            total_updated += 1;
                        }
                    }
                    Err(e) => {
                        tracing::error!("Failed to process FBS posting {}: {}", posting_number, e);
                        self.progress_tracker.add_error(
                            session_id,
                            Some(aggregate_index.to_string()),
                            format!("Failed to process FBS posting {}", posting_number),
                            Some(e.to_string()),
                        );
                    }
                }

                self.progress_tracker.update_aggregate(
                    session_id,
                    aggregate_index,
                    total_processed,
                    None,
                    total_inserted,
                    total_updated,
                );
            }

            // Проверяем, есть ли еще данные
            if !resp.result.has_next {
                break;
            }

            offset += limit;
            self.progress_tracker
                .set_current_item(session_id, aggregate_index, None);
        }

        self.progress_tracker
            .complete_aggregate(session_id, aggregate_index);
        tracing::info!(
            "OZON FBS postings import completed: processed={}, inserted={}, updated={}",
            total_processed,
            total_inserted,
            total_updated
        );
        Ok(())
    }

    /// Импорт OZON FBO Posting в a011
    async fn import_ozon_fbo_postings(
        &self,
        session_id: &str,
        connection: &contracts::domain::a006_connection_mp::aggregate::ConnectionMP,
        date_from: chrono::NaiveDate,
        date_to: chrono::NaiveDate,
    ) -> Result<()> {
        let aggregate_index = "a011_ozon_fbo_posting";
        let mut total_processed = 0;
        let mut total_inserted = 0;
        let mut total_updated = 0;

        let organization_id = self
            .resolve_organization_id(connection, session_id, aggregate_index)
            .await?;

        // Пагинация
        let limit = 100;
        let mut offset = 0;

        loop {
            let resp = self
                .api_client
                .fetch_fbo_postings(connection, date_from, date_to, limit, offset)
                .await?;

            let postings_count = resp.result.len();
            if postings_count == 0 {
                break;
            }

            tracing::info!(
                "Received {} FBO postings from API (offset={})",
                postings_count,
                offset
            );

            for posting in resp.result {
                let posting_number = posting.posting_number.clone();
                self.progress_tracker.set_current_item(
                    session_id,
                    aggregate_index,
                    Some(format!("FBO Posting {}", posting_number)),
                );

                match postings::process_fbo_posting(connection, &organization_id, &posting).await {
                    Ok(is_new) => {
                        total_processed += 1;
                        if is_new {
                            total_inserted += 1;
                        } else {
                            total_updated += 1;
                        }
                    }
                    Err(e) => {
                        tracing::error!("Failed to process FBO posting {}: {}", posting_number, e);
                        self.progress_tracker.add_error(
                            session_id,
                            Some(aggregate_index.to_string()),
                            format!("Failed to process FBO posting {}", posting_number),
                            Some(e.to_string()),
                        );
                    }
                }

                self.progress_tracker.update_aggregate(
                    session_id,
                    aggregate_index,
                    total_processed,
                    None,
                    total_inserted,
                    total_updated,
                );
            }

            // FBO API не имеет has_next, проверяем по количеству записей
            if postings_count < limit as usize {
                break;
            }

            offset += limit;
            self.progress_tracker
                .set_current_item(session_id, aggregate_index, None);
        }

        self.progress_tracker
            .complete_aggregate(session_id, aggregate_index);
        tracing::info!(
            "OZON FBO postings import completed: processed={}, inserted={}, updated={}",
            total_processed,
            total_inserted,
            total_updated
        );
        Ok(())
    }

    /// Импорт финансовых данных реализации OZON в p902
    pub async fn import_ozon_finance_realization(
        &self,
        session_id: &str,
        connection: &contracts::domain::a006_connection_mp::aggregate::ConnectionMP,
        date_from: chrono::NaiveDate,
        date_to: chrono::NaiveDate,
    ) -> Result<()> {
        let aggregate_index = "p902_ozon_finance_realization";
        let mut total_processed = 0;
        let mut total_inserted = 0;
        let mut total_updated = 0;

        let organization_id = self
            .resolve_organization_id(connection, session_id, aggregate_index)
            .await?;

        // Finance Realization API НЕ ПОДДЕРЖИВАЕТ ПАГИНАЦИЮ - возвращает все данные за месяц сразу
        // Генерируем уникальный registrator_ref для всей сессии импорта
        let registrator_ref = Uuid::new_v4().to_string();

        let resp = self
            .api_client
            .fetch_finance_realization(connection, date_from, date_to, 10000, 0)
            .await?;

        if !resp.rows.is_empty() {
            use chrono::Datelike;
            let rows_count = resp.rows.len();
            tracing::info!(
                "Received {} finance realization rows from API for month {}-{}",
                rows_count,
                date_from.year(),
                date_from.month()
            );

            let currency_code = resp.header.currency_sys_name.clone();
            let doc_date = resp.header.doc_date.clone();

            // Парсим дату документа как accrual_date
            let accrual_date =
                chrono::NaiveDate::parse_from_str(&doc_date, "%Y-%m-%d").unwrap_or(date_from);

            for row in resp.rows {
                match realization::process_realization_row(
                    connection,
                    &organization_id,
                    &registrator_ref,
                    &row,
                    &currency_code,
                    accrual_date,
                )
                .await
                {
                    Ok((ins, upd)) => {
                        total_inserted += ins;
                        total_updated += upd;
                        total_processed += 1;
                    }
                    Err(e) => {
                        tracing::error!("Failed to process realization row: {}", e);
                        self.progress_tracker.add_error(
                            session_id,
                            Some(aggregate_index.to_string()),
                            "Failed to process realization row".to_string(),
                            Some(e.to_string()),
                        );
                    }
                }

                self.progress_tracker.update_aggregate(
                    session_id,
                    aggregate_index,
                    total_processed,
                    None,
                    total_inserted,
                    total_updated,
                );
            }
        }

        self.progress_tracker
            .complete_aggregate(session_id, aggregate_index);
        tracing::info!(
            "OZON finance realization import completed: processed={}, inserted={}, updated={}",
            total_processed,
            total_inserted,
            total_updated
        );
        Ok(())
    }

    /// Импорт транзакций OZON через /v3/finance/transaction/list
    async fn import_ozon_transactions(
        &self,
        session_id: &str,
        connection: &contracts::domain::a006_connection_mp::aggregate::ConnectionMP,
        date_from: chrono::NaiveDate,
        date_to: chrono::NaiveDate,
    ) -> Result<()> {
        let aggregate_index = "a014_ozon_transactions";
        let mut total_processed = 0;
        let mut total_inserted = 0;
        let mut total_updated = 0;

        let organization_id = self
            .resolve_organization_id(connection, session_id, aggregate_index)
            .await?;

        // Пагинация по страницам
        let page_size = 1000; // Максимум для OZON API
        let mut current_page = 1;

        loop {
            let resp = self
                .api_client
                .fetch_transactions_list(connection, date_from, date_to, current_page, page_size)
                .await?;

            let operations_count = resp.result.operations.len();
            if operations_count == 0 {
                break;
            }

            tracing::info!(
                "Received {} transactions from API (page {}/{})",
                operations_count,
                current_page,
                resp.result.page_count
            );

            for operation in resp.result.operations {
                match transaction::process_transaction(connection, &organization_id, &operation)
                    .await
                {
                    Ok(is_new) => {
                        if is_new {
                            total_inserted += 1;
                        } else {
                            total_updated += 1;
                        }
                    }
                    Err(e) => {
                        tracing::error!("Failed to process transaction: {}", e);
                        self.progress_tracker.add_error(
                            session_id,
                            Some(aggregate_index.to_string()),
                            "Failed to process transaction".to_string(),
                            Some(e.to_string()),
                        );
                    }
                }

                total_processed += 1;

                // Обновляем прогресс
                self.progress_tracker.update_aggregate(
                    session_id,
                    aggregate_index,
                    total_processed,
                    Some(resp.result.row_count),
                    total_inserted,
                    total_updated,
                );
            }

            // Проверяем, есть ли еще страницы
            if current_page >= resp.result.page_count {
                break;
            }

            current_page += 1;
        }

        tracing::info!(
            "OZON Transactions import completed: processed={}, inserted={}, updated={}",
            total_processed,
            total_inserted,
            total_updated
        );

        self.progress_tracker
            .complete_aggregate(session_id, aggregate_index);

        Ok(())
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
