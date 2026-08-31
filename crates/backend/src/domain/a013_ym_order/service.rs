use super::repository;
use anyhow::Result;
use chrono::{DateTime, Utc};
use contracts::domain::a007_marketplace_product::aggregate::MarketplaceProductDto;
use contracts::domain::a013_ym_order::aggregate::{YmOrder, YmOrderLine};
use contracts::domain::common::AggregateId;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

const ZERO_UUID: &str = "00000000-0000-0000-0000-000000000000";

/// Автозаполнение marketplace_product_ref и nomenclature_ref для одной строки
/// Возвращает обновлённую строку
async fn fill_line_references(
    line: &YmOrderLine,
    connection_id: &str,
    marketplace_id: &str,
) -> Result<YmOrderLine> {
    let mut updated_line = line.clone();

    // Если уже заполнено - пропускаем
    if updated_line.marketplace_product_ref.is_some() && updated_line.nomenclature_ref.is_some() {
        return Ok(updated_line);
    }

    // Ищем marketplace_product по connection и SKU (используем offer_id или shop_sku)
    let sku = if !line.offer_id.is_empty() {
        &line.offer_id
    } else {
        &line.shop_sku
    };

    // Используем shop_sku для поиска в p901 (артикул YM)
    let ym_article = &line.shop_sku;

    let marketplace_product =
        crate::domain::a007_marketplace_product::service::get_by_connection_and_sku(
            connection_id,
            sku,
        )
        .await?;

    let mp_id = if let Some(existing) = marketplace_product {
        existing.base.id.as_string()
    } else {
        // Создаем новый a007_marketplace_product
        let dto = MarketplaceProductDto {
            id: None,
            code: Some(format!("YM-AUTO-{}", Uuid::new_v4())),
            description: if line.name.trim().is_empty() {
                format!("Артикул: {}", sku)
            } else {
                line.name.clone()
            },
            marketplace_ref: marketplace_id.to_string(),
            connection_mp_ref: connection_id.to_string(),
            marketplace_sku: sku.to_string(),
            barcode: None,
            article: sku.to_string(),
            brand: None,
            category_id: None,
            category_name: None,
            last_update: Some(chrono::Utc::now()),
            nomenclature_ref: None,
            comment: Some(format!(
                "Автоматически создано при импорте YM Order [{}]",
                chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC")
            )),
        };

        let created_id = crate::domain::a007_marketplace_product::service::create(dto).await?;
        tracing::info!("Created new marketplace_product with id: {}", created_id);
        created_id.to_string()
    };

    updated_line.marketplace_product_ref = Some(mp_id.clone());

    // Получаем nomenclature_ref из marketplace_product
    if updated_line.nomenclature_ref.is_none() {
        if let Ok(mp_uuid) = Uuid::parse_str(&mp_id) {
            // Сначала пробуем получить из существующего a007
            if let Ok(Some(mp)) =
                crate::domain::a007_marketplace_product::service::get_by_id(mp_uuid).await
            {
                if let Some(nom_ref) = mp.nomenclature_ref {
                    updated_line.nomenclature_ref = Some(nom_ref);
                }
            }

            // Если nomenclature_ref всё ещё не заполнен - пробуем найти через p901 по YM штрихкоду
            if updated_line.nomenclature_ref.is_none() && !ym_article.is_empty() {
                if let Ok(Some(nom_ref)) =
                    crate::domain::a007_marketplace_product::service::try_fill_nomenclature_from_ym_barcode(
                        mp_uuid,
                        ym_article,
                    )
                    .await
                {
                    updated_line.nomenclature_ref = Some(nom_ref);
                    tracing::info!(
                        "Filled nomenclature_ref for YM Order line {} via p901 barcode search (article: '{}')",
                        line.line_id,
                        ym_article
                    );
                }
            }
        }
    }

    // Устанавливаем price_plan = 0 если не задано
    if updated_line.price_plan.is_none() {
        updated_line.price_plan = Some(0.0);
    }

    Ok(updated_line)
}

/// Автозаполнение marketplace_product_ref и nomenclature_ref для всех строк документа
/// Обновляет поле is_error на основе наличия nomenclature_ref
pub async fn auto_fill_references(document: &mut YmOrder) -> Result<()> {
    let connection_id = &document.header.connection_id;
    let marketplace_id = &document.header.marketplace_id;

    let mut updated_lines = Vec::with_capacity(document.lines.len());

    for line in &document.lines {
        match fill_line_references(line, connection_id, marketplace_id).await {
            Ok(updated_line) => {
                updated_lines.push(updated_line);
            }
            Err(e) => {
                tracing::warn!("Failed to fill references for line {}: {}", line.line_id, e);
                // Добавляем оригинальную строку без изменений
                updated_lines.push(line.clone());
            }
        }
    }

    document.lines = updated_lines;

    // Обновляем is_error на основе строк
    document.update_is_error();
    // Пересчитываем итоги
    document.recalculate_totals();

    Ok(())
}

/// Заполнить dealer_price_ut для каждой строки документа из p906_nomenclature_prices.
/// Логика аналогична a015_wb_orders::fill_dealer_price:
/// 1. Цена на дату по nomenclature_ref
/// 2. Цена на дату по base_nomenclature_ref
/// 3. Первая ненулевая цена по nomenclature_ref
/// 4. Первая ненулевая цена по base_nomenclature_ref
pub async fn fill_dealer_price_for_lines(document: &mut YmOrder) -> Result<()> {
    // Определяем дату документа (creation_date или сегодня)
    let order_date = document
        .state
        .creation_date
        .map(|dt| dt.format("%Y-%m-%d").to_string())
        .unwrap_or_else(|| chrono::Utc::now().format("%Y-%m-%d").to_string());

    for line in document.lines.iter_mut() {
        let Some(ref nom_ref) = line.nomenclature_ref.clone() else {
            line.dealer_price_ut = None;
            continue;
        };

        let mut price_source = String::new();

        // 1. По nomenclature_ref на дату
        let mut price =
            crate::projections::p906_nomenclature_prices::repository::get_price_for_date(
                nom_ref,
                &order_date,
            )
            .await
            .unwrap_or_else(|e| {
                tracing::warn!(
                    "fill_dealer_price_for_lines: failed to get price for {}: {}",
                    nom_ref,
                    e
                );
                None
            });

        if price.is_some() && price.unwrap_or(0.0) > 0.0 {
            price_source = format!("nomenclature {} on date {}", nom_ref, order_date);
        } else {
            price = None;
        }

        // 2. По base_nomenclature_ref на дату
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
                                    "fill_dealer_price_for_lines: failed to get price for base_nomenclature {}: {}",
                                    base_ref,
                                    e
                                );
                                None
                            });

                            if price.is_some() && price.unwrap_or(0.0) > 0.0 {
                                price_source = format!(
                                    "base_nomenclature {} on date {}",
                                    base_ref, order_date
                                );
                            } else {
                                price = None;
                            }
                        }
                    }
                }
            }
        }

        // 3. Первая ненулевая по nomenclature_ref
        if price.is_none() {
            price =
                crate::projections::p906_nomenclature_prices::repository::get_first_nonzero_price(
                    nom_ref,
                )
                .await
                .unwrap_or_else(|e| {
                    tracing::warn!(
                        "fill_dealer_price_for_lines: failed to get first nonzero price for {}: {}",
                        nom_ref,
                        e
                    );
                    None
                });

            if price.is_some() {
                price_source = format!("nomenclature {} (first nonzero price)", nom_ref);
            }
        }

        // 4. Первая ненулевая по base_nomenclature_ref
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
                                    "fill_dealer_price_for_lines: failed to get first nonzero price for base_nomenclature {}: {}",
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
                "fill_dealer_price_for_lines: line {} -> dealer_price_ut={:?} (from {})",
                line.line_id,
                price,
                price_source
            );
        } else {
            tracing::warn!(
                "fill_dealer_price_for_lines: could not find dealer_price_ut for line {} (nomenclature: {})",
                line.line_id,
                nom_ref
            );
        }

        line.dealer_price_ut = price;
    }

    Ok(())
}

pub async fn fill_dealer_price_for_lines_resolved(document: &mut YmOrder) -> Result<()> {
    let order_date = document
        .state
        .creation_date
        .map(|dt| dt.format("%Y-%m-%d").to_string())
        .unwrap_or_else(|| chrono::Utc::now().format("%Y-%m-%d").to_string());

    for line in document.lines.iter_mut() {
        let Some(ref nom_ref) = line.nomenclature_ref.clone() else {
            line.dealer_price_ut = None;
            continue;
        };

        let resolved =
            crate::projections::p906_nomenclature_prices::service::resolve_price_for_nomenclature(
                nom_ref,
                &order_date,
            )
            .await
            .unwrap_or_else(|e| {
                tracing::warn!(
                    "fill_dealer_price_for_lines: failed to resolve price for {}: {}",
                    nom_ref,
                    e
                );
                None
            });

        if let Some(ref resolved_price) = resolved {
            tracing::info!(
                "fill_dealer_price_for_lines: line {} -> dealer_price_ut={:?} (from {})",
                line.line_id,
                resolved_price.price,
                resolved_price.describe(&order_date)
            );
        } else {
            tracing::warn!(
                "fill_dealer_price_for_lines: could not find dealer_price_ut for line {} (nomenclature: {})",
                line.line_id,
                nom_ref
            );
        }

        line.dealer_price_ut = resolved.map(|resolved_price| resolved_price.price);
    }

    Ok(())
}

/// Вычислить сумму субсидий документа.
/// Используем ТОЛЬКО субсидии из заголовка: header.subsidies_json содержит
/// агрегированный итог по всему заказу (равен сумме item-level субсидий).
/// Суммирование обоих источников приводит к двойному счёту.
fn calculate_subsidies_total(document: &YmOrder) -> f64 {
    if let Some(ref json) = document.header.subsidies_json {
        if let Ok(arr) = serde_json::from_str::<Vec<serde_json::Value>>(json) {
            return arr
                .iter()
                .filter_map(|sub| sub.get("amount").and_then(|a| a.as_f64()))
                .sum();
        }
    }

    // Fallback: если header subsidies отсутствуют — суммируем по строкам
    document
        .lines
        .iter()
        .filter_map(|line| line.subsidies_json.as_deref())
        .filter_map(|json| serde_json::from_str::<Vec<serde_json::Value>>(json).ok())
        .flatten()
        .filter_map(|sub| sub.get("amount").and_then(|a| a.as_f64()))
        .sum()
}

/// Рассчитать итоговую дилерскую сумму и маржу документа.
/// total_dealer_amount = sum(dealer_price_ut_i * qty_i) по всем строкам
/// effective_amount = sum(amount_line) + subsidies_total
/// margin_pro (%) = (effective_amount * (1 - commission/100) - total_dealer_amount) / total_dealer_amount * 100
/// Fallback (без комиссии): (effective_amount - total_dealer_amount) / total_dealer_amount * 100
pub async fn calculate_totals_and_margin(document: &mut YmOrder) -> Result<()> {
    let total_dealer_amount: f64 = document
        .lines
        .iter()
        .filter_map(|l| l.dealer_price_ut.map(|p| p * l.qty))
        .sum();

    if total_dealer_amount <= 0.0 {
        document.header.total_dealer_amount = None;
        document.header.margin_pro = None;
        return Ok(());
    }

    document.header.total_dealer_amount = Some(total_dealer_amount);

    // Сумма продажи из строк документа
    let total_amount: f64 = document.lines.iter().filter_map(|l| l.amount_line).sum();

    // Сумма субсидий (добавляется к сумме продажи при расчёте маржи)
    let subsidies_total = calculate_subsidies_total(document);
    let effective_amount = total_amount + subsidies_total;

    if subsidies_total > 0.0 {
        tracing::info!(
            "calculate_totals_and_margin: document {} -> subsidies_total={:.2}, effective_amount={:.2}",
            document.base.id.as_string(),
            subsidies_total,
            effective_amount
        );
    }

    // Пытаемся получить planned_commission_percent из подключения
    let mut margin = (effective_amount - total_dealer_amount) / total_dealer_amount * 100.0;

    match Uuid::parse_str(&document.header.connection_id) {
        Ok(connection_id) => {
            match crate::domain::a006_connection_mp::service::get_by_id(connection_id).await {
                Ok(Some(connection)) => {
                    if let Some(planned_percent) = connection.planned_commission_percent {
                        margin = (effective_amount * (100.0 - planned_percent) / 100.0
                            - total_dealer_amount)
                            / total_dealer_amount
                            * 100.0;
                    }
                }
                Ok(None) => {
                    tracing::warn!(
                        "calculate_totals_and_margin: connection {} not found for document {}, using fallback margin formula",
                        document.header.connection_id,
                        document.base.id.as_string()
                    );
                }
                Err(e) => {
                    tracing::warn!(
                        "calculate_totals_and_margin: failed to load connection for document {}: {}, using fallback margin formula",
                        document.base.id.as_string(),
                        e
                    );
                }
            }
        }
        Err(e) => {
            tracing::warn!(
                "calculate_totals_and_margin: invalid connection_id {} for document {}: {}, using fallback margin formula",
                document.header.connection_id,
                document.base.id.as_string(),
                e
            );
        }
    }

    document.header.margin_pro = Some(margin);

    tracing::info!(
        "calculate_totals_and_margin: document {} -> total_dealer_amount={:.2}, margin_pro={:.2}%",
        document.base.id.as_string(),
        total_dealer_amount,
        margin
    );

    Ok(())
}

pub async fn store_document_with_raw(mut document: YmOrder, raw_json: &str) -> Result<Uuid> {
    let raw_ref = crate::shared::data::raw_storage::save_raw_json(
        "YM",
        "YM_Order",
        &document.header.document_no,
        raw_json,
        document.source_meta.fetched_at,
    )
    .await?;

    if let Some(raw_ref) = raw_ref {
        document.source_meta.raw_payload_ref = raw_ref;
    }
    document
        .validate()
        .map_err(|e| anyhow::anyhow!("Validation failed: {}", e))?;
    document.before_write();

    let id = repository::upsert_document(&document).await?;

    tracing::info!("Successfully saved YM Order document with id: {}", id);

    // Проводим документ если is_posted = true
    if document.is_posted {
        if let Err(e) = super::posting::post_document(id).await {
            tracing::error!("Failed to post YM Order document: {}", e);
            // Не останавливаем выполнение, т.к. документ уже сохранен
        }
    } else {
        // Если is_posted = false, удаляем проекции (если были)
        if let Err(e) = crate::projections::p900_mp_sales_register::service::delete_by_registrator(
            &id.to_string(),
        )
        .await
        {
            tracing::error!("Failed to delete projections for YM Order document: {}", e);
        }
    }

    Ok(id)
}

pub async fn get_by_id(id: Uuid) -> Result<Option<YmOrder>> {
    repository::get_by_id(id).await
}

pub async fn get_by_document_no(document_no: &str) -> Result<Option<YmOrder>> {
    repository::get_by_document_no(document_no).await
}

pub async fn list_all() -> Result<Vec<YmOrder>> {
    repository::list_all().await
}

pub async fn delete(id: Uuid) -> Result<bool> {
    repository::soft_delete(id).await
}

/// Структуры для парсинга YM Order из raw JSON
#[derive(Debug, Clone, Serialize, Deserialize)]
struct YmOrderJson {
    #[serde(rename = "creationDate", default)]
    pub creation_date: Option<String>,
    #[serde(rename = "statusUpdateDate", default)]
    pub status_update_date: Option<String>,
    #[serde(default)]
    pub delivery: Option<YmOrderDeliveryJson>,
    #[serde(default)]
    pub status: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct YmOrderDeliveryJson {
    #[serde(default)]
    pub dates: Option<YmOrderDeliveryDatesJson>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct YmOrderDeliveryDatesJson {
    #[serde(rename = "realDeliveryDate", default)]
    pub real_delivery_date: Option<String>,
}

/// Парсинг даты из YM в разных форматах
fn parse_ym_date(date_str: &str) -> Option<DateTime<Utc>> {
    // Try RFC3339 first (e.g., "2024-01-15T10:30:00Z")
    if let Ok(dt) = DateTime::parse_from_rfc3339(date_str) {
        return Some(dt.with_timezone(&Utc));
    }

    // Try format "DD-MM-YYYY HH:MM:SS" (Yandex Market format with time)
    if let Ok(naive) = chrono::NaiveDateTime::parse_from_str(date_str, "%d-%m-%Y %H:%M:%S") {
        return Some(DateTime::from_naive_utc_and_offset(naive, Utc));
    }

    // Try format "DD-MM-YYYY" (Yandex Market format without time)
    if let Ok(naive_date) = chrono::NaiveDate::parse_from_str(date_str, "%d-%m-%Y") {
        let naive_datetime = naive_date.and_hms_opt(0, 0, 0)?;
        return Some(DateTime::from_naive_utc_and_offset(naive_datetime, Utc));
    }

    // Try format "YYYY-MM-DD HH:MM:SS"
    if let Ok(naive) = chrono::NaiveDateTime::parse_from_str(date_str, "%Y-%m-%d %H:%M:%S") {
        return Some(DateTime::from_naive_utc_and_offset(naive, Utc));
    }

    // Try format "YYYY-MM-DD"
    if let Ok(naive_date) = chrono::NaiveDate::parse_from_str(date_str, "%Y-%m-%d") {
        let naive_datetime = naive_date.and_hms_opt(0, 0, 0)?;
        return Some(DateTime::from_naive_utc_and_offset(naive_datetime, Utc));
    }

    tracing::warn!("Failed to parse YM date: {}", date_str);
    None
}

/// Заполнить отсутствующие поля из raw JSON
/// Используется для восстановления данных в старых документах при проведении
pub async fn refill_from_raw_json(document: &mut YmOrder) -> Result<bool> {
    // Проверяем, нужно ли заполнять (если creation_date отсутствует)
    if document.state.creation_date.is_some() {
        return Ok(false); // Ничего не заполняли
    }

    // Получаем raw JSON из хранилища
    let raw_ref = &document.source_meta.raw_payload_ref;
    if raw_ref.is_empty() {
        tracing::warn!(
            "Document {} has no raw_payload_ref, cannot refill",
            document.header.document_no
        );
        return Ok(false);
    }

    let Some(raw_json_str) = crate::shared::data::raw_storage::get_by_ref(raw_ref).await? else {
        tracing::warn!(
            "Raw JSON not found for document {}, ref {}; skipping refill",
            document.header.document_no,
            raw_ref
        );
        return Ok(false);
    };

    // Парсим JSON
    let ym_order_json: YmOrderJson = match serde_json::from_str(&raw_json_str) {
        Ok(value) => value,
        Err(e) => {
            tracing::warn!(
                "Failed to parse raw JSON for document {}, ref {}; skipping refill: {}",
                document.header.document_no,
                raw_ref,
                e
            );
            return Ok(false);
        }
    };

    let mut fields_updated = false;

    // Заполняем creation_date
    if document.state.creation_date.is_none() {
        if let Some(ref creation_date_str) = ym_order_json.creation_date {
            if let Some(parsed_date) = parse_ym_date(creation_date_str) {
                document.state.creation_date = Some(parsed_date);
                fields_updated = true;
                tracing::info!(
                    "Refilled creation_date for document {}: {}",
                    document.header.document_no,
                    creation_date_str
                );
            }
        }
    }

    // Заполняем status_changed_at
    if document.state.status_changed_at.is_none() {
        if let Some(ref status_update_date_str) = ym_order_json.status_update_date {
            if let Some(parsed_date) = parse_ym_date(status_update_date_str) {
                document.state.status_changed_at = Some(parsed_date);
                fields_updated = true;
                tracing::info!(
                    "Refilled status_changed_at for document {}: {}",
                    document.header.document_no,
                    status_update_date_str
                );
            }
        }
    }

    // Заполняем delivery_date из delivery.dates.realDeliveryDate
    if document.state.delivery_date.is_none() {
        if let Some(ref delivery) = ym_order_json.delivery {
            if let Some(ref dates) = delivery.dates {
                if let Some(ref real_delivery_date_str) = dates.real_delivery_date {
                    if let Some(parsed_date) = parse_ym_date(real_delivery_date_str) {
                        document.state.delivery_date = Some(parsed_date);
                        fields_updated = true;
                        tracing::info!(
                            "Refilled delivery_date for document {}: {}",
                            document.header.document_no,
                            real_delivery_date_str
                        );
                    }
                }
            }
        }
    }

    // Обновляем status_raw и status_norm если они пустые
    if document.state.status_raw.is_empty() {
        if let Some(ref status) = ym_order_json.status {
            document.state.status_raw = status.clone();
            document.state.status_norm = normalize_ym_status(status);
            fields_updated = true;
            tracing::info!(
                "Refilled status for document {}: {}",
                document.header.document_no,
                status
            );
        }
    }

    if fields_updated {
        tracing::info!(
            "Successfully refilled fields from raw JSON for document {}",
            document.header.document_no
        );
    }

    Ok(fields_updated)
}

/// Normalize Yandex Market order status
fn normalize_ym_status(status: &str) -> String {
    match status.to_uppercase().as_str() {
        "DELIVERED" => "DELIVERED".to_string(),
        "PICKUP" => "DELIVERED".to_string(),
        "PROCESSING" => "PROCESSING".to_string(),
        "DELIVERY" => "PROCESSING".to_string(),
        "CANCELLED" => "CANCELLED".to_string(),
        "CANCELLED_BEFORE_PROCESSING" => "CANCELLED".to_string(),
        "RETURNED" => "PARTIALLY_RETURNED".to_string(),
        "PARTIALLY_RETURNED" => "PARTIALLY_RETURNED".to_string(),
        _ => status.to_uppercase(),
    }
}

/// Заказы Яндекс.Маркет для вкладки «Заказы» на карточке номенклатуры.
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
            super::repository::list_lines_for_nomenclature(nomenclature_ref, date_from).await?;

        Ok(rows
            .into_iter()
            .map(|r| {
                // Маржа заказа относится ко всему заказу целиком, поэтому
                // переносим её на строку, только если заказ однострочный —
                // иначе она была бы приписана строке неверно.
                let margin_pro = match r.order_lines_count {
                    Some(1) => r.order_margin_pro,
                    _ => None,
                };
                NomenclatureOrderRowDto {
                    id: r.order_id,
                    marketplace: "YM".to_string(),
                    document_no: r.document_no,
                    order_date: r.creation_date,
                    is_cancel: None,
                    is_supply: None,
                    is_realization: None,
                    line_status: r.line_status,
                    status_norm: r.status_norm,
                    qty: r.qty,
                    price_before_discount: r.price_list,
                    price_after_discount: r.price_effective,
                    final_buyer_price: r.buyer_price,
                    dealer_price_ut: r.dealer_price_ut,
                    margin_pro,
                }
            })
            .collect())
    }
}
