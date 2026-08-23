#[allow(unused_imports)]
use super::wildberries_api_client::WbMarketplaceOrderRow;
use super::{
    processors::{
        commission, document, finance_report, goods_prices, marketplace_order, order, product,
        promotion, sales, supply,
    },
    progress_tracker::ProgressTracker,
    wildberries_api_client::{
        parse_sales_funnel_detail_zip, WbAdvertFullStat, WbAdvertFullStatApp, WbAdvertFullStatDay,
        WbAdvertFullStatNm, WbProductSnapshotRow, WbSalesFunnelDetailRow, WbSalesFunnelHistoryDay,
        WbSalesFunnelHistoryItem, WbSearchQueryRow, WbSearchReportRow, WildberriesApiClient,
    },
};
use crate::domain::a026_wb_advert_daily::posting_context::AdvertPostingContext;
use crate::shared::marketplaces::wildberries::datetime::{wb_day_end_utc, wb_day_start_utc};
use anyhow::{Context, Result};
use contracts::domain::a026_wb_advert_daily::aggregate::{
    WbAdvertDaily, WbAdvertDailyHeader, WbAdvertDailyLine, WbAdvertDailyMetrics,
    WbAdvertDailySourceMeta,
};
use contracts::domain::a030_wb_advert_campaign::aggregate::{
    WbAdvertCampaign, WbAdvertCampaignHeader, WbAdvertCampaignSourceMeta,
};
use contracts::domain::a036_wb_sales_funnel_daily::aggregate::{
    WbSalesFunnelDaily, WbSalesFunnelDailyHeader, WbSalesFunnelDailyLine,
    WbSalesFunnelDailyMetrics, WbSalesFunnelDailySourceMeta,
};
use contracts::domain::a037_wb_product_snapshot::aggregate::{
    WbProductSnapshot, WbProductSnapshotHeader, WbProductSnapshotLine, WbProductSnapshotSourceMeta,
    WbProductSnapshotState, WbProductSnapshotTotals,
};
use contracts::domain::a040_wb_search_analytics_daily::aggregate::{
    WbSearchAnalyticsDaily, WbSearchAnalyticsDailyHeader, WbSearchAnalyticsDailyLine,
    WbSearchAnalyticsDailySourceMeta, WbSearchAnalyticsDailyTotals, WbSearchMetrics,
    WbSearchQueryStat,
};
use contracts::domain::a043_wb_finance_report::{
    WbFinanceReport, WbFinanceReportHeader, WbFinanceReportSourceMeta,
};
use contracts::domain::common::AggregateId;
use contracts::system::tasks::progress::TaskProgress;
use contracts::usecases::u504_import_from_wildberries::{
    progress::ImportStatus,
    request::ImportRequest,
    response::{ImportResponse, ImportStartStatus},
};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::sync::Arc;
use uuid::Uuid;

/// Итог фонового импорта WB для планировщика: `wb_advert_partial_success` — не двигать watermark.
#[derive(Debug, Default, Clone, Copy)]
pub struct ImportRunFlags {
    pub wb_advert_partial_success: bool,
}

#[derive(Default)]
struct AdvertLineAccumulator {
    nm_name: String,
    metrics: WbAdvertDailyMetrics,
    advert_ids: BTreeSet<i64>,
    app_types: BTreeSet<i32>,
    placements: BTreeSet<String>,
}

#[derive(Default)]
struct AdvertDayAccumulator {
    totals: WbAdvertDailyMetrics,
    lines: BTreeMap<i64, AdvertLineAccumulator>,
}

const WB_ADVERT_MIN_REQUEST_INTERVAL_MS: u64 = 250;
const WB_ADVERT_CAMPAIGN_BATCH_SIZE: usize = 50;

fn wb_advert_info_batches(ids: &[i64]) -> std::slice::Chunks<'_, i64> {
    ids.chunks(WB_ADVERT_CAMPAIGN_BATCH_SIZE)
}
const WB_ADVERT_FULLSTATS_CHUNK_DELAY_SECS: u64 = 21;
const WB_ADVERT_FULLSTATS_CHUNK_SIZE: usize = 50;
const WB_ADVERT_RATE_LIMIT_MARKER: &str = "WB Advert API fullstats: 429";

/// Нарезка периода на календарные месяцы для `/adv/v3/fullstats`.
///
/// WB отвергает интервал длиннее 31 дня:
/// `400 {"detail":"max date range 31 days","field":"begin and end"}`.
/// Календарный месяц заведомо укладывается в лимит, а границы окон совпадают с
/// границами месяцев — так повторный запуск за тот же месяц шлёт тот же запрос
/// (стабильный ключ кэша на стороне WB) и данные читаются в логе глазами.
///
/// Первое и последнее окна обрезаются по фактическим границам периода.
fn calendar_month_windows(
    date_from: chrono::NaiveDate,
    date_to: chrono::NaiveDate,
) -> Vec<(chrono::NaiveDate, chrono::NaiveDate)> {
    use chrono::Datelike;

    let mut windows = Vec::new();
    if date_to < date_from {
        return windows;
    }

    let mut cursor = date_from;
    while cursor <= date_to {
        let (next_year, next_month) = if cursor.month() == 12 {
            (cursor.year() + 1, 1)
        } else {
            (cursor.year(), cursor.month() + 1)
        };
        let Some(next_month_start) = chrono::NaiveDate::from_ymd_opt(next_year, next_month, 1)
        else {
            break;
        };
        let Some(month_end) = next_month_start.pred_opt() else {
            break;
        };

        let window_end = month_end.min(date_to);
        windows.push((cursor, window_end));

        let Some(next_cursor) = window_end.succ_opt() else {
            break;
        };
        cursor = next_cursor;
    }

    windows
}

// Sales-funnel (Analytics API v3): лимит 3 запроса/мин на метод.
const WB_SALES_FUNNEL_REQUEST_DELAY_SECS: u64 = 21;
const WB_SALES_FUNNEL_CHUNK_SIZE: usize = 20;
const WB_SALES_FUNNEL_PAGE_LIMIT: usize = 1000;
/// Сколько раз повторить чанк при транзиентной ошибке (обрыв TLS-хэндшейка и т.п.)
/// перед тем как считать его проваленным. Rate limit (429) не ретраится.
const WB_SALES_FUNNEL_MAX_ATTEMPTS: usize = 3;
const WB_DETAIL_HISTORY_POLL_INTERVAL_SECS: u64 = 21;
const WB_DETAIL_HISTORY_MAX_POLLS: usize = 120;

fn is_wb_advert_fullstats_rate_limit(error: &str) -> bool {
    error.contains(WB_ADVERT_RATE_LIMIT_MARKER) || error.contains("429 Too Many Requests")
}

fn is_sqlite_lock_error(error: &anyhow::Error) -> bool {
    let message = format!("{:#}", error).to_ascii_lowercase();
    message.contains("database is locked")
        || message.contains("database table is locked")
        || message.contains("(code: 5)")
        || message.contains("(code: 6)")
        || message.contains("(code: 517)")
}

/// Retry the complete posting operation. SQLITE_BUSY_SNAPSHOT cannot be fixed by
/// busy_timeout: the stale DEFERRED transaction must be rolled back and recreated.
async fn post_wb_advert_document_with_retry(
    document_id: Uuid,
    context: &mut AdvertPostingContext,
) -> Result<()> {
    const MAX_ATTEMPTS: u32 = 8;

    for attempt in 1..=MAX_ATTEMPTS {
        match crate::domain::a026_wb_advert_daily::posting::post_document_with_context(
            document_id,
            context,
        )
        .await
        {
            Ok(()) => return Ok(()),
            Err(error) if attempt < MAX_ATTEMPTS && is_sqlite_lock_error(&error) => {
                let backoff_ms = 50u64 * u64::from(attempt);
                let jitter_ms = (document_id.as_u128() as u64 % 51) + 1;
                let delay_ms = backoff_ms + jitter_ms;
                tracing::warn!(
                    "WB advert auto-post hit SQLite lock: document_id={}, attempt={}/{}, retry_in_ms={}, error={:#}",
                    document_id,
                    attempt,
                    MAX_ATTEMPTS,
                    delay_ms,
                    error
                );
                tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
            }
            Err(error) => return Err(error),
        }
    }

    unreachable!("retry loop always returns on its final attempt")
}

fn extract_wb_rate_limit_retry_seconds(error: &str) -> Option<u64> {
    let marker = "retry=";
    let start = error.find(marker)? + marker.len();
    let rest = &error[start..];
    let digits: String = rest.chars().take_while(|ch| ch.is_ascii_digit()).collect();
    digits.parse().ok()
}

fn normalize_day_date(value: &str) -> String {
    if value.len() >= 10 {
        value[..10].to_string()
    } else {
        value.to_string()
    }
}

fn append_metrics(target: &mut WbAdvertDailyMetrics, source: &WbAdvertDailyMetrics) {
    target.views += source.views;
    target.clicks += source.clicks;
    target.atbs += source.atbs;
    target.orders += source.orders;
    target.shks += source.shks;
    target.sum += source.sum;
    target.sum_price += source.sum_price;
    target.canceled += source.canceled;
}

fn metrics_from_day(day: &WbAdvertFullStatDay) -> WbAdvertDailyMetrics {
    WbAdvertDailyMetrics {
        views: day.views,
        clicks: day.clicks,
        atbs: day.atbs,
        orders: day.orders,
        shks: day.shks,
        sum: day.sum,
        sum_price: day.sum_price,
        canceled: day.canceled,
        ..Default::default()
    }
}

fn metrics_from_nm(nm: &WbAdvertFullStatNm) -> WbAdvertDailyMetrics {
    WbAdvertDailyMetrics {
        views: nm.views,
        clicks: nm.clicks,
        atbs: nm.atbs,
        orders: nm.orders,
        shks: nm.shks,
        sum: nm.sum,
        sum_price: nm.sum_price,
        canceled: nm.canceled,
        ..Default::default()
    }
}

fn finalize_metrics(metrics: &mut WbAdvertDailyMetrics) {
    metrics.ctr = if metrics.views > 0 {
        (metrics.clicks as f64 / metrics.views as f64) * 100.0
    } else {
        0.0
    };
    metrics.cpc = if metrics.clicks > 0 {
        metrics.sum / metrics.clicks as f64
    } else {
        0.0
    };
    metrics.cr = if metrics.clicks > 0 {
        (metrics.orders as f64 / metrics.clicks as f64) * 100.0
    } else {
        0.0
    };
}

fn funnel_metrics_from_day(day: &WbSalesFunnelHistoryDay) -> WbSalesFunnelDailyMetrics {
    WbSalesFunnelDailyMetrics {
        open_count: day.open_count,
        cart_count: day.cart_count,
        order_count: day.order_count,
        order_sum: day.order_sum,
        buyout_count: day.buyout_count,
        buyout_sum: day.buyout_sum,
        cancel_count: day.cancel_count,
        cancel_sum: day.cancel_sum,
        buyout_percent: day.buyout_percent,
        add_to_cart_conversion: day.add_to_cart_conversion,
        cart_to_order_conversion: day.cart_to_order_conversion,
        add_to_wishlist_count: day.add_to_wishlist_count,
    }
}

fn funnel_metrics_is_empty(metrics: &WbSalesFunnelDailyMetrics) -> bool {
    metrics.open_count == 0
        && metrics.cart_count == 0
        && metrics.order_count == 0
        && metrics.order_sum == 0.0
        && metrics.buyout_count == 0
        && metrics.buyout_sum == 0.0
        && metrics.add_to_wishlist_count == 0
        && metrics.cancel_count.unwrap_or(0) == 0
        && metrics.cancel_sum.unwrap_or(0.0) == 0.0
}

/// Накопление опционального счётчика: `None` источника не превращает итог в 0,
/// но первое же `Some` делает итог определённым (N/A ≠ 0).
fn add_optional_i64(target: &mut Option<i64>, source: Option<i64>) {
    if let Some(value) = source {
        *target = Some(target.unwrap_or(0) + value);
    }
}

fn add_optional_f64(target: &mut Option<f64>, source: Option<f64>) {
    if let Some(value) = source {
        *target = Some(target.unwrap_or(0.0) + value);
    }
}

fn append_funnel_totals(
    target: &mut WbSalesFunnelDailyMetrics,
    source: &WbSalesFunnelDailyMetrics,
) {
    target.open_count += source.open_count;
    target.cart_count += source.cart_count;
    target.order_count += source.order_count;
    target.order_sum += source.order_sum;
    target.buyout_count += source.buyout_count;
    target.buyout_sum += source.buyout_sum;
    target.add_to_wishlist_count += source.add_to_wishlist_count;
    add_optional_i64(&mut target.cancel_count, source.cancel_count);
    add_optional_f64(&mut target.cancel_sum, source.cancel_sum);
}

/// Производные проценты итогов пересчитываются от сумм (не усредняются по строкам).
fn finalize_funnel_totals(totals: &mut WbSalesFunnelDailyMetrics) {
    totals.add_to_cart_conversion = if totals.open_count > 0 {
        (totals.cart_count as f64 / totals.open_count as f64) * 100.0
    } else {
        0.0
    };
    totals.cart_to_order_conversion = if totals.cart_count > 0 {
        (totals.order_count as f64 / totals.cart_count as f64) * 100.0
    } else {
        0.0
    };
    totals.buyout_percent = if totals.order_count > 0 {
        (totals.buyout_count as f64 / totals.order_count as f64) * 100.0
    } else {
        0.0
    };
}

fn wb_product_enrichment_score(
    product: &contracts::domain::a007_marketplace_product::aggregate::MarketplaceProduct,
) -> usize {
    usize::from(product.nomenclature_ref.is_some()) * 8
        + usize::from(!product.base.description.trim().is_empty()) * 4
        + usize::from(!product.article.trim().is_empty()) * 4
        + usize::from(
            product
                .brand
                .as_deref()
                .is_some_and(|v| !v.trim().is_empty()),
        ) * 2
        + usize::from(
            product
                .category_id
                .as_deref()
                .is_some_and(|v| !v.trim().is_empty()),
        )
        + usize::from(
            product
                .category_name
                .as_deref()
                .is_some_and(|v| !v.trim().is_empty()),
        )
        + usize::from(!product.base.code.starts_with("WB-AUTO-")) * 2
}

/// Выбирает наиболее полную запись a007. Если дубли ссылаются на разные
/// номенклатуры 1С, атрибуты товара использовать можно, но ссылку оставляем
/// пустой, чтобы не провести факты по неоднозначной номенклатуре.
fn select_wb_product_enrichment(
    mut candidates: Vec<contracts::domain::a007_marketplace_product::aggregate::MarketplaceProduct>,
) -> (
    contracts::domain::a007_marketplace_product::aggregate::MarketplaceProduct,
    bool,
) {
    debug_assert!(!candidates.is_empty());
    candidates.sort_by(|left, right| {
        wb_product_enrichment_score(right)
            .cmp(&wb_product_enrichment_score(left))
            .then_with(|| {
                right
                    .base
                    .metadata
                    .updated_at
                    .cmp(&left.base.metadata.updated_at)
            })
            .then_with(|| left.to_string_id().cmp(&right.to_string_id()))
    });

    let nomenclature_refs: BTreeSet<String> = candidates
        .iter()
        .filter_map(|product| product.nomenclature_ref.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .collect();
    let has_conflicting_nomenclature = nomenclature_refs.len() > 1;
    let resolved_nomenclature = if has_conflicting_nomenclature {
        None
    } else {
        nomenclature_refs.into_iter().next()
    };

    let mut selected = candidates.remove(0);
    selected.nomenclature_ref = resolved_nomenclature;
    (selected, has_conflicting_nomenclature)
}

/// Executor для UseCase импорта из Wildberries
pub struct ImportExecutor {
    api_client: Arc<WildberriesApiClient>,
    pub progress_tracker: Arc<ProgressTracker>,
}

impl ImportExecutor {
    pub fn new(progress_tracker: Arc<ProgressTracker>) -> Self {
        Self {
            api_client: Arc::new(WildberriesApiClient::new()),
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
                "a015_wb_orders" => "Заказы Wildberries (Backfill)",
                "a012_wb_sales" => "Продажи Wildberries",
                "p903_wb_finance_report" => "Финансовый отчет WB",
                "a043_wb_finance_report" => "Финансовые отчёты WB (Finance API v1)",
                "p905_wb_commission_history" => "История комиссий WB",
                "p908_wb_goods_prices" => "Цены товаров WB",
                "a020_wb_promotion" => "Акции WB (Календарь)",
                "a030_wb_advert_campaign" => "Рекламные кампании WB",
                "wb_advert_stats" | "wb_advert_stats_csv" => "Статистика рекламных кампаний WB",
                "a032_wb_returns_claims" => "Заявки на возврат WB",
                "a036_wb_sales_funnel_daily" => "Воронка продаж WB",
                "a036_wb_sales_funnel_daily_history" => "История воронки продаж WB (CSV)",
                "a037_wb_product_snapshot" => "Данные по товарам WB",
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
    ) -> Option<contracts::usecases::u504_import_from_wildberries::progress::ImportProgress> {
        self.progress_tracker.get_progress(session_id)
    }

    /// Подписи агрегатов (как в `start_import` + все ветки `execute_import`).
    fn wb_aggregate_display_name(aggregate_index: &str) -> &'static str {
        match aggregate_index {
            "a007_marketplace_product" => "Товары маркетплейса",
            "a015_wb_orders" => "Заказы Wildberries (Backfill)",
            "a012_wb_sales" => "Продажи Wildberries",
            "p903_wb_finance_report" => "Финансовый отчет WB",
            "a043_wb_finance_report" => "Финансовые отчёты WB (Finance API v1)",
            "p905_wb_commission_history" => "История комиссий WB",
            "p908_wb_goods_prices" => "Цены товаров WB",
            "a020_wb_promotion" => "Акции WB (Календарь)",
            "a030_wb_advert_campaign" => "Рекламные кампании WB",
            "wb_advert_stats" | "wb_advert_stats_csv" => "Статистика рекламных кампаний WB",
            "a027_wb_documents" => "Документы WB",
            "a029_wb_supply" => "Поставки WB",
            "a015_wb_orders_new" => "Новые заказы WB (оперативно)",
            "a015_wb_orders_supply_link" => "Связь заказов с поставками",
            "a032_wb_returns_claims" => "Заявки на возврат WB",
            "a036_wb_sales_funnel_daily" => "Воронка продаж WB",
            "a036_wb_sales_funnel_daily_history" => "История воронки продаж WB (CSV)",
            "a037_wb_product_snapshot" => "Данные по товарам WB",
            _ => "Unknown",
        }
    }

    /// Регламентные задачи вызывают `execute_import` напрямую — сессия в трекере должна существовать,
    /// иначе `get_progress` после завершения вернёт `None` и в `sys_task_runs` не попадут метрики.
    fn ensure_progress_session(&self, session_id: &str, request: &ImportRequest) {
        if self.progress_tracker.get_progress(session_id).is_some() {
            return;
        }
        self.progress_tracker.create_session(session_id.to_string());
        for aggregate_index in &request.target_aggregates {
            self.progress_tracker.add_aggregate(
                session_id,
                aggregate_index.clone(),
                Self::wb_aggregate_display_name(aggregate_index).to_string(),
            );
        }
    }

    /// Выполнить импорт.
    /// Гарантирует вызов `complete_session` в трекере при любом исходе —
    /// даже если внутренний шаг вернул ошибку.
    pub async fn execute_import(
        &self,
        session_id: &str,
        request: &ImportRequest,
        connection: &contracts::domain::a006_connection_mp::aggregate::ConnectionMP,
    ) -> Result<ImportRunFlags> {
        tracing::info!("Starting Wildberries import for session: {}", session_id);

        self.ensure_progress_session(session_id, request);

        let _http_tracking = self
            .api_client
            .bind_http_tracking(Arc::clone(&self.progress_tracker), session_id.to_string());
        let work_result = self.run_aggregates(session_id, request, connection).await;

        // Трекер ВСЕГДА получает финальный статус — не только в happy path.
        let final_status = match &work_result {
            Err(e) => {
                let error_message = e.to_string();
                // {:#} печатает всю цепочку anyhow (контекст + первопричина),
                // иначе текст reqwest-ошибки/ответа WB теряется под верхним контекстом.
                self.progress_tracker.add_error(
                    session_id,
                    None,
                    format!("Import failed: {:#}", e),
                    None,
                );
                if error_message.starts_with("WB_RATE_LIMIT_DEFERRED:") {
                    ImportStatus::CompletedWithErrors
                } else {
                    ImportStatus::Failed
                }
            }
            Ok(flags) => {
                let tracker_errors = self
                    .progress_tracker
                    .get_progress(session_id)
                    .map(|p| p.total_errors > 0)
                    .unwrap_or(false);
                if flags.wb_advert_partial_success || tracker_errors {
                    ImportStatus::CompletedWithErrors
                } else {
                    ImportStatus::Completed
                }
            }
        };

        let clean = matches!(final_status, ImportStatus::Completed);
        self.progress_tracker
            .complete_session(session_id, final_status);
        tracing::info!("Import completed for session: {}", session_id);

        // Факт для механизма Процессов (ADR-0011 п.5). Публикуется вручную и
        // здесь, а не «на всякий случай» из репозитория: смысл «день собран»
        // знает импорт, а не таблица.
        if clean {
            Self::publish_completed_days(request).await;
        }

        work_result
    }

    /// Опубликовать `import.day.completed` по дням завершённого импорта.
    ///
    /// Три условия, и каждое сужает факт до того, за что можно ручаться:
    ///
    /// - импорт закончился **чисто** (`Completed`, не `CompletedWithErrors`):
    ///   день, собранный наполовину, закрывать нельзя;
    /// - в задании были **продажи** (`a012_wb_sales`): снимок дня строится из
    ///   них, и без них «день собран» ничего не значит;
    /// - **сегодняшний день не публикуется**: он ещё накапливается.
    ///
    /// Ошибка публикации не роняет импорт: событие — производная от уже
    /// состоявшейся работы, и терять импорт из-за него нельзя.
    async fn publish_completed_days(request: &ImportRequest) {
        use contracts::processes::{CorrelationKey, DomainEventKind};

        if !request
            .target_aggregates
            .iter()
            .any(|aggregate| aggregate == "a012_wb_sales")
        {
            return;
        }

        let today = chrono::Utc::now().naive_utc().date();
        let db = crate::shared::data::db::get_connection();
        let mut day = request.date_from;
        // Потолок на всякий случай: импорт за год не должен превращаться в
        // сотни экземпляров одним махом.
        let mut published = 0;
        while day <= request.date_to && published < 62 {
            if day < today {
                let key = CorrelationKey::new()
                    .with("connection_id", request.connection_id.clone())
                    .with("business_date", day.format("%Y-%m-%d").to_string());
                if let Err(error) = crate::processes::events::publish(
                    db,
                    DomainEventKind::ImportDayCompleted,
                    key,
                    serde_json::json!({ "source_usecase": "u504" }),
                    "u504",
                )
                .await
                {
                    tracing::warn!("import.day.completed не опубликован за {day}: {error}");
                }
                published += 1;
            }
            day += chrono::Duration::days(1);
        }
    }

    /// Внутренний цикл по агрегатам; возвращает первую ошибку без изменения трекера.
    async fn run_aggregates(
        &self,
        session_id: &str,
        request: &ImportRequest,
        connection: &contracts::domain::a006_connection_mp::aggregate::ConnectionMP,
    ) -> Result<ImportRunFlags> {
        let mut flags = ImportRunFlags::default();
        for aggregate_index in &request.target_aggregates {
            match aggregate_index.as_str() {
                "a007_marketplace_product" => {
                    self.import_marketplace_products(session_id, connection)
                        .await?;
                }
                "a012_wb_sales" => {
                    self.import_wb_sales(
                        session_id,
                        connection,
                        request.date_from,
                        request.date_to,
                    )
                    .await?;
                }
                "a015_wb_orders" => {
                    self.import_wb_orders(
                        session_id,
                        connection,
                        request.date_from,
                        request.date_to,
                    )
                    .await?;
                }
                "p903_wb_finance_report" => {
                    self.import_wb_finance_report(
                        session_id,
                        connection,
                        request.date_from,
                        request.date_to,
                    )
                    .await?;
                }
                "a043_wb_finance_report" => {
                    self.import_wb_finance_report_v1(
                        session_id,
                        connection,
                        request.date_from,
                        request.date_to,
                    )
                    .await?;
                }
                "p905_wb_commission_history" => {
                    self.import_commission_history(session_id, connection)
                        .await?;
                }
                "p908_wb_goods_prices" => {
                    self.import_wb_goods_prices(session_id, connection).await?;
                }
                "a020_wb_promotion" => {
                    self.import_wb_promotions(
                        session_id,
                        connection,
                        request.date_from,
                        request.date_to,
                    )
                    .await?;
                }
                "a027_wb_documents" => {
                    self.import_wb_documents(
                        session_id,
                        connection,
                        request.date_from,
                        request.date_to,
                    )
                    .await?;
                }
                "a029_wb_supply" => {
                    self.import_wb_supplies(
                        session_id,
                        connection,
                        request.date_from,
                        request.date_to,
                    )
                    .await?;
                }
                "a030_wb_advert_campaign" => {
                    self.import_wb_advert_campaigns(session_id, connection)
                        .await?;
                }
                "wb_advert_stats" | "wb_advert_stats_csv" => {
                    let partial = self
                        .import_wb_advert_stats(
                            session_id,
                            connection,
                            request.date_from,
                            request.date_to,
                        )
                        .await?;
                    if partial {
                        flags.wb_advert_partial_success = true;
                    }
                }
                "a015_wb_orders_new" => {
                    self.import_wb_new_marketplace_orders(
                        session_id,
                        connection,
                        request.date_from,
                        request.date_to,
                    )
                    .await?;
                }
                "a015_wb_orders_supply_link" => {
                    tracing::info!(
                        "Aggregate a015_wb_orders_supply_link is deprecated; delegating to a029_wb_supply import"
                    );
                    self.import_wb_supplies(
                        session_id,
                        connection,
                        request.date_from,
                        request.date_to,
                    )
                    .await?;
                }
                "a032_wb_returns_claims" => {
                    self.import_wb_returns_claims(session_id, connection)
                        .await?;
                }
                "a036_wb_sales_funnel_daily" => {
                    self.import_wb_sales_funnel(
                        session_id,
                        connection,
                        request.date_from,
                        request.date_to,
                    )
                    .await?;
                }
                "a036_wb_sales_funnel_daily_history" => {
                    self.import_wb_sales_funnel_history_report(
                        session_id,
                        connection,
                        request.date_from,
                        request.date_to,
                    )
                    .await?;
                }
                "a037_wb_product_snapshot" => {
                    self.import_wb_product_snapshot(
                        session_id,
                        connection,
                        request.date_from,
                        request.date_to,
                    )
                    .await?;
                }
                "a040_wb_search_analytics_daily" => {
                    self.import_wb_search_analytics(
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
        Ok(flags)
    }

    /// Импорт товаров из Wildberries
    async fn import_marketplace_products(
        &self,
        session_id: &str,
        connection: &contracts::domain::a006_connection_mp::aggregate::ConnectionMP,
    ) -> Result<()> {
        tracing::info!("Importing marketplace products for session: {}", session_id);

        let aggregate_index = "a007_marketplace_product";
        let page_size: i32 = 100;
        let mut total_processed = 0;
        let mut total_inserted = 0;
        let mut total_updated = 0;
        let mut cursor: Option<super::wildberries_api_client::WildberriesCursor> = None;

        // Получаем товары страницами через Wildberries API.
        // WB v2 не сообщает «общее число карточек в кабинете»: cursor.total в ответе —
        // это количество карточек в текущей странице. Поэтому останавливаемся только
        // по сигналам «карточек нет» / «страница неполная».
        loop {
            let list_response = self
                .api_client
                .fetch_product_list(connection, page_size, cursor.clone())
                .await?;

            let response_cursor = list_response.cursor.clone();
            let cards = list_response.cards;
            let batch_size = cards.len();

            if cards.is_empty() {
                break;
            }

            // Обрабатываем каждый товар
            for card in cards {
                let product_name = card
                    .title
                    .clone()
                    .unwrap_or_else(|| "Без названия".to_string());
                let display_name = format!("{} - {}", card.nm_id, product_name);

                self.progress_tracker.set_current_item(
                    session_id,
                    aggregate_index,
                    Some(display_name),
                );

                match product::process_product(connection, &card).await {
                    Ok(is_new) => {
                        total_processed += 1;
                        if is_new {
                            total_inserted += 1;
                        } else {
                            total_updated += 1;
                        }
                    }
                    Err(e) => {
                        tracing::error!("Failed to process product {}: {}", card.nm_id, e);
                        self.progress_tracker.add_error(
                            session_id,
                            Some(aggregate_index.to_string()),
                            format!("Failed to process product {}", card.nm_id),
                            Some(e.to_string()),
                        );
                    }
                }

                // Обновить прогресс (общий total в кабинете неизвестен → None)
                self.progress_tracker.update_aggregate(
                    session_id,
                    aggregate_index,
                    total_processed,
                    None,
                    total_inserted,
                    total_updated,
                );
            }

            // Очистить текущий элемент после страницы
            self.progress_tracker
                .set_current_item(session_id, aggregate_index, None);

            // Если страница неполная — это последняя.
            if batch_size < page_size as usize {
                break;
            }

            // Иначе продолжаем пагинацию с курсором из ответа (updatedAt + nmID последней карточки).
            cursor = Some(response_cursor);
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

    /// Импорт продаж из Wildberries API в a012_wb_sales
    async fn import_wb_sales(
        &self,
        session_id: &str,
        connection: &contracts::domain::a006_connection_mp::aggregate::ConnectionMP,
        date_from: chrono::NaiveDate,
        date_to: chrono::NaiveDate,
    ) -> Result<()> {
        use crate::domain::a002_organization;

        let aggregate_index = "a012_wb_sales";
        let mut total_processed = 0;
        let mut total_inserted = 0;
        let mut total_updated = 0;

        tracing::info!(
            "Importing WB sales for session: {} from date: {} to date: {}",
            session_id,
            date_from,
            date_to
        );

        self.progress_tracker
            .update_aggregate(session_id, aggregate_index, 0, None, 0, 0);
        self.progress_tracker.set_current_item(
            session_id,
            aggregate_index,
            Some(format!(
                "Запрос WB Statistics /api/v1/supplier/sales за период {}..{}",
                date_from, date_to
            )),
        );

        // Получаем ID организации по UUID-ссылке из подключения
        let organization_id = match Uuid::parse_str(&connection.organization_ref) {
            Ok(org_uuid) => match a002_organization::service::get_by_id(
                crate::shared::data::db::get_connection(),
                org_uuid,
            )
            .await?
            {
                Some(org) => org.base.id.as_string(),
                None => {
                    let error_msg = format!(
                        "Организация с UUID '{}' не найдена в справочнике",
                        connection.organization_ref
                    );
                    tracing::error!("{}", error_msg);
                    self.progress_tracker.fail_aggregate(
                        session_id,
                        aggregate_index,
                        error_msg.clone(),
                    );
                    anyhow::bail!("{}", error_msg);
                }
            },
            Err(_) => {
                let error_msg = format!(
                    "Некорректный organization_ref UUID в подключении: '{}'",
                    connection.organization_ref
                );
                tracing::error!("{}", error_msg);
                self.progress_tracker.fail_aggregate(
                    session_id,
                    aggregate_index,
                    error_msg.clone(),
                );
                anyhow::bail!("{}", error_msg);
            }
        };

        // Получаем продажи из API WB
        let sales_rows = match self
            .api_client
            .fetch_sales(connection, date_from, date_to)
            .await
        {
            Ok(rows) => rows,
            Err(e) => {
                let error_msg = format!(
                    "Не удалось получить продажи WB за период {}..{}: {}",
                    date_from, date_to, e
                );
                self.progress_tracker.fail_aggregate(
                    session_id,
                    aggregate_index,
                    error_msg.clone(),
                );
                anyhow::bail!("{}", error_msg);
            }
        };

        tracing::info!("Received {} sale rows from WB API", sales_rows.len());
        let sales_total = sales_rows.len() as i32;
        self.progress_tracker.set_current_item(
            session_id,
            aggregate_index,
            Some(format!("Обработка продаж WB: {}", sales_total)),
        );
        self.progress_tracker.update_aggregate(
            session_id,
            aggregate_index,
            0,
            Some(sales_total),
            0,
            0,
        );

        // Pre-load existing sale_ids in one batch query to avoid per-row SELECTs.
        let all_sale_ids: Vec<String> = sales_rows
            .iter()
            .filter_map(|(row, _)| row.sale_id.clone())
            .collect();
        let existing_sale_ids =
            crate::domain::a012_wb_sales::repository::list_existing_sale_ids(&all_sale_ids)
                .await
                .unwrap_or_default();

        // Shared cache across all rows — avoids repeating product/org/price lookups.
        let mut shared_cache =
            crate::domain::a012_wb_sales::service::PostingPreparationCache::default();

        // Обрабатываем каждую продажу
        for (sale_row, raw_json) in sales_rows {
            match sales::process_sale_row(
                connection,
                &organization_id,
                &sale_row,
                &raw_json,
                &existing_sale_ids,
                &mut shared_cache,
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
                    tracing::error!("Failed to process WB sale: {}", e);
                    self.progress_tracker.add_error(
                        session_id,
                        Some(aggregate_index.to_string()),
                        "Failed to process WB sale".to_string(),
                        Some(e.to_string()),
                    );
                }
            }

            self.progress_tracker.update_aggregate(
                session_id,
                aggregate_index,
                total_processed,
                Some(sales_total),
                total_inserted,
                total_updated,
            );
        }

        self.progress_tracker
            .set_current_item(session_id, aggregate_index, None);
        self.progress_tracker
            .complete_aggregate(session_id, aggregate_index);

        tracing::info!(
            "WB sales import completed: processed={}, inserted={}, updated={}",
            total_processed,
            total_inserted,
            total_updated
        );

        Ok(())
    }

    /// Импорт заказов из Wildberries API в a015_wb_orders
    async fn import_wb_orders(
        &self,
        session_id: &str,
        connection: &contracts::domain::a006_connection_mp::aggregate::ConnectionMP,
        date_from: chrono::NaiveDate,
        date_to: chrono::NaiveDate,
    ) -> Result<()> {
        use crate::domain::a002_organization;

        let aggregate_index = "a015_wb_orders";
        let mut total_processed = 0;
        let mut total_inserted = 0;
        let mut total_updated = 0;

        tracing::info!(
            "Importing WB orders for session: {} from date: {} to date: {}",
            session_id,
            date_from,
            date_to
        );

        // Получаем ID организации по UUID-ссылке из подключения
        let organization_id = match Uuid::parse_str(&connection.organization_ref) {
            Ok(org_uuid) => match a002_organization::service::get_by_id(
                crate::shared::data::db::get_connection(),
                org_uuid,
            )
            .await?
            {
                Some(org) => org.base.id.as_string(),
                None => {
                    let error_msg = format!(
                        "Организация с UUID '{}' не найдена в справочнике",
                        connection.organization_ref
                    );
                    tracing::error!("{}", error_msg);
                    self.progress_tracker.add_error(
                        session_id,
                        Some(aggregate_index.to_string()),
                        error_msg.clone(),
                        None,
                    );
                    anyhow::bail!("{}", error_msg);
                }
            },
            Err(_) => {
                let error_msg = format!(
                    "Некорректный organization_ref UUID в подключении: '{}'",
                    connection.organization_ref
                );
                tracing::error!("{}", error_msg);
                self.progress_tracker.add_error(
                    session_id,
                    Some(aggregate_index.to_string()),
                    error_msg.clone(),
                    None,
                );
                anyhow::bail!("{}", error_msg);
            }
        };

        // Получаем заказы из API WB
        self.progress_tracker.update_aggregate(
            session_id,
            aggregate_index,
            total_processed,
            None,
            total_inserted,
            total_updated,
        );
        self.progress_tracker.set_current_item(
            session_id,
            aggregate_index,
            Some(format!(
                "WB Orders API: загрузка заказов за период {} - {}",
                date_from, date_to
            )),
        );

        let order_rows = self
            .api_client
            .fetch_orders(connection, date_from, date_to)
            .await?;

        tracing::info!("Received {} order rows from WB API", order_rows.len());
        let orders_total = order_rows.len() as i32;
        self.progress_tracker.set_current_item(
            session_id,
            aggregate_index,
            Some(format!("Обработка заказов WB: {}", orders_total)),
        );
        self.progress_tracker.update_aggregate(
            session_id,
            aggregate_index,
            total_processed,
            Some(orders_total),
            total_inserted,
            total_updated,
        );

        // Обрабатываем каждый заказ
        for order_row in order_rows {
            match order::process_order_row(connection, &organization_id, &order_row).await {
                Ok(is_new) => {
                    total_processed += 1;
                    if is_new {
                        total_inserted += 1;
                    } else {
                        total_updated += 1;
                    }
                }
                Err(e) => {
                    tracing::error!("Failed to process WB order: {}", e);
                    self.progress_tracker.add_error(
                        session_id,
                        Some(aggregate_index.to_string()),
                        "Failed to process WB order".to_string(),
                        Some(e.to_string()),
                    );
                }
            }

            self.progress_tracker.update_aggregate(
                session_id,
                aggregate_index,
                total_processed,
                Some(orders_total),
                total_inserted,
                total_updated,
            );
        }

        self.progress_tracker
            .set_current_item(session_id, aggregate_index, None);
        self.progress_tracker
            .complete_aggregate(session_id, aggregate_index);

        tracing::info!(
            "WB orders import completed: processed={}, inserted={}, updated={}",
            total_processed,
            total_inserted,
            total_updated
        );

        Ok(())
    }

    async fn import_wb_supplies(
        &self,
        session_id: &str,
        connection: &contracts::domain::a006_connection_mp::aggregate::ConnectionMP,
        date_from: chrono::NaiveDate,
        date_to: chrono::NaiveDate,
    ) -> Result<()> {
        use crate::domain::a002_organization;

        let aggregate_index = "a029_wb_supply";
        let mut total_processed = 0;
        let mut total_inserted = 0;
        let mut total_updated = 0;

        tracing::info!(
            "Importing WB supplies for session: {} from {} to {}",
            session_id,
            date_from,
            date_to
        );

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
                        "Организация с UUID '{}' не найдена",
                        connection.organization_ref
                    );
                    tracing::error!("{}", msg);
                    self.progress_tracker.add_error(
                        session_id,
                        Some(aggregate_index.to_string()),
                        msg.clone(),
                        None,
                    );
                    anyhow::bail!("{}", msg);
                }
            },
            Err(_) => {
                let msg = format!(
                    "Некорректный organization_ref UUID: '{}'",
                    connection.organization_ref
                );
                tracing::error!("{}", msg);
                self.progress_tracker.add_error(
                    session_id,
                    Some(aggregate_index.to_string()),
                    msg.clone(),
                    None,
                );
                anyhow::bail!("{}", msg);
            }
        };

        let supply_rows = self
            .api_client
            .fetch_supplies(connection, date_from, date_to)
            .await?;

        tracing::info!("Received {} supply rows from WB API", supply_rows.len());

        for supply_row in supply_rows {
            let income_id_opt = supply_row
                .id
                .rsplit('-')
                .next()
                .and_then(|s| s.parse::<i64>().ok());

            let (supply_order_ids_loaded, supply_order_ids) = match self
                .api_client
                .fetch_supply_order_ids(connection, &supply_row.id)
                .await
            {
                Ok(order_ids) => {
                    tracing::info!(
                        "Supply {}: fetched {} order ids from WB API",
                        supply_row.id,
                        order_ids.len()
                    );
                    (true, order_ids)
                }
                Err(e) => {
                    tracing::warn!(
                        "Failed to fetch order ids for supply {} (done={}): {}",
                        supply_row.id,
                        supply_row.done.unwrap_or(false),
                        e
                    );
                    (false, vec![])
                }
            };

            tracing::info!(
                "Preparing enrichment for supply {} (done={})",
                supply_row.id,
                supply_row.done.unwrap_or(false)
            );

            if supply_order_ids_loaded {
                if let Some(income_id) = income_id_opt {
                    if let Err(e) = self
                        .sync_a015_supply_links_for_supply(
                            &supply_row.id,
                            income_id,
                            &supply_order_ids,
                        )
                        .await
                    {
                        tracing::warn!(
                            "Supply {}: failed to sync a015 supply links: {}",
                            supply_row.id,
                            e
                        );
                    }
                }
            }

            let stat_orders_fallback = if let Some(income_id) = income_id_opt {
                match crate::domain::a015_wb_orders::service::list_by_income_id(income_id).await {
                    Ok(orders) => {
                        tracing::info!(
                            "Supply {}: found {} orders via a015 income_id={}",
                            supply_row.id,
                            orders.len(),
                            income_id
                        );
                        orders
                    }
                    Err(e) => {
                        tracing::warn!(
                            "Supply {}: a015 enrichment failed for income_id={}: {}",
                            supply_row.id,
                            income_id,
                            e
                        );
                        vec![]
                    }
                }
            } else {
                vec![]
            };

            let sticker_order_ids: Vec<i64> = supply_order_ids.clone();

            let sticker_rows = if sticker_order_ids.is_empty() {
                vec![]
            } else {
                match self
                    .api_client
                    .fetch_order_stickers(connection, &sticker_order_ids, "zplv", 58, 40)
                    .await
                {
                    Ok(mut stickers) => {
                        for sticker in &mut stickers {
                            sticker.file = None;
                        }
                        tracing::info!(
                            "Supply {}: fetched {} stickers from WB API",
                            supply_row.id,
                            stickers.len()
                        );
                        stickers
                    }
                    Err(e) => {
                        tracing::warn!(
                            "Failed to fetch stickers for supply {} ({} order ids): {}",
                            supply_row.id,
                            sticker_order_ids.len(),
                            e
                        );
                        vec![]
                    }
                }
            };

            match supply::process_supply_row(
                connection,
                &organization_id,
                &supply_row,
                supply_order_ids,
                sticker_rows,
                stat_orders_fallback,
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
                    tracing::error!("Failed to process WB supply {}: {}", supply_row.id, e);
                    self.progress_tracker.add_error(
                        session_id,
                        Some(aggregate_index.to_string()),
                        format!("Failed to process supply {}", supply_row.id),
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

            // Brief pause between supply order fetches
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        }

        self.progress_tracker
            .complete_aggregate(session_id, aggregate_index);

        tracing::info!(
            "WB supplies import completed: processed={}, inserted={}, updated={}",
            total_processed,
            total_inserted,
            total_updated
        );

        Ok(())
    }

    async fn sync_a015_supply_links_for_supply(
        &self,
        supply_id: &str,
        income_id: i64,
        current_order_ids: &[i64],
    ) -> Result<()> {
        use crate::domain::a015_wb_orders::service as orders_service;

        let current_order_ids: HashSet<i64> = current_order_ids
            .iter()
            .copied()
            .filter(|&order_id| order_id > 0)
            .collect();

        let currently_linked_orders = orders_service::list_by_income_id(income_id).await?;
        for order in &currently_linked_orders {
            let numeric_order_id = order.line.line_id.parse::<i64>().unwrap_or(0);
            if numeric_order_id > 0 && !current_order_ids.contains(&numeric_order_id) {
                orders_service::set_income_id_by_document_no(&order.header.document_no, None)
                    .await?;
            }
        }

        if current_order_ids.is_empty() {
            tracing::info!(
                "Supply {}: cleared links for income_id={} because WB returned no current orders",
                supply_id,
                income_id
            );
            return Ok(());
        }

        let known_orders = orders_service::list_by_numeric_order_ids(
            &current_order_ids.iter().copied().collect::<Vec<_>>(),
        )
        .await?;

        for order in known_orders {
            let current_income_id = order.source_meta.income_id.filter(|&value| value > 0);
            if current_income_id != Some(income_id) {
                orders_service::set_income_id_by_document_no(
                    &order.header.document_no,
                    Some(income_id),
                )
                .await?;
            }
        }

        Ok(())
    }

    /// Imports new FBS orders from Marketplace API for real-time order visibility.
    ///
    /// Flow:
    /// 1. GET /api/v3/orders/new — brand-new orders (status "waiting", not yet in supply)
    /// 2. GET /api/v3/orders?dateFrom=... — recent orders including those already in supplies
    ///
    /// For each order:
    /// - If not in a015 yet → INSERT with partial data (no financial fields)
    /// - If already in a015 → update income_id if supplyId is now known
    ///
    /// Statistics API (Backfill) should run separately to fill financial/analytics fields.
    async fn import_wb_new_marketplace_orders(
        &self,
        session_id: &str,
        connection: &contracts::domain::a006_connection_mp::aggregate::ConnectionMP,
        date_from: chrono::NaiveDate,
        date_to: chrono::NaiveDate,
    ) -> Result<()> {
        use crate::domain::a002_organization;

        let aggregate_index = "a015_wb_orders_new";
        let mut total_processed = 0i32;
        let mut total_inserted = 0i32;
        let mut total_updated = 0i32;
        let mut had_fetch_error = false;

        self.progress_tracker.add_aggregate(
            session_id,
            aggregate_index.to_string(),
            "Новые заказы WB (Оперативно)".to_string(),
        );
        self.progress_tracker
            .update_aggregate(session_id, aggregate_index, 0, Some(2), 0, 0);

        // Step 1: fetch brand-new orders
        self.progress_tracker.set_current_item(
            session_id,
            aggregate_index,
            Some("Запрос WB /api/v3/orders/new".to_string()),
        );
        let new_orders = match self
            .api_client
            .fetch_new_marketplace_orders(connection)
            .await
        {
            Ok(orders) => orders,
            Err(e) => {
                let msg = format!("Не удалось получить WB /api/v3/orders/new: {}", e);
                tracing::warn!("{}", msg);
                self.progress_tracker.add_error(
                    session_id,
                    Some(aggregate_index.to_string()),
                    msg,
                    None,
                );
                had_fetch_error = true;
                vec![]
            }
        };
        tracing::info!("New marketplace orders (/new): {}", new_orders.len());
        self.progress_tracker
            .update_aggregate(session_id, aggregate_index, 1, Some(2), 0, 0);

        // Step 2: fetch recent orders in date range (includes supplyId for assigned orders)
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
                        "Организация с UUID '{}' не найдена",
                        connection.organization_ref
                    );
                    tracing::error!("{}", msg);
                    self.progress_tracker.add_error(
                        session_id,
                        Some(aggregate_index.to_string()),
                        msg.clone(),
                        None,
                    );
                    anyhow::bail!("{}", msg);
                }
            },
            Err(_) => {
                let msg = format!(
                    "Некорректный organization_ref UUID: '{}'",
                    connection.organization_ref
                );
                tracing::error!("{}", msg);
                self.progress_tracker.add_error(
                    session_id,
                    Some(aggregate_index.to_string()),
                    msg.clone(),
                    None,
                );
                anyhow::bail!("{}", msg);
            }
        };

        let date_from_ts = wb_day_start_utc(date_from)
            .map(|dt| dt.timestamp())
            .unwrap_or(0);
        let date_to_ts = wb_day_end_utc(date_to)
            .map(|dt| dt.timestamp())
            .unwrap_or(0);

        self.progress_tracker.set_current_item(
            session_id,
            aggregate_index,
            Some(format!(
                "Запрос WB /api/v3/orders за период {}..{}",
                date_from, date_to
            )),
        );
        let recent_orders = match self
            .api_client
            .fetch_marketplace_orders(connection, date_from_ts, date_to_ts)
            .await
        {
            Ok(orders) => orders,
            Err(e) => {
                let msg = format!("Не удалось получить WB /api/v3/orders: {}", e);
                tracing::warn!("{}", msg);
                self.progress_tracker.add_error(
                    session_id,
                    Some(aggregate_index.to_string()),
                    msg,
                    None,
                );
                had_fetch_error = true;
                vec![]
            }
        };
        tracing::info!(
            "Recent marketplace orders (/orders): {}",
            recent_orders.len()
        );
        self.progress_tracker
            .update_aggregate(session_id, aggregate_index, 2, Some(2), 0, 0);

        // Merge: /new orders first, then recent (dedup by id handled naturally via document_no)
        let all_orders: Vec<_> = new_orders.into_iter().chain(recent_orders).collect();
        tracing::info!("Total marketplace orders to process: {}", all_orders.len());
        self.progress_tracker.set_current_item(
            session_id,
            aggregate_index,
            Some(format!("Обработка заказов WB: {}", all_orders.len())),
        );

        for order in &all_orders {
            match marketplace_order::process_marketplace_order(connection, &organization_id, order)
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
                    tracing::warn!("Failed to process marketplace order {}: {}", order.id, e);
                    self.progress_tracker.add_error(
                        session_id,
                        Some(aggregate_index.to_string()),
                        format!("Order {}", order.id),
                        Some(e.to_string()),
                    );
                }
            }

            if total_processed % 50 == 0 {
                self.progress_tracker.update_aggregate(
                    session_id,
                    aggregate_index,
                    total_processed,
                    Some(all_orders.len() as i32),
                    total_inserted,
                    total_updated,
                );
            }
        }

        self.progress_tracker.update_aggregate(
            session_id,
            aggregate_index,
            total_processed,
            Some(all_orders.len() as i32),
            total_inserted,
            total_updated,
        );
        self.progress_tracker
            .complete_aggregate(session_id, aggregate_index);

        tracing::info!(
            "Marketplace orders import done: processed={}, inserted={}, updated={}",
            total_processed,
            total_inserted,
            total_updated
        );

        if had_fetch_error {
            tracing::warn!(
                "Marketplace orders import completed with fetch errors: processed={}, inserted={}, updated={}",
                total_processed,
                total_inserted,
                total_updated
            );
        }

        Ok(())
    }

    /// Fetches FBS orders from /api/v3/orders (WB Marketplace API v3) and updates
    /// income_id in a015_wb_orders for orders that have a supplyId assigned.
    /// This provides real-time supply linkage without the statistics API delay.
    #[allow(dead_code)]
    async fn import_wb_orders_supply_link(
        &self,
        session_id: &str,
        connection: &contracts::domain::a006_connection_mp::aggregate::ConnectionMP,
        date_from: chrono::NaiveDate,
        date_to: chrono::NaiveDate,
    ) -> Result<()> {
        let aggregate_index = "a015_wb_orders_supply_link";
        let mut total_synced = 0;
        let mut total_supply_rows_refreshed = 0;
        let mut touched_income_ids = BTreeSet::new();

        tracing::info!(
            "Fetching marketplace orders to update supply links: {} to {}",
            date_from,
            date_to
        );

        self.progress_tracker.add_aggregate(
            session_id,
            aggregate_index.to_string(),
            "Связь заказов с поставками".to_string(),
        );

        let date_from_ts = wb_day_start_utc(date_from)
            .map(|dt| dt.timestamp())
            .unwrap_or(0);
        let date_to_ts = wb_day_end_utc(date_to)
            .map(|dt| dt.timestamp())
            .unwrap_or(0);

        let marketplace_orders = match self
            .api_client
            .fetch_marketplace_orders(connection, date_from_ts, date_to_ts)
            .await
        {
            Ok(orders) => orders,
            Err(e) => {
                let msg = format!("Failed to fetch marketplace orders: {}", e);
                tracing::error!("{}", msg);
                self.progress_tracker.add_error(
                    session_id,
                    Some(aggregate_index.to_string()),
                    msg,
                    None,
                );
                return Ok(());
            }
        };

        let total_fetched = marketplace_orders.len();
        tracing::info!("Marketplace orders fetched: {}", total_fetched);

        for order in &marketplace_orders {
            let document_no = match &order.rid {
                Some(rid) if !rid.is_empty() => rid.clone(),
                _ => continue,
            };

            let Some(existing_order) =
                crate::domain::a015_wb_orders::service::get_by_document_no(&document_no).await?
            else {
                continue;
            };

            if order.id > 0 {
                let _ = crate::domain::a015_wb_orders::service::update_line_id_by_document_no(
                    &document_no,
                    order.id,
                )
                .await;
            }

            let old_income_id = existing_order
                .source_meta
                .income_id
                .filter(|&value| value > 0);
            let new_income_id = match order.supply_id.as_deref().map(str::trim) {
                Some("") | None => None,
                Some(supply_id) => match supply_id
                    .rsplit('-')
                    .next()
                    .and_then(|s| s.parse::<i64>().ok())
                    .filter(|&value| value > 0)
                {
                    Some(value) => Some(value),
                    None => {
                        tracing::warn!("Cannot parse income_id from supplyId: {}", supply_id);
                        old_income_id
                    }
                },
            };

            if let Some(value) = old_income_id {
                touched_income_ids.insert(value);
            }
            if let Some(value) = new_income_id {
                touched_income_ids.insert(value);
            }

            if old_income_id == new_income_id {
                continue;
            }

            match crate::domain::a015_wb_orders::service::set_income_id_by_document_no(
                &document_no,
                new_income_id,
            )
            .await
            {
                Ok(_) => {
                    total_synced += 1;
                    tracing::debug!(
                        "Synced order {} supply link: {:?} -> {:?}",
                        document_no,
                        old_income_id,
                        new_income_id
                    );
                }
                Err(e) => {
                    tracing::warn!("Failed to sync income_id for order {}: {}", document_no, e);
                }
            }

            self.progress_tracker.update_aggregate(
                session_id,
                aggregate_index,
                total_synced as i32,
                Some(total_fetched as i32),
                total_synced as i32,
                0,
            );
        }

        for income_id in touched_income_ids {
            let supply_id = format!("WB-GI-{}", income_id);
            let Some(mut supply_doc) =
                crate::domain::a029_wb_supply::service::get_by_supply_id(&supply_id).await?
            else {
                continue;
            };

            let stat_orders =
                match crate::domain::a015_wb_orders::service::list_by_income_id(income_id).await {
                    Ok(orders) if !orders.is_empty() => orders,
                    Ok(_) => continue,
                    Err(e) => {
                        tracing::warn!(
                            "Failed to load a015 orders for missing supply income_id={}: {}",
                            income_id,
                            e
                        );
                        continue;
                    }
                };

            supply_doc.supply_orders = supply::build_supply_rows_from_stat_orders(&stat_orders);
            supply_doc.base.description = format!(
                "WB Supply {} - {} orders",
                supply_id,
                supply_doc.supply_orders.len()
            );

            match crate::domain::a029_wb_supply::service::store_document(supply_doc).await {
                Ok(_) => {
                    total_supply_rows_refreshed += 1;
                }
                Err(e) => {
                    tracing::warn!(
                        "Failed to refresh linked orders for supply {}: {}",
                        supply_id,
                        e
                    );
                    self.progress_tracker.add_error(
                        session_id,
                        Some(aggregate_index.to_string()),
                        supply_id,
                        Some(e.to_string()),
                    );
                }
            }
        }

        self.progress_tracker
            .complete_aggregate(session_id, aggregate_index);

        tracing::info!(
            "Supply link import completed: fetched={}, links_synced={}, existing_supplies_refreshed={}",
            total_fetched,
            total_synced,
            total_supply_rows_refreshed
        );

        Ok(())
    }

    /// Импорт финансовых отчетов Wildberries из API в p903_wb_finance_report
    ///
    /// ВАЖНО: API reportDetailByPeriod имеет лимит 1 запрос в минуту!
    /// Данные загружаются за весь период с пагинацией, а не по дням.
    async fn import_wb_finance_report(
        &self,
        session_id: &str,
        connection: &contracts::domain::a006_connection_mp::aggregate::ConnectionMP,
        date_from: chrono::NaiveDate,
        date_to: chrono::NaiveDate,
    ) -> Result<()> {
        use crate::domain::a002_organization;

        let aggregate_index = "p903_wb_finance_report";
        let mut processed_days = 0;
        let mut changed_days = 0;
        let mut total_source_rows = 0;
        let mut total_gl_rows = 0;

        tracing::info!(
            "Importing WB finance report for session: {} from date: {} to date: {}",
            session_id,
            date_from,
            date_to
        );

        // Получаем ID организации по UUID-ссылке из подключения
        let organization_id = match Uuid::parse_str(&connection.organization_ref) {
            Ok(org_uuid) => match a002_organization::service::get_by_id(
                crate::shared::data::db::get_connection(),
                org_uuid,
            )
            .await?
            {
                Some(org) => org.base.id.as_string(),
                None => {
                    let error_msg = format!(
                        "Организация с UUID '{}' не найдена в справочнике",
                        connection.organization_ref
                    );
                    tracing::error!("{}", error_msg);
                    self.progress_tracker.add_error(
                        session_id,
                        Some(aggregate_index.to_string()),
                        error_msg.clone(),
                        None,
                    );
                    anyhow::bail!("{}", error_msg);
                }
            },
            Err(_) => {
                let error_msg = format!(
                    "Некорректный organization_ref UUID в подключении: '{}'",
                    connection.organization_ref
                );
                tracing::error!("{}", error_msg);
                self.progress_tracker.add_error(
                    session_id,
                    Some(aggregate_index.to_string()),
                    error_msg.clone(),
                    None,
                );
                anyhow::bail!("{}", error_msg);
            }
        };

        self.progress_tracker.set_current_item(
            session_id,
            aggregate_index,
            Some(format!(
                "Дневной reconciliation {} - {} (API: 1 запрос/мин)",
                date_from.format("%Y-%m-%d"),
                date_to.format("%Y-%m-%d")
            )),
        );

        let total_days = (date_to - date_from).num_days() as i32 + 1;
        let mut current_date = date_from;
        while current_date <= date_to {
            self.progress_tracker.set_current_item(
                session_id,
                aggregate_index,
                Some(format!(
                    "Дата {}: загрузка и reconciliation",
                    current_date.format("%Y-%m-%d")
                )),
            );

            let report_rows = self
                .api_client
                .fetch_finance_report_by_period(connection, current_date, current_date)
                .await?;

            let mut entries = Vec::with_capacity(report_rows.len());
            for row in report_rows {
                match finance_report::map_finance_report_row(connection, &organization_id, &row)
                    .await
                {
                    Ok(entry) => entries.push(entry),
                    Err(e) => {
                        tracing::error!(
                            "Failed to map finance report row for {}: {}",
                            current_date,
                            e
                        );
                        self.progress_tracker.add_error(
                            session_id,
                            Some(aggregate_index.to_string()),
                            format!(
                                "Failed to map finance report row for {}",
                                current_date.format("%Y-%m-%d")
                            ),
                            Some(e.to_string()),
                        );
                    }
                }
            }

            match crate::projections::p903_wb_finance_report::service::reconcile_day(
                &connection.to_string_id(),
                current_date,
                &entries,
            )
            .await
            {
                Ok(result) => {
                    if result.changed {
                        changed_days += 1;
                    }
                    total_source_rows += result.source_rows as i32;
                    total_gl_rows += result.general_ledger_rows as i32;
                }
                Err(e) => {
                    tracing::error!(
                        "Failed to reconcile finance report day {}: {}",
                        current_date,
                        e
                    );
                    self.progress_tracker.add_error(
                        session_id,
                        Some(aggregate_index.to_string()),
                        format!(
                            "Failed to reconcile finance report day {}",
                            current_date.format("%Y-%m-%d")
                        ),
                        Some(e.to_string()),
                    );
                }
            }

            processed_days += 1;
            self.progress_tracker.update_aggregate(
                session_id,
                aggregate_index,
                processed_days,
                Some(total_days),
                total_source_rows,
                total_gl_rows,
            );

            current_date += chrono::Duration::days(1);
        }

        self.progress_tracker.update_aggregate(
            session_id,
            aggregate_index,
            processed_days,
            Some(total_days),
            total_source_rows,
            total_gl_rows,
        );

        self.progress_tracker
            .complete_aggregate(session_id, aggregate_index);
        tracing::info!(
            "WB finance report import completed: days={}, changed_days={}, source_rows={}, gl_rows={}",
            processed_days,
            changed_days,
            total_source_rows,
            total_gl_rows
        );

        Ok(())
    }

    /// Импорт истории комиссий Wildberries в p905
    /// Новый независимый Finance API → a043. Не создаёт проекций и не затрагивает p903.
    async fn import_wb_finance_report_v1(
        &self,
        session_id: &str,
        connection: &contracts::domain::a006_connection_mp::aggregate::ConnectionMP,
        date_from: chrono::NaiveDate,
        date_to: chrono::NaiveDate,
    ) -> Result<()> {
        let aggregate_index = "a043_wb_finance_report";
        let minimum = chrono::NaiveDate::from_ymd_opt(2025, 1, 1).expect("valid date");
        if date_from < minimum {
            anyhow::bail!("a043: период не может начинаться раньше 2025-01-01");
        }
        self.progress_tracker.set_current_item(
            session_id,
            aggregate_index,
            Some(format!(
                "WB Finance API v1: ежедневные отчёты {date_from}–{date_to}"
            )),
        );

        let reports = self
            .api_client
            .fetch_finance_reports_v1(connection, date_from, date_to)
            .await?;
        let total = reports.len() as i32;
        let mut saved = 0i32;
        let mut total_lines = 0i32;

        fn text(raw: &serde_json::Value, name: &str) -> Option<String> {
            match raw.get(name)? {
                serde_json::Value::String(v) => Some(v.clone()),
                serde_json::Value::Number(v) => Some(v.to_string()),
                _ => None,
            }
        }
        fn money(raw: &serde_json::Value, name: &str) -> Option<String> {
            text(raw, name).filter(|v| !v.trim().is_empty())
        }

        for fetched in reports {
            self.progress_tracker.set_current_item(
                session_id,
                aggregate_index,
                Some(format!(
                    "Сохранение отчёта WB {} ({} строк)",
                    fetched.report_id,
                    fetched.lines.len()
                )),
            );
            let create_date = text(&fetched.header, "createDate").unwrap_or_default();
            let header = WbFinanceReportHeader {
                document_no: fetched.report_id.clone(),
                document_date: create_date.clone(),
                connection_id: connection.to_string_id(),
                organization_id: connection.organization_ref.clone(),
                marketplace_id: connection.marketplace_id.clone(),
                report_id: fetched.report_id.clone(),
                period: "daily".into(),
                date_from: text(&fetched.header, "dateFrom").unwrap_or_default(),
                date_to: text(&fetched.header, "dateTo").unwrap_or_default(),
                create_date,
                seller_finance_name: text(&fetched.header, "sellerFinanceName").unwrap_or_default(),
                currency: text(&fetched.header, "currency").unwrap_or_default(),
                report_type: fetched.header.get("reportType").and_then(|v| v.as_i64()),
                retail_amount_sum: money(&fetched.header, "retailAmountSum"),
                for_pay_sum: money(&fetched.header, "forPaySum"),
                avg_sale_percent: fetched.header.get("avgSalePercent").cloned(),
                delivery_service_sum: money(&fetched.header, "deliveryServiceSum"),
                paid_storage_sum: money(&fetched.header, "paidStorageSum"),
                paid_acceptance_sum: money(&fetched.header, "paidAcceptanceSum"),
                deduction_sum: money(&fetched.header, "deductionSum"),
                penalty_sum: money(&fetched.header, "penaltySum"),
                additional_payment_sum: money(&fetched.header, "additionalPaymentSum"),
                cashback_amount_sum: money(&fetched.header, "cashbackAmountSum"),
                cashback_discount_sum: money(&fetched.header, "cashbackDiscountSum"),
                cashback_commission_change_sum: money(
                    &fetched.header,
                    "cashbackCommissionChangeSum",
                ),
                payment_schedule: text(&fetched.header, "paymentSchedule"),
                bank_payment_sum: money(&fetched.header, "bankPaymentSum"),
                raw: fetched.header.clone(),
            };
            let source_meta = WbFinanceReportSourceMeta {
                source: "wb_finance_api_v1".into(),
                list_endpoint: "/api/finance/v1/sales-reports/list".into(),
                detail_endpoint: format!(
                    "/api/finance/v1/sales-reports/detailed/{}",
                    fetched.report_id
                ),
                fetched_at: chrono::Utc::now().to_rfc3339(),
                pages_count: fetched.pages_count,
                last_rrd_id: fetched.last_rrd_id,
            };
            total_lines += fetched.lines.len() as i32;
            let document = WbFinanceReport::new_for_insert(header, fetched.lines, source_meta);
            crate::domain::a043_wb_finance_report::service::upsert_complete(&document).await?;
            saved += 1;
            self.progress_tracker.update_aggregate(
                session_id,
                aggregate_index,
                saved,
                Some(total),
                saved,
                0,
            );
        }
        self.progress_tracker
            .complete_aggregate(session_id, aggregate_index);
        tracing::info!(
            connection = %connection.to_string_id(),
            saved,
            total_lines,
            "WB Finance API v1 import complete"
        );
        Ok(())
    }

    async fn import_commission_history(
        &self,
        session_id: &str,
        connection: &contracts::domain::a006_connection_mp::aggregate::ConnectionMP,
    ) -> Result<()> {
        let aggregate_index = "p905_wb_commission_history";
        let mut new_records = 0;
        let mut updated_records = 0;
        let mut skipped_records = 0;

        tracing::info!(
            "Importing WB commission history for session: {}",
            session_id
        );

        // Получаем тарифы из API
        let tariffs = self.api_client.fetch_commission_tariffs(connection).await?;

        // Фильтруем пустые или некорректные записи
        let filtered_tariffs: Vec<_> = tariffs
            .into_iter()
            .filter(|t| t.subject_id > 0 && !t.subject_name.is_empty())
            .collect();

        tracing::info!("Processing {} commission tariffs", filtered_tariffs.len());

        let today = chrono::Utc::now().date_naive();

        for tariff in filtered_tariffs {
            match commission::process_commission_tariff(connection, &tariff, today).await {
                Ok((created, is_new)) => {
                    if created {
                        if is_new {
                            new_records += 1;
                        } else {
                            updated_records += 1;
                        }
                    } else {
                        skipped_records += 1;
                    }
                }
                Err(e) => {
                    tracing::error!("Failed to process commission tariff: {}", e);
                }
            }
        }

        self.progress_tracker
            .complete_aggregate(session_id, aggregate_index);
        tracing::info!(
            "WB commission history import completed: new={}, updated={}, skipped={}",
            new_records,
            updated_records,
            skipped_records
        );

        Ok(())
    }

    /// Импорт цен товаров Wildberries в p908
    async fn import_wb_goods_prices(
        &self,
        session_id: &str,
        connection: &contracts::domain::a006_connection_mp::aggregate::ConnectionMP,
    ) -> Result<()> {
        let aggregate_index = "p908_wb_goods_prices";
        let page_size = 1000;
        let mut offset = 0i32;
        let mut total_processed = 0i32;
        let mut total_upserted = 0i32;

        tracing::info!("Importing WB goods prices for session: {}", session_id);

        loop {
            let page = self
                .api_client
                .fetch_goods_prices(connection, page_size, offset)
                .await?;

            if page.is_empty() {
                break;
            }

            let page_len = page.len() as i32;

            for row in &page {
                match goods_prices::process_goods_price(connection, row).await {
                    Ok(_) => {
                        total_upserted += 1;
                    }
                    Err(e) => {
                        tracing::error!(
                            "Failed to process goods price row nm_id={}: {}",
                            row.nm_id,
                            e
                        );
                        self.progress_tracker.add_error(
                            session_id,
                            Some(aggregate_index.to_string()),
                            format!("Failed to process nm_id={}", row.nm_id),
                            Some(e.to_string()),
                        );
                    }
                }
                total_processed += 1;
            }

            self.progress_tracker.update_aggregate(
                session_id,
                aggregate_index,
                total_processed,
                None,
                total_upserted,
                0,
            );

            if page_len < page_size {
                break;
            }

            offset += page_size;
        }

        self.progress_tracker
            .complete_aggregate(session_id, aggregate_index);
        tracing::info!(
            "WB goods prices import completed: processed={}, upserted={}",
            total_processed,
            total_upserted
        );

        Ok(())
    }

    /// Импорт акций WB Calendar в a020
    async fn import_wb_promotions(
        &self,
        session_id: &str,
        connection: &contracts::domain::a006_connection_mp::aggregate::ConnectionMP,
        date_from: chrono::NaiveDate,
        date_to: chrono::NaiveDate,
    ) -> Result<()> {
        let aggregate_index = "a020_wb_promotion";
        let mut total_processed = 0i32;
        let mut total_new = 0i32;
        let mut total_updated = 0i32;

        tracing::info!(
            "Importing WB calendar promotions for session: {}, period: {} - {}",
            session_id,
            date_from,
            date_to
        );

        // Получить organization_id из connection
        let organization_id = {
            use contracts::domain::common::AggregateId;
            let org_id = connection.organization_ref.clone();
            if org_id.is_empty() {
                tracing::warn!(
                    "organization_ref is empty for connection {}",
                    connection.base.id.as_string()
                );
            }
            org_id
        };

        // Форматируем даты в RFC3339 (WB ожидает ISO 8601 с временной зоной)
        let start_dt = format!("{}T00:00:00Z", date_from.format("%Y-%m-%d"));
        let end_dt = format!("{}T23:59:59Z", date_to.format("%Y-%m-%d"));

        // Загружаем список акций
        let promotions = match self
            .api_client
            .fetch_calendar_promotions(connection, &start_dt, &end_dt, false)
            .await
        {
            Ok(p) => p,
            Err(e) => {
                tracing::error!("Failed to fetch WB calendar promotions: {}", e);
                self.progress_tracker.add_error(
                    session_id,
                    Some(aggregate_index.to_string()),
                    "Failed to fetch promotions list".to_string(),
                    Some(e.to_string()),
                );
                self.progress_tracker
                    .complete_aggregate(session_id, aggregate_index);
                return Ok(());
            }
        };

        tracing::info!("Found {} WB promotions in period", promotions.len());

        // Batch-fetch details для всех акций (по 100 за раз)
        let mut details_map: std::collections::HashMap<i64, crate::usecases::u504_import_from_wildberries::wildberries_api_client::WbCalendarPromotionDetail> =
            std::collections::HashMap::new();
        {
            let all_ids: Vec<i64> = promotions.iter().map(|p| p.id).collect();
            for chunk in all_ids.chunks(100) {
                match self
                    .api_client
                    .fetch_promotion_details(connection, chunk)
                    .await
                {
                    Ok(details_list) => {
                        for d in details_list {
                            details_map.insert(d.id, d);
                        }
                    }
                    Err(e) => {
                        tracing::warn!("Failed to fetch promotion details batch: {}", e);
                    }
                }
            }
            tracing::info!(
                "Loaded details for {}/{} promotions",
                details_map.len(),
                promotions.len()
            );
        }

        for promo in &promotions {
            let promo_name = promo
                .name
                .clone()
                .unwrap_or_else(|| format!("{}", promo.id));
            self.progress_tracker.set_current_item(
                session_id,
                aggregate_index,
                Some(format!("{} - {}", promo.id, promo_name)),
            );

            // Загружаем список nmId товаров для этой акции (не работает для type="auto")
            let promo_type = promo.promotion_type.as_deref();
            let nm_ids = match self
                .api_client
                .fetch_promotion_nomenclatures(connection, promo.id, promo_type)
                .await
            {
                Ok(ids) => ids,
                Err(e) => {
                    tracing::warn!(
                        "Failed to fetch nomenclatures for promotion {}: {}",
                        promo.id,
                        e
                    );
                    vec![]
                }
            };

            let details = details_map.get(&promo.id);

            match promotion::process_promotion(connection, &organization_id, promo, nm_ids, details)
                .await
            {
                Ok(is_new) => {
                    total_processed += 1;
                    if is_new {
                        total_new += 1;
                    } else {
                        total_updated += 1;
                    }
                }
                Err(e) => {
                    tracing::error!("Failed to process promotion {}: {}", promo.id, e);
                    self.progress_tracker.add_error(
                        session_id,
                        Some(aggregate_index.to_string()),
                        format!("Failed to process promotion {}", promo.id),
                        Some(e.to_string()),
                    );
                }
            }

            self.progress_tracker.update_aggregate(
                session_id,
                aggregate_index,
                total_processed,
                Some(promotions.len() as i32),
                total_new,
                total_updated,
            );
        }

        self.progress_tracker
            .complete_aggregate(session_id, aggregate_index);
        tracing::info!(
            "WB promotions import completed: new={}, updated={}",
            total_new,
            total_updated
        );

        Ok(())
    }

    /// Получить статистику рекламных кампаний WB за период и сохранить в CSV
    async fn import_wb_documents(
        &self,
        session_id: &str,
        connection: &contracts::domain::a006_connection_mp::aggregate::ConnectionMP,
        date_from: chrono::NaiveDate,
        date_to: chrono::NaiveDate,
    ) -> Result<()> {
        use crate::domain::a002_organization;

        let aggregate_index = "a027_wb_documents";
        let mut total_processed = 0;
        let mut total_inserted = 0;
        let mut total_updated = 0;

        let organization_id = match Uuid::parse_str(&connection.organization_ref) {
            Ok(org_uuid) => match a002_organization::service::get_by_id(
                crate::shared::data::db::get_connection(),
                org_uuid,
            )
            .await?
            {
                Some(org) => org.base.id.as_string(),
                None => {
                    let error_msg = format!(
                        "Организация с UUID '{}' не найдена в справочнике",
                        connection.organization_ref
                    );
                    tracing::error!("{}", error_msg);
                    self.progress_tracker.add_error(
                        session_id,
                        Some(aggregate_index.to_string()),
                        error_msg.clone(),
                        None,
                    );
                    anyhow::bail!("{}", error_msg);
                }
            },
            Err(_) => {
                let error_msg = format!(
                    "Некорректный organization_ref UUID в подключении: '{}'",
                    connection.organization_ref
                );
                tracing::error!("{}", error_msg);
                self.progress_tracker.add_error(
                    session_id,
                    Some(aggregate_index.to_string()),
                    error_msg.clone(),
                    None,
                );
                anyhow::bail!("{}", error_msg);
            }
        };

        let rows = self
            .api_client
            .fetch_documents_list(connection, date_from, date_to)
            .await?;

        for row in rows {
            self.progress_tracker.set_current_item(
                session_id,
                aggregate_index,
                Some(row.service_name.clone()),
            );

            match document::process_document_header(connection, &organization_id, &row).await {
                Ok(is_new) => {
                    total_processed += 1;
                    if is_new {
                        total_inserted += 1;
                    } else {
                        total_updated += 1;
                    }
                }
                Err(e) => {
                    tracing::error!("Failed to process WB document {}: {}", row.service_name, e);
                    self.progress_tracker.add_error(
                        session_id,
                        Some(aggregate_index.to_string()),
                        format!("Failed to process WB document {}", row.service_name),
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

        self.progress_tracker
            .set_current_item(session_id, aggregate_index, None);
        self.progress_tracker
            .complete_aggregate(session_id, aggregate_index);

        Ok(())
    }

    async fn import_wb_advert_campaigns(
        &self,
        session_id: &str,
        connection: &contracts::domain::a006_connection_mp::aggregate::ConnectionMP,
    ) -> Result<usize> {
        let aggregate_index = "a030_wb_advert_campaign";
        self.progress_tracker.set_current_item(
            session_id,
            aggregate_index,
            Some("Получение списка кампаний".into()),
        );

        let summaries = match self
            .api_client
            .fetch_advert_campaign_summaries(connection)
            .await
        {
            Ok(summaries) => summaries,
            Err(err) => {
                let existing = crate::domain::a030_wb_advert_campaign::service::list_by_connection(
                    &connection.to_string_id(),
                )
                .await
                .context("Failed to read existing a030_wb_advert_campaign fallback")?;
                let message = format!("Failed to fetch WB advert campaign summaries: {}", err);
                self.progress_tracker.add_error(
                    session_id,
                    Some(aggregate_index.to_string()),
                    message.clone(),
                    Some(err.to_string()),
                );

                if !existing.is_empty() {
                    tracing::warn!(
                        "{}; keeping existing a030 campaigns for connection={} count={}",
                        message,
                        connection.to_string_id(),
                        existing.len()
                    );
                    self.progress_tracker.update_aggregate(
                        session_id,
                        aggregate_index,
                        existing.len() as i32,
                        Some(existing.len() as i32),
                        0,
                        0,
                    );
                    self.progress_tracker
                        .complete_aggregate(session_id, aggregate_index);
                    return Ok(existing.len());
                }

                anyhow::bail!(
                    "{}; no existing a030 campaigns are available for fallback",
                    message
                );
            }
        };

        if summaries.is_empty() {
            self.progress_tracker
                .complete_aggregate(session_id, aggregate_index);
            return Ok(0);
        }

        self.progress_tracker.update_aggregate(
            session_id,
            aggregate_index,
            0,
            Some(summaries.len() as i32),
            0,
            0,
        );

        // Load lightweight snapshot (advert_id → change_time + has_info_json) to decide
        // which campaigns need a fresh API call.  Full aggregates are NOT loaded here —
        // the upsert will preserve existing info_json for campaigns we pass Null for.
        let snapshot = crate::domain::a030_wb_advert_campaign::service::list_info_snapshot(
            &connection.to_string_id(),
        )
        .await
        .unwrap_or_default();

        // Classify every campaign from the summaries response.
        // Priority 1 — new (not in DB): must fetch info.
        // Priority 2 — existing but change_time changed: fetch info (data may differ).
        // Priority 3 — existing, unchanged change_time, has info_json: skip API call.
        // Priority 4 — existing, unchanged change_time, no info_json: fetch info.
        let mut priority1: Vec<i64> = Vec::new(); // new
        let mut priority2: Vec<i64> = Vec::new(); // changed
        let mut priority4: Vec<i64> = Vec::new(); // no info yet

        for summary in &summaries {
            let advert_id = summary.advert_id;
            match snapshot.get(&advert_id) {
                None => priority1.push(advert_id),
                Some(snap) => {
                    let same_change_time = snap.change_time == summary.change_time;
                    if same_change_time && snap.has_info_json {
                        // nothing to do — cached info is still valid
                    } else if !snap.has_info_json {
                        priority4.push(advert_id);
                    } else {
                        priority2.push(advert_id);
                    }
                }
            }
        }

        // Fetch every new or changed campaign in API-sized batches. A failed batch is
        // intentionally absent from info_by_id, so the upsert preserves its old info_json.
        let need_info_ids: Vec<i64> = priority1
            .into_iter()
            .chain(priority4)
            .chain(priority2)
            .collect();
        let cached_count = summaries.len() - need_info_ids.len();
        tracing::info!(
            "WB advert campaign info: total={}, cached={}, fetch_now={}, batches={}",
            summaries.len(),
            cached_count,
            need_info_ids.len(),
            need_info_ids.len().div_ceil(WB_ADVERT_CAMPAIGN_BATCH_SIZE),
        );

        let mut info_by_id: HashMap<i64, serde_json::Value> = HashMap::new();
        for (batch_index, ids) in wb_advert_info_batches(&need_info_ids).enumerate() {
            if batch_index > 0 {
                tokio::time::sleep(tokio::time::Duration::from_millis(
                    WB_ADVERT_MIN_REQUEST_INTERVAL_MS,
                ))
                .await;
            }
            self.progress_tracker.set_current_item(
                session_id,
                aggregate_index,
                Some(format!(
                    "Свойства кампаний из WB API: batch {}/{} ({} ID)",
                    batch_index + 1,
                    need_info_ids.len().div_ceil(WB_ADVERT_CAMPAIGN_BATCH_SIZE),
                    ids.len()
                )),
            );

            match self
                .api_client
                .fetch_advert_campaign_info_values(connection, ids)
                .await
            {
                Ok(values) => {
                    for value in values {
                        if let Some(id) = value
                            .get("advertId")
                            .or_else(|| value.get("id"))
                            .and_then(|v| v.as_i64())
                        {
                            info_by_id.insert(id, value);
                        }
                    }
                    tracing::info!(
                        "WB advert campaign info batch fetched: requested={}, accumulated={}",
                        ids.len(),
                        info_by_id.len()
                    );
                }
                Err(err) => {
                    let message = format!(
                        "WB advert campaign info batch {}/{} failed for {} IDs; existing \
                         info_json is preserved by upsert: {}",
                        batch_index + 1,
                        need_info_ids.len().div_ceil(WB_ADVERT_CAMPAIGN_BATCH_SIZE),
                        ids.len(),
                        err,
                    );
                    tracing::warn!("{}", message);
                    self.progress_tracker.add_error(
                        session_id,
                        Some(aggregate_index.to_string()),
                        message,
                        Some(err.to_string()),
                    );
                }
            }
        }

        // Capture before the loop consumes info_by_id via .remove().
        let api_fetched_count = info_by_id.len();

        let fetched_at = chrono::Utc::now().to_rfc3339();
        let mut campaigns = Vec::with_capacity(summaries.len());
        for summary in &summaries {
            // Campaigns not in info_by_id get Null — the upsert will preserve existing info_json.
            let info_json = info_by_id
                .remove(&summary.advert_id)
                .unwrap_or(serde_json::Value::Null);
            let header = WbAdvertCampaignHeader {
                advert_id: summary.advert_id,
                connection_id: connection.to_string_id(),
                organization_id: connection.organization_ref.clone(),
                marketplace_id: connection.marketplace_id.clone(),
                campaign_type: summary.campaign_type,
                status: summary.status,
                change_time: summary.change_time.clone(),
                nm_count: 0, // recalculated by before_write() from info_json
            };
            let source_meta = WbAdvertCampaignSourceMeta {
                source: "wb_advert_campaigns".to_string(),
                fetched_at: fetched_at.clone(),
                info_json,
            };
            let mut campaign = WbAdvertCampaign::new_for_insert(header, source_meta);
            campaign.before_write();
            campaign.validate().map_err(|e| anyhow::anyhow!(e))?;
            campaigns.push(campaign);
        }

        let (new_count, total_count) =
            crate::domain::a030_wb_advert_campaign::service::upsert_many(&campaigns)
                .await
                .context("Failed to save a030_wb_advert_campaign")?;
        self.progress_tracker.update_aggregate(
            session_id,
            aggregate_index,
            total_count as i32,
            Some(summaries.len() as i32),
            new_count as i32,         // "Новые" = физически добавленные записи
            api_fetched_count as i32, // "Изменено" = получили свежий info_json из API
        );
        self.progress_tracker
            .complete_aggregate(session_id, aggregate_index);
        tracing::info!(
            "WB Advert campaigns synced: connection={}, total={}, new={}, api_fetched={}",
            connection.to_string_id(),
            total_count,
            new_count,
            api_fetched_count,
        );
        Ok(total_count)
    }

    /// Обновляет справочник кампаний a030 перед загрузкой статистики, чтобы новые
    /// кампании гарантированно попадали в fullstats, даже если task012 отстаёт или выключена.
    ///
    /// Синхронизируется только список advertId/статус/change_time одним дешёвым запросом
    /// `/adv/v1/promotion/count`; `info_json` НЕ запрашивается — им по-прежнему занимается
    /// task012 порциями (лимит WB на `/api/advert/v2/adverts`). Для fullstats info_json не нужен.
    ///
    /// Best-effort: при ошибке API справочник a030 остаётся прежним, а импорт статистики
    /// продолжается по уже известным кампаниям (ошибка фиксируется в прогрессе сессии).
    async fn refresh_advert_campaign_ids(
        &self,
        session_id: &str,
        connection: &contracts::domain::a006_connection_mp::aggregate::ConnectionMP,
    ) -> Result<()> {
        let summaries = match self
            .api_client
            .fetch_advert_campaign_summaries(connection)
            .await
        {
            Ok(summaries) => summaries,
            Err(err) => {
                let message = format!(
                    "WB Advert: обновление справочника кампаний перед статистикой не удалось; \
                     используется существующий список a030: {}",
                    err
                );
                tracing::warn!("{}", message);
                self.progress_tracker.add_error(
                    session_id,
                    Some("wb_advert_stats".to_string()),
                    message,
                    Some(err.to_string()),
                );
                return Ok(());
            }
        };

        if summaries.is_empty() {
            return Ok(());
        }

        let fetched_at = chrono::Utc::now().to_rfc3339();
        let mut campaigns = Vec::with_capacity(summaries.len());
        for summary in &summaries {
            // info_json = Null: upsert сохраняет уже накопленный info_json для известных
            // кампаний, а новым проставит null (info заполнит следующий прогон task012).
            let header = WbAdvertCampaignHeader {
                advert_id: summary.advert_id,
                connection_id: connection.to_string_id(),
                organization_id: connection.organization_ref.clone(),
                marketplace_id: connection.marketplace_id.clone(),
                campaign_type: summary.campaign_type,
                status: summary.status,
                change_time: summary.change_time.clone(),
                nm_count: 0, // recalculated by before_write() from info_json
            };
            let source_meta = WbAdvertCampaignSourceMeta {
                source: "wb_advert_campaigns".to_string(),
                fetched_at: fetched_at.clone(),
                info_json: serde_json::Value::Null,
            };
            let mut campaign = WbAdvertCampaign::new_for_insert(header, source_meta);
            campaign.before_write();
            campaign.validate().map_err(|e| anyhow::anyhow!(e))?;
            campaigns.push(campaign);
        }

        let (new_count, total_count) =
            crate::domain::a030_wb_advert_campaign::service::upsert_many(&campaigns)
                .await
                .context("Failed to refresh a030_wb_advert_campaign before stats")?;

        tracing::info!(
            "WB Advert: справочник кампаний обновлён перед статистикой: connection={}, total={}, new={}",
            connection.to_string_id(),
            total_count,
            new_count,
        );

        Ok(())
    }

    /// Загрузка статистики рекламы WB без промежуточного CSV.
    /// Возвращает `true`, если были частичные ошибки API (данные за период всё равно пересобираются из успешных ответов).
    async fn import_wb_advert_stats(
        &self,
        session_id: &str,
        connection: &contracts::domain::a006_connection_mp::aggregate::ConnectionMP,
        date_from: chrono::NaiveDate,
        date_to: chrono::NaiveDate,
    ) -> Result<bool> {
        let aggregate_index = "wb_advert_stats";
        let begin_date = date_from.format("%Y-%m-%d").to_string();
        let end_date = date_to.format("%Y-%m-%d").to_string();

        tracing::info!(
            "WB Advert stats: session={}, period={} to {}",
            session_id,
            begin_date,
            end_date
        );

        // Синхронизируем справочник кампаний a030 непосредственно перед fullstats: без этого
        // новые кампании, ещё не подхваченные task012, не попадут в статистику (100% покрытие).
        self.refresh_advert_campaign_ids(session_id, connection)
            .await?;

        let all_advert_ids =
            crate::domain::a030_wb_advert_campaign::service::list_advert_ids_by_connection(
                &connection.to_string_id(),
            )
            .await
            .context("Failed to read advert ids from a030_wb_advert_campaign")?;

        // Filter out completed campaigns (status=7) that ended before the period start —
        // they cannot have any activity in [date_from, date_to].
        let advert_ids =
            crate::domain::a030_wb_advert_campaign::service::list_advert_ids_for_period(
                &connection.to_string_id(),
                &begin_date,
            )
            .await
            .context("Failed to read filtered advert ids from a030_wb_advert_campaign")?;

        let skipped_count = all_advert_ids.len().saturating_sub(advert_ids.len());
        tracing::info!(
            "WB Advert: total={}, period_relevant={}, skipped_completed={}",
            all_advert_ids.len(),
            advert_ids.len(),
            skipped_count,
        );

        if advert_ids.is_empty() {
            tracing::info!(
                "WB Advert: no relevant campaigns for period, clearing existing documents"
            );
            crate::domain::a026_wb_advert_daily::service::replace_for_period(
                &connection.to_string_id(),
                &begin_date,
                &end_date,
                &[],
            )
            .await?;
            self.progress_tracker
                .complete_aggregate(session_id, aggregate_index);
            return Ok(false);
        }

        // Период режется на календарные месяцы: WB отвергает интервал > 31 дня.
        let windows = calendar_month_windows(date_from, date_to);
        let chunks: Vec<&[i64]> = advert_ids.chunks(WB_ADVERT_FULLSTATS_CHUNK_SIZE).collect();
        let total_chunks = chunks.len();
        let total_requests = windows.len() * total_chunks;

        tracing::info!(
            "WB Advert: {} campaigns × {} month windows → {} chunks of up to {} = {} requests (delay {}s each)",
            advert_ids.len(),
            windows.len(),
            total_chunks,
            WB_ADVERT_FULLSTATS_CHUNK_SIZE,
            total_requests,
            WB_ADVERT_FULLSTATS_CHUNK_DELAY_SECS,
        );
        self.progress_tracker.update_aggregate(
            session_id,
            aggregate_index,
            0,
            Some(advert_ids.len() as i32),
            0,
            0,
        );

        let mut had_fetch_errors = false;
        let mut all_stats: Vec<WbAdvertFullStat> = Vec::new();
        // Кампания считается загруженной, только если закрыты ВСЕ окна периода:
        // частично покрытую нельзя пускать в scoped-replace, иначе удаление за
        // весь период сотрёт месяцы, которые не удалось перезапросить.
        let mut covered_windows: HashMap<i64, usize> = HashMap::new();
        let mut completed_requests = 0usize;

        'windows: for (window_idx, (window_from, window_to)) in windows.iter().enumerate() {
            let window_begin = window_from.format("%Y-%m-%d").to_string();
            let window_end = window_to.format("%Y-%m-%d").to_string();

            for (chunk_idx, chunk) in chunks.iter().enumerate() {
                self.progress_tracker.set_current_item(
                    session_id,
                    aggregate_index,
                    Some(format!(
                        "Месяц {}/{} ({}..{}), чанк {}/{} (advertIds: {}..)",
                        window_idx + 1,
                        windows.len(),
                        window_begin,
                        window_end,
                        chunk_idx + 1,
                        total_chunks,
                        chunk[0]
                    )),
                );

                match self
                    .api_client
                    .fetch_advert_fullstats(connection, chunk, &window_begin, &window_end)
                    .await
                {
                    Ok(stats) => {
                        for advert_id in chunk.iter() {
                            *covered_windows.entry(*advert_id).or_insert(0) += 1;
                        }
                        all_stats.extend(stats.iter().cloned());
                    }
                    Err(e) => {
                        had_fetch_errors = true;
                        let error_text = e.to_string();
                        tracing::warn!(
                            "Failed to fetch fullstats for connection={} window={}..{} ({}/{}) chunk {}/{} campaigns={} first_advert_id={} error={}",
                            connection.to_string_id(),
                            window_begin,
                            window_end,
                            window_idx + 1,
                            windows.len(),
                            chunk_idx + 1,
                            total_chunks,
                            chunk.len(),
                            chunk.first().copied().unwrap_or_default(),
                            error_text
                        );
                        self.progress_tracker.add_error(
                            session_id,
                            Some(aggregate_index.to_string()),
                            format!(
                                "WB Advert fullstats: месяц {}..{}, чанк {}/{} не загружен для кабинета {} — {}",
                                window_begin,
                                window_end,
                                chunk_idx + 1,
                                total_chunks,
                                connection.to_string_id(),
                                error_text
                            ),
                            Some(error_text.clone()),
                        );

                        if is_wb_advert_fullstats_rate_limit(&error_text) {
                            let retry_seconds = extract_wb_rate_limit_retry_seconds(&error_text);
                            let retry_hint = retry_seconds
                                .map(|seconds| {
                                    format!(" Рекомендованный повтор через {seconds} сек.")
                                })
                                .unwrap_or_default();
                            let diagnostic = format!(
                                "WB Advert fullstats остановлен после 429 Too Many Requests: кабинет={}, период={}..{}, месяц {}/{}, чанк {}/{}, выполнено запросов={}/{}.{}",
                                connection.to_string_id(),
                                begin_date,
                                end_date,
                                window_idx + 1,
                                windows.len(),
                                chunk_idx + 1,
                                total_chunks,
                                completed_requests,
                                total_requests,
                                retry_hint
                            );
                            tracing::warn!("{}", diagnostic);
                            self.progress_tracker.add_error(
                                session_id,
                                Some(aggregate_index.to_string()),
                                diagnostic,
                                Some(error_text),
                            );
                            break 'windows;
                        }
                    }
                }

                completed_requests += 1;
                let fully_covered = covered_windows
                    .values()
                    .filter(|count| **count == windows.len())
                    .count();
                self.progress_tracker.update_aggregate(
                    session_id,
                    aggregate_index,
                    fully_covered as i32,
                    Some(advert_ids.len() as i32),
                    all_stats.len() as i32,
                    0,
                );

                if completed_requests < total_requests {
                    tokio::time::sleep(tokio::time::Duration::from_secs(
                        WB_ADVERT_FULLSTATS_CHUNK_DELAY_SECS,
                    ))
                    .await;
                }
            }
        }

        let mut successful_advert_ids: Vec<i64> = advert_ids
            .iter()
            .copied()
            .filter(|advert_id| {
                covered_windows.get(advert_id).copied().unwrap_or(0) == windows.len()
            })
            .collect();
        let processed_campaigns = successful_advert_ids.len() as i32;

        // Статистика частично покрытых кампаний отбрасывается: их документы за
        // удавшиеся месяцы не с чем согласовать, а вставка без парного удаления
        // упёрлась бы в UNIQUE(connection, date, advert_id).
        if had_fetch_errors {
            let covered: HashSet<i64> = successful_advert_ids.iter().copied().collect();
            let before = all_stats.len();
            all_stats.retain(|stat| covered.contains(&stat.advert_id));
            tracing::warn!(
                "WB Advert: dropped {} stat records of partially covered campaigns (connection={}, covered={}/{})",
                before - all_stats.len(),
                connection.to_string_id(),
                covered.len(),
                advert_ids.len(),
            );

            self.progress_tracker.add_error(
                session_id,
                Some(aggregate_index.to_string()),
                "Часть рекламной статистики не загрузилась; сохранены только кампании, закрытые за весь период"
                    .to_string(),
                None,
            );
        }

        if had_fetch_errors && successful_advert_ids.is_empty() {
            anyhow::bail!(
                "WB Advert fullstats: no campaign was covered across all {} month windows ({} requests); existing a026 data was left unchanged",
                windows.len(),
                total_requests
            );
        }

        let build_started_at = std::time::Instant::now();
        tracing::info!(
            "WB Advert document build started: connection={}, stats={}",
            connection.to_string_id(),
            all_stats.len()
        );
        let documents = self
            .build_wb_advert_documents(connection, &all_stats)
            .await
            .with_context(|| {
                format!(
                    "Failed during WB advert document build for connection={} period={}..{}",
                    connection.to_string_id(),
                    begin_date,
                    end_date
                )
            })?;
        let document_ids: Vec<Uuid> = documents
            .iter()
            .map(|document| document.base.id.value())
            .collect();
        tracing::info!(
            "WB Advert document build completed: connection={}, documents={}, elapsed_ms={}",
            connection.to_string_id(),
            documents.len(),
            build_started_at.elapsed().as_millis()
        );

        let replace_started_at = std::time::Instant::now();
        tracing::info!(
            "WB Advert document replace started: connection={}, period={}..{}, documents={}",
            connection.to_string_id(),
            begin_date,
            end_date,
            documents.len()
        );
        let documents_count = if had_fetch_errors {
            successful_advert_ids.sort_unstable();
            successful_advert_ids.dedup();
            crate::domain::a026_wb_advert_daily::service::replace_for_period_advert_ids(
                &connection.to_string_id(),
                &begin_date,
                &end_date,
                &successful_advert_ids,
                &documents,
            )
            .await
            .with_context(|| {
                format!(
                    "Failed during scoped WB advert replace_for_period for connection={} period={}..{} successful_adverts={} documents={}",
                    connection.to_string_id(),
                    begin_date,
                    end_date,
                    successful_advert_ids.len(),
                    documents.len()
                )
            })?
        } else {
            crate::domain::a026_wb_advert_daily::service::replace_for_period(
                &connection.to_string_id(),
                &begin_date,
                &end_date,
                &documents,
            )
            .await
            .with_context(|| {
                format!(
                    "Failed during WB advert replace_for_period for connection={} period={}..{} documents={}",
                    connection.to_string_id(),
                    begin_date,
                    end_date,
                    documents.len()
                )
            })?
        };

        let post_started_at = std::time::Instant::now();
        tracing::info!(
            "WB Advert auto-post started: connection={}, period={}..{}, documents={}",
            connection.to_string_id(),
            begin_date,
            end_date,
            document_ids.len()
        );
        // Контекст строится СТРОГО после replace_for_period: снимок p913 должен
        // содержать только «чужие» документы (свои за период уже удалены).
        let mut posting_context =
            AdvertPostingContext::prefetched(&connection.to_string_id(), &begin_date, &end_date)
                .await
                .with_context(|| {
                    format!(
                "Failed to prefetch WB advert posting context for connection={} period={}..{}",
                connection.to_string_id(),
                begin_date,
                end_date
            )
                })?;
        for document_id in &document_ids {
            post_wb_advert_document_with_retry(*document_id, &mut posting_context)
                .await
                .with_context(|| {
                    format!(
                        "Failed during WB advert auto-post for connection={} document_id={}",
                        connection.to_string_id(),
                        document_id
                    )
                })?;
        }
        tracing::info!(
            "WB Advert auto-post completed: connection={}, period={}..{}, documents={}, elapsed_ms={}",
            connection.to_string_id(),
            begin_date,
            end_date,
            document_ids.len(),
            post_started_at.elapsed().as_millis()
        );

        self.progress_tracker.update_aggregate(
            session_id,
            aggregate_index,
            processed_campaigns,
            Some(advert_ids.len() as i32),
            documents_count as i32,
            0,
        );

        tracing::info!(
            "WB Advert documents synced: connection={}, period={}..{}, documents={}, elapsed_ms={}",
            connection.to_string_id(),
            begin_date,
            end_date,
            documents_count,
            replace_started_at.elapsed().as_millis()
        );

        // Догрузка истории для впервые обнаруженных кампаний (best-effort, не влияет на watermark).
        if let Err(err) = self
            .backfill_new_advert_campaigns(session_id, connection, date_from)
            .await
        {
            tracing::warn!(
                "WB Advert backfill failed for connection={}: {}",
                connection.to_string_id(),
                err
            );
            self.progress_tracker.add_error(
                session_id,
                Some(aggregate_index.to_string()),
                "Догрузка истории новых рекламных кампаний не выполнена".to_string(),
                Some(err.to_string()),
            );
        }

        self.progress_tracker
            .complete_aggregate(session_id, aggregate_index);
        tracing::info!(
            "WB Advert stats completed: {} campaigns processed, {} stat records, partial={}",
            processed_campaigns,
            all_stats.len(),
            had_fetch_errors
        );

        Ok(had_fetch_errors)
    }

    /// Догружает историю рекламной статистики для впервые обнаруженных кампаний.
    ///
    /// Основное окно task011 движется вперёд от watermark, поэтому кампания, появившаяся
    /// в a030 позже, теряет свои ранние дни. Здесь для каждой кампании без покрытия ниже
    /// текущего окна (`min(document_date)` отсутствует или > `date_from`) один раз догружается
    /// диапазон `[account_floor, date_from-1]`, где `account_floor` — самая ранняя дата a026,
    /// уже загруженная по этому подключению (горизонт «полных данных»).
    ///
    /// Детекция по покрытию делает шаг самозавершающимся (после успешной догрузки `min_date`
    /// опускается ниже границы окна) и устойчивым к разовым сбоям (при ошибке ничего не
    /// пишется — кампания попадёт в догрузку на следующем запуске). Best-effort: ошибки
    /// логируются, watermark основного окна не затрагивается.
    async fn backfill_new_advert_campaigns(
        &self,
        session_id: &str,
        connection: &contracts::domain::a006_connection_mp::aggregate::ConnectionMP,
        date_from: chrono::NaiveDate,
    ) -> Result<()> {
        let aggregate_index = "wb_advert_stats";
        let connection_id = connection.to_string_id();

        // Покрытие уже загруженной статистики: min(document_date) по каждой кампании.
        let min_by_campaign =
            crate::domain::a026_wb_advert_daily::service::min_date_by_campaign(&connection_id)
                .await
                .context("Failed to read a026 coverage for advert backfill")?;

        // Горизонт «полных данных» — самая ранняя загруженная дата по подключению.
        // Если статистики ещё нет вовсе, догружать нечего (первый прогон закрывает всё сам).
        let Some(account_floor) = min_by_campaign.values().min().cloned() else {
            return Ok(());
        };
        let Ok(floor_date) = chrono::NaiveDate::parse_from_str(&account_floor, "%Y-%m-%d") else {
            return Ok(());
        };
        // Диапазон догрузки — строго до текущего окна.
        let Some(backfill_end) = date_from.pred_opt() else {
            return Ok(());
        };
        if floor_date > backfill_end {
            return Ok(()); // окно уже стоит на горизонте — пропусков ниже нет
        }

        let date_from_str = date_from.format("%Y-%m-%d").to_string();

        // Кандидаты: кампании, способные иметь активность с горизонта (тот же фильтр
        // завершённых, что и в основном окне, но с нижней границей = account_floor),
        // у которых нет покрытия ниже текущего окна.
        let candidates_raw =
            crate::domain::a030_wb_advert_campaign::service::list_advert_ids_for_period(
                &connection_id,
                &account_floor,
            )
            .await
            .context("Failed to read advert ids for backfill")?;

        let backfill_ids: Vec<i64> = candidates_raw
            .into_iter()
            .filter(|advert_id| match min_by_campaign.get(advert_id) {
                None => true,                                           // покрытия нет вовсе
                Some(min_d) => min_d.as_str() > date_from_str.as_str(), // покрытие только внутри/выше окна
            })
            .collect();

        if backfill_ids.is_empty() {
            return Ok(());
        }

        let begin_date = account_floor.clone();
        let end_date = backfill_end.format("%Y-%m-%d").to_string();

        tracing::info!(
            "WB Advert backfill: connection={}, campaigns={}, range={}..{}",
            connection_id,
            backfill_ids.len(),
            begin_date,
            end_date,
        );
        self.progress_tracker.set_current_item(
            session_id,
            aggregate_index,
            Some(format!(
                "Догрузка истории {} новых кампаний ({}..{})",
                backfill_ids.len(),
                begin_date,
                end_date
            )),
        );

        // Диапазон догрузки почти всегда длиннее 31 дня (он тянется от горизонта
        // данных до текущего окна), поэтому без нарезки по месяцам каждый запрос
        // здесь возвращал 400 «max date range 31 days» и шаг молча не работал.
        let windows = calendar_month_windows(floor_date, backfill_end);
        let chunks: Vec<&[i64]> = backfill_ids
            .chunks(WB_ADVERT_FULLSTATS_CHUNK_SIZE)
            .collect();
        let total_chunks = chunks.len();
        let total_requests = windows.len() * total_chunks;
        let mut all_stats: Vec<WbAdvertFullStat> = Vec::new();
        let mut covered_windows: HashMap<i64, usize> = HashMap::new();
        let mut completed_requests = 0usize;
        let mut stopped_by_rate_limit = false;

        'windows: for (window_idx, (window_from, window_to)) in windows.iter().enumerate() {
            let window_begin = window_from.format("%Y-%m-%d").to_string();
            let window_end = window_to.format("%Y-%m-%d").to_string();

            for (chunk_idx, chunk) in chunks.iter().enumerate() {
                match self
                    .api_client
                    .fetch_advert_fullstats(connection, chunk, &window_begin, &window_end)
                    .await
                {
                    Ok(stats) => {
                        for advert_id in chunk.iter() {
                            *covered_windows.entry(*advert_id).or_insert(0) += 1;
                        }
                        all_stats.extend(stats);
                    }
                    Err(e) => {
                        let error_text = e.to_string();
                        tracing::warn!(
                            "WB Advert backfill: window {}/{} ({}..{}) chunk {}/{} failed connection={} error={}",
                            window_idx + 1,
                            windows.len(),
                            window_begin,
                            window_end,
                            chunk_idx + 1,
                            total_chunks,
                            connection_id,
                            error_text
                        );
                        self.progress_tracker.add_error(
                            session_id,
                            Some(aggregate_index.to_string()),
                            format!(
                                "Догрузка истории рекламы: месяц {}..{}, чанк {}/{} не загружен (будет повторено позже) — {}",
                                window_begin,
                                window_end,
                                chunk_idx + 1,
                                total_chunks,
                                error_text
                            ),
                            Some(error_text.clone()),
                        );
                        if is_wb_advert_fullstats_rate_limit(&error_text) {
                            stopped_by_rate_limit = true;
                            break 'windows;
                        }
                    }
                }

                completed_requests += 1;
                if completed_requests < total_requests {
                    tokio::time::sleep(tokio::time::Duration::from_secs(
                        WB_ADVERT_FULLSTATS_CHUNK_DELAY_SECS,
                    ))
                    .await;
                }
            }
        }

        // Как и в основном окне: берём только кампании, закрытые за все месяцы.
        let mut successful_advert_ids: Vec<i64> = backfill_ids
            .iter()
            .copied()
            .filter(|advert_id| {
                covered_windows.get(advert_id).copied().unwrap_or(0) == windows.len()
            })
            .collect();
        if successful_advert_ids.len() != backfill_ids.len() {
            let covered: HashSet<i64> = successful_advert_ids.iter().copied().collect();
            all_stats.retain(|stat| covered.contains(&stat.advert_id));
        }

        if successful_advert_ids.is_empty() {
            tracing::warn!(
                "WB Advert backfill: no chunks succeeded connection={} rate_limited={}; \
                 campaigns remain uncovered and will retry next run",
                connection_id,
                stopped_by_rate_limit
            );
            return Ok(());
        }

        let documents = self
            .build_wb_advert_documents(connection, &all_stats)
            .await
            .context("Failed to build backfill advert documents")?;
        let document_ids: Vec<Uuid> = documents.iter().map(|d| d.base.id.value()).collect();

        successful_advert_ids.sort_unstable();
        successful_advert_ids.dedup();
        // Кандидаты не имеют покрытия в [begin_date, end_date], поэтому scoped-replace
        // ничего лишнего не удаляет — только вставляет догруженные документы.
        crate::domain::a026_wb_advert_daily::service::replace_for_period_advert_ids(
            &connection_id,
            &begin_date,
            &end_date,
            &successful_advert_ids,
            &documents,
        )
        .await
        .context("Failed to store backfill advert documents")?;

        let mut posting_context =
            AdvertPostingContext::prefetched(&connection_id, &begin_date, &end_date)
                .await
                .context("Failed to prefetch backfill advert posting context")?;
        for document_id in &document_ids {
            post_wb_advert_document_with_retry(*document_id, &mut posting_context)
                .await
                .with_context(|| {
                    format!("Failed to post backfill advert document {}", document_id)
                })?;
        }

        tracing::info!(
            "WB Advert backfill completed: connection={}, campaigns={}, documents={}, range={}..{}, rate_limited={}",
            connection_id,
            successful_advert_ids.len(),
            document_ids.len(),
            begin_date,
            end_date,
            stopped_by_rate_limit,
        );

        Ok(())
    }

    async fn build_wb_advert_documents(
        &self,
        connection: &contracts::domain::a006_connection_mp::aggregate::ConnectionMP,
        stats: &[WbAdvertFullStat],
    ) -> Result<Vec<WbAdvertDaily>> {
        let mut by_doc: BTreeMap<(String, i64), AdvertDayAccumulator> = BTreeMap::new();

        for stat in stats {
            for day in &stat.days {
                let date_key = normalize_day_date(&day.date);
                let doc_acc = by_doc.entry((date_key, stat.advert_id)).or_default();
                append_metrics(&mut doc_acc.totals, &metrics_from_day(day));

                for app in &day.apps {
                    self.accumulate_day_app(doc_acc, stat.advert_id, app);
                }
            }
        }

        let total_line_groups: usize = by_doc.values().map(|day| day.lines.len()).sum();
        tracing::info!(
            "WB Advert document build prepared: connection={}, documents={}, nm_groups={}",
            connection.to_string_id(),
            by_doc.len(),
            total_line_groups
        );

        let mut nomenclature_cache: HashMap<i64, Option<String>> = HashMap::new();
        let mut documents = Vec::with_capacity(by_doc.len());

        for ((document_date, advert_id), mut day_acc) in by_doc {
            let mut lines = Vec::with_capacity(day_acc.lines.len());
            let mut attributed_totals = WbAdvertDailyMetrics::default();

            for (nm_id, line_acc) in &mut day_acc.lines {
                let nomenclature_ref = self
                    .resolve_wb_nomenclature_ref(connection, *nm_id, &mut nomenclature_cache)
                    .await?;

                let mut metrics = line_acc.metrics.clone();
                finalize_metrics(&mut metrics);
                append_metrics(&mut attributed_totals, &metrics);

                lines.push(WbAdvertDailyLine {
                    nm_id: *nm_id,
                    nm_name: line_acc.nm_name.clone(),
                    nomenclature_ref,
                    advert_ids: line_acc.advert_ids.iter().copied().collect(),
                    app_types: line_acc.app_types.iter().copied().collect(),
                    placements: line_acc.placements.iter().cloned().collect(),
                    metrics,
                });
            }

            lines.sort_by(|a, b| {
                a.nm_name
                    .to_lowercase()
                    .cmp(&b.nm_name.to_lowercase())
                    .then_with(|| a.nm_id.cmp(&b.nm_id))
            });

            let mut totals = day_acc.totals.clone();
            finalize_metrics(&mut totals);

            let mut unattributed_totals =
                crate::domain::a026_wb_advert_daily::repository::subtract_metrics(
                    &day_acc.totals,
                    &attributed_totals,
                );
            finalize_metrics(&mut unattributed_totals);

            let header = WbAdvertDailyHeader {
                document_no: format!("WB-ADV-{}-{}", advert_id, document_date),
                document_date: document_date.clone(),
                advert_id,
                connection_id: connection.to_string_id(),
                organization_id: connection.organization_ref.clone(),
                marketplace_id: connection.marketplace_id.clone(),
            };

            let source_meta = WbAdvertDailySourceMeta {
                source: "wb_advert_stats".to_string(),
                fetched_at: chrono::Utc::now().to_rfc3339(),
            };

            let mut document = WbAdvertDaily::new_for_insert(
                header,
                totals,
                unattributed_totals,
                lines,
                source_meta,
            );
            document.before_write();
            document.validate().map_err(|e| anyhow::anyhow!(e))?;
            documents.push(document);
        }

        Ok(documents)
    }

    fn accumulate_day_app(
        &self,
        day_acc: &mut AdvertDayAccumulator,
        advert_id: i64,
        app: &WbAdvertFullStatApp,
    ) {
        for nm in &app.nms {
            let line = day_acc.lines.entry(nm.nm_id).or_default();
            if line.nm_name.is_empty() {
                line.nm_name = nm.name.clone().unwrap_or_default();
            }
            append_metrics(&mut line.metrics, &metrics_from_nm(nm));
            line.advert_ids.insert(advert_id);
            line.app_types.insert(app.app_type);
        }
    }

    async fn resolve_wb_nomenclature_ref(
        &self,
        connection: &contracts::domain::a006_connection_mp::aggregate::ConnectionMP,
        nm_id: i64,
        cache: &mut HashMap<i64, Option<String>>,
    ) -> Result<Option<String>> {
        if let Some(cached) = cache.get(&nm_id) {
            return Ok(cached.clone());
        }

        let resolved =
            crate::domain::a007_marketplace_product::service::resolve_wb_nomenclature_ref(
                &connection.to_string_id(),
                nm_id,
                None,
            )
            .await?;
        cache.insert(nm_id, resolved.clone());
        Ok(resolved)
    }

    /// Загрузка воронки продаж WB (Analytics API v3, sales-funnel).
    /// Один документ a036 = один кабинет + одна дата; строки — товары (nm_id).
    /// Данные доступны примерно за последнюю неделю; лимит 3 запроса/мин.
    async fn import_wb_sales_funnel(
        &self,
        session_id: &str,
        connection: &contracts::domain::a006_connection_mp::aggregate::ConnectionMP,
        date_from: chrono::NaiveDate,
        date_to: chrono::NaiveDate,
    ) -> Result<()> {
        let aggregate_index = "a036_wb_sales_funnel_daily";
        let begin_date = date_from.format("%Y-%m-%d").to_string();
        let end_date = date_to.format("%Y-%m-%d").to_string();

        tracing::info!(
            "WB Sales funnel: session={}, period={} to {}",
            session_id,
            begin_date,
            end_date
        );

        // Discovery: постранично собираем nmID товаров с активностью за период.
        self.progress_tracker.set_current_item(
            session_id,
            aggregate_index,
            Some("Поиск товаров с активностью за период".to_string()),
        );

        let mut nm_ids: Vec<i64> = Vec::new();
        let mut offset = 0usize;
        loop {
            // Ретраи транзиентных сбоев (обрыв TLS-хэндшейка): одна ошибка discovery
            // иначе завалила бы весь импорт. Rate limit не ретраим.
            let mut attempt = 0usize;
            let (page_ids, is_next_page) = loop {
                attempt += 1;
                match self
                    .api_client
                    .fetch_sales_funnel_products(
                        connection,
                        &begin_date,
                        &end_date,
                        offset,
                        WB_SALES_FUNNEL_PAGE_LIMIT,
                    )
                    .await
                {
                    Ok(page) => break page,
                    Err(e) => {
                        let error_text = e.to_string();
                        if attempt >= WB_SALES_FUNNEL_MAX_ATTEMPTS
                            || is_wb_advert_fullstats_rate_limit(&error_text)
                        {
                            return Err(e).with_context(|| {
                                format!(
                                    "Failed to discover sales-funnel products for connection={} period={}..{} offset={} after {} attempts",
                                    connection.to_string_id(),
                                    begin_date,
                                    end_date,
                                    offset,
                                    attempt
                                )
                            });
                        }
                        tracing::warn!(
                            "WB sales-funnel discovery offset={} attempt {}/{} failed ({}); retrying in {}s",
                            offset,
                            attempt,
                            WB_SALES_FUNNEL_MAX_ATTEMPTS,
                            error_text,
                            WB_SALES_FUNNEL_REQUEST_DELAY_SECS
                        );
                        tokio::time::sleep(tokio::time::Duration::from_secs(
                            WB_SALES_FUNNEL_REQUEST_DELAY_SECS,
                        ))
                        .await;
                    }
                }
            };
            nm_ids.extend(page_ids);
            if !is_next_page {
                break;
            }
            offset += WB_SALES_FUNNEL_PAGE_LIMIT;
            tokio::time::sleep(tokio::time::Duration::from_secs(
                WB_SALES_FUNNEL_REQUEST_DELAY_SECS,
            ))
            .await;
        }
        nm_ids.sort_unstable();
        nm_ids.dedup();

        if nm_ids.is_empty() {
            tracing::info!(
                "WB Sales funnel: no products with activity, clearing existing documents"
            );
            crate::domain::a036_wb_sales_funnel_daily::service::replace_for_period(
                &connection.to_string_id(),
                &begin_date,
                &end_date,
                &[],
            )
            .await?;
            self.progress_tracker
                .complete_aggregate(session_id, aggregate_index);
            return Ok(());
        }

        let chunks: Vec<&[i64]> = nm_ids.chunks(WB_SALES_FUNNEL_CHUNK_SIZE).collect();
        let total_chunks = chunks.len();
        tracing::info!(
            "WB Sales funnel: {} nm_ids → {} chunks of up to {} (delay {}s each)",
            nm_ids.len(),
            total_chunks,
            WB_SALES_FUNNEL_CHUNK_SIZE,
            WB_SALES_FUNNEL_REQUEST_DELAY_SECS,
        );
        self.progress_tracker.update_aggregate(
            session_id,
            aggregate_index,
            0,
            Some(nm_ids.len() as i32),
            0,
            0,
        );

        let mut processed_nm_ids = 0i32;
        let mut had_fetch_errors = false;
        let mut all_items: Vec<WbSalesFunnelHistoryItem> = Vec::new();

        // Пауза перед первым history-запросом: discovery и history делят лимит метода 3/мин.
        tokio::time::sleep(tokio::time::Duration::from_secs(
            WB_SALES_FUNNEL_REQUEST_DELAY_SECS,
        ))
        .await;

        for (chunk_idx, chunk) in chunks.iter().enumerate() {
            self.progress_tracker.set_current_item(
                session_id,
                aggregate_index,
                Some(format!(
                    "Чанк {}/{} (nmIds: {}..)",
                    chunk_idx + 1,
                    total_chunks,
                    chunk[0]
                )),
            );

            // Ретраи транзиентных сбоев (обрыв TLS-хэндшейка и т.п.): следующий
            // запрос обычно проходит. Rate limit (429) не ретраим — прерываем импорт.
            let mut attempt = 0usize;
            let fetch_result = loop {
                attempt += 1;
                match self
                    .api_client
                    .fetch_sales_funnel_history(connection, chunk, &begin_date, &end_date)
                    .await
                {
                    Ok(items) => break Ok(items),
                    Err(e) => {
                        let error_text = e.to_string();
                        if is_wb_advert_fullstats_rate_limit(&error_text) {
                            break Err((error_text, true));
                        }
                        if attempt >= WB_SALES_FUNNEL_MAX_ATTEMPTS {
                            break Err((error_text, false));
                        }
                        tracing::warn!(
                            "WB sales-funnel history chunk {}/{} attempt {}/{} failed ({}); retrying in {}s",
                            chunk_idx + 1,
                            total_chunks,
                            attempt,
                            WB_SALES_FUNNEL_MAX_ATTEMPTS,
                            error_text,
                            WB_SALES_FUNNEL_REQUEST_DELAY_SECS
                        );
                        tokio::time::sleep(tokio::time::Duration::from_secs(
                            WB_SALES_FUNNEL_REQUEST_DELAY_SECS,
                        ))
                        .await;
                    }
                }
            };

            match fetch_result {
                Ok(items) => {
                    processed_nm_ids += chunk.len() as i32;
                    all_items.extend(items);
                }
                Err((error_text, is_rate_limit)) => {
                    had_fetch_errors = true;
                    tracing::warn!(
                        "Failed to fetch sales-funnel history for connection={} period={}..{} chunk {}/{} after {} attempts error={}",
                        connection.to_string_id(),
                        begin_date,
                        end_date,
                        chunk_idx + 1,
                        total_chunks,
                        attempt,
                        error_text
                    );
                    self.progress_tracker.add_error(
                        session_id,
                        Some(aggregate_index.to_string()),
                        format!(
                            "WB воронка продаж: чанк {}/{} не загружен для кабинета {}",
                            chunk_idx + 1,
                            total_chunks,
                            connection.to_string_id()
                        ),
                        Some(error_text),
                    );

                    if is_rate_limit {
                        break;
                    }
                }
            }

            self.progress_tracker.update_aggregate(
                session_id,
                aggregate_index,
                processed_nm_ids,
                Some(nm_ids.len() as i32),
                all_items.len() as i32,
                0,
            );

            if chunk_idx + 1 < total_chunks {
                tokio::time::sleep(tokio::time::Duration::from_secs(
                    WB_SALES_FUNNEL_REQUEST_DELAY_SECS,
                ))
                .await;
            }
        }

        // Полный провал (не получили ни одной строки) — не трогаем существующие
        // данные и завершаемся ошибкой. Если же часть чанков загрузилась,
        // сохраняем частичный результат: воронка пересобирается при каждом
        // повторном импорте, поэтому недостающие товары дозагрузятся позже, а
        // «всё или ничего» на флаки-сети практически всегда давало бы пустой импорт.
        if all_items.is_empty() {
            if had_fetch_errors {
                anyhow::bail!(
                    "WB sales-funnel history failed for all {} chunks; existing a036 data was left unchanged",
                    total_chunks
                );
            }
            tracing::info!(
                "WB Sales funnel: no rows returned for period {}..{}, clearing existing documents",
                begin_date,
                end_date
            );
        } else if had_fetch_errors {
            self.progress_tracker.add_error(
                session_id,
                Some(aggregate_index.to_string()),
                "Часть чанков воронки продаж не загрузилась; сохранён частичный результат, повторите импорт для полноты"
                    .to_string(),
                None,
            );
        }

        let documents = self
            .build_wb_sales_funnel_documents(connection, &all_items, &begin_date, &end_date)
            .await
            .with_context(|| {
                format!(
                    "Failed during WB sales-funnel document build for connection={} period={}..{}",
                    connection.to_string_id(),
                    begin_date,
                    end_date
                )
            })?;

        let documents_count =
            crate::domain::a036_wb_sales_funnel_daily::service::replace_for_period(
                &connection.to_string_id(),
                &begin_date,
                &end_date,
                &documents,
            )
            .await
            .with_context(|| {
                format!(
                    "Failed during WB sales-funnel replace_for_period for connection={} period={}..{} documents={}",
                    connection.to_string_id(),
                    begin_date,
                    end_date,
                    documents.len()
                )
            })?;

        self.progress_tracker.update_aggregate(
            session_id,
            aggregate_index,
            processed_nm_ids,
            Some(nm_ids.len() as i32),
            documents_count as i32,
            0,
        );
        self.progress_tracker
            .complete_aggregate(session_id, aggregate_index);
        tracing::info!(
            "WB Sales funnel completed: connection={}, period={}..{}, nm_ids={}, documents={}",
            connection.to_string_id(),
            begin_date,
            end_date,
            nm_ids.len(),
            documents_count
        );

        Ok(())
    }

    /// Историческая загрузка воронки WB через асинхронный CSV-отчёт
    /// `DETAIL_HISTORY_REPORT`. Это отдельная цель u504; оперативная загрузка
    /// `a036_wb_sales_funnel_daily` через `/products/history` не изменяется.
    async fn import_wb_sales_funnel_history_report(
        &self,
        session_id: &str,
        connection: &contracts::domain::a006_connection_mp::aggregate::ConnectionMP,
        date_from: chrono::NaiveDate,
        date_to: chrono::NaiveDate,
    ) -> Result<()> {
        let aggregate_index = "a036_wb_sales_funnel_daily_history";
        if date_from > date_to {
            anyhow::bail!(
                "Invalid WB DETAIL_HISTORY_REPORT period: {} is after {}",
                date_from,
                date_to
            );
        }
        if (date_to - date_from).num_days() >= 365 {
            anyhow::bail!(
                "WB DETAIL_HISTORY_REPORT period must not exceed 365 days: {}..{}",
                date_from,
                date_to
            );
        }

        let begin_date = date_from.format("%Y-%m-%d").to_string();
        let end_date = date_to.format("%Y-%m-%d").to_string();
        let download_id = Uuid::new_v4();
        self.progress_tracker.set_current_item(
            session_id,
            aggregate_index,
            Some(format!(
                "Создание DETAIL_HISTORY_REPORT {}..{}",
                begin_date, end_date
            )),
        );

        self.api_client
            .create_sales_funnel_detail_report(connection, download_id, &begin_date, &end_date)
            .await?;

        let report_status = {
            let mut ready = None;
            for poll in 1..=WB_DETAIL_HISTORY_MAX_POLLS {
                tokio::time::sleep(tokio::time::Duration::from_secs(
                    WB_DETAIL_HISTORY_POLL_INTERVAL_SECS,
                ))
                .await;
                self.progress_tracker.set_current_item(
                    session_id,
                    aggregate_index,
                    Some(format!(
                        "Ожидание DETAIL_HISTORY_REPORT, проверка {}/{}",
                        poll, WB_DETAIL_HISTORY_MAX_POLLS
                    )),
                );
                let report = self
                    .api_client
                    .get_sales_funnel_detail_report_status(connection, download_id)
                    .await?;
                match report.status.trim().to_ascii_uppercase().as_str() {
                    "SUCCESS" => {
                        ready = Some(report);
                        break;
                    }
                    "FAILED" => {
                        anyhow::bail!(
                            "WB DETAIL_HISTORY_REPORT generation failed: download_id={}",
                            download_id
                        );
                    }
                    _ => {
                        tracing::info!(
                            "WB DETAIL_HISTORY_REPORT waiting: download_id={}, status={}, poll={}",
                            download_id,
                            report.status,
                            poll
                        );
                    }
                }
            }
            ready.ok_or_else(|| {
                anyhow::anyhow!(
                    "WB DETAIL_HISTORY_REPORT did not become ready after {} checks: download_id={}",
                    WB_DETAIL_HISTORY_MAX_POLLS,
                    download_id
                )
            })?
        };

        if !report_status.start_date.is_empty()
            && normalize_day_date(&report_status.start_date) != begin_date
        {
            anyhow::bail!(
                "WB DETAIL_HISTORY_REPORT start date mismatch: requested={}, report={}",
                begin_date,
                report_status.start_date
            );
        }
        if !report_status.end_date.is_empty()
            && normalize_day_date(&report_status.end_date) != end_date
        {
            anyhow::bail!(
                "WB DETAIL_HISTORY_REPORT end date mismatch: requested={}, report={}",
                end_date,
                report_status.end_date
            );
        }

        self.progress_tracker.set_current_item(
            session_id,
            aggregate_index,
            Some(format!("Скачивание DETAIL_HISTORY_REPORT {}", download_id)),
        );
        let zip_bytes = self
            .api_client
            .download_sales_funnel_detail_report(connection, download_id)
            .await?;
        let rows = tokio::task::spawn_blocking(move || parse_sales_funnel_detail_zip(&zip_bytes))
            .await
            .context("WB DETAIL_HISTORY_REPORT parser task failed")??;

        let documents = self
            .build_wb_sales_funnel_history_documents(connection, &rows, &begin_date, &end_date)
            .await?;

        let documents_count =
            crate::domain::a036_wb_sales_funnel_daily::service::replace_for_period(
                &connection.to_string_id(),
                &begin_date,
                &end_date,
                &documents,
            )
            .await?;

        self.progress_tracker.update_aggregate(
            session_id,
            aggregate_index,
            rows.len() as i32,
            Some(rows.len() as i32),
            documents_count as i32,
            0,
        );
        self.progress_tracker
            .complete_aggregate(session_id, aggregate_index);
        tracing::info!(
            "WB DETAIL_HISTORY_REPORT completed: connection={}, download_id={}, report_size={}, rows={}, documents={}",
            connection.to_string_id(),
            download_id,
            report_status.size,
            rows.len(),
            documents_count
        );
        Ok(())
    }

    async fn build_wb_sales_funnel_history_documents(
        &self,
        connection: &contracts::domain::a006_connection_mp::aggregate::ConnectionMP,
        rows: &[WbSalesFunnelDetailRow],
        date_from: &str,
        date_to: &str,
    ) -> Result<Vec<WbSalesFunnelDaily>> {
        let products = crate::domain::a007_marketplace_product::repository::list_by_connection(
            &connection.to_string_id(),
        )
        .await?;
        let mut candidates_by_nm_id: HashMap<i64, Vec<_>> = HashMap::new();
        for product in products {
            let Ok(nm_id) = product.marketplace_sku.trim().parse::<i64>() else {
                continue;
            };
            candidates_by_nm_id.entry(nm_id).or_default().push(product);
        }
        let mut products_by_nm_id = HashMap::new();
        let mut duplicate_nm_ids = 0usize;
        let mut conflicting_nomenclature_nm_ids = 0usize;
        for (nm_id, candidates) in candidates_by_nm_id {
            let candidate_count = candidates.len();
            let candidate_ids: Vec<String> =
                candidates.iter().map(|item| item.to_string_id()).collect();
            let (selected, has_conflicting_nomenclature) = select_wb_product_enrichment(candidates);
            if candidate_count > 1 {
                duplicate_nm_ids += 1;
                if has_conflicting_nomenclature {
                    conflicting_nomenclature_nm_ids += 1;
                }
                tracing::warn!(
                    "WB DETAIL_HISTORY_REPORT a007 duplicate: connection={}, nmID={}, candidates={}, selected={}, conflicting_nomenclature={}, candidate_ids={:?}",
                    connection.to_string_id(),
                    nm_id,
                    candidate_count,
                    selected.to_string_id(),
                    has_conflicting_nomenclature,
                    candidate_ids
                );
            }
            products_by_nm_id.insert(nm_id, selected);
        }
        tracing::info!(
            "WB DETAIL_HISTORY_REPORT a007 enrichment: products={}, duplicate_nm_ids={}, conflicting_nomenclature_nm_ids={}",
            products_by_nm_id.len(),
            duplicate_nm_ids,
            conflicting_nomenclature_nm_ids
        );

        let mut seen_keys = HashSet::new();
        let mut by_date: BTreeMap<String, Vec<WbSalesFunnelDailyLine>> = BTreeMap::new();
        let mut currency_by_date: HashMap<String, String> = HashMap::new();
        let mut imported_cancel_count = 0i64;
        let mut imported_cancel_sum = 0.0f64;

        for row in rows {
            let date = normalize_day_date(&row.date);
            chrono::NaiveDate::parse_from_str(&date, "%Y-%m-%d").with_context(|| {
                format!(
                    "Invalid dt in WB DETAIL_HISTORY_REPORT: nmID={}, dt={}",
                    row.nm_id, row.date
                )
            })?;
            if date.as_str() < date_from || date.as_str() > date_to {
                anyhow::bail!(
                    "WB DETAIL_HISTORY_REPORT row is outside requested period: nmID={}, dt={}, period={}..{}",
                    row.nm_id,
                    row.date,
                    date_from,
                    date_to
                );
            }
            if row.nm_id <= 0 {
                anyhow::bail!(
                    "WB DETAIL_HISTORY_REPORT contains invalid nmID={} on {}",
                    row.nm_id,
                    date
                );
            }
            let key = (date.clone(), row.nm_id);
            if !seen_keys.insert(key) {
                anyhow::bail!(
                    "WB DETAIL_HISTORY_REPORT contains duplicate nmID={} for {}",
                    row.nm_id,
                    date
                );
            }
            if row.open_count < 0
                || row.cart_count < 0
                || row.order_count < 0
                || row.buyout_count < 0
                || row.cancel_count < 0
                || row.add_to_wishlist_count < 0
                || !row.order_sum.is_finite()
                || row.order_sum < 0.0
                || !row.buyout_sum.is_finite()
                || row.buyout_sum < 0.0
                || !row.cancel_sum.is_finite()
                || row.cancel_sum < 0.0
                || !row.add_to_cart_conversion.is_finite()
                || row.add_to_cart_conversion < 0.0
                || !row.cart_to_order_conversion.is_finite()
                || row.cart_to_order_conversion < 0.0
                || !row.buyout_percent.is_finite()
                || row.buyout_percent < 0.0
            {
                anyhow::bail!(
                    "WB DETAIL_HISTORY_REPORT contains invalid metrics: nmID={}, dt={}",
                    row.nm_id,
                    date
                );
            }

            let currency = row.currency.trim();
            if currency.is_empty() {
                anyhow::bail!(
                    "WB DETAIL_HISTORY_REPORT contains empty currency: nmID={}, dt={}",
                    row.nm_id,
                    date
                );
            }
            match currency_by_date.get(&date) {
                Some(existing) if existing != currency => {
                    anyhow::bail!(
                        "WB DETAIL_HISTORY_REPORT contains mixed currencies for {}: {} and {}",
                        date,
                        existing,
                        currency
                    );
                }
                None => {
                    currency_by_date.insert(date.clone(), currency.to_string());
                }
                _ => {}
            }

            imported_cancel_count += row.cancel_count;
            imported_cancel_sum += row.cancel_sum;
            let metrics = WbSalesFunnelDailyMetrics {
                open_count: row.open_count,
                cart_count: row.cart_count,
                order_count: row.order_count,
                order_sum: row.order_sum,
                buyout_count: row.buyout_count,
                buyout_sum: row.buyout_sum,
                // Колонка обязательна в CSV-отчёте (см. WB_DETAIL_HISTORY_REQUIRED_HEADERS),
                // поэтому значение всегда определено — 0 здесь означает «отмен не было».
                cancel_count: Some(row.cancel_count),
                cancel_sum: Some(row.cancel_sum),
                buyout_percent: row.buyout_percent,
                add_to_cart_conversion: row.add_to_cart_conversion,
                cart_to_order_conversion: row.cart_to_order_conversion,
                add_to_wishlist_count: row.add_to_wishlist_count,
            };
            if funnel_metrics_is_empty(&metrics) {
                continue;
            }

            let product = products_by_nm_id.get(&row.nm_id);
            by_date
                .entry(date)
                .or_default()
                .push(WbSalesFunnelDailyLine {
                    nm_id: row.nm_id,
                    title: product
                        .map(|item| item.base.description.clone())
                        .unwrap_or_default(),
                    vendor_code: product.map(|item| item.article.clone()).unwrap_or_default(),
                    brand_name: product
                        .and_then(|item| item.brand.clone())
                        .unwrap_or_default(),
                    subject_id: product
                        .and_then(|item| item.category_id.as_deref())
                        .and_then(|value| value.parse::<i64>().ok())
                        .unwrap_or(0),
                    subject_name: product
                        .and_then(|item| item.category_name.clone())
                        .unwrap_or_default(),
                    nomenclature_ref: product.and_then(|item| item.nomenclature_ref.clone()),
                    metrics,
                });
        }

        let mut documents = Vec::with_capacity(by_date.len());
        for (document_date, mut lines) in by_date {
            lines.sort_by(|a, b| a.nm_id.cmp(&b.nm_id));
            let mut totals = WbSalesFunnelDailyMetrics::default();
            for line in &lines {
                append_funnel_totals(&mut totals, &line.metrics);
            }
            finalize_funnel_totals(&mut totals);

            let header = WbSalesFunnelDailyHeader {
                document_no: format!("WB-SF-{}", document_date),
                document_date: document_date.clone(),
                connection_id: connection.to_string_id(),
                organization_id: connection.organization_ref.clone(),
                marketplace_id: connection.marketplace_id.clone(),
                currency: currency_by_date
                    .get(&document_date)
                    .cloned()
                    .unwrap_or_default(),
            };
            let source_meta = WbSalesFunnelDailySourceMeta {
                source: "wb_detail_history_report".to_string(),
                fetched_at: chrono::Utc::now().to_rfc3339(),
            };
            let mut document =
                WbSalesFunnelDaily::new_for_insert(header, totals, lines, source_meta);
            document.before_write();
            document
                .validate()
                .map_err(|error| anyhow::anyhow!(error))?;
            documents.push(document);
        }

        tracing::info!(
            "WB DETAIL_HISTORY_REPORT cancellations imported into a036 (funnel counter, отличен от order-level a015): count={}, sum={}",
            imported_cancel_count,
            imported_cancel_sum
        );
        Ok(documents)
    }

    /// Ежедневный снимок остатков и рейтингов товаров WB (агрегат a037).
    /// Один проход /products за скользящее окно активности (date_from..date_to)
    /// собирает все товары; документ создаётся за СЕГОДНЯ (snapshot_date), т.к.
    /// остатки/рейтинги — состояние «на сейчас», а не за исторический день.
    async fn import_wb_product_snapshot(
        &self,
        session_id: &str,
        connection: &contracts::domain::a006_connection_mp::aggregate::ConnectionMP,
        date_from: chrono::NaiveDate,
        date_to: chrono::NaiveDate,
    ) -> Result<()> {
        let aggregate_index = "a037_wb_product_snapshot";
        let begin_date = date_from.format("%Y-%m-%d").to_string();
        let end_date = date_to.format("%Y-%m-%d").to_string();
        let snapshot_date = chrono::Local::now().format("%Y-%m-%d").to_string();

        tracing::info!(
            "WB Product snapshot: session={}, activity_window={}..{}, snapshot_date={}",
            session_id,
            begin_date,
            end_date,
            snapshot_date
        );

        self.progress_tracker.set_current_item(
            session_id,
            aggregate_index,
            Some("Сбор остатков и рейтингов товаров".to_string()),
        );

        // Постраничный проход /products (limit/offset) с ретраями транзиентных ошибок.
        let mut rows: Vec<WbProductSnapshotRow> = Vec::new();
        let mut offset = 0usize;
        loop {
            let mut attempt = 0usize;
            let (page_rows, is_next_page) = loop {
                attempt += 1;
                match self
                    .api_client
                    .fetch_sales_funnel_products_full(
                        connection,
                        &begin_date,
                        &end_date,
                        offset,
                        WB_SALES_FUNNEL_PAGE_LIMIT,
                    )
                    .await
                {
                    Ok(page) => break page,
                    Err(e) => {
                        let error_text = e.to_string();
                        if attempt >= WB_SALES_FUNNEL_MAX_ATTEMPTS
                            || is_wb_advert_fullstats_rate_limit(&error_text)
                        {
                            return Err(e).with_context(|| {
                                format!(
                                    "Failed to fetch WB product snapshot for connection={} offset={} after {} attempts",
                                    connection.to_string_id(),
                                    offset,
                                    attempt
                                )
                            });
                        }
                        tracing::warn!(
                            "WB product snapshot offset={} attempt {}/{} failed ({}); retrying in {}s",
                            offset,
                            attempt,
                            WB_SALES_FUNNEL_MAX_ATTEMPTS,
                            error_text,
                            WB_SALES_FUNNEL_REQUEST_DELAY_SECS
                        );
                        tokio::time::sleep(tokio::time::Duration::from_secs(
                            WB_SALES_FUNNEL_REQUEST_DELAY_SECS,
                        ))
                        .await;
                    }
                }
            };
            rows.extend(page_rows);
            self.progress_tracker.update_aggregate(
                session_id,
                aggregate_index,
                rows.len() as i32,
                None,
                rows.len() as i32,
                0,
            );
            if !is_next_page {
                break;
            }
            offset += WB_SALES_FUNNEL_PAGE_LIMIT;
            tokio::time::sleep(tokio::time::Duration::from_secs(
                WB_SALES_FUNNEL_REQUEST_DELAY_SECS,
            ))
            .await;
        }

        let document = self
            .build_wb_product_snapshot_document(connection, &rows, &snapshot_date)
            .await
            .with_context(|| {
                format!(
                    "Failed during WB product snapshot build for connection={} date={}",
                    connection.to_string_id(),
                    snapshot_date
                )
            })?;

        let documents_count = crate::domain::a037_wb_product_snapshot::service::replace_for_period(
            &connection.to_string_id(),
            &snapshot_date,
            &snapshot_date,
            &[document],
        )
        .await
        .with_context(|| {
            format!(
                "Failed during WB product snapshot replace_for_period for connection={} date={}",
                connection.to_string_id(),
                snapshot_date
            )
        })?;

        self.progress_tracker.update_aggregate(
            session_id,
            aggregate_index,
            rows.len() as i32,
            Some(rows.len() as i32),
            documents_count as i32,
            0,
        );
        self.progress_tracker
            .complete_aggregate(session_id, aggregate_index);
        tracing::info!(
            "WB Product snapshot completed: connection={}, snapshot_date={}, products={}",
            connection.to_string_id(),
            snapshot_date,
            rows.len()
        );

        Ok(())
    }

    async fn build_wb_product_snapshot_document(
        &self,
        connection: &contracts::domain::a006_connection_mp::aggregate::ConnectionMP,
        rows: &[WbProductSnapshotRow],
        snapshot_date: &str,
    ) -> Result<WbProductSnapshot> {
        let mut nomenclature_cache: HashMap<i64, Option<String>> = HashMap::new();
        let mut seen: HashSet<i64> = HashSet::new();
        let mut lines: Vec<WbProductSnapshotLine> = Vec::with_capacity(rows.len());
        let mut totals = WbProductSnapshotTotals::default();

        for row in rows {
            // dedup по nm_id — последнее значение в проходе
            if !seen.insert(row.nm_id) {
                if let Some(existing) = lines.iter_mut().find(|l| l.nm_id == row.nm_id) {
                    existing.state = WbProductSnapshotState {
                        stock_wb: row.stock_wb,
                        stock_mp: row.stock_mp,
                        stock_balance_sum: row.stock_balance_sum,
                        product_rating: row.product_rating,
                        feedback_rating: row.feedback_rating,
                    };
                }
                continue;
            }
            let nomenclature_ref = self
                .resolve_wb_nomenclature_ref(connection, row.nm_id, &mut nomenclature_cache)
                .await?;
            lines.push(WbProductSnapshotLine {
                nm_id: row.nm_id,
                title: row.title.clone(),
                vendor_code: row.vendor_code.clone(),
                brand_name: row.brand_name.clone(),
                subject_id: row.subject_id,
                subject_name: row.subject_name.clone(),
                nomenclature_ref,
                state: WbProductSnapshotState {
                    stock_wb: row.stock_wb,
                    stock_mp: row.stock_mp,
                    stock_balance_sum: row.stock_balance_sum,
                    product_rating: row.product_rating,
                    feedback_rating: row.feedback_rating,
                },
            });
        }

        lines.sort_by(|a, b| {
            a.title
                .to_lowercase()
                .cmp(&b.title.to_lowercase())
                .then_with(|| a.nm_id.cmp(&b.nm_id))
        });

        for line in &lines {
            totals.total_stock_wb += line.state.stock_wb;
            totals.total_stock_mp += line.state.stock_mp;
            totals.total_balance_sum += line.state.stock_balance_sum;
        }

        let header = WbProductSnapshotHeader {
            document_no: format!("WB-SNAP-{}", snapshot_date),
            snapshot_date: snapshot_date.to_string(),
            connection_id: connection.to_string_id(),
            organization_id: connection.organization_ref.clone(),
            marketplace_id: connection.marketplace_id.clone(),
        };
        let source_meta = WbProductSnapshotSourceMeta {
            source: "wb_product_snapshot".to_string(),
            fetched_at: chrono::Utc::now().to_rfc3339(),
        };

        let mut document = WbProductSnapshot::new_for_insert(header, totals, lines, source_meta);
        document.before_write();
        document.validate().map_err(|e| anyhow::anyhow!(e))?;
        Ok(document)
    }

    /// Импорт поисковой аналитики WB (a040): один снимок за сегодня по кабинету.
    /// Видимость/позиции/переходы из search-report + топ-запросы из search-texts. В воронку
    /// p916 НЕ входит: `/table/details` даёт только `visibility` (%), не счётчик показов, поэтому
    /// `show_free_count` остаётся `N/A`. Требует подписки «Джем» — при 403/пустом ответе мягко
    /// деградирует (лог + пустой прогон).
    async fn import_wb_search_analytics(
        &self,
        session_id: &str,
        connection: &contracts::domain::a006_connection_mp::aggregate::ConnectionMP,
        _date_from: chrono::NaiveDate,
        date_to: chrono::NaiveDate,
    ) -> Result<()> {
        let aggregate_index = "a040_wb_search_analytics_daily";
        // Снимок за один день (date_to, обычно = сегодня): период отчёта = этот день.
        let snapshot_date = date_to.format("%Y-%m-%d").to_string();

        tracing::info!(
            "WB Search analytics: session={}, snapshot_date={}",
            session_id,
            snapshot_date
        );
        self.progress_tracker.set_current_item(
            session_id,
            aggregate_index,
            Some("Сбор поисковой аналитики (видимость, позиции)".to_string()),
        );

        let rows = match self
            .api_client
            .fetch_search_report(connection, &snapshot_date, &snapshot_date)
            .await
        {
            Ok(rows) => rows,
            Err(e) => {
                // Нет «Джем»/доступа или временная ошибка — не роняем прогон.
                let msg = format!("WB search-report недоступен: {}", e);
                tracing::warn!("{}", msg);
                self.progress_tracker.add_error(
                    session_id,
                    Some(aggregate_index.to_string()),
                    msg,
                    None,
                );
                self.progress_tracker
                    .complete_aggregate(session_id, aggregate_index);
                return Ok(());
            }
        };

        // Топ поисковых запросов по товарам (best-effort, чанками по 20 nm_id).
        if rows.is_empty() {
            let msg = format!(
                "WB search-report вернул 0 товаров за {} (документ a040 не создан)",
                snapshot_date
            );
            tracing::warn!("{}", msg);
            self.progress_tracker.add_error(
                session_id,
                Some(aggregate_index.to_string()),
                msg,
                None,
            );
            self.progress_tracker
                .complete_aggregate(session_id, aggregate_index);
            return Ok(());
        }

        // Search-text details are enrichment only. Fetch them once for up to 50 products
        // with actual search activity; requesting every product with a 21-second rate-limit
        // pause made one cabinet take several minutes.
        let nm_ids: Vec<i64> = rows
            .iter()
            .filter(|row| row.open_card > 0 || row.add_to_cart > 0 || row.orders > 0)
            .take(50)
            .map(|row| row.nm_id)
            .collect();
        let mut queries_by_nm: HashMap<i64, Vec<WbSearchQueryRow>> = HashMap::new();
        if !nm_ids.is_empty() {
            match self
                .api_client
                .fetch_search_texts(connection, &nm_ids, &snapshot_date, &snapshot_date, 30)
                .await
            {
                Ok(qrows) => {
                    for query in qrows {
                        queries_by_nm.entry(query.nm_id).or_default().push(query);
                    }
                }
                Err(error) => tracing::warn!(
                    "WB search-texts enrichment failed ({}); document will be saved without queries",
                    error
                ),
            }
        }

        let document = self
            .build_wb_search_analytics_document(connection, &rows, &queries_by_nm, &snapshot_date)
            .await?;

        let documents_count =
            crate::domain::a040_wb_search_analytics_daily::service::replace_for_period(
                &connection.to_string_id(),
                &snapshot_date,
                &snapshot_date,
                &[document],
            )
            .await?;

        self.progress_tracker.update_aggregate(
            session_id,
            aggregate_index,
            rows.len() as i32,
            Some(rows.len() as i32),
            documents_count as i32,
            0,
        );
        self.progress_tracker
            .complete_aggregate(session_id, aggregate_index);
        tracing::info!(
            "WB Search analytics completed: connection={}, snapshot_date={}, products={}",
            connection.to_string_id(),
            snapshot_date,
            rows.len()
        );
        Ok(())
    }

    async fn build_wb_search_analytics_document(
        &self,
        connection: &contracts::domain::a006_connection_mp::aggregate::ConnectionMP,
        rows: &[WbSearchReportRow],
        queries_by_nm: &HashMap<i64, Vec<WbSearchQueryRow>>,
        snapshot_date: &str,
    ) -> Result<WbSearchAnalyticsDaily> {
        let mut nomenclature_cache: HashMap<i64, Option<String>> = HashMap::new();
        let mut seen: HashSet<i64> = HashSet::new();
        let mut lines: Vec<WbSearchAnalyticsDailyLine> = Vec::with_capacity(rows.len());
        let mut totals = WbSearchAnalyticsDailyTotals::default();

        for row in rows {
            if !seen.insert(row.nm_id) {
                continue;
            }
            let nomenclature_ref = self
                .resolve_wb_nomenclature_ref(connection, row.nm_id, &mut nomenclature_cache)
                .await?;
            let top_queries = queries_by_nm
                .get(&row.nm_id)
                .map(|qs| {
                    qs.iter()
                        .map(|q| WbSearchQueryStat {
                            text: q.text.clone(),
                            frequency: q.frequency,
                            impressions: q.impressions,
                            clicks: q.clicks,
                            orders: q.orders,
                            avg_position: q.avg_position,
                        })
                        .collect()
                })
                .unwrap_or_default();
            lines.push(WbSearchAnalyticsDailyLine {
                nm_id: row.nm_id,
                title: row.title.clone(),
                vendor_code: row.vendor_code.clone(),
                brand_name: row.brand_name.clone(),
                subject_id: row.subject_id,
                subject_name: row.subject_name.clone(),
                nomenclature_ref,
                metrics: WbSearchMetrics {
                    impressions: row.impressions,
                    open_card: row.open_card,
                    ctr: row.ctr,
                    add_to_cart: row.add_to_cart,
                    orders: row.orders,
                    avg_position: row.avg_position,
                    visibility: row.visibility,
                    open_to_cart_conv: 0.0,
                    cart_to_order_conv: 0.0,
                },
                top_queries,
            });
        }

        lines.sort_by(|a, b| b.metrics.impressions.cmp(&a.metrics.impressions));
        for line in &lines {
            totals.total_impressions += line.metrics.impressions;
            totals.total_open_card += line.metrics.open_card;
            totals.total_orders += line.metrics.orders;
        }

        let header = WbSearchAnalyticsDailyHeader {
            document_no: format!("WB-SEARCH-{}", snapshot_date),
            snapshot_date: snapshot_date.to_string(),
            connection_id: connection.to_string_id(),
            organization_id: connection.organization_ref.clone(),
            marketplace_id: connection.marketplace_id.clone(),
        };
        let source_meta = WbSearchAnalyticsDailySourceMeta {
            source: "wb_search_analytics".to_string(),
            fetched_at: chrono::Utc::now().to_rfc3339(),
        };

        let mut document =
            WbSearchAnalyticsDaily::new_for_insert(header, totals, lines, source_meta);
        document.before_write();
        document.validate().map_err(|e| anyhow::anyhow!(e))?;
        Ok(document)
    }

    async fn build_wb_sales_funnel_documents(
        &self,
        connection: &contracts::domain::a006_connection_mp::aggregate::ConnectionMP,
        items: &[WbSalesFunnelHistoryItem],
        date_from: &str,
        date_to: &str,
    ) -> Result<Vec<WbSalesFunnelDaily>> {
        let mut by_date: BTreeMap<String, Vec<WbSalesFunnelDailyLine>> = BTreeMap::new();
        let mut currency = String::new();
        let mut nomenclature_cache: HashMap<i64, Option<String>> = HashMap::new();

        for item in items {
            if currency.is_empty() && !item.currency.is_empty() {
                currency = item.currency.clone();
            }
            let nomenclature_ref = self
                .resolve_wb_nomenclature_ref(
                    connection,
                    item.product.nm_id,
                    &mut nomenclature_cache,
                )
                .await?;

            for day in &item.history {
                let date_key = normalize_day_date(&day.date);
                if date_key.as_str() < date_from || date_key.as_str() > date_to {
                    continue;
                }
                let metrics = funnel_metrics_from_day(day);
                if funnel_metrics_is_empty(&metrics) {
                    continue;
                }
                by_date
                    .entry(date_key)
                    .or_default()
                    .push(WbSalesFunnelDailyLine {
                        nm_id: item.product.nm_id,
                        title: item.product.title.clone(),
                        vendor_code: item.product.vendor_code.clone(),
                        brand_name: item.product.brand_name.clone(),
                        subject_id: item.product.subject_id,
                        subject_name: item.product.subject_name.clone(),
                        nomenclature_ref: nomenclature_ref.clone(),
                        metrics,
                    });
            }
        }

        let mut documents = Vec::with_capacity(by_date.len());
        for (document_date, mut lines) in by_date {
            lines.sort_by(|a, b| {
                a.title
                    .to_lowercase()
                    .cmp(&b.title.to_lowercase())
                    .then_with(|| a.nm_id.cmp(&b.nm_id))
            });

            let mut totals = WbSalesFunnelDailyMetrics::default();
            for line in &lines {
                append_funnel_totals(&mut totals, &line.metrics);
            }
            finalize_funnel_totals(&mut totals);

            let header = WbSalesFunnelDailyHeader {
                document_no: format!("WB-SF-{}", document_date),
                document_date: document_date.clone(),
                connection_id: connection.to_string_id(),
                organization_id: connection.organization_ref.clone(),
                marketplace_id: connection.marketplace_id.clone(),
                currency: currency.clone(),
            };

            let source_meta = WbSalesFunnelDailySourceMeta {
                source: "wb_sales_funnel".to_string(),
                fetched_at: chrono::Utc::now().to_rfc3339(),
            };

            let mut document =
                WbSalesFunnelDaily::new_for_insert(header, totals, lines, source_meta);
            document.before_write();
            document.validate().map_err(|e| anyhow::anyhow!(e))?;
            documents.push(document);
        }

        Ok(documents)
    }

    /// Загрузка заявок покупателей на возврат WB.
    /// API: GET https://feedbacks-api.wildberries.ru/api/v1/claims
    /// Требует токена с категорией "Buyers Returns".
    async fn import_wb_returns_claims(
        &self,
        session_id: &str,
        connection: &contracts::domain::a006_connection_mp::aggregate::ConnectionMP,
    ) -> Result<()> {
        use crate::domain::a002_organization;
        use crate::domain::a032_wb_returns_claims;
        use crate::shared::marketplaces::wildberries::datetime::parse_wb_datetime;
        use chrono::Utc;
        use contracts::domain::a032_wb_returns_claims::aggregate::WbReturnsClaims;

        let aggregate_index = "a032_wb_returns_claims";
        let mut total_inserted = 0i32;
        let mut total_updated = 0i32;

        self.progress_tracker
            .update_aggregate(session_id, aggregate_index, 0, None, 0, 0);
        self.progress_tracker.set_current_item(
            session_id,
            aggregate_index,
            Some("Запрос WB feedbacks-api /api/v1/claims".to_string()),
        );

        // Разрешаем organization_id и marketplace_id из подключения
        let organization_id = match uuid::Uuid::parse_str(&connection.organization_ref) {
            Ok(org_uuid) => match a002_organization::service::get_by_id(
                crate::shared::data::db::get_connection(),
                org_uuid,
            )
            .await?
            {
                Some(org) => org.base.id.as_string(),
                None => {
                    let msg = format!("Организация '{}' не найдена", connection.organization_ref);
                    self.progress_tracker
                        .fail_aggregate(session_id, aggregate_index, msg.clone());
                    anyhow::bail!("{}", msg);
                }
            },
            Err(_) => {
                let msg = format!(
                    "Некорректный organization_ref: '{}'",
                    connection.organization_ref
                );
                self.progress_tracker
                    .fail_aggregate(session_id, aggregate_index, msg.clone());
                anyhow::bail!("{}", msg);
            }
        };

        let marketplace_id = connection.marketplace_id.clone();

        let claim_rows = match self.api_client.fetch_claims(connection).await {
            Ok(rows) => rows,
            Err(e) => {
                let msg = format!("Не удалось получить заявки на возврат WB: {}", e);
                self.progress_tracker
                    .fail_aggregate(session_id, aggregate_index, msg.clone());
                anyhow::bail!("{}", msg);
            }
        };

        let total = claim_rows.len() as i32;
        tracing::info!("WB Returns Claims: received {} rows", total);
        self.progress_tracker
            .update_aggregate(session_id, aggregate_index, 0, Some(total), 0, 0);

        let connection_id = connection.to_string_id();

        for (idx, row) in claim_rows.iter().enumerate() {
            let nm_id = row.nm_id.unwrap_or(0);

            let parse_dt = |s: &Option<String>| -> Option<chrono::DateTime<Utc>> {
                s.as_deref().and_then(parse_wb_datetime)
            };

            let dt = parse_dt(&row.dt).unwrap_or_else(Utc::now);
            let code = format!("WB-RC-{}", &row.id);
            let description = row
                .imt_name
                .clone()
                .unwrap_or_else(|| format!("Заявка {}", row.id));

            let actions_json = row.actions.as_ref().and_then(|a| {
                if a.is_empty() {
                    None
                } else {
                    serde_json::to_string(a).ok()
                }
            });

            let agg = WbReturnsClaims::new_for_insert(
                code,
                description,
                connection_id.clone(),
                organization_id.clone(),
                marketplace_id.clone(),
                row.id.clone(),
                row.claim_type,
                row.status,
                row.status_ex,
                nm_id,
                row.imt_name.clone(),
                row.user_comment.clone(),
                row.wb_comment.clone(),
                dt,
                parse_dt(&row.order_dt),
                parse_dt(&row.dt_update),
                parse_dt(&row.delivery_dt),
                row.price,
                row.currency_code.clone(),
                row.srid.clone(),
                row.origin_id_info.clone(),
                actions_json,
                row.is_archive,
            );

            match a032_wb_returns_claims::service::upsert(&agg).await {
                Ok((_, inserted)) => {
                    if inserted {
                        total_inserted += 1;
                    } else {
                        total_updated += 1;
                    }
                }
                Err(e) => {
                    tracing::warn!("WB Returns Claims upsert error for {}: {}", row.id, e);
                    self.progress_tracker.add_error(
                        session_id,
                        Some(aggregate_index.to_string()),
                        format!("Upsert error for claim_id={}: {}", row.id, e),
                        None,
                    );
                }
            }

            self.progress_tracker.update_aggregate(
                session_id,
                aggregate_index,
                (idx + 1) as i32,
                Some(total),
                total_inserted,
                total_updated,
            );
        }

        self.progress_tracker
            .complete_aggregate(session_id, aggregate_index);
        tracing::info!(
            "WB Returns Claims import done: inserted={}, updated={}",
            total_inserted,
            total_updated
        );
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

#[cfg(test)]
mod calendar_month_windows_tests {
    use super::calendar_month_windows;
    use chrono::NaiveDate;

    fn date(value: &str) -> NaiveDate {
        NaiveDate::parse_from_str(value, "%Y-%m-%d").expect("valid test date")
    }

    fn windows(from: &str, to: &str) -> Vec<(String, String)> {
        calendar_month_windows(date(from), date(to))
            .into_iter()
            .map(|(start, end)| (start.to_string(), end.to_string()))
            .collect()
    }

    #[test]
    fn splits_period_on_calendar_month_boundaries() {
        assert_eq!(
            windows("2025-09-01", "2025-12-31"),
            vec![
                ("2025-09-01".to_string(), "2025-09-30".to_string()),
                ("2025-10-01".to_string(), "2025-10-31".to_string()),
                ("2025-11-01".to_string(), "2025-11-30".to_string()),
                ("2025-12-01".to_string(), "2025-12-31".to_string()),
            ]
        );
    }

    #[test]
    fn clips_first_and_last_windows_to_the_period() {
        assert_eq!(
            windows("2025-09-14", "2025-11-07"),
            vec![
                ("2025-09-14".to_string(), "2025-09-30".to_string()),
                ("2025-10-01".to_string(), "2025-10-31".to_string()),
                ("2025-11-01".to_string(), "2025-11-07".to_string()),
            ]
        );
    }

    #[test]
    fn single_day_and_within_one_month_stay_one_window() {
        assert_eq!(
            windows("2026-02-10", "2026-02-10"),
            vec![("2026-02-10".to_string(), "2026-02-10".to_string())]
        );
        assert_eq!(
            windows("2026-02-01", "2026-02-28"),
            vec![("2026-02-01".to_string(), "2026-02-28".to_string())]
        );
    }

    #[test]
    fn crosses_year_boundary_and_leap_february() {
        assert_eq!(
            windows("2023-12-20", "2024-02-29"),
            vec![
                ("2023-12-20".to_string(), "2023-12-31".to_string()),
                ("2024-01-01".to_string(), "2024-01-31".to_string()),
                ("2024-02-01".to_string(), "2024-02-29".to_string()),
            ]
        );
    }

    #[test]
    fn inverted_period_yields_no_windows() {
        assert!(windows("2026-03-01", "2026-02-01").is_empty());
    }

    /// Главный инвариант: WB отвергает интервал длиннее 31 дня.
    #[test]
    fn every_window_fits_wb_31_day_limit() {
        let full_year = calendar_month_windows(date("2025-08-01"), date("2026-07-31"));
        assert_eq!(full_year.len(), 12);
        for (start, end) in full_year {
            let span = (end - start).num_days();
            assert!(span >= 0 && span < 31, "window {start}..{end} spans {span}");
        }
    }
}

#[cfg(test)]
mod wb_funnel_enrichment_tests {
    use super::*;
    use contracts::domain::a007_marketplace_product::aggregate::MarketplaceProduct;

    fn product(
        code: &str,
        description: &str,
        article: &str,
        brand: Option<&str>,
        category_id: Option<&str>,
        nomenclature_ref: Option<&str>,
    ) -> MarketplaceProduct {
        MarketplaceProduct::new_for_insert(
            code.to_string(),
            description.to_string(),
            "wb".to_string(),
            "connection".to_string(),
            "372038568".to_string(),
            None,
            article.to_string(),
            brand.map(str::to_string),
            category_id.map(str::to_string),
            None,
            None,
            nomenclature_ref.map(str::to_string),
            None,
        )
    }

    #[test]
    fn duplicate_a007_with_same_nomenclature_uses_richer_record() {
        let auto = product(
            "WB-AUTO-1",
            "SANSTAR",
            "476.1-3.4.1.Р",
            None,
            None,
            Some("nom-1"),
        );
        let rich = product(
            "476.1-3.4.1.Р",
            "Шкаф-пенал напольно-подвесной Diva",
            "476.1-3.4.1.Р",
            Some("SANSTAR"),
            Some("7436"),
            Some("nom-1"),
        );

        let (selected, conflict) = select_wb_product_enrichment(vec![auto, rich]);
        assert!(!conflict);
        assert_eq!(
            selected.base.description,
            "Шкаф-пенал напольно-подвесной Diva"
        );
        assert_eq!(selected.brand.as_deref(), Some("SANSTAR"));
        assert_eq!(selected.nomenclature_ref.as_deref(), Some("nom-1"));
    }

    #[test]
    fn duplicate_a007_with_conflicting_nomenclature_drops_mapping_only() {
        let first = product(
            "SKU",
            "Товар",
            "SKU",
            Some("Brand"),
            Some("1"),
            Some("nom-1"),
        );
        let second = product("WB-AUTO-2", "Товар", "SKU", None, None, Some("nom-2"));

        let (selected, conflict) = select_wb_product_enrichment(vec![first, second]);
        assert!(conflict);
        assert_eq!(selected.base.description, "Товар");
        assert_eq!(selected.nomenclature_ref, None);
    }
}

#[cfg(test)]
mod wb_advert_batch_tests {
    use super::*;

    #[test]
    fn more_than_fifty_campaigns_are_split_without_deferral() {
        let ids: Vec<i64> = (1..=121).collect();
        let batches: Vec<Vec<i64>> = wb_advert_info_batches(&ids).map(<[i64]>::to_vec).collect();
        assert_eq!(
            batches.iter().map(Vec::len).collect::<Vec<_>>(),
            [50, 50, 21]
        );
        assert_eq!(batches.concat(), ids);
    }
}
