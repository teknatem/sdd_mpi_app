use super::progress_tracker::ProgressTracker;
use anyhow::{anyhow, Result};
use chrono::NaiveDate;
use contracts::domain::common::AggregateId;
use contracts::projections::p916_mp_sales_funnel_turnovers::dto::FunnelRebuildRequest;
use contracts::usecases::u508_repost_documents::{
    aggregate::AggregateOption,
    aggregate_request::AggregateRepostRequest,
    progress::RepostStatus,
    projection::ProjectionOption,
    request::RepostRequest,
    response::{RepostResponse, RepostStartStatus},
};
use std::sync::{
    atomic::{AtomicI32, Ordering},
    Arc,
};
use uuid::Uuid;

const P904_SALES_DATA: &str = "p904_sales_data";
const P903_FINANCE_REPORT: &str = "p903_wb_finance_report";
const P907_PAYMENT_REPORT: &str = "p907_ym_payment_report";
const A012_WB_SALES: &str = "a012_wb_sales";
const A015_WB_ORDERS: &str = "a015_wb_orders";
const A021_PRODUCTION_OUTPUT: &str = "a021_production_output";
const A023_PURCHASE_OF_GOODS: &str = "a023_purchase_of_goods";
const A026_WB_ADVERT_DAILY: &str = "a026_wb_advert_daily";
const A034_YM_REALIZATION: &str = "a034_ym_realization";
const A013_YM_ORDER: &str = "a013_ym_order";
const A016_YM_RETURNS: &str = "a016_ym_returns";

pub struct RepostExecutor {
    pub progress_tracker: Arc<ProgressTracker>,
}

impl RepostExecutor {
    pub fn new(progress_tracker: Arc<ProgressTracker>) -> Self {
        Self { progress_tracker }
    }

    pub fn list_available_projections(&self) -> Vec<ProjectionOption> {
        vec![
            ProjectionOption {
                key: P903_FINANCE_REPORT.to_string(),
                label: "p903 — WB Finance Report".to_string(),
                description:
                    "Локальная пересборка general ledger по сохранённым строкам p903_wb_finance_report".to_string(),
            },
            ProjectionOption {
                key: P904_SALES_DATA.to_string(),
                label: "p904 — Sales Data".to_string(),
                description:
                    "Перепроведение документов по registrator_ref из p904_sales_data".to_string(),
            },
            ProjectionOption {
                key: P907_PAYMENT_REPORT.to_string(),
                label: "p907 — YM Payment Report".to_string(),
                description:
                    "Пересборка general ledger по всем строкам p907 за период (включая перечисления Дт51/Кт7609 по банковским ордерам) и событий оплаты p915".to_string(),
            },
        ]
    }

    pub fn list_available_aggregates(&self) -> Vec<AggregateOption> {
        vec![
            AggregateOption {
                key: A012_WB_SALES.to_string(),
                label: "a012 — WB Sales".to_string(),
                description:
                    "Перепроведение документов a012_wb_sales с пересборкой связанных проекций"
                        .to_string(),
            },
            AggregateOption {
                key: A015_WB_ORDERS.to_string(),
                label: "a015 — WB Orders".to_string(),
                description:
                    "Перепроведение документов a015_wb_orders с пересборкой строк p909"
                        .to_string(),
            },
            AggregateOption {
                key: A021_PRODUCTION_OUTPUT.to_string(),
                label: "a021 — Production Output".to_string(),
                description:
                    "Перепроведение документов a021_production_output с пересборкой связанных проекций"
                        .to_string(),
            },
            AggregateOption {
                key: A023_PURCHASE_OF_GOODS.to_string(),
                label: "a023 — Purchase Of Goods".to_string(),
                description:
                    "Перепроведение документов a023_purchase_of_goods с пересборкой связанных проекций"
                        .to_string(),
            },
            AggregateOption {
                key: A026_WB_ADVERT_DAILY.to_string(),
                label: "a026 — WB Advert Daily".to_string(),
                description: "Перепроведение проведённых документов a026_wb_advert_daily с пересборкой связанных проекций".to_string(),
            },
            AggregateOption {
                key: A034_YM_REALIZATION.to_string(),
                label: "a034 — YM Realization".to_string(),
                description: "Перепроведение документов a034_ym_realization с пересборкой GL-проводок слоя ybuh и событий реализации/возврата p915".to_string(),
            },
            AggregateOption {
                key: A013_YM_ORDER.to_string(),
                label: "a013 — YM Order".to_string(),
                description: "Перепроведение заказов a013_ym_order с пересборкой p900/p904/p915 и движений воронки p916 (заказы, отмены, выкупы)".to_string(),
            },
            AggregateOption {
                key: A016_YM_RETURNS.to_string(),
                label: "a016 — YM Returns".to_string(),
                description: "Перепроведение возвратов a016_ym_returns с пересборкой p904 и движений возврата в воронке p916 (только return_type=RETURN)".to_string(),
            },
        ]
    }

    pub async fn start_repost(&self, request: RepostRequest) -> Result<RepostResponse> {
        let database_activity = crate::system::maintenance::try_begin_database_activity()
            .ok_or_else(|| {
                anyhow!("Перепроведение недоступно во время обслуживания базы данных")
            })?;
        Self::validate_request(&request)?;

        let session_id = Uuid::new_v4().to_string();
        self.progress_tracker.create_session(session_id.clone());

        let executor = Arc::new(Self {
            progress_tracker: self.progress_tracker.clone(),
        });
        let sid = session_id.clone();
        let req = request.clone();

        tokio::spawn(async move {
            let _database_activity = database_activity;
            if let Err(error) = executor.execute_repost(&sid, &req).await {
                tracing::error!("Projection repost failed: {}", error);
                executor
                    .progress_tracker
                    .add_error(&sid, format!("Repost failed: {}", error));
                executor
                    .progress_tracker
                    .complete_session(&sid, RepostStatus::Failed);
            }
        });

        Ok(RepostResponse {
            session_id,
            status: RepostStartStatus::Started,
            message: "Repost started".to_string(),
        })
    }

    pub async fn start_aggregate_repost(
        &self,
        request: AggregateRepostRequest,
    ) -> Result<RepostResponse> {
        let database_activity = crate::system::maintenance::try_begin_database_activity()
            .ok_or_else(|| {
                anyhow!("Перепроведение недоступно во время обслуживания базы данных")
            })?;
        Self::validate_aggregate_request(&request)?;

        let session_id = Uuid::new_v4().to_string();
        self.progress_tracker.create_session(session_id.clone());

        let executor = Arc::new(Self {
            progress_tracker: self.progress_tracker.clone(),
        });
        let sid = session_id.clone();
        let req = request.clone();

        tokio::spawn(async move {
            let _database_activity = database_activity;
            if let Err(error) = executor.execute_aggregate_repost(&sid, &req).await {
                tracing::error!("Aggregate repost failed: {}", error);
                executor
                    .progress_tracker
                    .add_error(&sid, format!("Repost failed: {}", error));
                executor
                    .progress_tracker
                    .complete_session(&sid, RepostStatus::Failed);
            }
        });

        Ok(RepostResponse {
            session_id,
            status: RepostStartStatus::Started,
            message: "Aggregate repost started".to_string(),
        })
    }

    pub fn get_progress(
        &self,
        session_id: &str,
    ) -> Option<contracts::usecases::u508_repost_documents::progress::RepostProgress> {
        self.progress_tracker.get_progress(session_id)
    }

    /// Запустить пересбор воронки p916 за период: фоновая сессия, три шага
    /// (a015 → a012 → a036 стадия 1). Прогресс читается общим `get_progress`.
    pub async fn start_funnel_rebuild(
        &self,
        request: FunnelRebuildRequest,
    ) -> Result<RepostResponse> {
        let database_activity = crate::system::maintenance::try_begin_database_activity()
            .ok_or_else(|| anyhow!("Пересбор недоступен во время обслуживания базы данных"))?;
        let date_from = NaiveDate::parse_from_str(&request.date_from, "%Y-%m-%d")
            .map_err(|_| anyhow!("Invalid date_from: {}", request.date_from))?;
        let date_to = NaiveDate::parse_from_str(&request.date_to, "%Y-%m-%d")
            .map_err(|_| anyhow!("Invalid date_to: {}", request.date_to))?;
        if date_from > date_to {
            return Err(anyhow!("date_from must be less than or equal to date_to"));
        }

        let session_id = Uuid::new_v4().to_string();
        self.progress_tracker.create_session(session_id.clone());

        let executor = Arc::new(Self {
            progress_tracker: self.progress_tracker.clone(),
        });
        let sid = session_id.clone();
        let req = request.clone();

        tokio::spawn(async move {
            let _database_activity = database_activity;
            if let Err(error) = executor.execute_funnel_rebuild(&sid, &req).await {
                tracing::error!("Funnel rebuild failed: {}", error);
                executor
                    .progress_tracker
                    .add_error(&sid, format!("Funnel rebuild failed: {}", error));
                executor
                    .progress_tracker
                    .complete_session(&sid, RepostStatus::Failed);
            }
        });

        Ok(RepostResponse {
            session_id,
            status: RepostStartStatus::Started,
            message: "Funnel rebuild started".to_string(),
        })
    }

    /// Шесть шагов пересбора воронки: WB (a015 → a012 → a026 → a036) и YM (a013 → a016).
    /// Перепроведение агрегатов переиспользует `dispatch_aggregate_repost_with_retry`
    /// (внутри — p916-хуки в проведении); стадия 1 a036 —
    /// `a036::service::rebuild_stage1_for_period`.
    ///
    /// Когортный отбор: a012 — по srid'ам заказов a015 периода, a016 — по номерам заказов
    /// a013 периода, a026/a036 — по `document_date`. Порядок a013 → a016 обязателен:
    /// когорта возврата резолвится по дате заказа из a013. Ошибки шага не прерывают
    /// прогон — копятся в сессии.
    async fn execute_funnel_rebuild(
        &self,
        session_id: &str,
        request: &FunnelRebuildRequest,
    ) -> Result<()> {
        // Перепроводим все документы периода (не только проведённые), чтобы гарантированно
        // пересобрать движения воронки.
        let only_posted = false;

        // Перечень документов-источников за период.
        let a015_ids = crate::domain::a015_wb_orders::repository::list_ids_by_date_range_scoped(
            &request.date_from,
            &request.date_to,
            &request.connection_mp_refs,
            only_posted,
        )
        .await?;

        // a012 отбираем «по периоду заказов»: srid'ы заказов a015 за период → продажи a012 по этим
        // srid'ам с нижней границей sale_date >= date_from (без верхней), чтобы захватить выкупы
        // когорты, проданные после конца периода.
        let a012_order_srids =
            crate::domain::a015_wb_orders::repository::list_order_srids_by_date_range(
                &request.date_from,
                &request.date_to,
                &request.connection_mp_refs,
            )
            .await?;
        let a012_ids = crate::domain::a012_wb_sales::repository::list_ids_by_document_nos_since(
            &request.date_from,
            &a012_order_srids,
            only_posted,
        )
        .await?;

        // Документы рекламы a026 за период (стадия 1, платные показы p916).
        let a026_ids = crate::domain::a026_wb_advert_daily::repository::list_ids_by_period_scoped(
            &request.date_from,
            &request.date_to,
            &request.connection_mp_refs,
            only_posted,
        )
        .await?;

        // YM: заказы когорты (заказ/отмена/выкуп) и возвраты этих заказов.
        // Отбор возвратов идёт по заказам периода, а не по своей дате — движение
        // возврата должно лечь в когорту заказа.
        let ym_orders = crate::domain::a013_ym_order::repository::list_ids_by_creation_period(
            &request.date_from,
            &request.date_to,
            &request.connection_mp_refs,
            only_posted,
        )
        .await?;
        let ym_order_numbers: Vec<i64> = ym_orders
            .iter()
            .filter_map(|(_, document_no)| document_no.parse::<i64>().ok())
            .collect();
        let a013_ids: Vec<String> = ym_orders.into_iter().map(|(id, _)| id).collect();
        let a016_ids = crate::domain::a016_ym_returns::repository::list_ids_by_order_ids(
            &ym_order_numbers,
            only_posted,
        )
        .await?;

        // +1 — шаг пересборки стадии 1 (a036).
        let total = (a015_ids.len()
            + a012_ids.len()
            + a026_ids.len()
            + a013_ids.len()
            + a016_ids.len()
            + 1) as i32;
        self.progress_tracker.set_total(session_id, total);
        self.progress_tracker.set_chunks_total(session_id, 6);

        // Сквозные счётчики прогресса по всем шагам (конкурентные шаги пишут в атомики;
        // последовательные — в те же счётчики).
        let processed = Arc::new(AtomicI32::new(0));
        let reposted = Arc::new(AtomicI32::new(0));

        // === Шаг 1/6: a015 — заказы/отмены WB (стадия 2) ===
        self.progress_tracker.update_chunk_progress(
            session_id,
            0,
            None,
            None,
            Some("Шаг 1/6: заказы a015 (стадия 2)".to_string()),
        );
        repost_ids_concurrent(
            &self.progress_tracker,
            session_id,
            A015_WB_ORDERS,
            a015_ids,
            &processed,
            &reposted,
        )
        .await?;

        // === Шаг 2/6: a012 — выкупы/возвраты (стадия 2) ===
        // Кэш-по-дню: группируем когортный набор по дню продажи и один раз прогреваем
        // PostingPreparationCache на весь день (как в execute_a012_chunked_repost) — иначе
        // post_document создаёт пустой кэш на каждый документ и повторяет дорогие lookups.
        self.progress_tracker.update_chunk_progress(
            session_id,
            1,
            None,
            None,
            Some("Шаг 2/6: продажи a012 (стадия 2)".to_string()),
        );
        self.rebuild_funnel_a012_cached(session_id, a012_ids, &processed, &reposted)
            .await?;

        // === Шаг 3/6: a026 — реклама/платные показы (стадия 1) ===
        self.progress_tracker.update_chunk_progress(
            session_id,
            2,
            None,
            None,
            Some("Шаг 3/6: реклама a026 (стадия 1)".to_string()),
        );
        repost_ids_concurrent(
            &self.progress_tracker,
            session_id,
            A026_WB_ADVERT_DAILY,
            a026_ids,
            &processed,
            &reposted,
        )
        .await?;

        // === Шаг 4/6: a013 — заказы/отмены/выкупы YM (стадия 2) ===
        self.progress_tracker.update_chunk_progress(
            session_id,
            3,
            None,
            None,
            Some("Шаг 4/6: заказы a013 YM (стадия 2)".to_string()),
        );
        repost_ids_concurrent(
            &self.progress_tracker,
            session_id,
            A013_YM_ORDER,
            a013_ids,
            &processed,
            &reposted,
        )
        .await?;

        // === Шаг 5/6: a016 — возвраты YM (стадия 2) ===
        // Строго после a013: когорта возврата резолвится по дате заказа из a013.
        self.progress_tracker.update_chunk_progress(
            session_id,
            4,
            None,
            None,
            Some("Шаг 5/6: возвраты a016 YM (стадия 2)".to_string()),
        );
        repost_ids_concurrent(
            &self.progress_tracker,
            session_id,
            A016_YM_RETURNS,
            a016_ids,
            &processed,
            &reposted,
        )
        .await?;

        // === Шаг 6/6: a036 — стадия 1 (маркетинг) из сохранённых документов ===
        self.progress_tracker.update_chunk_progress(
            session_id,
            5,
            None,
            None,
            Some("Шаг 6/6: воронка a036 (стадия 1)".to_string()),
        );
        match crate::domain::a036_wb_sales_funnel_daily::service::rebuild_stage1_for_period(
            &request.connection_mp_refs,
            &request.date_from,
            &request.date_to,
        )
        .await
        {
            Ok(_) => {
                reposted.fetch_add(1, Ordering::Relaxed);
            }
            Err(error) => self
                .progress_tracker
                .add_error(session_id, format!("a036 стадия 1: {}", error)),
        }
        let processed_final = processed.fetch_add(1, Ordering::Relaxed) + 1;
        let reposted_final = reposted.load(Ordering::Relaxed);
        self.progress_tracker
            .update_progress(session_id, processed_final, reposted_final, None);
        self.progress_tracker.update_chunk_progress(
            session_id,
            6,
            None,
            None,
            Some("Готово".to_string()),
        );

        let final_status = if self
            .progress_tracker
            .get_progress(session_id)
            .map(|progress| progress.errors > 0)
            .unwrap_or(false)
        {
            RepostStatus::CompletedWithErrors
        } else {
            RepostStatus::Completed
        };
        self.progress_tracker
            .complete_session(session_id, final_status);

        Ok(())
    }

    /// Синхронный вход для оркестраторов, которым нужен подтверждённый итог шага,
    /// а не только фоновый session id.
    pub async fn run_funnel_rebuild(
        &self,
        session_id: &str,
        request: &FunnelRebuildRequest,
    ) -> Result<()> {
        self.execute_funnel_rebuild(session_id, request).await
    }

    /// Шаг 2/4 воронки: пересбор a012 с кэшем-по-дню. Когортный набор `a012_ids`
    /// (отобран по srid заказов периода) группируется по дню продажи; на каждый день
    /// один раз прогревается `PostingPreparationCache` через
    /// `preload_prod_cost_context_for_documents`, и все продажи дня проводятся через
    /// `post_document_with_cache`, переиспользуя дорогие lookups (цены/номенклатуры/киты/
    /// prod cost). Последовательно внутри дня (кэш — `&mut`, один писатель), как в
    /// `execute_a012_chunked_repost`. Прогресс/ошибки — в сквозные счётчики воронки.
    async fn rebuild_funnel_a012_cached(
        &self,
        session_id: &str,
        a012_ids: Vec<String>,
        processed: &Arc<AtomicI32>,
        reposted: &Arc<AtomicI32>,
    ) -> Result<()> {
        if a012_ids.is_empty() {
            return Ok(());
        }

        let id_days =
            crate::domain::a012_wb_sales::repository::list_id_sale_days_by_ids(&a012_ids).await?;

        // Группируем id по дню продажи (id_days отсортирован по дню, затем по id).
        let mut covered: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut day_groups: Vec<(String, Vec<String>)> = Vec::new();
        for (id, day) in id_days {
            covered.insert(id.clone());
            match day_groups.last_mut() {
                Some((current_day, ids)) if *current_day == day => ids.push(id),
                _ => day_groups.push((day, vec![id])),
            }
        }

        for (day, day_ids) in day_groups {
            let mut posting_cache =
                crate::domain::a012_wb_sales::service::PostingPreparationCache::default();
            let day_documents =
                crate::domain::a012_wb_sales::repository::list_by_ids(&day_ids).await?;
            crate::domain::a012_wb_sales::service::preload_prod_cost_context_for_documents(
                &mut posting_cache,
                &day_documents,
            )
            .await?;

            for id_str in &day_ids {
                let current_item = format!("a012 {} | {}", id_str, day);
                match Uuid::parse_str(id_str) {
                    Ok(id) => {
                        let post_start = std::time::Instant::now();
                        let post_result =
                            crate::domain::a012_wb_sales::posting::post_document_with_cache(
                                id,
                                &mut posting_cache,
                            )
                            .await;
                        let elapsed_ms = post_start.elapsed().as_millis() as i64;
                        self.progress_tracker
                            .record_post_timing(session_id, id_str, elapsed_ms);
                        const SLOW_DOC_MS: i64 = 500;
                        if elapsed_ms > SLOW_DOC_MS {
                            tracing::warn!(
                                "Slow {} post: {} took {} ms",
                                A012_WB_SALES,
                                id,
                                elapsed_ms
                            );
                        }
                        match post_result {
                            Ok(()) => {
                                reposted.fetch_add(1, Ordering::Relaxed);
                            }
                            Err(error) => self
                                .progress_tracker
                                .add_error(session_id, format!("a012 {}: {}", id_str, error)),
                        }
                    }
                    Err(error) => self
                        .progress_tracker
                        .add_error(session_id, format!("Invalid a012 id {}: {}", id_str, error)),
                }
                let p = processed.fetch_add(1, Ordering::Relaxed) + 1;
                let r = reposted.load(Ordering::Relaxed);
                self.progress_tracker
                    .update_progress(session_id, p, r, Some(current_item));
            }
        }

        // id, пропавшие из выборки между отбором и пересбором (soft-delete/удаление), —
        // отмечаем ошибкой и учитываем в прогрессе, чтобы processed совпадал с числом отобранных.
        for id_str in &a012_ids {
            if !covered.contains(id_str) {
                self.progress_tracker.add_error(
                    session_id,
                    format!("a012 {}: документ не найден при пересборе", id_str),
                );
                let p = processed.fetch_add(1, Ordering::Relaxed) + 1;
                let r = reposted.load(Ordering::Relaxed);
                self.progress_tracker
                    .update_progress(session_id, p, r, None);
            }
        }

        Ok(())
    }

    fn validate_request(request: &RepostRequest) -> Result<()> {
        if request.projection_key != P903_FINANCE_REPORT
            && request.projection_key != P904_SALES_DATA
            && request.projection_key != P907_PAYMENT_REPORT
        {
            return Err(anyhow!(
                "Unsupported projection_key: {}",
                request.projection_key
            ));
        }

        let date_from = NaiveDate::parse_from_str(&request.date_from, "%Y-%m-%d")
            .map_err(|_| anyhow!("Invalid date_from: {}", request.date_from))?;
        let date_to = NaiveDate::parse_from_str(&request.date_to, "%Y-%m-%d")
            .map_err(|_| anyhow!("Invalid date_to: {}", request.date_to))?;

        if date_from > date_to {
            return Err(anyhow!("date_from must be less than or equal to date_to"));
        }

        Ok(())
    }

    fn validate_aggregate_request(request: &AggregateRepostRequest) -> Result<()> {
        if request.aggregate_key != A012_WB_SALES
            && request.aggregate_key != A015_WB_ORDERS
            && request.aggregate_key != A021_PRODUCTION_OUTPUT
            && request.aggregate_key != A023_PURCHASE_OF_GOODS
            && request.aggregate_key != A026_WB_ADVERT_DAILY
            && request.aggregate_key != A034_YM_REALIZATION
            && request.aggregate_key != A013_YM_ORDER
            && request.aggregate_key != A016_YM_RETURNS
        {
            return Err(anyhow!(
                "Unsupported aggregate_key: {}",
                request.aggregate_key
            ));
        }

        let date_from = NaiveDate::parse_from_str(&request.date_from, "%Y-%m-%d")
            .map_err(|_| anyhow!("Invalid date_from: {}", request.date_from))?;
        let date_to = NaiveDate::parse_from_str(&request.date_to, "%Y-%m-%d")
            .map_err(|_| anyhow!("Invalid date_to: {}", request.date_to))?;

        if date_from > date_to {
            return Err(anyhow!("date_from must be less than or equal to date_to"));
        }

        Ok(())
    }

    async fn execute_repost(&self, session_id: &str, request: &RepostRequest) -> Result<()> {
        let registrators = match request.projection_key.as_str() {
            P903_FINANCE_REPORT => {
                self.progress_tracker.set_total(session_id, 1);
                self.progress_tracker.update_progress(
                    session_id,
                    0,
                    0,
                    Some("Rebuilding p903 general ledger".to_string()),
                );
                crate::projections::p903_wb_finance_report::service::rebuild_range_from_existing(
                    &request.date_from,
                    &request.date_to,
                )
                .await?;
                self.progress_tracker.update_progress(
                    session_id,
                    1,
                    1,
                    Some("Rebuilding p903 general ledger".to_string()),
                );
                self.progress_tracker
                    .complete_session(session_id, RepostStatus::Completed);
                return Ok(());
            }
            P907_PAYMENT_REPORT => {
                let ids =
                    crate::projections::p907_ym_payment_report::repository::list_ids_by_transaction_date_range(
                        &request.date_from,
                        &request.date_to,
                    )
                    .await?;

                let total = ids.len() as i32;
                self.progress_tracker.set_total(session_id, total);

                let mut reposted = 0;
                for (index, id) in ids.iter().enumerate() {
                    let current_item = format!("{} {}", P907_PAYMENT_REPORT, id);
                    self.progress_tracker.update_progress(
                        session_id,
                        index as i32,
                        reposted,
                        Some(current_item.clone()),
                    );

                    match crate::projections::p907_ym_payment_report::service::rebuild_entry_from_existing(
                        id,
                    )
                    .await
                    {
                        Ok(_) => reposted += 1,
                        Err(error) => self.progress_tracker.add_error(
                            session_id,
                            format!("Failed to repost {} {}: {}", P907_PAYMENT_REPORT, id, error),
                        ),
                    }

                    self.progress_tracker.update_progress(
                        session_id,
                        (index + 1) as i32,
                        reposted,
                        Some(current_item),
                    );
                }

                // Перечисления (Дт51/Кт7609) строятся одной проводкой на банковский
                // ордер — отдельно от построчного GL, поэтому перестраиваем их за тот
                // же период здесь же: один репост p907 покрывает все записи периода.
                if let Err(error) =
                    crate::projections::p907_ym_payment_report::settlement_posting::rebuild_settlements_for_range(
                        &request.date_from,
                        &request.date_to,
                    )
                    .await
                {
                    self.progress_tracker.add_error(
                        session_id,
                        format!("Failed to rebuild {} settlements: {}", P907_PAYMENT_REPORT, error),
                    );
                }

                self.progress_tracker
                    .update_progress(session_id, total, reposted, None);

                let final_status = if self
                    .progress_tracker
                    .get_progress(session_id)
                    .map(|progress| progress.errors > 0)
                    .unwrap_or(false)
                {
                    RepostStatus::CompletedWithErrors
                } else {
                    RepostStatus::Completed
                };

                self.progress_tracker
                    .complete_session(session_id, final_status);
                return Ok(());
            }
            P904_SALES_DATA => {
                crate::projections::p904_sales_data::repository::list_registrators_by_period(
                    &request.date_from,
                    &request.date_to,
                )
                .await?
            }
            _ => {
                return Err(anyhow!(
                    "Unsupported projection_key: {}",
                    request.projection_key
                ))
            }
        };

        let total = registrators.len() as i32;
        self.progress_tracker.set_total(session_id, total);

        let mut reposted = 0;

        for (index, registrator) in registrators.iter().enumerate() {
            let current_item = format!(
                "{} {}",
                registrator.registrator_type, registrator.registrator_ref
            );
            self.progress_tracker.update_progress(
                session_id,
                index as i32,
                reposted,
                Some(current_item.clone()),
            );

            let registrator_id = match Uuid::parse_str(&registrator.registrator_ref) {
                Ok(value) => value,
                Err(error) => {
                    self.progress_tracker.add_error(
                        session_id,
                        format!(
                            "Invalid registrator_ref {}: {}",
                            registrator.registrator_ref, error
                        ),
                    );
                    self.progress_tracker.update_progress(
                        session_id,
                        (index + 1) as i32,
                        reposted,
                        Some(current_item),
                    );
                    continue;
                }
            };

            if let Err(error) = dispatch_repost(&registrator.registrator_type, registrator_id).await
            {
                self.progress_tracker.add_error(
                    session_id,
                    format!(
                        "Failed to repost {} {}: {}",
                        registrator.registrator_type, registrator.registrator_ref, error
                    ),
                );
                self.progress_tracker.update_progress(
                    session_id,
                    (index + 1) as i32,
                    reposted,
                    Some(current_item),
                );
                continue;
            }

            reposted += 1;
            self.progress_tracker.update_progress(
                session_id,
                (index + 1) as i32,
                reposted,
                Some(current_item),
            );
        }

        self.progress_tracker
            .update_progress(session_id, total, reposted, None);

        let final_status = if self
            .progress_tracker
            .get_progress(session_id)
            .map(|progress| progress.errors > 0)
            .unwrap_or(false)
        {
            RepostStatus::CompletedWithErrors
        } else {
            RepostStatus::Completed
        };

        self.progress_tracker
            .complete_session(session_id, final_status);

        Ok(())
    }

    async fn execute_aggregate_repost(
        &self,
        session_id: &str,
        request: &AggregateRepostRequest,
    ) -> Result<()> {
        if request.aggregate_key == A012_WB_SALES {
            return self.execute_a012_chunked_repost(session_id, request).await;
        }

        let document_ids = match request.aggregate_key.as_str() {
            A015_WB_ORDERS => {
                crate::domain::a015_wb_orders::repository::list_ids_by_date_range(
                    &request.date_from,
                    &request.date_to,
                    request.only_posted,
                )
                .await?
            }
            A021_PRODUCTION_OUTPUT => {
                crate::domain::a021_production_output::repository::list_ids_by_document_date_range(
                    &request.date_from,
                    &request.date_to,
                    request.only_posted,
                )
                .await?
            }
            A023_PURCHASE_OF_GOODS => {
                crate::domain::a023_purchase_of_goods::repository::list_ids_by_document_date_range(
                    &request.date_from,
                    &request.date_to,
                    request.only_posted,
                )
                .await?
            }
            A026_WB_ADVERT_DAILY => {
                crate::domain::a026_wb_advert_daily::repository::list_ids_by_period(
                    &request.date_from,
                    &request.date_to,
                    request.only_posted,
                )
                .await?
            }
            A034_YM_REALIZATION => {
                crate::domain::a034_ym_realization::repository::list_ids_by_period(
                    &request.date_from,
                    &request.date_to,
                    request.only_posted,
                )
                .await?
            }
            A013_YM_ORDER => crate::domain::a013_ym_order::repository::list_ids_by_creation_period(
                &request.date_from,
                &request.date_to,
                &[],
                request.only_posted,
            )
            .await?
            .into_iter()
            .map(|(id, _)| id)
            .collect(),
            A016_YM_RETURNS => {
                // Возвраты отбираются по заказам периода (когорта), а не по своей дате.
                let order_numbers: Vec<i64> =
                    crate::domain::a013_ym_order::repository::list_ids_by_creation_period(
                        &request.date_from,
                        &request.date_to,
                        &[],
                        false,
                    )
                    .await?
                    .into_iter()
                    .filter_map(|(_, document_no)| document_no.parse::<i64>().ok())
                    .collect();
                crate::domain::a016_ym_returns::repository::list_ids_by_order_ids(
                    &order_numbers,
                    request.only_posted,
                )
                .await?
            }
            _ => {
                return Err(anyhow!(
                    "Unsupported aggregate_key: {}",
                    request.aggregate_key
                ));
            }
        };

        let total = document_ids.len() as i32;
        self.progress_tracker.set_total(session_id, total);

        let processed = Arc::new(AtomicI32::new(0));
        let reposted = Arc::new(AtomicI32::new(0));

        repost_ids_concurrent(
            &self.progress_tracker,
            session_id,
            &request.aggregate_key,
            document_ids,
            &processed,
            &reposted,
        )
        .await?;

        let final_reposted = reposted.load(Ordering::Relaxed);
        self.progress_tracker
            .update_progress(session_id, total, final_reposted, None);

        let final_status = if self
            .progress_tracker
            .get_progress(session_id)
            .map(|progress| progress.errors > 0)
            .unwrap_or(false)
        {
            RepostStatus::CompletedWithErrors
        } else {
            RepostStatus::Completed
        };

        self.progress_tracker
            .complete_session(session_id, final_status);

        Ok(())
    }

    async fn execute_a012_chunked_repost(
        &self,
        session_id: &str,
        request: &AggregateRepostRequest,
    ) -> Result<()> {
        let connection_labels: std::collections::HashMap<String, String> =
            crate::domain::a006_connection_mp::service::list_all()
                .await?
                .into_iter()
                .map(|connection| {
                    let label = if connection.base.description.trim().is_empty() {
                        connection.base.code.clone()
                    } else {
                        connection.base.description.clone()
                    };
                    (connection.base.id.as_string(), label)
                })
                .collect();

        let chunks =
            crate::domain::a012_wb_sales::repository::list_repost_chunks_by_sale_date_range(
                &request.date_from,
                &request.date_to,
                request.only_posted,
                &request.connection_mp_refs,
            )
            .await?;

        let mut prepared_chunks = Vec::with_capacity(chunks.len());
        let mut total_documents = 0_i32;

        for chunk in chunks {
            let ids =
                crate::domain::a012_wb_sales::repository::list_ids_by_sale_date_and_connection(
                    &chunk.sale_date,
                    &chunk.connection_mp_ref,
                    request.only_posted,
                )
                .await?;
            total_documents += ids.len() as i32;
            prepared_chunks.push((chunk, ids));
        }

        self.progress_tracker.set_total(session_id, total_documents);
        self.progress_tracker
            .set_chunks_total(session_id, prepared_chunks.len() as i32);

        let mut processed = 0_i32;
        let mut reposted = 0_i32;
        let mut chunks_processed = 0_i32;
        let mut day_start = 0_usize;
        while day_start < prepared_chunks.len() {
            let current_day = prepared_chunks[day_start].0.sale_date.clone();
            let mut day_end = day_start;
            while day_end < prepared_chunks.len()
                && prepared_chunks[day_end].0.sale_date == current_day
            {
                day_end += 1;
            }

            let mut posting_cache =
                crate::domain::a012_wb_sales::service::PostingPreparationCache::default();
            let day_document_ids = prepared_chunks[day_start..day_end]
                .iter()
                .flat_map(|(_, ids)| ids.iter().cloned())
                .collect::<Vec<_>>();
            if !day_document_ids.is_empty() {
                let day_documents =
                    crate::domain::a012_wb_sales::repository::list_by_ids(&day_document_ids)
                        .await?;
                crate::domain::a012_wb_sales::service::preload_prod_cost_context_for_documents(
                    &mut posting_cache,
                    &day_documents,
                )
                .await?;
            }

            for (chunk, ids) in &prepared_chunks[day_start..day_end] {
                let cabinet_label = connection_labels
                    .get(&chunk.connection_mp_ref)
                    .cloned()
                    .unwrap_or_else(|| chunk.connection_mp_ref.clone());
                let chunk_label = format!("{} | {}", chunk.sale_date, cabinet_label);
                self.progress_tracker.update_chunk_progress(
                    session_id,
                    chunks_processed,
                    Some(chunk.sale_date.clone()),
                    Some(chunk.connection_mp_ref.clone()),
                    Some(chunk_label.clone()),
                );

                for document_id in ids {
                    let current_item = format!("{} {}", A012_WB_SALES, document_id);
                    self.progress_tracker.update_progress(
                        session_id,
                        processed,
                        reposted,
                        Some(current_item.clone()),
                    );

                    let aggregate_id = match Uuid::parse_str(document_id) {
                        Ok(id) => id,
                        Err(error) => {
                            processed += 1;
                            self.progress_tracker.add_error(
                                session_id,
                                format!("Invalid aggregate id {}: {}", document_id, error),
                            );
                            self.progress_tracker.update_progress(
                                session_id,
                                processed,
                                reposted,
                                Some(current_item),
                            );
                            continue;
                        }
                    };

                    let post_start = std::time::Instant::now();
                    let post_result =
                        crate::domain::a012_wb_sales::posting::post_document_with_cache(
                            aggregate_id,
                            &mut posting_cache,
                        )
                        .await;
                    let elapsed_ms = post_start.elapsed().as_millis() as i64;
                    self.progress_tracker
                        .record_post_timing(session_id, document_id, elapsed_ms);
                    // Порог предупреждения о медленном проведении (диагностика динамики).
                    const SLOW_DOC_MS: i64 = 500;
                    if elapsed_ms > SLOW_DOC_MS {
                        tracing::warn!(
                            "Slow {} post: {} took {} ms",
                            A012_WB_SALES,
                            aggregate_id,
                            elapsed_ms
                        );
                    }
                    match post_result {
                        Ok(()) => reposted += 1,
                        Err(error) => self.progress_tracker.add_error(
                            session_id,
                            format!(
                                "Failed to repost {} {}: {}",
                                A012_WB_SALES, aggregate_id, error
                            ),
                        ),
                    }

                    processed += 1;
                    self.progress_tracker.update_progress(
                        session_id,
                        processed,
                        reposted,
                        Some(current_item),
                    );
                }

                chunks_processed += 1;
                self.progress_tracker.update_chunk_progress(
                    session_id,
                    chunks_processed,
                    Some(chunk.sale_date.clone()),
                    Some(chunk.connection_mp_ref.clone()),
                    Some(chunk_label),
                );
            }

            day_start = day_end;
        }

        self.progress_tracker
            .update_progress(session_id, processed, reposted, None);

        let final_status = if self
            .progress_tracker
            .get_progress(session_id)
            .map(|progress| progress.errors > 0)
            .unwrap_or(false)
        {
            RepostStatus::CompletedWithErrors
        } else {
            RepostStatus::Completed
        };

        self.progress_tracker
            .complete_session(session_id, final_status);

        Ok(())
    }
}

/// Конкурентный репост набора id одного агрегата с ограничением параллелизма.
///
/// Счётчики `processed`/`reposted` — общие атомики, передаются снаружи: одиночный
/// aggregate-repost создаёт их на прогон, а пересбор воронки прокидывает сквозные
/// счётчики через все шаги. SQLite (WAL) сериализует запись, но параллелизм ускоряет
/// CPU-bound вычисления и перекрытие read-фазы (get_by_id, lookups) с write-фазой;
/// `dispatch_aggregate_repost_with_retry` гасит конфликты снапшота при CONCURRENCY > 1.
async fn repost_ids_concurrent(
    tracker: &Arc<ProgressTracker>,
    session_id: &str,
    aggregate_key: &str,
    document_ids: Vec<String>,
    processed: &Arc<AtomicI32>,
    reposted: &Arc<AtomicI32>,
) -> Result<()> {
    const CONCURRENCY: usize = 4;
    let semaphore = Arc::new(tokio::sync::Semaphore::new(CONCURRENCY));
    let session_id_str = session_id.to_string();

    let mut join_set = tokio::task::JoinSet::new();

    for document_id_str in document_ids {
        let aggregate_id = match Uuid::parse_str(&document_id_str) {
            Ok(id) => id,
            Err(error) => {
                tracker.add_error(
                    &session_id_str,
                    format!("Invalid aggregate id {}: {}", document_id_str, error),
                );
                processed.fetch_add(1, Ordering::Relaxed);
                let p = processed.load(Ordering::Relaxed);
                let r = reposted.load(Ordering::Relaxed);
                tracker.update_progress(&session_id_str, p, r, None);
                continue;
            }
        };

        let permit = semaphore
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| anyhow!("Semaphore closed"))?;

        let tracker_task = tracker.clone();
        let sid = session_id_str.clone();
        let agg_key = aggregate_key.to_string();
        let processed_ref = processed.clone();
        let reposted_ref = reposted.clone();

        join_set.spawn(async move {
            let _permit = permit;
            let result = dispatch_aggregate_repost_with_retry(&agg_key, aggregate_id).await;
            let proc_count = processed_ref.fetch_add(1, Ordering::Relaxed) + 1;
            let repo_count = match result {
                Ok(()) => reposted_ref.fetch_add(1, Ordering::Relaxed) + 1,
                Err(error) => {
                    tracker_task.add_error(
                        &sid,
                        format!("Failed to repost {} {}: {}", agg_key, aggregate_id, error),
                    );
                    reposted_ref.load(Ordering::Relaxed)
                }
            };
            tracker_task.update_progress(&sid, proc_count, repo_count, None);
        });
    }

    while let Some(task_result) = join_set.join_next().await {
        task_result.map_err(|e| anyhow!("Task panicked: {}", e))?;
    }

    Ok(())
}

async fn dispatch_repost(registrator_type: &str, registrator_id: Uuid) -> Result<()> {
    match registrator_type {
        "WB_Sales" => crate::domain::a012_wb_sales::posting::post_document(registrator_id).await,
        "OZON_Transactions" => {
            crate::domain::a014_ozon_transactions::posting::post_document(registrator_id).await
        }
        "YM_Order" => crate::domain::a013_ym_order::posting::post_document(registrator_id).await,
        "YM_Returns" => {
            crate::domain::a016_ym_returns::posting::post_document(registrator_id).await
        }
        "OZON_FBS" => {
            crate::domain::a010_ozon_fbs_posting::posting::post_document(registrator_id).await
        }
        "OZON_FBO" => {
            crate::domain::a011_ozon_fbo_posting::posting::post_document(registrator_id).await
        }
        "a021_production_output" => {
            crate::domain::a021_production_output::service::post_document(registrator_id).await
        }
        "a023_purchase_of_goods" => {
            crate::domain::a023_purchase_of_goods::service::post_document(registrator_id).await
        }
        "OZON_Returns" => {
            crate::domain::a009_ozon_returns::posting::post_document(registrator_id).await
        }
        _ => Err(anyhow!(
            "Unsupported registrator_type: {}",
            registrator_type
        )),
    }
}

/// Признак ошибки блокировки SQLite (`SQLITE_BUSY` = 5, `SQLITE_BUSY_SNAPSHOT` = 517).
///
/// `busy_timeout` сериализует ожидание write-lock, но НЕ помогает при конфликте
/// снапшота: SeaORM открывает все транзакции как `BEGIN DEFERRED`, поэтому txn
/// становится читателем на первом SELECT и не может «дорасти» до писателя, если
/// другое соединение успело закоммититься. Единственное корректное лечение —
/// откатить и повторить транзакцию целиком.
fn is_database_locked(error: &anyhow::Error) -> bool {
    let message = format!("{:#}", error);
    message.contains("database is locked")
        || message.contains("(code: 5)")
        || message.contains("(code: 517)")
}

/// Перепроведение одного документа с повторами при блокировке SQLite.
///
/// При CONCURRENCY > 1 параллельные писатели гарантированно ловят конфликты
/// снапшота; джиттер в backoff декоррелирует воркеры, чтобы они не повторяли
/// попытку синхронно.
async fn dispatch_aggregate_repost_with_retry(
    aggregate_key: &str,
    aggregate_id: Uuid,
) -> Result<()> {
    const MAX_ATTEMPTS: u32 = 8;

    let mut attempt = 0u32;
    loop {
        attempt += 1;
        match dispatch_aggregate_repost(aggregate_key, aggregate_id).await {
            Ok(()) => return Ok(()),
            Err(error) if attempt < MAX_ATTEMPTS && is_database_locked(&error) => {
                // Backoff с джиттером: базовая задержка растёт с попыткой,
                // джиттер берём из младших бит UUID (без внешних зависимостей).
                let base_ms = 20u64 * attempt as u64;
                let jitter_ms = (aggregate_id.as_u128() as u64 % 40) + 1;
                tokio::time::sleep(std::time::Duration::from_millis(base_ms + jitter_ms)).await;
            }
            Err(error) => return Err(error),
        }
    }
}

async fn dispatch_aggregate_repost(aggregate_key: &str, aggregate_id: Uuid) -> Result<()> {
    match aggregate_key {
        A012_WB_SALES => crate::domain::a012_wb_sales::posting::post_document(aggregate_id).await,
        A015_WB_ORDERS => crate::domain::a015_wb_orders::posting::post_document(aggregate_id).await,
        A021_PRODUCTION_OUTPUT => {
            crate::domain::a021_production_output::service::post_document(aggregate_id).await
        }
        A023_PURCHASE_OF_GOODS => {
            crate::domain::a023_purchase_of_goods::service::post_document(aggregate_id).await
        }
        A026_WB_ADVERT_DAILY => {
            crate::domain::a026_wb_advert_daily::posting::post_document(aggregate_id).await
        }
        A034_YM_REALIZATION => {
            crate::domain::a034_ym_realization::posting::post_document(aggregate_id).await
        }
        A013_YM_ORDER => crate::domain::a013_ym_order::posting::post_document(aggregate_id).await,
        A016_YM_RETURNS => {
            crate::domain::a016_ym_returns::posting::post_document(aggregate_id).await
        }
        _ => Err(anyhow!("Unsupported aggregate_key: {}", aggregate_key)),
    }
}
