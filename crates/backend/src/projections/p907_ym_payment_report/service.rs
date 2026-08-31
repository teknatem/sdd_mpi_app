use anyhow::Result;
use contracts::domain::common::AggregateId;
use sea_orm::{EntityTrait, Set, TransactionTrait};

use crate::shared::data::db::get_connection;

pub async fn rebuild_entry_from_existing(id: &str) -> Result<usize> {
    let Some(row) = crate::projections::p907_ym_payment_report::repository::get_by_uuid(id).await?
    else {
        return Ok(0);
    };
    rebuild_from_row(row).await
}

/// Перепровести уже загруженную строку p907: дозаполнить производные ссылки и
/// перестроить GL/p914. Принимает строку напрямую, избегая повторного SELECT
/// (вызывается и по `id`, и по `record_key`).
async fn rebuild_from_row(
    mut row: crate::projections::p907_ym_payment_report::repository::Model,
) -> Result<usize> {
    // Первый этап проведения: дозаполнить производные ссылки (если пусто) и
    // сохранить в строке p907 — далее они просто копируются в p914.
    resolve_and_persist_marketplace_refs(&mut row).await?;

    let db = get_connection();
    let txn = db.begin().await?;

    crate::general_ledger::repository::delete_by_registrator_with_conn(
        &txn,
        "p907_ym_payment_report",
        &row.id,
    )
    .await?;
    crate::projections::p914_mp_finance_turnovers::repository::delete_by_registrator_refs_with_conn(
        &txn,
        std::slice::from_ref(&row.id),
    )
    .await?;

    let general_ledger_entries =
        crate::projections::p907_ym_payment_report::general_ledger_builder::build_general_ledger_entries(
            &row,
            "",
        )?;
    for entry in &general_ledger_entries {
        crate::general_ledger::repository::save_entry_with_conn(&txn, entry).await?;
    }

    let finance_turnovers =
        crate::projections::p907_ym_payment_report::general_ledger_builder::build_finance_turnover_entries(
            &row,
            &general_ledger_entries,
        );
    for turnover in &finance_turnovers {
        crate::projections::p914_mp_finance_turnovers::repository::save_entry_with_conn(
            &txn, turnover,
        )
        .await?;
    }

    // Таймлайн событий заказа (p915): оплата / возврат оплаты по этой строке p907.
    crate::projections::p915_mp_order_events::repository::delete_by_registrator_refs_with_conn(
        &txn,
        std::slice::from_ref(&row.id),
    )
    .await?;
    let order_events = crate::projections::p915_mp_order_events::builder::from_ym_payment_row(&row);
    for event in &order_events {
        crate::projections::p915_mp_order_events::repository::insert_entry_raw_with_conn(
            &txn, event,
        )
        .await?;
    }

    txn.commit().await?;

    Ok(general_ledger_entries.len())
}

/// Резолвит и заполняет производные ссылки строки p907 и сохраняет изменения в БД:
/// `marketplace_product_ref` (a007 по shop_sku) и `marketplace_order_ref`
/// (a013_ym_order по order_id) — резолвятся только если ещё пусто.
/// `nomenclature_ref` — зеркало `a007.nomenclature_ref` по marketplace_product_ref;
/// перерезолвится на каждом проведении, чтобы отражать актуальную привязку a007 к
/// номенклатуре 1С (она может появиться позже через сопоставление u505), по аналогии
/// с WB-веткой (p903 → resolve_wb_nomenclature_ref). Все три копируются затем в p914.
async fn resolve_and_persist_marketplace_refs(
    row: &mut crate::projections::p907_ym_payment_report::repository::Model,
) -> Result<()> {
    use crate::projections::p907_ym_payment_report::repository::{ActiveModel, Entity};

    let mut changed = false;

    if row.marketplace_product_ref.is_none() {
        if let Some(sku) = row
            .shop_sku
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty())
        {
            if let Some(mp_ref) =
                crate::domain::a007_marketplace_product::service::resolve_marketplace_product_ref(
                    &row.connection_mp_ref,
                    sku,
                    None,
                )
                .await?
            {
                row.marketplace_product_ref = Some(mp_ref);
                changed = true;
            }
        }
    }

    if row.marketplace_order_ref.is_none() {
        if let Some(order_id) = row.order_id {
            if let Some(order) =
                crate::domain::a013_ym_order::repository::get_by_document_no(&order_id.to_string())
                    .await?
            {
                row.marketplace_order_ref = Some(order.base.id.as_string());
                changed = true;
            }
        }
    }

    // Зеркалим актуальную привязку a007 → номенклатура 1С (всегда, не только если пусто).
    let nomenclature_ref = match row
        .marketplace_product_ref
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .and_then(|v| uuid::Uuid::parse_str(v).ok())
    {
        Some(mp_id) => crate::domain::a007_marketplace_product::service::get_by_id(mp_id)
            .await?
            .and_then(|product| product.nomenclature_ref)
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty()),
        None => None,
    };
    if row.nomenclature_ref != nomenclature_ref {
        row.nomenclature_ref = nomenclature_ref;
        changed = true;
    }

    if changed {
        let am = ActiveModel {
            record_key: Set(row.record_key.clone()),
            marketplace_product_ref: Set(row.marketplace_product_ref.clone()),
            marketplace_order_ref: Set(row.marketplace_order_ref.clone()),
            nomenclature_ref: Set(row.nomenclature_ref.clone()),
            ..Default::default()
        };
        Entity::update(am).exec(get_connection()).await?;
    }

    Ok(())
}

/// Массовое перепроведение всех существующих строк p907: для каждой строки
/// перерезолвит ссылки и перестроит GL/p914. Возвращает (число обработанных строк,
/// суммарное число GL-проводок). Используется после изменения маппинга оборотов,
/// чтобы провести ранее не отражавшиеся операции.
pub async fn repost_all() -> Result<(usize, usize)> {
    let ids = crate::projections::p907_ym_payment_report::repository::list_all_ids().await?;
    let mut rows = 0usize;
    let mut gl_entries = 0usize;
    for id in ids {
        gl_entries += rebuild_entry_from_existing(&id).await?;
        rows += 1;
    }
    Ok((rows, gl_entries))
}

/// Снимает pending-строки «Будет …» (прогноз выплаты), у которых уже есть проведённый
/// двойник (тот же `order_id` / `transaction_type` / `shop_sku` / `|transaction_sum|`;
/// статус двойника НЕ «Будет …» и НЕ «Справочно …»). Прогноз и факт — одни деньги; двойной
/// учёт задваивает сумму в документе a013.
///
/// Дату в сопоставлении НЕ учитываем — прогноз может быть датирован иначе, чем фактическая
/// выплата. Если проведённого двойника нет, строка сохраняется («невозможно
/// идентифицировать — оставить»). Снимает строку целиком: GL (`sys_general_ledger`), p914,
/// p915 и саму p907. Возвращает число удалённых строк.
///
/// **Производительность:** проверяются только заказы `candidate_order_ids` (обычно —
/// `order_id` pending-строк текущей партии импорта), поэтому обе стороны сопоставления идут
/// по `idx_p907_order_id`, а стоимость зависит от размера партии, а не от размера всей
/// (очень большой) таблицы p907. Пустой список — мгновенный `Ok(0)` без запроса.
///
/// Вызывается фазой импорта (u503) — не даёт появляться новым дублям; ту же логику для
/// уже накопленных строк единожды выполнила миграция `0186_p907_dedup_pending_payouts`.
pub async fn purge_superseded_pending_payouts(candidate_order_ids: &[i64]) -> Result<usize> {
    use crate::projections::p907_ym_payment_report::repository::{Column, Entity};
    use sea_orm::{ColumnTrait, ConnectionTrait, DatabaseBackend, QueryFilter, Statement};

    if candidate_order_ids.is_empty() {
        return Ok(0);
    }

    let db = get_connection();

    // `id` pending-строк «Будет …» с проведённым двойником — только среди заданных заказов.
    // Порциями по order_id (лимит выражений SQLite), каждая порция бьёт по idx_p907_order_id.
    let mut dead_ids: Vec<String> = Vec::new();
    for chunk in candidate_order_ids.chunks(900) {
        // order_id — i64 из БД, не пользовательский ввод: инлайн безопасен (нет инъекции).
        let in_list = chunk
            .iter()
            .map(i64::to_string)
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!(
            "SELECT p.id AS id \
             FROM p907_ym_payment_report p \
             WHERE p.order_id IN ({in_list}) \
               AND p.payment_status LIKE 'Будет %' \
               AND EXISTS ( \
                 SELECT 1 FROM p907_ym_payment_report s \
                 WHERE s.order_id = p.order_id \
                   AND s.transaction_type = p.transaction_type \
                   AND IFNULL(s.shop_sku, '') = IFNULL(p.shop_sku, '') \
                   AND CAST(ROUND(ABS(s.transaction_sum) * 100) AS INTEGER) \
                     = CAST(ROUND(ABS(p.transaction_sum) * 100) AS INTEGER) \
                   AND s.payment_status NOT LIKE 'Будет %' \
                   AND s.payment_status NOT LIKE 'Справочно%' \
                   AND s.id <> p.id \
               )"
        );
        let rows = db
            .query_all(Statement::from_string(DatabaseBackend::Sqlite, sql))
            .await?;
        dead_ids.extend(
            rows.iter()
                .filter_map(|row| row.try_get::<String>("", "id").ok()),
        );
    }

    if dead_ids.is_empty() {
        return Ok(0);
    }

    let txn = db.begin().await?;
    for id in &dead_ids {
        // Те же helper'ы снятия проводок, что и в `rebuild_from_row` (GL/p914/p915).
        crate::general_ledger::repository::delete_by_registrator_with_conn(
            &txn,
            "p907_ym_payment_report",
            id,
        )
        .await?;
        crate::projections::p914_mp_finance_turnovers::repository::delete_by_registrator_refs_with_conn(
            &txn,
            std::slice::from_ref(id),
        )
        .await?;
        crate::projections::p915_mp_order_events::repository::delete_by_registrator_refs_with_conn(
            &txn,
            std::slice::from_ref(id),
        )
        .await?;
        Entity::delete_many()
            .filter(Column::Id.eq(id))
            .exec(&txn)
            .await?;
    }
    txn.commit().await?;

    tracing::info!(
        "p907 purge_superseded_pending_payouts: снято {} pending-строк «Будет …» с проведённым двойником",
        dead_ids.len()
    );
    Ok(dead_ids.len())
}

pub async fn rebuild_record_key_from_existing(record_key: &str) -> Result<usize> {
    let Some(row) =
        crate::projections::p907_ym_payment_report::repository::get_by_record_key(record_key)
            .await?
    else {
        return Ok(0);
    };

    // Строка уже загружена — перестраиваем напрямую, без повторного SELECT по id.
    rebuild_from_row(row).await
}

/// Пересбор p907 за период для страницы перепроведения `u508`.
///
/// Два прохода, и второй не забыть: построчный GL пересобирается по каждой
/// записи периода, а перечисления (Дт51/Кт7609) строятся одной проводкой на
/// банковский ордер — отдельно от строк. Один репост p907 обязан покрывать и
/// то и другое, иначе остаток по 7609 разъедется молча.
pub struct Repost;

#[async_trait::async_trait]
impl crate::usecases::u508_repost_documents::ProjectionRepost for Repost {
    fn key(&self) -> &'static str {
        "p907_ym_payment_report"
    }

    fn option(&self) -> crate::usecases::u508_repost_documents::ProjectionOptionInfo {
        crate::usecases::u508_repost_documents::ProjectionOptionInfo {
            label: "p907 — YM Payment Report",
            description: "Пересборка general ledger по всем строкам p907 за период (включая перечисления Дт51/Кт7609 по банковским ордерам) и событий оплаты p915",
        }
    }

    async fn rebuild(
        &self,
        ctx: &crate::usecases::u508_repost_documents::RepostContext<'_>,
    ) -> anyhow::Result<()> {
        const KEY: &str = "p907_ym_payment_report";

        let ids = super::repository::list_ids_by_transaction_date_range(ctx.date_from, ctx.date_to)
            .await?;
        let total = ids.len() as i32;
        ctx.tracker.set_total(ctx.session_id, total);

        let mut reposted = 0;
        for (index, id) in ids.iter().enumerate() {
            let current_item = format!("{KEY} {id}");
            ctx.tracker.update_progress(
                ctx.session_id,
                index as i32,
                reposted,
                Some(current_item.clone()),
            );

            match rebuild_entry_from_existing(id).await {
                Ok(_) => reposted += 1,
                Err(error) => ctx.tracker.add_error(
                    ctx.session_id,
                    format!("Failed to repost {KEY} {id}: {error}"),
                ),
            }

            ctx.tracker.update_progress(
                ctx.session_id,
                (index + 1) as i32,
                reposted,
                Some(current_item),
            );
        }

        if let Err(error) =
            super::settlement_posting::rebuild_settlements_for_range(ctx.date_from, ctx.date_to)
                .await
        {
            ctx.tracker.add_error(
                ctx.session_id,
                format!("Failed to rebuild {KEY} settlements: {error}"),
            );
        }

        ctx.tracker
            .update_progress(ctx.session_id, total, reposted, None);
        Ok(())
    }
}
