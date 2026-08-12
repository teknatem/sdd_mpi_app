use anyhow::Result;
use chrono::Utc;
use sea_orm::entity::prelude::*;
use sea_orm::sea_query::OnConflict;
use sea_orm::{
    ColumnTrait, EntityTrait, QueryFilter, QueryOrder, QuerySelect, Set, TransactionTrait,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

use crate::shared::data::db::get_connection;

/// SeaORM entity for p907_ym_payment_report
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "p907_ym_payment_report")]
pub struct Model {
    /// Business deduplication key (ymid_ format, previously SYNTH_...).
    /// Used only during import for idempotent upsert.
    #[sea_orm(primary_key, auto_increment = false)]
    pub record_key: String,

    /// Internal stable UUID for UI navigation and internal links.
    /// Assigned once on first insert; never changes on upsert.
    pub id: String,

    // Metadata
    pub connection_mp_ref: String,
    pub organization_ref: String,

    // Business info
    #[sea_orm(nullable)]
    pub business_id: Option<i64>,
    #[sea_orm(nullable)]
    pub partner_id: Option<i64>,
    #[sea_orm(nullable)]
    pub shop_name: Option<String>,
    #[sea_orm(nullable)]
    pub inn: Option<String>,
    #[sea_orm(nullable)]
    pub model: Option<String>,

    // Transaction info
    #[sea_orm(nullable)]
    pub transaction_id: Option<String>, // real YM transaction ID (nullable)
    #[sea_orm(nullable)]
    pub transaction_date: Option<String>,
    #[sea_orm(nullable)]
    pub transaction_type: Option<String>,
    #[sea_orm(nullable)]
    pub transaction_source: Option<String>,
    #[sea_orm(nullable)]
    pub transaction_sum: Option<f64>,
    #[sea_orm(nullable)]
    pub payment_status: Option<String>,

    // Order info
    #[sea_orm(nullable)]
    pub order_id: Option<i64>,
    #[sea_orm(nullable)]
    pub shop_order_id: Option<String>,
    #[sea_orm(nullable)]
    pub order_creation_date: Option<String>,
    #[sea_orm(nullable)]
    pub order_delivery_date: Option<String>,
    #[sea_orm(nullable)]
    pub order_type: Option<String>,

    // Product/service info
    #[sea_orm(nullable)]
    pub shop_sku: Option<String>,
    #[sea_orm(nullable)]
    pub offer_or_service_name: Option<String>,
    #[sea_orm(nullable)]
    pub count: Option<i32>,

    // Bank / Act info
    #[sea_orm(nullable)]
    pub act_id: Option<i64>,
    #[sea_orm(nullable)]
    pub act_date: Option<String>,
    #[sea_orm(nullable)]
    pub bank_order_id: Option<i64>,
    #[sea_orm(nullable)]
    pub bank_order_date: Option<String>,
    #[sea_orm(nullable)]
    pub bank_sum: Option<f64>,

    // Extra
    #[sea_orm(nullable)]
    pub claim_number: Option<String>,
    #[sea_orm(nullable)]
    pub bonus_account_year_month: Option<String>,
    #[sea_orm(nullable)]
    pub comments: Option<String>,

    // Производные ссылки: резолвятся на первом этапе проведения (если пусто)
    // и копируются в p914.
    /// uuid a007_marketplace_product (по shop_sku).
    #[sea_orm(nullable)]
    pub marketplace_product_ref: Option<String>,
    /// uuid a013_ym_order (по order_id).
    #[sea_orm(nullable)]
    pub marketplace_order_ref: Option<String>,
    /// uuid a004_nomenclature (зеркало a007.nomenclature_ref по marketplace_product_ref).
    #[sea_orm(nullable)]
    pub nomenclature_ref: Option<String>,

    // Technical
    pub loaded_at_utc: String,
    pub payload_version: i32,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}

/// Entry struct for upsert operations
#[derive(Debug, Clone)]
pub struct YmPaymentReportEntry {
    /// Business deduplication key (ymid_ format).
    pub record_key: String,
    /// Internal stable UUID for navigation.
    /// Generated once on first insert; preserved on conflict.
    pub id: String,
    pub connection_mp_ref: String,
    pub organization_ref: String,
    pub business_id: Option<i64>,
    pub partner_id: Option<i64>,
    pub shop_name: Option<String>,
    pub inn: Option<String>,
    pub model: Option<String>,
    /// Real YM transaction ID from CSV — None when YM leaves it empty
    pub transaction_id: Option<String>,
    pub transaction_date: Option<String>,
    pub transaction_type: Option<String>,
    pub transaction_source: Option<String>,
    pub transaction_sum: Option<f64>,
    pub payment_status: Option<String>,
    pub order_id: Option<i64>,
    pub shop_order_id: Option<String>,
    pub order_creation_date: Option<String>,
    pub order_delivery_date: Option<String>,
    pub order_type: Option<String>,
    pub shop_sku: Option<String>,
    pub offer_or_service_name: Option<String>,
    pub count: Option<i32>,
    pub act_id: Option<i64>,
    pub act_date: Option<String>,
    pub bank_order_id: Option<i64>,
    pub bank_order_date: Option<String>,
    pub bank_sum: Option<f64>,
    pub claim_number: Option<String>,
    pub bonus_account_year_month: Option<String>,
    pub comments: Option<String>,
    pub payload_version: i32,
}

/// Build the import `ActiveModel` from an entry. Derived refs
/// (`marketplace_product_ref` / `marketplace_order_ref` / `nomenclature_ref`)
/// never come from import — they are resolved at posting time and excluded from
/// the conflict UPDATE so previously filled values survive a re-import.
fn build_active_model(entry: &YmPaymentReportEntry, loaded_at_utc: String) -> ActiveModel {
    ActiveModel {
        record_key: Set(entry.record_key.clone()),
        id: Set(entry.id.clone()),
        connection_mp_ref: Set(entry.connection_mp_ref.clone()),
        organization_ref: Set(entry.organization_ref.clone()),
        business_id: Set(entry.business_id),
        partner_id: Set(entry.partner_id),
        shop_name: Set(entry.shop_name.clone()),
        inn: Set(entry.inn.clone()),
        model: Set(entry.model.clone()),
        transaction_id: Set(entry.transaction_id.clone()),
        transaction_date: Set(entry.transaction_date.clone()),
        transaction_type: Set(entry.transaction_type.clone()),
        transaction_source: Set(entry.transaction_source.clone()),
        transaction_sum: Set(entry.transaction_sum),
        payment_status: Set(entry.payment_status.clone()),
        order_id: Set(entry.order_id),
        shop_order_id: Set(entry.shop_order_id.clone()),
        order_creation_date: Set(entry.order_creation_date.clone()),
        order_delivery_date: Set(entry.order_delivery_date.clone()),
        order_type: Set(entry.order_type.clone()),
        shop_sku: Set(entry.shop_sku.clone()),
        offer_or_service_name: Set(entry.offer_or_service_name.clone()),
        count: Set(entry.count),
        act_id: Set(entry.act_id),
        act_date: Set(entry.act_date.clone()),
        bank_order_id: Set(entry.bank_order_id),
        bank_order_date: Set(entry.bank_order_date.clone()),
        bank_sum: Set(entry.bank_sum),
        claim_number: Set(entry.claim_number.clone()),
        bonus_account_year_month: Set(entry.bonus_account_year_month.clone()),
        comments: Set(entry.comments.clone()),
        marketplace_product_ref: Set(None),
        marketplace_order_ref: Set(None),
        nomenclature_ref: Set(None),
        loaded_at_utc: Set(loaded_at_utc),
        payload_version: Set(entry.payload_version),
    }
}

/// `ON CONFLICT(record_key) DO UPDATE SET …` clause shared by single and bulk
/// upsert. `id` is intentionally NOT in the list — it stays fixed once assigned,
/// so the internal UUID survives re-imports.
fn record_key_conflict() -> OnConflict {
    OnConflict::column(Column::RecordKey)
        .update_columns([
            Column::ConnectionMpRef,
            Column::OrganizationRef,
            Column::BusinessId,
            Column::PartnerId,
            Column::ShopName,
            Column::Inn,
            Column::Model,
            Column::TransactionId,
            Column::TransactionDate,
            Column::TransactionType,
            Column::TransactionSource,
            Column::TransactionSum,
            Column::PaymentStatus,
            Column::OrderId,
            Column::ShopOrderId,
            Column::OrderCreationDate,
            Column::OrderDeliveryDate,
            Column::OrderType,
            Column::ShopSku,
            Column::OfferOrServiceName,
            Column::Count,
            Column::ActId,
            Column::ActDate,
            Column::BankOrderId,
            Column::BankOrderDate,
            Column::BankSum,
            Column::ClaimNumber,
            Column::BonusAccountYearMonth,
            Column::Comments,
            Column::LoadedAtUtc,
            Column::PayloadVersion,
        ])
        .to_owned()
}

/// Upsert a single entry using INSERT ... ON CONFLICT(record_key) DO UPDATE SET ...
pub async fn upsert_entry(entry: &YmPaymentReportEntry) -> Result<()> {
    let db = get_connection();
    let model = build_active_model(entry, Utc::now().to_rfc3339());

    Entity::insert(model)
        .on_conflict(record_key_conflict())
        .exec(db)
        .await?;

    Ok(())
}

/// Bulk upsert: one multi-row `INSERT … ON CONFLICT(record_key) DO UPDATE` per
/// call, replacing N autocommit round-trips with a single statement — the key
/// optimisation for importing large payment-report CSVs.
///
/// The caller MUST ensure `entries` contains no duplicate `record_key` within
/// the slice: SQLite rejects a statement whose `ON CONFLICT DO UPDATE` would
/// touch the same row twice ("cannot affect row a second time"). De-duplication
/// happens during CSV parsing (see `parse_payment_report_csv`).
pub async fn bulk_upsert_entries(entries: &[YmPaymentReportEntry]) -> Result<u64> {
    if entries.is_empty() {
        return Ok(0);
    }

    let db = get_connection();
    let loaded_at_utc = Utc::now().to_rfc3339();

    let models = entries
        .iter()
        .map(|entry| build_active_model(entry, loaded_at_utc.clone()))
        .collect::<Vec<_>>();

    Entity::insert_many(models)
        .on_conflict(record_key_conflict())
        .exec_without_returning(db)
        .await?;

    Ok(entries.len() as u64)
}

/// List entries with filters and pagination
pub async fn list_with_filters(
    date_from: &str,
    date_to: &str,
    transaction_type: Option<String>,
    payment_status: Option<String>,
    transaction_source: Option<String>,
    shop_sku: Option<String>,
    order_id: Option<i64>,
    bank_order_id: Option<i64>,
    connection_mp_ref: Option<String>,
    organization_ref: Option<String>,
    sort_by: &str,
    sort_desc: bool,
    limit: i32,
    offset: i32,
) -> Result<(Vec<Model>, i32)> {
    let db = get_connection();
    const MAX_LIMIT: i32 = 1000;
    const MAX_TOTAL_COUNT: i32 = 10_000;
    const COUNT_SCAN_LIMIT: u64 = (MAX_TOTAL_COUNT as u64) + 1;

    let safe_limit = limit.max(1).min(MAX_LIMIT);
    let safe_offset = offset.max(0);
    let date_from_bound = normalize_date_filter(date_from);
    let date_to_bound = inclusive_date_to_bound(date_to);

    let apply_filters = |mut q: sea_orm::Select<Entity>| -> sea_orm::Select<Entity> {
        if let Some(ref bound) = date_from_bound {
            q = q.filter(Column::TransactionDate.gte(bound));
        }
        if let Some(ref bound) = date_to_bound {
            q = q.filter(Column::TransactionDate.lte(bound));
        }
        if let Some(ref tt) = transaction_type {
            if !tt.is_empty() {
                q = q.filter(Column::TransactionType.eq(tt));
            }
        }
        if let Some(ref ps) = payment_status {
            if !ps.is_empty() {
                q = q.filter(Column::PaymentStatus.eq(ps));
            }
        }
        if let Some(ref source) = transaction_source {
            if !source.is_empty() {
                q = q.filter(Column::TransactionSource.eq(source));
            }
        }
        if let Some(ref sku) = shop_sku {
            if !sku.is_empty() {
                q = q.filter(Column::ShopSku.contains(sku));
            }
        }
        if let Some(oid) = order_id {
            q = q.filter(Column::OrderId.eq(oid));
        }
        if let Some(boid) = bank_order_id {
            q = q.filter(Column::BankOrderId.eq(boid));
        }
        if let Some(ref conn) = connection_mp_ref {
            q = q.filter(Column::ConnectionMpRef.eq(conn));
        }
        if let Some(ref org) = organization_ref {
            q = q.filter(Column::OrganizationRef.eq(org));
        }
        q
    };

    let total_count = apply_filters(Entity::find())
        .select_only()
        .column(Column::RecordKey)
        .limit(COUNT_SCAN_LIMIT)
        .into_tuple::<String>()
        .all(db)
        .await?
        .len() as i32;
    let total_count = total_count.min(MAX_TOTAL_COUNT);

    let mut query = apply_filters(Entity::find());

    query = match sort_by {
        "transaction_date" => {
            if sort_desc {
                query.order_by_desc(Column::TransactionDate)
            } else {
                query.order_by_asc(Column::TransactionDate)
            }
        }
        "transaction_sum" => {
            if sort_desc {
                query.order_by_desc(Column::TransactionSum)
            } else {
                query.order_by_asc(Column::TransactionSum)
            }
        }
        "order_id" => {
            if sort_desc {
                query.order_by_desc(Column::OrderId)
            } else {
                query.order_by_asc(Column::OrderId)
            }
        }
        "transaction_type" => {
            if sort_desc {
                query.order_by_desc(Column::TransactionType)
            } else {
                query.order_by_asc(Column::TransactionType)
            }
        }
        "bank_sum" => {
            if sort_desc {
                query.order_by_desc(Column::BankSum)
            } else {
                query.order_by_asc(Column::BankSum)
            }
        }
        "bank_order_id" => {
            if sort_desc {
                query.order_by_desc(Column::BankOrderId)
            } else {
                query.order_by_asc(Column::BankOrderId)
            }
        }
        _ => {
            if sort_desc {
                query.order_by_desc(Column::TransactionDate)
            } else {
                query.order_by_asc(Column::TransactionDate)
            }
        }
    };

    let items = query
        .limit(safe_limit as u64)
        .offset(safe_offset as u64)
        .all(db)
        .await?;

    Ok((items, total_count))
}

/// Выгрузка строк отчёта по платежам YM за период для внешнего BI-API
/// (`/api/ext/v1/ym-payment-report`) — полный аналог `p903::list_raw_for_ext`.
///
/// Фильтр по `transaction_date` (верхняя граница инклюзивна вместе со временем),
/// опционально по кабинету. Порядок стабилен (`transaction_date`, затем `record_key`),
/// чтобы постраничная выгрузка через `limit`/`offset` не дублировала и не теряла строки.
/// `total` считается ограниченным сканом (не более `MAX_TOTAL_COUNT`).
pub async fn list_raw_for_ext(
    date_from: &str,
    date_to: &str,
    connection_mp_ref: Option<String>,
    limit: i32,
    offset: i32,
) -> Result<(Vec<Model>, i32)> {
    let db = get_connection();
    const MAX_LIMIT: i32 = 20_000;
    const MAX_TOTAL_COUNT: i32 = 200_000;
    const COUNT_SCAN_LIMIT: u64 = (MAX_TOTAL_COUNT as u64) + 1;

    let safe_limit = limit.max(1).min(MAX_LIMIT);
    let safe_offset = offset.max(0);
    let date_from_bound = normalize_date_filter(date_from);
    let date_to_bound = inclusive_date_to_bound(date_to);

    let apply_filters = |mut q: sea_orm::Select<Entity>| -> sea_orm::Select<Entity> {
        if let Some(ref bound) = date_from_bound {
            q = q.filter(Column::TransactionDate.gte(bound));
        }
        if let Some(ref bound) = date_to_bound {
            q = q.filter(Column::TransactionDate.lte(bound));
        }
        if let Some(conn) = connection_mp_ref.as_deref().filter(|v| !v.is_empty()) {
            q = q.filter(Column::ConnectionMpRef.eq(conn));
        }
        q
    };

    let total_count = apply_filters(Entity::find())
        .select_only()
        .column(Column::RecordKey)
        .limit(COUNT_SCAN_LIMIT)
        .into_tuple::<String>()
        .all(db)
        .await?
        .len() as i32;
    let total_count = total_count.min(MAX_TOTAL_COUNT);

    let items = apply_filters(Entity::find())
        .order_by_asc(Column::TransactionDate)
        .order_by_asc(Column::RecordKey)
        .limit(safe_limit as u64)
        .offset(safe_offset as u64)
        .all(db)
        .await?;

    Ok((items, total_count))
}

pub async fn list_filter_options(
    date_from: &str,
    date_to: &str,
    connection_mp_ref: Option<String>,
    organization_ref: Option<String>,
) -> Result<(Vec<String>, Vec<String>, Vec<String>)> {
    let db = get_connection();
    let date_from_bound = normalize_date_filter(date_from);
    let date_to_bound = inclusive_date_to_bound(date_to);

    let apply_scope = |mut q: sea_orm::Select<Entity>| -> sea_orm::Select<Entity> {
        if let Some(ref bound) = date_from_bound {
            q = q.filter(Column::TransactionDate.gte(bound));
        }
        if let Some(ref bound) = date_to_bound {
            q = q.filter(Column::TransactionDate.lte(bound));
        }
        if let Some(ref conn) = connection_mp_ref {
            if !conn.is_empty() {
                q = q.filter(Column::ConnectionMpRef.eq(conn));
            }
        }
        if let Some(ref org) = organization_ref {
            if !org.is_empty() {
                q = q.filter(Column::OrganizationRef.eq(org));
            }
        }
        q
    };

    let transaction_types = collect_non_empty(
        apply_scope(
            Entity::find()
                .select_only()
                .column(Column::TransactionType)
                .order_by_asc(Column::TransactionType),
        )
        .into_tuple::<Option<String>>()
        .all(db)
        .await?,
    );

    let payment_statuses = collect_non_empty(
        apply_scope(
            Entity::find()
                .select_only()
                .column(Column::PaymentStatus)
                .order_by_asc(Column::PaymentStatus),
        )
        .into_tuple::<Option<String>>()
        .all(db)
        .await?,
    );

    let transaction_sources = collect_non_empty(
        apply_scope(
            Entity::find()
                .select_only()
                .column(Column::TransactionSource)
                .order_by_asc(Column::TransactionSource),
        )
        .into_tuple::<Option<String>>()
        .all(db)
        .await?,
    );

    Ok((transaction_types, payment_statuses, transaction_sources))
}

fn collect_non_empty(values: Vec<Option<String>>) -> Vec<String> {
    values
        .into_iter()
        .flatten()
        .filter_map(|value| {
            let trimmed = value.trim();
            (!trimmed.is_empty()).then(|| trimmed.to_string())
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn normalize_date_filter(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }

    parse_display_date(trimmed).or_else(|| Some(trimmed.to_string()))
}

fn inclusive_date_to_bound(date_to: &str) -> Option<String> {
    let trimmed = date_to.trim();
    if trimmed.is_empty() {
        return None;
    }

    let normalized = parse_display_date(trimmed).unwrap_or_else(|| trimmed.to_string());
    if normalized.len() == 10 && normalized.as_bytes().get(4) == Some(&b'-') {
        Some(format!("{normalized} 23:59:59"))
    } else {
        Some(normalized)
    }
}

fn parse_display_date(value: &str) -> Option<String> {
    let mut parts = value.split('.');
    let day = parts.next()?;
    let month = parts.next()?;
    let year = parts.next()?;
    if parts.next().is_some() || day.len() != 2 || month.len() != 2 || year.len() != 4 {
        return None;
    }
    Some(format!("{year}-{month}-{day}"))
}

/// Get a single entry by internal UUID (`id` column).
pub async fn get_by_uuid(id: &str) -> Result<Option<Model>> {
    let db = get_connection();
    let item = Entity::find().filter(Column::Id.eq(id)).one(db).await?;
    Ok(item)
}

/// Строки «Платёж/Возврат платежа покупателя» одного кабинета за день доставки
/// (`order_delivery_date` = day). Для мини-отчёта деталей a034 (fina-детализация
/// доставленных заказов и возвратов этого дня). `day` — "YYYY-MM-DD".
pub async fn list_buyer_movements_by_delivery_day(
    connection_mp_ref: &str,
    day: &str,
) -> Result<Vec<Model>> {
    let db = get_connection();
    let rows = Entity::find()
        .filter(Column::ConnectionMpRef.eq(connection_mp_ref))
        .filter(Column::OrderDeliveryDate.like(format!("{}%", day)))
        .filter(
            Column::TransactionSource.is_in(["Платёж покупателя", "Возврат платежа покупателя"]),
        )
        .order_by_asc(Column::TransactionSource)
        .order_by_asc(Column::OrderId)
        .order_by_asc(Column::TransactionDate)
        .all(db)
        .await?;
    Ok(rows)
}

/// Get a single entry by record_key (legacy/internal use).
pub async fn get_by_record_key(record_key: &str) -> Result<Option<Model>> {
    let db = get_connection();
    let item = Entity::find_by_id(record_key).one(db).await?;
    Ok(item)
}

/// Все внутренние `id` записей p907 — для массового перепроведения (repost-all).
pub async fn list_all_ids() -> Result<Vec<String>> {
    use sea_orm::QuerySelect;

    let db = get_connection();
    let ids = Entity::find()
        .select_only()
        .column(Column::Id)
        .into_tuple::<String>()
        .all(db)
        .await?;
    Ok(ids)
}

/// Внутренние `id` записей p907 в диапазоне дат транзакции — для перепроведения
/// за период (u508). Верхняя граница инклюзивна (date_to включается целиком,
/// даже если transaction_date содержит время).
pub async fn list_ids_by_transaction_date_range(
    date_from: &str,
    date_to: &str,
) -> Result<Vec<String>> {
    let db = get_connection();
    let date_to_bound = format!("{date_to} 23:59:59");
    let ids = Entity::find()
        .select_only()
        .column(Column::Id)
        .filter(Column::TransactionDate.gte(date_from))
        .filter(Column::TransactionDate.lte(date_to_bound))
        .order_by_asc(Column::TransactionDate)
        .into_tuple::<String>()
        .all(db)
        .await?;
    Ok(ids)
}

/// Migrate all SYNTH_... record_keys to ymid_... format.
///
/// For each SYNTH_ record, computes the new ymid_ key from the stored data fields
/// and re-inserts the row under the new key (preserving `id`), then deletes the
/// old SYNTH_ row.  Records that would produce a ymid_ key that already exists are
/// left untouched (the canonical ymid_ entry wins).
///
/// Returns `(migrated, already_ymid, errors)`.
pub async fn migrate_synth_keys(
    build_ymid: impl Fn(&Model) -> String,
) -> Result<(usize, usize, usize)> {
    let db = get_connection();

    let records: Vec<Model> = Entity::find()
        .filter(Column::RecordKey.starts_with("SYNTH_"))
        .all(db)
        .await?;

    let total = records.len();
    tracing::info!("migrate_synth_keys: found {} SYNTH_ records", total);

    let mut migrated = 0usize;
    let mut errors = 0usize;

    for record in records {
        let new_key = build_ymid(&record);

        // Root cause of the original bug: the SELECT-based INSERT copied the `id`
        // from the source row while it was still present in the table, causing a
        // silent conflict on idx_p907_id (UNIQUE on id). OR IGNORE skipped the
        // insert, then the SYNTH_ row was deleted — row gone entirely.
        //
        // Fix: wrap DELETE + INSERT in a SeaORM transaction so that:
        //  1. The SYNTH_ row is removed first (freeing the id for reuse).
        //  2. The ymid_ row is inserted with the preserved id.
        //  3. If the ymid_ key already exists (record_key conflict), INSERT OR IGNORE
        //     skips it and the SYNTH_ row is still removed — ymid_ entry wins.
        //  4. On any error the transaction rolls back and both rows are preserved.
        let old_key = record.record_key.clone();
        let new_model = ActiveModel {
            record_key: Set(new_key.clone()),
            id: Set(record.id.clone()),
            connection_mp_ref: Set(record.connection_mp_ref.clone()),
            organization_ref: Set(record.organization_ref.clone()),
            business_id: Set(record.business_id),
            partner_id: Set(record.partner_id),
            shop_name: Set(record.shop_name.clone()),
            inn: Set(record.inn.clone()),
            model: Set(record.model.clone()),
            transaction_id: Set(record.transaction_id.clone()),
            transaction_date: Set(record.transaction_date.clone()),
            transaction_type: Set(record.transaction_type.clone()),
            transaction_source: Set(record.transaction_source.clone()),
            transaction_sum: Set(record.transaction_sum),
            payment_status: Set(record.payment_status.clone()),
            order_id: Set(record.order_id),
            shop_order_id: Set(record.shop_order_id.clone()),
            order_creation_date: Set(record.order_creation_date.clone()),
            order_delivery_date: Set(record.order_delivery_date.clone()),
            order_type: Set(record.order_type.clone()),
            shop_sku: Set(record.shop_sku.clone()),
            offer_or_service_name: Set(record.offer_or_service_name.clone()),
            count: Set(record.count),
            act_id: Set(record.act_id),
            act_date: Set(record.act_date.clone()),
            bank_order_id: Set(record.bank_order_id),
            bank_order_date: Set(record.bank_order_date.clone()),
            bank_sum: Set(record.bank_sum),
            claim_number: Set(record.claim_number.clone()),
            bonus_account_year_month: Set(record.bonus_account_year_month.clone()),
            comments: Set(record.comments.clone()),
            marketplace_product_ref: Set(record.marketplace_product_ref.clone()),
            marketplace_order_ref: Set(record.marketplace_order_ref.clone()),
            nomenclature_ref: Set(record.nomenclature_ref.clone()),
            loaded_at_utc: Set(record.loaded_at_utc.clone()),
            payload_version: Set(record.payload_version),
        };

        let result = db
            .transaction::<_, (), sea_orm::DbErr>(|txn| {
                Box::pin(async move {
                    // 1. Remove the SYNTH_ row — this frees the `id` value.
                    Entity::delete_by_id(old_key).exec(txn).await?;

                    // 2. Insert under the new ymid_ key, preserving `id`.
                    //    ON CONFLICT(record_key) DO NOTHING: if a ymid_ row already
                    //    exists the SYNTH_ row is still gone (ymid_ entry wins).
                    Entity::insert(new_model)
                        .on_conflict(
                            OnConflict::column(Column::RecordKey)
                                .do_nothing()
                                .to_owned(),
                        )
                        .exec_without_returning(txn)
                        .await?;

                    Ok(())
                })
            })
            .await;

        match result {
            Ok(()) => {
                migrated += 1;
            }
            Err(e) => {
                tracing::error!(
                    "migrate_synth_keys: transaction failed for {} → {}: {}",
                    record.record_key,
                    new_key,
                    e
                );
                errors += 1;
            }
        }
    }

    tracing::info!(
        "migrate_synth_keys: done. migrated={}, errors={}",
        migrated,
        errors
    );

    Ok((migrated, 0, errors))
}

/// Delete all entries for a connection in a date range (for re-import)
pub async fn delete_by_connection_and_date_range(
    connection_mp_ref: &str,
    date_from: &str,
    date_to: &str,
) -> Result<u64> {
    let db = get_connection();
    let ids = Entity::find()
        .select_only()
        .column(Column::Id)
        .filter(Column::ConnectionMpRef.eq(connection_mp_ref))
        .filter(Column::TransactionDate.gte(date_from))
        .filter(Column::TransactionDate.lte(date_to))
        .into_tuple::<String>()
        .all(db)
        .await?;

    let txn = db.begin().await?;
    crate::general_ledger::repository::delete_by_registrator_refs_with_conn(&txn, &ids).await?;

    let result = Entity::delete_many()
        .filter(Column::ConnectionMpRef.eq(connection_mp_ref))
        .filter(Column::TransactionDate.gte(date_from))
        .filter(Column::TransactionDate.lte(date_to))
        .exec(&txn)
        .await?;
    txn.commit().await?;

    Ok(result.rows_affected)
}
