use super::repository;
use anyhow::Result;
use contracts::domain::a006_connection_mp::aggregate::ConnectionMP;
use uuid::Uuid;

/// Загрузить подключение маркетплейса для документа. Используется единожды в начале
/// `post_document` и передаётся во все под-функции, которые в нём нуждаются.
async fn load_connection(
    document: &contracts::domain::a015_wb_orders::aggregate::WbOrders,
) -> Option<ConnectionMP> {
    let connection_uuid = Uuid::parse_str(&document.header.connection_id).ok()?;
    crate::domain::a006_connection_mp::service::get_by_id(connection_uuid)
        .await
        .unwrap_or_else(|e| {
            tracing::warn!(
                "Failed to load connection {} for WB Orders {}: {}",
                document.header.connection_id,
                document.base.id.value(),
                e
            );
            None
        })
}

/// Синхронизировать organization_id из уже загруженного подключения.
async fn sync_organization_from_connection(
    document: &mut contracts::domain::a015_wb_orders::aggregate::WbOrders,
    connection: Option<&ConnectionMP>,
) -> Result<()> {
    let Some(connection) = connection else {
        tracing::warn!(
            "Skip organization sync for WB Orders {}: connection not found, connection_id={}",
            document.base.id.value(),
            document.header.connection_id
        );
        return Ok(());
    };

    let organization_ref = connection.organization_ref.trim().trim_matches('"');
    let organization_uuid = match Uuid::parse_str(organization_ref) {
        Ok(uuid) => uuid,
        Err(_) => {
            tracing::warn!(
                "Skip organization sync for WB Orders {}: invalid organization_ref={}",
                document.base.id.value(),
                connection.organization_ref
            );
            return Ok(());
        }
    };

    if crate::domain::a002_organization::service::get_by_id(
        crate::shared::data::db::get_connection(),
        organization_uuid,
    )
    .await?
    .is_none()
    {
        tracing::warn!(
            "Skip organization sync for WB Orders {}: organization_ref not found={}",
            document.base.id.value(),
            connection.organization_ref
        );
        return Ok(());
    }

    let resolved_org_id = organization_uuid.to_string();
    if document.header.organization_id != resolved_org_id {
        tracing::info!(
            "Sync organization for WB Orders {}: {} -> {}",
            document.base.id.value(),
            document.header.organization_id,
            resolved_org_id
        );
        document.header.organization_id = resolved_org_id;
    }

    Ok(())
}

const REGISTRATOR_TYPE: &str = "a015_wb_orders";

fn registrator_ref(id: Uuid) -> String {
    format!("a015:{id}")
}

pub async fn post_document(id: Uuid) -> Result<()> {
    let mut document = repository::get_by_id(id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Document not found: {}", id))?;

    // Load connection once — reused by sync_org and calculate_margin_pro,
    // avoiding two separate DB round-trips to a006_connection_mp.
    let connection = load_connection(&document).await;

    sync_organization_from_connection(&mut document, connection.as_ref()).await?;

    // auto_fill_references calls refill_base_nomenclature_ref internally —
    // do NOT call refill_base_nomenclature_ref separately after this.
    super::service::auto_fill_references(&mut document).await?;

    // document.base_nomenclature_ref is already resolved above;
    // passing it through skips the third a004_nomenclature DB lookup.
    super::service::fill_dealer_price_with_known_base_ref(&mut document).await?;
    // Восстанавливаем salePrice из Marketplace-пейлоада, если Statistics-импорт его затёр:
    // без него base_price=0 и margin_pro не считается. Делает перепроведение
    // самовосстанавливающимся для заказов, у которых salePrice потерялся на строке.
    super::service::fill_sale_price_from_marketplace_raw(&mut document).await;
    super::service::calculate_margin_pro_with_connection(&mut document, connection.as_ref())
        .await?;

    document.is_posted = true;
    document.base.metadata.is_posted = true;
    document.before_write();

    // Direct UPDATE by ID — skips the get_by_document_no round-trip of upsert_document.
    repository::update_posted_document(&document).await?;

    // Идемпотентное перепроведение: удаляем старые строки p909 и GL, затем
    // создаём заново, чтобы повторный Post всегда давал актуальный результат.
    let p909_ref = registrator_ref(id);
    crate::projections::p909_mp_order_line_turnovers::service::remove_by_registrator_ref(&p909_ref)
        .await?;
    crate::general_ledger::service::remove_by_registrator(REGISTRATOR_TYPE, &id.to_string())
        .await?;

    crate::projections::p909_mp_order_line_turnovers::service::project_wb_order(&document, id)
        .await?;

    // Стадия 2 воронки p916: движения «заказ»/«отмена». delete-by-registrator + insert
    // атомарны в собственной транзакции. Проведение a015 в целом не обёрнуто в единую
    // транзакцию (p909/GL идут отдельными autocommit-вызовами) — это существующее свойство
    // модуля, не специфика p916; общий рефактор проведения — вне рамок этой задачи.
    {
        use sea_orm::TransactionTrait;
        let reg_ref = id.to_string();
        let db = crate::shared::data::db::get_connection();
        let txn = db.begin().await?;
        crate::projections::p916_mp_sales_funnel_turnovers::repository::delete_by_registrator_with_conn(
            &txn,
            crate::projections::p916_mp_sales_funnel_turnovers::builder::REG_A015,
            &reg_ref,
        )
        .await?;
        let rows = crate::projections::p916_mp_sales_funnel_turnovers::builder::from_wb_orders(
            &document, &reg_ref,
        );
        crate::projections::p916_mp_sales_funnel_turnovers::repository::insert_many_with_conn(
            &txn, &rows,
        )
        .await?;
        txn.commit().await?;
    }

    // Сигнал клиентам обновить открытые списки a015 (margin_pro и пр. могли измениться).
    super::change_token::TOKEN.bump();

    tracing::info!("Posted WB Orders document: {}", id);
    Ok(())
}

pub async fn unpost_document(id: Uuid) -> Result<()> {
    let mut document = repository::get_by_id(id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Document not found: {}", id))?;

    document.is_posted = false;
    document.base.metadata.is_posted = false;
    document.before_write();

    repository::update_posted_document(&document).await?;

    // Убираем все связанные результаты при отмене проведения.
    let p909_ref = registrator_ref(id);
    crate::projections::p909_mp_order_line_turnovers::service::remove_by_registrator_ref(&p909_ref)
        .await?;
    crate::general_ledger::service::remove_by_registrator(REGISTRATOR_TYPE, &id.to_string())
        .await?;
    crate::projections::p916_mp_sales_funnel_turnovers::repository::delete_by_registrator_ref(
        &id.to_string(),
    )
    .await?;

    // Сигнал клиентам обновить открытые списки a015.
    super::change_token::TOKEN.bump();

    tracing::info!("Unposted WB Orders document: {}", id);

    Ok(())
}
