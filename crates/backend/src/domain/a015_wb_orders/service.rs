use super::repository;
use anyhow::Result;
use chrono::NaiveDate;
use contracts::domain::a015_wb_orders::aggregate::WbOrders;
use contracts::domain::common::AggregateId;
use uuid::Uuid;

use crate::shared::marketplaces::wildberries::datetime::wb_business_date;

/// Автозаполнение marketplace_product_ref и nomenclature_ref
pub async fn auto_fill_references(document: &mut WbOrders) -> Result<()> {
    // Автозаполнение marketplace_product_ref если пустой
    if document.marketplace_product_ref.is_none() {
        if let Some(marketplace_sku) =
            crate::domain::a007_marketplace_product::service::wb_marketplace_sku(
                document.line.nm_id,
            )
        {
            let mp_id = crate::domain::a007_marketplace_product::service::find_or_create_for_sale(
                crate::domain::a007_marketplace_product::service::FindOrCreateParams {
                    marketplace_ref: document.header.marketplace_id.clone(),
                    connection_mp_ref: document.header.connection_id.clone(),
                    marketplace_sku,
                    article: Some(document.line.supplier_article.clone()),
                    barcode: Some(document.line.barcode.clone()),
                    title: if let Some(ref brand) = document.line.brand {
                        if !brand.trim().is_empty() {
                            brand.clone()
                        } else {
                            format!("Артикул: {}", document.line.supplier_article)
                        }
                    } else {
                        format!("Артикул: {}", document.line.supplier_article)
                    },
                },
            )
            .await?;

            document.marketplace_product_ref = Some(mp_id.to_string());
        }
    }

    // Перепроведение всегда зеркалит актуальное состояние a007 — единый резолвер.
    document.nomenclature_ref =
        crate::domain::a007_marketplace_product::service::resolve_wb_nomenclature_ref(
            &document.header.connection_id,
            document.line.nm_id,
            Some(&document.line.supplier_article),
        )
        .await?;

    refill_base_nomenclature_ref(document).await?;

    Ok(())
}

/// Принудительно пересчитывает base_nomenclature_ref по алгоритму:
/// 1) если у nomenclature_ref заполнен base_nomenclature_ref -> используем его
/// 2) иначе используем сам nomenclature_ref
pub async fn refill_base_nomenclature_ref(document: &mut WbOrders) -> Result<()> {
    const ZERO_UUID: &str = "00000000-0000-0000-0000-000000000000";
    document.base_nomenclature_ref = None;

    if let Some(ref nom_ref) = document.nomenclature_ref {
        document.base_nomenclature_ref = Some(nom_ref.clone());
        if let Ok(nom_uuid) = uuid::Uuid::parse_str(nom_ref) {
            if let Ok(Some(nomenclature)) =
                crate::domain::a004_nomenclature::service::get_by_id(nom_uuid).await
            {
                if let Some(base_ref) = nomenclature.base_nomenclature_ref {
                    if !base_ref.is_empty() && base_ref != ZERO_UUID {
                        document.base_nomenclature_ref = Some(base_ref);
                    }
                }
            }
        }
    }

    Ok(())
}

/// Автозаполнение dealer_price_ut из p906_nomenclature_prices.
/// Логика аналогична a012_wb_sales:
/// 1. Цена на дату по nomenclature_ref
/// 2. Цена на дату по base_nomenclature_ref
/// 3. Первая ненулевая цена по nomenclature_ref
/// 4. Первая ненулевая цена по base_nomenclature_ref
pub async fn fill_dealer_price(document: &mut WbOrders) -> Result<()> {
    const ZERO_UUID: &str = "00000000-0000-0000-0000-000000000000";

    let Some(ref nom_ref) = document.nomenclature_ref else {
        document.line.dealer_price_ut = None;
        return Ok(());
    };

    let order_date = wb_business_date(&document.state.order_dt).to_string();
    let mut price_source = String::new();

    let mut price = crate::projections::p906_nomenclature_prices::repository::get_price_for_date(
        nom_ref,
        &order_date,
    )
    .await
    .unwrap_or_else(|e| {
        tracing::warn!("Failed to get dealer price for {}: {}", nom_ref, e);
        None
    });

    if price.is_some() && price.unwrap_or(0.0) > 0.0 {
        price_source = format!("nomenclature {} on date {}", nom_ref, order_date);
    } else {
        price = None;
    }

    if price.is_none() {
        if let Ok(nom_uuid) = Uuid::parse_str(nom_ref) {
            if let Ok(Some(nomenclature)) =
                crate::domain::a004_nomenclature::service::get_by_id(nom_uuid).await
            {
                if let Some(ref base_ref) = nomenclature.base_nomenclature_ref {
                    if !base_ref.is_empty() && base_ref != ZERO_UUID {
                        price = crate::projections::p906_nomenclature_prices::repository::get_price_for_date(
                            base_ref,
                            &order_date,
                        )
                        .await
                        .unwrap_or_else(|e| {
                            tracing::warn!(
                                "Failed to get dealer price for base_nomenclature {}: {}",
                                base_ref,
                                e
                            );
                            None
                        });

                        if price.is_some() && price.unwrap_or(0.0) > 0.0 {
                            price_source =
                                format!("base_nomenclature {} on date {}", base_ref, order_date);
                        } else {
                            price = None;
                        }
                    }
                }
            }
        }
    }

    if price.is_none() {
        price = crate::projections::p906_nomenclature_prices::repository::get_first_nonzero_price(
            nom_ref,
        )
        .await
        .unwrap_or_else(|e| {
            tracing::warn!("Failed to get first nonzero price for {}: {}", nom_ref, e);
            None
        });

        if price.is_some() {
            price_source = format!("nomenclature {} (first nonzero price)", nom_ref);
        }
    }

    if price.is_none() {
        if let Ok(nom_uuid) = Uuid::parse_str(nom_ref) {
            if let Ok(Some(nomenclature)) =
                crate::domain::a004_nomenclature::service::get_by_id(nom_uuid).await
            {
                if let Some(ref base_ref) = nomenclature.base_nomenclature_ref {
                    if !base_ref.is_empty() && base_ref != ZERO_UUID {
                        price = crate::projections::p906_nomenclature_prices::repository::get_first_nonzero_price(
                            base_ref,
                        )
                        .await
                        .unwrap_or_else(|e| {
                            tracing::warn!(
                                "Failed to get first nonzero price for base_nomenclature {}: {}",
                                base_ref,
                                e
                            );
                            None
                        });

                        if price.is_some() {
                            price_source =
                                format!("base_nomenclature {} (first nonzero price)", base_ref);
                        }
                    }
                }
            }
        }
    }

    if price.is_some() {
        tracing::info!(
            "Filled dealer_price_ut = {:?} for WB Orders document {} (from {})",
            price,
            document.base.id.as_string(),
            price_source
        );
    } else {
        tracing::warn!(
            "Could not find dealer_price_ut for WB Orders document {} (nomenclature: {})",
            document.base.id.as_string(),
            nom_ref
        );
    }

    document.line.dealer_price_ut = price;
    Ok(())
}

pub async fn fill_dealer_price_resolved(document: &mut WbOrders) -> Result<()> {
    let Some(ref nom_ref) = document.nomenclature_ref else {
        document.line.dealer_price_ut = None;
        return Ok(());
    };

    let order_date = wb_business_date(&document.state.order_dt).to_string();
    let resolved =
        crate::projections::p906_nomenclature_prices::service::resolve_price_for_nomenclature(
            nom_ref,
            &order_date,
        )
        .await
        .unwrap_or_else(|e| {
            tracing::warn!("Failed to resolve dealer price for {}: {}", nom_ref, e);
            None
        });

    if let Some(ref resolved_price) = resolved {
        tracing::info!(
            "Filled dealer_price_ut = {:?} for WB Orders document {} (from {})",
            resolved_price.price,
            document.base.id.as_string(),
            resolved_price.describe(&order_date)
        );
    } else {
        tracing::warn!(
            "Could not find dealer_price_ut for WB Orders document {} (nomenclature: {})",
            document.base.id.as_string(),
            nom_ref
        );
    }

    document.line.dealer_price_ut = resolved.map(|resolved_price| resolved_price.price);
    Ok(())
}

/// Variant that uses the already-computed `document.base_nomenclature_ref`, skipping the
/// `a004_nomenclature` DB lookup inside `resolve_price_for_nomenclature`. Call this after
/// `auto_fill_references` (which runs `refill_base_nomenclature_ref` internally).
pub async fn fill_dealer_price_with_known_base_ref(document: &mut WbOrders) -> Result<()> {
    let nom_ref = match document.nomenclature_ref.clone() {
        Some(r) => r,
        None => {
            document.line.dealer_price_ut = None;
            return Ok(());
        }
    };

    let order_date = wb_business_date(&document.state.order_dt).to_string();
    let base_ref = document.base_nomenclature_ref.as_deref();

    let resolved = crate::projections::p906_nomenclature_prices::service::resolve_price_for_nomenclature_with_known_base(
        &nom_ref,
        base_ref,
        &order_date,
    )
    .await
    .unwrap_or_else(|e| {
        tracing::warn!("Failed to resolve dealer price for {}: {}", nom_ref, e);
        None
    });

    if let Some(ref resolved_price) = resolved {
        tracing::info!(
            "Filled dealer_price_ut = {:?} for WB Orders document {} (from {})",
            resolved_price.price,
            document.base.id.as_string(),
            resolved_price.describe(&order_date)
        );
    } else {
        tracing::warn!(
            "Could not find dealer_price_ut for WB Orders document {} (nomenclature: {})",
            document.base.id.as_string(),
            nom_ref
        );
    }

    document.line.dealer_price_ut = resolved.map(|r| r.price);
    Ok(())
}

/// Дополняет `line.price` из marketplace raw, если в строке ещё нет цены для allocation_basis.
pub async fn fill_line_price_from_marketplace_raw(order: &mut WbOrders) {
    if order.line.allocation_basis() > f64::EPSILON {
        return;
    }
    if let Some(price) = marketplace_price_rubles(order).await {
        order.line.price = Some(price);
    }
}

/// Из привязанного Marketplace-пейлоада проставить валюту заказа (`currency_code`/`fx_rate`)
/// и пересчитать рублёвый `sale_price`.
///
/// Зачем: Statistics API не отдаёт `salePrice` и при ре-импорте затирает `line_json`, а у
/// трансграничных заказов СНГ исходный `salePrice` приходит в валюте продажи (`currencyCode`,
/// напр. 398 KZT). `base_price` для `margin_pro` держится на `salePrice` и обязан быть в
/// рублях. Поэтому дочитываем значение из пейлоада (ref переживает ре-импорт) и:
/// - проставляем валюту-маркер (только для не-рублёвых заказов, иначе None);
/// - ПЕРЕЗАПИСЫВАЕМ `sale_price` из пейлоада с применением курса (`marketplace_field_rubles`).
///
/// Перезапись, а не «заполнить если пусто»: у заказов, импортированных до фикса валют,
/// на строке лежит salePrice в исходной валюте (раздутый), и без перерасчёта перепроведение
/// не исправляло бы `margin_pro`. FBW-заказы (без marketplace-пейлоада) — no-op.
pub async fn fill_sale_price_from_marketplace_raw(order: &mut WbOrders) {
    let Some(value) = marketplace_raw_json(order).await else {
        return;
    };

    let fx = marketplace_fx_rate(&value);
    match value.get("currencyCode").and_then(|v| v.as_i64()) {
        Some(code) if code != 643 => {
            order.line.currency_code = Some(code);
            order.line.fx_rate = Some(fx);
        }
        _ => {
            order.line.currency_code = None;
            order.line.fx_rate = None;
        }
    }

    if let Some(sale_price) = marketplace_field_rubles(&value, "salePrice") {
        order.line.sale_price = Some(sale_price);
    }
}

pub async fn update_line_price_if_missing(document_no: &str, price_rub: f64) -> Result<bool> {
    repository::update_line_price_if_missing(document_no, price_rub).await
}

async fn marketplace_raw_json(document: &WbOrders) -> Option<serde_json::Value> {
    let raw_ref = document
        .source_meta
        .marketplace_raw_payload_ref
        .as_deref()?;
    let raw_json = crate::shared::data::raw_storage::get_by_ref(raw_ref)
        .await
        .unwrap_or_else(|e| {
            tracing::warn!("Failed to load marketplace raw payload {}: {}", raw_ref, e);
            None
        })?;
    serde_json::from_str(&raw_json).ok()
}

/// WB exchange rate (RUB per unit of the order selling currency) from a Marketplace payload.
///
/// WB returns `price`/`salePrice` in the order selling currency (`currencyCode`); the RUB
/// equivalent lives in `convertedPrice`/`convertedFinalPrice` (`convertedCurrencyCode = 643`).
/// The rate is the ratio of a converted amount to its source amount (both minor units, so the
/// ratio is dimensionless). Falls back to 1.0 for RUB orders or legacy payloads without
/// converted amounts. Mirrors `wb_fx_rate` in the u504 marketplace_order processor.
fn marketplace_fx_rate(value: &serde_json::Value) -> f64 {
    let ratio = |num: &str, den: &str| {
        let n = value.get(num).and_then(|v| v.as_f64())?;
        let d = value.get(den).and_then(|v| v.as_f64())?;
        (n > 0.0 && d > 0.0).then_some(n / d)
    };
    ratio("convertedPrice", "price")
        .or_else(|| ratio("convertedFinalPrice", "finalPrice"))
        .unwrap_or(1.0)
}

/// Read a WB selling-currency amount (kopecks) from the payload and convert it to RUB.
/// Applies `marketplace_fx_rate`, so cross-border (KZT/BYN/AMD) amounts come out in rubles —
/// this keeps `base_price` for `margin_pro` truthful (must be a ruble sum).
fn marketplace_field_rubles(value: &serde_json::Value, field: &str) -> Option<f64> {
    let kopecks = value.get(field).and_then(|v| v.as_f64())?;
    if kopecks <= 0.0 {
        return None;
    }
    Some(kopecks / 100.0 * marketplace_fx_rate(value))
}

async fn marketplace_price_rubles(document: &WbOrders) -> Option<f64> {
    let value = marketplace_raw_json(document).await?;
    marketplace_field_rubles(&value, "price")
}

/// Базовая цена для расчёта margin_pro, взятая готовой со строки a015.
/// Приоритет: salePrice (Marketplace API) → price_with_disc (Statistics API).
/// Никаких обращений к raw-payload — все поля уже на агрегате.
fn resolve_base_price(document: &WbOrders) -> (f64, &'static str) {
    match document.line.sale_price {
        Some(sp) if sp > 0.0 => (sp, "salePrice (Marketplace API)"),
        _ => (
            document.line.price_with_disc.unwrap_or(0.0),
            "price_with_disc (Statistics API)",
        ),
    }
}

/// Расчёт плановой маржи margin_pro в процентах.
///
/// margin_pro = (base_price * (100 - П1 - П2) / 100 - dealer_price_ut)
///              / dealer_price_ut * 100
///
/// `base_price` = salePrice из Marketplace API (если есть и > 0), иначе
/// price_with_disc из Statistics API. salePrice доступен оперативно для
/// FBS-заказов, поэтому имеет приоритет.
///
/// где П1 — плановый процент комиссии, П2 — плановый процент эквайринга,
/// оба берутся из подключения маркетплейса (a006_connection_mp).
/// Возвращает None, если dealer_price_ut или base_price <= 0.
pub async fn calculate_margin_pro(document: &mut WbOrders) -> Result<()> {
    let dealer_price = document.line.dealer_price_ut.unwrap_or(0.0);

    // base_price берём готовым со строки: salePrice (Marketplace API) приоритетнее
    // price_with_disc (Statistics API). Оба хранятся на a015 — никаких обращений к
    // raw-payload здесь нет.
    let (base_price, price_source) = resolve_base_price(document);

    if dealer_price <= 0.0 || base_price <= 0.0 {
        document.line.margin_pro = None;
        return Ok(());
    }

    // Плановые проценты комиссии (П1) и эквайринга (П2) из подключения МП.
    let (commission_percent, acquiring_percent) =
        load_planned_percents(&document.header.connection_id).await;

    let net_price = base_price * (100.0 - commission_percent - acquiring_percent) / 100.0;
    let margin = (net_price - dealer_price) / dealer_price * 100.0;
    tracing::debug!(
        "margin_pro for WB Orders {} = {:.2}% (base_price={:.2} from {})",
        document.base.id.as_string(),
        margin,
        base_price,
        price_source
    );
    document.line.margin_pro = Some(margin);
    Ok(())
}

/// Variant that accepts a pre-loaded connection, skipping the `a006_connection_mp` DB lookup.
/// Call this when the connection was already loaded earlier in the same posting flow.
pub async fn calculate_margin_pro_with_connection(
    document: &mut WbOrders,
    connection: Option<&contracts::domain::a006_connection_mp::aggregate::ConnectionMP>,
) -> Result<()> {
    let dealer_price = document.line.dealer_price_ut.unwrap_or(0.0);

    let (base_price, price_source) = resolve_base_price(document);

    if dealer_price <= 0.0 || base_price <= 0.0 {
        document.line.margin_pro = None;
        return Ok(());
    }

    let (commission_percent, acquiring_percent) = match connection {
        Some(conn) => (
            conn.planned_commission_percent.unwrap_or(0.0),
            conn.planned_acquiring_percent.unwrap_or(0.0),
        ),
        None => load_planned_percents(&document.header.connection_id).await,
    };

    let net_price = base_price * (100.0 - commission_percent - acquiring_percent) / 100.0;
    let margin = (net_price - dealer_price) / dealer_price * 100.0;
    tracing::debug!(
        "margin_pro for WB Orders {} = {:.2}% (base_price={:.2} from {})",
        document.base.id.as_string(),
        margin,
        base_price,
        price_source
    );
    document.line.margin_pro = Some(margin);
    Ok(())
}

/// Загрузить плановые проценты комиссии и эквайринга из подключения маркетплейса.
/// Отсутствующие значения трактуются как 0.
async fn load_planned_percents(connection_id: &str) -> (f64, f64) {
    let Ok(connection_uuid) = Uuid::parse_str(connection_id) else {
        tracing::warn!(
            "calculate_margin_pro: invalid connection_id={}, using 0% commission/acquiring",
            connection_id
        );
        return (0.0, 0.0);
    };

    match crate::domain::a006_connection_mp::service::get_by_id(connection_uuid).await {
        Ok(Some(connection)) => (
            connection.planned_commission_percent.unwrap_or(0.0),
            connection.planned_acquiring_percent.unwrap_or(0.0),
        ),
        Ok(None) => {
            tracing::warn!(
                "calculate_margin_pro: connection not found, connection_id={}, using 0% commission/acquiring",
                connection_id
            );
            (0.0, 0.0)
        }
        Err(e) => {
            tracing::warn!(
                "calculate_margin_pro: failed to load connection {}: {}, using 0% commission/acquiring",
                connection_id,
                e
            );
            (0.0, 0.0)
        }
    }
}

pub async fn store_document_with_raw(mut document: WbOrders, raw_json: &str) -> Result<Uuid> {
    let raw_ref = crate::shared::data::raw_storage::save_raw_json(
        "WB",
        "WB_Orders",
        &document.header.document_no,
        raw_json,
        document.source_meta.fetched_at,
    )
    .await?;

    if let Some(raw_ref) = raw_ref {
        document.source_meta.raw_payload_ref = raw_ref;
    }

    // Preserve income_id and numeric order ID set by Marketplace API.
    // Marketplace API populates supplyId in real-time; statistics API lags 1-3 days.
    // Also, marketplace API stores the numeric WB order ID in line_id; statistics API only has srid.
    let needs_income_check = document.source_meta.income_id.is_none();
    // Statistics API sets line_id to srid (non-numeric); marketplace set it to digits-only order ID.
    let current_line_id_is_numeric = document.line.line_id.chars().all(|c| c.is_ascii_digit());
    if needs_income_check || !current_line_id_is_numeric {
        if let Ok(Some(existing)) =
            repository::get_by_document_no(&document.header.document_no).await
        {
            if needs_income_check {
                if let Some(existing_income_id) = existing.source_meta.income_id {
                    if existing_income_id != 0 {
                        document.source_meta.income_id = Some(existing_income_id);
                    }
                }
            }
            document.source_meta.marketplace_raw_payload_ref =
                existing.source_meta.marketplace_raw_payload_ref.clone();
            // Preserve numeric WB order ID stored by marketplace import
            if !current_line_id_is_numeric
                && existing.line.line_id.chars().all(|c| c.is_ascii_digit())
                && !existing.line.line_id.is_empty()
            {
                document.line.line_id = existing.line.line_id;
            }
        }
    }

    // Statistics API не отдаёт salePrice и затирает им строку при ре-импорте; дочитываем
    // его обратно из сохранённого Marketplace-пейлоада, иначе base_price=0 и margin_pro
    // не посчитается. marketplace_raw_payload_ref уже восстановлен из existing выше.
    fill_sale_price_from_marketplace_raw(&mut document).await;

    // Автозаполнение ссылок
    auto_fill_references(&mut document).await?;

    document
        .validate()
        .map_err(|e| anyhow::anyhow!("Validation failed: {}", e))?;
    document.before_write();

    let id = repository::upsert_document(&document).await?;

    // Проводим документ если is_posted = true
    if document.is_posted {
        if let Err(e) = super::posting::post_document(id).await {
            tracing::error!("Failed to post WB Orders document: {}", e);
            // Не останавливаем выполнение, т.к. документ уже сохранен
        }
    }

    Ok(id)
}

pub async fn store_marketplace_raw_payload(
    document_no: &str,
    raw_json: &str,
) -> Result<Option<String>> {
    let raw_ref = crate::shared::data::raw_storage::save_raw_json(
        "WB",
        "WB_Orders_Marketplace",
        document_no,
        raw_json,
        chrono::Utc::now(),
    )
    .await?;

    // salePrice парсим из пейлоада один раз — здесь, на записи, — и кладём готовым
    // на строку a015. Дальше расчёт margin_pro берёт его прямо со строки и больше
    // не читает raw-payload.
    let sale_price = serde_json::from_str::<serde_json::Value>(raw_json)
        .ok()
        .and_then(|value| marketplace_field_rubles(&value, "salePrice"));

    let updated = repository::update_marketplace_payload_by_document_no(
        document_no,
        raw_ref.as_deref(),
        sale_price,
    )
    .await?;

    // Привязка свежего Marketplace-пейлоада приносит `salePrice`, который является
    // основным источником `base_price` для `margin_pro` у FBS-заказов (Statistics API
    // отдаёт `price_with_disc` с задержкой). Пейлоад прикрепляется ПОСЛЕ авто-проведения
    // в `store_document_with_raw`, поэтому при первичном проведении `margin_pro` не мог
    // быть рассчитан. Перепроводим уже проведённый документ, чтобы пересчитать
    // `margin_pro` (и проекции p909/GL) с актуальным `salePrice`.
    if updated {
        if let Ok(Some(document)) = repository::get_by_document_no(document_no).await {
            if document.is_posted {
                if let Err(e) = super::posting::post_document(document.base.id.value()).await {
                    tracing::error!(
                        "Failed to re-post WB Orders {} after attaching marketplace payload: {}",
                        document_no,
                        e
                    );
                }
            }
        }
    }

    Ok(if updated { raw_ref } else { None })
}

pub async fn get_by_id(id: Uuid) -> Result<Option<WbOrders>> {
    repository::get_by_id(id).await
}

pub async fn get_by_document_no(document_no: &str) -> Result<Option<WbOrders>> {
    repository::get_by_document_no(document_no).await
}

pub async fn list_all() -> Result<Vec<WbOrders>> {
    repository::list_all().await
}

pub async fn list_by_date_range(
    date_from: Option<NaiveDate>,
    date_to: Option<NaiveDate>,
) -> Result<Vec<WbOrders>> {
    repository::list_by_date_range(date_from, date_to).await
}

/// Find orders that belong to a supply, matched by incomeID from WB Statistics API.
/// Supply ID like "WB-GI-229481414" → income_id = 229481414.
pub async fn list_by_income_id(income_id: i64) -> Result<Vec<WbOrders>> {
    repository::list_by_income_id(income_id).await
}

pub async fn list_by_numeric_order_ids(order_ids: &[i64]) -> Result<Vec<WbOrders>> {
    repository::list_by_numeric_order_ids(order_ids).await
}

/// Помечает заказ отменённым (FBS-путь импорта) и перепроводит его, чтобы движение
/// отмены появилось в p916. Forward-only: уже отменённый заказ не трогается, поэтому
/// повторные проходы поллинга не переписывают дату отмены из Statistics API.
///
/// Возвращает `true`, если заказ только что стал отменённым.
pub async fn mark_cancelled_by_document_no(
    document_no: &str,
    observed_at: chrono::DateTime<chrono::Utc>,
) -> Result<bool> {
    let changed = repository::mark_cancelled_by_document_no(document_no, observed_at).await?;
    if !changed {
        return Ok(false);
    }

    // Перепроведение обязательно: p916 наполняется push-ом при проведении, сам по себе
    // UPDATE документа движений не создаст.
    match repository::get_by_document_no(document_no).await? {
        Some(document) => {
            let id = document.base.id.value();
            if let Err(e) = super::posting::post_document(id).await {
                tracing::error!(
                    "Failed to repost cancelled WB order {} ({}): {}",
                    document_no,
                    id,
                    e
                );
            }
        }
        None => tracing::warn!(
            "WB order {} disappeared right after cancel update — p916 not rebuilt",
            document_no
        ),
    }
    Ok(true)
}

/// Update income_id for an order by its document_no (srid).
/// Called when marketplace API provides supply assignment in real-time.
pub async fn update_income_id_by_document_no(document_no: &str, income_id: i64) -> Result<bool> {
    repository::update_income_id_by_document_no(document_no, income_id).await
}

pub async fn set_income_id_by_document_no(
    document_no: &str,
    income_id: Option<i64>,
) -> Result<bool> {
    repository::set_income_id_by_document_no(document_no, income_id).await
}

/// Update numeric WB order ID (line_id) for an existing order.
/// Only updates if line_id is currently non-numeric (e.g. srid from Statistics API).
pub async fn update_line_id_by_document_no(document_no: &str, line_id: i64) -> Result<()> {
    repository::update_line_id_by_document_no(document_no, line_id).await
}

pub async fn delete(id: Uuid) -> Result<bool> {
    repository::soft_delete(id).await
}

/// Заказы WB для вкладки «Заказы» на карточке номенклатуры.
pub struct NomenclatureOrders;

#[async_trait::async_trait]
impl crate::domain::a004_nomenclature::service::NomenclatureOrderSource for NomenclatureOrders {
    async fn orders_for_nomenclature(
        &self,
        nomenclature_ref: &str,
        date_from: &str,
    ) -> anyhow::Result<
        Vec<contracts::domain::a004_nomenclature::orders_dto::NomenclatureOrderRowDto>,
    > {
        use contracts::domain::a004_nomenclature::orders_dto::NomenclatureOrderRowDto;

        let rows =
            super::repository::list_orders_for_nomenclature(nomenclature_ref, date_from).await?;

        Ok(rows
            .into_iter()
            .map(|r| NomenclatureOrderRowDto {
                id: r.id,
                marketplace: "WB".to_string(),
                document_no: r.document_no,
                order_date: r.document_date,
                is_cancel: r.is_cancel,
                is_supply: r.is_supply,
                is_realization: r.is_realization,
                line_status: None,
                status_norm: None,
                qty: r.qty.unwrap_or(1.0),
                price_before_discount: r.total_price.or(r.price),
                price_after_discount: r.price_with_disc.or(r.finished_price),
                final_buyer_price: r.finished_price,
                dealer_price_ut: r.dealer_price_ut,
                margin_pro: r.margin_pro,
            })
            .collect())
    }
}
