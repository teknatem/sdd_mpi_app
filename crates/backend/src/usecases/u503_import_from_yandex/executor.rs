use super::{
    processors::{order, payment_report, product, realization, returns, shows_sales},
    progress_tracker::ProgressTracker,
    yandex_api_client::{parse_shows_sales_report, OrderDateField, YandexApiClient},
};
use anyhow::Result;
use contracts::domain::common::AggregateId;
use contracts::system::tasks::progress::TaskProgress;
use contracts::usecases::u503_import_from_yandex::{
    progress::ImportStatus,
    request::ImportRequest,
    response::{ImportResponse, ImportStartStatus},
};
use std::sync::Arc;
use uuid::Uuid;

/// Executor для UseCase импорта из Yandex Market
pub struct ImportExecutor {
    api_client: Arc<YandexApiClient>,
    pub progress_tracker: Arc<ProgressTracker>,
}

impl ImportExecutor {
    pub fn new(progress_tracker: Arc<ProgressTracker>) -> Self {
        Self {
            api_client: Arc::new(YandexApiClient::new()),
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
                "a013_ym_order" => "Заказы Yandex Market",
                "a016_ym_returns" => "Возвраты Yandex Market",
                "p907_ym_payment_report" => "Отчёт по платежам YM",
                "a034_ym_realization" => "Отчёт о реализации YM",
                "a041_ym_shows_sales_daily" => "Воронка продаж YM (Аналитика продаж)",
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
    ) -> Option<contracts::usecases::u503_import_from_yandex::progress::ImportProgress> {
        self.progress_tracker.get_progress(session_id)
    }

    /// Выполнить импорт
    pub async fn execute_import(
        &self,
        session_id: &str,
        request: &ImportRequest,
        connection: &contracts::domain::a006_connection_mp::aggregate::ConnectionMP,
    ) -> Result<()> {
        tracing::info!("Starting Yandex Market import for session: {}", session_id);

        if self.progress_tracker.get_progress(session_id).is_none() {
            self.progress_tracker.create_session(session_id.to_string());
            for aggregate_index in &request.target_aggregates {
                let aggregate_name = match aggregate_index.as_str() {
                    "a007_marketplace_product" => "Товары маркетплейса",
                    "a013_ym_order" => "Заказы Yandex Market",
                    "a016_ym_returns" => "Возвраты Yandex Market",
                    "p907_ym_payment_report" => "Отчёт по платежам YM",
                    "a034_ym_realization" => "Отчёт о реализации YM",
                    "a041_ym_shows_sales_daily" => "Воронка продаж YM (Аналитика продаж)",
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

        let final_status = if work_result.is_err() {
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
            let result = match aggregate_index.as_str() {
                "a007_marketplace_product" => {
                    self.import_marketplace_products(session_id, connection)
                        .await
                }
                "a013_ym_order" => {
                    let date_field = if request.incremental_by_update {
                        OrderDateField::Updated
                    } else {
                        OrderDateField::Creation
                    };
                    self.import_ym_orders(
                        session_id,
                        connection,
                        request.date_from,
                        request.date_to,
                        date_field,
                    )
                    .await
                }
                "a016_ym_returns" => {
                    self.import_ym_returns(
                        session_id,
                        connection,
                        request.date_from,
                        request.date_to,
                    )
                    .await
                }
                "p907_ym_payment_report" => {
                    self.import_ym_payment_report(
                        session_id,
                        connection,
                        request.date_from,
                        request.date_to,
                    )
                    .await
                }
                "a034_ym_realization" => {
                    self.import_realization(
                        session_id,
                        connection,
                        request.date_from,
                        request.date_to,
                    )
                    .await
                }
                "a041_ym_shows_sales_daily" => {
                    self.import_ym_shows_sales(
                        session_id,
                        connection,
                        request.date_from,
                        request.date_to,
                    )
                    .await
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
                    Ok(())
                }
            };

            if let Err(error) = result {
                self.progress_tracker.fail_aggregate(
                    session_id,
                    aggregate_index,
                    error.to_string(),
                );
                return Err(error);
            }
        }
        Ok(())
    }

    /// Импорт товаров из Yandex Market
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
        let mut total_barcodes_imported = 0;
        let mut page_token: Option<String> = None;
        let mut expected_total: Option<i32> = None;

        // Получаем товары страницами через /campaigns/{campaignId}/offer-mappings
        loop {
            let list_response = self
                .api_client
                .fetch_product_list(connection, page_size, page_token.clone())
                .await?;

            // Если API вернул total, сохраняем его (только при первом запросе)
            if expected_total.is_none() {
                expected_total = Some(list_response.result.paging.total.unwrap_or(0) as i32);
            }

            let offers = list_response.result.offer_mapping_entries;
            let next_page_token = list_response.result.paging.next_page_token;

            if offers.is_empty() {
                break;
            }

            let batch_size = offers.len();
            tracing::info!(
                "Processing batch: {} items, total so far: {}",
                batch_size,
                total_processed
            );

            // Обрабатываем каждый товар
            for offer_mapping in offers {
                let offer = &offer_mapping.offer;
                let mapping = &offer_mapping.mapping;

                let product_name = offer
                    .name
                    .clone()
                    .unwrap_or_else(|| "Без названия".to_string());
                let display_name = format!("{} - {}", offer.offer_id, product_name);

                self.progress_tracker.set_current_item(
                    session_id,
                    aggregate_index,
                    Some(display_name),
                );

                match product::process_product_from_offer(connection, offer, mapping).await {
                    Ok((is_new, barcodes_count)) => {
                        total_processed += 1;
                        if is_new {
                            total_inserted += 1;
                        } else {
                            total_updated += 1;
                        }
                        total_barcodes_imported += barcodes_count;
                    }
                    Err(e) => {
                        tracing::error!("Failed to process product {}: {}", offer.offer_id, e);
                        self.progress_tracker.add_error(
                            session_id,
                            Some(aggregate_index.to_string()),
                            format!("Failed to process product {}", offer.offer_id),
                            Some(e.to_string()),
                        );
                    }
                }

                // Обновить прогресс
                self.progress_tracker.update_aggregate(
                    session_id,
                    aggregate_index,
                    total_processed,
                    expected_total,
                    total_inserted,
                    total_updated,
                );

                // Обновить счетчик штрихкодов
                self.progress_tracker.update_barcodes_count(
                    session_id,
                    aggregate_index,
                    total_barcodes_imported as i32,
                );
            }

            // Очистить текущий элемент после страницы
            self.progress_tracker
                .set_current_item(session_id, aggregate_index, None);

            // Обновляем page_token для следующей страницы
            let old_token = page_token.clone();
            page_token = next_page_token;

            // Если нет next_page_token, значит это последняя страница
            if page_token.is_none() {
                break;
            }

            // Защита от зацикливания: если токен не изменился, прекращаем
            if old_token.is_some() && old_token == page_token {
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

    /// Определить список магазинов (campaign_id + placement_type) для campaign-уровневых
    /// обменов YM (заказы, возвраты). Модель «подключение = бизнес»: если задан
    /// `business_account_id`, перечисляем все кампании бизнеса через `GET /campaigns`
    /// и отбираем принадлежащие этому бизнесу. Если бизнес не задан или вызов не удался —
    /// откатываемся на единственный `supplier_id` подключения (placement_type неизвестен).
    ///
    /// `aggregate_index` — для отнесения возможной ошибки к нужному агрегату в прогрессе.
    async fn resolve_campaigns(
        &self,
        session_id: &str,
        aggregate_index: &str,
        connection: &contracts::domain::a006_connection_mp::aggregate::ConnectionMP,
    ) -> Vec<(String, Option<String>)> {
        let fallback: Vec<(String, Option<String>)> = connection
            .supplier_id
            .clone()
            .into_iter()
            .map(|id| (id, None))
            .collect();

        // Бизнес не задан → подключение покрывает один магазин (legacy).
        let biz = match connection
            .business_account_id
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            Some(s) => s.to_string(),
            None => return fallback,
        };
        let biz_id = biz.parse::<i64>().ok();

        match self.api_client.fetch_campaigns(connection).await {
            Ok(list) => {
                let campaigns: Vec<(String, Option<String>)> = list
                    .iter()
                    .filter(|c| match (biz_id, c.business.as_ref()) {
                        // Если бизнес задан числом — берём только его магазины; иначе все.
                        (Some(b), Some(cb)) => cb.id == b,
                        _ => true,
                    })
                    .map(|c| (c.id.to_string(), c.placement_type.clone()))
                    .collect();

                if campaigns.is_empty() {
                    tracing::warn!(
                        "YM: GET /campaigns вернул 0 подходящих магазинов; fallback на supplier_id"
                    );
                    fallback
                } else {
                    tracing::info!("YM: обмен по {} магазину(ам) бизнеса", campaigns.len());
                    campaigns
                }
            }
            Err(e) => {
                tracing::error!("YM: не удалось получить список магазинов: {}", e);
                self.progress_tracker.add_error(
                    session_id,
                    Some(aggregate_index.to_string()),
                    format!(
                        "Не удалось получить список магазинов (GET /campaigns): {}",
                        e
                    ),
                    None,
                );
                fallback
            }
        }
    }

    /// Импорт заказов Yandex Market
    async fn import_ym_orders(
        &self,
        session_id: &str,
        connection: &contracts::domain::a006_connection_mp::aggregate::ConnectionMP,
        date_from: chrono::NaiveDate,
        date_to: chrono::NaiveDate,
        date_field: OrderDateField,
    ) -> Result<()> {
        use crate::domain::a002_organization;

        tracing::info!("Importing Yandex Market orders for session: {}", session_id);

        let aggregate_index = "a013_ym_order";
        let mut total_processed = 0;
        let mut total_inserted = 0;
        let mut total_updated = 0;

        // 1. Resolve organization by UUID reference from connection
        let organization_id = match Uuid::parse_str(&connection.organization_ref) {
            Ok(org_uuid) => match a002_organization::service::get_by_id(
                crate::shared::data::db::get_connection(),
                org_uuid,
            )
            .await?
            {
                Some(org) => org.base.id.as_string(),
                None => {
                    let msg = format!(
                        "Organization UUID '{}' not found",
                        connection.organization_ref
                    );
                    tracing::error!("{}", msg);
                    anyhow::bail!("{}", msg);
                }
            },
            Err(_) => {
                let msg = format!(
                    "Invalid organization_ref UUID in connection: '{}'",
                    connection.organization_ref
                );
                tracing::error!("{}", msg);
                anyhow::bail!("{}", msg);
            }
        };

        // 2. Determine which stores (campaigns) to import (fan-out по бизнесу).
        let campaigns = self
            .resolve_campaigns(session_id, aggregate_index, connection)
            .await;
        if campaigns.is_empty() {
            anyhow::bail!(
                "Не задан магазин: у подключения нет supplier_id и GET /campaigns не вернул кампаний"
            );
        }

        // 3. Import each store with a per-campaign connection clone so that both the
        //    orders fetch and process_order (campaign_id in header) use the right id.
        //    placement_type кампании пишем в заказ как fulfillment_type.
        for (campaign_id, placement_type) in &campaigns {
            let mut conn = connection.clone();
            conn.supplier_id = Some(campaign_id.clone());

            let orders = self
                .api_client
                .fetch_orders(&conn, date_from, date_to, date_field)
                .await?;

            tracing::info!(
                "Received {} orders from YM API (campaign {}, placement {:?})",
                orders.len(),
                campaign_id,
                placement_type
            );

            for order_item in orders {
                let order_id_str = order_item.id.to_string();
                self.progress_tracker.set_current_item(
                    session_id,
                    aggregate_index,
                    Some(format!("YM Order {}", order_id_str)),
                );

                // Fetch detailed order info to get realDeliveryDate
                let order_details = match self
                    .api_client
                    .fetch_order_details(&conn, order_item.id)
                    .await
                {
                    Ok(details) => details,
                    Err(e) => {
                        tracing::warn!(
                            "Failed to fetch details for order {}: {}, using basic data",
                            order_id_str,
                            e
                        );
                        order_item.clone() // Use original order if details fetch fails
                    }
                };

                match order::process_order(
                    &conn,
                    &organization_id,
                    &order_details,
                    placement_type.clone(),
                )
                .await
                {
                    Ok(is_new) => {
                        total_processed += 1;
                        if is_new {
                            total_inserted += 1;
                        } else {
                            total_updated += 1;
                        }
                    }
                    Err(e) => {
                        tracing::error!("Failed to process YM order {}: {}", order_id_str, e);
                        self.progress_tracker.add_error(
                            session_id,
                            Some(aggregate_index.to_string()),
                            format!("Failed to process order {}", order_id_str),
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
            "YM orders import completed: stores={}, processed={}, inserted={}, updated={}",
            campaigns.len(),
            total_processed,
            total_inserted,
            total_updated
        );

        Ok(())
    }

    /// Импорт возвратов Yandex Market
    async fn import_ym_returns(
        &self,
        session_id: &str,
        connection: &contracts::domain::a006_connection_mp::aggregate::ConnectionMP,
        date_from: chrono::NaiveDate,
        date_to: chrono::NaiveDate,
    ) -> Result<()> {
        use crate::domain::a002_organization;

        tracing::info!(
            "Importing Yandex Market returns for session: {}",
            session_id
        );

        let aggregate_index = "a016_ym_returns";
        let mut total_processed = 0;
        let mut total_inserted = 0;
        let mut total_updated = 0;

        // 1. Resolve organization by UUID reference from connection
        let organization_id = match Uuid::parse_str(&connection.organization_ref) {
            Ok(org_uuid) => match a002_organization::service::get_by_id(
                crate::shared::data::db::get_connection(),
                org_uuid,
            )
            .await?
            {
                Some(org) => org.base.id.as_string(),
                None => {
                    let msg = format!(
                        "Organization UUID '{}' not found",
                        connection.organization_ref
                    );
                    tracing::error!("{}", msg);
                    anyhow::bail!("{}", msg);
                }
            },
            Err(_) => {
                let msg = format!(
                    "Invalid organization_ref UUID in connection: '{}'",
                    connection.organization_ref
                );
                tracing::error!("{}", msg);
                anyhow::bail!("{}", msg);
            }
        };

        // 2. Determine which stores (campaigns) to import (fan-out по бизнесу).
        let campaigns = self
            .resolve_campaigns(session_id, aggregate_index, connection)
            .await;
        if campaigns.is_empty() {
            anyhow::bail!(
                "Не задан магазин: у подключения нет supplier_id и GET /campaigns не вернул кампаний"
            );
        }

        // 3. Fetch + process returns per store (per-campaign connection clone).
        for (campaign_id, _placement_type) in &campaigns {
            let mut conn = connection.clone();
            conn.supplier_id = Some(campaign_id.clone());

            let returns = self
                .api_client
                .fetch_returns(&conn, date_from, date_to)
                .await?;

            tracing::info!(
                "Received {} returns from YM API (campaign {})",
                returns.len(),
                campaign_id
            );

            for return_item in returns {
                let return_id_str = return_item.id.to_string();
                self.progress_tracker.set_current_item(
                    session_id,
                    aggregate_index,
                    Some(format!("YM Return {}", return_id_str)),
                );

                match returns::process_return(&conn, &organization_id, &return_item).await {
                    Ok(is_new) => {
                        total_processed += 1;
                        if is_new {
                            total_inserted += 1;
                        } else {
                            total_updated += 1;
                        }
                    }
                    Err(e) => {
                        tracing::error!("Failed to process YM return {}: {}", return_id_str, e);
                        self.progress_tracker.add_error(
                            session_id,
                            Some(aggregate_index.to_string()),
                            format!("Failed to process return {}", return_id_str),
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
            "YM returns import completed: stores={}, processed={}, inserted={}, updated={}",
            campaigns.len(),
            total_processed,
            total_inserted,
            total_updated
        );

        Ok(())
    }

    /// Импорт отчёта «Аналитика продаж» (shows-sales) → агрегат a041 → стадия
    /// marketing воронки p916. Трёхфазный: generate → poll → download.
    ///
    /// Отчёт содержит данные всех магазинов кабинета. Для API выбирается один campaignId
    /// бизнеса как контекст запроса; период заменяется целиком, поскольку YM пересчитывает
    /// статистику задним числом.
    async fn import_ym_shows_sales(
        &self,
        session_id: &str,
        connection: &contracts::domain::a006_connection_mp::aggregate::ConnectionMP,
        date_from: chrono::NaiveDate,
        date_to: chrono::NaiveDate,
    ) -> Result<()> {
        const MAX_POLL_ATTEMPTS: u32 = 60;
        const POLL_INTERVAL_SECS: u64 = 5;

        let aggregate_index = "a041_ym_shows_sales_daily";
        let date_from_str = date_from.format("%Y-%m-%d").to_string();
        let date_to_str = date_to.format("%Y-%m-%d").to_string();

        let campaigns = self
            .resolve_campaigns(session_id, aggregate_index, connection)
            .await;
        if campaigns.is_empty() {
            anyhow::bail!(
                "Не задан магазин: у подключения нет supplier_id и GET /campaigns не вернул кампаний"
            );
        }

        // «Аналитика продаж» формируется на уровне кабинета: разные campaignId
        // одного businessId возвращают одинаковые данные за завершённые дни.
        // Один campaignId нужен только как допустимый контекст API-запроса.
        let campaign_id = connection
            .supplier_id
            .as_ref()
            .filter(|configured| campaigns.iter().any(|(id, _)| id == *configured))
            .unwrap_or(&campaigns[0].0);
        let mut conn = connection.clone();
        conn.supplier_id = Some(campaign_id.clone());

        tracing::info!(
            "YM shows-sales: selected campaign={} as API context for business; available_campaigns={}",
            campaign_id,
            campaigns.len()
        );
        self.progress_tracker.set_current_item(
            session_id,
            aggregate_index,
            Some(format!("Генерация отчёта (кампания {})", campaign_id)),
        );

        let report_id = self
            .api_client
            .generate_shows_sales_report(&conn, date_from, date_to)
            .await?;

        let mut download_url: Option<String> = None;
        for attempt in 1..=MAX_POLL_ATTEMPTS {
            self.progress_tracker.set_current_item(
                session_id,
                aggregate_index,
                Some(format!(
                    "Ожидание отчёта (кампания {})... ({}/{})",
                    campaign_id, attempt, MAX_POLL_ATTEMPTS
                )),
            );
            tokio::time::sleep(tokio::time::Duration::from_secs(POLL_INTERVAL_SECS)).await;

            match self.api_client.poll_report_status(&conn, &report_id).await {
                Ok((status, file_url)) => match status.as_str() {
                    "DONE" => {
                        download_url = file_url;
                        break;
                    }
                    "FAILED" => {
                        anyhow::bail!(
                            "YM вернул FAILED при генерации отчёта «Аналитика продаж» (кампания {})",
                            campaign_id
                        );
                    }
                    _ => {}
                },
                // Сетевая ошибка поллинга не должна ронять импорт — повторяем.
                Err(e) => tracing::warn!("Shows-sales poll attempt {} failed: {}", attempt, e),
            }
        }

        let Some(url) = download_url else {
            anyhow::bail!(
                "Превышено время ожидания отчёта «Аналитика продаж» ({} попыток по {}с, кампания {})",
                MAX_POLL_ATTEMPTS,
                POLL_INTERVAL_SECS,
                campaign_id
            );
        };

        let body = self
            .api_client
            .download_report_text(&url, "ym_shows_sales")
            .await?;
        let rows = parse_shows_sales_report(&body)?;
        tracing::info!(
            "YM shows-sales report parsed: campaign={}, rows={}",
            campaign_id,
            rows.len()
        );

        let documents = shows_sales::build_documents(
            &conn,
            Some(campaign_id),
            &rows,
            &date_from_str,
            &date_to_str,
        )
        .await?;

        // Непустой API-отчёт не может бесследно превратиться в успешный импорт:
        // это означает несовместимый формат либо ошибочную фильтрацию.
        if !rows.is_empty() && documents.is_empty() {
            anyhow::bail!(
                "YM shows-sales: parsed {} rows, but built 0 documents for period {}..{}; existing data was not replaced",
                rows.len(),
                date_from_str,
                date_to_str
            );
        }

        let total_documents = crate::domain::a041_ym_shows_sales_daily::service::replace_for_period(
            &connection.base.id.as_string(),
            &date_from_str,
            &date_to_str,
            &documents,
        )
        .await? as i32;

        self.progress_tracker.update_aggregate(
            session_id,
            aggregate_index,
            total_documents,
            None,
            total_documents,
            0,
        );

        self.progress_tracker
            .complete_aggregate(session_id, aggregate_index);
        tracing::info!(
            "YM shows-sales import completed: campaign_context={}, available_campaigns={}, documents={}, period={}..{}",
            campaign_id,
            campaigns.len(),
            total_documents,
            date_from_str,
            date_to_str
        );
        Ok(())
    }

    /// Импорт отчёта по платежам Yandex Market (двухфазный процесс)
    ///
    /// Фаза 1: POST /v2/reports/united-netting/generate → получить reportId
    /// Фаза 2: GET /v2/reports/info/{reportId} → polling до DONE (макс. 60 попыток по 5с)
    /// Фаза 3: Скачать CSV и разобрать каждую строку в p907_ym_payment_report
    async fn import_ym_payment_report(
        &self,
        session_id: &str,
        connection: &contracts::domain::a006_connection_mp::aggregate::ConnectionMP,
        date_from: chrono::NaiveDate,
        date_to: chrono::NaiveDate,
    ) -> Result<()> {
        use crate::domain::a002_organization;

        tracing::info!("Importing YM payment report for session: {}", session_id);

        let aggregate_index = "p907_ym_payment_report";

        // Resolve organization
        let organization_id = match Uuid::parse_str(&connection.organization_ref) {
            Ok(org_uuid) => match a002_organization::service::get_by_id(
                crate::shared::data::db::get_connection(),
                org_uuid,
            )
            .await?
            {
                Some(org) => org.base.id.as_string(),
                None => {
                    let msg = format!(
                        "Organization UUID '{}' not found",
                        connection.organization_ref
                    );
                    tracing::error!("{}", msg);
                    anyhow::bail!("{}", msg);
                }
            },
            Err(_) => {
                let msg = format!(
                    "Invalid organization_ref UUID in connection: '{}'",
                    connection.organization_ref
                );
                tracing::error!("{}", msg);
                anyhow::bail!("{}", msg);
            }
        };

        // Phase 1: request report generation
        self.progress_tracker.set_current_item(
            session_id,
            aggregate_index,
            Some("Запрос генерации отчёта...".to_string()),
        );

        let report_id = self
            .api_client
            .generate_payment_report(connection, date_from, date_to)
            .await
            .map_err(|e| {
                self.progress_tracker.add_error(
                    session_id,
                    Some(aggregate_index.to_string()),
                    format!("Ошибка запроса генерации отчёта: {}", e),
                    None,
                );
                e
            })?;

        tracing::info!("Payment report requested, reportId={}", report_id);

        // Phase 2: poll until DONE (up to 60 attempts, 5s each = max 5 minutes)
        const MAX_POLL_ATTEMPTS: u32 = 60;
        const POLL_INTERVAL_SECS: u64 = 5;

        let mut download_url: Option<String> = None;

        for attempt in 1..=MAX_POLL_ATTEMPTS {
            self.progress_tracker.set_current_item(
                session_id,
                aggregate_index,
                Some(format!(
                    "Ожидание готовности отчёта... ({}/{})",
                    attempt, MAX_POLL_ATTEMPTS
                )),
            );

            tokio::time::sleep(tokio::time::Duration::from_secs(POLL_INTERVAL_SECS)).await;

            let (status, file_url) = self
                .api_client
                .poll_report_status(connection, &report_id)
                .await
                .map_err(|e| {
                    self.progress_tracker.add_error(
                        session_id,
                        Some(aggregate_index.to_string()),
                        format!("Ошибка получения статуса отчёта: {}", e),
                        None,
                    );
                    e
                })?;

            tracing::info!("Payment report status (attempt {}): {}", attempt, status);

            match status.as_str() {
                "DONE" => {
                    download_url = file_url;
                    break;
                }
                "FAILED" => {
                    let msg = "Генерация отчёта завершилась ошибкой на стороне YM".to_string();
                    self.progress_tracker.add_error(
                        session_id,
                        Some(aggregate_index.to_string()),
                        msg.clone(),
                        None,
                    );
                    anyhow::bail!("{}", msg);
                }
                _ => {
                    // PENDING / PROCESSING — continue polling
                }
            }

            if attempt == MAX_POLL_ATTEMPTS {
                let msg = format!(
                    "Превышено время ожидания готовности отчёта ({} попыток по {}с)",
                    MAX_POLL_ATTEMPTS, POLL_INTERVAL_SECS
                );
                self.progress_tracker.add_error(
                    session_id,
                    Some(aggregate_index.to_string()),
                    msg.clone(),
                    None,
                );
                anyhow::bail!("{}", msg);
            }
        }

        let url = download_url.ok_or_else(|| {
            let msg = "Отчёт DONE, но URL файла не получен";
            self.progress_tracker.add_error(
                session_id,
                Some(aggregate_index.to_string()),
                msg.to_string(),
                None,
            );
            anyhow::anyhow!("{}", msg)
        })?;

        // Phase 3: download ZIP and extract CSV
        self.progress_tracker.set_current_item(
            session_id,
            aggregate_index,
            Some("Загрузка ZIP-архива...".to_string()),
        );

        let (csv_text, zip_path, csv_path) = self
            .api_client
            .download_report_zip(&url, "p907_ym_payment_report")
            .await
            .map_err(|e| {
                self.progress_tracker.add_error(
                    session_id,
                    Some(aggregate_index.to_string()),
                    format!("Ошибка загрузки ZIP: {}", e),
                    None,
                );
                e
            })?;

        tracing::info!(
            "Payment report ZIP saved to: {}, CSV saved to: {}",
            zip_path,
            csv_path
        );

        // Phase 4: parse CSV in memory (no DB work — cheap even for large files)
        self.progress_tracker.set_current_item(
            session_id,
            aggregate_index,
            Some(format!("Разбор CSV ({})...", csv_path)),
        );

        let parsed =
            payment_report::parse_payment_report_csv(connection, &organization_id, &csv_text)
                .map_err(|e| {
                    self.progress_tracker.add_error(
                        session_id,
                        Some(aggregate_index.to_string()),
                        format!("Ошибка разбора CSV: {}", e),
                        None,
                    );
                    e
                })?;

        let entries = parsed.entries;
        let total = entries.len() as i32;
        if parsed.skipped > 0 {
            tracing::warn!(
                "YM payment report: {} CSV rows skipped during parse",
                parsed.skipped
            );
        }

        // Phase 5: bulk-upsert raw rows in batches. One multi-row statement per
        // batch replaces thousands of per-row autocommit transactions — the main
        // fix for large-file import stalls.
        //
        // Batch sized so columns × rows stays well under SQLite's bound-parameter
        // limit (~32k): 36 columns × 500 = 18k params.
        const UPSERT_BATCH: usize = 500;
        let mut saved = 0i32;
        for chunk in entries.chunks(UPSERT_BATCH) {
            crate::projections::p907_ym_payment_report::repository::bulk_upsert_entries(chunk)
                .await
                .map_err(|e| {
                    self.progress_tracker.add_error(
                        session_id,
                        Some(aggregate_index.to_string()),
                        format!("Ошибка сохранения строк отчёта: {}", e),
                        None,
                    );
                    e
                })?;
            saved += chunk.len() as i32;
            self.progress_tracker.set_current_item(
                session_id,
                aggregate_index,
                Some(format!("Сохранение строк отчёта... ({}/{})", saved, total)),
            );
            self.progress_tracker.update_aggregate(
                session_id,
                aggregate_index,
                saved,
                Some(total),
                saved,
                0,
            );
        }

        // Phase 5.5: снять pending-дубли «Будет … по графику выплат», у которых уже есть
        // проведённый двойник (прогноз выплаты + факт = одни деньги; иначе задваивается
        // сумма документа a013). Делаем до проводки — удалённые строки уже не проводятся
        // (Phase 6 идёт по `entries`: для снятого record_key rebuild вернёт 0).
        //
        // Проверяем только заказы pending-строк ЭТОЙ партии (обычно единицы), поэтому запрос
        // идёт по idx_p907_order_id и не сканирует всю таблицу p907; если pending-строк в
        // партии нет — фаза пропускается без запросов к БД.
        let pending_order_ids: Vec<i64> = entries
            .iter()
            .filter(|e| {
                e.payment_status
                    .as_deref()
                    .map(|s| s.trim_start().starts_with("Будет "))
                    .unwrap_or(false)
            })
            .filter_map(|e| e.order_id)
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect();
        if !pending_order_ids.is_empty() {
            match crate::projections::p907_ym_payment_report::service::purge_superseded_pending_payouts(
                &pending_order_ids,
            )
            .await
            {
                Ok(n) if n > 0 => {
                    tracing::info!("YM payment report: снято {} pending-дублей «Будет …»", n)
                }
                Ok(_) => {}
                Err(e) => {
                    tracing::error!("YM payment report: дедуп pending-строк не удался: {}", e);
                    self.progress_tracker.add_error(
                        session_id,
                        Some(aggregate_index.to_string()),
                        "Ошибка дедупа pending-строк (Будет … по графику выплат)".to_string(),
                        Some(e.to_string()),
                    );
                }
            }
        }

        // Phase 6: post each row (GL/p914) one by one, tolerant of per-row errors
        // so a single bad row no longer aborts the whole import. Reuses the same
        // rebuild path as u508 repost. Progress is updated periodically.
        let mut posted = 0i32;
        for (idx, entry) in entries.iter().enumerate() {
            match crate::projections::p907_ym_payment_report::service::rebuild_record_key_from_existing(
                &entry.record_key,
            )
            .await
            {
                Ok(_) => posted += 1,
                Err(e) => {
                    tracing::error!(
                        "Failed to post payment report row {}: {}",
                        entry.record_key,
                        e
                    );
                    self.progress_tracker.add_error(
                        session_id,
                        Some(aggregate_index.to_string()),
                        format!("Ошибка проведения строки {}", entry.record_key),
                        Some(e.to_string()),
                    );
                }
            }

            let done = idx + 1;
            if done % 50 == 0 || done == entries.len() {
                self.progress_tracker.set_current_item(
                    session_id,
                    aggregate_index,
                    Some(format!("Проведение GL/p914... ({}/{})", done, total)),
                );
                self.progress_tracker.update_aggregate(
                    session_id,
                    aggregate_index,
                    total,
                    Some(total),
                    saved,
                    posted,
                );
            }
        }

        // Phase 7: завершить денежный контур — перестроить проводки перечислений
        // (Дт51/Кт7609) по банковским ордерам. Distinct bank_order_id'ов немного
        // (десятки), поэтому перестраиваем весь диапазон — дёшево и идемпотентно.
        match crate::projections::p907_ym_payment_report::settlement_posting::rebuild_settlements_for_range(
            "0000-01-01",
            "9999-12-31",
        )
        .await
        {
            Ok(n) => tracing::info!("YM settlements rebuilt: {} bank-order entries", n),
            Err(e) => {
                tracing::error!("YM settlement rebuild failed: {}", e);
                self.progress_tracker.add_error(
                    session_id,
                    Some(aggregate_index.to_string()),
                    "Ошибка перестроения перечислений (ym_settlement)".to_string(),
                    Some(e.to_string()),
                );
            }
        }

        self.progress_tracker
            .set_current_item(session_id, aggregate_index, None);
        self.progress_tracker
            .complete_aggregate(session_id, aggregate_index);

        tracing::info!(
            "YM payment report import completed: saved={}, posted={}, skipped={}",
            saved,
            posted,
            parsed.skipped
        );

        Ok(())
    }
}

impl ImportExecutor {
    /// Импорт «Отчёта о реализации» YM → агрегат a034_ym_realization (слой ybuh).
    /// Отчёт помесячный на кампанию: перебираем месяцы периода, для каждого —
    /// генерация (year/month) → polling → download → parse → replace_for_period →
    /// авто-пост. Между месяцами лимит YM (1/мин) гасит wait-and-retry в генераторе.
    async fn import_realization(
        &self,
        session_id: &str,
        connection: &contracts::domain::a006_connection_mp::aggregate::ConnectionMP,
        date_from: chrono::NaiveDate,
        date_to: chrono::NaiveDate,
    ) -> Result<()> {
        use crate::domain::a002_organization;

        tracing::info!(
            "Importing YM realization report for session: {}",
            session_id
        );
        let aggregate_index = "a034_ym_realization";

        let organization_id = match Uuid::parse_str(&connection.organization_ref) {
            Ok(org_uuid) => match a002_organization::service::get_by_id(
                crate::shared::data::db::get_connection(),
                org_uuid,
            )
            .await?
            {
                Some(org) => org.base.id.as_string(),
                None => anyhow::bail!(
                    "Organization UUID '{}' not found",
                    connection.organization_ref
                ),
            },
            Err(_) => anyhow::bail!(
                "Invalid organization_ref UUID in connection: '{}'",
                connection.organization_ref
            ),
        };
        let connection_id = connection.base.id.as_string();

        // Кампании бизнеса (fan-out): отчёт о реализации YM помесячный НА КАМПАНИЮ.
        // Раньше тянули только `supplier_id` (одна модель, обычно FBS) — из-за чего
        // ybuh не покрывал FBY и сверка выручки давала стабильный односторонний
        // перекос. Перебираем все магазины бизнеса, как orders/returns.
        let campaigns = self
            .resolve_campaigns(session_id, aggregate_index, connection)
            .await;
        if campaigns.is_empty() {
            anyhow::bail!(
                "Не задан магазин: у подключения нет supplier_id и GET /campaigns не вернул кампаний"
            );
        }

        // Месяцы, попадающие в период [date_from, date_to].
        let months = months_in_range(date_from, date_to);
        let total_months = months.len() as i32;

        let mut doc_total = 0i32;
        let mut posted_total = 0i32;

        for (month_idx, (year, month)) in months.iter().enumerate() {
            let (year, month) = (*year, *month);
            let first_day = chrono::NaiveDate::from_ymd_opt(year, month, 1)
                .ok_or_else(|| anyhow::anyhow!("Invalid month {}-{}", year, month))?;
            let last_day = {
                let (ny, nm) = if month == 12 {
                    (year + 1, 1)
                } else {
                    (year, month + 1)
                };
                chrono::NaiveDate::from_ymd_opt(ny, nm, 1)
                    .and_then(|d| d.pred_opt())
                    .ok_or_else(|| anyhow::anyhow!("Invalid month end {}-{}", year, month))?
            };
            let month_first = first_day.format("%Y-%m-%d").to_string();
            let month_last = last_day.format("%Y-%m-%d").to_string();

            // Каждая кампания месяца импортируется НЕЗАВИСИМО: fetch → покампанийная
            // замена (`replace_for_period_campaign` трогает только свои документы) →
            // проведение. Сбой одной кампании (сеть/лимит/FAILED/отсутствие отчёта у
            // модели типа DBS) не роняет ни другие кампании, ни другие месяцы — он
            // только логируется и попадает в ошибки импорта.
            for (campaign_id, placement_type) in &campaigns {
                let model = placement_type.as_deref().unwrap_or("?");

                let docs = match self
                    .fetch_realization_for_campaign(
                        session_id,
                        aggregate_index,
                        connection,
                        &organization_id,
                        campaign_id,
                        placement_type.as_deref(),
                        year,
                        month,
                        month_idx + 1,
                        total_months,
                        &month_first,
                        &month_last,
                    )
                    .await
                {
                    Ok(docs) => docs,
                    Err(e) => {
                        tracing::error!(
                            "Realization {} за {}-{:02} не загружен: {}",
                            model,
                            year,
                            month,
                            e
                        );
                        self.progress_tracker.add_error(
                            session_id,
                            Some(aggregate_index.to_string()),
                            format!(
                                "Отчёт {} за {}-{:02} не загружен: {}",
                                model, year, month, e
                            ),
                            None,
                        );
                        continue;
                    }
                };

                // Phase 5: покампанийная замена документов месяца (идемпотентно).
                if let Err(e) =
                    crate::domain::a034_ym_realization::service::replace_for_period_campaign(
                        &connection_id,
                        campaign_id,
                        &month_first,
                        &month_last,
                        &docs,
                    )
                    .await
                {
                    tracing::error!(
                        "Ошибка сохранения a034 {} за {}-{:02}: {}",
                        model,
                        year,
                        month,
                        e
                    );
                    self.progress_tracker.add_error(
                        session_id,
                        Some(aggregate_index.to_string()),
                        format!(
                            "Ошибка сохранения {} за {}-{:02}: {}",
                            model, year, month, e
                        ),
                        None,
                    );
                    continue;
                }

                // Phase 6: провести каждый документ кампании в GL (слой ybuh).
                for document in &docs {
                    doc_total += 1;
                    let id = match Uuid::parse_str(&document.base.id.as_string()) {
                        Ok(v) => v,
                        Err(e) => {
                            tracing::error!("Invalid a034 document id: {}", e);
                            continue;
                        }
                    };
                    match crate::domain::a034_ym_realization::posting::post_document(id).await {
                        Ok(_) => posted_total += 1,
                        Err(e) => {
                            tracing::error!("Failed to post realization document {}: {}", id, e);
                            self.progress_tracker.add_error(
                                session_id,
                                Some(aggregate_index.to_string()),
                                format!("Ошибка проведения документа {}", id),
                                Some(e.to_string()),
                            );
                        }
                    }
                }
                self.progress_tracker.update_aggregate(
                    session_id,
                    aggregate_index,
                    doc_total,
                    Some(doc_total),
                    doc_total,
                    posted_total,
                );
            }
        }

        self.progress_tracker
            .set_current_item(session_id, aggregate_index, None);
        self.progress_tracker
            .complete_aggregate(session_id, aggregate_index);
        tracing::info!(
            "YM realization import completed: months={}, documents={}, posted={}",
            total_months,
            doc_total,
            posted_total
        );
        Ok(())
    }

    /// Загрузка отчёта о реализации ОДНОЙ кампании за месяц: generate → poll →
    /// download → parse. Ошибки возвращаются вызывающему (изолируются на уровне
    /// месяца), поэтому сбой одной кампании не роняет весь импорт.
    ///
    /// Поллинг статуса терпит временные сетевые ошибки: единичный таймаут к YM не
    /// прерывает импорт — попытка просто повторяется на следующем интервале. Валит
    /// кампанию только явный `FAILED` от YM или исчерпание всех попыток.
    #[allow(clippy::too_many_arguments)]
    async fn fetch_realization_for_campaign(
        &self,
        session_id: &str,
        aggregate_index: &str,
        connection: &contracts::domain::a006_connection_mp::aggregate::ConnectionMP,
        organization_id: &str,
        campaign_id: &str,
        placement_type: Option<&str>,
        year: i32,
        month: u32,
        month_pos: usize,
        total_months: i32,
        month_first: &str,
        month_last: &str,
    ) -> Result<Vec<contracts::domain::a034_ym_realization::aggregate::YmRealization>> {
        let model = placement_type.unwrap_or("?");

        // Соединение с supplier_id = campaignId: generate_realization_report берёт
        // campaignId именно из supplier_id (как orders/returns).
        let mut conn = connection.clone();
        conn.supplier_id = Some(campaign_id.to_string());

        // Phase 1: generate (год/месяц). Лимит goods-realization — 1/мин на бизнес;
        // ретраи с ожиданием живут в самом generate_realization_report.
        self.progress_tracker.set_current_item(
            session_id,
            aggregate_index,
            Some(format!(
                "Генерация отчёта {} за {}-{:02} ({}/{})...",
                model, year, month, month_pos, total_months
            )),
        );
        let report_id = self
            .api_client
            .generate_realization_report(&conn, year, month)
            .await
            .map_err(|e| anyhow::anyhow!("генерация: {}", e))?;

        // Phase 2: poll until DONE (терпим временные ошибки статуса).
        const MAX_POLL_ATTEMPTS: u32 = 60;
        const POLL_INTERVAL_SECS: u64 = 5;
        let mut download_url: Option<String> = None;
        let mut poll_errors = 0u32;
        for attempt in 1..=MAX_POLL_ATTEMPTS {
            self.progress_tracker.set_current_item(
                session_id,
                aggregate_index,
                Some(format!(
                    "Ожидание отчёта {} за {}-{:02}... ({}/{})",
                    model, year, month, attempt, MAX_POLL_ATTEMPTS
                )),
            );
            tokio::time::sleep(tokio::time::Duration::from_secs(POLL_INTERVAL_SECS)).await;
            match self
                .api_client
                .poll_report_status(connection, &report_id)
                .await
            {
                Ok((status, file_url)) => match status.as_str() {
                    "DONE" => {
                        download_url = file_url;
                        break;
                    }
                    "FAILED" => {
                        anyhow::bail!("YM вернул FAILED при генерации отчёта");
                    }
                    _ => {}
                },
                Err(e) => {
                    // Временная ошибка (сеть/таймаут) — не роняем, повторяем поллинг.
                    poll_errors += 1;
                    tracing::warn!(
                        "poll {} {}-{:02} attempt {}/{}: {} (повтор)",
                        model,
                        year,
                        month,
                        attempt,
                        MAX_POLL_ATTEMPTS,
                        e
                    );
                }
            }
            if attempt == MAX_POLL_ATTEMPTS {
                anyhow::bail!(
                    "превышено время ожидания отчёта (сетевых ошибок статуса: {})",
                    poll_errors
                );
            }
        }
        let url =
            download_url.ok_or_else(|| anyhow::anyhow!("отчёт DONE, но URL файла не получен"))?;

        // Phase 3: download + extract ВСЕ CSV (delivered/returned/…).
        let csvs = self
            .api_client
            .download_report_csvs(&url, "a034_ym_realization")
            .await
            .map_err(|e| anyhow::anyhow!("загрузка ZIP: {}", e))?;

        // Phase 4: parse (delivered − returned; строки тегируются кампанией/моделью).
        let parsed = realization::parse_realization_files(
            connection,
            organization_id,
            &csvs,
            month_first,
            month_last,
            Some(campaign_id),
            placement_type,
        )
        .map_err(|e| anyhow::anyhow!("разбор CSV: {}", e))?;

        Ok(parsed.documents)
    }
}

/// Список (год, месяц), попадающих в период [from, to] включительно.
fn months_in_range(from: chrono::NaiveDate, to: chrono::NaiveDate) -> Vec<(i32, u32)> {
    use chrono::Datelike;
    let mut out = Vec::new();
    let (mut y, mut m) = (from.year(), from.month());
    let (end_y, end_m) = (to.year(), to.month());
    while (y, m) <= (end_y, end_m) {
        out.push((y, m));
        if m == 12 {
            y += 1;
            m = 1;
        } else {
            m += 1;
        }
    }
    out
}

impl Clone for ImportExecutor {
    fn clone(&self) -> Self {
        Self {
            api_client: Arc::clone(&self.api_client),
            progress_tracker: Arc::clone(&self.progress_tracker),
        }
    }
}
