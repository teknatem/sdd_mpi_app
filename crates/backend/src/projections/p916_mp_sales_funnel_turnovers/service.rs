//! Пересбор воронки p916 за период.
//!
//! **Почему здесь, а не в `u508`.** Процедура жила в `u508::executor` и была
//! единственной причиной, по которой движок перепроведения знал имена шести
//! маркетплейсных агрегатов (a015, a012, a026, a013, a016, a036). Сам порядок
//! шагов — свойство воронки, а не движка: он выведен из того, как считаются её
//! движения, и меняется вместе с ней. Поэтому оркестрация переехала к
//! проекции, которую пересобирает, а `u508` остался движком — сессия,
//! прогресс, конкурентность, повторы при блокировке SQLite.
//!
//! **Что не переехало.** Отбор документов, перепроведение и учёт прогресса
//! по-прежнему делает `u508`: [`repost_ids_concurrent`] вызывается отсюда.
//!
//! **Порядок шагов обязателен.** a013 идёт строго перед a016: когорта возврата
//! резолвится по дате заказа из a013. a015 перед a012: выкуп относится к
//! когорте своего заказа.

use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::Arc;

use anyhow::{anyhow, Result};
use chrono::NaiveDate;
use contracts::projections::p916_mp_sales_funnel_turnovers::dto::FunnelRebuildRequest;
use contracts::usecases::u508_repost_documents::{
    progress::RepostStatus,
    response::{RepostResponse, RepostStartStatus},
};
use uuid::Uuid;

use crate::usecases::u508_repost_documents::{repost_ids_concurrent, ProgressTracker};

const A013_YM_ORDER: &str = "a013_ym_order";
const A015_WB_ORDERS: &str = "a015_wb_orders";
const A016_YM_RETURNS: &str = "a016_ym_returns";
const A026_WB_ADVERT_DAILY: &str = "a026_wb_advert_daily";

/// Запустить пересбор фоновой сессией. Прогресс читается общим
/// `GET /api/u508/repost/progress/:session_id`, поэтому трекер передаётся тот
/// же, что у перепроведения, — иначе сессия воронки была бы невидимой.
pub async fn start_rebuild(
    tracker: Arc<ProgressTracker>,
    request: FunnelRebuildRequest,
) -> Result<RepostResponse> {
    let database_activity = crate::system::maintenance::try_begin_database_activity()
        .ok_or_else(|| anyhow!("Пересбор недоступен во время обслуживания базы данных"))?;
    validate_period(&request)?;

    let session_id = Uuid::new_v4().to_string();
    tracker.create_session(session_id.clone());

    let sid = session_id.clone();
    let tracker_task = Arc::clone(&tracker);

    tokio::spawn(async move {
        let _database_activity = database_activity;
        if let Err(error) = run_rebuild(&tracker_task, &sid, &request).await {
            tracing::error!("Funnel rebuild failed: {}", error);
            tracker_task.add_error(&sid, format!("Funnel rebuild failed: {}", error));
            tracker_task.complete_session(&sid, RepostStatus::Failed);
        }
    });

    Ok(RepostResponse {
        session_id,
        status: RepostStartStatus::Started,
        message: "Funnel rebuild started".to_string(),
    })
}

fn validate_period(request: &FunnelRebuildRequest) -> Result<()> {
    let date_from = NaiveDate::parse_from_str(&request.date_from, "%Y-%m-%d")
        .map_err(|_| anyhow!("Invalid date_from: {}", request.date_from))?;
    let date_to = NaiveDate::parse_from_str(&request.date_to, "%Y-%m-%d")
        .map_err(|_| anyhow!("Invalid date_to: {}", request.date_to))?;
    if date_from > date_to {
        return Err(anyhow!("date_from must be less than or equal to date_to"));
    }
    Ok(())
}

/// Шесть шагов пересбора: WB (a015 → a012 → a026 → a036) и YM (a013 → a016).
///
/// Когортный отбор: a012 — по srid'ам заказов a015 периода, a016 — по номерам
/// заказов a013 периода, a026/a036 — по `document_date`. Ошибки шага не
/// прерывают прогон: они копятся в сессии, потому что частично пересобранная
/// воронка полезнее, чем оборванная на первом плохом документе.
///
/// Зовётся и напрямую — из ремонта воронки (`super::repair`), которому
/// нужен синхронный прогон в уже созданной сессии.
pub async fn run_rebuild(
    tracker: &Arc<ProgressTracker>,
    session_id: &str,
    request: &FunnelRebuildRequest,
) -> Result<()> {
    // Перепроводим все документы периода (не только проведённые), чтобы
    // гарантированно пересобрать движения воронки.
    let only_posted = false;

    let a015_ids = crate::domain::a015_wb_orders::repository::list_ids_by_date_range_scoped(
        &request.date_from,
        &request.date_to,
        &request.connection_mp_refs,
        only_posted,
    )
    .await?;

    // a012 отбираем «по периоду заказов»: srid'ы заказов a015 за период → продажи
    // a012 по этим srid'ам с нижней границей sale_date >= date_from (без верхней),
    // чтобы захватить выкупы когорты, проданные после конца периода.
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

    // YM: заказы когорты (заказ/отмена/выкуп) и возвраты этих заказов. Отбор
    // возвратов идёт по заказам периода, а не по своей дате — движение возврата
    // должно лечь в когорту заказа.
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
    let total =
        (a015_ids.len() + a012_ids.len() + a026_ids.len() + a013_ids.len() + a016_ids.len() + 1)
            as i32;
    tracker.set_total(session_id, total);
    tracker.set_chunks_total(session_id, 6);

    // Сквозные счётчики прогресса по всем шагам (конкурентные шаги пишут в
    // атомики; последовательные — в те же счётчики).
    let processed = Arc::new(AtomicI32::new(0));
    let reposted = Arc::new(AtomicI32::new(0));

    // === Шаг 1/6: a015 — заказы/отмены WB (стадия 2) ===
    step(tracker, session_id, 0, "Шаг 1/6: заказы a015 (стадия 2)");
    repost_ids_concurrent(
        tracker,
        session_id,
        A015_WB_ORDERS,
        a015_ids,
        &processed,
        &reposted,
    )
    .await?;

    // === Шаг 2/6: a012 — выкупы/возвраты (стадия 2) ===
    step(tracker, session_id, 1, "Шаг 2/6: продажи a012 (стадия 2)");
    crate::domain::a012_wb_sales::service::repost_ids_with_daily_cache(
        tracker, session_id, a012_ids, &processed, &reposted,
    )
    .await?;

    // === Шаг 3/6: a026 — реклама/платные показы (стадия 1) ===
    step(tracker, session_id, 2, "Шаг 3/6: реклама a026 (стадия 1)");
    repost_ids_concurrent(
        tracker,
        session_id,
        A026_WB_ADVERT_DAILY,
        a026_ids,
        &processed,
        &reposted,
    )
    .await?;

    // === Шаг 4/6: a013 — заказы/отмены/выкупы YM (стадия 2) ===
    step(tracker, session_id, 3, "Шаг 4/6: заказы a013 YM (стадия 2)");
    repost_ids_concurrent(
        tracker,
        session_id,
        A013_YM_ORDER,
        a013_ids,
        &processed,
        &reposted,
    )
    .await?;

    // === Шаг 5/6: a016 — возвраты YM (стадия 2) ===
    // Строго после a013: когорта возврата резолвится по дате заказа из a013.
    step(
        tracker,
        session_id,
        4,
        "Шаг 5/6: возвраты a016 YM (стадия 2)",
    );
    repost_ids_concurrent(
        tracker,
        session_id,
        A016_YM_RETURNS,
        a016_ids,
        &processed,
        &reposted,
    )
    .await?;

    // === Шаг 6/6: a036 — стадия 1 (маркетинг) из сохранённых документов ===
    step(tracker, session_id, 5, "Шаг 6/6: воронка a036 (стадия 1)");
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
        Err(error) => tracker.add_error(session_id, format!("a036 стадия 1: {}", error)),
    }
    let processed_final = processed.fetch_add(1, Ordering::Relaxed) + 1;
    let reposted_final = reposted.load(Ordering::Relaxed);
    tracker.update_progress(session_id, processed_final, reposted_final, None);
    step(tracker, session_id, 6, "Готово");

    let final_status = if tracker
        .get_progress(session_id)
        .map(|progress| progress.errors > 0)
        .unwrap_or(false)
    {
        RepostStatus::CompletedWithErrors
    } else {
        RepostStatus::Completed
    };
    tracker.complete_session(session_id, final_status);

    Ok(())
}

fn step(tracker: &Arc<ProgressTracker>, session_id: &str, index: i32, label: &str) {
    tracker.update_chunk_progress(session_id, index, None, None, Some(label.to_string()));
}
