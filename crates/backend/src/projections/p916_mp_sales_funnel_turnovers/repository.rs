//! Репозиторий проекции `p916_mp_sales_funnel_turnovers` (универсальная воронка продаж).
//!
//! Наполняется push-ом из регистраторов при проведении/импорте: каждый регистратор
//! удаляет свои строки (delete-by-registrator) и вставляет заново. По образцу
//! `p915_mp_order_events::repository` / `p914_mp_finance_turnovers::repository`.
//! Агрегация метрик — SUM на чтении.

use anyhow::Result;
use sea_orm::entity::prelude::*;
use sea_orm::sea_query::OnConflict;
use sea_orm::{ConnectionTrait, Set, Statement};
use serde::{Deserialize, Serialize};

use crate::shared::data::db::get_connection;
use contracts::projections::p916_mp_sales_funnel_turnovers::dto::{
    FunnelDateAxis, FunnelPeriodSummary, MpFunnelAggRow, MpFunnelListRequest,
};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "p916_mp_sales_funnel_turnovers")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub stage: String,
    pub cohort_date: String,
    pub event_date: String,
    pub connection_mp_ref: String,
    #[sea_orm(nullable)]
    pub marketplace_product_ref: Option<String>,
    #[sea_orm(nullable)]
    pub nomenclature_ref: Option<String>,
    #[sea_orm(nullable)]
    pub nm_id: Option<i64>,
    pub registrator_type: String,
    pub registrator_ref: String,
    /// srid заказа (только fulfillment-строки) — мост к атрибуции рекламы p913 (канальный сплит).
    #[sea_orm(nullable)]
    pub order_key: Option<String>,

    // стадия 1 (маркетинговая воронка):
    /// Бесплатные/органические показы. Источник счётчика пока не подключён (a040 отдаёт только
    /// visibility %, а не показы, и в воронку не входит) → обычно NULL/N/A.
    #[sea_orm(nullable)]
    pub show_free_count: Option<i64>,
    /// Платные показы (реклама a026, views).
    #[sea_orm(nullable)]
    pub show_paid_count: Option<i64>,
    /// Платные переходы (реклама a026, clicks).
    #[sea_orm(nullable)]
    pub paid_open_count: Option<i64>,
    /// Платная корзина (реклама a026, atbs).
    #[sea_orm(nullable)]
    pub paid_cart_count: Option<i64>,
    /// Общие показы (impressions) как их отдаёт сам маркетплейс, без деления на
    /// платные/органические. Заполняет YM (a041, «Показы моих товаров»); у WB NULL —
    /// там общего счётчика показов нет, есть только платный `show_paid_count`.
    /// Не складывать с `show_free_count`/`show_paid_count`: это другой разрез.
    #[sea_orm(nullable)]
    pub total_impressions: Option<i64>,
    pub open_count: i64,
    pub cart_count: i64,
    pub wishlist_count: i64,
    pub funnel_order_count: i64,
    pub funnel_order_sum: f64,
    /// Отмены по дневному счётчику воронки маркетплейса (a036 у WB). Отличается от
    /// `cancel_count` (стадия fulfillment, документы a015): это маркетинговый счётчик
    /// самого маркетплейса. NULL — источник счётчик не отдал (N/A ≠ 0).
    #[sea_orm(nullable)]
    pub funnel_cancel_count: Option<i64>,
    #[sea_orm(nullable)]
    pub funnel_cancel_sum: Option<f64>,

    // стадия 2 (fulfillment/когорта):
    pub order_count: i64,
    pub order_sum: f64,
    pub cancel_count: i64,
    pub cancel_sum: f64,
    pub buyout_count: i64,
    pub buyout_sum: f64,
    pub return_count: i64,
    pub return_sum: f64,

    pub created_at_msk: String,
    pub updated_at_msk: String,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}

fn conn() -> &'static DatabaseConnection {
    get_connection()
}

fn active_from_model(entry: &Model) -> ActiveModel {
    ActiveModel {
        id: Set(entry.id.clone()),
        stage: Set(entry.stage.clone()),
        cohort_date: Set(entry.cohort_date.clone()),
        event_date: Set(entry.event_date.clone()),
        connection_mp_ref: Set(entry.connection_mp_ref.clone()),
        marketplace_product_ref: Set(entry.marketplace_product_ref.clone()),
        nomenclature_ref: Set(entry.nomenclature_ref.clone()),
        nm_id: Set(entry.nm_id),
        registrator_type: Set(entry.registrator_type.clone()),
        registrator_ref: Set(entry.registrator_ref.clone()),
        order_key: Set(entry.order_key.clone()),
        show_free_count: Set(entry.show_free_count),
        show_paid_count: Set(entry.show_paid_count),
        paid_open_count: Set(entry.paid_open_count),
        paid_cart_count: Set(entry.paid_cart_count),
        total_impressions: Set(entry.total_impressions),
        open_count: Set(entry.open_count),
        cart_count: Set(entry.cart_count),
        wishlist_count: Set(entry.wishlist_count),
        funnel_order_count: Set(entry.funnel_order_count),
        funnel_order_sum: Set(entry.funnel_order_sum),
        funnel_cancel_count: Set(entry.funnel_cancel_count),
        funnel_cancel_sum: Set(entry.funnel_cancel_sum),
        order_count: Set(entry.order_count),
        order_sum: Set(entry.order_sum),
        cancel_count: Set(entry.cancel_count),
        cancel_sum: Set(entry.cancel_sum),
        buyout_count: Set(entry.buyout_count),
        buyout_sum: Set(entry.buyout_sum),
        return_count: Set(entry.return_count),
        return_sum: Set(entry.return_sum),
        created_at_msk: Set(entry.created_at_msk.clone()),
        updated_at_msk: Set(entry.updated_at_msk.clone()),
    }
}

/// Конфликт по детерминированному `id` (натуральный ключ строки движения): при повторной
/// вставке той же строки перезаписываем метрики, а не задваиваем обороты. `created_at_msk`
/// в перезапись не входит (сохраняем момент первой записи). Защита сверх delete-by-period:
/// даже при неверном scope удаления повтор не породит дубль.
fn funnel_on_conflict() -> OnConflict {
    OnConflict::column(Column::Id)
        .update_columns([
            Column::OrderKey,
            Column::ShowFreeCount,
            Column::ShowPaidCount,
            Column::PaidOpenCount,
            Column::PaidCartCount,
            Column::TotalImpressions,
            Column::OpenCount,
            Column::CartCount,
            Column::WishlistCount,
            Column::FunnelOrderCount,
            Column::FunnelOrderSum,
            Column::FunnelCancelCount,
            Column::FunnelCancelSum,
            Column::OrderCount,
            Column::OrderSum,
            Column::CancelCount,
            Column::CancelSum,
            Column::BuyoutCount,
            Column::BuyoutSum,
            Column::ReturnCount,
            Column::ReturnSum,
            Column::MarketplaceProductRef,
            Column::NomenclatureRef,
            Column::UpdatedAtMsk,
        ])
        .to_owned()
}

/// Прямой upsert без SELECT-проверки. Используется в контексте проведения,
/// где строки регистратора предварительно удалены (delete-by-registrator).
pub async fn insert_entry_raw_with_conn<C: ConnectionTrait>(db: &C, entry: &Model) -> Result<()> {
    Entity::insert(active_from_model(entry))
        .on_conflict(funnel_on_conflict())
        .exec(db)
        .await?;
    Ok(())
}

/// Пакетный upsert строк движений (один INSERT ... VALUES ON CONFLICT на чанк). Используется в
/// бэкфиллах/пересборах, где на кабинет × период приходятся сотни строк nm_id × дата.
/// Чанкуем, чтобы не упереться в лимит переменных SQLite (999). Пустой срез — no-op.
pub async fn insert_many_with_conn<C: ConnectionTrait>(db: &C, entries: &[Model]) -> Result<()> {
    if entries.is_empty() {
        return Ok(());
    }
    // ~26 колонок на строку → 30 строк ≈ 780 плейсхолдеров, с запасом до лимита 999.
    for chunk in entries.chunks(30) {
        let models = chunk.iter().map(active_from_model);
        Entity::insert_many(models)
            .on_conflict(funnel_on_conflict())
            .exec(db)
            .await?;
    }
    Ok(())
}

/// Удаление строк по ссылке регистратора (autocommit). Используется при
/// распроведении, где нет внешней транзакции.
pub async fn delete_by_registrator_ref(registrator_ref: &str) -> Result<u64> {
    let result = Entity::delete_many()
        .filter(Column::RegistratorRef.eq(registrator_ref))
        .exec(conn())
        .await?;
    Ok(result.rows_affected)
}

/// Все строки движения конкретного регистратора (тип + ссылка). Используется закладкой
/// «Проекции» документа-источника — показывает ровно те движения воронки, что он породил.
pub async fn list_by_registrator(
    registrator_type: &str,
    registrator_ref: &str,
) -> Result<Vec<Model>> {
    let rows = Entity::find()
        .filter(Column::RegistratorType.eq(registrator_type))
        .filter(Column::RegistratorRef.eq(registrator_ref))
        .all(conn())
        .await?;
    Ok(rows)
}

/// Удаление строк конкретного регистратора (тип + ссылка) в рамках транзакции.
/// Используется при проведении/распроведении a015/a012.
pub async fn delete_by_registrator_with_conn<C: ConnectionTrait>(
    db: &C,
    registrator_type: &str,
    registrator_ref: &str,
) -> Result<u64> {
    let result = Entity::delete_many()
        .filter(Column::RegistratorType.eq(registrator_type))
        .filter(Column::RegistratorRef.eq(registrator_ref))
        .exec(db)
        .await?;
    Ok(result.rows_affected)
}

/// Удаление маркетинговых строк (стадия 1) за период по кабинету в рамках
/// транзакции. Используется в хуке импорта a036 (`replace_for_period`), который
/// заменяет весь период целиком.
pub async fn delete_marketing_for_period_with_conn<C: ConnectionTrait>(
    db: &C,
    registrator_type: &str,
    connection_mp_ref: &str,
    date_from: &str,
    date_to: &str,
) -> Result<u64> {
    let result = Entity::delete_many()
        .filter(Column::RegistratorType.eq(registrator_type))
        .filter(Column::ConnectionMpRef.eq(connection_mp_ref))
        .filter(Column::CohortDate.gte(date_from))
        .filter(Column::CohortDate.lte(date_to))
        .exec(db)
        .await?;
    Ok(result.rows_affected)
}

/// Полное удаление всех строк указанного регистратора-типа в рамках транзакции.
/// Используется разовым бэкфиллом стадии 1 (перестройка всех a036-движений).
pub async fn delete_all_by_registrator_type_with_conn<C: ConnectionTrait>(
    db: &C,
    registrator_type: &str,
) -> Result<u64> {
    let result = Entity::delete_many()
        .filter(Column::RegistratorType.eq(registrator_type))
        .exec(db)
        .await?;
    Ok(result.rows_affected)
}

/// Агрегированная воронка `товар × дата` (SUM движений). Ось выбирается запросом:
/// когортная (по дате заказа) или потоковая (по дате транзакции). Имена товаров
/// здесь не джойнятся — это делает потребитель на своём уровне.
pub async fn aggregate_by_product(request: &MpFunnelListRequest) -> Result<Vec<MpFunnelAggRow>> {
    let db = get_connection();

    let date_col = match request.axis {
        FunnelDateAxis::Cohort => "cohort_date",
        FunnelDateAxis::Event => "event_date",
    };

    let mut conditions: Vec<String> = Vec::new();
    if let Some(from) = request.date_from.as_ref().filter(|v| !v.is_empty()) {
        conditions.push(format!("{date_col} >= '{}'", from.replace('\'', "''")));
    }
    if let Some(to) = request.date_to.as_ref().filter(|v| !v.is_empty()) {
        conditions.push(format!("{date_col} <= '{}'", to.replace('\'', "''")));
    }
    if let Some(conn_ref) = request.connection_mp_ref.as_ref().filter(|v| !v.is_empty()) {
        conditions.push(format!(
            "connection_mp_ref = '{}'",
            conn_ref.replace('\'', "''")
        ));
    }
    if let Some(nm_id) = request.nm_id {
        conditions.push(format!("nm_id = {nm_id}"));
    }
    let where_clause = if conditions.is_empty() {
        "1 = 1".to_string()
    } else {
        conditions.join(" AND ")
    };

    let limit = request.limit.unwrap_or(5000).min(50000);
    let offset = request.offset.unwrap_or(0);

    // Канальный сплит: fulfillment-строка «платная», если её srid (order_key) есть в атрибуции
    // рекламы p913 (advert_clicks_order_accrual). DISTINCT — чтобы множественные кампании одного
    // заказа не размножали строки. Верх воронки (paid показы/переходы/корзина) — из собственных
    // платных колонок a026. `advert_present` — есть ли рекламные данные в срезе (иначе paid = N/A).
    let sql = format!(
        "SELECT
            {date_col} AS d,
            t.connection_mp_ref AS connection_mp_ref,
            t.marketplace_product_ref AS marketplace_product_ref,
            t.nomenclature_ref AS nomenclature_ref,
            t.nm_id AS nm_id,
            SUM(COALESCE(show_free_count, 0)) AS show_free_count,
            SUM(COALESCE(show_paid_count, 0)) AS show_paid_count,
            SUM(COALESCE(paid_open_count, 0)) AS paid_open_count,
            SUM(COALESCE(paid_cart_count, 0)) AS paid_cart_count,
            SUM(CASE WHEN show_free_count IS NOT NULL THEN 1 ELSE 0 END) AS show_free_present,
            SUM(CASE WHEN show_paid_count IS NOT NULL THEN 1 ELSE 0 END) AS show_paid_present,
            SUM(COALESCE(total_impressions, 0)) AS total_impressions,
            SUM(CASE WHEN total_impressions IS NOT NULL THEN 1 ELSE 0 END) AS total_impressions_present,
            MAX(CASE WHEN pj.order_key IS NOT NULL OR show_paid_count IS NOT NULL
                          OR paid_open_count IS NOT NULL OR paid_cart_count IS NOT NULL
                     THEN 1 ELSE 0 END) AS advert_present,
            SUM(open_count) AS open_count,
            SUM(cart_count) AS cart_count,
            SUM(wishlist_count) AS wishlist_count,
            SUM(funnel_order_count) AS funnel_order_count,
            SUM(funnel_order_sum) AS funnel_order_sum,
            SUM(COALESCE(funnel_cancel_count, 0)) AS funnel_cancel_count,
            SUM(COALESCE(funnel_cancel_sum, 0)) AS funnel_cancel_sum,
            SUM(CASE WHEN funnel_cancel_count IS NOT NULL THEN 1 ELSE 0 END) AS funnel_cancel_present,
            SUM(order_count) AS order_count,
            SUM(order_sum) AS order_sum,
            SUM(CASE WHEN pj.order_key IS NOT NULL THEN order_count ELSE 0 END) AS paid_order_count,
            SUM(CASE WHEN pj.order_key IS NOT NULL THEN order_sum ELSE 0 END) AS paid_order_sum,
            SUM(cancel_count) AS cancel_count,
            SUM(cancel_sum) AS cancel_sum,
            SUM(CASE WHEN pj.order_key IS NOT NULL THEN cancel_count ELSE 0 END) AS paid_cancel_count,
            SUM(CASE WHEN pj.order_key IS NOT NULL THEN cancel_sum ELSE 0 END) AS paid_cancel_sum,
            SUM(buyout_count) AS buyout_count,
            SUM(buyout_sum) AS buyout_sum,
            SUM(CASE WHEN pj.order_key IS NOT NULL THEN buyout_count ELSE 0 END) AS paid_buyout_count,
            SUM(CASE WHEN pj.order_key IS NOT NULL THEN buyout_sum ELSE 0 END) AS paid_buyout_sum,
            SUM(return_count) AS return_count,
            SUM(return_sum) AS return_sum,
            SUM(CASE WHEN pj.order_key IS NOT NULL THEN return_count ELSE 0 END) AS paid_return_count,
            SUM(CASE WHEN pj.order_key IS NOT NULL THEN return_sum ELSE 0 END) AS paid_return_sum
         FROM p916_mp_sales_funnel_turnovers t
         LEFT JOIN (SELECT DISTINCT order_key FROM p913_wb_advert_order_attr
                    WHERE turnover_code = 'advert_clicks_order_accrual' AND order_key <> '') pj
                ON pj.order_key = t.order_key
         WHERE {where_clause}
         GROUP BY {date_col}, t.connection_mp_ref, t.nm_id, t.marketplace_product_ref, t.nomenclature_ref
         ORDER BY {date_col} ASC, t.nm_id ASC
         LIMIT {limit} OFFSET {offset}"
    );

    let rows = db
        .query_all(Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            sql,
        ))
        .await?;

    let items = rows
        .into_iter()
        .map(|row| MpFunnelAggRow {
            date: row.try_get("", "d").unwrap_or_default(),
            connection_mp_ref: row.try_get("", "connection_mp_ref").unwrap_or_default(),
            marketplace_product_ref: row.try_get("", "marketplace_product_ref").ok(),
            nomenclature_ref: row.try_get("", "nomenclature_ref").ok(),
            nm_id: row.try_get("", "nm_id").ok(),
            show_free_count: row.try_get("", "show_free_count").unwrap_or(0),
            show_paid_count: row.try_get("", "show_paid_count").unwrap_or(0),
            paid_open_count: row.try_get("", "paid_open_count").unwrap_or(0),
            paid_cart_count: row.try_get("", "paid_cart_count").unwrap_or(0),
            show_free_available: row.try_get::<i64>("", "show_free_present").unwrap_or(0) > 0,
            show_paid_available: row.try_get::<i64>("", "show_paid_present").unwrap_or(0) > 0,
            total_impressions: row.try_get("", "total_impressions").unwrap_or(0),
            total_impressions_available: row
                .try_get::<i64>("", "total_impressions_present")
                .unwrap_or(0)
                > 0,
            advert_available: row.try_get::<i64>("", "advert_present").unwrap_or(0) > 0,
            open_count: row.try_get("", "open_count").unwrap_or(0),
            cart_count: row.try_get("", "cart_count").unwrap_or(0),
            wishlist_count: row.try_get("", "wishlist_count").unwrap_or(0),
            funnel_order_count: row.try_get("", "funnel_order_count").unwrap_or(0),
            funnel_order_sum: row.try_get("", "funnel_order_sum").unwrap_or(0.0),
            funnel_cancel_count: row.try_get("", "funnel_cancel_count").unwrap_or(0),
            funnel_cancel_sum: row.try_get("", "funnel_cancel_sum").unwrap_or(0.0),
            funnel_cancel_available: row.try_get::<i64>("", "funnel_cancel_present").unwrap_or(0)
                > 0,
            order_count: row.try_get("", "order_count").unwrap_or(0),
            order_sum: row.try_get("", "order_sum").unwrap_or(0.0),
            paid_order_count: row.try_get("", "paid_order_count").unwrap_or(0),
            paid_order_sum: row.try_get("", "paid_order_sum").unwrap_or(0.0),
            cancel_count: row.try_get("", "cancel_count").unwrap_or(0),
            cancel_sum: row.try_get("", "cancel_sum").unwrap_or(0.0),
            paid_cancel_count: row.try_get("", "paid_cancel_count").unwrap_or(0),
            paid_cancel_sum: row.try_get("", "paid_cancel_sum").unwrap_or(0.0),
            buyout_count: row.try_get("", "buyout_count").unwrap_or(0),
            buyout_sum: row.try_get("", "buyout_sum").unwrap_or(0.0),
            paid_buyout_count: row.try_get("", "paid_buyout_count").unwrap_or(0),
            paid_buyout_sum: row.try_get("", "paid_buyout_sum").unwrap_or(0.0),
            return_count: row.try_get("", "return_count").unwrap_or(0),
            return_sum: row.try_get("", "return_sum").unwrap_or(0.0),
            paid_return_count: row.try_get("", "paid_return_count").unwrap_or(0),
            paid_return_sum: row.try_get("", "paid_return_sum").unwrap_or(0.0),
        })
        .collect();

    Ok(items)
}

/// Диагностическая сводка воронки за период по когортной оси (SUM всех движений).
/// Пустой `connection_mp_refs` → все кабинеты. Используется для отчёта после пересбора.
pub async fn funnel_period_summary(
    date_from: &str,
    date_to: &str,
    connection_mp_refs: &[String],
) -> Result<FunnelPeriodSummary> {
    let db = get_connection();

    let mut conditions = vec![
        format!("cohort_date >= '{}'", date_from.replace('\'', "''")),
        format!("cohort_date <= '{}'", date_to.replace('\'', "''")),
    ];
    let filtered: Vec<String> = connection_mp_refs
        .iter()
        .filter(|v| !v.trim().is_empty())
        .map(|v| format!("'{}'", v.replace('\'', "''")))
        .collect();
    if !filtered.is_empty() {
        conditions.push(format!("connection_mp_ref IN ({})", filtered.join(", ")));
    }
    let where_clause = conditions.join(" AND ");

    let sql = format!(
        "SELECT
            SUM(COALESCE(show_free_count, 0)) AS show_free_count,
            SUM(COALESCE(show_paid_count, 0)) AS show_paid_count,
            SUM(CASE WHEN show_free_count IS NOT NULL THEN 1 ELSE 0 END) AS show_free_present,
            SUM(CASE WHEN show_paid_count IS NOT NULL THEN 1 ELSE 0 END) AS show_paid_present,
            SUM(COALESCE(total_impressions, 0)) AS total_impressions,
            SUM(CASE WHEN total_impressions IS NOT NULL THEN 1 ELSE 0 END) AS total_impressions_present,
            SUM(open_count) AS open_count,
            SUM(cart_count) AS cart_count,
            SUM(wishlist_count) AS wishlist_count,
            SUM(funnel_order_count) AS funnel_order_count,
            SUM(funnel_order_sum) AS funnel_order_sum,
            SUM(COALESCE(funnel_cancel_count, 0)) AS funnel_cancel_count,
            SUM(COALESCE(funnel_cancel_sum, 0)) AS funnel_cancel_sum,
            SUM(CASE WHEN funnel_cancel_count IS NOT NULL THEN 1 ELSE 0 END) AS funnel_cancel_present,
            SUM(order_count) AS order_count,
            SUM(order_sum) AS order_sum,
            SUM(cancel_count) AS cancel_count,
            SUM(cancel_sum) AS cancel_sum,
            SUM(buyout_count) AS buyout_count,
            SUM(buyout_sum) AS buyout_sum,
            SUM(return_count) AS return_count,
            SUM(return_sum) AS return_sum,
            SUM(CASE WHEN stage = 'marketing' THEN 1 ELSE 0 END) AS marketing_rows,
            SUM(CASE WHEN stage = 'fulfillment' THEN 1 ELSE 0 END) AS fulfillment_rows
         FROM p916_mp_sales_funnel_turnovers
         WHERE {where_clause}"
    );

    let row = db
        .query_one(Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            sql,
        ))
        .await?;

    let mut summary = FunnelPeriodSummary {
        date_from: date_from.to_string(),
        date_to: date_to.to_string(),
        ..Default::default()
    };
    if let Some(row) = row {
        summary.show_free_count = row.try_get("", "show_free_count").unwrap_or(0);
        summary.show_paid_count = row.try_get("", "show_paid_count").unwrap_or(0);
        summary.show_free_available = row.try_get::<i64>("", "show_free_present").unwrap_or(0) > 0;
        summary.show_paid_available = row.try_get::<i64>("", "show_paid_present").unwrap_or(0) > 0;
        summary.total_impressions = row.try_get("", "total_impressions").unwrap_or(0);
        summary.total_impressions_available = row
            .try_get::<i64>("", "total_impressions_present")
            .unwrap_or(0)
            > 0;
        summary.open_count = row.try_get("", "open_count").unwrap_or(0);
        summary.cart_count = row.try_get("", "cart_count").unwrap_or(0);
        summary.wishlist_count = row.try_get("", "wishlist_count").unwrap_or(0);
        summary.funnel_order_count = row.try_get("", "funnel_order_count").unwrap_or(0);
        summary.funnel_order_sum = row.try_get("", "funnel_order_sum").unwrap_or(0.0);
        summary.funnel_cancel_count = row.try_get("", "funnel_cancel_count").unwrap_or(0);
        summary.funnel_cancel_sum = row.try_get("", "funnel_cancel_sum").unwrap_or(0.0);
        summary.funnel_cancel_available =
            row.try_get::<i64>("", "funnel_cancel_present").unwrap_or(0) > 0;
        summary.order_count = row.try_get("", "order_count").unwrap_or(0);
        summary.order_sum = row.try_get("", "order_sum").unwrap_or(0.0);
        summary.cancel_count = row.try_get("", "cancel_count").unwrap_or(0);
        summary.cancel_sum = row.try_get("", "cancel_sum").unwrap_or(0.0);
        summary.buyout_count = row.try_get("", "buyout_count").unwrap_or(0);
        summary.buyout_sum = row.try_get("", "buyout_sum").unwrap_or(0.0);
        summary.return_count = row.try_get("", "return_count").unwrap_or(0);
        summary.return_sum = row.try_get("", "return_sum").unwrap_or(0.0);
        summary.marketing_rows = row.try_get("", "marketing_rows").unwrap_or(0);
        summary.fulfillment_rows = row.try_get("", "fulfillment_rows").unwrap_or(0);
    }
    Ok(summary)
}
